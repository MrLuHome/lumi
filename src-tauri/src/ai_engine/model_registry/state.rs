//! 模型运行时状态机与失败态。
//!
//! 判定顺序**固定**（注册表准入链）：
//!
//! ```text
//! ① 许可证 → ② 文件存在 → ③ 格式 → ④ 契约（输入输出） → ⑤ SHA256 → ⑥ 解析
//! ```
//!
//! 三条理由（写在代码里而不是文档里，避免被后来的改动悄悄推翻）：
//! 1. **法务优先**：未授权权重一个字节都不该被读进进程，连“读文件算哈希”都不做；
//! 2. **成本**：许可判断是纯内存比较（µs），文件校验是 IO（ms 级），失败快的先做；
//! 3. **可解释性**：用户看到“这个模型没通过许可审核”比“文件缺失”重要 ——
//!    后者可能只是打包漏了，前者是红线。
//!
//! 同时提供 `blocking`（顺序上第一个）与 `findings`（全量）：
//! 顺序判定会让 `Missing` 掩盖 `License`，运维排查时需要一次看到全部问题。

use serde::ser::{Serialize, SerializeStruct, Serializer};

use crate::ai_engine::inference::InferenceError;

/// 失败态。`which` / `what_to_check` 一律是可展示的中文短语。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelFailure {
    /// 许可未通过 —— 唯一一个**先于任何文件读取**判定的失败态。
    LicenseNotCleared {
        model_id: String,
        /// 具体是哪个字段没过：`weights` / `commercialUse` / `redistribution`。
        which: &'static str,
        what_to_check: String,
    },
    /// 所有候选路径都不存在。
    Missing {
        model_id: String,
        searched: Vec<String>,
    },
    /// 存在但不可信：体积不符 / 非 ONNX 头 / SHA 不符。
    Corrupt {
        model_id: String,
        detail: String,
        expected_sha: String,
        actual_sha: Option<String>,
    },
    /// 结构可读但与登记的输入输出契约不符（形状 / head 数 / dtype）。
    VersionMismatch {
        model_id: String,
        expected: String,
        actual: String,
    },
    /// 已通过前置检查但 tract 解析 / 优化 / 编译失败。
    LoadFailed { model_id: String, detail: String },
}

impl ModelFailure {
    /// 与前端 `ModelState` 非正常值一一对应的短标签。
    pub fn kind(&self) -> &'static str {
        match self {
            ModelFailure::LicenseNotCleared { .. } => "licenseBlocked",
            ModelFailure::Missing { .. } => "missing",
            ModelFailure::Corrupt { .. } => "corrupt",
            ModelFailure::VersionMismatch { .. } => "versionMismatch",
            ModelFailure::LoadFailed { .. } => "loadFailed",
        }
    }

    pub fn model_id(&self) -> &str {
        match self {
            ModelFailure::LicenseNotCleared { model_id, .. }
            | ModelFailure::Missing { model_id, .. }
            | ModelFailure::Corrupt { model_id, .. }
            | ModelFailure::VersionMismatch { model_id, .. }
            | ModelFailure::LoadFailed { model_id, .. } => model_id.as_str(),
        }
    }

    /// 一句话说明（用户可见）。
    pub fn message(&self) -> String {
        match self {
            ModelFailure::LicenseNotCleared { which, .. } => {
                format!("模型未通过许可审核（{which}）")
            }
            ModelFailure::Missing { .. } => "模型文件缺失".into(),
            ModelFailure::Corrupt { detail, .. } => format!("模型文件已损坏：{detail}"),
            ModelFailure::VersionMismatch { .. } => "模型版本与登记的契约不符".into(),
            ModelFailure::LoadFailed { detail, .. } => format!("模型无法加载：{detail}"),
        }
    }

    pub fn detail(&self) -> Option<&str> {
        match self {
            ModelFailure::LicenseNotCleared { what_to_check, .. } => Some(what_to_check),
            ModelFailure::Corrupt { detail, .. } | ModelFailure::LoadFailed { detail, .. } => {
                Some(detail)
            }
            ModelFailure::Missing { .. } | ModelFailure::VersionMismatch { .. } => None,
        }
    }

    pub fn searched(&self) -> Option<&[String]> {
        match self {
            ModelFailure::Missing { searched, .. } => Some(searched),
            _ => None,
        }
    }

    pub fn expected(&self) -> Option<&str> {
        match self {
            ModelFailure::Corrupt { expected_sha, .. } => Some(expected_sha),
            ModelFailure::VersionMismatch { expected, .. } => Some(expected),
            _ => None,
        }
    }

    pub fn actual(&self) -> Option<&str> {
        match self {
            ModelFailure::Corrupt { actual_sha, .. } => actual_sha.as_deref(),
            ModelFailure::VersionMismatch { actual, .. } => Some(actual),
            _ => None,
        }
    }

    /// 是否属于「不可通过重试修复」的失败（许可类）。
    pub fn is_permanent(&self) -> bool {
        matches!(self, ModelFailure::LicenseNotCleared { .. })
    }
}

