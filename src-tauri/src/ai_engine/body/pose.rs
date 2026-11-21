//! 身体姿态 Provider。
//!
//! 与分割相同：没有经过许可证与运行时验证的模型，就不启用。
//! 身材调整不依赖姿态点，使用 Alpha 轮廓 + 用户锚点，能力查询如实说明。

use serde::Serialize;

use crate::ai_engine::segmentation::model::{ModelStatus, CATALOG};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PoseCapability {
    pub pose_available: bool,
    pub provider: String,
    pub license: String,
    pub reason: String,
    pub note: String,
}

pub trait PoseProvider {
    fn id(&self) -> &str;
    fn available(&self) -> bool;
    fn reason(&self) -> &str;
}

#[derive(Debug, Default)]
pub struct DisabledPoseProvider;

impl PoseProvider for DisabledPoseProvider {
    fn id(&self) -> &str {
        "mediapipe-pose"
    }

    fn available(&self) -> bool {
        false
    }

    fn reason(&self) -> &str {
        "人体姿态模型未启用：没有可再分发的权重，桌面端运行时未验证。身材调整使用蒙版轮廓 + 用户锚点，属于轮廓级近似。"
    }
}

pub fn capability() -> PoseCapability {
    let provider = DisabledPoseProvider;
    let entry = CATALOG.iter().find(|entry| entry.id == provider.id());
    let license = entry
        .map(|entry| entry.license.to_string())
        .unwrap_or_else(|| "Apache-2.0（候选）".into());
    let verified = entry.is_some_and(|entry| entry.status == ModelStatus::Verified);
    PoseCapability {
        pose_available: provider.available() && verified,
        provider: provider.id().into(),
        license,
        reason: provider.reason().into(),
        note: "身材参数只改变人物蒙版内的轮廓，不移动背景，也不修改人脸。".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pose_is_reported_unavailable_with_reason() {
        let capability = capability();
        assert!(!capability.pose_available);
        assert!(!capability.reason.is_empty());
        assert!(capability.note.contains("轮廓"));
    }
}
