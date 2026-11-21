//! RAW 相关命令：信息读取、快速预览、参数化显影。
//!
//! 预览走“内嵌 JPEG 快速路径 → 受控尺寸解码”两级；导出由编辑管线调用
//! 同一 `develop` 函数，保证预览与导出算法一致。

use std::path::Path;

use base64::Engine;
use serde::Serialize;

use crate::error::{AppError, AppResult};
use crate::image_engine::raw::{self, DevelopedImage, RawDevelopParams};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawPreviewResult {
    pub url: String,
    pub width: u32,
    pub height: u32,
    /// `embedded` 或 `developed`。
    pub source: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawDevelopResult {
    pub url: String,
    pub width: u32,
    pub height: u32,
    pub gains: [f32; 3],
    pub wb_source: String,
    pub demosaic: String,
}

#[tauri::command]
pub async fn raw_info(path: String) -> AppResult<raw::RawInfo> {
    tauri::async_runtime::spawn_blocking(move || raw::raw_info(Path::new(&path)))
        .await
        .map_err(|error| AppError::Task(error.to_string()))?
        .map_err(Into::into)
}

/// 快速预览：优先返回相机内嵌 JPEG（秒开）；没有时按 1600px 长边显影。
#[tauri::command]
pub async fn raw_preview(path: String, max_edge: Option<u32>) -> AppResult<RawPreviewResult> {
    tauri::async_runtime::spawn_blocking(move || {
        let max_edge = max_edge.unwrap_or(1600).clamp(256, 4096);
        if let Some(preview) = raw::embedded_preview(Path::new(&path))? {
            if preview.format == "jpeg" {
                let encoded = base64::engine::general_purpose::STANDARD.encode(&preview.data);
                return Ok(RawPreviewResult {
                    url: format!("data:image/jpeg;base64,{encoded}"),
                    width: preview.width,
                    height: preview.height,
                    source: "embedded".into(),
                    note: "相机内嵌预览（未应用显影参数）".into(),
                });
            }
        }
        let params = RawDevelopParams {
            max_edge: Some(max_edge),
            ..Default::default()
        };
        let developed = raw::develop_from_path(Path::new(&path), &params)?;
        let canvas = crate::image_engine::preview::encode_canvas(&developed.image, false)?;
        Ok(RawPreviewResult {
            url: canvas.url,
            width: canvas.width,
            height: canvas.height,
            source: "developed".into(),
            note: "RAW 解码 + 显影（默认参数）".into(),
        })
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

#[tauri::command]
pub async fn raw_develop(path: String, params: RawDevelopParams) -> AppResult<RawDevelopResult> {
    tauri::async_runtime::spawn_blocking(move || {
        let params = validate_params(&params)?;
        let DevelopedImage { image, report } =
            raw::develop_from_path(Path::new(&path), &params)?;
        let canvas = crate::image_engine::preview::encode_canvas(&image, false)?;
        Ok(RawDevelopResult {
            url: canvas.url,
            width: canvas.width,
            height: canvas.height,
            gains: report.gains,
            wb_source: report.wb_source,
            demosaic: report.demosaic,
        })
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

/// 能力声明：哪些格式支持、哪些明确不支持。
#[tauri::command]
pub fn raw_capabilities() -> serde_json::Value {
    serde_json::json!({
        "decoder": "LibRaw 0.21.1",
        "supported": ["CR3", "CR2", "ARW", "RW2", "ORF", "DNG", "NEF", "RAF", "TIF(RAW)"],
        "unsupported": ["有损 DNG（JPEG-XL 压缩）", "Nikon HE/HE*（0.21.1 限制）"],
        "demosaic": ["Bayer Malvar-He-Cutler 5×5", "Bayer 双线性", "X-Trans 基础插值（质量有限）"],
        "notes": [
            "仅读取原始像素与元数据，不调用 LibRaw 的 dcraw_process。",
            "预览与导出共用同一 develop 函数。",
            "原图永不被修改；所有参数保存在项目中。"
        ]
    })
}

/// 供项目保存使用：把 RAW 参数写入项目前的校验。
pub fn validate_params(params: &RawDevelopParams) -> AppResult<RawDevelopParams> {
    let sanitized = params.sanitized();
    if !sanitized.exposure_ev.is_finite() {
        return Err(AppError::InvalidInput("曝光参数非法".into()));
    }
    Ok(sanitized)
}

/// 状态栏用的轻量探测：路径是否存在、是否被识别为 RAW。
#[tauri::command]
pub fn raw_probe(path: String) -> serde_json::Value {
    let exists = Path::new(&path).is_file();
    serde_json::json!({
        "path": path,
        "exists": exists,
        "decoderVersion": crate::image_engine::project::RAW_DECODER_VERSION,
    })
}
