//! 去马赛克：把单通道 CFA 还原为线性 RGB。
//!
//! 默认使用 Malvar-He-Cutler（5×5 梯度修正线性插值），并保留朴素双线性作为对照。
//! 输出为归一化到白点的 16-bit 线性 RGB（行主序 3 通道），不做 gamma、
//! 不乘白平衡、不做色彩矩阵，这些统一在 `develop.rs` 中完成。

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use super::linear::LinearImage;
use super::{RawError, RawResult};

/// 去马赛克算法。`Mhc` 为默认；`Bilinear` 仅用于对照与诊断。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DemosaicMode {
    #[default]
    Mhc,
    Bilinear,
}

/// 线性 RGB 图像（16-bit，3 通道交错）。
#[derive(Debug, Clone)]
pub struct RgbLinear {
    pub width: u32,
    pub height: u32,
    /// 行主序 RGB，长度 = `width * height * 3`。
    pub data: Vec<u16>,
    /// 解码时使用的白点（用于日志/复算）。
    pub white_level: u32,
    /// 实际使用的算法描述，供 UI 与报告展示。
    pub algorithm: String,
}

impl RgbLinear {
    #[inline]
    pub fn pixel(&self, x: u32, y: u32) -> [u16; 3] {
        let index = ((y as usize * self.width as usize) + x as usize) * 3;
        [self.data[index], self.data[index + 1], self.data[index + 2]]
    }

    pub fn pixel_count(&self) -> usize {
        self.width as usize * self.height as usize
    }
}

/// 2×2 Bayer 排列 → 每个位置的色号（0=R, 1=G, 2=B）。
fn bayer_map(pattern: &str) -> Option<[[u8; 2]; 2]> {
    let bytes = pattern.as_bytes();
    if bytes.len() != 4 {
        return None;
    }
    let mut map = [[0u8; 2]; 2];
    for (index, byte) in bytes.iter().enumerate() {
        let color = match byte {
            b'R' => 0,
            b'G' => 1,
            b'B' => 2,
            _ => return None,
        };
        map[index / 2][index % 2] = color;
    }
    Some(map)
}

pub fn demosaic(image: &LinearImage, mode: DemosaicMode) -> RawResult<RgbLinear> {
    if image.width == 0 || image.height == 0 {
        return Err(RawError::Unsupported("CFA 尺寸为 0".to_string()));
    }
    let white = if image.white_level > 0 {
        image.white_level
    } else {
        u16::MAX as u32
    };
    let scale = 1.0 / white as f32;

    if image.cfa == "X-Trans" {
        let matrix = image
            .xtrans
            .ok_or_else(|| RawError::Unsupported("X-Trans 缺少 6×6 排列矩阵".to_string()))?;
        return Ok(super::xtrans::interpolate(image, &matrix, scale));
    }

    let map = bayer_map(&image.cfa)
        .ok_or_else(|| RawError::Unsupported(format!("无法识别的 CFA 排列：{}", image.cfa)))?;

    let width = image.width as usize;
    let height = image.height as usize;
    let sample = |x: isize, y: isize| -> f32 {
        let x = x.clamp(0, width as isize - 1) as usize;
        let y = y.clamp(0, height as isize - 1) as usize;
        image.data[y * width + x] as f32 * scale
    };
    let color_at = |x: usize, y: usize| -> u8 { map[y & 1][x & 1] };

    let mut data = vec![0u16; width * height * 3];
    data.par_chunks_mut(3)
        .enumerate()
        .for_each(|(index, pixel)| {
            let x = index % width;
            let y = index / width;
            let (xi, yi) = (x as isize, y as isize);
            let center = sample(xi, yi);
            let center_color = color_at(x, y);

            let rgb = match mode {
                DemosaicMode::Bilinear => {
                    let mut sums = [0.0f32; 3];
                    let mut counts = [0u32; 3];
                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            let color = color_at(
                                (xi + dx).clamp(0, width as isize - 1) as usize,
                                (yi + dy).clamp(0, height as isize - 1) as usize,
                            );
                            sums[color as usize] += sample(xi + dx, yi + dy);
                            counts[color as usize] += 1;
                        }
                    }
                    let mut out = [0.0f32; 3];
                    for channel in 0..3 {
                        out[channel] = if counts[channel] == 0 {
                            0.0
                        } else {
                            sums[channel] / counts[channel] as f32
                        };
                    }
                    out[center_color as usize] = center;
                    out
                }
                DemosaicMode::Mhc => mhc_pixel(&sample, &color_at, x, y, center, center_color),
            };

            pixel[0] = rgb[0].clamp(0.0, 1.0).mul_add(65535.0, 0.5) as u16;
            pixel[1] = rgb[1].clamp(0.0, 1.0).mul_add(65535.0, 0.5) as u16;
            pixel[2] = rgb[2].clamp(0.0, 1.0).mul_add(65535.0, 0.5) as u16;
        });

    Ok(RgbLinear {
        width: image.width,
        height: image.height,
        data,
        white_level: white,
        algorithm: match mode {
            DemosaicMode::Mhc => "Malvar-He-Cutler 5×5".to_string(),
            DemosaicMode::Bilinear => "双线性 3×3".to_string(),
        },
    })
}

