//! 用户模型清单（manifest）。
//!
//! Manifest 是模型输入/输出契约的唯一来源：LUMI 不猜测任意 ONNX 模型的后处理，
//! 不支持的契约在导入阶段明确拒绝。

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelTask {
    FaceLandmark,
}

impl ModelTask {
    pub fn label(self) -> &'static str {
        match self {
            ModelTask::FaceLandmark => "人脸关键点",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TensorLayout {
    /// NCHW，当前只接受该布局。
    Nchw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Normalization {
    /// 0..1
    ZeroToOne,
    /// -1..1
    MinusOneToOne,
    /// ImageNet mean/std
    Imagenet,
    /// 0..255
    Raw255,
}

impl Normalization {
    pub fn label(self) -> &'static str {
        match self {
            Normalization::ZeroToOne => "0..1",
            Normalization::MinusOneToOne => "-1..1",
            Normalization::Imagenet => "ImageNet 均值/方差",
            Normalization::Raw255 => "0..255",
        }
    }

    pub fn apply(self, value: f32) -> f32 {
        match self {
            Normalization::ZeroToOne => value,
            Normalization::MinusOneToOne => value * 2.0 - 1.0,
            Normalization::Imagenet => (value - 0.485) / 0.229,
            Normalization::Raw255 => value * 255.0,
        }
    }
}

/// 关键点输出张量布局。只支持静态、单批次、单输出的常见契约。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OutputLayout {
    /// `[1, N, 2]`
    N2,
    /// `[1, N, 3]`，第三通道为置信度
    N3,
    /// `[1, 2, N]`
    C2N,
    /// `[1, 3, N]`，第二通道为置信度
    C3N,
}

impl OutputLayout {
    pub fn has_confidence(self) -> bool {
        matches!(self, OutputLayout::N3 | OutputLayout::C3N)
    }

    pub fn label(self) -> &'static str {
        match self {
            OutputLayout::N2 => "[1,N,2]",
            OutputLayout::N3 => "[1,N,3]",
            OutputLayout::C2N => "[1,2,N]",
            OutputLayout::C3N => "[1,3,N]",
        }
    }

    /// 从静态输出形状推断布局；不接受动态维度。
    pub fn infer(shape: &[Option<i64>]) -> Result<Self, String> {
        if shape.len() != 3 {
            return Err(format!(
                "关键点输出必须是 3 维，实际 {} 维（{:?}）",
                shape.len(),
                shape
            ));
        }
        if shape[0] != Some(1) {
            return Err("关键点输出第一维必须固定为 1（单批次）".into());
        }
        let a = shape[1];
        let b = shape[2];
        match (a, b) {
            (Some(n), Some(2)) if (5..=2000).contains(&n) => Ok(OutputLayout::N2),
            (Some(n), Some(3)) if (5..=2000).contains(&n) => Ok(OutputLayout::N3),
            (Some(2), Some(n)) if (5..=2000).contains(&n) => Ok(OutputLayout::C2N),
            (Some(3), Some(n)) if (5..=2000).contains(&n) => Ok(OutputLayout::C3N),
            _ => Err(format!(
                "无法从输出形状 {:?} 推断关键点布局；需要 [1,N,2/3] 或 [1,2/3,N]，N∈5..2000",
                shape
            )),
        }
    }

