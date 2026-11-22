//! ROUND 10：类型化 ChangeSet 保存协议。
//!
//! 前端不再整体覆盖项目文件，而是提交一组显式操作：
//! - 每条操作只修改明确声明的子树/字段；
//! - 未出现的字段保持磁盘原值；
//! - `null` 表示显式置空（仅对 `Option` 字段合法），缺失表示“不修改”；
//! - 删除必须使用显式删除操作；
//! - 未知字段（磁盘上已存在但本版本不认识）原样保留；
//! - 每个操作校验项目 ID 与目标实体标识；
//! - 整个 ChangeSet 作为一次原子写提交，失败不落盘；
//! - 成功后才递增 revision，响应返回最新 revision；
//! - `request_id` 重复提交不会重复执行破坏性操作。

use std::collections::BTreeSet;
use std::path::Path;

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::AppError;
use crate::image_engine::params::{BackgroundParams, EditParams, ManualMask, SubjectMaskParams};
use crate::image_engine::project::{self, ProjectDoc, PROJECT_VERSION};

/// 每个项目最多记住的已执行请求数，用于请求级幂等。
const APPLIED_REQUEST_LIMIT: usize = 64;

/// 变更请求头。`request_id` 用于幂等，`base_revision` 用于冲突检测。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeSet {
    /// 目标项目身份。必须与磁盘上的 `projectUuid` 一致。
    pub project_id: String,
    /// 前端读到的保存代数。磁盘更新时必须拒绝。
    pub base_revision: u64,
    /// 调用方生成的唯一请求 ID（非空）。
    pub request_id: String,
    /// 按顺序执行的操作列表。
    pub operations: Vec<Operation>,
    #[serde(default)]
    pub created_at: String,
    /// 冲突时允许在最新磁盘状态上重新套用这些**增量操作**。
    /// 绝不是用旧快照覆盖：未声明的字段仍然保持磁盘最新值。
    #[serde(default)]
    pub rebase_on_conflict: bool,
}

/// 显式操作。顺序即执行顺序，任一步失败整包回滚。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Operation {
    /// 项目级字段：activeIndex / doc export / compute。
    #[serde(rename_all = "camelCase")]
    UpdateDocMeta { patch: Value },
    #[serde(rename_all = "camelCase")]
    UpdatePhotoParams { photo: String, patch: Value },
    #[serde(rename_all = "camelCase")]
    UpdateFaceParams {
        photo: String,
        face_uuid: String,
        patch: Value,
    },
    #[serde(rename_all = "camelCase")]
    UpdateReshapeParams {
        photo: String,
        face_uuid: String,
        patch: Value,
    },
    #[serde(rename_all = "camelCase")]
    UpdateSubjectMask { photo: String, patch: Value },
    #[serde(rename_all = "camelCase")]
    UpdateManualMask { photo: String, patch: Value },
    #[serde(rename_all = "camelCase")]
    UpdateBackground { photo: String, patch: Value },
    #[serde(rename_all = "camelCase")]
    UpdateRawParams { photo: String, patch: Value },
    #[serde(rename_all = "camelCase")]
    UpdateColorParams { photo: String, patch: Value },
    /// `photo` 为空时修改项目级导出设置，否则修改该照片的导出设置。
    #[serde(rename_all = "camelCase")]
    UpdateExportParams {
        #[serde(default)]
        photo: Option<String>,
        patch: Value,
    },
    /// 新增一张人脸参数（`params.faces[]`）。UUID 必须唯一。
    #[serde(rename_all = "camelCase")]
    AddFace { photo: String, face: Value },
    /// 删除一张人脸参数。必须显式调用，不能用空数组隐式删除。
    #[serde(rename_all = "camelCase")]
    RemoveFace { photo: String, face_uuid: String },
    /// 新增蒙版层（`masks[]`）。
    #[serde(rename_all = "camelCase")]
    AddMask { photo: String, mask: Value },
    #[serde(rename_all = "camelCase")]
    UpdateMask {
        photo: String,
        mask_id: String,
        patch: Value,
    },
    /// 删除蒙版层。必须显式调用。
    #[serde(rename_all = "camelCase")]
    RemoveMask { photo: String, mask_id: String },
    /// 新增或替换持久化人脸记录（`faces[]`，按 UUID 匹配）。
    #[serde(rename_all = "camelCase")]
    UpsertFaceRecord { photo: String, record: Value },
    #[serde(rename_all = "camelCase")]
    RemoveFaceRecord { photo: String, face_uuid: String },
    /// 新导入的照片。已存在的路径会被拒绝（避免重复图元）。
    #[serde(rename_all = "camelCase")]
    AddPhoto { photo: Value },
    /// 来源信息（sourceType/path/hash/metadata）。`metadata: null` 表示清空快照。
    #[serde(rename_all = "camelCase")]
    UpdateSource { photo: String, patch: Value },
    /// 批量参数：把同一份参数补丁应用到多张照片。
    #[serde(rename_all = "camelCase")]
    UpdateBatchParams { photos: Vec<String>, patch: Value },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeSetOutcome {
    pub project_uuid: String,
    pub revision: u64,
    /// 该 `request_id` 之前已执行过，本次没有再次应用操作。
    pub duplicate: bool,
}

