//! 内置（随包）模型登记表 —— **唯一真相**。
//!
//! `ai/model.rs` 的常量已经从这张表派生（见该文件的 `MODEL_*` 定义），
//! 不再维护第二份字形。
//!
//! 当前只登记一条：YuNet 2023mar 人脸检测。皮肤解析 / 人物分割登记表刻意留空 ——
//! 没有通过许可审查的权重就不登记，而不是登记了再指望闸门拦住（那样一旦闸门
//! 有 bug 就会把未授权权重跑起来）。
//!
//! ⚠️ **新增随包权重必须同步 `src-tauri/tauri.conf.json` 的 `resources` 列表**，
//! 否则开发机能跑、打包后 `locate()` 永远返回 `None`（历史风险 R8）。

use crate::ai_engine::inference::tensor::{ChannelOrder, TensorSpec, ValueRange};

use super::license::{LicenseEvidence, LicenseEvidenceKind, LicenseRecord, Verdict3};
use super::spec::{ModelFormat, ModelOrigin, ModelSpec, ModelTask, OutputHead, Provenance};

/// 字面量集中在此定义，`ModelSpec` 与 `ai/model.rs` 的 `MODEL_*` 常量都引用它们。
///
/// 为什么不直接写 `pub const MODEL_FILE: &str = YUNET.file_name;`：
/// `TensorSpec.shape` 是 `Cow`（需要 drop），对非 `Copy` 的 const 项做字段读取会
/// 产生一个"需要在常量求值期 drop 的临时量"，编译器报
/// `E0493: destructor of ModelSpec cannot be evaluated at compile-time`。
/// 把字面量提成独立 `const`，就既保住「单一真相」又保住 const 链。
pub const YUNET_ID: &str = "face-detect-yunet-2023mar";
pub const YUNET_NAME: &str = "YuNet 2023mar";
pub const YUNET_VERSION: &str = "2023mar";
pub const YUNET_FILE: &str = "face_detection_yunet_2023mar.onnx";
/// 实测 232,589 B（2026-11-22）。
pub const YUNET_SIZE_BYTES: u64 = 232_589;
pub const YUNET_SHA256: &str =
    "8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4";
/// SPDX 标识。未核实时应为 `None`，不留空串。
pub const YUNET_LICENSE_SPDX: &str = "MIT";
pub const YUNET_DIR: &str = "models/yunet";
pub const YUNET_WEIGHTS_URL: &str =
    "https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet";
pub const YUNET_LICENSE_URL: &str =
    "https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet/LICENSE";
pub const YUNET_BUNDLED_LICENSE_FILE: &str = "src-tauri/resources/models/yunet/LICENSE";

/// 内置 YuNet 2023mar 人脸检测模型。
pub const YUNET: ModelSpec = ModelSpec {
    id: YUNET_ID,
    display_name: YUNET_NAME,
    version: YUNET_VERSION,
    task: ModelTask::FaceDetect,
    format: ModelFormat::Onnx,
    origin: ModelOrigin::Builtin,
    dir: YUNET_DIR,
    file_name: YUNET_FILE,
    size_bytes: YUNET_SIZE_BYTES,
    sha256: YUNET_SHA256,
    // 口径：**BGR，Raw255（u16 ÷ 257 得 0..255）**。
    // `ai/face.rs` 的预处理是 `u16 像素 / 257.0`，而 `u16::MAX / 257.0 == 255.0`，
    // 即 16 位内部表示（0..65535）压回 0..255，与这里的登记值一致。
    // 不是 0..1：若登记成 0..1，未来按契约实现的预处理会差 255 倍，
    // 且 `TensorSpec::validate` 的 bounds(0,1) 会直接让每次检测硬失败 `E_INPUT`。
    input: TensorSpec::declared(&[1, 3, 640, 640], ValueRange::Raw255, ChannelOrder::Bgr),
    outputs: &[
        OutputHead {
            name: "cls_8",
            shape: &[1, 6400, 1],
            semantic: "stride 8 分类置信度（80×80 网格展平）",
        },
        OutputHead {
            name: "cls_16",
            shape: &[1, 1600, 1],
            semantic: "stride 16 分类置信度（40×40 网格展平）",
        },
        OutputHead {
            name: "cls_32",
            shape: &[1, 400, 1],
            semantic: "stride 32 分类置信度（20×20 网格展平）",
        },
        OutputHead {
            name: "obj_8",
            shape: &[1, 6400, 1],
            semantic: "stride 8 目标性置信度",
        },
        OutputHead {
            name: "obj_16",
            shape: &[1, 1600, 1],
            semantic: "stride 16 目标性置信度",
        },
        OutputHead {
            name: "obj_32",
            shape: &[1, 400, 1],
            semantic: "stride 32 目标性置信度",
        },
        OutputHead {
            name: "bbox_8",
            shape: &[1, 6400, 4],
            semantic: "stride 8 边框偏移（log 尺度）",
        },
        OutputHead {
            name: "bbox_16",
            shape: &[1, 1600, 4],
            semantic: "stride 16 边框偏移（log 尺度）",
        },
        OutputHead {
            name: "bbox_32",
            shape: &[1, 400, 4],
            semantic: "stride 32 边框偏移（log 尺度）",
        },
        OutputHead {
            name: "kps_8",
            shape: &[1, 6400, 10],
            semantic: "stride 8 五点关键点偏移",
        },
        OutputHead {
            name: "kps_16",
            shape: &[1, 1600, 10],
            semantic: "stride 16 五点关键点偏移",
        },
        OutputHead {
            name: "kps_32",
            shape: &[1, 400, 10],
            semantic: "stride 32 五点关键点偏移",
        },
    ],
    license: LicenseRecord {
        weights: Verdict3::Allowed,
        training_data: Verdict3::Unverified {
            what_to_check: "需人工核对 WIDER FACE 数据集使用条款（https://shuoyang1213.me/WIDERFACE/），README 未声明数据集许可",
        },
        commercial_use: Verdict3::Allowed,
        redistribution: Verdict3::Allowed,
        spdx: Some(YUNET_LICENSE_SPDX),
        evidence: &[
            LicenseEvidence {
                kind: LicenseEvidenceKind::OfficialPage,
                url: YUNET_WEIGHTS_URL,
                quote: "All files in this directory are licensed under MIT License.",
                accessed_at: "2026-11-22",
            },
            LicenseEvidence {
                kind: LicenseEvidenceKind::BundledLicenseFile,
                url: YUNET_BUNDLED_LICENSE_FILE,
                quote: "MIT License / Copyright (c) 2020 Shiqi Yu <shiqi.yu@gmail.com>",
                accessed_at: "2026-11-22",
            },
        ],
        note: "权重许可 MIT 已核对官方发布页与随包 LICENSE 原文，可商用、可再分发；训练数据许可未核实，上游权重仓（ShiqiYu/libfacedetection.train）许可亦未核实，需在法务文档登记。",
    },
    provenance: Provenance {
        weights_url: YUNET_WEIGHTS_URL,
        license_url: YUNET_LICENSE_URL,
        bundled_license_file: Some(YUNET_BUNDLED_LICENSE_FILE),
        verified_at: "2026-11-22",
        verified_by: "高见远（人工核对官方页面与随包 LICENSE 原文）",
    },
};

