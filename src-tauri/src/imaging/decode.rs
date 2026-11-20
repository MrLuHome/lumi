use std::io::Cursor;
use std::path::Path;

use image::imageops::FilterType;
use image::{DynamicImage, ImageDecoder, ImageReader, RgbaImage};

use crate::error::{AppError, AppResult};

/// 预览图长边上限。桌面端画布通常在 2000px 以内，超出的部分对预览没有意义，
/// 却会让每次参数回调的渲染时间成倍增加。
pub const PREVIEW_MAX_EDGE: u32 = 1800;
/// 左侧图库缩略图长边。
pub const THUMB_MAX_EDGE: u32 = 400;

const SUPPORTED_EXTENSIONS: [&str; 4] = ["jpg", "jpeg", "png", "webp"];

pub fn is_supported_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| SUPPORTED_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

pub fn format_label(path: &Path) -> String {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => "JPEG".to_string(),
        Some("png") => "PNG".to_string(),
        Some("webp") => "WebP".to_string(),
        Some(other) => other.to_ascii_uppercase(),
        None => "未知".to_string(),
    }
}

/// 解码任意受支持图片，并按 EXIF 方向摆正。
pub fn decode(path: &Path) -> AppResult<DynamicImage> {
    if !is_supported_extension(path) {
        return Err(AppError::Unsupported(format_label(path)));
    }
    let bytes = std::fs::read(path)?;
    let reader = ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|err| AppError::Decode(err.to_string()))?;
    let mut decoder = reader
        .into_decoder()
        .map_err(|err| AppError::Decode(err.to_string()))?;
    // 手机/相机竖拍照片依赖 EXIF Orientation，不处理会导致预览横置。
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut image = DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    Ok(image)
}

/// 等比缩放到长边不超过 `max_edge`；小于上限时保持原始像素。
pub fn fit_within(image: &RgbaImage, max_edge: u32) -> RgbaImage {
    let (width, height) = image.dimensions();
    let longest = width.max(height);
    if longest <= max_edge || width == 0 || height == 0 {
        return image.clone();
    }
    let scale = max_edge as f32 / longest as f32;
    let target_width = ((width as f32 * scale).round() as u32).max(1);
    let target_height = ((height as f32 * scale).round() as u32).max(1);
    image::imageops::resize(image, target_width, target_height, FilterType::Lanczos3)
}

/// 是否包含真实透明像素（决定预览编码用 PNG 还是 JPEG）。
pub fn has_alpha(image: &RgbaImage) -> bool {
    image.pixels().any(|pixel| pixel[3] < 255)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    #[test]
    fn detects_supported_extensions_case_insensitively() {
        assert!(is_supported_extension(Path::new("/tmp/a.JPG")));
        assert!(is_supported_extension(Path::new("/tmp/a.jpeg")));
        assert!(is_supported_extension(Path::new("/tmp/a.png")));
        assert!(is_supported_extension(Path::new("/tmp/a.webp")));
        assert!(!is_supported_extension(Path::new("/tmp/a.tiff")));
        assert!(!is_supported_extension(Path::new("/tmp/a")));
    }

    #[test]
    fn fit_within_keeps_small_images_untouched() {
        let image = RgbaImage::from_pixel(100, 60, Rgba([200, 100, 50, 255]));
        let fitted = fit_within(&image, THUMB_MAX_EDGE);
        assert_eq!(fitted.dimensions(), (100, 60));
    }

    #[test]
    fn fit_within_preserves_aspect_ratio() {
        let image = RgbaImage::new(4000, 3000);
        let fitted = fit_within(&image, 800);
        assert_eq!(fitted.dimensions(), (800, 600));
    }

    #[test]
    fn alpha_detection_ignores_opaque_images() {
        let opaque = RgbaImage::from_pixel(2, 2, Rgba([10, 20, 30, 255]));
        assert!(!has_alpha(&opaque));
        let translucent = RgbaImage::from_pixel(2, 2, Rgba([10, 20, 30, 128]));
        assert!(has_alpha(&translucent));
    }
}
