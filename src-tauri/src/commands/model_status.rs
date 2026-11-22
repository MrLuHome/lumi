//! 模型注册表 / 推理准入的命令边界。
//!
//! 四个命令的设计约束：
//! - **不改变** `FaceModelStatus` / `SkinModelStatus` / `BeautyAnalysis` 等既有形状
//!   （`BeautyPanel.tsx`、`FacePanel.tsx`、`ModelHubPanel.tsx` 零改动）；
//! - 新命令返回 `InferenceError`（`{code, message, modelId, detail}`），
//!   而不是 `AppError` 的裸字符串；
//! - 所有会做文件 IO 的命令都进 `spawn_blocking`，唯一例外是 `model_unload`
//!   （纯内存操作，见注释）；
//! - `resource_dir()` 只在本层调用，`ai_engine` 保持无 Tauri 依赖。

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::ai::FaceEngine;
use crate::ai_engine::inference::InferenceError;
use crate::ai_engine::model_registry::locate;
use crate::ai_engine::model_registry::{
    builtin, require, ModelFailure, ModelRuntimeState, ModelSpec, ModelTask, Registry,
    RegistrySnapshot,
};

/// 资源基础目录候选。
///
/// 顺序刻意与 `commands/faces.rs::model_path` 保持一致，再追加仓库内
/// `resources/`（开发期回退，打包后不会命中）：
/// 1. `<resource_dir>/resources` → macOS/Windows 打包后的实际布局
/// 2. `<resource_dir>` → 部分打包器把 resources 直接摊平
/// 3. `<CARGO_MANIFEST_DIR>/resources` / `<CARGO_MANIFEST_DIR>` → `cargo test` / `tauri dev`
///
/// Windows 的第二候选本轮**未实测**（任务书 Q5），但 `locate()` 接受候选列表，
/// 上 Windows 时只需调整这里的顺序。
fn base_dirs(app: &AppHandle) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(resources) = app.path().resource_dir() {
        dirs.push(resources.join("resources"));
        dirs.push(resources);
    }
    dirs.extend(locate::dev_base_dirs());
    dirs
}

/// 单条模型的运行时结果（`model_load` / `model_unload` 的返回）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRuntimeStatus {
    pub model_id: String,
    pub state: &'static str,
    pub loaded: bool,
    pub elapsed_ms: u64,
    pub failure: Option<ModelFailure>,
    pub backend: &'static str,
}

fn status_from_state(
    model_id: &str,
    state: &ModelRuntimeState,
    elapsed_ms: u64,
    backend: &'static str,
) -> ModelRuntimeStatus {
    ModelRuntimeStatus {
        model_id: model_id.to_string(),
        state: state.as_str(),
        loaded: state.is_loaded(),
        elapsed_ms,
        failure: state.failure().cloned(),
        backend,
    }
}

/// 把结构化错误还原成失败态，用于写回 registry 状态。
fn failure_of(spec: &ModelSpec, error: &InferenceError) -> ModelFailure {
    match error {
        InferenceError::LicenseNotCleared {
            which, detail, ..
        } => ModelFailure::LicenseNotCleared {
            model_id: spec.id.to_string(),
            which,
            what_to_check: detail.clone(),
        },
        InferenceError::ModelMissing { model_id, searched } => ModelFailure::Missing {
            model_id: model_id.clone(),
            searched: searched.clone(),
        },
        InferenceError::ModelCorrupt { model_id, detail } => ModelFailure::Corrupt {
            model_id: model_id.clone(),
            detail: detail.clone(),
            expected_sha: spec.sha256.to_string(),
            actual_sha: None,
        },
        InferenceError::ModelVersionMismatch {
            model_id,
            expected,
            actual,
        } => ModelFailure::VersionMismatch {
            model_id: model_id.clone(),
            expected: expected.clone(),
            actual: actual.clone(),
        },
        other => ModelFailure::LoadFailed {
            model_id: spec.id.to_string(),
            detail: other.to_string(),
        },
    }
}