#[inline]
fn mhc_pixel(
    sample: &impl Fn(isize, isize) -> f32,
    color_at: &impl Fn(usize, usize) -> u8,
    x: usize,
    y: usize,
    center: f32,
    center_color: u8,
) -> [f32; 3] {
    let (xi, yi) = (x as isize, y as isize);
    let n = sample(xi, yi - 1);
    let s = sample(xi, yi + 1);
    let e = sample(xi + 1, yi);
    let w = sample(xi - 1, yi);
    let nn = sample(xi, yi - 2);
    let ss = sample(xi, yi + 2);
    let ee = sample(xi + 2, yi);
    let ww = sample(xi - 2, yi);
    let ne = sample(xi + 1, yi - 1);
    let nw = sample(xi - 1, yi - 1);
    let se = sample(xi + 1, yi + 1);
    let sw = sample(xi - 1, yi + 1);

    let mut rgb = [0.0f32; 3];
    rgb[center_color as usize] = center;

    if center_color == 1 {
        // 绿点：R 与 B 分别在水平轴与垂直轴上（或反之）。
        // 用绿点所在行/列判断 R 的轴向，B 取另一轴。
        let horizontal = color_at(x.saturating_add(1), y);
        let r_horizontal = horizontal == 0;
        rgb[0] = channel_at_green(
            center,
            e,
            w,
            n,
            s,
            ee,
            ww,
            nn,
            ss,
            ne,
            nw,
            se,
            sw,
            r_horizontal,
        );
        rgb[2] = channel_at_green(
            center,
            e,
            w,
            n,
            s,
            ee,
            ww,
            nn,
            ss,
            ne,
            nw,
            se,
            sw,
            !r_horizontal,
        );
    } else {
        // R/B 点：G = 4 邻域平均 + α·Δ(R/B)，α = 1/2；另一色 = 对角平均 + γ·Δ，γ = 3/4。
        rgb[1] = (8.0 * center + 4.0 * (n + s + e + w) - 2.0 * (nn + ss + ee + ww)) / 16.0;
        let other = if center_color == 0 { 2usize } else { 0usize };
        rgb[other] = (12.0 * center + 4.0 * (ne + nw + se + sw) - 3.0 * (nn + ss + ee + ww)) / 16.0;
    }

    rgb
}

