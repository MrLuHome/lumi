//! AI 分割 Provider 的禁用实现。
//!
//! 保留完整 trait 形状，未来替换为已验证权重时只需要换实现，
//! 管线与前端能力查询不需要重写。

use crate::error::{AppError, AppResult};
use crate::image_engine::buffer::Image16;

use super::{MaskProvider, SubjectMask};

#[derive(Debug)]
pub struct AiMaskProvider {
    reason: String,
}

impl Default for AiMaskProvider {
    fn default() -> Self {
        Self {
            reason: Self::default_reason(),
        }
    }
}

impl AiMaskProvider {
    pub fn with_reason(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }

    fn default_reason() -> String {
        "AI 人物分割未启用：没有经过再分发审查的权重，且当前 ONNX 运行时未验证对应模型。请使用手动蒙版。".into()
    }
}

impl MaskProvider for AiMaskProvider {
    fn id(&self) -> &str {
        "ai-subject-segmentation"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn license(&self) -> &str {
        "未启用（无已授权权重）"
    }

    fn available(&self) -> bool {
        false
    }

    fn unavailable_reason(&self) -> &str {
        if self.reason.is_empty() {
            return "AI 人物分割未启用";
        }
        &self.reason
    }

    fn segment(&self, _image: &Image16) -> AppResult<SubjectMask> {
        let reason = if self.reason.is_empty() {
            Self::default_reason()
        } else {
            self.reason.clone()
        };
        Err(AppError::Unsupported(reason))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_never_claims_availability() {
        let provider = AiMaskProvider::default();
        assert!(!provider.available());
        assert!(!provider.unavailable_reason().is_empty());
        let image: Image16 = Image16::from_pixel(4, 4, image::Rgba([1, 2, 3, 65535]));
        assert!(provider.segment(&image).is_err());
    }
}
