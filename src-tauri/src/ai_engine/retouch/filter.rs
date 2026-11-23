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
///
/// `pad`：在 bbox 外扩采样的边距（像素）。滤波在含 halo 的平面上做，
/// 再裁回 bbox，消除 ROI 边界处 box/guided 窗口截断造成的横向条纹与矩形接缝。
pub struct RgbPlanes {
    pub width: u32,
    pub height: u32,
    pub x0: i32,
    pub y0: i32,
    pub planes: [Vec<f32>; 3],
}

impl RgbPlanes {
    pub fn from_image(
        image: &crate::image_engine::buffer::Image16,
        mask: &super::SkinMask,
    ) -> Self {
        Self::from_image_padded(image, mask, 0)
    }

    /// 从图像读取 mask 区域并外扩 `pad` 像素；返回的平面尺寸为
    /// `(width+2*pad) × (height+2*pad)`，`x0/y0` 为外扩后左上角。
    /// 滤波后需用 [`RgbPlanes::crop_center`] 裁回 mask 尺寸。
    pub fn from_image_padded(
        image: &crate::image_engine::buffer::Image16,
        mask: &super::SkinMask,
        pad: u32,
    ) -> Self {
        let pad = pad.min(64);
        let width = mask.width + pad * 2;
        let height = mask.height + pad * 2;
        let x0 = mask.x0 - pad as i32;
        let y0 = mask.y0 - pad as i32;
        let mut planes = [
            vec![0f32; (width * height) as usize],
            vec![0f32; (width * height) as usize],
            vec![0f32; (width * height) as usize],
        ];
        let img_w = image.width() as i32;
        let img_h = image.height() as i32;
        for local_y in 0..height {
            for local_x in 0..width {
                // 边缘复制，禁止用 0 填充（0 会把 box_mean 拉向黑，产生条纹）。
                let x = (x0 + local_x as i32).clamp(0, img_w - 1);
                let y = (y0 + local_y as i32).clamp(0, img_h - 1);
                let pixel = image.get_pixel(x as u32, y as u32).0;
                let index = (local_y * width + local_x) as usize;
                for channel in 0..3 {
                    planes[channel][index] = pixel[channel] as f32 / CHANNEL_MAX;
                }
            }
        }
        Self {
            width,
            height,
            x0,
            y0,
            planes,
        }
    }

    /// 外扩平面裁回中心 `out_w × out_h`（通常为 mask 尺寸）。
    pub fn crop_center(&self, out_w: u32, out_h: u32) -> Self {
        let pad_x = (self.width.saturating_sub(out_w)) / 2;
        let pad_y = (self.height.saturating_sub(out_h)) / 2;
        let mut planes = [
            vec![0f32; (out_w * out_h) as usize],
            vec![0f32; (out_w * out_h) as usize],
            vec![0f32; (out_w * out_h) as usize],
        ];
        for y in 0..out_h {
            for x in 0..out_w {
                let src = ((y + pad_y) * self.width + (x + pad_x)) as usize;
                let dst = (y * out_w + x) as usize;
                for channel in 0..3 {
                    planes[channel][dst] = self.planes[channel][src];
                }
            }
        }
        Self {
            width: out_w,
            height: out_h,
            x0: self.x0 + pad_x as i32,
            y0: self.y0 + pad_y as i32,
            planes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_engine::skin_segmentation::SkinMask;
    use crate::image_engine::buffer::Image16;
    use image::Rgba;

    #[test]
    fn padded_sampling_uses_edge_replication_not_black() {
        // Halo 外扩必须复制真实边缘像素，禁止用 0（黑色）填充代替。
        let mut img = Image16::from_pixel(16, 16, Rgba([40_000, 30_000, 20_000, 65535]));
        img.put_pixel(0, 0, Rgba([50_000, 40_000, 30_000, 65535]));
        let mask = SkinMask {
            x0: 4,
            y0: 4,
            width: 8,
            height: 8,
            alpha: vec![1.0; 64],
        };
        let padded = RgbPlanes::from_image_padded(&img, &mask, 6);
        // 左上角外扩点 (x0=-2,y0=-2) 被 clamp 到 (0,0) → 应为真实边缘像素，而非 0。
        let corner = padded.planes[0][0];
        assert!(
            corner > 0.7,
            "外扩区域必须是真实边缘像素（复制），不得黑填充 got={corner}"
        );
        // 所有平面值有限且非全零
        for c in 0..3 {
            assert!(padded.planes[c].iter().all(|v| v.is_finite() && *v >= 0.0));
            assert!(padded.planes[c].iter().any(|v| *v > 0.1));
        }
        // crop_center 能裁回 mask 尺寸
        let cropped = padded.crop_center(8, 8);
        assert_eq!((cropped.width, cropped.height), (8, 8));
        // 中心区域应等于原 ROI 像素
        let src = img.get_pixel(4, 4).0[0] as f32 / 65535.0;
        assert!((cropped.planes[0][0] - src).abs() < 1e-6);
    }
}