/// 可定位的失败类型。命令层序列化为 `{kind, message, currentRevision}`。
#[derive(Debug)]
pub enum ChangeSetError {
    /// 项目身份不匹配：拒绝写入，避免 A 项目覆盖 B 项目。
    ProjectMismatch { expected: String, actual: String },
    /// base_revision 与磁盘不一致。`current` 是磁盘最新代数。
    RevisionConflict { base: u64, current: u64 },
    /// 操作本身非法（未知字段、null 用于非空字段、重复 UUID 等）。
    InvalidOperation(String),
    /// 目标实体不存在。
    TargetNotFound(String),
    /// 底层 IO/序列化错误。
    Storage(String),
}

impl std::fmt::Display for ChangeSetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChangeSetError::ProjectMismatch { expected, actual } => write!(
                f,
                "目标文件属于另一个项目（磁盘 {actual}，请求 {expected}），已拒绝写入"
            ),
            ChangeSetError::RevisionConflict { base, current } => write!(
                f,
                "项目已在别处更新（磁盘 revision {current}，本次基于 {base}），请重新读取后再保存"
            ),
            ChangeSetError::InvalidOperation(message) => write!(f, "保存操作无效：{message}"),
            ChangeSetError::TargetNotFound(message) => write!(f, "保存目标不存在：{message}"),
            ChangeSetError::Storage(message) => write!(f, "项目存储失败：{message}"),
        }
    }
}

impl ChangeSetError {
    pub fn kind(&self) -> &'static str {
        match self {
            ChangeSetError::ProjectMismatch { .. } => "PROJECT_MISMATCH",
            ChangeSetError::RevisionConflict { .. } => "REVISION_CONFLICT",
            ChangeSetError::InvalidOperation(_) => "INVALID_OPERATION",
            ChangeSetError::TargetNotFound(_) => "TARGET_NOT_FOUND",
            ChangeSetError::Storage(_) => "STORAGE_ERROR",
        }
    }

    pub fn current_revision(&self) -> Option<u64> {
        match self {
            ChangeSetError::RevisionConflict { current, .. } => Some(*current),
            _ => None,
        }
    }
}

impl From<AppError> for ChangeSetError {
    fn from(error: AppError) -> Self {
        ChangeSetError::Storage(error.to_string())
    }
}

impl From<std::io::Error> for ChangeSetError {
    fn from(error: std::io::Error) -> Self {
        ChangeSetError::Storage(error.to_string())
    }
}

