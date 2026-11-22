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
///
/// 为什么不带 `UserImported { manifest_id: String }` 这样的 payload：
/// 真实理由是**没有信息可丢** —— 用户模型的 ID 直接取自 `manifest.json`
/// （`Registry::user_models()` → `UserModelSnapshot.id` → `ModelEntryView.id`），
/// 不必在来源枚举里再存一份，否则同一个 ID 会有两个真相、还要考虑不一致时以谁为准。
///
/// 注意：**不要**把这个理由写成"带 payload 的枚举无法进 const"——那是错的，
/// `Builtin` 变体照样能在 `const` 中构造，只有带 `String` 的那一档不行。
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
///
/// 不是 `Copy`：`TensorSpec.shape` 是 `Cow<'static, [usize]>`（见该类型的说明），
/// 但 `Cow::Borrowed` 分支让本类型仍可用 `const` 构造，
/// 因此 `pub const YUNET: ModelSpec = …` 与 `const ALL: &[&ModelSpec] = &[&YUNET]` 都成立。
#[derive(Debug, Clone, PartialEq, Eq)]
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

    /// 任务 → ID 前缀。新增任务时必须在这里登记前缀，否则 ID 测试会失败。
    fn task_prefix(task: ModelTask) -> &'static str {
        match task {
            ModelTask::FaceDetect => "face-detect-",
            ModelTask::FaceLandmark => "face-landmark-",
            ModelTask::SkinParse => "skin-parse-",
            ModelTask::FaceParse => "face-parse-",
            ModelTask::SubjectSegment => "subject-segment-",
            ModelTask::BodyPose => "body-pose-",
        }
    }

    /// 内置 ID 必须遵守 `<task>-<family>-<version>` 的更严子集
    /// （比 `model_hub` 的校验更紧，避免新 ID 漂移）。
    ///
    /// 旧版只校验函数里硬编码的一个字面量，不碰 `builtin::ALL` 任何条目，
    /// **永不失败**（QA 指出，已修）。现在遍历全部登记记录。
    #[test]
    fn every_registered_model_id_follows_the_naming_contract() {
        use crate::ai_engine::model_registry::builtin;

        assert!(!builtin::ALL.is_empty(), "登记表为空时本测试无意义");

        let mut seen: Vec<&str> = Vec::new();
        for spec in builtin::ALL {
            let id = spec.id;
            assert!(id.len() <= 48, "`{id}` 超过 48 字符");
            assert!(!id.is_empty(), "模型 ID 不能为空");
            assert!(
                id.chars()
                    .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-'),
                "`{id}` 含非法字符（只允许 [a-z0-9-]）"
            );
            assert!(!id.starts_with('-') && !id.ends_with('-'), "`{id}` 首尾不能是 -");
            assert!(!id.contains("--"), "`{id}` 出现连续 -");
            assert!(
                id.starts_with(task_prefix(spec.task)),
                "`{id}` 不符合 `<task>-<family>-<version>`：任务 {:?} 要求前缀 `{}`",
                spec.task,
                task_prefix(spec.task)
            );
            assert!(
                id.rsplit('-').next().unwrap_or_default().len() >= 2,
                "`{id}` 缺少版本尾段"
            );
            assert!(!seen.contains(&id), "模型 ID `{id}` 重复登记");
            seen.push(id);
        }
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
