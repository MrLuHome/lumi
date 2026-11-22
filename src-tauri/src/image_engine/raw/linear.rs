//! 把 LibRaw unpack 出来的 u16 原始像素裁剪到有效区域，并做黑电平减法。
//!
//! 输出保持单通道 CFA（不去马赛克、不做 gamma、不乘白平衡增益），
//! 白平衡系数只随结构体一起传下去，保证后续阶段可以自行决定如何使用。

#[cfg(feature = "raw")]
use super::libraw_ffi::RawFile;
#[cfg(feature = "raw")]
use super::metadata::{cfa_color, cfa_pattern};
#[cfg(feature = "raw")]
use super::{RawError, RawResult};

/// 线性（已减黑电平）单通道 CFA 图像。
#[derive(Debug, Clone)]
pub struct LinearImage {
    pub width: u32,
    pub height: u32,
    /// 行主序单通道像素，长度 = `width * height`。
    pub data: Vec<u16>,
    /// 有效区域左上角的 CFA 排列（RGGB / BGGR / GRBG / GBRG / X-Trans）。
    pub cfa: String,
    /// 每个 CFA 色号的基准黑电平（不含逐像素模式）。
    pub black_level: [u32; 4],
    /// 相机 as-shot 白平衡系数（未乘到像素上）。
    pub white_balance: [f32; 4],
    /// 白点（`color.maximum`），0 表示文件未提供。
    pub white_level: u32,
    /// X-Trans 的 6×6 色号矩阵（仅 X-Trans 有值），供基础插值使用。
    pub xtrans: Option<[[u8; 6]; 6]>,
    /// 相机 RGB → XYZ 的 3×3 矩阵（来自文件 `cam_xyz`，未做归一化）。
    pub camera_matrix: [[f32; 3]; 3],
}

impl LinearImage {
    pub fn pixel_count(&self) -> usize {
        self.data.len()
    }
}

#[cfg(feature = "raw")]
pub(crate) fn from_raw(file: &RawFile) -> RawResult<LinearImage> {
    let pixels = file
        .pixels
        .as_ref()
        .ok_or_else(|| RawError::Unsupported("原始像素尚未解包".to_string()))?;

    let raw_width = file.raw_width as usize;
    let raw_height = file.raw_height as usize;
    if raw_width == 0 || raw_height == 0 || pixels.len() < raw_width * raw_height {
        return Err(RawError::Unsupported(format!(
            "原始像素缓冲不完整：{}×{} / {} 像素",
            raw_width,
            raw_height,
            pixels.len()
        )));
    }

    let width = if file.width > 0 {
        file.width as usize
    } else {
        raw_width.saturating_sub(file.left_margin as usize)
    };
    let height = if file.height > 0 {
        file.height as usize
    } else {
        raw_height.saturating_sub(file.top_margin as usize)
    };
    let left = (file.left_margin as usize).min(raw_width.saturating_sub(1));
    let top = (file.top_margin as usize).min(raw_height.saturating_sub(1));
    let width = width.min(raw_width - left);
    let height = height.min(raw_height - top);
    if width == 0 || height == 0 {
        return Err(RawError::Unsupported("裁剪后有效区域为空".to_string()));
    }

    let base = base_black(file);
    let pattern = pattern_black(file);
    let white_balance = effective_white_balance(file);

    let mut data = vec![0u16; width * height];
    for row in 0..height {
        let src_row = (top + row) * raw_width + left;
        let dst_row = row * width;
        match &pattern {
            Some((rows, cols, offsets)) => {
                for col in 0..width {
                    let value = pixels[src_row + col] as i32;
                    let color = cfa_color(
                        file.filters,
                        top as u32 + row as u32,
                        left as u32 + col as u32,
                    ) as usize;
                    let offset = offsets[(row % rows) * cols + (col % cols)] as i32;
                    let black = base[color] as i32 + offset;
                    data[dst_row + col] = (value - black).clamp(0, u16::MAX as i32) as u16;
                }
            }
            None => {
                for col in 0..width {
                    let value = pixels[src_row + col] as i32;
                    let color = cfa_color(
                        file.filters,
                        top as u32 + row as u32,
                        left as u32 + col as u32,
                    ) as usize;
                    let black = base[color] as i32;
                    data[dst_row + col] = (value - black).clamp(0, u16::MAX as i32) as u16;
                }
            }
        }
    }

    Ok(LinearImage {
        width: width as u32,
        height: height as u32,
        data,
        cfa: cfa_pattern(
            file.filters,
            file.colors,
            file.xtrans,
            top as u32,
            left as u32,
        )
        .pattern,
        black_level: base,
        white_balance,
        white_level: file.maximum,
        xtrans: file.xtrans,
        camera_matrix: [
            [file.cam_xyz[0][0], file.cam_xyz[0][1], file.cam_xyz[0][2]],
            [file.cam_xyz[1][0], file.cam_xyz[1][1], file.cam_xyz[1][2]],
            [file.cam_xyz[2][0], file.cam_xyz[2][1], file.cam_xyz[2][2]],
        ],
    })
}

#[cfg(feature = "raw")]
/// 每通道基准黑电平：优先使用 `cblack[0..4]`，全 0 时回退到 `black`。
fn base_black(file: &RawFile) -> [u32; 4] {
    let cblack = &file.cblack;
    let has_per_color = cblack.iter().take(4).any(|value| *value > 0);
    if has_per_color && cblack.len() >= 4 {
        [cblack[0], cblack[1], cblack[2], cblack[3]]
    } else {
        [file.black; 4]
    }
}

#[cfg(feature = "raw")]
/// 逐像素黑电平模式（`cblack[4] × cblack[5]`），坐标基于有效区域。
fn pattern_black(file: &RawFile) -> Option<(usize, usize, Vec<u32>)> {
    let cblack = &file.cblack;
    if cblack.len() < 6 {
        return None;
    }
    let rows = cblack[4] as usize;
    let cols = cblack[5] as usize;
    if rows == 0 || cols == 0 || rows * cols > 64 {
        return None;
    }
    let start = 6;
    if cblack.len() < start + rows * cols {
        return None;
    }
    Some((rows, cols, cblack[start..start + rows * cols].to_vec()))
}

#[cfg(feature = "raw")]
/// `cam_mul` 全 0 时回退到 `pre_mul`（LibRaw 对未提供 as-shot 系数的机型的约定）。
pub(crate) fn effective_white_balance(file: &RawFile) -> [f32; 4] {
    if file.cam_mul.iter().all(|value| *value == 0.0) {
        file.pre_mul
    } else {
        file.cam_mul
    }
}
