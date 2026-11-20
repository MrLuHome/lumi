use std::path::Path;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::codecs::webp::WebPEncoder;
use image::{ExtendedColorType, ImageEncoder, RgbaImage};

use crate::error::{AppError, AppResult};

pub fn to_data_url(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", STANDARD.encode(bytes))
}

fn rgb_bytes(image: &RgbaImage) -> Vec<u8> {
    let mut out = Vec::with_capacity((image.width() * image.height() * 3) as usize);
    for pixel in image.pixels() {
        out.extend_from_slice(&pixel.0[..3]);
    }
    out
}

fn jpeg_bytes(image: &RgbaImage, quality: u8) -> AppResult<Vec<u8>> {
    let mut buffer = Vec::new();
    let rgb = rgb_bytes(image);
    JpegEncoder::new_with_quality(&mut buffer, quality)
        .write_image(&rgb, image.width(), image.height(), ExtendedColorType::Rgb8)
        .map_err(|err| AppError::Encode(err.to_string()))?;
    Ok(buffer)
}

fn png_bytes(image: &RgbaImage) -> AppResult<Vec<u8>> {
    let mut buffer = Vec::new();
    PngEncoder::new(&mut buffer)
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(|err| AppError::Encode(err.to_string()))?;
    Ok(buffer)
}

/// 预览图统一编码：不透明图用 JPEG 换取交互速度，带透明通道的用 PNG 保真。
pub fn preview_data_url(image: &RgbaImage, has_alpha: bool) -> AppResult<String> {
    if has_alpha {
        Ok(to_data_url("image/png", &png_bytes(image)?))
    } else {
        Ok(to_data_url("image/jpeg", &jpeg_bytes(image, 90)?))
    }
}

pub fn thumb_data_url(image: &RgbaImage, has_alpha: bool) -> AppResult<String> {
    if has_alpha {
        Ok(to_data_url("image/png", &png_bytes(image)?))
    } else {
        Ok(to_data_url("image/jpeg", &jpeg_bytes(image, 82)?))
    }
}

/// 按目标扩展名导出，返回写入字节数。
pub fn write_to_path(image: &RgbaImage, path: &Path) -> AppResult<u64> {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .unwrap_or_else(|| "jpg".to_string());

    let bytes = match extension.as_str() {
        "png" => png_bytes(image)?,
        "webp" => {
            let mut buffer = Vec::new();
            WebPEncoder::new_lossless(&mut buffer)
                .write_image(
                    image.as_raw(),
                    image.width(),
                    image.height(),
                    ExtendedColorType::Rgba8,
                )
                .map_err(|err| AppError::Encode(err.to_string()))?;
            buffer
        }
        _ => jpeg_bytes(image, 92)?,
    };

    std::fs::write(path, &bytes)?;
    Ok(bytes.len() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GenericImageView, Rgba};

    #[test]
    fn opaque_image_encodes_as_jpeg_data_url() {
        let image = RgbaImage::from_pixel(8, 8, Rgba([120, 40, 200, 255]));
        let url = preview_data_url(&image, false).unwrap();
        assert!(url.starts_with("data:image/jpeg;base64,"));
    }

    #[test]
    fn alpha_image_encodes_as_png_data_url() {
        let image = RgbaImage::from_pixel(8, 8, Rgba([120, 40, 200, 100]));
        let url = preview_data_url(&image, true).unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn export_roundtrips_to_jpeg_file() {
        let dir = std::env::temp_dir().join("lumi-export-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("out.jpg");
        let image = RgbaImage::from_pixel(16, 16, Rgba([10, 200, 90, 255]));
        let bytes = write_to_path(&image, &path).unwrap();
        assert!(bytes > 0);
        let decoded = crate::imaging::decode::decode(&path).unwrap();
        assert_eq!(decoded.dimensions(), (16, 16));
        std::fs::remove_file(&path).ok();
    }
}
