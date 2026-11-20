pub const MODEL_FILE: &str = "face_detection_yunet_2023mar.onnx";
pub const MODEL_NAME: &str = "YuNet 2023mar";
pub const MODEL_VERSION: &str = "2023mar";
pub const MODEL_SHA256: &str = "8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4";
pub const MODEL_LICENSE: &str = "MIT";
pub const MODEL_SOURCE: &str = "https://huggingface.co/opencv/face_detection_yunet";
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

    #[test]
    fn sha_matches_known_empty_hash() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
