use image::RgbaImage;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use super::blur;

/// 统一编辑参数。前后端共用同一套字段名（camelCase），
/// 滑杆、撤销快照、Rust 渲染管线都直接使用这个结构，避免出现第二份参数定义。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EditParams {
    /// 曝光，-100..100
    pub exposure: f32,
    /// 对比度，-100..100
    pub contrast: f32,
    /// 高光，-100..100
    pub highlights: f32,
    /// 阴影，-100..100
    pub shadows: f32,
    /// 色温，-100..100（负冷正暖）
    pub temperature: f32,
    /// 色调，-100..100（负绿正品红）
    pub tint: f32,
    /// 饱和度，-100..100
    pub saturation: f32,
    /// 自然饱和度，-100..100
    pub vibrance: f32,
    /// 清晰度（大半径局部对比），0..100
    pub clarity: f32,
    /// 锐化，0..100
    pub sharpness: f32,
    /// 颗粒，0..100
    pub grain: f32,
    /// 暗角，0..100
    pub vignette: f32,
    /// 磨皮强度，0..100
    pub skin_smooth: f32,
}

impl Default for EditParams {
    fn default() -> Self {
        Self {
            exposure: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            temperature: 0.0,
            tint: 0.0,
            saturation: 0.0,
            vibrance: 0.0,
            clarity: 0.0,
            sharpness: 0.0,
            grain: 0.0,
            vignette: 0.0,
            skin_smooth: 0.0,
        }
    }
}

impl EditParams {
    /// 把越界值收敛到 UI 允许的区间，防止前端异常数据把管线算到无效区间。
    pub fn clamped(mut self) -> Self {
        for value in [
            &mut self.exposure,
            &mut self.contrast,
            &mut self.highlights,
            &mut self.shadows,
            &mut self.temperature,
            &mut self.tint,
            &mut self.saturation,
            &mut self.vibrance,
        ] {
            *value = value.clamp(-100.0, 100.0);
        }
        for value in [
            &mut self.clarity,
            &mut self.sharpness,
            &mut self.grain,
            &mut self.vignette,
            &mut self.skin_smooth,
        ] {
            *value = value.clamp(0.0, 100.0);
        }
        self
    }

    pub fn is_neutral(&self) -> bool {
        self.clamped() == EditParams::default()
    }
}

