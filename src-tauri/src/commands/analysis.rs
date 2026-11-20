use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::Serialize;
use tauri::State;

use crate::ai_engine::face_frame::FaceFrame;
use crate::error::AppResult;
use crate::image_engine::analysis::{
    self, AnalysisReport, RetouchMode, RetouchSuggestion, ANALYSIS_ALGORITHM_VERSION,
    ANALYSIS_RESULT_VERSION,
};
use crate::image_engine::params::EditParams;
use crate::state::{FaceRepository, ImageCache};

#[derive(Clone)]
struct CacheEntry {
    image_hash: String,
    face_signature: String,
    params_signature: String,
    mode: RetouchMode,
    report: AnalysisReport,
}

/// 分析缓存只存最近一次结果。原图、人脸、参数或算法版本变化都会失效，
/// 前端拿到 `stale = true` 时会提示重新分析。
#[derive(Default, Clone)]
pub struct AnalysisCache {
    inner: Arc<Mutex<HashMap<PathBuf, CacheEntry>>>,
}

impl AnalysisCache {
    fn get(&self, path: &Path) -> Option<CacheEntry> {
        self.inner.lock().ok()?.get(path).cloned()
    }

    fn put(&self, path: PathBuf, entry: CacheEntry) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.insert(path, entry);
        }
    }
}

fn canonical(path: &str) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path))
}

fn params_signature(params: &EditParams) -> String {
    serde_json::to_string(params).unwrap_or_default()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisResponse {
    pub report: AnalysisReport,
    /// 命中缓存（相同原图、人脸、参数与模式）。
    pub cached: bool,
    /// 之前存在分析结果但输入或算法版本已变化，本次是重新计算。
    pub stale: bool,
    pub elapsed_ms: u64,
}

#[tauri::command]
pub async fn analyze_retouch(
    state: State<'_, ImageCache>,
    repo: State<'_, FaceRepository>,
    cache: State<'_, AnalysisCache>,
    path: String,
    params: EditParams,
    mode: Option<RetouchMode>,
) -> AppResult<AnalysisResponse> {
    let cache_handle = state.inner().clone();
    let repo = repo.inner().clone();
    let analysis_cache = cache.inner().clone();
    let mode = mode.unwrap_or_default();
    let key = canonical(&path);

    tauri::async_runtime::spawn_blocking(move || {
        let started = Instant::now();
        let loaded = cache_handle.load(Path::new(&path))?;
        // 分析在预览尺寸上执行，最多 720px，避免每次拖动都扫描全图。
        let faces: Vec<FaceFrame> = repo
            .get(Path::new(&path))
            .map(|entry| entry.faces)
            .unwrap_or_default();
        let image_hash = analysis::image_hash(&loaded.preview);
        let face_signature = format!(
            "{}:{}",
            repo.signature(Path::new(&path)),
            faces
                .iter()
                .map(|face| face.stable_key())
                .collect::<Vec<_>>()
                .join(",")
        );
        let signature = params_signature(&params);

        let mut stale = false;
        if let Some(entry) = analysis_cache.get(&key) {
            stale = true;
            if entry.image_hash == image_hash
                && entry.face_signature == face_signature
                && entry.params_signature == signature
                && entry.mode == mode
                && entry.report.algorithm_version == ANALYSIS_ALGORITHM_VERSION
                && entry.report.result_version == ANALYSIS_RESULT_VERSION
            {
                return Ok(AnalysisResponse {
                    report: entry.report,
                    cached: true,
                    stale: false,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                });
            }
        }

        let report = analysis::analyze(&loaded.preview, &faces, &params, mode);
        analysis_cache.put(
            key,
            CacheEntry {
                image_hash,
                face_signature,
                params_signature: signature,
                mode,
                report: report.clone(),
            },
        );
        let elapsed = started.elapsed().as_millis() as u64;
        Ok(AnalysisResponse {
            report,
            cached: false,
            // 缓存里之前有旧结果说明输入或算法版本已变化，本次为重新计算。
            stale,
            elapsed_ms: elapsed,
        })
    })
    .await
    .map_err(|error| crate::error::AppError::Task(error.to_string()))?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedSuggestions {
    pub params: EditParams,
    pub warnings: Vec<String>,
}

/// 应用建议只逐字段合并，返回普通 `EditParams`，由前端写入历史后即可撤销。
#[tauri::command]
pub fn apply_retouch_suggestions(
    repo: State<'_, FaceRepository>,
    path: String,
    params: EditParams,
    suggestions: Vec<RetouchSuggestion>,
) -> AppliedSuggestions {
    let faces: Vec<FaceFrame> = repo
        .get(Path::new(&path))
        .map(|entry| entry.faces)
        .unwrap_or_default();
    let (next, warnings) = analysis::apply_suggestions(&params, &suggestions, &faces);
    AppliedSuggestions {
        params: next,
        warnings,
    }
}
