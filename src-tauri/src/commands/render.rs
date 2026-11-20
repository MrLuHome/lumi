use std::path::Path;
use std::time::Instant;

use serde::Serialize;
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::imaging::{adjustments, encode, EditParams};
use crate::state::ImageCache;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderResult {
    pub preview_url: String,
    pub width: u32,
    pub height: u32,
    /// 后端耗时，用于底部状态栏显示真实处理时间。
    pub elapsed_ms: u64,
}

/// 用当前参数渲染预览图。始终基于 preview（长边 ≤ 1800）计算，
/// 保证拖动滑杆时保持交互帧率；导出时才回到原始像素。
#[tauri::command]
pub async fn render_preview(
    state: State<'_, ImageCache>,
    path: String,
    params: EditParams,
) -> AppResult<RenderResult> {
    let cache = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let started = Instant::now();
        let cached = cache.load(Path::new(&path))?;
        let rendered = adjustments::render(&cached.preview, &params);
        let preview_url = encode::preview_data_url(&rendered, cached.has_alpha)?;
        Ok(RenderResult {
            preview_url,
            width: rendered.width(),
            height: rendered.height(),
            elapsed_ms: started.elapsed().as_millis() as u64,
        })
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub path: String,
    pub bytes: u64,
    pub width: u32,
    pub height: u32,
    pub elapsed_ms: u64,
}

/// 按原始分辨率渲染并写出文件。未指定目标路径时弹出系统保存对话框。
#[tauri::command]
pub async fn export_image(
    state: State<'_, ImageCache>,
    path: String,
    params: EditParams,
    destination: Option<String>,
) -> AppResult<ExportResult> {
    let cache = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let started = Instant::now();
        let cached = cache.load(Path::new(&path))?;

        let target = match destination {
            Some(value) if !value.trim().is_empty() => std::path::PathBuf::from(value),
            _ => {
                let suggested = default_export_name(&cached.file_name);
                match rfd::FileDialog::new()
                    .set_title("导出照片")
                    .set_file_name(&suggested)
                    .add_filter("JPEG", &["jpg", "jpeg"])
                    .add_filter("PNG", &["png"])
                    .add_filter("WebP", &["webp"])
                    .save_file()
                {
                    Some(mut path) => {
                        if path.extension().is_none() {
                            path.set_extension("jpg");
                        }
                        path
                    }
                    None => return Err(AppError::Cancelled),
                }
            }
        };

        let rendered = adjustments::render(&cached.full, &params);
        let bytes = encode::write_to_path(&rendered, &target)?;

        Ok(ExportResult {
            path: target.to_string_lossy().into_owned(),
            bytes,
            width: rendered.width(),
            height: rendered.height(),
            elapsed_ms: started.elapsed().as_millis() as u64,
        })
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

fn default_export_name(file_name: &str) -> String {
    let stem = Path::new(file_name)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "lumi".to_string());
    format!("{stem}-lumi.jpg")
}

#[cfg(test)]
mod tests {
    use super::default_export_name;

    #[test]
    fn export_name_keeps_stem_and_switches_extension() {
        assert_eq!(default_export_name("portrait.CR2.png"), "portrait.CR2-lumi.jpg");
        assert_eq!(default_export_name("无扩展名"), "无扩展名-lumi.jpg");
    }
}
