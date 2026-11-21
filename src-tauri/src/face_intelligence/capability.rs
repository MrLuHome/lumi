//! 能力声明与证据等级。
//!
//! 规则：能力只有在**实测通过**时才是 `Verified`；供应商文档或模型卡写的
//! 只能记为 `Declared`；没有证据一律 `Unverified`；模型确实没有输出则
//! `Unsupported`。UI 与 LEVEL 3 门控只承认 `Verified`。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvidenceLevel {
    /// 本机真实照片实测并记录阈值。
    Verified,
    /// 文档/模型卡声明，尚未实测。
    Declared,
    /// 没有证据。
    Unverified,
    /// 明确不支持。
    Unsupported,
}

impl EvidenceLevel {
    pub fn is_verified(self) -> bool {
        self == EvidenceLevel::Verified
    }

    pub fn label(self) -> &'static str {
        match self {
            EvidenceLevel::Verified => "已实测",
            EvidenceLevel::Declared => "仅声明",
            EvidenceLevel::Unverified => "未验证",
            EvidenceLevel::Unsupported => "不支持",
        }
    }
}

/// 统一能力位。来源于实际模型输出与测试结果，而不是“模型名字听起来高级”。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCapabilities {
    pub face_detection: bool,
    pub sparse_landmarks: bool,
    pub dense_landmarks: bool,
    pub eye_landmarks: bool,
    pub eyelid_landmarks: bool,
    pub nose_landmarks: bool,
    pub lip_landmarks: bool,
    pub face_mesh: bool,
    pub face_parsing: bool,
    pub skin_segmentation: bool,
    pub hair_segmentation: bool,
    pub face_matting: bool,
    pub multi_face: bool,
    pub offline_inference: bool,
    pub static_image: bool,
    pub max_input_resolution: Option<(u32, u32)>,
}

impl Default for ProviderCapabilities {
    fn default() -> Self {
        Self {
            face_detection: false,
            sparse_landmarks: false,
            dense_landmarks: false,
            eye_landmarks: false,
            eyelid_landmarks: false,
            nose_landmarks: false,
            lip_landmarks: false,
            face_mesh: false,
            face_parsing: false,
            skin_segmentation: false,
            hair_segmentation: false,
            face_matting: false,
            multi_face: false,
            offline_inference: true,
            static_image: true,
            max_input_resolution: None,
        }
    }
}

