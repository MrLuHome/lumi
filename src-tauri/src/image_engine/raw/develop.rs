//! RAW 显影：白平衡、相机矩阵、曝光、高光/阴影、sRGB 编码。
//!
//! 输入是 `demosaic` 得到的相机原生线性 RGB，输出是编辑层使用的 sRGB 编码
//! `Image16`（0..65535）。所有参数都在线性空间生效，参数化、非破坏性，
//! 不修改任何原图文件。

use image::ImageBuffer;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::image_engine::buffer::Image16;

use super::demosaic::{demosaic, DemosaicMode};
use super::linear::LinearImage;
use super::RawResult;

/// 白平衡来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum WbMode {
    /// 使用文件中的 as-shot 系数（默认）。
    #[default]
    Camera,
    /// 灰度世界自动白平衡。
    Auto,
    /// 用户指定色温/色调。
    Custom,
}

/// RAW 显影参数。与编辑层 `EditParams` 完全独立，避免互相污染。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawDevelopParams {
    /// 曝光补偿（EV），-5..=5。
    pub exposure_ev: f32,
    /// 高光：0 不处理，>0 压缩高光，<0 提亮高光。
    pub highlights: f32,
    /// 阴影：0 不处理，>0 提亮阴影，<0 压暗阴影。
    pub shadows: f32,
    pub wb_mode: WbMode,
    /// 色温（K），仅在 `WbMode::Custom` 生效。
    pub temperature: f32,
    /// 色调（-100..=100），正值偏洋红，负值偏绿。
    pub tint: f32,
    pub demosaic: DemosaicMode,
    /// 预览长边上限；导出时传 `None`。
    pub max_edge: Option<u32>,
}

impl Default for RawDevelopParams {
    fn default() -> Self {
        Self {
            exposure_ev: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            wb_mode: WbMode::Camera,
            temperature: 6500.0,
            tint: 0.0,
            demosaic: DemosaicMode::Mhc,
            max_edge: None,
        }
    }
}

impl RawDevelopParams {
    /// 参数收敛到安全范围，避免 UI 传入极端值导致 NaN。
    pub fn sanitized(&self) -> Self {
        let mut out = self.clone();
        out.exposure_ev = finite(self.exposure_ev, 0.0).clamp(-5.0, 5.0);
        out.highlights = finite(self.highlights, 0.0).clamp(-1.0, 1.0);
        out.shadows = finite(self.shadows, 0.0).clamp(-1.0, 1.0);
        out.temperature = finite(self.temperature, 6500.0).clamp(2000.0, 12000.0);
        out.tint = finite(self.tint, 0.0).clamp(-100.0, 100.0);
        out.max_edge = out.max_edge.map(|edge| edge.clamp(64, 12000));
        out
    }
}

