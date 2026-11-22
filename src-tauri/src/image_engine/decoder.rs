use std::io::Cursor;
use std::path::Path;

use image::{DynamicImage, ImageDecoder, ImageReader};

use crate::error::{AppError, AppResult};
use crate::image_engine::buffer::{ensure_reasonable_size, Image16};

pub const SUPPORTED_EXTENSIONS: [&str; 6] = ["jpg", "jpeg", "png", "webp", "tif", "tiff"];

/// RAW 扩展名：当前 Beta 未开放导入（LibRaw 许可仍待审核）。
const RAW_EXTENSIONS: [&str; 20] = [
    "cr2", "cr3", "crw", "nef", "nrw", "arw", "srf", "sr2", "rw2", "orf", "raf", "dng", "pef",
    "srw", "erf", "kdc", "mrw", "x3f", "3fr", "iiq",
];

fn extension_of(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
}

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
    extension_of(path)
        .map(|ext| SUPPORTED_EXTENSIONS.contains(&ext.as_str()))
        .unwrap_or(false)
}

/// 是否是 RAW 扩展名（当前 Beta 明确拒绝，不静默跳过）。
pub fn is_raw_extension(path: &Path) -> bool {
    extension_of(path)
        .map(|ext| RAW_EXTENSIONS.contains(&ext.as_str()))
        .unwrap_or(false)
}

pub fn format_label(path: &Path) -> String {
    match extension_of(path).as_deref() {
        Some("jpg") | Some("jpeg") => "JPEG".to_string(),
        Some("png") => "PNG".to_string(),
        Some("webp") => "WebP".to_string(),
        Some("tif") | Some("tiff") => "TIFF".to_string(),
        Some(other) => other.to_ascii_uppercase(),
        None => "未知".to_string(),
    }
}

/// 解码并按 EXIF Orientation 摆正；16-bit 源图保留 16-bit 精度。
pub fn decode(path: &Path) -> AppResult<DecodedImage> {
    let extension = extension_of(path).unwrap_or_default();
    if RAW_EXTENSIONS.contains(&extension.as_str()) {
        // 诚实报错：RAW 解码器存在，但本轮 Beta 未开放导入。
        return Err(AppError::Unsupported(format!(
            "{} 是 RAW 文件，当前 Beta 未开放 RAW 导入（LibRaw 许可审核中）",
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "文件".into())
        )));
    }
    if !is_supported_extension(path) {
        return Err(AppError::Unsupported(format!(
            "{} 不是支持的图片格式（JPG / PNG / WebP / TIFF）",
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
        image::ColorType::Rgba16
            | image::ColorType::Rgb16
            | image::ColorType::La16
            | image::ColorType::L16
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

    // 输入 ICC：文件内嵌 RGB Profile 先转换到 sRGB 工作空间，再做后续编辑。
    // 无 Profile、非 RGB 或损坏 Profile 时按 sRGB 处理（在色彩状态中标注）。
    let mut pixels = image.to_rgba16();
    if let Ok(Some(icc)) = crate::color_management::embed::extract_icc(&bytes) {
        if let Ok((converted, _label)) =
            crate::color_management::transform::from_input_profile(&pixels, Some(&icc))
        {
            pixels = converted;
        }
    }

    Ok(DecodedImage {
        image: pixels,
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
        assert!(is_supported_extension(Path::new("/tmp/a.tiff")));
        assert!(is_supported_extension(Path::new("/tmp/a.TIF")));
    }

    #[test]
    fn tiff_is_decoded_with_orientation_and_label() {
        let path = std::env::temp_dir().join("lumi-tiff-round10.tiff");
        RgbaImage::from_pixel(6, 4, Rgba([10, 20, 30, 255]))
            .save(&path)
            .unwrap();
        let decoded = decode(&path).unwrap();
        assert_eq!(decoded.format, "TIFF");
        assert_eq!(decoded.image.dimensions(), (6, 4));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn raw_is_rejected_with_explicit_message() {
        let path = std::env::temp_dir().join("lumi-fake-round10.cr3");
        std::fs::write(&path, b"fake raw").unwrap();
        let error = decode(&path).unwrap_err();
        assert!(error.to_string().contains("未开放 RAW"));
        std::fs::remove_file(&path).ok();
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

    #[test]
    fn embedded_rgb_profile_is_converted_to_srgb_workspace() {
        let plain = std::env::temp_dir().join("lumi-icc-plain-round7.png");
        let tagged = std::env::temp_dir().join("lumi-icc-tagged-round7.png");
        RgbaImage::from_pixel(4, 4, Rgba([90, 140, 200, 255]))
            .save(&plain)
            .unwrap();
        let icc = crate::color_management::profiles::builtin_bytes(
            crate::color_management::ProfileKind::ProPhoto,
        )
        .unwrap();
        let bytes = std::fs::read(&plain).unwrap();
        std::fs::write(
            &tagged,
            crate::color_management::embed::embed(bytes, &icc).unwrap(),
        )
        .unwrap();

        let before = decode(&plain).unwrap().image.get_pixel(0, 0).0;
        let after = decode(&tagged).unwrap().image.get_pixel(0, 0).0;
        // ProPhoto 原色与 sRGB 差异明显，转换必须真实改变像素而不是只读元数据。
        assert_ne!(before, after, "嵌入 ICC 应触发输入到 sRGB 的真实转换");
        assert_eq!(after[3], 65535, "Alpha 必须保留");

        std::fs::remove_file(&plain).ok();
        std::fs::remove_file(&tagged).ok();
    }
}