/// 应用并原子提交 ChangeSet。
pub fn apply(path: &Path, changeset: &ChangeSet) -> Result<ChangeSetOutcome, ChangeSetError> {
    if changeset.request_id.trim().is_empty() {
        return Err(ChangeSetError::InvalidOperation(
            "requestId 不能为空（幂等键缺失）".into(),
        ));
    }
    if !path.exists() {
        return Err(ChangeSetError::Storage(format!(
            "项目文件不存在：{}",
            path.display()
        )));
    }
    // 与整包保存共用同一把锁：两条路径不会交错写入同一文件。
    let _guard = project::lock_saves()?;

    let bytes = std::fs::read(path)?;
    let mut raw: Value = serde_json::from_slice(&bytes)
        .map_err(|error| ChangeSetError::Storage(format!("项目文件解析失败：{error}")))?;
    if !raw.is_object() {
        return Err(ChangeSetError::Storage("项目文件不是对象".into()));
    }

    // ---- 项目身份 ----
    let disk_uuid = raw
        .get("projectUuid")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if !disk_uuid.is_empty() && disk_uuid != changeset.project_id {
        return Err(ChangeSetError::ProjectMismatch {
            expected: changeset.project_id.clone(),
            actual: disk_uuid,
        });
    }
    let disk_revision = raw.get("revision").and_then(Value::as_u64).unwrap_or(0);

    // ---- 幂等：相同 request_id + 相同操作内容直接返回当前状态 ----
    // 必须在 revision 检查之前：保存已成功但响应丢失时，重试携带的是旧
    // base_revision，此时应当识别为重放，而不是误报冲突。
    // 同一个 request_id 携带不同操作时不算重放，避免掩盖客户端 bug。
    let token = request_token(changeset);
    if applied_requests(&raw).iter().any(|entry| entry == &token) {
        return Ok(ChangeSetOutcome {
            project_uuid: if disk_uuid.is_empty() {
                changeset.project_id.clone()
            } else {
                disk_uuid
            },
            revision: disk_revision,
            duplicate: true,
        });
    }
    if disk_revision != changeset.base_revision && !changeset.rebase_on_conflict {
        return Err(ChangeSetError::RevisionConflict {
            base: changeset.base_revision,
            current: disk_revision,
        });
    }
    // rebase 分支无需额外动作：`raw` 是加锁后重新读取的最新状态，
    // 增量操作会重新套用它，旧快照从未参与合并，因此不会覆盖其他窗口写入的字段。

    // ---- 按顺序应用操作 ----
    for (index, operation) in changeset.operations.iter().enumerate() {
        apply_operation(&mut raw, operation).map_err(|error| match error {
            ChangeSetError::InvalidOperation(message) => {
                ChangeSetError::InvalidOperation(format!("第 {} 个操作：{message}", index + 1))
            }
            ChangeSetError::TargetNotFound(message) => {
                ChangeSetError::TargetNotFound(format!("第 {} 个操作：{message}", index + 1))
            }
            other => other,
        })?;
    }

    // ---- 提交元数据 ----
    let project_uuid = if disk_uuid.is_empty() {
        if changeset.project_id.trim().is_empty() {
            crate::ai_engine::face_frame::new_face_uuid()
        } else {
            changeset.project_id.clone()
        }
    } else {
        disk_uuid
    };
    let new_revision = disk_revision + 1;
    let created_at = if changeset.created_at.trim().is_empty() {
        project::iso_now()
    } else {
        changeset.created_at.clone()
    };
    let mut applied = applied_requests(&raw);
    applied.push(token);
    let keep_from = applied.len().saturating_sub(APPLIED_REQUEST_LIMIT);
    let applied: Vec<Value> = applied[keep_from..]
        .iter()
        .cloned()
        .map(Value::String)
        .collect();
    let root = raw.as_object_mut().expect("checked object");
    root.insert("projectUuid".into(), Value::String(project_uuid.clone()));
    root.insert("revision".into(), Value::from(new_revision));
    if root
        .get("createdAt")
        .and_then(Value::as_str)
        .map(str::is_empty)
        .unwrap_or(true)
    {
        root.insert("createdAt".into(), Value::String(created_at));
    }
    root.insert("updatedAt".into(), Value::String(project::iso_now()));
    root.insert("lumiAppliedRequests".into(), Value::Array(applied));

    // ---- 提交前整体校验，并同步派生记录 ----
    let mut typed: ProjectDoc = serde_json::from_value(raw.clone())
        .map_err(|error| ChangeSetError::InvalidOperation(format!("操作结果结构无效：{error}")))?;
    if typed.version > PROJECT_VERSION {
        return Err(ChangeSetError::InvalidOperation(format!(
            "项目版本 {} 高于当前支持的 {}",
            typed.version, PROJECT_VERSION
        )));
    }
    for (index, photo) in typed.photos.iter_mut().enumerate() {
        photo.sync_subject_records();
        let subjects = serde_json::to_value(&photo.subjects)
            .map_err(|error| ChangeSetError::Storage(error.to_string()))?;
        raw["photos"][index]["subjects"] = subjects;
    }

    // ---- 单次原子写：失败不会留下半成品 ----
    let json = serde_json::to_vec_pretty(&raw)
        .map_err(|error| ChangeSetError::Storage(format!("序列化失败：{error}")))?;
    project::write_atomically(path, &json)?;

    // ---- Debug 往返断言：写盘后重新读取，逐字段比较 ----
    #[cfg(debug_assertions)]
    verify_roundtrip(path, &typed, &project_uuid)?;

    Ok(ChangeSetOutcome {
        project_uuid,
        revision: new_revision,
        duplicate: false,
    })
}

/// 幂等 token：`requestId` + 操作内容指纹。
/// 同一个 requestId 携带不同操作时不会被误认为重放。
fn request_token(changeset: &ChangeSet) -> String {
    use sha2::{Digest, Sha256};
    let payload = serde_json::to_vec(&changeset.operations).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(&payload);
    let digest = hasher.finalize();
    let fingerprint: String = digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("{}#{fingerprint}", changeset.request_id)
}

