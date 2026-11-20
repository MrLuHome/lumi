use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::ai::face::{FaceDetectionResult, FaceModelStatus};
use crate::ai::model::MODEL_FILE;
use crate::ai::FaceEngine;
use crate::ai_engine::face_frame::FaceFrame;
use crate::error::{AppError, AppResult};
use crate::state::{FaceRepository, ImageCache};

/// 模型随安装包分发；开发时回退到仓库内的 resources 目录。
fn model_path(app: &AppHandle) -> PathBuf {
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
struct FaceProgressPayload {
    stage: String,
    progress: f32,
}

fn emit_progress(app: &AppHandle, stage: &'static str, progress: f32) {
    let _ = app.emit(
        "lumi://face-progress",
        FaceProgressPayload {
            stage: stage.to_string(),
            progress,
        },
    );
}

/// 查询本地模型可用性与已登记的许可证信息。
#[tauri::command]
pub fn face_model_status(app: AppHandle, engine: State<'_, FaceEngine>) -> FaceModelStatus {
    engine.status(Some(&model_path(&app)))
}

/// 对当前照片做真实 CPU 推理，返回原图像素坐标的人脸框与关键点。
#[tauri::command]
pub async fn detect_faces(
    app: AppHandle,
    state: State<'_, ImageCache>,
    repo: State<'_, FaceRepository>,
    engine: State<'_, FaceEngine>,
    path: String,
) -> AppResult<FaceDetectionResult> {
    let cache = state.inner().clone();
    let repo = repo.inner().clone();
    let engine = engine.inner().clone();
    let model = model_path(&app);

    tauri::async_runtime::spawn_blocking(move || {
        let cached = cache.load(Path::new(&path))?;
        emit_progress(&app, "加载模型", 0.05);
        if !model.is_file() {
            return Err(AppError::Unsupported(
                "安装包中缺少人脸检测模型，请重新安装或检查 resources 目录".into(),
            ));
        }
        let mut result = engine.detect(
            &model,
            &cached.preview,
            cached.width,
            cached.height,
            &mut |stage, value| {
                emit_progress(&app, stage, value);
            },
        )?;

        // 归一化缓存 + 稳定 ID：按 IoU 匹配上一轮结果，检测顺序变化也不会错配。
        let frames: Vec<FaceFrame> = result
            .faces
            .iter()
            .map(|face| FaceFrame::from_detected(face, cached.width, cached.height))
            .collect();
        let stored = repo.store(Path::new(&path), frames);
        for face in result.faces.iter_mut() {
            let frame = FaceFrame::from_detected(face, cached.width, cached.height);
            if let Some(best) = stored
                .faces
                .iter()
                .max_by(|a, b| {
                    frame
                        .iou(a)
                        .partial_cmp(&frame.iou(b))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
            {
                if frame.iou(best) > 0.2 {
                    face.id = best.id as usize;
                    face.uuid = best.uuid.clone();
                }
            }
        }
        result.faces.sort_by_key(|face| face.id);
        Ok(result)
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}