/// 皮肤解析登记表。当前为空：BiSeNet / CelebAMask-HQ 系权重受数据集非商用条款限制，
/// InsightFace 多数模型仅限研究，均不满足打包条件。
pub const SKIN_MODELS: &[&ModelSpec] = &[];

/// 人物分割 / 人体姿态登记表。当前为空（候选调研清单在
/// `ai_engine::segmentation::CATALOG`，那是**调研目录**而不是加载源）。
pub const SEGMENTATION_MODELS: &[&ModelSpec] = &[];

/// 全部内置登记记录。
pub const ALL: &[&ModelSpec] = &[&YUNET];

/// 按 ID 查找内置记录。
pub fn find(id: &str) -> Option<&'static ModelSpec> {
    ALL.iter().copied().find(|spec| spec.id == id)
}

/// 全部 ID 列表。
pub fn ids() -> Vec<&'static str> {
    ALL.iter().map(|spec| spec.id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yunet_matches_the_shipped_artifact() {
        assert_eq!(YUNET.size_bytes, 232_589);
        assert_eq!(YUNET.file_name, "face_detection_yunet_2023mar.onnx");
        assert_eq!(
            YUNET.sha256,
            "8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4"
        );
        assert_eq!(YUNET.output_count(), 12);
        assert_eq!(YUNET.input.element_count(), 3 * 640 * 640);
    }

    #[test]
    fn yunet_artifact_on_disk_matches_the_registration() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join(YUNET.dir)
            .join(YUNET.file_name);
        let metadata = std::fs::metadata(&path).expect("随包 YuNet 缺失");
        assert_eq!(metadata.len(), YUNET.size_bytes, "登记体积与实测不一致");
    }

    #[test]
    fn yunet_passes_the_license_gate() {
        assert!(super::super::license::gate(YUNET.origin, &YUNET.license, YUNET.id).is_ok());
        // 训练数据未核实必须被显式披露。
        assert_eq!(YUNET.license.unverified_fields(), vec!["trainingData"]);
    }

    #[test]
    fn only_builtin_face_detect_is_default_eligible() {
        assert!(YUNET.is_default_eligible());
        assert!(ALL.iter().all(|spec| spec.origin == ModelOrigin::Builtin));
    }

    #[test]
    fn skin_and_segmentation_tables_stay_empty_until_a_weight_passes_review() {
        assert!(SKIN_MODELS.is_empty());
        assert!(SEGMENTATION_MODELS.is_empty());
    }

    #[test]
    fn find_resolves_known_and_unknown_ids() {
        assert_eq!(find(YUNET.id).map(|spec| spec.id), Some(YUNET.id));
        assert!(find("no-such-model").is_none());
        assert_eq!(ids(), vec![YUNET.id]);
    }
}
