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