fn luma(r: f32, g: f32, b: f32) -> f32 {
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    if edge1 <= edge0 {
        return if value < edge0 { 0.0 } else { 1.0 };
    }
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn par_pixels_mut<F>(image: &mut RgbaImage, apply: F)
where
    F: Fn(u32, u32, &mut [u8]) + Sync,
{
    let width = image.width();
    if width == 0 || image.height() == 0 {
        return;
    }
    let raw: &mut [u8] = image.as_mut();
    raw.par_chunks_mut(4).enumerate().for_each(|(index, pixel)| {
        let x = index as u32 % width;
        let y = index as u32 / width;
        apply(x, y, pixel);
    });
}

/// 完整渲染管线：磨皮 → 明暗 → 色彩 → 清晰度 → 锐化 → 颗粒 → 暗角。
pub fn render(image: &RgbaImage, params: &EditParams) -> RgbaImage {
    let params = params.clamped();
    if params.is_neutral() {
        return image.clone();
    }

    let mut output = image.clone();
    if params.skin_smooth > 0.0 {
        output = smooth_skin(&output, params.skin_smooth);
    }
    apply_light(&mut output, &params);
    apply_color(&mut output, &params);
    if params.clarity > 0.0 {
        let radius = (output.width().min(output.height()) / 160).clamp(2, 18);
        output = unsharp_mask(&output, radius, params.clarity / 100.0 * 0.6);
    }
    if params.sharpness > 0.0 {
        output = unsharp_mask(&output, 1, params.sharpness / 100.0 * 1.5);
    }
    if params.grain > 0.0 {
        add_grain(&mut output, params.grain / 100.0);
    }
    if params.vignette > 0.0 {
        add_vignette(&mut output, params.vignette / 100.0);
    }
    output
}

fn apply_light(image: &mut RgbaImage, params: &EditParams) {
    let exposure_mul = 2f32.powf(params.exposure / 100.0 * 1.4);
    let contrast_mul = 1.0 + params.contrast / 100.0 * 0.75;
    let shadow_shift = params.shadows / 100.0 * 0.32;
    let highlight_shift = params.highlights / 100.0 * 0.32;

    par_pixels_mut(image, |_, _, pixel| {
        let mut rgb = [
            pixel[0] as f32 / 255.0 * exposure_mul,
            pixel[1] as f32 / 255.0 * exposure_mul,
            pixel[2] as f32 / 255.0 * exposure_mul,
        ];
        let light = luma(rgb[0], rgb[1], rgb[2]).clamp(0.0, 1.0);
        if shadow_shift != 0.0 {
            let mask = (1.0 - light).powi(2);
            for channel in rgb.iter_mut() {
                *channel += shadow_shift * mask;
            }
        }
        if highlight_shift != 0.0 {
            let mask = light.powi(2);
            for channel in rgb.iter_mut() {
                *channel += highlight_shift * mask;
            }
        }
        if contrast_mul != 1.0 {
            for channel in rgb.iter_mut() {
                *channel = (*channel - 0.5) * contrast_mul + 0.5;
            }
        }
        for index in 0..3 {
            pixel[index] = (rgb[index].clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    });
}

fn apply_color(image: &mut RgbaImage, params: &EditParams) {
    let warm = params.temperature / 100.0 * 0.16;
    let green = params.tint / 100.0 * 0.10;
    let saturation = params.saturation / 100.0;
    let vibrance = params.vibrance / 100.0;

    if warm == 0.0 && green == 0.0 && saturation == 0.0 && vibrance == 0.0 {
        return;
    }

    par_pixels_mut(image, |_, _, pixel| {
        let mut rgb = [
            pixel[0] as f32 / 255.0 * (1.0 + warm),
            pixel[1] as f32 / 255.0 * (1.0 - green),
            pixel[2] as f32 / 255.0 * (1.0 - warm),
        ];
        let light = luma(rgb[0], rgb[1], rgb[2]);
        let max_channel = rgb[0].max(rgb[1]).max(rgb[2]);
        let min_channel = rgb[0].min(rgb[1]).min(rgb[2]);
        let chroma = (max_channel - min_channel).clamp(0.0, 1.0);

        // 自然饱和度优先作用于低饱和区域，避免肤色被推成荧光色。
        let factor = (1.0 + saturation + vibrance * (1.0 - chroma)).max(0.0);
        for channel in rgb.iter_mut() {
            *channel = light + (*channel - light) * factor;
        }
        for index in 0..3 {
            pixel[index] = (rgb[index].clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    });
}

/// 磨皮：两次盒式模糊生成低频底图，再按高频差值做保边混合。
/// 这是纯 CPU 的基础磨皮，不含 AI 检测，作为后续模型管线的占位工序。
fn smooth_skin(image: &RgbaImage, strength: f32) -> RgbaImage {
    let ratio = (strength / 100.0).clamp(0.0, 1.0);
    let radius = (2.0 + ratio * 3.0).round() as u32;
    let blurred = blur::gaussian_blur(image, radius);
    let blend = ratio * 0.85;
    let edge = 0.05 + (1.0 - ratio) * 0.14;

    let mut output = image.clone();
    par_pixels_mut(&mut output, |x, y, pixel| {
        let source = image.get_pixel(x, y).0;
        let target = blurred.get_pixel(x, y).0;
        let source_light = luma(
            source[0] as f32 / 255.0,
            source[1] as f32 / 255.0,
            source[2] as f32 / 255.0,
        );
        let target_light = luma(
            target[0] as f32 / 255.0,
            target[1] as f32 / 255.0,
            target[2] as f32 / 255.0,
        );
        let difference = (source_light - target_light).abs();
        let weight = 1.0 - smoothstep(edge, edge * 2.0 + 0.02, difference);
        let mix = blend * weight;
        for channel in 0..3 {
            let value = source[channel] as f32 + (target[channel] as f32 - source[channel] as f32) * mix;
            pixel[channel] = value.clamp(0.0, 255.0).round() as u8;
        }
    });
    output
}

fn unsharp_mask(image: &RgbaImage, radius: u32, amount: f32) -> RgbaImage {
    if amount <= 0.0 || radius == 0 {
        return image.clone();
    }
    let blurred = blur::gaussian_blur(image, radius);
    let mut output = image.clone();
    par_pixels_mut(&mut output, |x, y, pixel| {
        let low = blurred.get_pixel(x, y).0;
        for channel in 0..3 {
            let original = pixel[channel] as f32;
            let detail = original - low[channel] as f32;
            pixel[channel] = (original + detail * amount).clamp(0.0, 255.0).round() as u8;
        }
    });
    output
}

fn hash_noise(x: u32, y: u32) -> f32 {
    let mut hash = x
        .wrapping_mul(374_761_393)
        .wrapping_add(y.wrapping_mul(668_265_263));
    hash = (hash ^ (hash >> 13)).wrapping_mul(1_274_126_177);
    let unit = ((hash ^ (hash >> 16)) & 0xFFFF) as f32 / 65_535.0;
    unit * 2.0 - 1.0
}

fn add_grain(image: &mut RgbaImage, strength: f32) {
    let amplitude = strength * 0.055;
    par_pixels_mut(image, |x, y, pixel| {
        let noise = hash_noise(x, y) * amplitude * 255.0;
        for channel in 0..3 {
            pixel[channel] = (pixel[channel] as f32 + noise).clamp(0.0, 255.0).round() as u8;
        }
    });
}

fn add_vignette(image: &mut RgbaImage, strength: f32) {
    let width = image.width().max(1) as f32;
    let height = image.height().max(1) as f32;
    let center_x = width / 2.0;
    let center_y = height / 2.0;
    let max_distance = (center_x * center_x + center_y * center_y).sqrt().max(1.0);

    par_pixels_mut(image, |x, y, pixel| {
        let dx = x as f32 - center_x;
        let dy = y as f32 - center_y;
        let distance = (dx * dx + dy * dy).sqrt() / max_distance;
        let falloff = smoothstep(0.42, 1.0, distance);
        let factor = 1.0 - strength * 0.62 * falloff;
        for channel in 0..3 {
            pixel[channel] = (pixel[channel] as f32 * factor).clamp(0.0, 255.0).round() as u8;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn flat(value: u8, width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_pixel(width, height, Rgba([value, value, value, 255]))
    }

    fn mean_luma(image: &RgbaImage) -> f32 {
        let mut sum = 0.0f64;
        for pixel in image.pixels() {
            sum += luma(
                pixel[0] as f32 / 255.0,
                pixel[1] as f32 / 255.0,
                pixel[2] as f32 / 255.0,
            ) as f64;
        }
        (sum / image.pixels().count() as f64) as f32
    }

    #[test]
    fn neutral_params_are_pixel_identical() {
        let mut image = RgbaImage::from_pixel(8, 8, Rgba([10, 20, 30, 255]));
        image.put_pixel(3, 4, Rgba([200, 150, 90, 255]));
        let rendered = render(&image, &EditParams::default());
        assert_eq!(rendered.into_raw(), image.into_raw());
    }

    #[test]
    fn exposure_raises_luminance() {
        let image = flat(120, 16, 16);
        let params = EditParams {
            exposure: 50.0,
            ..Default::default()
        };
        assert!(mean_luma(&render(&image, &params)) > mean_luma(&image) + 0.05);
    }

    #[test]
    fn negative_saturation_desaturates() {
        let image = RgbaImage::from_pixel(8, 8, Rgba([220, 40, 60, 255]));
        let params = EditParams {
            saturation: -100.0,
            ..Default::default()
        };
        let pixel = render(&image, &params).get_pixel(4, 4).0;
        let max = pixel[0].max(pixel[1]).max(pixel[2]);
        let min = pixel[0].min(pixel[1]).min(pixel[2]);
        assert!(max - min <= 1, "应变为灰阶，实际 {pixel:?}");
    }

    #[test]
    fn positive_contrast_expands_tones() {
        let mut image = flat(160, 8, 8);
        image.put_pixel(0, 0, Rgba([90, 90, 90, 255]));
        let params = EditParams {
            contrast: 80.0,
            ..Default::default()
        };
        let rendered = render(&image, &params);
        assert!(rendered.get_pixel(0, 0).0[0] < 85);
        assert!(rendered.get_pixel(4, 4).0[0] > 140);
    }

    #[test]
    fn temperature_warms_highlights() {
        let image = flat(128, 8, 8);
        let params = EditParams {
            temperature: 80.0,
            ..Default::default()
        };
        let pixel = render(&image, &params).get_pixel(4, 4).0;
        assert!(pixel[0] > pixel[2], "暖调应让红色高于蓝色");
    }

    #[test]
    fn vignette_darkens_corners_more_than_center() {
        let image = flat(200, 41, 41);
        let params = EditParams {
            vignette: 100.0,
            ..Default::default()
        };
        let rendered = render(&image, &params);
        assert!(rendered.get_pixel(20, 20).0[0] > rendered.get_pixel(0, 0).0[0]);
    }

    #[test]
    fn grain_is_deterministic() {
        let image = flat(120, 24, 24);
        let params = EditParams {
            grain: 60.0,
            ..Default::default()
        };
        let first = render(&image, &params);
        let second = render(&image, &params);
        assert_eq!(first.into_raw(), second.into_raw());
    }

    #[test]
    fn out_of_range_params_are_clamped_not_panicking() {
        let image = flat(100, 8, 8);
        let params = EditParams {
            exposure: 9_999.0,
            saturation: -9_999.0,
            vignette: 9_999.0,
            skin_smooth: 9_999.0,
            ..Default::default()
        };
        let rendered = render(&image, &params);
        assert_eq!(rendered.dimensions(), (8, 8));
    }

    #[test]
    fn skin_smooth_preserves_flat_region() {
        let image = flat(150, 16, 16);
        let params = EditParams {
            skin_smooth: 100.0,
            ..Default::default()
        };
        let pixel = render(&image, &params).get_pixel(8, 8).0;
        assert_eq!(pixel[0], pixel[1]);
        assert_eq!(pixel[1], pixel[2]);
        assert!((pixel[0] as i32 - 150).abs() <= 2);
    }

    #[test]
    fn skin_smooth_keeps_alpha_channel() {
        let image = RgbaImage::from_pixel(8, 8, Rgba([120, 80, 60, 128]));
        let params = EditParams {
            skin_smooth: 80.0,
            ..Default::default()
        };
        assert_eq!(render(&image, &params).get_pixel(3, 3).0[3], 128);
    }

    #[test]
    fn sharpness_increases_edge_contrast() {
        let mut image = flat(100, 16, 16);
        for y in 0..16 {
            for x in 8..16 {
                image.put_pixel(x, y, Rgba([160, 160, 160, 255]));
            }
        }
        let params = EditParams {
            sharpness: 100.0,
            ..Default::default()
        };
        let rendered = render(&image, &params);
        assert!(rendered.get_pixel(7, 8).0[0] <= 100);
        assert!(rendered.get_pixel(8, 8).0[0] >= 160);
    }
}
