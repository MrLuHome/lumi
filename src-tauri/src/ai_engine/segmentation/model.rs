//! 分割/姿态模型目录。
//!
//! 这里记录的是「候选项与审查状态」，不是已打包的权重。
//! 只有 `status == Verified` 且权重 SHA-256 记录在案，才允许启用推理。
//!
//! R11 定位：本文件是**调研目录（非加载源）**。可加载登记表唯一来源是
//! `ai_engine::model_registry::builtin`；该登记表**刻意不引用** `CATALOG`，
//! 避免「候选被误当作已授权权重」。

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelStatus {
    /// 仓库内没有权重，或再分发条件未核实。
    Disabled,
    /// 权重与许可证已核实，但尚未完成运行时验证。
    Evaluating,
    /// 已完成许可证与运行时验证。
    Verified,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelEntry {
    pub id: &'static str,
    pub name: &'static str,
    pub task: &'static str,
    pub license: &'static str,
    pub status: ModelStatus,
    pub reason: &'static str,
}

pub const CATALOG: &[ModelEntry] = &[
    ModelEntry {
        id: "birefnet",
        name: "BiRefNet（候选）",
        task: "subject-segmentation",
        license: "MIT（代码）；具体 ONNX 权重来源与再分发条件未核实",
        status: ModelStatus::Disabled,
        reason: "未取得可再分发的 ONNX 权重与 SHA-256；tract-onnx 算子兼容性未验证。",
    },
    ModelEntry {
        id: "sam2",
        name: "SAM 2（候选）",
        task: "subject-segmentation",
        license: "Apache-2.0（代码与检查点）",
        status: ModelStatus::Disabled,
        reason: "依赖 PyTorch 运行时，需要独立推理进程与额外分发体积；本轮未验证通过。",
    },
    ModelEntry {
        id: "mediapipe-pose",
        name: "MediaPipe Pose Landmarker（候选）",
        task: "body-pose",
        license: "Apache-2.0",
        status: ModelStatus::Disabled,
        reason: "桌面端 .task 模型分发路径未确定，与第四轮 478 点评估的阻塞相同。",
    },
    ModelEntry {
        id: "rvm",
        name: "Robust Video Matting（候选）",
        task: "subject-segmentation",
        license: "GPL-3.0",
        status: ModelStatus::Disabled,
        reason: "GPL-3.0 不适合当前商业发行策略，仅作技术参考。",
    },
];

pub fn entry(id: &str) -> Option<&'static ModelEntry> {
    CATALOG.iter().find(|entry| entry.id == id)
}

pub fn verified_models() -> Vec<&'static ModelEntry> {
    CATALOG
        .iter()
        .filter(|entry| entry.status == ModelStatus::Verified)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_model_is_marked_verified_without_weights() {
        assert!(verified_models().is_empty());
        for model in CATALOG {
            assert_ne!(model.status, ModelStatus::Verified);
            assert!(!model.reason.is_empty());
        }
    }
}
