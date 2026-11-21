use image::imageops::FilterType;
use image::{DynamicImage, ImageBuffer, Rgba, RgbaImage};
use rayon::prelude::*;

use crate::error::{AppError, AppResult};

/// 引擎内部统一使用 16-bit RGBA：8-bit 原图按 ×257 归一化，
/// 16-bit PNG/WebP 保留原始精度。所有算法都在这一个缓冲区上执行，
/// 预览与导出不会出现两套处理分支。
pub type Image16 = ImageBuffer<Rgba<u16>, Vec<u16>>;

pub const CHANNEL_MAX: f32 = 65535.0;

pub fn from_dynamic(image: &DynamicImage) -> Image16 {
    image.to_rgba16()
}

pub fn to_rgba8(image: &Image16) -> RgbaImage {
    RgbaImage::from_fn(image.width(), image.height(), |x, y| {
        let pixel = image.get_pixel(x, y).0;
        Rgba([
            (pixel[0] as f32 / 257.0).round() as u8,
            (pixel[1] as f32 / 257.0).round() as u8,
            (pixel[2] as f32 / 257.0).round() as u8,
            (pixel[3] as f32 / 257.0).round() as u8,
        ])
    })
}

pub fn resize(image: &Image16, width: u32, height: u32) -> Image16 {
    image::imageops::resize(image, width.max(1), height.max(1), FilterType::Lanczos3)
}

/// 等比缩放到长边不超过 `max_edge`；小于上限时保持原始像素。
pub fn fit_within(image: &Image16, max_edge: u32) -> Image16 {
    let (width, height) = image.dimensions();
    let longest = width.max(height);
    if longest <= max_edge || width == 0 || height == 0 {
        return image.clone();
    }
    let scale = max_edge as f32 / longest as f32;
    resize(
        image,
        ((width as f32 * scale).round() as u32).max(1),
        ((height as f32 * scale).round() as u32).max(1),
    )
}

pub fn has_alpha(image: &Image16) -> bool {
    image.pixels().any(|pixel| pixel.0[3] < 65535)
}

/// 每像素并行遍历，回调拿到归一化 RGB 与可写的 16-bit 像素。
pub fn par_pixels_mut<F>(image: &mut Image16, apply: F)
where
    F: Fn(u32, u32, &mut [u16]) + Sync,
{
    let width = image.width();
    if width == 0 || image.height() == 0 {
        return;
    }
    let raw: &mut [u16] = image.as_mut();
    raw.par_chunks_mut(4)
        .enumerate()
        .for_each(|(index, pixel)| {
            apply(index as u32 % width, index as u32 / width, pixel);
        });
}

pub fn estimated_bytes(width: u32, height: u32) -> usize {
    width as usize * height as usize * 8
}

/// 拒绝超过内存上限的图片，给出明确错误而不是让进程被 OOM 杀死。
pub fn ensure_reasonable_size(width: u32, height: u32) -> AppResult<()> {
    const MAX_PIXELS: u64 = 120_000_000;
    if width as u64 * height as u64 > MAX_PIXELS {
        return Err(AppError::Unsupported(format!(
            "图片尺寸 {width}×{height} 超过 1.2 亿像素上限"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eight_bit_roundtrip_keeps_values() {
        let source = RgbaImage::from_pixel(4, 3, Rgba([12, 200, 77, 255]));
        let wide = from_dynamic(&DynamicImage::ImageRgba8(source.clone()));
        assert_eq!(wide.get_pixel(0, 0).0[0], 12 * 257);
        assert_eq!(to_rgba8(&wide), source);
    }

    #[test]
    fn fit_within_respects_longest_edge() {
        let image: Image16 = Image16::from_pixel(4000, 2000, Rgba([1000, 2000, 3000, 65535]));
        let fitted = fit_within(&image, 1800);
        assert_eq!(fitted.dimensions(), (1800, 900));
    }

    #[test]
    fn oversized_images_are_rejected() {
        assert!(ensure_reasonable_size(20000, 20000).is_err());
        assert!(ensure_reasonable_size(4000, 3000).is_ok());
    }
}
