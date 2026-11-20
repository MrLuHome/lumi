use std::io::Cursor;
use std::path::Path;

use image::{DynamicImage, ImageDecoder, ImageReader};

use crate::error::{AppError, AppResult};
use crate::image_engine::buffer::{ensure_reasonable_size, Image16};

pub const SUPPORTED_EXTENSIONS: [&str; 4] = ["jpg", "jpeg", "png", "webp"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BitDepth {
    Eight,
    Sixteen,
}

impl BitDepth {
    pub fn bits(self) -> u8 {
        match self {
            BitDepth::Eight => 8,
            BitDepth::Sixteen => 16,
        }
    }
}

#[derive(Debug)]
pub struct DecodedImage {
    pub image: Image16,
    pub format: String,
    pub bit_depth: BitDepth,
}

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

/// 解码并按 EXIF Orientation 摆正；16-bit 源图保留 16-bit 精度。
pub fn decode(path: &Path) -> AppResult<DecodedImage> {
    if !is_supported_extension(path) {
        return Err(AppError::Unsupported(format!(
            "{} 不是支持的图片格式（JPG / PNG / WebP）",
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "文件".into())
        )));
    }
    let bytes = std::fs::read(path)?;
    if bytes.is_empty() {
        return Err(AppError::Decode(format!(
            "{} 是空文件",
            path.file_name().unwrap_or_default().to_string_lossy()
        )));
    }

    let reader = ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|error| AppError::Decode(error.to_string()))?;
    let mut decoder = reader
        .into_decoder()
        .map_err(|error| AppError::Decode(format!("无法识别图片数据：{error}")))?;
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let image = DynamicImage::from_decoder(decoder)
        .map_err(|error| AppError::Decode(format!("图片解码失败（可能已损坏）：{error}")))?;
    let mut image = image;
    image.apply_orientation(orientation);

    let color = image.color();
    let bit_depth = if matches!(
        color,
        image::ColorType::Rgba16 | image::ColorType::Rgb16 | image::ColorType::La16 | image::ColorType::L16
    ) {
        BitDepth::Sixteen
    } else {
        BitDepth::Eight
    };

    let (width, height) = (image.width(), image.height());
    if width == 0 || height == 0 {
        return Err(AppError::Decode("图片尺寸为 0".into()));
    }
    ensure_reasonable_size(width, height)?;

    Ok(DecodedImage {
        image: image.to_rgba16(),
        format: format_label(path),
        bit_depth,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    #[test]
    fn detects_supported_extensions_case_insensitively() {
        assert!(is_supported_extension(Path::new("/tmp/a.JPG")));
        assert!(is_supported_extension(Path::new("/tmp/a.webp")));
        assert!(!is_supported_extension(Path::new("/tmp/a.tiff")));
    }

    #[test]
    fn corrupt_file_returns_decode_error() {
        let path = std::env::temp_dir().join("lumi-corrupt-round2.jpg");
        std::fs::write(&path, b"not really a jpeg").unwrap();
        let error = decode(&path).unwrap_err();
        assert!(matches!(error, AppError::Decode(_)));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn sixteen_bit_png_keeps_depth() {
        let path = std::env::temp_dir().join("lumi-16bit-round2.png");
        let image: image::ImageBuffer<Rgba<u16>, Vec<u16>> =
            image::ImageBuffer::from_pixel(8, 8, Rgba([1000, 2000, 3000, 65535]));
        image.save(&path).unwrap();
        let decoded = decode(&path).unwrap();
        assert_eq!(decoded.bit_depth, BitDepth::Sixteen);
        assert_eq!(decoded.image.get_pixel(0, 0).0[0], 1000);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn decodes_eight_bit_png_with_full_range() {
        let path = std::env::temp_dir().join("lumi-8bit-round2.png");
        RgbaImage::from_pixel(4, 4, Rgba([255, 0, 128, 255]))
            .save(&path)
            .unwrap();
        let decoded = decode(&path).unwrap();
        assert_eq!(decoded.bit_depth, BitDepth::Eight);
        let pixel = decoded.image.get_pixel(0, 0).0;
        assert_eq!(pixel[0], 65535);
        assert_eq!(pixel[1], 0);
        std::fs::remove_file(&path).ok();
    }
}
