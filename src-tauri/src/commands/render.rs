use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::error::{AppError, AppResult};
use crate::ai_engine::face_frame::FaceFrame;
use crate::image_engine::buffer::fit_within;
use crate::image_engine::cancel::CancelToken;
use crate::image_engine::export::{self, ExportFormat, ExportSettings};
use crate::image_engine::params::EditParams;
use crate::image_engine::preview::{encode_canvas, DETAIL_MAX_EDGE};
use crate::image_engine::pipeline;
use crate::state::{FaceRepository, ImageCache};

/// 渲染任务注册表：只保留最新请求，旧请求在阶段边界主动退出。
#[derive(Default)]
pub struct RenderRegistry {
    sequence: AtomicU64,
    tasks: Mutex<HashMap<u64, Arc<AtomicBool>>>,
}

impl RenderRegistry {
    pub fn begin(&self, request_id: u64) -> CancelToken {
        let flag = Arc::new(AtomicBool::new(false));
        {
            let mut guard = self.tasks.lock().expect("渲染任务表锁失败");
            // 旧任务一律取消，避免旧结果覆盖新结果，也避免 CPU 堆积。
            for (id, previous) in guard.iter() {
                if *id != request_id {
                    previous.store(true, Ordering::Relaxed);
                }
            }
            guard.insert(request_id, flag.clone());
            if guard.len() > 16 {
                let current = request_id;
                guard.retain(|id, _| *id == current || *id + 8 > current);
            }
        }
        self.sequence.fetch_add(1, Ordering::Relaxed);
        CancelToken::new(flag)
    }

    pub fn cancel(&self, request_id: u64) {
        if let Ok(guard) = self.tasks.lock() {
            if let Some(flag) = guard.get(&request_id) {
                flag.store(true, Ordering::Relaxed);
            }
        }
    }

    pub fn cancel_all(&self) {
        if let Ok(guard) = self.tasks.lock() {
            for flag in guard.values() {
                flag.store(true, Ordering::Relaxed);
            }
        }
    }
}

/// 相同原图 + 相同参数 + 相同尺寸直接复用结果（撤销/重做、对比切换很常见）。
#[derive(Clone, Default)]
pub struct RenderCache {
    inner: Arc<Mutex<RenderCacheInner>>,
}

#[derive(Default)]
struct RenderCacheInner {
    entries: VecDeque<(String, Arc<RenderResult>)>,
    bytes: usize,
}

const RENDER_CACHE_BUDGET: usize = 160 * 1024 * 1024;

impl RenderCache {
    fn get(&self, key: &str) -> Option<Arc<RenderResult>> {
        let mut guard = self.inner.lock().ok()?;
        let index = guard.entries.iter().position(|(stored, _)| stored == key)?;
        let entry = guard.entries.remove(index)?;
        let value = entry.1.clone();
        guard.entries.push_back(entry);
        Some(value)
    }

