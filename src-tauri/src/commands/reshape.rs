//! 第四轮塑形相关命令：能力查询、网格预览、人脸身份持久化与重开匹配。

use std::path::Path;

use serde::Serialize;
use tauri::State;

use crate::ai_engine::face_frame::{id_from_uuid, FaceFrame};
use crate::error::{AppError, AppResult};
use crate::face_reshape::face_mesh;
use crate::face_reshape::identity::{self, FaceMatchOutcome, ProjectFace};
use crate::face_reshape::mesh::MeshPreview;
use crate::face_reshape::{capability, ReshapeCapability};
use crate::image_engine::params::EditParams;
use crate::state::{FaceRepository, ImageCache};

/// 匹配发生在 `apply_identities` 之前，当时用的是检测临时 ID。
/// 高置信人脸最终 ID 由项目 UUID 推导；低置信保持检测 ID，等待人工确认。
fn assign_final_face_ids(faces: &[FaceFrame], outcome: &mut FaceMatchOutcome) {
    let final_ids: std::collections::HashMap<&str, u32> =
        faces.iter().map(|face| (face.uuid.as_str(), face.id)).collect();
    for entry in outcome.restored.iter_mut() {
        entry.final_face_id = final_ids
            .get(entry.face_uuid.as_str())
            .copied()
            .unwrap_or_else(|| id_from_uuid(&entry.face_uuid));
    }
}

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
        let faces = repo
            .get(Path::new(&path))
            .map(|entry| entry.faces)
            .unwrap_or_default();
        let Some(face) = faces.iter().find(|face| face.id == face_id) else {
            return Ok(None);
        };
        let reshape = params
            .face_reshape_match(&face.uuid, face.id)
            .cloned()
            .unwrap_or_else(|| crate::image_engine::params::FaceReshape::new(face.id));
        Ok(face_mesh::preview(
            face,
            cached.width,
            cached.height,
            &reshape,
        ))
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
    /// 因 Provider/拓扑/模型版本/源图不一致而丢弃的旧稠密结论数量。
    pub discarded_dense: usize,
}

/// 项目重开：比较保存的哈希与当前文件，再按几何一一匹配。
/// 高置信自动恢复 UUID；低置信进入 pending；源图变化则全部转人工确认。
///
/// 稠密关键点与 LEVEL 3 门控只有在 Provider ID、Provider 版本、拓扑、模型版本
/// 与照片来源全部一致时才恢复；否则丢弃旧结论，回到 5 点并锁定 LEVEL 3。
#[tauri::command]
pub async fn restore_faces(
    repo: State<'_, FaceRepository>,
    intelligence: State<'_, std::sync::Arc<crate::face_intelligence::FaceIntelligence>>,
    path: String,
    records: Vec<ProjectFace>,
    source_hash: Option<String>,
) -> AppResult<FaceRestoreResult> {
    let repo = repo.inner().clone();
    let intelligence = intelligence.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let frames = repo
            .get(Path::new(&path))
            .map(|entry| entry.faces)
            .unwrap_or_default();
        let current = identity::source_hash(Path::new(&path)).ok();
        let source_changed = match (current, source_hash) {
            (Some(current), Some(saved)) if !saved.is_empty() => current != saved,
            _ => false,
        };
        let mut outcome = identity::match_faces(&records, &frames, source_changed);
        let assignments: Vec<(u32, String)> = outcome
            .restored
            .iter()
            .map(|entry| (entry.face_id, entry.face_uuid.clone()))
            .collect();
        let faces = repo
            .apply_identities(Path::new(&path), &assignments)
            .map(|entry| entry.faces)
            .unwrap_or(frames);

        let active_id = intelligence.active_id();
        let provider = intelligence.descriptor(&active_id).ok();
        let expected_model = intelligence
            .last_analysis()
            .filter(|record| record.provider_id == active_id)
            .and_then(|record| {
                record
                    .faces
                    .iter()
                    .find(|face| !face.gate.model_version.is_empty())
                    .map(|face| face.gate.model_version.clone())
            })
            .unwrap_or_default();
        let mut discarded = 0usize;
        // 恢复项目里保存的稠密关键点与门控；身份不一致或源图变化时一律丢弃。
        let analyses: Vec<_> = records
            .iter()
            .map(|record| {
                let restored = provider.as_ref().and_then(|provider| {
                    record.restorable_analysis(
                        &provider.id,
                        &provider.version,
                        provider.topology_id.as_deref().unwrap_or_default(),
                        &expected_model,
                        source_changed,
                    )
                });
                match restored {
                    Some((dense, gate)) => {
                        (record.uuid.clone(), Some(dense.clone()), Some(gate.clone()))
                    }
                    None => {
                        if record.dense.is_some() || record.level3.is_some() {
                            discarded += 1;
                        }
                        (record.uuid.clone(), None, None)
                    }
                }
            })
            .collect();
        let faces = repo
            .attach_analyses(Path::new(&path), &analyses)
            .map(|entry| entry.faces)
            .unwrap_or(faces);
        // 匹配发生在 apply_identities 之前，当时用的是检测临时 ID。
        // 高置信人脸最终 ID 由项目 UUID 推导；低置信保持检测 ID，等待人工确认。
        assign_final_face_ids(&faces, &mut outcome);
        Ok(FaceRestoreResult {
            outcome,
            faces,
            discarded_dense: discarded,
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_engine::face_frame::FaceFrame;
    use crate::face_reshape::identity::{FaceMatch, MatchConfidence};

    fn frame(id: u32, uuid: &str) -> FaceFrame {
        FaceFrame {
            id,
            uuid: uuid.into(),
            score: 0.95,
            x: 0.2,
            y: 0.2,
            width: 0.3,
            height: 0.3,
            landmarks: Vec::new(),
            dense: None,
            level3: None,
        }
    }

    fn matched(face_id: u32, uuid: &str) -> FaceMatch {
        FaceMatch {
            record_index: 0,
            face_id,
            final_face_id: face_id,
            face_uuid: uuid.into(),
            score: 0.98,
            confidence: MatchConfidence::High,
        }
    }

    #[test]
    fn final_face_id_uses_persisted_identity_not_detection_id() {
        // apply_identities 之后仓库里的人脸 id 已经由项目 UUID 推导。
        let faces = vec![frame(1700055149, "18e9-uuid")];
        let mut outcome = FaceMatchOutcome {
            restored: vec![matched(101, "18e9-uuid")],
            pending: Vec::new(),
            missing: Vec::new(),
            source_changed: false,
        };
        assign_final_face_ids(&faces, &mut outcome);
        assert_eq!(outcome.restored[0].face_id, 101);
        assert_eq!(outcome.restored[0].final_face_id, 1700055149);
    }

    #[test]
    fn final_face_id_falls_back_to_uuid_derived_id() {
        let outcome = FaceMatchOutcome {
            restored: vec![matched(101, "standalone-uuid")],
            pending: Vec::new(),
            missing: Vec::new(),
            source_changed: false,
        };
        let mut outcome = outcome;
        assign_final_face_ids(&[], &mut outcome);
        assert_eq!(outcome.restored[0].final_face_id, id_from_uuid("standalone-uuid"));
    }

    #[test]
    fn final_face_id_keeps_pending_detection_id() {
        let faces = vec![frame(1700055149, "18e9-uuid")];
        let mut outcome = FaceMatchOutcome {
            restored: Vec::new(),
            pending: vec![matched(303, "18e9-uuid")],
            missing: Vec::new(),
            source_changed: false,
        };
        assign_final_face_ids(&faces, &mut outcome);
        assert_eq!(outcome.pending[0].final_face_id, 303);
    }
}