/// 查询全部模型状态。
///
/// **只读内存 + 存在性 + 体积 + 格式头，不计算哈希**：面板挂载时可安全调用。
/// 之所以仍然进 `spawn_blocking`：读取用户模型 manifest 需要若干次小文件 IO。
#[tauri::command]
pub async fn model_registry_snapshot(
    app: AppHandle,
    registry: State<'_, Arc<Registry>>,
    engine: State<'_, FaceEngine>,
) -> Result<RegistrySnapshot, InferenceError> {
    let registry = registry.inner().clone();
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let dirs = base_dirs(&app);
        let mut snapshot = registry.snapshot(&dirs);
        // plan 由共享 FaceEngine 持有，registry 不重复记录：这里回填真实加载态。
        if engine.is_loaded() {
            snapshot.mark_ready(builtin::YUNET.id, true);
        }
        Ok(snapshot)
    })
    .await
    .map_err(|error| InferenceError::Runtime(format!("状态查询失败：{error}")))?
}

/// 全量校验（体积 + 格式头 + 契约 + SHA256）。`model_ids` 省略 = 全部内置模型。
///
/// 耗时与权重体积成正比，绝不能在同步命令里调用。
#[tauri::command]
pub async fn model_registry_check(
    app: AppHandle,
    registry: State<'_, Arc<Registry>>,
    model_ids: Option<Vec<String>>,
) -> Result<RegistrySnapshot, InferenceError> {
    let registry = registry.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let dirs = base_dirs(&app);
        let ids = match model_ids {
            Some(ids) if !ids.is_empty() => ids,
            _ => builtin::ids().iter().map(|id| id.to_string()).collect(),
        };
        for id in ids {
            let spec = require(&id).ok_or_else(|| InferenceError::ModelLoadFailed {
                model_id: id.clone(),
                detail: "该模型 ID 未在登记表中登记".into(),
            })?;
            registry.check(spec, &dirs, true);
        }
        Ok(registry.snapshot(&dirs))
    })
    .await
    .map_err(|error| InferenceError::Runtime(format!("模型校验失败：{error}")))?
}

/// 预热加载：走完整许可闸门 → 校验 → 解析优化。
#[tauri::command]
pub async fn model_load(
    app: AppHandle,
    registry: State<'_, Arc<Registry>>,
    engine: State<'_, FaceEngine>,
    model_id: String,
) -> Result<ModelRuntimeStatus, InferenceError> {
    let registry = registry.inner().clone();
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let started = Instant::now();
        let spec = require(&model_id).ok_or_else(|| InferenceError::ModelLoadFailed {
            model_id: model_id.clone(),
            detail: "该模型 ID 未在登记表中登记".into(),
        })?;
        let dirs = base_dirs(&app);
        // ①–③ + ⑤：许可 → 存在 → 格式 → SHA256（契约在解析后补验）。
        let health = registry.check(spec, &dirs, false);
        if let Some(error) = health.error() {
            return Err(error);
        }
        let path = health
            .resolved_path
            .clone()
            .ok_or_else(|| InferenceError::ModelLoadFailed {
                model_id: spec.id.to_string(),
                detail: "校验通过但未能定位权重路径".into(),
            })?;
        // 目前只有人脸检测有真实运行时；其它任务登记后也要先有后端实现。
        if spec.task != ModelTask::FaceDetect {
            return Err(InferenceError::BackendUnsupported(format!(
                "{} 尚无运行时实现",
                spec.task.label()
            )));
        }
        match engine.load_checked(&path) {
            Ok(()) => {
                registry.mark_ready(spec.id, path.clone());
                Ok(ModelRuntimeStatus {
                    model_id: spec.id.to_string(),
                    state: "ready",
                    loaded: true,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    failure: None,
                    backend: "cpu-tract",
                })
            }
            Err(error) => {
                registry.mark_failure(spec.id, failure_of(spec, &error));
                Err(error)
            }
        }
    })
    .await
    .map_err(|error| InferenceError::Runtime(format!("模型加载失败：{error}")))?
}

