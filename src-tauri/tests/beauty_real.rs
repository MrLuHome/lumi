//! 真实照片上的第三轮算法验证：使用仓库内 YuNet 权重检测真人脸，
//! 再跑皮肤蒙版 / 分析 / 美肤，断言可量化指标，不使用任何模拟检测结果。

use std::path::{Path, PathBuf};

use lumi_retouch_lib::ai::face::FaceDetectionResult;
use lumi_retouch_lib::ai::model::MODEL_FILE;
use lumi_retouch_lib::ai::FaceEngine;
use lumi_retouch_lib::ai_engine::face_frame::{FaceFrame, FaceGeometry};
use lumi_retouch_lib::ai_engine::retouch::analyze::{analyze_face, preset};
use lumi_retouch_lib::ai_engine::retouch::apply_face_retouch;
use lumi_retouch_lib::ai_engine::skin_segmentation::{apply_ownership, build_skin_mask, SkinMask};
use lumi_retouch_lib::image_engine::buffer::{fit_within, CHANNEL_MAX};
use lumi_retouch_lib::image_engine::cancel::CancelToken;
use lumi_retouch_lib::image_engine::decoder;
use lumi_retouch_lib::image_engine::params::{EditParams, FaceBeauty, ManualMask};
use lumi_retouch_lib::image_engine::{decoder::DecodedImage, pipeline};

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

fn decode_and_detect(name: &str) -> (DecodedImage, FaceDetectionResult) {
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
        "[beauty-real] {name}: {} 张脸 · 推理 {} ms",
        result.faces.len(),
        result.inference_ms
    );
    (decoded, result)
}

fn frames(result: &FaceDetectionResult) -> Vec<FaceFrame> {
    result
        .faces
        .iter()
        .map(|face| FaceFrame::from_detected(face, result.image_width, result.image_height))
        .collect()
}

fn luma(pixel: [u16; 4]) -> f32 {
    (0.299 * pixel[0] as f32 + 0.587 * pixel[1] as f32 + 0.114 * pixel[2] as f32) / CHANNEL_MAX
}

/// 进程常驻内存（MB），用于记录真实渲染的内存占用。
fn resident_mb() -> f32 {
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output();
    match output {
        Ok(output) => String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<f32>()
            .map(|kb| kb / 1024.0)
            .unwrap_or(0.0),
        Err(_) => 0.0,
    }
}

#[test]
fn real_portraits_have_sane_skin_masks_and_protected_features() {
    for name in ["portrait.jpg", "portrait_bright.jpg", "portrait_dark.jpg"] {
        let (decoded, result) = decode_and_detect(name);
        let Some(face) = frames(&result).into_iter().next() else {
            eprintln!("[beauty-real] {name}: 未检测到人脸，跳过蒙版断言（真实输出）");
            continue;
        };
        let mask = build_skin_mask(&decoded.image, &face, &ManualMask::default()).unwrap();
        let geometry =
            FaceGeometry::from_face(&face, decoded.image.width(), decoded.image.height()).unwrap();

        let total = (decoded.image.width() * decoded.image.height()) as f32;
        assert!(
            mask.active_pixels() as f32 > total * 0.005,
            "{name}: 皮肤蒙版面积过小（{} px）",
            mask.active_pixels()
        );
        let forehead = geometry.from_local(0.0, -0.85);
        let cheek = geometry.from_local(0.45, 0.35);
        assert!(
            mask.at(forehead.0 as u32, forehead.1 as u32) > 0.35,
            "{name}: 额头蒙版过低"
        );
        assert!(
            mask.at(cheek.0 as u32, cheek.1 as u32) > 0.35,
            "{name}: 脸颊蒙版过低"
        );
        for (eye_x, eye_y) in geometry.eyes {
            assert!(
                mask.at(eye_x as u32, eye_y as u32) < 0.35,
                "{name}: 眼睛未被保护"
            );
        }

        let metrics = analyze_face(&decoded.image, &mask, &geometry, &CancelToken::default())
            .unwrap()
            .suggested;
        for value in metrics.fields() {
            assert!(
                (0.0..=100.0).contains(&value),
                "{name}: 分析参数越界 {value}"
            );
        }
        eprintln!(
            "[beauty-real] {name}: 蒙版 {} px · 平均 alpha {:.2} · 建议磨皮 {:.0}",
            mask.active_pixels(),
            mask.mean_alpha(),
            metrics.skin_smooth
        );
    }
}

