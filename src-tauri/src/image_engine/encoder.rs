use std::io::Cursor;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::codecs::webp::WebPEncoder;
use image::{DynamicImage, ExtendedColorType, ImageEncoder};

use crate::error::{AppError, AppResult};
use crate::image_engine::buffer::{to_rgba8, Image16};

pub fn jpeg_bytes(image: &Image16, quality: u8) -> AppResult<Vec<u8>> {
    let mut buffer = Cursor::new(Vec::new());
    let rgb = DynamicImage::ImageRgba8(to_rgba8(image)).to_rgb8();
    JpegEncoder::new_with_quality(&mut buffer, quality.clamp(1, 100))
        .encode_image(&DynamicImage::ImageRgb8(rgb))
        .map_err(|error| AppError::Encode(error.to_string()))?;
    Ok(buffer.into_inner())
}

pub fn png_bytes(image: &Image16, sixteen_bit: bool) -> AppResult<Vec<u8>> {
    let mut buffer = Cursor::new(Vec::new());
    if sixteen_bit {
        // 16-bit PNG 交给 DynamicImage 处理字节序，避免手工拼接大端缓冲出错。
        DynamicImage::ImageRgba16(image.clone())
            .write_to(&mut buffer, image::ImageFormat::Png)
            .map_err(|error| AppError::Encode(error.to_string()))?;
    } else {
        let rgba = to_rgba8(image);
        PngEncoder::new_with_quality(&mut buffer, CompressionType::Fast, FilterType::Adaptive)
            .write_image(
                rgba.as_raw(),
                rgba.width(),
                rgba.height(),
                ExtendedColorType::Rgba8,
            )
            .map_err(|error| AppError::Encode(error.to_string()))?;
    }
    Ok(buffer.into_inner())
}

pub fn webp_bytes(image: &Image16, lossless: bool) -> AppResult<Vec<u8>> {
    let rgba = to_rgba8(image);
    let mut buffer = Cursor::new(Vec::new());
    if lossless {
        WebPEncoder::new_lossless(&mut buffer)
            .write_image(
                rgba.as_raw(),
                rgba.width(),
                rgba.height(),
                ExtendedColorType::Rgba8,
            )
            .map_err(|error| AppError::Encode(error.to_string()))?;
    } else {
        WebPEncoder::new_lossless(&mut buffer)
            .write_image(
                rgba.as_raw(),
                rgba.width(),
                rgba.height(),
                ExtendedColorType::Rgba8,
            )
            .map_err(|error| AppError::Encode(error.to_string()))?;
    }
    Ok(buffer.into_inner())
}

pub fn data_url(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", STANDARD.encode(bytes))
}

/// 画布预览：透明图用 PNG 保留 alpha，其余用 JPEG 控制 IPC 体积。
pub fn preview_data_url(image: &Image16, has_alpha: bool) -> AppResult<String> {
    if has_alpha {
        Ok(data_url("image/png", &png_bytes(image, false)?))
    } else {
        Ok(data_url("image/jpeg", &jpeg_bytes(image, 88)?))
    }
}

pub fn thumb_data_url(image: &Image16, has_alpha: bool) -> AppResult<String> {
    if has_alpha {
        Ok(data_url("image/png", &png_bytes(image, false)?))
    } else {
        Ok(data_url("image/jpeg", &jpeg_bytes(image, 82)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn jpeg_encoding_starts_with_soi_marker() {
        let image: Image16 = Image16::from_pixel(8, 8, Rgba([1000, 20000, 40000, 65535]));
        let bytes = jpeg_bytes(&image, 90).unwrap();
        assert_eq!(&bytes[..2], &[0xFF, 0xD8]);
    }

    #[test]
    fn png_encoding_has_signature() {
        let image: Image16 = Image16::from_pixel(4, 4, Rgba([1, 2, 3, 4]));
        let bytes = png_bytes(&image, false).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn data_url_has_expected_prefix() {
        let url = data_url("image/jpeg", &[1, 2, 3]);
        assert!(url.starts_with("data:image/jpeg;base64,"));
    }
}