    fn put(&self, key: String, value: Arc<RenderResult>) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.bytes += value.preview_url.len();
            guard.entries.push_back((key, value));
            while guard.bytes > RENDER_CACHE_BUDGET && guard.entries.len() > 1 {
                if let Some((_, removed)) = guard.entries.pop_front() {
                    guard.bytes = guard.bytes.saturating_sub(removed.preview_url.len());
                }
            }
            while guard.entries.len() > 12 {
                if let Some((_, removed)) = guard.entries.pop_front() {
                    guard.bytes = guard.bytes.saturating_sub(removed.preview_url.len());
                }
            }
        }
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RenderResult {
    pub request_id: u64,
    pub preview_url: String,
    pub width: u32,
    pub height: u32,
    pub elapsed_ms: u64,
    /// false 表示命中后端缓存，前端可跳过进度动画。
    pub computed: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ProgressPayload {
    request_id: u64,
    stage: String,
    progress: f32,
}

fn emit_progress(app: &AppHandle, request_id: u64, stage: &'static str, progress: f32) {
    let _ = app.emit(
        "lumi://render-progress",
        ProgressPayload {
            request_id,
            stage: stage.to_string(),
            progress,
        },
    );
}

fn cache_key(path: &Path, params: &EditParams, max_edge: u32, faces: &str) -> String {
    let serialized = serde_json::to_string(params).unwrap_or_default();
    format!("{}|{}|{}|{}", path.to_string_lossy(), max_edge, faces, serialized)
}

/// 渲染画布预览。`maxEdge` 用于 100% / 200% 高清查看，
/// 未传则使用缓存中的 1800px 预览，保证滑杆实时性。
#[tauri::command]
pub async fn render_preview(
    app: AppHandle,
    state: State<'_, ImageCache>,
    repo: State<'_, FaceRepository>,
    render_cache: State<'_, RenderCache>,
    registry: State<'_, RenderRegistry>,
    request_id: u64,
    path: String,
    params: EditParams,
    max_edge: Option<u32>,
) -> AppResult<RenderResult> {
    let cache = state.inner().clone();
    let repo = repo.inner().clone();
    let render_cache = render_cache.inner().clone();
    let cancel = registry.begin(request_id);
    let max_edge = max_edge.unwrap_or(crate::image_engine::PREVIEW_MAX_EDGE);
    let max_edge = max_edge.clamp(320, DETAIL_MAX_EDGE);
    let signature = repo.signature(Path::new(&path));
    let key = cache_key(Path::new(&path), &params, max_edge, &signature);

    if let Some(hit) = render_cache.get(&key) {
        return Ok(RenderResult {
            request_id,
            computed: false,
            ..(*hit).clone()
        });
    }

    tauri::async_runtime::spawn_blocking(move || {
        let started = Instant::now();
        let cached = cache.load(Path::new(&path))?;
        cancel.check()?;
        let faces: Vec<FaceFrame> = repo
            .get(Path::new(&path))
            .map(|entry| entry.faces)
            .unwrap_or_default();

        let source = if max_edge > crate::image_engine::PREVIEW_MAX_EDGE {
            &cached.full
        } else {
            &cached.preview
        };
        let working = fit_within(source, max_edge);
        emit_progress(&app, request_id, "开始", 0.02);
        let rendered = pipeline::render_with_progress(&working, &params, &faces, &cancel, &mut |value, stage| {
            emit_progress(&app, request_id, stage, value);
        })?;
        let preview = encode_canvas(&rendered, cached.has_alpha)?;
        cancel.check()?;

        let result = Arc::new(RenderResult {
            request_id,
            preview_url: preview.url,
            width: preview.width,
            height: preview.height,
            elapsed_ms: started.elapsed().as_millis() as u64,
            computed: true,
        });
        render_cache.put(key, result.clone());
        Ok((*result).clone())
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

#[tauri::command]
pub fn cancel_render(registry: State<'_, RenderRegistry>, request_id: u64) {
    registry.cancel(request_id);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub path: String,
    pub bytes: u64,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub elapsed_ms: u64,
}

#[tauri::command]
pub async fn export_image(
    app: AppHandle,
    state: State<'_, ImageCache>,
    repo: State<'_, FaceRepository>,
    registry: State<'_, RenderRegistry>,
    request_id: u64,
    path: String,
    params: EditParams,
    settings: Option<ExportSettings>,
    destination: Option<String>,
) -> AppResult<ExportResult> {
    let cache = state.inner().clone();
    let repo = repo.inner().clone();
    let cancel = registry.begin(request_id);
    let settings = settings.unwrap_or_default().clamped();

    tauri::async_runtime::spawn_blocking(move || {
        let started = Instant::now();
        let cached = cache.load(Path::new(&path))?;
        cancel.check()?;
        // 导出复用同一份归一化人脸结果，AI 检测不重跑；蒙版按全分辨率重建。
        let faces: Vec<FaceFrame> = repo
            .get(Path::new(&path))
            .map(|entry| entry.faces)
            .unwrap_or_default();

        let target = match destination {
            Some(value) if !value.trim().is_empty() => PathBuf::from(value),
            _ => {
                let suggested = export::suggested_name(&cached.path, settings.format);
                let dialog = rfd::FileDialog::new()
                    .set_title("导出照片")
                    .set_file_name(&suggested)
                    .add_filter("JPEG", &["jpg", "jpeg"])
                    .add_filter("PNG", &["png"])
                    .add_filter("WebP", &["webp"]);
                match dialog.save_file() {
                    Some(mut path) => {
                        if ExportFormat::from_extension(&path).is_none() {
                            path.set_extension(settings.format.extension());
                        }
                        path
                    }
                    None => return Err(AppError::Cancelled),
                }
            }
        };

        emit_progress(&app, request_id, "全尺寸渲染", 0.05);
        let rendered = pipeline::render_with_progress(&cached.full, &params, &faces, &cancel, &mut |value, stage| {
            emit_progress(&app, request_id, stage, 0.05 + value * 0.85);
        })?;
        cancel.check()?;
        emit_progress(&app, request_id, "写出文件", 0.96);
        let outcome = export::write(&target, &cached.path, &rendered, settings, cached.depth)?;
        emit_progress(&app, request_id, "完成", 1.0);

        Ok(ExportResult {
            path: outcome.path.to_string_lossy().into_owned(),
            bytes: outcome.bytes,
            width: rendered.width(),
            height: rendered.height(),
            format: outcome.format,
            elapsed_ms: started.elapsed().as_millis() as u64,
        })
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(id: u64, url: &str) -> Arc<RenderResult> {
        Arc::new(RenderResult {
            request_id: id,
            preview_url: url.to_string(),
            width: 100,
            height: 100,
            elapsed_ms: 1,
            computed: true,
        })
    }

    #[test]
    fn starting_new_request_cancels_previous() {
        let registry = RenderRegistry::default();
        let first = registry.begin(1);
        let second = registry.begin(2);
        assert!(first.is_cancelled(), "旧请求必须在阶段边界退出");
        assert!(!second.is_cancelled());

        registry.cancel(2);
        assert!(second.is_cancelled());
    }

    #[test]
    fn render_cache_hits_and_reuses_result() {
        let cache = RenderCache::default();
        assert!(cache.get("a|1").is_none());
        cache.put("a|1".into(), result(1, "data:image/jpeg;base64,AAA"));
        let hit = cache.get("a|1").unwrap();
        assert_eq!(hit.preview_url, "data:image/jpeg;base64,AAA");
        assert!(cache.get("a|2").is_none());
    }

    #[test]
    fn render_cache_respects_entry_limit() {
        let cache = RenderCache::default();
        for index in 0..20 {
            cache.put(format!("k{index}"), result(index as u64, "x"));
        }
        assert!(cache.get("k0").is_none(), "最旧条目应被淘汰");
        assert!(cache.get("k19").is_some());
    }
}
