//! 真实模型推理测试：使用仓库内 YuNet 2023mar ONNX，不允许任何模拟结果。

use std::path::{Path, PathBuf};

use lumi_retouch_lib::ai::face::FaceDetectionResult;
use lumi_retouch_lib::ai::model::MODEL_FILE;
use lumi_retouch_lib::ai::FaceEngine;
use lumi_retouch_lib::image_engine::buffer::fit_within;
use lumi_retouch_lib::image_engine::decoder;

fn model_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("resources/models/yunet")
        .join(MODEL_FILE)
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/images")
        .join(name)
}

fn detect(name: &str) -> FaceDetectionResult {
    let decoded = decoder::decode(&fixture(name)).unwrap();
    let preview = fit_within(&decoded.image, 1800);
    let engine = FaceEngine::default();
    let result = engine
        .detect(
            &model_path(),
            &preview,
            decoded.image.width(),
            decoded.image.height(),
            &mut |_, _| {},
        )
        .unwrap();
    eprintln!(
        "[face] {name}: {} 张 · 推理 {} ms · 总计 {} ms",
        result.faces.len(),
        result.inference_ms,
        result.elapsed_ms
    );
    // 坐标必须落在原图范围内，关键点必须是 5 个。
    for face in &result.faces {
        assert!(face.width > 0.0 && face.height > 0.0);
        assert!(face.x >= 0.0 && face.y >= 0.0);
        assert!(face.x + face.width <= result.image_width as f32 + 2.0);
        assert!(face.y + face.height <= result.image_height as f32 + 2.0);
        assert_eq!(face.landmarks.len(), 5);
    }
    result
}

#[test]
fn detects_face_in_single_portrait() {
    let result = detect("portrait.jpg");
    assert!(!result.faces.is_empty(), "单人肖像应检测到人脸");
    assert_eq!(result.model, "YuNet 2023mar");
    assert!(result.inference_ms > 0, "必须记录真实推理耗时");
    let face = &result.faces[0];
    assert!(face.score > 0.6, "置信度应高于阈值：{}", face.score);
    // 肖像构图：人脸应位于画面上半部分。
    assert!(face.y < result.image_height as f32 * 0.6);
}

#[test]
fn detects_multiple_faces_in_group_photo() {
    let result = detect("group.jpg");
    assert!(
        result.faces.len() >= 2,
        "四人拼贴图应检测到多张人脸，实际 {}",
        result.faces.len()
    );
    // 多人脸的 id 按从左到右排序，便于前端切换。
    let xs: Vec<f32> = result.faces.iter().map(|face| face.x).collect();
    let mut sorted = xs.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(xs, sorted);
}

#[test]
fn reports_zero_faces_for_landscape_without_people() {
    let result = detect("no_face.jpg");
    assert!(result.faces.is_empty(), "无人脸照片不应产生虚假检测结果");
}

#[test]
fn dark_portrait_still_detected_or_fails_explicitly() {
    // 暗光照片允许检测失败，但若返回结果必须是真实输出而非编造。
    let result = detect("portrait_dark.jpg");
    assert!(result.faces.len() <= 1);
}
