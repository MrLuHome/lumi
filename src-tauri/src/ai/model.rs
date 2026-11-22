//! 随包 YuNet 模型的公开常量。
//!
//! R11 起这些常量**从登记表派生**（单一真相）：
//! `ai_engine::model_registry::builtin::YUNET` 是唯一的登记处，
//! 这里只保留符号名不变，避免打断 `ai/face.rs`、`ai_engine/skin_segmentation`
//! 等既有引用点。
//!
//! 为什么不是简单的 `const X: &str = builtin::YUNET.field`：`MODEL_INPUT` 需要的是
//! “结构化契约的人可读投影”，而 `format!` 不是 const 表达式。因此
//! `MODEL_INPUT` 仍是字面量，但由单元测试
//! `model_input_matches_the_structured_spec` 强制与
//! `builtin::YUNET.input.format_human()` 相等 —— 一旦量纲变化，测试立刻变红。

use crate::ai_engine::model_registry::builtin;

// 这些常量引用 `builtin.rs` 里的字面量 `const`（而不是 `builtin::YUNET.file_name`）：
// `ModelSpec` 含 `Cow`（需要 drop），`const X = CONST_ITEM.field` 会先求值出一个
// 需要在常量期 drop 的临时量（E0493）。字面量提成独立 `const` 后，
// 单一真相与 const 链都保住了。
pub const MODEL_FILE: &str = builtin::YUNET_FILE;
pub const MODEL_NAME: &str = builtin::YUNET_NAME;
pub const MODEL_VERSION: &str = builtin::YUNET_VERSION;
pub const MODEL_SHA256: &str = builtin::YUNET_SHA256;
pub const MODEL_LICENSE: &str = builtin::YUNET_LICENSE_SPDX;
pub const MODEL_SOURCE: &str = builtin::YUNET_WEIGHTS_URL;

/// 结构化投影：`TensorSpec { shape: [1,3,640,640], F32, NCHW, BGR, Raw255 }`。
pub const MODEL_INPUT: &str = "1×3×640×640 float32（BGR，0..255）";
pub const MODEL_OUTPUTS: &str = "cls/obj/bbox/kps，stride 8/16/32，共 12 个输出";
pub const DETECT_SIZE: u32 = 640;
pub const SCORE_THRESHOLD: f32 = 0.6;
pub const NMS_THRESHOLD: f32 = 0.3;

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_engine::inference::ValueRange;

    #[test]
    fn sha_matches_known_empty_hash() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn constants_are_derived_from_the_registry() {
        assert_eq!(MODEL_FILE, builtin::YUNET.file_name);
        assert_eq!(MODEL_NAME, builtin::YUNET.display_name);
        assert_eq!(MODEL_VERSION, builtin::YUNET.version);
        assert_eq!(MODEL_SHA256, builtin::YUNET.sha256);
        assert_eq!(MODEL_SOURCE, builtin::YUNET.provenance.weights_url);
        assert_eq!(MODEL_LICENSE, "MIT");
        assert_eq!(DETECT_SIZE as usize, builtin::YUNET.input.shape[2]);
        assert_eq!(builtin::YUNET.input.shape[3], DETECT_SIZE as usize);
    }

    /// R9 的回归护栏：常量的量纲描述必须与结构化契约逐字一致。
    #[test]
    fn model_input_matches_the_structured_spec() {
        assert_eq!(MODEL_INPUT, builtin::YUNET.input.format_human());
        assert_eq!(builtin::YUNET.input.value_range, ValueRange::Raw255);
        assert!(MODEL_INPUT.contains("0..255"));
        assert!(!MODEL_INPUT.contains("0..1）"));
    }

    #[test]
    fn outputs_constant_matches_the_head_count() {
        assert_eq!(builtin::YUNET.output_count(), 12);
        assert!(MODEL_OUTPUTS.contains("12"));
    }
}
