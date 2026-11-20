//! 第四轮塑形相关命令：能力查询、网格预览、人脸身份持久化与重开匹配。

use std::path::Path;

use serde::Serialize;
use tauri::State;

use crate::ai_engine::face_frame::FaceFrame;
use crate::error::{AppError, AppResult};
use crate::face_reshape::face_mesh;
use crate::face_reshape::identity::{self, FaceMatchOutcome, ProjectFace};
use crate::face_reshape::mesh::MeshPreview;
use crate::face_reshape::{capability, ReshapeCapability};
use crate::image_engine::params::EditParams;
use crate::state::{FaceRepository, ImageCache};

/// 当前真实可用的塑形能力（LEVEL 1 + LEVEL 2）。UI 据此禁用未验证功能。
#[tauri::command]
pub fn reshape_capability() -> ReshapeCapability {
    capability()
}

/// 返回真实控制网格，用于画布“网格显示”。没有任何人脸时返回 null。
#[tauri::command]
pub async fn reshape_mesh(
    state: State<'_, ImageCache>,
    repo: State<'_, FaceRepository>,
    path: String,
    face_id: u32,
    params: EditParams,
) -> AppResult<Option<MeshPreview>> {
    let cache = state.inner().clone();
    let repo = repo.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let cached = cache.load(Path::new(&path))?;
        let faces = repo.get(Path::new(&path)).map(|entry| entry.faces).unwrap_or_default();
        let Some(face) = faces.iter().find(|face| face.id == face_id) else {
            return Ok(None);
        };
        let reshape = params
            .face_reshape_match(&face.uuid, face.id)
            .cloned()
            .unwrap_or_else(|| crate::image_engine::params::FaceReshape::new(face.id));
        Ok(face_mesh::preview(face, cached.width, cached.height, &reshape))
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFaces {
    pub source_hash: String,
    pub faces: Vec<ProjectFace>,
}

/// 保存项目前读取当前检测结果对应的持久化人脸记录。
#[tauri::command]
pub async fn export_face_records(
    repo: State<'_, FaceRepository>,
    path: String,
) -> AppResult<ProjectFaces> {
    let repo = repo.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let source_hash = identity::source_hash(Path::new(&path)).unwrap_or_default();
        let faces = repo
            .get(Path::new(&path))
            .map(|entry| {
                entry
                    .faces
                    .iter()
                    .filter(|face| face.is_usable())
                    .map(|face| ProjectFace::from_frame(face, identity::MATCH_MODEL_VERSION))
                    .collect()
            })
            .unwrap_or_default();
        Ok(ProjectFaces { source_hash, faces })
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceRestoreResult {
    #[serde(flatten)]
    pub outcome: FaceMatchOutcome,
    /// 已写回持久 UUID 的检测结果；前端用它替换本地人脸列表。
    pub faces: Vec<FaceFrame>,
}

/// 项目重开：比较保存的哈希与当前文件，再按几何一一匹配。
/// 高置信自动恢复 UUID；低置信进入 pending；源图变化则全部转人工确认。
#[tauri::command]
pub async fn restore_faces(
    repo: State<'_, FaceRepository>,
    path: String,
    records: Vec<ProjectFace>,
    source_hash: Option<String>,
) -> AppResult<FaceRestoreResult> {
    let repo = repo.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let frames = repo.get(Path::new(&path)).map(|entry| entry.faces).unwrap_or_default();
        let current = identity::source_hash(Path::new(&path)).ok();
        let source_changed = match (current, source_hash) {
            (Some(current), Some(saved)) if !saved.is_empty() => current != saved,
            _ => false,
        };
        let outcome = identity::match_faces(&records, &frames, source_changed);
        let assignments: Vec<(u32, String)> = outcome
            .restored
            .iter()
            .map(|entry| (entry.face_id, entry.face_uuid.clone()))
            .collect();
        let faces = repo
            .apply_identities(Path::new(&path), &assignments)
            .map(|entry| entry.faces)
            .unwrap_or(frames);
        Ok(FaceRestoreResult { outcome, faces })
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceConfirmed {
    pub faces: Vec<FaceFrame>,
}

/// 人工确认低置信匹配：把指定 UUID 绑定到当前检测脸。
#[tauri::command]
pub async fn confirm_face_match(
    repo: State<'_, FaceRepository>,
    path: String,
    face_id: u32,
    face_uuid: String,
) -> AppResult<FaceConfirmed> {
    let repo = repo.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if face_uuid.trim().is_empty() {
            return Err(AppError::Unsupported("人脸 UUID 不能为空".into()));
        }
        let faces = repo
            .apply_identities(Path::new(&path), &[(face_id, face_uuid)])
            .map(|entry| entry.faces)
            .ok_or_else(|| AppError::Unsupported("当前照片还没有人脸检测结果".into()))?;
        Ok(FaceConfirmed { faces })
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}
