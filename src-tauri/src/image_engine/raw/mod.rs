//! RAW 引擎（第七轮）：真实读取相机 RAW 的原始像素与元数据。
//!
//! 本阶段只做“读”，不做美化：
//! - 不调用 LibRaw 的 `dcraw_process`（不做去马赛克、色彩转换、gamma）；
//! - 不把 LibRaw 输出的 JPEG/预览当 RAW 结果；
//! - 像素保持单通道 CFA，只做黑电平减法，白平衡系数随结构体传递。
//!
//! 所有 `unsafe` 集中在 `libraw_ffi.rs`，对外只暴露安全 API。

#[cfg(feature = "raw")]
mod libraw_ffi;
mod linear;
mod metadata;

pub mod demosaic;
pub mod develop;
pub mod xtrans;

pub use demosaic::{DemosaicMode, RgbLinear};
pub use develop::{DevelopReport, DevelopedImage, RawDevelopParams, WbMode};
pub use linear::LinearImage;
pub use metadata::{BlackLevel, CfaInfo, ExifSummary, PreviewInfo, RawInfo, WhiteBalance};

use std::path::Path;

/// RAW 读取错误。统一转成中文信息，便于命令层直接展示。
#[derive(Debug, thiserror::Error)]
pub enum RawError {
    #[error("RAW 文件读写失败：{0}")]
    Io(#[from] std::io::Error),

    #[error("RAW 路径包含非法字符：{0}")]
    InvalidPath(String),

    #[error("LibRaw 打开 RAW 失败（{code}）：{message}")]
    Open { code: i32, message: String },

    #[error("LibRaw 解包原始像素失败（{code}）：{message}")]
    Unpack { code: i32, message: String },

    #[error("该 RAW 暂不支持：{0}")]
    Unsupported(String),
}

pub type RawResult<T> = Result<T, RawError>;

impl From<RawError> for crate::error::AppError {
    fn from(error: RawError) -> Self {
        match error {
            RawError::Io(inner) => crate::error::AppError::Io(inner),
            RawError::Unsupported(message) => crate::error::AppError::Unsupported(message),
            other => crate::error::AppError::Decode(other.to_string()),
        }
    }
}

/// 常见 RAW 扩展名。`tif/tiff` 不在此列，避免把普通 TIFF 当成相机 RAW。
pub const RAW_EXTENSIONS: &[&str] = &[
    "cr2", "cr3", "crw", "nef", "nrw", "arw", "srf", "sr2", "rw2", "orf", "raf", "dng", "pef",
    "srw", "erf", "kdc", "mrw", "x3f", "3fr", "fff", "iiq", "mos", "mef", "rwl", "raw",
];

/// 根据扩展名判断是否应按 RAW 解码。真正的可读性仍由 LibRaw 决定。
pub fn is_raw_path(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| RAW_EXTENSIONS.contains(&value.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// 内嵌预览（通常是相机生成的 JPEG 缩略图）。
#[derive(Debug, Clone)]
pub struct PreviewImage {
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

/// 读取元数据，不解包像素（速度快，适合图库列表）。
#[cfg(feature = "raw")]
pub fn raw_info(path: &Path) -> RawResult<RawInfo> {
    let file = libraw_ffi::open_metadata(path)?;
    let size = std::fs::metadata(path)?.len();
    let cfa = metadata::cfa_pattern(
        file.filters,
        file.colors,
        file.xtrans,
        file.top_margin,
        file.left_margin,
    );
    let cam_mul = linear::effective_white_balance(&file);
    let raw_cam_mul = file.cam_mul.to_vec();
    let source = if file.cam_mul.iter().all(|value| *value == 0.0) {
        "pre_mul"
    } else {
        "cam_mul"
    };
    let preview = if file.thumb_format != 0 && file.thumb_width > 0 && file.thumb_height > 0 {
        Some(PreviewInfo {
            format: libraw_ffi::preview_format_name(file.thumb_format),
            width: file.thumb_width,
            height: file.thumb_height,
            bytes: file.thumb_bytes,
        })
    } else {
        None
    };

    Ok(RawInfo {
        path: path.display().to_string(),
        file_size: size,
        width: file.width,
        height: file.height,
        raw_width: file.raw_width,
        raw_height: file.raw_height,
        top_margin: file.top_margin,
        left_margin: file.left_margin,
        cfa,
        black_level: BlackLevel {
            black: file.black,
            cblack: file.cblack.iter().take(6).copied().collect(),
        },
        white_balance: WhiteBalance {
            cam_mul: cam_mul.to_vec(),
            raw_cam_mul,
            pre_mul: file.pre_mul.to_vec(),
            source: source.to_string(),
        },
        camera_matrix: file
            .cam_xyz
            .iter()
            .take(3)
            .map(|row| row.to_vec())
            .collect(),
        make: file.make,
        model: file.model,
        exif: ExifSummary {
            iso: file.iso,
            shutter: file.shutter,
            aperture: file.aperture,
            focal_length: file.focal_len,
            timestamp: file.timestamp,
        },
        white_level: file.maximum,
        has_embedded_preview: file.thumb_count > 0 || file.thumb_format != 0,
        preview,
    })
}

/// 解包并返回有效区域内的线性 CFA 像素（已减黑电平，未做 gamma）。
#[cfg(feature = "raw")]
pub fn decode_linear(path: &Path) -> RawResult<LinearImage> {
    let file = libraw_ffi::read_pixels(path)?;
    linear::from_raw(&file)
}

/// 一步完成“解包 → 去马赛克 → 显影”。
///
/// 预览与导出都调用同一函数，仅通过 `params.max_edge` 控制尺寸，
/// 保证两者算法完全一致。
pub fn develop_from_path(path: &Path, params: &RawDevelopParams) -> RawResult<DevelopedImage> {
    let linear = decode_linear(path)?;
    develop::develop(&linear, params)
}

/// 读取内嵌预览原始字节；文件没有可用预览时返回 `Ok(None)`。
#[cfg(feature = "raw")]
pub fn embedded_preview(path: &Path) -> RawResult<Option<PreviewImage>> {
    let preview = libraw_ffi::read_preview(path)?;
    Ok(preview.map(|preview| PreviewImage {
        format: libraw_ffi::preview_format_name(preview.format),
        width: preview.width,
        height: preview.height,
        data: preview.data,
    }))
}

// —— 不含 LibRaw 的发行构建（1.0 Beta）——
// 代码保留同一组 API，调用方无需分叉；所有 RAW 入口统一返回明确错误，
// 保证二进制里没有 LibRaw 符号，也不存在“假装成功”的路径。

#[cfg(not(feature = "raw"))]
const RAW_DISABLED: &str = "此构建未包含 RAW 解码（LibRaw 许可审核中）";

#[cfg(not(feature = "raw"))]
pub fn raw_info(_path: &Path) -> RawResult<RawInfo> {
    Err(RawError::Unsupported(RAW_DISABLED.into()))
}

#[cfg(not(feature = "raw"))]
pub fn decode_linear(_path: &Path) -> RawResult<LinearImage> {
    Err(RawError::Unsupported(RAW_DISABLED.into()))
}

#[cfg(not(feature = "raw"))]
pub fn embedded_preview(_path: &Path) -> RawResult<Option<PreviewImage>> {
    Err(RawError::Unsupported(RAW_DISABLED.into()))
}