fn applied_requests(raw: &Value) -> Vec<String> {
    raw.get("lumiAppliedRequests")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// 按声明顺序应用单条操作。只触碰该操作声明的子树。
fn apply_operation(raw: &mut Value, operation: &Operation) -> Result<(), ChangeSetError> {
    match operation {
        Operation::UpdateDocMeta { patch } => {
            ensure_patch(patch, &doc_meta_keys())?;
            let root = raw
                .as_object_mut()
                .ok_or_else(|| invalid("项目根不是对象"))?;
            merge_object(root, patch, "项目");
            Ok(())
        }
        Operation::UpdatePhotoParams { photo, patch } => {
            let index = photo_index(raw, photo)?;
            ensure_patch(patch, &struct_keys::<EditParams>(EditParams::default()))?;
            let target = raw["photos"][index]["params"]
                .as_object_mut()
                .ok_or_else(|| invalid(format!("{photo} 缺少 params")))?;
            merge_object(target, patch, "params");
            Ok(())
        }
        Operation::UpdateFaceParams {
            photo,
            face_uuid,
            patch,
        } => {
            let index = photo_index(raw, photo)?;
            ensure_uuid(face_uuid, "faceUuid")?;
            ensure_patch(patch, &face_keys())?;
            let faces = raw["photos"][index]["params"]["faces"]
                .as_array_mut()
                .ok_or_else(|| invalid(format!("{photo} 缺少 params.faces")))?;
            let target = faces
                .iter_mut()
                .find(|face| face.get("faceUuid").and_then(Value::as_str) == Some(face_uuid))
                .and_then(Value::as_object_mut)
                .ok_or_else(|| not_found(format!("{photo} 中的人脸 {face_uuid}")))?;
            merge_object(target, patch, "face");
            Ok(())
        }
        Operation::UpdateReshapeParams {
            photo,
            face_uuid,
            patch,
        } => {
            let index = photo_index(raw, photo)?;
            ensure_uuid(face_uuid, "faceUuid")?;
            ensure_patch(patch, &reshape_keys())?;
            let items = raw["photos"][index]["params"]["reshape"]
                .as_array_mut()
                .ok_or_else(|| invalid(format!("{photo} 缺少 params.reshape")))?;
            let target = items
                .iter_mut()
                .find(|item| item.get("faceUuid").and_then(Value::as_str) == Some(face_uuid))
                .and_then(Value::as_object_mut)
                .ok_or_else(|| not_found(format!("{photo} 中的塑形记录 {face_uuid}")))?;
            merge_object(target, patch, "reshape");
            Ok(())
        }
        Operation::UpdateSubjectMask { photo, patch } => {
            let index = photo_index(raw, photo)?;
            ensure_patch(
                patch,
                &struct_keys::<SubjectMaskParams>(SubjectMaskParams::default()),
            )?;
            let target = raw["photos"][index]["params"]["subjectMask"]
                .as_object_mut()
                .ok_or_else(|| invalid(format!("{photo} 缺少 params.subjectMask")))?;
            merge_object(target, patch, "subjectMask");
            Ok(())
        }
        Operation::UpdateManualMask { photo, patch } => {
            let index = photo_index(raw, photo)?;
            ensure_patch(
                patch,
                &struct_keys::<ManualMask>(ManualMask::default()),
            )?;
            let target = raw["photos"][index]["params"]["manualMask"]
                .as_object_mut()
                .ok_or_else(|| invalid(format!("{photo} 缺少 params.manualMask")))?;
            merge_object(target, patch, "manualMask");
            Ok(())
        }
        Operation::UpdateBackground { photo, patch } => {
            let index = photo_index(raw, photo)?;
            ensure_patch(
                patch,
                &struct_keys::<BackgroundParams>(BackgroundParams::default()),
            )?;
            let target = raw["photos"][index]["params"]["background"]
                .as_object_mut()
                .ok_or_else(|| invalid(format!("{photo} 缺少 params.background")))?;
            merge_object(target, patch, "background");
            Ok(())
        }
        Operation::UpdateRawParams { photo, patch } => {
            let index = photo_index(raw, photo)?;
            ensure_patch(patch, &raw_keys())?;
            let target = raw["photos"][index]["raw"]["params"]
                .as_object_mut()
                .ok_or_else(|| not_found(format!("{photo} 的 RAW 参数（照片不是 RAW）")))?;
            merge_object(target, patch, "raw.params");
            Ok(())
        }
        Operation::UpdateColorParams { photo, patch } => {
            let index = photo_index(raw, photo)?;
            ensure_patch(patch, &color_keys())?;
            let target = raw["photos"][index]["color"]["output"]
                .as_object_mut()
                .ok_or_else(|| not_found(format!("{photo} 的色彩输出设置")))?;
            merge_object(target, patch, "color.output");
            Ok(())
        }
        Operation::UpdateExportParams { photo, patch } => {
            ensure_patch(patch, &export_keys())?;
            match photo {
                Some(photo) => {
                    let index = photo_index(raw, photo)?;
                    let target = raw["photos"][index]["export"]
                        .as_object_mut()
                        .ok_or_else(|| not_found(format!("{photo} 的导出设置")))?;
                    merge_object(target, patch, "export");
                }
                None => {
                    let target = raw["export"]
                        .as_object_mut()
                        .ok_or_else(|| invalid("项目缺少 export"))?;
                    merge_object(target, patch, "export");
                }
            }
            Ok(())
        }
        Operation::AddFace { photo, face } => {
            let index = photo_index(raw, photo)?;
            let uuid = face
                .get("faceUuid")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("AddFace 缺少 faceUuid"))?
                .to_string();
            ensure_uuid(&uuid, "faceUuid")?;
            let faces = raw["photos"][index]["params"]["faces"]
                .as_array_mut()
                .ok_or_else(|| invalid(format!("{photo} 缺少 params.faces")))?;
            if faces
                .iter()
                .any(|entry| entry.get("faceUuid").and_then(Value::as_str) == Some(uuid.as_str()))
            {
                return Err(invalid(format!("人脸 {uuid} 已存在，不能重复新增")));
            }
            faces.push(face.clone());
            Ok(())
        }
        Operation::RemoveFace { photo, face_uuid } => {
            let index = photo_index(raw, photo)?;
            ensure_uuid(face_uuid, "faceUuid")?;
            let faces = raw["photos"][index]["params"]["faces"]
                .as_array_mut()
                .ok_or_else(|| invalid(format!("{photo} 缺少 params.faces")))?;
            let before = faces.len();
            faces.retain(|entry| entry.get("faceUuid").and_then(Value::as_str) != Some(face_uuid));
            if faces.len() == before {
                return Err(not_found(format!("{photo} 中的人脸 {face_uuid}")));
            }
            Ok(())
        }
        Operation::AddMask { photo, mask } => {
            let index = photo_index(raw, photo)?;
            let id = mask
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("AddMask 缺少 id"))?
                .to_string();
            ensure_uuid(&id, "mask.id")?;
            let masks = raw["photos"][index]["masks"]
                .as_array_mut()
                .ok_or_else(|| invalid(format!("{photo} 缺少 masks")))?;
            if masks
                .iter()
                .any(|entry| entry.get("id").and_then(Value::as_str) == Some(id.as_str()))
            {
                return Err(invalid(format!("蒙版 {id} 已存在，不能重复新增")));
            }
            masks.push(mask.clone());
            Ok(())
        }
        Operation::UpdateMask {
            photo,
            mask_id,
            patch,
        } => {
            let index = photo_index(raw, photo)?;
            ensure_uuid(mask_id, "maskId")?;
            ensure_patch(patch, &mask_keys())?;
            let masks = raw["photos"][index]["masks"]
                .as_array_mut()
                .ok_or_else(|| invalid(format!("{photo} 缺少 masks")))?;
            let target = masks
                .iter_mut()
                .find(|entry| entry.get("id").and_then(Value::as_str) == Some(mask_id))
                .and_then(Value::as_object_mut)
                .ok_or_else(|| not_found(format!("{photo} 中的蒙版 {mask_id}")))?;
            merge_object(target, patch, "mask");
            Ok(())
        }
        Operation::RemoveMask { photo, mask_id } => {
            let index = photo_index(raw, photo)?;
            ensure_uuid(mask_id, "maskId")?;
            let masks = raw["photos"][index]["masks"]
                .as_array_mut()
                .ok_or_else(|| invalid(format!("{photo} 缺少 masks")))?;
            let before = masks.len();
            masks.retain(|entry| entry.get("id").and_then(Value::as_str) != Some(mask_id));
            if masks.len() == before {
                return Err(not_found(format!("{photo} 中的蒙版 {mask_id}")));
            }
            Ok(())
        }
        Operation::UpsertFaceRecord { photo, record } => {
            let index = photo_index(raw, photo)?;
            let uuid = record
                .get("uuid")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("UpsertFaceRecord 缺少 uuid"))?
                .to_string();
            ensure_uuid(&uuid, "face.uuid")?;
            let records = raw["photos"][index]["faces"]
                .as_array_mut()
                .ok_or_else(|| invalid(format!("{photo} 缺少 faces")))?;
            match records
                .iter_mut()
                .find(|entry| entry.get("uuid").and_then(Value::as_str) == Some(uuid.as_str()))
            {
                Some(slot) => *slot = record.clone(),
                None => records.push(record.clone()),
            }
            Ok(())
        }
        Operation::RemoveFaceRecord { photo, face_uuid } => {
            let index = photo_index(raw, photo)?;
            ensure_uuid(face_uuid, "faceUuid")?;
            let records = raw["photos"][index]["faces"]
                .as_array_mut()
                .ok_or_else(|| invalid(format!("{photo} 缺少 faces")))?;
            let before = records.len();
            records.retain(|entry| entry.get("uuid").and_then(Value::as_str) != Some(face_uuid));
            if records.len() == before {
                return Err(not_found(format!("{photo} 的人脸记录 {face_uuid}")));
            }
            Ok(())
        }
        Operation::AddPhoto { photo } => {
            let path = photo
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("AddPhoto 缺少 path"))?;
            let photos = raw["photos"]
                .as_array_mut()
                .ok_or_else(|| invalid("项目缺少 photos"))?;
            if photos
                .iter()
                .any(|entry| entry.get("path").and_then(Value::as_str) == Some(path))
            {
                return Err(invalid(format!("照片 {path} 已存在")));
            }
            photos.push(photo.clone());
            Ok(())
        }
        Operation::UpdateSource { photo, patch } => {
            let index = photo_index(raw, photo)?;
            ensure_patch(patch, &source_keys())?;
            if raw["photos"][index]["source"].is_null() {
                raw["photos"][index]["source"] = Value::Object(Map::new());
            }
            let target = raw["photos"][index]["source"]
                .as_object_mut()
                .ok_or_else(|| invalid(format!("{photo} 的 source 结构无效")))?;
            merge_object(target, patch, "source");
            Ok(())
        }
        Operation::UpdateBatchParams { photos, patch } => {
            ensure_patch(patch, &struct_keys::<EditParams>(EditParams::default()))?;
            for photo in photos {
                let index = photo_index(raw, photo)?;
                let target = raw["photos"][index]["params"]
                    .as_object_mut()
                    .ok_or_else(|| invalid(format!("{photo} 缺少 params")))?;
                merge_object(target, patch, "params");
            }
            Ok(())
        }
    }
}