#[test]
fn real_portrait_retouch_changes_skin_but_not_eyes() {
    let (decoded, result) = decode_and_detect("portrait.jpg");
    let face = frames(&result).into_iter().next().expect("portrait 应有脸");
    let mask = build_skin_mask(&decoded.image, &face, &ManualMask::default()).unwrap();
    let geometry =
        FaceGeometry::from_face(&face, decoded.image.width(), decoded.image.height()).unwrap();
    let mut image = decoded.image.clone();
    let report = apply_face_retouch(
        &mut image,
        &mask,
        &geometry,
        &preset("natural"),
        0.0,
        &[],
        &CancelToken::default(),
    )
    .unwrap();

    // 蒙版内应有真实变化，但变化不能是“整图重绘”。
    let mut changed = 0usize;
    for (x, y, alpha) in mask.iter() {
        if alpha < 0.5 {
            continue;
        }
        let before = decoded.image.get_pixel(x, y).0;
        let after = image.get_pixel(x, y).0;
        let delta = (0..3)
            .map(|channel| (before[channel] as i32 - after[channel] as i32).abs())
            .max()
            .unwrap_or(0);
        if delta > 32 {
            changed += 1;
        }
    }
    assert!(
        changed > 200,
        "真实人像应有可见但受控的皮肤变化：{changed} px"
    );

    for (eye_x, eye_y) in geometry.eyes {
        let x = eye_x.clamp(0.0, decoded.image.width() as f32 - 1.0) as u32;
        let y = eye_y.clamp(0.0, decoded.image.height() as f32 - 1.0) as u32;
        let before = luma(decoded.image.get_pixel(x, y).0);
        let after = luma(image.get_pixel(x, y).0);
        assert!(
            (after - before).abs() < 0.03,
            "眼睛区域必须保持不变：{before} → {after}"
        );
    }
    eprintln!(
        "[beauty-real] portrait 美肤：{} px 变化 · 瑕疵 {}/{} · 皮肤像素 {}",
        changed, report.blemish_repaired, report.blemish_detected, report.skin_pixels
    );
}

#[test]
fn real_group_faces_get_disjoint_owned_masks() {
    let (decoded, result) = decode_and_detect("group.jpg");
    let faces = frames(&result);
    assert!(faces.len() >= 2, "合照应检测到多张脸");
    let mut masks: Vec<SkinMask> = faces
        .iter()
        .map(|face| build_skin_mask(&decoded.image, face, &ManualMask::default()).unwrap())
        .collect();
    for (index, mask) in masks.iter().enumerate() {
        assert!(mask.active_pixels() > 500, "人脸 {index} 蒙版面积过小");
    }
    let centers: Vec<(f32, f32)> = faces
        .iter()
        .map(|face| {
            (
                (face.x + face.width * 0.5) * decoded.image.width() as f32,
                (face.y + face.height * 0.5) * decoded.image.height() as f32,
            )
        })
        .collect();
    apply_ownership(&mut masks, &centers);
    // 归属后任意两张脸的蒙版不应在同一像素同时为高频覆盖。
    for first in 0..masks.len() {
        for second in first + 1..masks.len() {
            let mut shared = 0usize;
            for (x, y, alpha) in masks[first].iter() {
                if alpha > 0.4 && masks[second].at(x, y) > 0.4 {
                    shared += 1;
                }
            }
            let smaller = masks[first]
                .active_pixels()
                .min(masks[second].active_pixels());
            assert!(
                shared < (smaller / 50).max(10),
                "人脸 {first}/{second} 蒙版重叠过多：{shared}"
            );
        }
    }
}

#[test]
fn real_high_resolution_beauty_keeps_dimensions() {
    let (decoded, result) = decode_and_detect("portrait_hires.jpg");
    let faces = frames(&result);
    assert!(!faces.is_empty(), "高分辨率肖像应检测到人脸");
    let params = EditParams {
        faces: vec![FaceBeauty {
            face_id: faces[0].id,
            face_uuid: String::new(),
            enabled: true,
            beauty: preset("natural"),
        }],
        ..Default::default()
    };
    let original = decoded.image.clone().into_raw();
    let before_mb = resident_mb();
    let started = std::time::Instant::now();
    let rendered =
        pipeline::render(&decoded.image, &params, &faces, &CancelToken::default()).unwrap();
    let elapsed = started.elapsed();
    let after_mb = resident_mb();
    assert_eq!(
        (rendered.width(), rendered.height()),
        (decoded.image.width(), decoded.image.height())
    );
    assert_eq!(
        decoded.image.clone().into_raw(),
        original,
        "原始图不得被修改"
    );
    eprintln!(
        "[beauty-real] portrait_hires {}x{} 全分辨率美肤：{elapsed:?} · RSS {before_mb:.0}→{after_mb:.0} MB",
        rendered.width(),
        rendered.height()
    );
}