    pub fn point_count(self, shape: &[Option<i64>]) -> Option<usize> {
        match (self, shape) {
            (OutputLayout::N2 | OutputLayout::N3, [Some(1), Some(n), _]) => Some(*n as usize),
            (OutputLayout::C2N | OutputLayout::C3N, [Some(1), _, Some(n)]) => Some(*n as usize),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OutputCoordinate {
    /// 相对裁剪区域 0..1。
    NormalizedCrop,
    /// 相对裁剪区域像素坐标。
    PixelCrop,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputSpec {
    pub width: u32,
    pub height: u32,
    pub layout: TensorLayout,
    pub normalization: Normalization,
    /// 裁剪框向外扩展比例，用于保留完整五官。
    pub crop_margin: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputSpec {
    pub layout: OutputLayout,
    pub coordinate: OutputCoordinate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseDeclaration {
    pub name: String,
    pub url: Option<String>,
    /// 用户声明；LUMI 不把它当作已验证事实。
    pub commercial_use_claimed: bool,
    pub redistribution_claimed: bool,
    pub note: String,
}

impl Default for LicenseDeclaration {
    fn default() -> Self {
        Self {
            name: "UNKNOWN".into(),
            url: None,
            commercial_use_claimed: false,
            redistribution_claimed: false,
            note: "用户自备模型；许可证状态未验证".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationSummary {
    pub ok: bool,
    pub sha256: String,
    pub file_size: u64,
    pub input_shape: Vec<Option<i64>>,
    pub output_shape: Vec<Option<i64>>,
    pub operator_count: usize,
    pub elapsed_ms: u64,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelManifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub task: ModelTask,
    /// 当前只支持有明确语义定义的 `face-68`。
    pub topology_id: String,
    pub input: InputSpec,
    pub output: OutputSpec,
    pub license: LicenseDeclaration,
    pub sha256: String,
    pub file_name: String,
    pub source_url: Option<String>,
    /// 导入时的原始路径，仅用于追溯，不参与推理。
    pub source_path: String,
    pub imported_at: String,
    pub validation: Option<ValidationSummary>,
}

impl ModelManifest {
    pub fn validate_schema(&self) -> Vec<String> {
        let mut issues = Vec::new();
        if self.schema_version != SCHEMA_VERSION {
            issues.push(format!(
                "manifest 版本 {} 不受支持（期望 {}）",
                self.schema_version, SCHEMA_VERSION
            ));
        }
        if self.id.is_empty() || self.id.len() > 64 {
            issues.push("模型 ID 长度必须在 1..64".into());
        }
        if !self
            .id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        {
            issues.push("模型 ID 只能包含字母、数字、-、_、.".into());
        }
        if self.topology_id != "face-68" {
            issues.push(format!("当前只支持拓扑 face-68，实际 {}", self.topology_id));
        }
        if self.input.width < 32
            || self.input.height < 32
            || self.input.width > 1024
            || self.input.height > 1024
        {
            issues.push("输入宽高必须在 32..1024".into());
        }
        if self.input.layout != TensorLayout::Nchw {
            issues.push("只支持 NCHW 输入".into());
        }
        if !(0.0..=0.5).contains(&self.input.crop_margin) {
            issues.push("cropMargin 必须在 0..0.5".into());
        }
        if self.file_name.is_empty()
            || self.file_name.contains('/')
            || self.file_name.contains('\\')
            || self.file_name.contains("..")
        {
            issues.push("模型文件名非法".into());
        }
        issues
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_layout_inference_accepts_common_shapes() {
        assert_eq!(
            OutputLayout::infer(&[Some(1), Some(68), Some(2)]).unwrap(),
            OutputLayout::N2
        );
        assert_eq!(
            OutputLayout::infer(&[Some(1), Some(2), Some(68)]).unwrap(),
            OutputLayout::C2N
        );
        assert_eq!(
            OutputLayout::infer(&[Some(1), Some(68), Some(3)]).unwrap(),
            OutputLayout::N3
        );
    }

    #[test]
    fn output_layout_inference_rejects_dynamic_and_small() {
        assert!(OutputLayout::infer(&[None, Some(68), Some(2)]).is_err());
        assert!(OutputLayout::infer(&[Some(1), Some(3), Some(2)]).is_err());
        assert!(OutputLayout::infer(&[Some(1), None, Some(2)]).is_err());
        assert!(OutputLayout::infer(&[Some(1), Some(68)]).is_err());
    }

    #[test]
    fn normalization_is_explicit() {
        assert_eq!(Normalization::ZeroToOne.apply(1.0), 1.0);
        assert_eq!(Normalization::MinusOneToOne.apply(0.0), -1.0);
        assert!(Normalization::Imagenet.apply(0.5) > -0.5);
        assert_eq!(Normalization::Raw255.apply(1.0), 255.0);
    }
}