/// 释放已加载模型。
///
/// **同步命令是安全的**：只 drop 内存里的 plan 句柄，再用
/// `resource_dir()` + `is_file()` + `metadata` 重新给出浅状态，
/// 没有任何整文件读取或哈希（否则就会重演历史风险 R1）。
#[tauri::command]
pub fn model_unload(
    app: AppHandle,
    registry: State<'_, Arc<Registry>>,
    engine: State<'_, FaceEngine>,
    model_id: String,
) -> Result<ModelRuntimeStatus, InferenceError> {
    let started = Instant::now();
    let spec = require(&model_id).ok_or_else(|| InferenceError::ModelLoadFailed {
        model_id: model_id.clone(),
        detail: "该模型 ID 未在登记表中登记".into(),
    })?;
    engine.unload();
    let dirs = base_dirs(&app);
    let health = registry.check_shallow(spec, &dirs);
    let state = health.state();
    Ok(status_from_state(
        spec.id,
        &state,
        started.elapsed().as_millis() as u64,
        "cpu-tract",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_engine::inference::InferenceError;

    /// 命令层**不依赖 tauri runtime** 的契约测试：只验证"前端拿到什么形状"。
    ///
    /// 覆盖不到 `failure_of()` 的分支选择（那需要真实 IO），但能锁住
    /// camelCase 键名与 `InferenceError` 的四字段形状 —— 这两点一旦漂移，
    /// 前端 `switch (error.code)` 会静默失效。
    #[test]
    fn runtime_status_serializes_with_camel_case_keys() {
        let status = ModelRuntimeStatus {
            model_id: "face-detect-yunet-2023mar".into(),
            state: "ready",
            loaded: true,
            elapsed_ms: 128,
            failure: None,
            backend: "cpu-tract",
        };
        let value = serde_json::to_value(&status).unwrap();
        for key in ["modelId", "state", "loaded", "elapsedMs", "failure", "backend"] {
            assert!(value.get(key).is_some(), "缺少字段 `{key}`：{value}");
        }
        assert_eq!(value["modelId"], "face-detect-yunet-2023mar");
        assert_eq!(value["state"], "ready");
        assert_eq!(value["loaded"], true);
        assert_eq!(value["elapsedMs"], 128);
        assert!(value["failure"].is_null());
    }

    #[test]
    fn status_from_state_maps_failures_into_the_view() {
        let failure = crate::ai_engine::model_registry::ModelFailure::LicenseNotCleared {
            model_id: "m".into(),
            which: "weights",
            what_to_check: "核对发布页".into(),
        };
        let state = ModelRuntimeState::from_failure(failure);
        let status = status_from_state("m", &state, 3, "cpu-tract");
        assert_eq!(status.state, "licenseBlocked");
        assert!(!status.loaded);
        assert_eq!(status.elapsed_ms, 3);
        let value = serde_json::to_value(&status).unwrap();
        assert_eq!(value["failure"]["kind"], "licenseBlocked");
        assert_eq!(value["failure"]["modelId"], "m");
        // 状态里携带的失败态必须能换回同一个错误码（前端据此分支）。
        // `ModelFailure` 是**单向视图**（只 Serialize），所以这里从 state 里取原值。
        let carried = state.failure().expect("失败态必须保留在状态里");
        assert_eq!(
            InferenceError::from(carried.clone()).code(),
            "E_MODEL_LICENSE"
        );
    }

    #[test]
    fn inference_error_serializes_to_the_four_field_contract() {
        let error = InferenceError::ModelCorrupt {
            model_id: "face-detect-yunet-2023mar".into(),
            detail: "SHA256 与登记值不符".into(),
        };
        let value = serde_json::to_value(&error).unwrap();
        let object = value.as_object().expect("必须是对象而不是字符串");
        assert_eq!(object.len(), 4, "只暴露 4 个字段：{value}");
        assert_eq!(value["code"], "E_MODEL_CORRUPT");
        assert_eq!(value["modelId"], "face-detect-yunet-2023mar");
        assert!(value["message"].as_str().unwrap().contains("损坏"));
        assert!(value["detail"].as_str().unwrap().contains("SHA256"));
    }

    /// 命令层真正发给前端的**快照**形状：字段名与层级一旦漂移，
    /// `ModelHubPanel` 这类消费方会静默读到 `undefined`。
    ///
    /// 说明一处与任务描述不一致的地方：前端契约里**没有** `blocking` 字段。
    /// `blocking` 是 `ModelHealth`（准入链内部结果）的概念，序列化给前端的是
    /// `failure`（顺序上第一个阻塞项）+ `findings`（全量）。这样前端只需读
    /// `state` 做分支、读 `failure` 做说明、读 `findings` 做诊断，不必理解"判定顺序"。
    #[test]
    fn registry_snapshot_serializes_with_the_frontend_key_set() {
        use crate::ai_engine::model_registry::{locate, Registry};

        let registry = Registry::new(None);
        let mut dirs = locate::dev_base_dirs();
        dirs.push(PathBuf::from("/definitely/not/here"));
        let value = serde_json::to_value(registry.snapshot(&dirs)).unwrap();

        // 顶层。
        for key in ["schemaVersion", "generatedAtUnix", "models", "summary"] {
            assert!(value.get(key).is_some(), "快照缺少 `{key}`");
        }
        for key in [
            "total",
            "official",
            "ready",
            "verified",
            "blocked",
            "userImported",
        ] {
            assert!(value["summary"].get(key).is_some(), "summary 缺少 `{key}`");
        }

        // 单条模型。
        let model = &value["models"][0];
        for key in [
            "id",
            "displayName",
            "version",
            "task",
            "origin",
            "official",
            "format",
            "file",
            "sizeBytes",
            "sizeOnDiskBytes",
            "sha256",
            "input",
            "outputs",
            "license",
            "state",
            "loaded",
            "path",
            "failure",
            "findings",
            "checkedAtUnix",
        ] {
            assert!(model.get(key).is_some(), "模型条目缺少 `{key}`");
        }
        // 前端没有 `blocking`；被暴露的是 `failure` + `findings`。
        assert!(
            model.get("blocking").is_none(),
            "`blocking` 是内部概念，不应出现在前端契约里"
        );
        assert!(model["findings"].is_array());
        assert_eq!(model["sha256"], crate::ai_engine::model_registry::builtin::YUNET.sha256);
        assert_eq!(model["official"], true);

        // 许可视图：四字段独立 + 未核实字段单列。
        let license = &model["license"];
        for key in [
            "state",
            "spdx",
            "weights",
            "commercialUse",
            "redistribution",
            "trainingData",
            "unverifiedFields",
            "evidence",
            "note",
        ] {
            assert!(license.get(key).is_some(), "许可视图缺少 `{key}`");
        }
        assert_eq!(license["unverifiedFields"], serde_json::json!(["trainingData"]));

        // 结构化输入契约（不是自由字符串）。
        for key in ["dtype", "layout", "shape", "valueRange", "channelOrder"] {
            assert!(model["input"].get(key).is_some(), "input 缺少 `{key}`");
        }
    }

    /// `failure_of()` 的映射是命令层唯一的"错误 → 失败态"翻译点。
    /// 注意 `ModelCorrupt` 分支目前把 `actual_sha` 填成 `None`（R12 待修，
    /// 见 `docs/R11_ROUND_REPORT.md` 已知问题），本测试**如实记录现状**而不是背书。
    #[test]
    fn failure_of_maps_every_code_to_a_matching_kind() {
        let spec = &crate::ai_engine::model_registry::builtin::YUNET;
        let cases = [
            (
                InferenceError::LicenseNotCleared {
                    model_id: spec.id.into(),
                    which: "weights",
                    detail: "未核实".into(),
                },
                "licenseBlocked",
            ),
            (
                InferenceError::ModelMissing {
                    model_id: spec.id.into(),
                    searched: vec!["/a".into()],
                },
                "missing",
            ),
            (
                InferenceError::ModelCorrupt {
                    model_id: spec.id.into(),
                    detail: "sha".into(),
                },
                "corrupt",
            ),
            (
                InferenceError::ModelVersionMismatch {
                    model_id: spec.id.into(),
                    expected: "1x3x640x640".into(),
                    actual: "1x3x320x320".into(),
                },
                "versionMismatch",
            ),
            (
                InferenceError::ModelLoadFailed {
                    model_id: spec.id.into(),
                    detail: "parse".into(),
                },
                "loadFailed",
            ),
        ];
        for (error, expected) in cases {
            let failure = failure_of(spec, &error);
            assert_eq!(failure.kind(), expected, "错误码 {} 映射错了", error.code());
            assert_eq!(failure.model_id(), spec.id);
        }
        // 现状记录：corrupt 分支没有回填 actual_sha。
        let failure = failure_of(
            spec,
            &InferenceError::ModelCorrupt {
                model_id: spec.id.into(),
                detail: "sha".into(),
            },
        );
        assert!(failure.actual().is_none(), "若已回填 actual_sha 请更新本条与报告");
    }
}
