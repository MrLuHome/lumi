//! 模型规格（登记表的行定义）。
//!
//! 与旧结构的差异（这是 R11 的核心增量）：
//! - 输入输出是**结构化契约**（[`TensorSpec`] / [`OutputHead`]），不是自由字符串；
//! - 许可拆成**四个独立字段**（见 [`super::license::LicenseRecord`]）；
//! - 每条记录都带**溯源**（[`Provenance`]），每个结论都能指到页面或文件；
//! - 文件名与登记目录分开，`locate()` 只负责把候选目录拼成路径。

use serde::Serialize;

use crate::ai_engine::inference::tensor::TensorSpec;

use super::license::LicenseRecord;

/// 模型任务。前端 `ModelEntryView.task` 用的就是这些值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelTask {
    FaceDetect,
    FaceLandmark,
    SkinParse,
    FaceParse,
    SubjectSegment,
    BodyPose,
}

impl ModelTask {
    pub fn label(self) -> &'static str {
        match self {
            ModelTask::FaceDetect => "人脸检测",
            ModelTask::FaceLandmark => "人脸关键点",
            ModelTask::SkinParse => "皮肤解析",
            ModelTask::FaceParse => "人脸解析",
            ModelTask::SubjectSegment => "人物分割",
            ModelTask::BodyPose => "人体姿态",
        }
    }

    /// 该任务是否允许参与「默认美肤 / 默认精修」。
    ///
    /// 用户自备模型（`ModelOrigin::UserImported`）不参与，需在调用点另行判断来源。
    pub fn is_default_pipeline_capable(self) -> bool {
        matches!(self, ModelTask::FaceDetect)
    }
}

/// 模型文件格式。R11 只接受单文件 ONNX。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelFormat {
    Onnx,
}

impl ModelFormat {
    pub fn label(self) -> &'static str {
        match self {
            ModelFormat::Onnx => "onnx",
        }
    }
}

/// 模型来源。用户自备模型**不进入**正式注册清单。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelOrigin {
    /// 随安装包分发，必须通过完整许可闸门。
    Builtin,
    /// 用户通过 Model Hub 导入，仅本机可用，标为「未审核 · 仅本地」。
    UserImported,
}

impl ModelOrigin {
    pub fn is_official(self) -> bool {
        matches!(self, ModelOrigin::Builtin)
    }

    pub fn label(self) -> &'static str {
        match self {
            ModelOrigin::Builtin => "随包",
            ModelOrigin::UserImported => "用户自备 · 仅本地",
        }
    }
}

/// 一个输出 head 的声明：名字 + 形状 + 语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputHead {
    /// 例如 `cls_8`。
    pub name: &'static str,
    pub shape: &'static [usize],
    /// 该 head 的含义，便于后续接新模型时不被“看起来像”误导。
    pub semantic: &'static str,
}

/// 溯源：每个字段都必须能指到「哪个页面 / 哪个文件」。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Provenance {
    /// 权重官方发布页。
    pub weights_url: &'static str,
    /// 许可原文页。
    pub license_url: &'static str,
    /// 随包 LICENSE 文件（相对仓库根）。
    pub bundled_license_file: Option<&'static str>,
    pub verified_at: &'static str,
    pub verified_by: &'static str,
}

/// 一条模型登记记录。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelSpec {
    /// `<task>-<family>-<version>`，全小写，只允许 `[a-z0-9-]`，长度 ≤48。
    pub id: &'static str,
    pub display_name: &'static str,
    pub version: &'static str,
    pub task: ModelTask,
    pub format: ModelFormat,
    pub origin: ModelOrigin,
    /// 相对资源根的目录，例如 `models/yunet`。
    pub dir: &'static str,
    pub file_name: &'static str,
    /// 期望文件体积。体积不符是「格式」检查的第一步，成本仅一次 `metadata`。
    pub size_bytes: u64,
    pub sha256: &'static str,
    pub input: TensorSpec,
    pub outputs: &'static [OutputHead],
    pub license: LicenseRecord,
    pub provenance: Provenance,
}

impl ModelSpec {
    /// 输出 head 数量。
    pub fn output_count(&self) -> usize {
        self.outputs.len()
    }

    /// 输出 head 名字列表（用于 `FaceModelStatus.outputs` 之类的人可读字段）。
    pub fn output_names(&self) -> Vec<&'static str> {
        self.outputs.iter().map(|head| head.name).collect()
    }

    /// 是否允许参与默认美肤 / 默认精修。
    ///
    /// **用户自备模型永远为 false**：未审核的东西不能成为默认行为。
    pub fn is_default_eligible(&self) -> bool {
        self.origin.is_official() && self.task.is_default_pipeline_capable()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_id_follows_the_naming_contract() {
        // 内置 ID 必须遵守更严子集（比 model_hub 的校验更紧，避免新 ID 漂移）。
        let id = "face-detect-yunet-2023mar";
        assert!(id.len() <= 48);
        assert!(id
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-'));
    }

    #[test]
    fn origin_and_task_serialize_as_camel_case() {
        assert_eq!(
            serde_json::to_value(ModelOrigin::UserImported).unwrap(),
            "userImported"
        );
        assert_eq!(
            serde_json::to_value(ModelTask::FaceDetect).unwrap(),
            "faceDetect"
        );
        assert_eq!(serde_json::to_value(ModelFormat::Onnx).unwrap(), "onnx");
    }

    #[test]
    fn official_origin_is_the_only_default_eligible_one() {
        assert!(ModelOrigin::Builtin.is_official());
        assert!(!ModelOrigin::UserImported.is_official());
        assert!(ModelTask::FaceDetect.is_default_pipeline_capable());
        assert!(!ModelTask::SkinParse.is_default_pipeline_capable());
    }
}