fn finite(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

/// 显影结果。
pub struct DevelopedImage {
    pub image: Image16,
    /// 实际使用的三通道增益与矩阵，写入日志与测试断言。
    pub report: DevelopReport,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopReport {
    pub gains: [f32; 3],
    pub wb_source: String,
    pub matrix: [[f32; 3]; 3],
    pub demosaic: String,
}

/// 相机 RGB → XYZ(D65) → 线性 sRGB。
const XYZ_TO_SRGB: [[f32; 3]; 3] = [
    [3.2404542, -1.5371385, -0.4985314],
    [-0.969266, 1.8760108, 0.0415560],
    [0.0556434, -0.2040259, 1.0572252],
];

pub fn develop(linear: &LinearImage, params: &RawDevelopParams) -> RawResult<DevelopedImage> {
    let params = params.sanitized();
    let rgb = demosaic(linear, params.demosaic)?;
    let (gains, wb_source) = white_balance(linear, &rgb, &params);
    let matrix = camera_to_srgb(&linear.camera_matrix);

    let exposure = 2.0f32.powf(params.exposure_ev);
    let highlights = params.highlights;
    let shadows = params.shadows;

    let width = rgb.width;
    let height = rgb.height;
    let pixels = rgb.pixel_count();
    let mut out: Image16 = ImageBuffer::new(width, height);
    out.as_mut()
        .par_chunks_mut(4)
        .enumerate()
        .for_each(|(index, pixel)| {
            let base = index * 3;
            let r = rgb.data[base] as f32 / 65535.0 * gains[0];
            let g = rgb.data[base + 1] as f32 / 65535.0 * gains[1];
            let b = rgb.data[base + 2] as f32 / 65535.0 * gains[2];

            let mut lr = matrix[0][0] * r + matrix[0][1] * g + matrix[0][2] * b;
            let mut lg = matrix[1][0] * r + matrix[1][1] * g + matrix[1][2] * b;
            let mut lb = matrix[2][0] * r + matrix[2][1] * g + matrix[2][2] * b;

            lr *= exposure;
            lg *= exposure;
            lb *= exposure;

            lr = tone(lr, highlights, shadows);
            lg = tone(lg, highlights, shadows);
            lb = tone(lb, highlights, shadows);

            pixel[0] = encode_srgb(lr);
            pixel[1] = encode_srgb(lg);
            pixel[2] = encode_srgb(lb);
            pixel[3] = u16::MAX;
        });
    let _ = pixels;

    let image = match params.max_edge {
        Some(edge) => crate::image_engine::buffer::fit_within(&out, edge),
        None => out,
    };

    Ok(DevelopedImage {
        image,
        report: DevelopReport {
            gains,
            wb_source,
            matrix,
            demosaic: rgb.algorithm,
        },
    })
}

/// 白平衡增益（绿色归一为 1）。
fn white_balance(
    linear: &LinearImage,
    rgb: &super::demosaic::RgbLinear,
    params: &RawDevelopParams,
) -> ([f32; 3], String) {
    match params.wb_mode {
        WbMode::Camera => {
            let wb = linear.white_balance;
            let green = if wb[1].abs() > 1e-4 { wb[1] } else { 1.0 };
            let gains = [
                (wb[0] / green).clamp(0.05, 20.0),
                1.0,
                (wb[2] / green).clamp(0.05, 20.0),
            ];
            (normalize_green(gains), "camera".to_string())
        }
        WbMode::Auto => {
            let (mut sum, mut count) = ([0.0f64; 3], 0u64);
            // 采样步长，避免对超大图做全量统计。
            let step = (rgb.pixel_count() / 200_000).max(1);
            let mut index = 0usize;
            while index < rgb.pixel_count() {
                let base = index * 3;
                let pixel = [
                    rgb.data[base] as f64,
                    rgb.data[base + 1] as f64,
                    rgb.data[base + 2] as f64,
                ];
                // 剔除接近饱和的像素（高光会破坏灰度世界假设）。
                if pixel.iter().all(|value| *value < 60000.0) {
                    for channel in 0..3 {
                        sum[channel] += pixel[channel];
                    }
                    count += 1;
                }
                index += step;
            }
            if count == 0 || sum[0] <= 1.0 || sum[2] <= 1.0 {
                return white_balance(
                    linear,
                    rgb,
                    &RawDevelopParams {
                        wb_mode: WbMode::Camera,
                        ..params.clone()
                    },
                );
            }
            let mean = [
                sum[0] / count as f64,
                sum[1] / count as f64,
                sum[2] / count as f64,
            ];
            let gains = [
                (mean[1] / mean[0]).clamp(0.05, 20.0) as f32,
                1.0,
                (mean[1] / mean[2]).clamp(0.05, 20.0) as f32,
            ];
            (gains, "auto".to_string())
        }
        WbMode::Custom => {
            let reference = illuminant_srgb(6500.0);
            let target = illuminant_srgb(params.temperature);
            let gains = [
                (reference[0] / target[0]).clamp(0.05, 20.0),
                1.0,
                (reference[2] / target[2]).clamp(0.05, 20.0),
            ];
            let mut gains = normalize_green(gains);
            // 色调：正值偏洋红（降低绿通道），负值偏绿。
            gains[1] *= (1.0 - params.tint * 0.004).clamp(0.55, 1.8);
            (normalize_green(gains), "custom".to_string())
        }
    }
}

fn normalize_green(mut gains: [f32; 3]) -> [f32; 3] {
    let green = if gains[1].abs() > 1e-4 { gains[1] } else { 1.0 };
    gains[0] /= green;
    gains[1] = 1.0;
    gains[2] /= green;
    gains
}

/// 行归一化后的相机矩阵 × XYZ→sRGB。中性输入保持中性。
fn camera_to_srgb(camera: &[[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let mut normalized = *camera;
    for row in normalized.iter_mut() {
        let sum: f32 = row.iter().sum();
        if sum.abs() > 1e-6 {
            for value in row.iter_mut() {
                *value /= sum;
            }
        } else {
            *row = match row.as_ptr() as usize {
                0 => [1.0, 0.0, 0.0],
                _ => [0.0, 1.0, 0.0],
            };
        }
    }
    if normalized
        .iter()
        .all(|row| row.iter().all(|value| *value == 0.0))
    {
        return [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    }
    let mut out = [[0.0f32; 3]; 3];
    for row in 0..3 {
        for col in 0..3 {
            out[row][col] = (0..3)
                .map(|k| XYZ_TO_SRGB[row][k] * normalized[k][col])
                .sum();
        }
    }
    out
}

/// 黑体轨迹近似（Kim et al. 2002），返回色度坐标 xy。
fn planckian_xy(temperature: f32) -> (f32, f32) {
    let t = temperature.clamp(1667.0, 25000.0);
    let t2 = t * t;
    let t3 = t2 * t;
    let x = if t <= 4000.0 {
        -0.2661239e9 / t3 - 0.2343589e6 / t2 + 0.8776956e3 / t + 0.179910
    } else {
        -3.0258469e9 / t3 + 2.107038e6 / t2 + 0.2226347e3 / t + 0.240390
    };
    let y = if t <= 2222.0 {
        -1.1063814 * x * x * x - 1.3481102 * x * x + 2.1855583 * x - 0.20219683
    } else if t <= 4000.0 {
        -0.9549476 * x * x * x - 1.374186 * x * x + 2.0913701 * x - 0.16748867
    } else {
        3.081758 * x * x * x - 5.8733867 * x * x + 3.75113 * x - 0.37001483
    };
    (x, y.max(1e-4))
}

/// 黑体光源在 sRGB 线性空间的三刺激值（绿色归一为 1）。
fn illuminant_srgb(temperature: f32) -> [f32; 3] {
    let (x, y) = planckian_xy(temperature);
    let xyz = [x / y, 1.0, (1.0 - x - y) / y];
    let mut rgb = [0.0f32; 3];
    for (row, value) in rgb.iter_mut().enumerate() {
        *value = (0..3)
            .map(|k| XYZ_TO_SRGB[row][k] * xyz[k])
            .sum::<f32>()
            .max(1e-4);
    }
    let green = rgb[1];
    [rgb[0] / green, 1.0, rgb[2] / green]
}

/// 线性空间的高光压缩与阴影提亮。
fn tone(value: f32, highlights: f32, shadows: f32) -> f32 {
    let mut v = value.max(0.0);
    if highlights > 0.0 {
        // 端点保持的软压缩：0→0，1→1，中间段被压向高光端。
        let knee = 0.65;
        if v > knee {
            let t = (v - knee) / (1.0 - knee);
            let k = highlights * 2.0;
            let compressed = t * (1.0 + k) / (1.0 + k * t);
            v = knee + (1.0 - knee) * compressed;
        }
    } else if highlights < 0.0 {
        v *= 1.0 + 0.35 * highlights;
    }
    if shadows > 0.0 {
        v *= 1.0 + shadows * (1.0 - v).powi(2);
    } else if shadows < 0.0 {
        v *= 1.0 + 0.6 * shadows * (1.0 - v).powi(2);
    }
    v.clamp(0.0, 1.0)
}

fn encode_srgb(value: f32) -> u16 {
    let v = value.clamp(0.0, 1.0);
    let encoded = if v <= 0.003_130_8 {
        12.92 * v
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    (encoded.clamp(0.0, 1.0) * 65535.0).round() as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_image() -> LinearImage {
        let width = 8u32;
        let height = 8u32;
        let mut data = Vec::with_capacity((width * height) as usize);
        for y in 0..height {
            for x in 0..width {
                let color = ((y % 2) * 2 + (x % 2)) as usize;
                data.push(match color {
                    0 => 2000,
                    1 | 3 => 4000,
                    _ => 1500,
                });
            }
        }
        LinearImage {
            width,
            height,
            data,
            cfa: "RGGB".into(),
            black_level: [0; 4],
            white_balance: [2.0, 1.0, 1.5, 1.0],
            white_level: 65535,
            xtrans: None,
            camera_matrix: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        }
    }

    #[test]
    fn exposure_changes_pixels_and_never_touches_source() {
        let image = test_image();
        let before = image.data.clone();
        let base = develop(&image, &RawDevelopParams::default()).unwrap();
        let brighter = develop(
            &image,
            &RawDevelopParams {
                exposure_ev: 1.0,
                ..Default::default()
            },
        )
        .unwrap();
        assert_ne!(base.image, brighter.image, "曝光必须真实改变像素");
        assert_eq!(image.data, before, "显影不得修改原始 CFA 数据");
    }

    #[test]
    fn shadows_and_highlights_move_in_expected_direction() {
        let image = test_image();
        let neutral = develop(&image, &RawDevelopParams::default()).unwrap();
        let lifted = develop(
            &image,
            &RawDevelopParams {
                shadows: 0.8,
                ..Default::default()
            },
        )
        .unwrap();
        let lifted_sum: u64 = lifted.image.as_raw().iter().map(|v| *v as u64).sum();
        let neutral_sum: u64 = neutral.image.as_raw().iter().map(|v| *v as u64).sum();
        assert!(lifted_sum > neutral_sum, "提亮阴影必须增加整体亮度");

        let recovered = develop(
            &image,
            &RawDevelopParams {
                highlights: 0.9,
                ..Default::default()
            },
        )
        .unwrap();
        let recovered_sum: u64 = recovered.image.as_raw().iter().map(|v| *v as u64).sum();
        assert!(recovered_sum <= neutral_sum, "高光压缩不得提升整体亮度");
    }

    #[test]
    fn custom_temperature_direction_is_correct() {
        let image = test_image();
        let warm = develop(
            &image,
            &RawDevelopParams {
                wb_mode: WbMode::Custom,
                temperature: 3000.0,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(warm.report.gains[2] > 1.0, "低色温应提升蓝通道增益");
        let cool = develop(
            &image,
            &RawDevelopParams {
                wb_mode: WbMode::Custom,
                temperature: 10000.0,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(cool.report.gains[2] < 1.0, "高色温应降低蓝通道增益");
    }

    #[test]
    fn camera_white_balance_normalizes_green() {
        let image = test_image();
        let out = develop(&image, &RawDevelopParams::default()).unwrap();
        assert_eq!(out.report.gains[1], 1.0);
        assert!((out.report.gains[0] - 2.0).abs() < 1e-4);
        assert!((out.report.gains[2] - 1.5).abs() < 1e-4);
    }

    #[test]
    fn extreme_params_are_sanitized() {
        let params = RawDevelopParams {
            exposure_ev: f32::NAN,
            temperature: 999_999.0,
            tint: f32::INFINITY,
            ..Default::default()
        }
        .sanitized();
        assert_eq!(params.exposure_ev, 0.0);
        assert_eq!(params.temperature, 12000.0);
        assert_eq!(params.tint, 0.0);
    }
}
