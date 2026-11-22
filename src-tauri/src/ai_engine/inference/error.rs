//! 推理层结构化错误。
//!
//! 与 [`crate::error::AppError`] 的关键差异：`AppError` 序列化为**纯字符串**，
//! 前端只能拿来显示，无法 `switch(code)` 做分支（缺失 → 引导重装，
//! 损坏 → 引导重新导入，未授权 → 灰按钮 + 说明）。
//! 因此新的模型 / 推理能力使用本类型，序列化成
//! `{code, message, modelId, detail}` 对象。
//!
//! **不改 `AppError` 的 `Serialize`**：`src/lib/backend.ts` 里 10+ 处
//! `catch` 直接把错误当字符串显示（`BeautyPanel.tsx`、`FacePanel.tsx` 等），
//! 改成对象会让既有面板全部显示 `[object Object]`。

use serde::ser::{Serialize, SerializeStruct, Serializer};

use crate::error::AppError;

/// 11 个稳定错误码。改动必须同步 `src/types.ts` 的 `ModelErrorCode` 联合类型。
pub const ERROR_CODES: [&str; 11] = [
    "E_MODEL_LICENSE",
    "E_MODEL_MISSING",
    "E_MODEL_CORRUPT",
    "E_MODEL_VERSION",
    "E_MODEL_LOAD",
    "E_MODEL_CONTRACT",
    "E_INPUT",
    "E_INFER",
    "E_TIMEOUT",
    "E_CANCELLED",
    "E_BACKEND",
];

#[derive(Debug, thiserror::Error)]
pub enum InferenceError {
    #[error("模型未通过许可审核：{model_id}")]
    LicenseNotCleared {
        model_id: String,
        which: &'static str,
        detail: String,
    },

    #[error("模型文件缺失：{model_id}")]
    ModelMissing {
        model_id: String,
        searched: Vec<String>,
    },

    #[error("模型文件损坏：{model_id}")]
    ModelCorrupt {
        model_id: String,
        detail: String,
    },

    #[error("模型版本与预期契约不符：{model_id}")]
    ModelVersionMismatch {
        model_id: String,
        expected: String,
        actual: String,
    },

    #[error("模型加载失败：{model_id}")]
    ModelLoadFailed { model_id: String, detail: String },

    #[error("模型 IO 契约不符：{detail}")]
    ContractMismatch { model_id: String, detail: String },

    #[error("输入无效：{0}")]
    InvalidInput(String),

    #[error("推理失败：{0}")]
    Runtime(String),

    #[error("推理超时（{ms} ms）")]
    Timeout { ms: u64 },

    #[error("已取消")]
    Cancelled,

    #[error("后端不支持：{0}")]
    BackendUnsupported(String),
}

impl InferenceError {
    /// 稳定错误码。
    pub fn code(&self) -> &'static str {
        match self {
            InferenceError::LicenseNotCleared { .. } => "E_MODEL_LICENSE",
            InferenceError::ModelMissing { .. } => "E_MODEL_MISSING",
            InferenceError::ModelCorrupt { .. } => "E_MODEL_CORRUPT",
            InferenceError::ModelVersionMismatch { .. } => "E_MODEL_VERSION",
            InferenceError::ModelLoadFailed { .. } => "E_MODEL_LOAD",
            InferenceError::ContractMismatch { .. } => "E_MODEL_CONTRACT",
            InferenceError::InvalidInput(_) => "E_INPUT",
            InferenceError::Runtime(_) => "E_INFER",
            InferenceError::Timeout { .. } => "E_TIMEOUT",
            InferenceError::Cancelled => "E_CANCELLED",
            InferenceError::BackendUnsupported(_) => "E_BACKEND",
        }
    }

    /// 关联的模型 ID（输入类错误没有模型上下文）。
    pub fn model_id(&self) -> Option<&str> {
        match self {
            InferenceError::LicenseNotCleared { model_id, .. }
            | InferenceError::ModelMissing { model_id, .. }
            | InferenceError::ModelCorrupt { model_id, .. }
            | InferenceError::ModelVersionMismatch { model_id, .. }
            | InferenceError::ModelLoadFailed { model_id, .. } => Some(model_id.as_str()),
            InferenceError::ContractMismatch { model_id, .. } => {
                if model_id.is_empty() {
                    None
                } else {
                    Some(model_id.as_str())
                }
            }
            InferenceError::InvalidInput(_)
            | InferenceError::Runtime(_)
            | InferenceError::Timeout { .. }
            | InferenceError::Cancelled
            | InferenceError::BackendUnsupported(_) => None,
        }
    }

    /// 供前端展示的补充细节（与 `message` 分开，避免拼成一整句）。
    pub fn detail(&self) -> Option<String> {
        match self {
            InferenceError::LicenseNotCleared {
                which, detail, ..
            } => Some(format!("{which}：{detail}")),
            InferenceError::ModelMissing { searched, .. } => {
                if searched.is_empty() {
                    None
                } else {
                    Some(format!("已搜索：{}", searched.join("；")))
                }
            }
            InferenceError::ModelCorrupt { detail, .. }
            | InferenceError::ModelLoadFailed { detail, .. }
            | InferenceError::ContractMismatch { detail, .. } => Some(detail.clone()),
            InferenceError::ModelVersionMismatch {
                expected, actual, ..
            } => Some(format!("期望 {expected}，实际 {actual}")),
            InferenceError::InvalidInput(message) | InferenceError::Runtime(message) => {
                Some(message.clone())
            }
            InferenceError::BackendUnsupported(reason) => Some(reason.clone()),
            InferenceError::Timeout { ms } => Some(format!("超过 {ms} ms")),
            InferenceError::Cancelled => None,
        }
    }

    /// 是否应该静默处理（用户主动取消，不弹 toast）。
    pub fn is_silent(&self) -> bool {
        matches!(self, InferenceError::Cancelled)
    }
}

