use std::path::Path;
use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::ai::face::FaceDetectionResult;
use crate::ai::model::MODEL_FILE;
use crate::ai::FaceEngine;
use crate::ai_engine::face_frame::{FaceFrame, FaceGeometry};
use crate::ai_engine::retouch::analyze::{analyze_face, FaceSuggestion};
use crate::ai_engine::skin_segmentation::model::{self, SkinModelStatus};
use crate::ai_engine::skin_segmentation::{build_skin_mask, visualization};
use crate::error::{AppError, AppResult};
use crate::image_engine::params::{EditParams, ManualMask};
use crate::image_engine::preview::encode_canvas;
use crate::state::{CachedFaces, FaceRepository, ImageCache};

fn model_path(app: &AppHandle) -> std::path::PathBuf {
    if let Ok(resources) = app.path().resource_dir() {
        for relative in ["resources/models/yunet", "models/yunet"] {
            let candidate = resources.join(relative).join(MODEL_FILE);
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("resources/models/yunet")
        .join(MODEL_FILE)
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct BeautyProgress {
    stage: String,
    progress: f32,
}

fn emit(app: &AppHandle, stage: &'static str, progress: f32) {
    let _ = app.emit(
        "lumi://beauty-progress",
        BeautyProgress {
            stage: stage.to_string(),
            progress,
        },
    );
}

/// 查询皮肤分割能力：当前没有可商用权重时明确返回几何方案，不冒充模型。
#[tauri::command]
pub fn skin_model_status(app: AppHandle) -> SkinModelStatus {
    let resource = app.path().resource_dir().ok();
    model::status(resource.as_deref())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BeautyAnalysis {
    pub faces: Vec<FaceSuggestion>,
    pub method: String,
    pub model: String,
    pub elapsed_ms: u64,
    pub skin_model: SkinModelStatus,
    pub warnings: Vec<String>,
}

/// 一键美肤：检测（若无缓存）→ 皮肤区域 → 指标分析 → 可编辑参数建议。
#[tauri::command]
pub async fn analyze_beauty(
    app: AppHandle,
    state: State<'_, ImageCache>,
    engine: State<'_, FaceEngine>,
    repo: State<'_, FaceRepository>,
    path: String,
) -> AppResult<BeautyAnalysis> {
    let cache = state.inner().clone();
    let engine = engine.inner().clone();
    let repo = repo.inner().clone();
    let model_file = model_path(&app);
    let resource = app.path().resource_dir().ok();
    let skin_model = model::status(resource.as_deref());

    tauri::async_runtime::spawn_blocking(move || {
        let started = Instant::now();
        let cached = cache.load(Path::new(&path))?;
        emit(&app, "人脸检测", 0.08);
        let cached_faces = ensure_faces(
            &app,
            &engine,
            &repo,
            &path,
            &cached.preview,
            cached.width,
            cached.height,
            &model_file,
        )?;

        let mut suggestions = Vec::new();
        let mut warnings = Vec::new();
        let total = cached_faces.faces.len().max(1);
        for (index, face) in cached_faces.faces.iter().enumerate() {
            emit(
                &app,
                "皮肤分析",
                0.35 + 0.55 * (index as f32 / total as f32),
            );
            let mask = match build_skin_mask(&cached.preview, face, &ManualMask::default()) {
                Ok(mask) => mask,
                Err(error) => {
                    warnings.push(format!("人脸 {}：{error}", face.id));
                    continue;
                }
            };
            let geometry = match FaceGeometry::from_face(face, cached.preview.width(), cached.preview.height()) {
                Ok(geometry) => geometry,
                Err(error) => {
                    warnings.push(format!("人脸 {}：{error}", face.id));
                    continue;
                }
            };
            suggestions.push(analyze_face(
                &cached.preview,
                &mask,
                &geometry,
                &crate::image_engine::cancel::CancelToken::default(),
            )?);
        }
        emit(&app, "完成", 1.0);

        Ok(BeautyAnalysis {
            faces: suggestions,
            method: skin_model.method.clone(),
            model: format!("YuNet 2023mar + {}", skin_model.method),
            elapsed_ms: started.elapsed().as_millis() as u64,
            skin_model,
            warnings,
        })
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaskPreview {
    pub url: String,
    pub width: u32,
    pub height: u32,
    pub skin_ratio: f32,
    pub faces: usize,
    pub method: String,
}

/// 生成“显示蒙版”叠加图。使用与渲染完全相同的蒙版构建函数。
#[tauri::command]
pub async fn preview_skin_mask(
    state: State<'_, ImageCache>,
    repo: State<'_, FaceRepository>,
    path: String,
    params: EditParams,
) -> AppResult<MaskPreview> {
    let cache = state.inner().clone();
    let repo = repo.inner().clone();

    tauri::async_runtime::spawn_blocking(move || {
        let cached = cache.load(Path::new(&path))?;
        let faces = repo.get(Path::new(&path)).ok_or_else(|| {
            AppError::Unsupported("尚未检测人脸，请先运行人脸检测或一键美肤".into())
        })?;
        let params = params.clamped();
        let mut masks = Vec::new();
        let mut total = 0.0f32;
        for face in faces.faces.iter() {
            if let Ok(mask) = build_skin_mask(&cached.preview, face, &params.manual_mask) {
                total += mask.alpha.len() as f32;
                masks.push(mask);
            }
        }
        let active: usize = masks.iter().map(|mask| mask.active_pixels()).sum();
        let skin_ratio = if total > 0.0 {
            active as f32 / total
        } else {
            0.0
        };
        let image = visualization(&cached.preview, &masks, params.manual_mask.invert);
        let encoded = encode_canvas(&image, cached.has_alpha)?;
        Ok(MaskPreview {
            url: encoded.url,
            width: encoded.width,
            height: encoded.height,
            skin_ratio,
            faces: masks.len(),
            method: model::status(None).method,
        })
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

fn ensure_faces(
    app: &AppHandle,
    engine: &FaceEngine,
    repo: &FaceRepository,
    path: &str,
    preview: &crate::image_engine::buffer::Image16,
    width: u32,
    height: u32,
    model: &Path,
) -> AppResult<CachedFaces> {
    if let Some(cached) = repo.get(Path::new(path)) {
        if !cached.faces.is_empty() {
            return Ok(cached);
        }
    }
    if !model.is_file() {
        return Err(AppError::Unsupported(
            "安装包中缺少人脸检测模型，无法执行美肤分析".into(),
        ));
    }
    let result: FaceDetectionResult =
        engine.detect(model, preview, width, height, &mut |stage, value| {
            emit(app, stage, 0.08 + value * 0.22);
        })?;
    let frames: Vec<FaceFrame> = result
        .faces
        .iter()
        .map(|face| FaceFrame::from_detected(face, width, height))
        .collect();
    Ok(repo.store(Path::new(path), frames))
}
