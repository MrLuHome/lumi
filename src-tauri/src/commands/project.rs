use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::State;

use crate::ai_engine::face_frame::FaceFrame;
use crate::error::{AppError, AppResult};
use crate::face_reshape::identity::{self, ProjectFace};
use crate::image_engine::operations::estimate_white_balance;
use crate::image_engine::project::{self, ProjectDoc, PROJECT_EXTENSION};
use crate::state::{FaceRepository, ImageCache};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSaved {
    pub path: String,
    pub bytes: u64,
    /// 项目身份与保存代数：前端保存后必须回写，供下一次并发检查使用。
    pub project_uuid: String,
    pub revision: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectOpened {
    pub path: String,
    pub project: ProjectDoc,
}

/// 合并「本次会话检测到的人脸」与「项目里已保存的人脸记录」。
///
/// 保存不是重置：本次因为 Provider 切换/未重新分析而缺失的稠密结论，以及
/// 本次没有检测到的人脸记录，都必须原样保留。否则“打开项目→保存”就会
/// 永久删掉用户已有的分析数据。缺失的稠密结论是否可用于编辑，仍由加载期的
/// Provider 一致性检查决定，这里只负责不丢数据。
fn merge_saved_faces(previous: &[ProjectFace], live: &[FaceFrame]) -> Vec<ProjectFace> {
    let by_uuid: std::collections::HashMap<&str, &ProjectFace> = previous
        .iter()
        .map(|record| (record.uuid.as_str(), record))
        .collect();
    let mut merged: Vec<ProjectFace> = Vec::new();
    let mut live_uuids: std::collections::HashSet<String> = std::collections::HashSet::new();

    for face in live.iter().filter(|face| face.is_usable()) {
        let mut record = ProjectFace::from_frame(face, identity::MATCH_MODEL_VERSION);
        record.ensure_uuid();
        if let Some(old) = by_uuid.get(record.uuid.as_str()) {
            // 未知字段、蒙版引用属于“本次没有编辑”的数据，必须原样保留。
            record.extra = old.extra.clone();
            if record.mask_ref.is_none() {
                record.mask_ref = old.mask_ref.clone();
            }
            if record.dense.is_none() {
                // 稠密结论与门控是一对：只补 dense 不补 level3 会造出
                // “旧 Provider 的稠密点 + 新 Provider 的锁定结论”这种自相矛盾的记录。
                record.dense = old.dense.clone();
                record.level3 = old.level3.clone();
            } else if record.level3.is_none() {
                record.level3 = old.level3.clone();
            }
        }
        live_uuids.insert(record.uuid.clone());
        merged.push(record);
    }
    for record in previous {
        if record.uuid.is_empty() || !live_uuids.contains(&record.uuid) {
            // 旧项目可能有人脸记录没有 UUID。若同一保存流程重复运行，
            // 不能每次保存都追加一条相同记录（否则文件会随保存次数膨胀）。
            if record.uuid.is_empty()
                && merged.iter().any(|entry| structurally_equal(entry, record))
            {
                continue;
            }
            merged.push(record.clone());
        }
    }
    merged
}

/// 无 UUID 的旧记录只能按内容判重：框、关键点、分数与模型版本完全一致
/// 才视为同一条记录。
fn structurally_equal(a: &ProjectFace, b: &ProjectFace) -> bool {
    a.uuid.is_empty()
        && b.uuid.is_empty()
        && a.bbox == b.bbox
        && a.landmarks == b.landmarks
        && a.score.to_bits() == b.score.to_bits()
        && a.model == b.model
        && a.dense == b.dense
        && a.level3 == b.level3
}

/// 保存非破坏性项目：只写参数、蒙版、导出设置，不写预览图。
/// 同时写入源图哈希与当前检测出的人脸身份记录，供重开时匹配。
#[tauri::command]
pub async fn save_project(
    repo: State<'_, FaceRepository>,
    mut project: ProjectDoc,
    destination: Option<String>,
) -> AppResult<ProjectSaved> {
    let repo = repo.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        for photo in project.photos.iter_mut() {
            if let Some(entry) = repo.get(Path::new(&photo.path)) {
                if !entry.faces.is_empty() {
                    photo.faces = merge_saved_faces(&photo.faces, &entry.faces);
                }
            }
            if photo.source_hash.is_empty() {
                photo.source_hash =
                    identity::source_hash(Path::new(&photo.path)).unwrap_or_default();
            }
        }
        let target = match destination {
            Some(value) if !value.trim().is_empty() => PathBuf::from(value),
            _ => {
                let suggested = format!(
                    "lumi-project-{}.{}",
                    project.active_index + 1,
                    PROJECT_EXTENSION
                );
                match rfd::FileDialog::new()
                    .set_title("保存 LUMI 项目")
                    .set_file_name(&suggested)
                    .add_filter("LUMI 项目", &[PROJECT_EXTENSION])
                    .add_filter("JSON", &["json"])
                    .save_file()
                {
                    Some(mut path) => {
                        if path.extension().is_none() {
                            path.set_extension(PROJECT_EXTENSION);
                        }
                        path
                    }
                    None => return Err(AppError::Cancelled),
                }
            }
        };
        let outcome = project::save_with_meta(&target, &project)?;
        let bytes = std::fs::metadata(&target)
            .map(|meta| meta.len())
            .unwrap_or(0);
        Ok(ProjectSaved {
            path: target.to_string_lossy().into_owned(),
            bytes,
            project_uuid: outcome.project_uuid,
            revision: outcome.revision,
        })
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

/// 选择并读取项目文件。返回参数与路径，图片由前端调用 load_images 重新载入。
#[tauri::command]
pub async fn open_project(destination: Option<String>) -> AppResult<ProjectOpened> {
    tauri::async_runtime::spawn_blocking(move || {
        let target = match destination {
            Some(value) if !value.trim().is_empty() => PathBuf::from(value),
            _ => match rfd::FileDialog::new()
                .set_title("打开 LUMI 项目")
                .add_filter("LUMI 项目", &[PROJECT_EXTENSION, "json"])
                .pick_file()
            {
                Some(path) => path,
                None => return Err(AppError::Cancelled),
            },
        };
        let project = project::load(&target)?;
        Ok(ProjectOpened {
            path: target.to_string_lossy().into_owned(),
            project,
        })
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LutPicked {
    pub path: String,
    pub name: String,
}

/// 选择一个 .cube 3D LUT 文件；路径会写进项目参数。
#[tauri::command]
pub async fn pick_lut() -> AppResult<Option<LutPicked>> {
    tauri::async_runtime::spawn_blocking(move || {
        match rfd::FileDialog::new()
            .set_title("选择 3D LUT")
            .add_filter("3D LUT", &["cube"])
            .pick_file()
        {
            Some(path) => {
                let name = path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "LUT".into());
                Ok(Some(LutPicked {
                    path: path.to_string_lossy().into_owned(),
                    name,
                }))
            }
            None => Ok(None),
        }
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhiteBalanceEstimate {
    pub temperature: f32,
    pub tint: f32,
}

/// 灰世界自动白平衡：基于原图像素统计，不改变原图，只返回建议偏移。
#[tauri::command]
pub async fn auto_white_balance(
    state: State<'_, ImageCache>,
    path: String,
) -> AppResult<WhiteBalanceEstimate> {
    let cache = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let cached = cache.load(Path::new(&path))?;
        let (temperature, tint) = estimate_white_balance(&cached.preview);
        Ok(WhiteBalanceEstimate { temperature, tint })
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_engine::face_frame::NormPoint;
    use crate::face_intelligence::quality::Level3Gate;
    use crate::face_intelligence::types::{BoundingBox, DenseFace};

    fn dense(provider: &str) -> DenseFace {
        DenseFace {
            topology_id: "face-68".into(),
            provider_id: provider.into(),
            provider_version: "rev1".into(),
            image_width: 100,
            image_height: 100,
            bounding_box: BoundingBox {
                x: 0.1,
                y: 0.1,
                width: 0.3,
                height: 0.3,
            },
            confidence: 0.9,
            regions: Vec::new(),
        }
    }

    fn frame(uuid: &str, dense: Option<DenseFace>) -> FaceFrame {
        FaceFrame {
            id: 1,
            uuid: uuid.into(),
            score: 0.9,
            x: 0.1,
            y: 0.1,
            width: 0.3,
            height: 0.3,
            landmarks: vec![
                NormPoint { x: 0.2, y: 0.2 },
                NormPoint { x: 0.3, y: 0.2 },
                NormPoint { x: 0.25, y: 0.3 },
                NormPoint { x: 0.2, y: 0.35 },
                NormPoint { x: 0.3, y: 0.35 },
            ],
            dense,
            level3: None,
        }
    }

    #[test]
    fn save_keeps_previous_dense_when_session_has_none() {
        let previous = ProjectFace::from_frame(&frame("uuid-a", Some(dense("apple-vision"))), "yunet-5");
        // 会话里重新检测但没重新分析：dense 为 None。
        let merged = merge_saved_faces(&[previous], &[frame("uuid-a", None)]);
        assert_eq!(merged.len(), 1);
        let kept = merged[0].dense.as_ref().expect("旧稠密关键点必须保留");
        assert_eq!(kept.provider_id, "apple-vision");
    }

    #[test]
    fn save_prefers_fresh_dense_from_current_session() {
        let previous = ProjectFace::from_frame(&frame("uuid-a", Some(dense("apple-vision"))), "yunet-5");
        let merged = merge_saved_faces(&[previous], &[frame("uuid-a", Some(dense("yunet-68")))]);
        assert_eq!(
            merged[0].dense.as_ref().unwrap().provider_id,
            "yunet-68"
        );
    }

    #[test]
    fn save_keeps_records_missing_from_current_detection() {
        let a = ProjectFace::from_frame(&frame("uuid-a", Some(dense("apple-vision"))), "yunet-5");
        let b = ProjectFace::from_frame(&frame("uuid-b", None), "yunet-5");
        let merged = merge_saved_faces(&[a, b], &[frame("uuid-a", None)]);
        assert_eq!(merged.len(), 2);
        assert!(merged.iter().any(|record| record.uuid == "uuid-b"));
    }

    #[test]
    fn save_keeps_unlocked_gate_with_kept_dense() {
        let mut previous =
            ProjectFace::from_frame(&frame("uuid-a", Some(dense("apple-vision"))), "yunet-5");
        let mut gate = Level3Gate::locked("apple-vision", "uuid-a", "test");
        gate.dense_landmarks = true;
        previous.level3 = Some(gate);
        // 本次会话重新检测但没有分析：level3 是“已锁定”的新结论。
        let mut live = frame("uuid-a", None);
        live.level3 = Some(Level3Gate::locked("yunet", "uuid-a", "re-detected"));
        let merged = merge_saved_faces(&[previous], &[live]);
        let kept = merged[0].level3.as_ref().expect("门控必须保留");
        assert!(kept.dense_landmarks, "保留旧 dense 时必须同时保留旧门控");
        assert_eq!(kept.provider_id, "apple-vision");
    }

    #[test]
    fn save_without_previous_records_writes_live_faces() {
        let merged = merge_saved_faces(&[], &[frame("uuid-new", None)]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].uuid, "uuid-new");
    }

    #[test]
    fn save_preserves_mask_ref_unknown_fields_and_topology() {
        let mut previous =
            ProjectFace::from_frame(&frame("uuid-a", Some(dense("apple-vision"))), "yunet-5");
        previous.mask_ref = Some("mask-1".into());
        previous
            .extra
            .insert("futureFaceField".into(), serde_json::json!({ "note": "preserve" }));
        let mut live = frame("uuid-a", None);
        live.landmarks = previous.landmarks.clone();
        let merged = merge_saved_faces(&[previous], &[live]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].mask_ref.as_deref(), Some("mask-1"));
        assert_eq!(
            merged[0].extra.get("futureFaceField"),
            Some(&serde_json::json!({ "note": "preserve" }))
        );
        assert_eq!(
            merged[0].dense.as_ref().unwrap().topology_id,
            "face-68"
        );
        assert_eq!(merged[0].landmarks.len(), 5, "五点拓扑不得变化");
    }

    #[test]
    fn save_does_not_duplicate_records_without_uuid() {
        let mut legacy = ProjectFace::from_frame(&frame("", None), "yunet-5");
        legacy.uuid.clear();
        let merged = merge_saved_faces(std::slice::from_ref(&legacy), &[]);
        assert_eq!(merged.len(), 1);
        // 再保存一次（同一份无 UUID 记录）不得追加第二条。
        let merged_again = merge_saved_faces(&merged, &[]);
        assert_eq!(merged_again.len(), 1);
    }

    #[test]
    fn save_keeps_multiple_faces_with_distinct_uuids() {
        let a = ProjectFace::from_frame(&frame("uuid-a", Some(dense("apple-vision"))), "yunet-5");
        let b = ProjectFace::from_frame(&frame("uuid-b", Some(dense("apple-vision"))), "yunet-5");
        let merged = merge_saved_faces(&[a, b], &[frame("uuid-a", None), frame("uuid-b", None)]);
        let mut uuids: Vec<&str> = merged.iter().map(|record| record.uuid.as_str()).collect();
        uuids.sort();
        assert_eq!(uuids, vec!["uuid-a", "uuid-b"]);
        assert!(merged.iter().all(|record| record.dense.is_some()));
    }
}

