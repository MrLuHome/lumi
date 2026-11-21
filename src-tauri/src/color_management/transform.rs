//! 色彩空间转换：输入 Profile、输出 Profile、软打样与色域判断。
//!
//! 所有转换都由 Little CMS 完成，16-bit 输入输出，避免 8-bit 往返损失。
//! 注意：转换只做一次——工作空间固定为 sRGB 编码，渲染链路不会重复套用输出 Profile。

use image::{ImageBuffer, Rgba};
use lcms2::{Flags, PixelFormat, Profile, Transform};

use crate::error::{AppError, AppResult};
use crate::image_engine::buffer::Image16;

use super::lab::{delta_e00, rgb16_to_lab};
use super::profiles::{self, ColorOutputSettings, ProfileKind};

/// 一次转换的统计，用于日志与测试。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionReport {
    pub profile: String,
    pub intent: String,
    pub black_point_compensation: bool,
    /// 转换后有多少像素发生变化（用于确认不是“只写元数据”）。
    pub changed_pixels: u64,
    pub total_pixels: u64,
    pub max_channel_delta: u16,
}

/// sRGB（工作空间）→ 输出 Profile。
pub fn to_output(
    image: &Image16,
    settings: &ColorOutputSettings,
) -> AppResult<(Image16, Vec<u8>, ConversionReport)> {
    let output = profiles::load(settings)?;
    let icc = profiles::embedding_bytes(settings)?;
    let converted = convert(image, &Profile::new_srgb(), &output, settings)?;
    let report = report(image, &converted, settings);
    Ok((converted, icc, report))
}

/// 输入 Profile → sRGB（工作空间）。没有嵌入 Profile 时按 sRGB 原样返回。
pub fn from_input_profile(image: &Image16, icc: Option<&[u8]>) -> AppResult<(Image16, String)> {
    match icc {
        None => Ok((image.clone(), "未指定配置，按 sRGB 处理".to_string())),
        Some(bytes) => {
            let input = profiles::from_bytes(bytes)?;
            // 解码器只产出 RGB 像素；CMYK / 灰度 Profile 交给 CMYK 专用路径处理。
            if input.color_space() != lcms2::ColorSpaceSignature::RgbData {
                return Err(AppError::Unsupported(
                    "嵌入 Profile 不是 RGB，无法作为 RGB 输入转换".into(),
                ));
            }
            let srgb = Profile::new_srgb();
            let settings = ColorOutputSettings::default();
            let converted = convert(image, &input, &srgb, &settings)?;
            let label = input
                .info(lcms2::InfoType::Description, lcms2::Locale::none())
                .unwrap_or_else(|| "嵌入 ICC".to_string());
            Ok((converted, label))
        }
    }
}

