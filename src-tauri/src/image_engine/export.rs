use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::image_engine::buffer::Image16;
use crate::image_engine::decoder::BitDepth;
use crate::image_engine::encoder;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    #[default]
    Jpeg,
    Png,
    Webp,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Jpeg => "jpg",
            ExportFormat::Png => "png",
            ExportFormat::Webp => "webp",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ExportFormat::Jpeg => "JPEG",
            ExportFormat::Png => "PNG",
            ExportFormat::Webp => "WebP",
        }
    }

    pub fn from_extension(path: &Path) -> Option<Self> {
        match path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref()
        {
            Some("jpg") | Some("jpeg") => Some(ExportFormat::Jpeg),
            Some("png") => Some(ExportFormat::Png),
            Some("webp") => Some(ExportFormat::Webp),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExportSettings {
    pub format: ExportFormat,
    /// JPEG 质量 40..100。
    pub quality: u8,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            format: ExportFormat::Jpeg,
            quality: 92,
        }
    }
}

impl ExportSettings {
    pub fn clamped(mut self) -> Self {
        self.quality = self.quality.clamp(40, 100);
        self
    }

    /// 用户选择的扩展名优先于设置里的格式（保存对话框写什么后缀就按什么格式）。
    pub fn with_destination(self, destination: &Path) -> Self {
        match ExportFormat::from_extension(destination) {
            Some(format) => Self {
                format,
                quality: self.quality,
            },
            None => self,
        }
    }
}

#[derive(Debug)]
pub struct ExportOutcome {
    pub path: PathBuf,
    pub bytes: u64,
    pub format: String,
}

/// 写出渲染结果。用户目标路径永远不会与原始照片相同，防止覆盖原图。
pub fn write(
    destination: &Path,
    original: &Path,
    image: &Image16,
    settings: ExportSettings,
    source_depth: BitDepth,
) -> AppResult<ExportOutcome> {
    write_atomic(destination, original, image, settings, source_depth)
}

/// 原子写出：先写同目录临时文件，成功后重命名。
/// 取消或异常退出不会在目标位置留下半张损坏的图片。
pub fn write_atomic(
    destination: &Path,
    original: &Path,
    image: &Image16,
    settings: ExportSettings,
    source_depth: BitDepth,
) -> AppResult<ExportOutcome> {
    if same_path(destination, original) {
        return Err(AppError::Unsupported(
            "导出路径不能与原图相同，原图不会被覆盖".into(),
        ));
    }
    let settings = settings.clamped().with_destination(destination);
    let sixteen_bit = settings.format == ExportFormat::Png && source_depth == BitDepth::Sixteen;
    let bytes = match settings.format {
        ExportFormat::Jpeg => encoder::jpeg_bytes(image, settings.quality)?,
        ExportFormat::Png => encoder::png_bytes(image, sixteen_bit)?,
        ExportFormat::Webp => encoder::webp_bytes(image, true)?,
    };
    if let Some(parent) = destination.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    let file_name = destination
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "export".into());
    let temp = destination.with_file_name(format!(".lumi-tmp-{stamp}-{file_name}"));
    std::fs::write(&temp, &bytes)?;
    if let Err(error) = std::fs::rename(&temp, destination) {
        std::fs::remove_file(&temp).ok();
        return Err(error.into());
    }
    Ok(ExportOutcome {
        path: destination.to_path_buf(),
        bytes: bytes.len() as u64,
        format: settings.format.label().to_string(),
    })
}

fn same_path(left: &Path, right: &Path) -> bool {
    let canonical_left = std::fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf());
    let canonical_right = std::fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf());
    canonical_left == canonical_right
}

/// 默认导出文件名：保留原文件名主体，替换扩展名，绝不直接覆盖原图。
pub fn suggested_name(original: &Path, format: ExportFormat) -> String {
    let stem = original
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "lumi-export".into());
    format!("{stem}-lumi.{}", format.extension())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn suggested_name_keeps_stem_and_switches_extension() {
        let name = suggested_name(Path::new("/photos/portrait.png"), ExportFormat::Jpeg);
        assert_eq!(name, "portrait-lumi.jpg");
    }

    #[test]
    fn refuses_to_overwrite_original() {
        let dir = std::env::temp_dir().join("lumi-export-test");
        std::fs::create_dir_all(&dir).unwrap();
        let original = dir.join("source.png");
        image::RgbaImage::from_pixel(2, 2, Rgba([1, 2, 3, 255]))
            .save(&original)
            .unwrap();
        let image: Image16 = Image16::from_pixel(2, 2, Rgba([1, 2, 3, 65535]));
        let error = write(
            &original,
            &original,
            &image,
            ExportSettings::default(),
            BitDepth::Eight,
        )
        .unwrap_err();
        assert!(error.to_string().contains("不能与原图相同"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn writes_requested_format_from_extension() {
        let dir = std::env::temp_dir().join("lumi-export-format");
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("original.jpg");
        image::RgbImage::from_pixel(4, 4, image::Rgb([10, 20, 30]))
            .save(&source)
            .unwrap();
        let target = dir.join("out.png");
        let image: Image16 = Image16::from_pixel(4, 4, Rgba([4000, 8000, 12000, 65535]));
        let outcome = write(
            &target,
            &source,
            &image,
            ExportSettings::default(),
            BitDepth::Eight,
        )
        .unwrap();
        assert_eq!(outcome.format, "PNG");
        assert!(target.exists());
        std::fs::remove_dir_all(&dir).ok();
    }
}