impl Serialize for ModelFailure {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("ModelFailure", 7)?;
        state.serialize_field("kind", self.kind())?;
        state.serialize_field("modelId", self.model_id())?;
        state.serialize_field("message", &self.message())?;
        if let Some(detail) = self.detail() {
            state.serialize_field("detail", detail)?;
        }
        if let Some(searched) = self.searched() {
            state.serialize_field("searched", searched)?;
        }
        if let Some(expected) = self.expected() {
            state.serialize_field("expected", expected)?;
        }
        if let Some(actual) = self.actual() {
            state.serialize_field("actual", actual)?;
        }
        state.end()
    }
}

impl From<ModelFailure> for InferenceError {
    fn from(failure: ModelFailure) -> Self {
        match failure {
            ModelFailure::LicenseNotCleared {
                model_id,
                which,
                what_to_check,
            } => InferenceError::LicenseNotCleared {
                model_id,
                which,
                detail: what_to_check,
            },
            ModelFailure::Missing { model_id, searched } => {
                InferenceError::ModelMissing { model_id, searched }
            }
            ModelFailure::Corrupt {
                model_id, detail, ..
            } => InferenceError::ModelCorrupt { model_id, detail },
            ModelFailure::VersionMismatch {
                model_id,
                expected,
                actual,
            } => InferenceError::ModelVersionMismatch {
                model_id,
                expected,
                actual,
            },
            ModelFailure::LoadFailed { model_id, detail } => {
                InferenceError::ModelLoadFailed { model_id, detail }
            }
        }
    }
}

/// 模型运行时状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelRuntimeState {
    LicenseBlocked(ModelFailure),
    Missing(ModelFailure),
    Corrupt(ModelFailure),
    VersionMismatch(ModelFailure),
    LoadFailed(ModelFailure),
    /// 通过闸门与校验，但尚未加载进内存。
    Verified { path: std::path::PathBuf },
    /// 已加载进内存（plan 已缓存）。
    Ready { path: std::path::PathBuf },
}

impl ModelRuntimeState {
    /// 由失败态构造对应状态。
    pub fn from_failure(failure: ModelFailure) -> Self {
        match failure.kind() {
            "licenseBlocked" => ModelRuntimeState::LicenseBlocked(failure),
            "missing" => ModelRuntimeState::Missing(failure),
            "corrupt" => ModelRuntimeState::Corrupt(failure),
            "versionMismatch" => ModelRuntimeState::VersionMismatch(failure),
            _ => ModelRuntimeState::LoadFailed(failure),
        }
    }

    /// 前端 `ModelState` 的字符串值。
    pub fn as_str(&self) -> &'static str {
        match self {
            ModelRuntimeState::LicenseBlocked(_) => "licenseBlocked",
            ModelRuntimeState::Missing(_) => "missing",
            ModelRuntimeState::Corrupt(_) => "corrupt",
            ModelRuntimeState::VersionMismatch(_) => "versionMismatch",
            ModelRuntimeState::LoadFailed(_) => "loadFailed",
            ModelRuntimeState::Verified { .. } => "verified",
            ModelRuntimeState::Ready { .. } => "ready",
        }
    }

    pub fn is_loadable(&self) -> bool {
        matches!(
            self,
            ModelRuntimeState::Verified { .. } | ModelRuntimeState::Ready { .. }
        )
    }

    pub fn failure(&self) -> Option<&ModelFailure> {
        match self {
            ModelRuntimeState::LicenseBlocked(failure)
            | ModelRuntimeState::Missing(failure)
            | ModelRuntimeState::Corrupt(failure)
            | ModelRuntimeState::VersionMismatch(failure)
            | ModelRuntimeState::LoadFailed(failure) => Some(failure),
            ModelRuntimeState::Verified { .. } | ModelRuntimeState::Ready { .. } => None,
        }
    }

    pub fn path(&self) -> Option<&std::path::Path> {
        match self {
            ModelRuntimeState::Verified { path } | ModelRuntimeState::Ready { path } => Some(path),
            _ => None,
        }
    }

    pub fn is_loaded(&self) -> bool {
        matches!(self, ModelRuntimeState::Ready { .. })
    }
}