/// 序列化为对象而不是字符串。
impl Serialize for InferenceError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("InferenceError", 4)?;
        state.serialize_field("code", self.code())?;
        state.serialize_field("message", &self.to_string())?;
        state.serialize_field("modelId", &self.model_id())?;
        state.serialize_field("detail", &self.detail())?;
        state.end()
    }
}

/// 兼容既有命令层：需要时降级为 `AppError`（继续序列化成字符串）。
impl From<InferenceError> for AppError {
    fn from(error: InferenceError) -> Self {
        match error {
            InferenceError::Cancelled => AppError::Cancelled,
            InferenceError::InvalidInput(message) => AppError::InvalidInput(message),
            // 模型类错误统一走 Task，避免 `AppError::Unsupported` 的
            // “不支持的图片格式：”前缀误导用户去换图片（R7）。
            other => AppError::Task(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<InferenceError> {
        vec![
            InferenceError::LicenseNotCleared {
                model_id: "m".into(),
                which: "weights",
                detail: "未核实".into(),
            },
            InferenceError::ModelMissing {
                model_id: "m".into(),
                searched: vec!["/a".into()],
            },
            InferenceError::ModelCorrupt {
                model_id: "m".into(),
                detail: "SHA 不符".into(),
            },
            InferenceError::ModelVersionMismatch {
                model_id: "m".into(),
                expected: "1x3x640x640".into(),
                actual: "1x3x320x320".into(),
            },
            InferenceError::ModelLoadFailed {
                model_id: "m".into(),
                detail: "parse".into(),
            },
            InferenceError::ContractMismatch {
                model_id: "m".into(),
                detail: "shape".into(),
            },
            InferenceError::InvalidInput("empty".into()),
            InferenceError::Runtime("boom".into()),
            InferenceError::Timeout { ms: 1000 },
            InferenceError::Cancelled,
            InferenceError::BackendUnsupported("gpu".into()),
        ]
    }

    #[test]
    fn every_variant_has_a_stable_code() {
        let codes: Vec<&str> = sample().iter().map(|error| error.code()).collect();
        assert_eq!(codes, ERROR_CODES.to_vec());
        assert_eq!(codes.len(), 11);
    }

    #[test]
    fn serialize_yields_object_with_four_fields() {
        let error = InferenceError::ModelCorrupt {
            model_id: "face-detect-yunet-2023mar".into(),
            detail: "SHA256 不符".into(),
        };
        let value = serde_json::to_value(&error).unwrap();
        assert_eq!(value["code"], "E_MODEL_CORRUPT");
        assert_eq!(value["modelId"], "face-detect-yunet-2023mar");
        assert_eq!(value["message"], "模型文件损坏：face-detect-yunet-2023mar");
        assert!(value["detail"].as_str().unwrap().contains("SHA256"));
    }

    #[test]
    fn cancelled_is_silent_and_has_no_model_id() {
        let error = InferenceError::Cancelled;
        assert!(error.is_silent());
        assert!(error.model_id().is_none());
        let value = serde_json::to_value(&error).unwrap();
        assert_eq!(value["code"], "E_CANCELLED");
        assert!(value["modelId"].is_null());
        assert!(value["detail"].is_null());
    }

    #[test]
    fn converts_to_app_error_without_image_prefix() {
        let error = InferenceError::ModelMissing {
            model_id: "m".into(),
            searched: Vec::new(),
        };
        let app: AppError = error.into();
        assert!(!app.to_string().contains("不支持的图片格式"));
        assert_eq!(
            AppError::from(InferenceError::Cancelled).to_string(),
            "导出已取消"
        );
    }
}