fn invalid(message: impl Into<String>) -> ChangeSetError {
    ChangeSetError::InvalidOperation(message.into())
}

fn not_found(message: impl Into<String>) -> ChangeSetError {
    ChangeSetError::TargetNotFound(message.into())
}

fn ensure_uuid(value: &str, field: &str) -> Result<(), ChangeSetError> {
    if value.trim().is_empty() {
        return Err(invalid(format!("{field} 不能为空")));
    }
    Ok(())
}

fn photo_index(raw: &Value, photo: &str) -> Result<usize, ChangeSetError> {
    let photos = raw
        .get("photos")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("项目缺少 photos"))?;
    photos
        .iter()
        .position(|entry| entry.get("path").and_then(Value::as_str) == Some(photo))
        .ok_or_else(|| not_found(format!("照片 {photo}")))
}

/// 补丁必须是对象；未出现的字段不修改，`null` 表示显式置空。
fn ensure_patch(patch: &Value, allowed: &BTreeSet<String>) -> Result<(), ChangeSetError> {
    let object = patch
        .as_object()
        .ok_or_else(|| invalid("patch 必须是对象"))?;
    for key in object.keys() {
        if !allowed.contains(key) {
            return Err(invalid(format!("未知字段 {key}（不属于该操作目标）")));
        }
    }
    Ok(())
}

