//! 色彩管理命令：状态、Profile 列表、输出设置、软打样与 CMYK 读取。

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::State;

use crate::color_management::{self, embed, profiles, transform, ColorEngine, ColorOutputSettings};
use crate::error::{AppError, AppResult};
use crate::image_engine::cancel::CancelToken;
use crate::image_engine::params::EditParams;
use crate::image_engine::pipeline;
use crate::image_engine::preview;
use crate::state::{FaceRepository, ImageCache};

use crate::commands::batch::BatchEngine;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftProofResult {
    pub url: String,
    pub width: u32,
    pub height: u32,
    pub out_of_gamut_pixels: u64,
    pub total_pixels: u64,
    pub ratio: f32,
    pub max_delta_e00: f32,
    pub intent: String,
    pub profile: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CmykResult {
    pub url: String,
    pub width: u32,
    pub height: u32,
    pub quality: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayPreviewResult {
    pub url: String,
    pub width: u32,
    pub height: u32,
    /// `colorsync` 或 `srgb-fallback`。
    pub source: String,
    pub description: String,
    pub reason: String,
}

#[tauri::command]
pub fn color_status(engine: State<'_, ColorEngine>) -> color_management::ColorStatus {
    color_management::status(&engine)
}

#[tauri::command]
pub fn color_profiles() -> Vec<color_management::ProfileOption> {
    color_management::ProfileKind::all()
        .iter()
        .map(|kind| color_management::ProfileOption {
            kind: *kind,
            label: kind.label().to_string(),
            builtin: *kind != color_management::ProfileKind::Custom,
        })
        .collect()
}

#[tauri::command]
pub fn color_set_output(
    engine: State<'_, ColorEngine>,
    batch: State<'_, BatchEngine>,
    settings: ColorOutputSettings,
) -> AppResult<color_management::ColorStatus> {
    if settings.profile == color_management::ProfileKind::Custom {
        let path = settings
            .custom_path
            .as_ref()
            .ok_or_else(|| AppError::InvalidInput("请选择自定义 ICC 文件".into()))?;
        profiles::describe_file(Path::new(path))?;
    }
    // 内置 Profile 也必须真实可创建，避免保存了一个无法转换的设置。
    profiles::load(&settings)?;
    engine.set_output(settings.clone());
    batch.set_color(settings);
    Ok(color_management::status(&engine))
}

#[tauri::command]
pub fn color_check_profile(path: String) -> AppResult<profiles::ProfileSummary> {
    profiles::describe_file(Path::new(&path))
}

/// 软打样：真实渲染当前照片后做 sRGB→输出→sRGB 往返，统计 ΔE00。
#[tauri::command]
pub async fn soft_proof(
    state: State<'_, ImageCache>,
    repo: State<'_, FaceRepository>,
    engine: State<'_, ColorEngine>,
    path: String,
    params: EditParams,
    warn: Option<bool>,
) -> AppResult<SoftProofResult> {
    let cache = state.inner().clone();
    let repo = repo.inner().clone();
    let settings = engine.settings();
    tauri::async_runtime::spawn_blocking(move || {
        let cached = cache.load(Path::new(&path))?;
        let faces = repo
            .get(Path::new(&path))
            .map(|entry| entry.faces)
            .unwrap_or_default();
        let rendered = pipeline::render(
            &cached.full,
            &params,
            &faces,
            &CancelToken::default(),
        )?;
        let (preview_image, report) = transform::soft_proof(&rendered, &settings, warn.unwrap_or(true))?;
        let canvas = preview::encode_canvas(&preview_image, false)?;
        Ok(SoftProofResult {
            url: canvas.url,
            width: canvas.width,
            height: canvas.height,
            out_of_gamut_pixels: report.out_of_gamut_pixels,
            total_pixels: report.total_pixels,
            ratio: report.ratio,
            max_delta_e00: report.max_delta_e00,
            intent: settings.intent.label().to_string(),
            profile: settings.profile.label().to_string(),
        })
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

/// 显示器色彩预览：把渲染结果从 sRGB 转到 ColorSync 主显示器 Profile，
/// 并把显示 ICC 嵌入返回的 PNG，避免 WebView 再按 sRGB 二次转换。
#[tauri::command]
pub async fn display_preview(
    state: State<'_, ImageCache>,
    repo: State<'_, FaceRepository>,
    path: String,
    params: EditParams,
) -> AppResult<DisplayPreviewResult> {
    let cache = state.inner().clone();
    let repo = repo.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        use base64::Engine;
        let cached = cache.load(Path::new(&path))?;
        let faces = repo
            .get(Path::new(&path))
            .map(|entry| entry.faces)
            .unwrap_or_default();
        let rendered = pipeline::render(
            &cached.full,
            &params,
            &faces,
            &CancelToken::default(),
        )?;
        let (display_image, status) = color_management::display::to_display(&rendered)?;
        let preview_image =
            crate::image_engine::preview::build_preview(&display_image, 1800);
        let canvas = preview::encode_canvas(&preview_image, false)?;

        // 只有真实读取到显示器 Profile 时才标记，回退 sRGB 时保持普通 PNG。
        let url = if status.available {
            color_management::display::display_icc()
                .and_then(|icc| {
                    let data = canvas.url.strip_prefix("data:image/png;base64,")?;
                    let bytes = base64::engine::general_purpose::STANDARD.decode(data).ok()?;
                    let tagged = embed::embed(bytes, &icc).ok()?;
                    Some(format!(
                        "data:image/png;base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(tagged)
                    ))
                })
                .unwrap_or(canvas.url)
        } else {
            canvas.url
        };

        Ok(DisplayPreviewResult {
            url,
            width: canvas.width,
            height: canvas.height,
            source: status.source,
            description: status.description,
            reason: status.reason,
        })
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

/// CMYK TIFF → sRGB。优先使用文件内嵌 ICC；否则使用用户 ICC；都没有时
/// 使用通用公式并在 `quality` 中标注“基础”。
#[tauri::command]
pub async fn color_convert_cmyk(
    path: String,
    icc_path: Option<String>,
) -> AppResult<CmykResult> {
    tauri::async_runtime::spawn_blocking(move || {
        let source = PathBuf::from(&path);
        let bytes = std::fs::read(&source)?;
        let embedded = embed::extract_icc(&bytes).ok().flatten();
        let icc = match icc_path {
            Some(user) if !user.trim().is_empty() => Some(std::fs::read(&user)?),
            _ => embedded,
        };

        let file = std::fs::File::open(&source)?;
        let mut decoder = tiff::decoder::Decoder::new(std::io::BufReader::new(file))
            .map_err(|error| AppError::Decode(format!("打开 TIFF 失败：{error}")))?;
        let (width, height) = decoder
            .dimensions()
            .map_err(|error| AppError::Decode(format!("读取 TIFF 尺寸失败：{error}")))?;
        let color_type = decoder
            .colortype()
            .map_err(|error| AppError::Decode(format!("读取 TIFF 色彩类型失败：{error}")))?;
        if !matches!(color_type, tiff::ColorType::CMYK(_)) {
            return Err(AppError::Unsupported(format!(
                "该 TIFF 不是 CMYK（实际 {color_type:?}）"
            )));
        }
        let decoded = decoder
            .read_image()
            .map_err(|error| AppError::Decode(format!("解码 TIFF 失败：{error}")))?;
        let tiff::decoder::DecodingResult::U8(pixels) = decoded else {
            return Err(AppError::Unsupported("CMYK TIFF 仅支持 8-bit".into()));
        };
        let (image, quality) = transform::cmyk_to_srgb(&pixels, width, height, icc.as_deref())?;
        let canvas = preview::encode_canvas(&image, false)?;
        Ok(CmykResult {
            url: canvas.url,
            width: canvas.width,
            height: canvas.height,
            quality,
        })
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

/// 显示器 Profile 刷新（多显示器切换后手动调用）。
#[tauri::command]
pub fn display_profile_refresh() -> color_management::display::DisplayStatus {
    color_management::display::status()
}