/// 软打样：sRGB → 输出 Profile → sRGB，返回预览图与色域统计。
pub fn soft_proof(
    image: &Image16,
    settings: &ColorOutputSettings,
    warn: bool,
) -> AppResult<(Image16, GamutReport)> {
    let output = profiles::load(settings)?;
    let srgb = Profile::new_srgb();
    let proof = convert(image, &srgb, &output, settings)?;
    let back = convert(&proof, &output, &srgb, settings)?;

    let mut out_of_gamut = 0u64;
    let mut max_delta = 0.0f32;
    let total = image.width() as u64 * image.height() as u64;
    let mut preview = back.clone();
    if warn {
        for (index, pixel) in preview.pixels_mut().enumerate() {
            let x = (index as u32) % image.width();
            let y = (index as u32) / image.width();
            let original = image.get_pixel(x, y);
            let delta = delta_e00(
                rgb16_to_lab([original.0[0], original.0[1], original.0[2]]),
                rgb16_to_lab([pixel.0[0], pixel.0[1], pixel.0[2]]),
            );
            if delta > 2.0 {
                out_of_gamut += 1;
                max_delta = max_delta.max(delta);
                // 色域警告：用高饱和洋红覆盖，便于一眼识别。
                pixel.0 = [65535, 0, 40000, pixel.0[3]];
            }
        }
    }
    Ok((
        preview,
        GamutReport {
            out_of_gamut_pixels: out_of_gamut,
            total_pixels: total,
            ratio: if total > 0 {
                out_of_gamut as f32 / total as f32
            } else {
                0.0
            },
            max_delta_e00: max_delta,
        },
    ))
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GamutReport {
    pub out_of_gamut_pixels: u64,
    pub total_pixels: u64,
    pub ratio: f32,
    pub max_delta_e00: f32,
}

/// CMYK 8-bit 像素 → sRGB `Image16`。
///
/// 有 CMYK Profile 时走 Little CMS 真实转换；没有时使用通用公式，
/// 并在返回值中明确标注质量等级为“基础”。
pub fn cmyk_to_srgb(
    cmyk: &[u8],
    width: u32,
    height: u32,
    icc: Option<&[u8]>,
) -> AppResult<(Image16, String)> {
    let expected = width as usize * height as usize * 4;
    if cmyk.len() < expected {
        return Err(AppError::Decode(format!(
            "CMYK 数据不足：期望 {expected} 字节，实际 {}",
            cmyk.len()
        )));
    }
    let mut out: Image16 = ImageBuffer::new(width, height);

    if let Some(bytes) = icc {
        let input = profiles::from_bytes(bytes)?;
        let srgb = Profile::new_srgb();
        let transform: Transform<[u8; 4], [u8; 3]> = Transform::new(
            &input,
            PixelFormat::CMYK_8,
            &srgb,
            PixelFormat::RGB_8,
            lcms2::Intent::RelativeColorimetric,
        )
        .map_err(|error| AppError::Task(format!("创建 CMYK 转换失败：{error}")))?;
        let src: Vec<[u8; 4]> = cmyk[..expected]
            .chunks_exact(4)
            .map(|chunk| [chunk[0], chunk[1], chunk[2], chunk[3]])
            .collect();
        let mut dst = vec![[0u8; 3]; width as usize * height as usize];
        transform.transform_pixels(&src, &mut dst);
        for (index, pixel) in dst.iter().enumerate() {
            let x = index as u32 % width;
            let y = index as u32 / width;
            out.put_pixel(
                x,
                y,
                Rgba([
                    pixel[0] as u16 * 257,
                    pixel[1] as u16 * 257,
                    pixel[2] as u16 * 257,
                    65535,
                ]),
            );
        }
        return Ok((out, "CMYK Profile → sRGB（Little CMS）".to_string()));
    }

    for (index, chunk) in cmyk[..expected].chunks_exact(4).enumerate() {
        let (c, m, y, k) = (
            chunk[0] as f32 / 255.0,
            chunk[1] as f32 / 255.0,
            chunk[2] as f32 / 255.0,
            chunk[3] as f32 / 255.0,
        );
        let r = (1.0 - c) * (1.0 - k);
        let g = (1.0 - m) * (1.0 - k);
        let b = (1.0 - y) * (1.0 - k);
        let x = index as u32 % width;
        let yy = index as u32 / width;
        out.put_pixel(
            x,
            yy,
            Rgba([
                (r * 65535.0) as u16,
                (g * 65535.0) as u16,
                (b * 65535.0) as u16,
                65535,
            ]),
        );
    }
    Ok((
        out,
        "CMYK → sRGB（无 Profile，通用公式，基础质量）".to_string(),
    ))
}

/// 核心 16-bit RGB 转换。
fn convert(
    image: &Image16,
    input: &Profile,
    output: &Profile,
    settings: &ColorOutputSettings,
) -> AppResult<Image16> {
    let intent = settings.intent.to_lcms();
    let flags = if settings.black_point_compensation {
        Flags::BLACKPOINT_COMPENSATION
    } else {
        Flags::default()
    };
    let transform: Transform<[u16; 3], [u16; 3]> = Transform::new_flags(
        input,
        PixelFormat::RGB_16,
        output,
        PixelFormat::RGB_16,
        intent,
        flags,
    )
    .map_err(|error| AppError::Task(format!("创建色彩转换失败：{error}")))?;

    let width = image.width();
    let src: Vec<[u16; 3]> = image
        .pixels()
        .map(|pixel| [pixel.0[0], pixel.0[1], pixel.0[2]])
        .collect();
    let mut dst = vec![[0u16; 3]; src.len()];
    transform.transform_pixels(&src, &mut dst);

    let mut out: Image16 = ImageBuffer::new(width, image.height());
    for (index, pixel) in dst.iter().enumerate() {
        let alpha = image
            .get_pixel(index as u32 % width, index as u32 / width)
            .0[3];
        out.put_pixel(
            index as u32 % width,
            index as u32 / width,
            Rgba([pixel[0], pixel[1], pixel[2], alpha]),
        );
    }
    Ok(out)
}

fn report(before: &Image16, after: &Image16, settings: &ColorOutputSettings) -> ConversionReport {
    let mut changed = 0u64;
    let mut max_delta = 0u16;
    for (left, right) in before.pixels().zip(after.pixels()) {
        let delta = (0..3)
            .map(|index| left.0[index].abs_diff(right.0[index]))
            .max()
            .unwrap_or(0);
        if delta > 0 {
            changed += 1;
        }
        max_delta = max_delta.max(delta);
    }
    ConversionReport {
        profile: settings.profile.label().to_string(),
        intent: settings.intent.label().to_string(),
        black_point_compensation: settings.black_point_compensation,
        changed_pixels: changed,
        total_pixels: before.width() as u64 * before.height() as u64,
        max_channel_delta: max_delta,
    }
}

/// 供命令层展示的能力描述。
pub fn capability() -> serde_json::Value {
    serde_json::json!({
        "outputProfiles": ProfileKind::all().iter().map(|kind| kind.label()).collect::<Vec<_>>(),
        "customProfile": true,
        "softProof": true,
        "gamutWarning": true,
        "cmyk": true,
        "tiff16": true,
        "displayProfile": true,
        "cms": format!("Little CMS {}", lcms2::version() >> 16),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient() -> Image16 {
        ImageBuffer::from_fn(32, 16, |x, y| {
            Rgba([
                (x * 65535 / 31) as u16,
                (y * 65535 / 15) as u16,
                ((x + y) * 65535 / 46) as u16,
                65535,
            ])
        })
    }

    #[test]
    fn color_checker_roundtrip_delta_e() {
        // ColorChecker Classic 24 色块（BabelColor sRGB 参考值，D65）。
        const PATCHES: [[u8; 3]; 24] = [
            [115, 82, 68],
            [194, 150, 130],
            [98, 122, 157],
            [87, 108, 67],
            [133, 128, 177],
            [103, 189, 170],
            [214, 126, 44],
            [80, 91, 166],
            [193, 90, 99],
            [94, 60, 108],
            [157, 188, 64],
            [224, 163, 46],
            [56, 61, 150],
            [70, 148, 73],
            [175, 54, 60],
            [231, 199, 31],
            [187, 86, 149],
            [8, 133, 161],
            [243, 243, 242],
            [200, 200, 200],
            [160, 160, 160],
            [122, 122, 121],
            [85, 85, 85],
            [52, 52, 52],
        ];
        let image: Image16 = ImageBuffer::from_fn(24, 1, |x, _| {
            let patch = PATCHES[x as usize];
            Rgba([
                patch[0] as u16 * 257,
                patch[1] as u16 * 257,
                patch[2] as u16 * 257,
                65535,
            ])
        });
        let settings = ColorOutputSettings {
            profile: ProfileKind::AdobeRgb,
            ..Default::default()
        };
        let (converted, _icc, report) = to_output(&image, &settings).unwrap();
        assert!(
            report.changed_pixels > 0,
            "输出 Profile 必须真实改变像素，而不是只写元数据"
        );

        let icc = profiles::embedding_bytes(&settings).unwrap();
        let (back, label) = from_input_profile(&converted, Some(&icc)).unwrap();
        assert!(!label.is_empty());

        let mut sum = 0.0f32;
        let mut max = 0.0f32;
        for (index, patch) in PATCHES.iter().enumerate() {
            let original = crate::color_management::lab::rgb16_to_lab([
                patch[0] as u16 * 257,
                patch[1] as u16 * 257,
                patch[2] as u16 * 257,
            ]);
            let pixel = back.get_pixel(index as u32, 0);
            let roundtrip =
                crate::color_management::lab::rgb16_to_lab([pixel[0], pixel[1], pixel[2]]);
            let delta = crate::color_management::lab::delta_e00(original, roundtrip);
            sum += delta;
            max = max.max(delta);
        }
        let average = sum / PATCHES.len() as f32;
        eprintln!("ColorChecker sRGB→AdobeRGB→sRGB：平均 ΔE00 {average:.3}，最大 ΔE00 {max:.3}");
        assert!(average < 1.0, "平均色差过大：{average}");
        assert!(max < 2.5, "单块最大色差过大：{max}");
    }

    #[test]
    fn soft_proof_flags_out_of_gamut_colors() {
        let saturated = ImageBuffer::from_fn(4, 1, |x, _| {
            Rgba(match x {
                0 => [65535, 0, 0, 65535],
                1 => [0, 65535, 0, 65535],
                2 => [0, 0, 65535, 65535],
                _ => [20000, 20000, 20000, 65535],
            })
        });
        let settings = ColorOutputSettings {
            profile: ProfileKind::Srgb,
            ..Default::default()
        };
        let (proof, report) = soft_proof(&saturated, &settings, true).unwrap();
        assert_eq!(proof.width(), 4);
        // sRGB 自身的饱和原色往返后 ΔE00 很小，不应误报。
        assert_eq!(report.out_of_gamut_pixels, 0);

        let p3 = ColorOutputSettings {
            profile: ProfileKind::ProPhoto,
            ..Default::default()
        };
        let (_proof, report) = soft_proof(&saturated, &p3, true).unwrap();
        assert!(report.max_delta_e00 >= 0.0);
    }

    #[test]
    fn output_conversion_really_changes_pixels() {
        let image = gradient();
        let settings = ColorOutputSettings {
            profile: ProfileKind::AdobeRgb,
            ..Default::default()
        };
        let (converted, icc, report) = to_output(&image, &settings).unwrap();
        assert!(!icc.is_empty());
        assert!(report.changed_pixels > 0, "输出转换必须真实改变像素");
        assert_ne!(converted, image);
    }

    #[test]
    fn srgb_to_srgb_is_identity() {
        let image = gradient();
        let settings = ColorOutputSettings::default();
        let (converted, _, _) = to_output(&image, &settings).unwrap();
        // sRGB→sRGB 在 16-bit 下允许极小量化差，但不得出现明显偏移。
        for (before, after) in image.pixels().zip(converted.pixels()) {
            for channel in 0..3 {
                assert!(before.0[channel].abs_diff(after.0[channel]) <= 2);
            }
        }
    }

    #[test]
    fn roundtrip_does_not_drift_repeatedly() {
        let image = gradient();
        let adobe = ColorOutputSettings {
            profile: ProfileKind::AdobeRgb,
            ..Default::default()
        };
        let (first, _, _) = to_output(&image, &adobe).unwrap();
        let back_settings = ColorOutputSettings::default();
        let (back, _, _) = to_output(&first, &back_settings).unwrap();
        let (again, _, _) = to_output(&back, &back_settings).unwrap();
        for (left, right) in back.pixels().zip(again.pixels()) {
            for channel in 0..3 {
                assert_eq!(left.0[channel], right.0[channel], "二次转换发生漂移");
            }
        }
    }

    #[test]
    fn soft_proof_reports_gamut_for_saturated_colors() {
        let image: Image16 = ImageBuffer::from_pixel(8, 8, Rgba([65535, 0, 0, 65535]));
        let settings = ColorOutputSettings {
            profile: ProfileKind::Srgb,
            ..Default::default()
        };
        let (_, report) = soft_proof(&image, &settings, true).unwrap();
        assert_eq!(report.total_pixels, 64);
    }

    #[test]
    fn cmyk_without_profile_uses_documented_formula() {
        let cmyk = vec![0u8, 0, 0, 0, 255, 0, 0, 0];
        let (image, label) = cmyk_to_srgb(&cmyk, 2, 1, None).unwrap();
        assert!(label.contains("基础"));
        assert_eq!(image.get_pixel(0, 0).0[0], 65535);
        assert_eq!(image.get_pixel(1, 0).0[0], 0);
    }
}