impl ProviderCapabilities {
    /// 只保留达到要求证据等级的能力。`declared` 不能直接打开 LEVEL 3。
    pub fn verified_only(&self, evidence: &[CapabilityEvidence]) -> Self {
        let allowed = |name: &str| {
            evidence
                .iter()
                .any(|item| item.capability == name && item.level == EvidenceLevel::Verified)
        };
        let flag = |name: &str, value: bool| if allowed(name) { value } else { false };
        Self {
            face_detection: flag("face_detection", self.face_detection),
            sparse_landmarks: flag("sparse_landmarks", self.sparse_landmarks),
            dense_landmarks: flag("dense_landmarks", self.dense_landmarks),
            eye_landmarks: flag("eye_landmarks", self.eye_landmarks),
            eyelid_landmarks: flag("eyelid_landmarks", self.eyelid_landmarks),
            nose_landmarks: flag("nose_landmarks", self.nose_landmarks),
            lip_landmarks: flag("lip_landmarks", self.lip_landmarks),
            face_mesh: flag("face_mesh", self.face_mesh),
            face_parsing: flag("face_parsing", self.face_parsing),
            skin_segmentation: flag("skin_segmentation", self.skin_segmentation),
            hair_segmentation: flag("hair_segmentation", self.hair_segmentation),
            face_matting: flag("face_matting", self.face_matting),
            multi_face: flag("multi_face", self.multi_face),
            offline_inference: self.offline_inference,
            static_image: self.static_image,
            max_input_resolution: self.max_input_resolution,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityEvidence {
    pub capability: String,
    pub level: EvidenceLevel,
    pub note: String,
    /// 实测数值（例如“3 次重复最大偏差 0.4% 眼距”）。
    #[serde(default)]
    pub measurement: Option<String>,
}

impl CapabilityEvidence {
    pub fn new(
        capability: impl Into<String>,
        level: EvidenceLevel,
        note: impl Into<String>,
    ) -> Self {
        Self {
            capability: capability.into(),
            level,
            note: note.into(),
            measurement: None,
        }
    }

    pub fn with_measurement(mut self, measurement: impl Into<String>) -> Self {
        self.measurement = Some(measurement.into());
        self
    }
}

/// 许可证状态。用户自备模型永远不能因为“放在 Model Hub 里”就变成官方商用模型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LicenseState {
    /// 已核实允许商业使用。
    VerifiedCommercial,
    /// 已核实仅允许个人使用。
    VerifiedPersonal,
    /// 操作系统自带框架：不随包分发权重，商用需遵守平台条款。
    SystemFramework,
    /// 仅限研究用途。
    ResearchOnly,
    /// 用户自备，许可证未验证。
    UserUnverified,
    /// 明确禁止使用。
    Prohibited,
    /// 与 LUMI 发行方式不兼容。
    Incompatible,
    /// 已从 Model Hub 移除。
    Removed,
}

impl LicenseState {
    pub fn label(self) -> &'static str {
        match self {
            LicenseState::VerifiedCommercial => "已验证·允许商用",
            LicenseState::VerifiedPersonal => "已验证·仅个人",
            LicenseState::SystemFramework => "系统框架·需遵守平台条款",
            LicenseState::ResearchOnly => "仅研究用途",
            LicenseState::UserUnverified => "用户自备·未验证",
            LicenseState::Prohibited => "禁止使用",
            LicenseState::Incompatible => "许可不兼容",
            LicenseState::Removed => "已移除",
        }
    }

    /// 是否允许作为正式商业功能默认模型。
    pub fn can_be_official_commercial(self) -> bool {
        self == LicenseState::VerifiedCommercial
    }

    pub fn is_usable_locally(self) -> bool {
        matches!(
            self,
            LicenseState::VerifiedCommercial
                | LicenseState::VerifiedPersonal
                | LicenseState::SystemFramework
                | LicenseState::ResearchOnly
                | LicenseState::UserUnverified
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseInfo {
    pub name: String,
    pub url: Option<String>,
    pub state: LicenseState,
    pub note: String,
}

impl Default for LicenseInfo {
    fn default() -> Self {
        Self {
            name: "UNKNOWN".into(),
            url: None,
            state: LicenseState::UserUnverified,
            note: "未验证许可证：不得随 LUMI 安装包分发，也不得标记为官方商用模型".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderKind {
    /// LUMI 随包内置模型。
    Builtin,
    /// 操作系统框架（例如 Apple Vision）。
    System,
    /// 用户自备 ONNX 模型。
    UserModel,
    /// 商业 SDK（本轮不进入发行包）。
    CommercialPoc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderState {
    /// 可用且已选中。
    Ready,
    /// 可用但未选中。
    Idle,
    /// 正在分析。
    Analyzing,
    /// 模型缺失。
    Missing,
    /// 平台不支持。
    Unsupported,
    /// 初始化失败。
    Failed,
    /// 质量不足。
    LowQuality,
    /// 许可证未验证。
    LicenseUnverified,
}

/// 前端 Provider 列表的一行。`verified_capabilities` 是门控真正使用的部分。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDescriptor {
    pub id: String,
    pub display_name: String,
    pub version: String,
    pub kind: ProviderKind,
    pub state: ProviderState,
    pub available: bool,
    pub enabled: bool,
    pub license: LicenseInfo,
    pub capabilities: ProviderCapabilities,
    pub verified_capabilities: ProviderCapabilities,
    pub evidence: Vec<CapabilityEvidence>,
    pub topology_id: Option<String>,
    pub note: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_capability_does_not_unlock_verified_gate() {
        let caps = ProviderCapabilities {
            dense_landmarks: true,
            eye_landmarks: true,
            ..Default::default()
        };
        let evidence = vec![
            CapabilityEvidence::new("dense_landmarks", EvidenceLevel::Declared, "模型卡"),
            CapabilityEvidence::new("eye_landmarks", EvidenceLevel::Verified, "实测"),
        ];
        let verified = caps.verified_only(&evidence);
        assert!(!verified.dense_landmarks);
        assert!(verified.eye_landmarks);
    }

    #[test]
    fn unsupported_capability_stays_false() {
        let caps = ProviderCapabilities {
            face_parsing: true,
            ..Default::default()
        };
        let evidence = vec![CapabilityEvidence::new(
            "face_parsing",
            EvidenceLevel::Unsupported,
            "没有输出",
        )];
        assert!(!caps.verified_only(&evidence).face_parsing);
    }

    #[test]
    fn user_unverified_cannot_be_official_commercial() {
        assert!(!LicenseState::UserUnverified.can_be_official_commercial());
        assert!(LicenseState::UserUnverified.is_usable_locally());
        assert!(!LicenseState::Prohibited.is_usable_locally());
        assert!(LicenseState::VerifiedCommercial.can_be_official_commercial());
    }
}
