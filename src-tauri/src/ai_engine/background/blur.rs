//! 背景专用模糊。
//!
//! 使用可分离盒式模糊多次迭代近似高斯，半径按图像短边换算；
//! 只对背景平面操作，不触碰前景。

use crate::ai_engine::segmentation::alpha::box_blur;
use crate::image_engine::buffer::Image16;

/// `amount` 0..100 映射到短边 0..3% 的模糊半径。
pub fn radius_for(amount: f32, width: u32, height: u32) -> u32 {
    let short = width.min(height) as f32;
    ((amount.clamp(0.0, 100.0) / 100.0) * short * 0.03).round() as u32
}

pub fn blur(image: &Image16, radius_px: u32, passes: u32) -> Image16 {
    if radius_px == 0 {
        return image.clone();
    }
    let width = image.width();
    let height = image.height();
    let mut planes: [Vec<f32>; 4] = [
        Vec::with_capacity(width as usize * height as usize),
        Vec::with_capacity(width as usize * height as usize),
        Vec::with_capacity(width as usize * height as usize),
        Vec::with_capacity(width as usize * height as usize),
    ];
    for pixel in image.pixels() {
        for channel in 0..4 {
            planes[channel].push(pixel.0[channel] as f32);
        }
    }
    for plane in planes.iter_mut() {
        *plane = box_blur(plane, width, height, radius_px, passes);
    }
    Image16::from_fn(width, height, |x, y| {
        let index = y as usize * width as usize + x as usize;
        image::Rgba([
            planes[0][index].round().clamp(0.0, 65535.0) as u16,
            planes[1][index].round().clamp(0.0, 65535.0) as u16,
            planes[2][index].round().clamp(0.0, 65535.0) as u16,
            planes[3][index].round().clamp(0.0, 65535.0) as u16,
        ])
    })
}

/// 按权重平面混合两张同尺寸图像，`weight` 0 取 `near`，1 取 `far`。
pub fn blend(near: &Image16, far: &Image16, weight: &[f32]) -> Image16 {
    let width = near.width();
    let height = near.height();
    if far.dimensions() != near.dimensions() || weight.len() != width as usize * height as usize {
        return near.clone();
    }
    Image16::from_fn(width, height, |x, y| {
        let index = y as usize * width as usize + x as usize;
        let t = weight[index].clamp(0.0, 1.0);
        let a = near.get_pixel(x, y).0;
        let b = far.get_pixel(x, y).0;
        let mut out = [0u16; 4];
        for channel in 0..4 {
            out[channel] = (a[channel] as f32 * (1.0 - t) + b[channel] as f32 * t).round() as u16;
        }
        image::Rgba(out)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn radius_scales_with_short_side() {
        assert_eq!(radius_for(0.0, 1000, 800), 0);
        assert!(radius_for(100.0, 1000, 800) >= 20);
    }

    #[test]
    fn blur_preserves_uniform_image() {
        let image: Image16 = Image16::from_pixel(64, 48, Rgba([30000, 20000, 10000, 65535]));
        let blurred = blur(&image, 6, 2);
        let pixel = blurred.get_pixel(30, 20).0;
        assert!((pixel[0] as i32 - 30000).abs() <= 1);
        assert!((pixel[2] as i32 - 10000).abs() <= 1);
    }

    #[test]
    fn blur_softens_sharp_edge() {
        let image: Image16 = Image16::from_fn(32, 8, |x, _| {
            if x < 16 {
                Rgba([0, 0, 0, 65535])
            } else {
                Rgba([65535, 65535, 65535, 65535])
            }
        });
        let blurred = blur(&image, 4, 2);
        let edge = blurred.get_pixel(16, 4).0[0];
        assert!(edge > 0 && edge < 65535);
    }

    #[test]
    fn blend_respects_weight() {
        let near: Image16 = Image16::from_pixel(4, 4, Rgba([0, 0, 0, 65535]));
        let far: Image16 = Image16::from_pixel(4, 4, Rgba([10000, 10000, 10000, 65535]));
        let weight = vec![0.0f32; 16];
        assert_eq!(blend(&near, &far, &weight).get_pixel(1, 1).0[0], 0);
    }
}