/// 递归合并：对象逐键合并；数组/标量/`null` 直接替换。
/// 缺失的键保持原值，因此不会把未修改字段重置为默认值。
fn merge_object(target: &mut Map<String, Value>, patch: &Value, path: &str) {
    let Some(object) = patch.as_object() else {
        return;
    };
    for (key, value) in object {
        match (target.get_mut(key), value) {
            (Some(Value::Object(existing)), Value::Object(_)) => {
                merge_object(existing, value, &format!("{path}.{key}"));
            }
            _ => {
                target.insert(key.clone(), value.clone());
            }
        }
    }
}

fn struct_keys<T: Serialize + Default>(sample: T) -> BTreeSet<String> {
    value_keys(&serde_json::to_value(sample).unwrap_or(Value::Null))
}

fn sample_keys<T: DeserializeOwned + Serialize>(sample: Value) -> BTreeSet<String> {
    let parsed: T = serde_json::from_value(sample).unwrap_or_else(|_| unreachable!("sample"));
    value_keys(&serde_json::to_value(parsed).unwrap_or(Value::Null))
}

fn value_keys(value: &Value) -> BTreeSet<String> {
    value
        .as_object()
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default()
}

fn doc_meta_keys() -> BTreeSet<String> {
    ["activeIndex", "export", "compute"]
        .iter()
        .map(|key| key.to_string())
        .collect()
}

fn face_keys() -> BTreeSet<String> {
    value_keys(
        &serde_json::to_value(crate::image_engine::params::FaceBeauty::new(0))
            .expect("FaceBeauty 可序列化"),
    )
}

fn reshape_keys() -> BTreeSet<String> {
    sample_keys::<crate::image_engine::params::FaceReshape>(serde_json::json!({ "faceId": 0 }))
}

fn raw_keys() -> BTreeSet<String> {
    struct_keys(crate::image_engine::raw::RawDevelopParams::default())
}

fn color_keys() -> BTreeSet<String> {
    struct_keys(crate::color_management::ColorOutputSettings::default())
}

fn export_keys() -> BTreeSet<String> {
    struct_keys(crate::image_engine::export::ExportSettings::default())
}

fn source_keys() -> BTreeSet<String> {
    struct_keys(crate::image_engine::project::ProjectSource::default())
}

fn mask_keys() -> BTreeSet<String> {
    ["id", "name", "kind", "enabled", "invert", "feather", "opacity", "points"]
        .iter()
        .map(|key| key.to_string())
        .collect()
}

/// Debug 构建的持久化往返断言：
/// 目标状态 → 写盘 → 重新读取 → 规范化 → 比较。
/// 数字使用 1e-6 容差，其余（UUID、topology、模型版本、蒙版 hash、未知字段）严格相等。
#[cfg(debug_assertions)]
fn verify_roundtrip(
    path: &Path,
    expected: &ProjectDoc,
    project_uuid: &str,
) -> Result<(), ChangeSetError> {
    let bytes = std::fs::read(path)?;
    let actual: ProjectDoc = serde_json::from_slice(&bytes)
        .map_err(|error| ChangeSetError::Storage(format!("回读项目失败：{error}")))?;
    let left = serde_json::to_value(expected)
        .map_err(|error| ChangeSetError::Storage(error.to_string()))?;
    let right = serde_json::to_value(&actual)
        .map_err(|error| ChangeSetError::Storage(error.to_string()))?;
    let mut differences = Vec::new();
    compare_values("", &left, &right, &mut differences);
    if differences.is_empty() {
        return Ok(());
    }
    let detail = differences.join("; ");
    // 不记录照片内容，只记录字段路径、实体标识与代数。
    Err(ChangeSetError::Storage(format!(
        "持久化往返校验失败（project {project_uuid}, revision {}）：{detail}",
        expected.revision
    )))
}