/// 绿点上的目标通道插值（Malvar-He-Cutler 5×5，系数按 16 缩放）。
///
/// `horizontal` 为 true 表示目标通道位于水平轴（E/W），否则位于垂直轴（N/S）。
/// 交叉轴向距离 2 的绿色样本系数为 +1，目标通道轴向距离 2 的绿色样本为 -2，
/// 四个对角绿色样本为 -2，中心与轴向距离 1 的目标色样本为 10 与 8。
#[inline]
#[allow(clippy::too_many_arguments)]
fn channel_at_green(
    center: f32,
    e: f32,
    w: f32,
    n: f32,
    s: f32,
    ee: f32,
    ww: f32,
    nn: f32,
    ss: f32,
    ne: f32,
    nw: f32,
    se: f32,
    sw: f32,
    horizontal: bool,
) -> f32 {
    let (a1, a2, along2a, along2b, cross2a, cross2b) = if horizontal {
        (e, w, ee, ww, nn, ss)
    } else {
        (n, s, nn, ss, ee, ww)
    };
    (10.0 * center + 8.0 * (a1 + a2) + (cross2a + cross2b)
        - 2.0 * (along2a + along2b)
        - 2.0 * (ne + nw + se + sw))
        / 16.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic(pattern: &str) -> LinearImage {
        // 6×6 CFA：R=1000, G=2000, B=3000，纯色块便于检查通道落位。
        let colors: Vec<u8> = pattern
            .bytes()
            .map(|byte| match byte {
                b'R' => 0,
                b'G' => 1,
                _ => 2,
            })
            .collect();
        let (width, height) = (8u32, 8u32);
        let mut data = Vec::with_capacity((width * height) as usize);
        for y in 0..height {
            for x in 0..width {
                let color = colors[((y % 2) * 2 + (x % 2)) as usize];
                data.push(match color {
                    0 => 1000,
                    1 => 2000,
                    _ => 3000,
                });
            }
        }
        LinearImage {
            width,
            height,
            data,
            cfa: pattern.to_string(),
            black_level: [0; 4],
            white_balance: [1.0; 4],
            white_level: 65535,
            xtrans: None,
            camera_matrix: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        }
    }

    #[test]
    fn bayer_orders_reconstruct_flat_colors() {
        for pattern in ["RGGB", "BGGR", "GRBG", "GBRG"] {
            let image = demosaic(&synthetic(pattern), DemosaicMode::Mhc).unwrap();
            // 仅检查内部像素：边缘通过坐标钳制采样，不参与精度断言。
            for y in 2..image.height - 2 {
                for x in 2..image.width - 2 {
                    let [r, g, b] = image.pixel(x, y);
                    // 均匀色块下每个像素都必须还原为 (R=1000, G=2000, B=3000)。
                    assert!(
                        (r as i32 - 1000).abs() <= 3
                            && (g as i32 - 2000).abs() <= 3
                            && (b as i32 - 3000).abs() <= 3,
                        "{pattern} 在 ({x},{y}) 通道还原错误：{r},{g},{b}"
                    );
                }
            }
        }
    }

    #[test]
    fn bayer_phase_is_preserved() {
        // 只点亮左上角 R：RGGB 的第一个像素必须是红色通道最强。
        let mut image = synthetic("RGGB");
        for value in image.data.iter_mut() {
            *value = 0;
        }
        image.data[0] = 60000;
        let rgb = demosaic(&image, DemosaicMode::Mhc).unwrap();
        let [r, g, b] = rgb.pixel(0, 0);
        assert!(r > g && r > b, "RGGB 相位错误：{r},{g},{b}");
    }

    #[test]
    fn bilinear_runs_and_matches_flat_field() {
        let image = demosaic(&synthetic("GRBG"), DemosaicMode::Bilinear).unwrap();
        let [r, g, b] = image.pixel(3, 3);
        assert!((r as i32 - 1000).abs() <= 3);
        assert!((g as i32 - 2000).abs() <= 3);
        assert!((b as i32 - 3000).abs() <= 3);
    }

    #[test]
    fn unknown_pattern_is_rejected() {
        let mut image = synthetic("RGGB");
        image.cfa = "MONO".into();
        assert!(demosaic(&image, DemosaicMode::Mhc).is_err());
    }
}
