//! 局部滤波与颜色辅助函数。所有操作都只在蒙版包围盒内计算，
//! 避免大图全量扫描造成的内存与时间开销。

use crate::image_engine::buffer::CHANNEL_MAX;

/// 从 16-bit 像素取归一化亮度。
pub fn luma(pixel: [u16; 4]) -> f32 {
    (0.299 * pixel[0] as f32 + 0.587 * pixel[1] as f32 + 0.114 * pixel[2] as f32) / CHANNEL_MAX
}

/// 单通道滑动窗口均值，O(N)。
pub fn box_mean(data: &[f32], width: u32, height: u32, radius: u32) -> Vec<f32> {
    let w = width as usize;
    let h = height as usize;
    if data.len() != w * h || radius == 0 {
        return data.to_vec();
    }
    let r = radius as usize;
    let mut temp = vec![0f32; data.len()];
    let mut prefix = vec![0f32; w + 1];
    for y in 0..h {
        let row = y * w;
        prefix[0] = 0.0;
        for x in 0..w {
            prefix[x + 1] = prefix[x] + data[row + x];
        }
        for x in 0..w {
            let low = x.saturating_sub(r);
            let high = (x + r).min(w - 1);
            temp[row + x] = (prefix[high + 1] - prefix[low]) / (high - low + 1) as f32;
        }
    }
    let mut prefix = vec![0f32; h + 1];
    for x in 0..w {
        prefix[0] = 0.0;
        for y in 0..h {
            prefix[y + 1] = prefix[y] + temp[y * w + x];
        }
        for y in 0..h {
            let low = y.saturating_sub(r);
            let high = (y + r).min(h - 1);
            temp[y * w + x] = (prefix[high + 1] - prefix[low]) / (high - low + 1) as f32;
        }
    }
    temp
}

/// 自引导滤波（He et al.），用于边缘保护的皮肤平滑：
/// 平坦区域趋于均值，强边缘（睫毛、唇线、痣）保持。
pub fn guided_filter(plane: &[f32], width: u32, height: u32, radius: u32, eps: f32) -> Vec<f32> {
    if plane.len() != (width * height) as usize || radius == 0 {
        return plane.to_vec();
    }
    let mean = box_mean(plane, width, height, radius);
    let mut squared: Vec<f32> = plane.iter().map(|value| value * value).collect();
    let mut mean_squared = box_mean(&squared, width, height, radius);
    // 复用平方与平方均值缓冲存放 a/b，减少大图上的临时内存。
    for index in 0..plane.len() {
        let variance = (mean_squared[index] - mean[index] * mean[index]).max(0.0);
        let coefficient = variance / (variance + eps);
        mean_squared[index] = coefficient;
        squared[index] = (1.0 - coefficient) * mean[index];
    }
    let mean_a = box_mean(&mean_squared, width, height, radius);
    let mean_b = box_mean(&squared, width, height, radius);
    drop(mean);
    drop(mean_squared);
    drop(squared);
    plane
        .iter()
        .enumerate()
        .map(|(index, value)| mean_a[index] * value + mean_b[index])
        .collect()
}

/// 分块裁剪：返回 bbox 内的 RGB 平面（0..1）。
pub struct RgbPlanes {
    pub width: u32,
    pub height: u32,
    pub x0: i32,
    pub y0: i32,
    pub planes: [Vec<f32>; 3],
}

impl RgbPlanes {
    pub fn from_image(image: &crate::image_engine::buffer::Image16, mask: &super::SkinMask) -> Self {
        let width = mask.width;
        let height = mask.height;
        let mut planes = [
            vec![0f32; (width * height) as usize],
            vec![0f32; (width * height) as usize],
            vec![0f32; (width * height) as usize],
        ];
        for local_y in 0..height {
            for local_x in 0..width {
                let x = mask.x0 as u32 + local_x;
                let y = mask.y0 as u32 + local_y;
                if x >= image.width() || y >= image.height() {
                    continue;
                }
                let pixel = image.get_pixel(x, y).0;
                let index = (local_y * width + local_x) as usize;
                for channel in 0..3 {
                    planes[channel][index] = pixel[channel] as f32 / CHANNEL_MAX;
                }
            }
        }
        Self {
            width,
            height,
            x0: mask.x0,
            y0: mask.y0,
            planes,
        }
    }
}