#[cfg(debug_assertions)]
fn compare_values(path: &str, left: &Value, right: &Value, out: &mut Vec<String>) {
    const TOLERANCE: f64 = 1e-6;
    if out.len() >= 8 {
        return;
    }
    match (left, right) {
        (Value::Number(a), Value::Number(b)) => {
            let (Some(a), Some(b)) = (a.as_f64(), b.as_f64()) else {
                if a != b {
                    out.push(format!("{} 数字表示不同", label(path)));
                }
                return;
            };
            if (a - b).abs() > TOLERANCE {
                out.push(format!("{} 数值不同（{a} != {b}）", label(path)));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            let keys: BTreeSet<&String> = a.keys().chain(b.keys()).collect();
            for key in keys {
                let next = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                match (a.get(key), b.get(key)) {
                    (Some(a), Some(b)) => compare_values(&next, a, b, out),
                    (Some(_), None) => out.push(format!("{} 回读后丢失", label(&next))),
                    (None, Some(_)) => out.push(format!("{} 回读后多出", label(&next))),
                    (None, None) => {}
                }
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                out.push(format!("{} 长度不同（{} != {}）", label(path), a.len(), b.len()));
                return;
            }
            for (index, (a, b)) in a.iter().zip(b.iter()).enumerate() {
                compare_values(&format!("{path}[{index}]"), a, b, out);
            }
        }
        (Value::Null, Value::Null) => {}
        (a, b) if a == b => {}
        _ => out.push(format!("{} 值不同", label(path))),
    }
}

#[cfg(debug_assertions)]
fn label(path: &str) -> String {
    if path.is_empty() {
        return "项目根".into();
    }
    // 只保留结构定位，不输出照片内容。
    format!("字段 {path}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_project(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("lumi-round10-changeset-unit-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("project.lumi")
    }

    fn write_base(path: &Path) {
        let doc = json!({
            "version": 3,
            "createdAt": "2026-01-01T00:00:00Z",
            "activeIndex": 0,
            "export": { "format": "jpeg", "quality": 92 },
            "photos": [{
                "path": "/photos/a.jpg",
                "params": {
                    "exposure": 0.0,
                    "contrast": 0.0,
                    "highlights": 0.0,
                    "shadows": 0.0,
                    "whites": 0.0,
                    "blacks": 0.0,
                    "temperature": 0.0,
                    "tint": 0.0,
                    "saturation": 0.0,
                    "vibrance": 0.0,
                    "clarity": 0.0,
                    "sharpness": 0.0,
                    "grain": 0.0,
                    "vignette": 0.0,
                    "skinSmooth": 0.0,
                    "faces": [],
                    "reshape": [],
                    "subjectMask": { "enabled": false, "invert": false, "feather": 40.0, "expand": 0.0, "source": "manual", "modelVersion": "", "strokes": [] },
                    "body": [],
                    "background": { "enabled": false, "mode": "blur", "blur": 0.0, "depth": 0.0, "brightness": 0.0, "temperature": 0.0, "saturation": 0.0, "color": "#000000", "gradient": { "angle": 0.0, "stops": [] }, "imagePath": "", "imageFit": "cover", "imageScale": 100.0, "imageOffsetX": 0.0, "imageOffsetY": 0.0, "imageBlur": 0.0, "decontaminate": 0.0 },
                    "manualMask": { "enabled": false, "invert": false, "feather": 40.0, "strength": 100.0, "strokes": [] },
                    "repairPoints": [],
                    "hsl": { "red": { "hue": 0.0, "saturation": 0.0, "luminance": 0.0 } },
                    "curves": {},
                    "lut": null,
                    "futureParamField": { "keep": true }
                },
                "masks": [],
                "export": { "format": "jpeg", "quality": 92 },
                "sourceHash": "abc",
                "faces": [],
                "subjects": [],
                "maskMeta": null,
                "source": { "sourceType": "raster", "path": "/photos/a.jpg", "hash": "abc", "metadata": { "camera": "X" } },
                "raw": null,
                "color": { "output": { "profile": "srgb", "customPath": null, "intent": "relative", "blackPointCompensation": true, "embed": true } },
                "futurePhotoField": 7
            }],
            "projectUuid": "11111111-1111-4111-8111-111111111111",
            "revision": 4,
            "updatedAt": "2026-01-01T00:00:00Z",
            "futureDocField": ["keep"]
        });
        std::fs::write(path, serde_json::to_vec_pretty(&doc).unwrap()).unwrap();
    }

    fn cs(operations: Vec<Operation>) -> ChangeSet {
        ChangeSet {
            project_id: "11111111-1111-4111-8111-111111111111".into(),
            base_revision: 4,
            request_id: "req-1".into(),
            operations,
            created_at: "2026-01-02T00:00:00Z".into(),
            rebase_on_conflict: false,
        }
    }

    #[test]
    fn patch_only_touches_declared_fields() {
        let path = temp_project("declared");
        write_base(&path);
        let outcome = apply(
            &path,
            &cs(vec![Operation::UpdatePhotoParams {
                photo: "/photos/a.jpg".into(),
                patch: json!({ "exposure": 12.0 }),
            }]),
        )
        .unwrap();
        assert_eq!(outcome.revision, 5);
        let doc = project::load(&path).unwrap();
        assert_eq!(doc.photos[0].params.exposure, 12.0);
        assert_eq!(doc.photos[0].params.contrast, 0.0);
        assert_eq!(doc.photos[0].source_hash, "abc");
        assert_eq!(
            doc.photos[0].extra.get("futurePhotoField"),
            Some(&json!(7))
        );
        assert_eq!(
            doc.photos[0].params.extra.get("futureParamField"),
            Some(&json!({ "keep": true }))
        );
        assert_eq!(doc.extra.get("futureDocField"), Some(&json!(["keep"])));
        assert_eq!(doc.created_at, "2026-01-01T00:00:00Z");
    }

    #[test]
    fn null_clears_optional_but_missing_leaves_value() {
        let path = temp_project("null");
        write_base(&path);
        apply(
            &path,
            &cs(vec![Operation::UpdateSource {
                photo: "/photos/a.jpg".into(),
                patch: json!({ "metadata": null }),
            }]),
        )
        .unwrap();
        let doc = project::load(&path).unwrap();
        assert_eq!(doc.photos[0].source.as_ref().unwrap().metadata, None);
        // hash 未出现 → 不变。
        assert_eq!(doc.photos[0].source.as_ref().unwrap().hash, "abc");
    }

    #[test]
    fn unknown_patch_key_is_rejected() {
        let path = temp_project("unknown-key");
        write_base(&path);
        let error = apply(
            &path,
            &cs(vec![Operation::UpdatePhotoParams {
                photo: "/photos/a.jpg".into(),
                patch: json!({ "exposrue": 1.0 }),
            }]),
        )
        .unwrap_err();
        assert_eq!(error.kind(), "INVALID_OPERATION");
        assert_eq!(project::load(&path).unwrap().revision, 4, "失败不得写盘");
    }

    #[test]
    fn project_and_entity_ids_are_validated() {
        let path = temp_project("identity");
        write_base(&path);
        let mut request = cs(vec![Operation::UpdatePhotoParams {
            photo: "/photos/missing.jpg".into(),
            patch: json!({ "exposure": 1.0 }),
        }]);
        assert_eq!(
            apply(&path, &request).unwrap_err().kind(),
            "TARGET_NOT_FOUND"
        );
        request.operations = vec![Operation::UpdatePhotoParams {
            photo: "/photos/a.jpg".into(),
            patch: json!({ "exposure": 1.0 }),
        }];
        request.project_id = "22222222-2222-4222-8222-222222222222".into();
        assert_eq!(
            apply(&path, &request).unwrap_err().kind(),
            "PROJECT_MISMATCH"
        );
    }

    #[test]
    fn stale_revision_is_rejected_without_writing() {
        let path = temp_project("stale");
        write_base(&path);
        let mut request = cs(vec![Operation::UpdatePhotoParams {
            photo: "/photos/a.jpg".into(),
            patch: json!({ "exposure": 50.0 }),
        }]);
        request.base_revision = 3;
        let error = apply(&path, &request).unwrap_err();
        assert_eq!(error.kind(), "REVISION_CONFLICT");
        assert_eq!(error.current_revision(), Some(4));
        assert_eq!(project::load(&path).unwrap().photos[0].params.exposure, 0.0);
    }

    #[test]
    fn repeated_request_is_idempotent() {
        let path = temp_project("idempotent");
        write_base(&path);
        let request = cs(vec![Operation::AddMask {
            photo: "/photos/a.jpg".into(),
            mask: json!({ "id": "mask-1", "name": "m", "kind": "brush", "enabled": true, "invert": false, "feather": 40.0, "opacity": 100.0, "points": [] }),
        }]);
        let first = apply(&path, &request).unwrap();
        assert!(!first.duplicate);
        // 同一个 requestId 重放：不重复新增、不递增 revision。
        let second = apply(&path, &request).unwrap();
        assert!(second.duplicate);
        assert_eq!(second.revision, first.revision);
        let doc = project::load(&path).unwrap();
        assert_eq!(doc.photos[0].masks.len(), 1);
        assert_eq!(doc.revision, first.revision);
    }

    #[test]
    fn removal_must_be_explicit() {
        let path = temp_project("explicit-remove");
        write_base(&path);
        apply(
            &path,
            &cs(vec![Operation::UpdateMask {
                photo: "/photos/a.jpg".into(),
                mask_id: "nope".into(),
                patch: json!({ "opacity": 10.0 }),
            }]),
        )
        .unwrap_err();
        apply(
            &path,
            &cs(vec![Operation::RemoveMask {
                photo: "/photos/a.jpg".into(),
                mask_id: "nope".into(),
            }]),
        )
        .unwrap_err();
        // 失败请求没有写入，也没有被记为已执行。
        let doc = project::load(&path).unwrap();
        assert_eq!(doc.revision, 4);
    }

    #[test]
    fn batch_operation_applies_same_patch() {
        let path = temp_project("batch");
        write_base(&path);
        apply(
            &path,
            &cs(vec![Operation::UpdateBatchParams {
                photos: vec!["/photos/a.jpg".into()],
                patch: json!({ "contrast": -5.0 }),
            }]),
        )
        .unwrap();
        let doc = project::load(&path).unwrap();
        assert_eq!(doc.photos[0].params.contrast, -5.0);
    }
}