/// 一次准入检查的结果：既要「顺序」也要「全量」。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelHealth {
    /// 判定顺序上的第一个阻塞项，决定最终状态。
    pub blocking: Option<ModelFailure>,
    /// 全部发现（含被 blocking 掩盖的），供前端展示与运维诊断。
    pub findings: Vec<ModelFailure>,
    /// 命中并校验通过的权重路径（未通过时为 `None`）。
    pub resolved_path: Option<std::path::PathBuf>,
}

impl ModelHealth {
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一个发现。第一个进入的发现即 `blocking`。
    pub fn push(&mut self, failure: ModelFailure) {
        if self.blocking.is_none() {
            self.blocking = Some(failure.clone());
        }
        self.findings.push(failure);
    }

    pub fn is_blocked(&self) -> bool {
        self.blocking.is_some()
    }

    /// 通过检查时的状态；有阻塞时返回失败态。
    pub fn state(&self) -> ModelRuntimeState {
        match (&self.blocking, &self.resolved_path) {
            (Some(failure), _) => ModelRuntimeState::from_failure(failure.clone()),
            (None, Some(path)) => ModelRuntimeState::Verified { path: path.clone() },
            // 理论上不可达：没有 blocking 就一定有 resolved_path。
            (None, None) => ModelRuntimeState::Missing(ModelFailure::Missing {
                model_id: String::new(),
                searched: Vec::new(),
            }),
        }
    }

    /// 转成结构化错误（供命令层返回 `{code,...}`）。
    pub fn error(&self) -> Option<InferenceError> {
        self.blocking.clone().map(InferenceError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn license_failure() -> ModelFailure {
        ModelFailure::LicenseNotCleared {
            model_id: "m".into(),
            which: "weights",
            what_to_check: "核对权重发布页".into(),
        }
    }

    fn missing_failure() -> ModelFailure {
        ModelFailure::Missing {
            model_id: "m".into(),
            searched: vec!["/a".into(), "/b".into()],
        }
    }

    #[test]
    fn blocking_is_the_first_finding_in_order() {
        let mut health = ModelHealth::new();
        health.push(license_failure());
        health.push(missing_failure());
        assert_eq!(health.findings.len(), 2, "findings 必须是全量");
        assert_eq!(
            health.blocking.as_ref().map(|failure| failure.kind()),
            Some("licenseBlocked")
        );
        assert_eq!(health.state().as_str(), "licenseBlocked");
    }

    #[test]
    fn state_maps_every_kind() {
        for (failure, expected) in [
            (license_failure(), "licenseBlocked"),
            (missing_failure(), "missing"),
            (
                ModelFailure::Corrupt {
                    model_id: "m".into(),
                    detail: "sha".into(),
                    expected_sha: "aa".into(),
                    actual_sha: Some("bb".into()),
                },
                "corrupt",
            ),
            (
                ModelFailure::VersionMismatch {
                    model_id: "m".into(),
                    expected: "1x3x640x640".into(),
                    actual: "1x3x320x320".into(),
                },
                "versionMismatch",
            ),
            (
                ModelFailure::LoadFailed {
                    model_id: "m".into(),
                    detail: "parse".into(),
                },
                "loadFailed",
            ),
        ] {
            assert_eq!(ModelRuntimeState::from_failure(failure).as_str(), expected);
        }
    }

    #[test]
    fn failure_serializes_with_kind_and_model_id() {
        let value = serde_json::to_value(missing_failure()).unwrap();
        assert_eq!(value["kind"], "missing");
        assert_eq!(value["modelId"], "m");
        assert_eq!(value["searched"][0], "/a");
        assert!(value.get("detail").is_none(), "缺失态不应有 detail 字段");
    }

    #[test]
    fn license_failure_is_permanent() {
        assert!(license_failure().is_permanent());
        assert!(!missing_failure().is_permanent());
    }

    #[test]
    fn converts_to_inference_error_with_matching_code() {
        assert_eq!(
            InferenceError::from(license_failure()).code(),
            "E_MODEL_LICENSE"
        );
        assert_eq!(
            InferenceError::from(missing_failure()).code(),
            "E_MODEL_MISSING"
        );
    }
}
