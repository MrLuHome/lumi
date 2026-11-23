//! G13 · AI 瘦脸 / 瘦身算法的**像素级**契约测试。
//!
//! 证明：计划函数写出的参数经既有内核真正改变像素；
//! 低置信/强度 0 不产生激进变形；质量报告无翻转/折叠硬失败。

use image::Rgba;

use lumi_retouch_lib::ai_engine::body::ai_slim::{plan_body_slim, precheck_body_plan};
use lumi_retouch_lib::ai_engine::body::analysis::analyze_body;
use lumi_retouch_lib::ai_engine::body::{apply_body, SubjectMasks};
use lumi_retouch_lib::ai_engine::face_frame::{FaceFrame, NormPoint};
use lumi_retouch_lib::face_reshape::ai_slim::{plan_face_slim, SlimStyle};
use lumi_retouch_lib::face_reshape::contour::analyze_face_contour;
use lumi_retouch_lib::image_engine::buffer::Image16;
use lumi_retouch_lib::image_engine::cancel::CancelToken;
use lumi_retouch_lib::image_engine::params::{BodyAdjust, EditParams, FaceReshape};
use lumi_retouch_lib::image_engine::pipeline::render_with_report;

const W: u32 = 400;
const H: u32 = 520;

fn face() -> FaceFrame {
    FaceFrame {
        id: 7,
        uuid: "g13-face".into(),
        score: 0.96,
        x: 0.30,
        y: 0.22,
        width: 0.40,
        height: 0.50,
        landmarks: vec![
            NormPoint { x: 0.42, y: 0.38 },
            NormPoint { x: 0.58, y: 0.38 },
            NormPoint { x: 0.50, y: 0.47 },
            NormPoint { x: 0.45, y: 0.54 },
            NormPoint { x: 0.55, y: 0.54 },
        ],
        dense: None,
        level3: None,
    }
}

fn synthetic_portrait() -> Image16 {
    let mut image: Image16 = Image16::new(W, H);
    for y in 0..H {
        for x in 0..W {
            let inside_face = x > 120 && x < 280 && y > 110 && y < 380;
            let pixel = if inside_face {
                let shade = ((x as f32 / W as f32) * 8000.0) as u16;
                Rgba([42_000 + shade, 32_000 + shade / 2, 26_000, 65_535])
            } else {
                let stripe = if (x / 8) % 2 == 0 { 12_000 } else { 52_000 };
                Rgba([stripe, stripe, stripe, 65_535])
            };
            image.put_pixel(x, y, pixel);
        }
    }
    image
}

/// 竖直「人体」色块 + 背景条纹，供瘦身。
fn synthetic_body() -> Image16 {
    let mut image: Image16 = Image16::new(W, H);
    for y in 0..H {
        for x in 0..W {
            let in_body = x > 140 && x < 260 && y > 40 && y < 500;
            let pixel = if in_body {
                Rgba([30_000, 28_000, 40_000, 65_535])
            } else {
                let stripe = if (y / 6) % 2 == 0 { 8_000 } else { 55_000 };
                Rgba([stripe, stripe, stripe, 65_535])
            };
            image.put_pixel(x, y, pixel);
        }
    }
    image
}

fn body_alpha() -> Vec<f32> {
    let mut alpha = vec![0.0f32; (W * H) as usize];
    for y in 40..500u32 {
        for x in 140..260u32 {
            alpha[(y * W + x) as usize] = 1.0;
        }
    }
    alpha
}

fn count_diff(a: &Image16, b: &Image16, rect: (u32, u32, u32, u32)) -> u32 {
    let (x0, y0, x1, y1) = rect;
    let mut n = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            if a.get_pixel(x, y) != b.get_pixel(x, y) {
                n += 1;
            }
        }
    }
    n
}

#[test]
fn g13_face_slim_from_analysis_changes_pixels_and_protects_background() {
    let image = synthetic_portrait();
    let contour = analyze_face_contour(&face(), W, H).expect("contour");
    assert!(contour.allow_aggressive);

    let (shape, notes) = plan_face_slim(&contour, SlimStyle::Advanced, 70.0);
    assert!(shape.slim > 0.0, "高级档应有正瘦脸强度: {shape:?}");
    assert!(notes.is_empty() || notes.iter().all(|n| !n.contains("降至")));

    let params = EditParams {
        reshape: vec![{
            let mut entry = FaceReshape::new(7);
            entry.face_uuid = "g13-face".into();
            entry.face = shape;
            entry
        }],
        ..Default::default()
    };

    let (rendered, report) = render_with_report(
        &image,
        &params,
        &[face()],
        &CancelToken::default(),
        &mut |_, _| {},
    )
    .unwrap();

    assert_eq!(report.reshape.len(), 1);
    let entry = &report.reshape[0];
    assert!(entry.warped_pixels > 500, "瘦脸应改脸内像素");
    assert_eq!(entry.flipped_triangles, 0, "质量门必须消除翻转");

    let changed = count_diff(&rendered, &image, (150, 140, 250, 340));
    assert!(changed > 500, "脸区变化 {changed}");

    for (x, y) in [(0, 0), (399, 0), (0, 519), (399, 519)] {
        assert_eq!(rendered.get_pixel(x, y), image.get_pixel(x, y));
    }
}

#[test]
fn g13_face_slim_zero_strength_is_identity() {
    let image = synthetic_portrait();
    let contour = analyze_face_contour(&face(), W, H).unwrap();
    let (shape, _) = plan_face_slim(&contour, SlimStyle::Advanced, 0.0);
    assert!(shape.is_neutral());

    let params = EditParams {
        reshape: vec![{
            let mut entry = FaceReshape::new(7);
            entry.face_uuid = "g13-face".into();
            entry.face = shape;
            entry
        }],
        ..Default::default()
    };
    let (rendered, report) = render_with_report(
        &image,
        &params,
        &[face()],
        &CancelToken::default(),
        &mut |_, _| {},
    )
    .unwrap();
    if let Some(entry) = report.reshape.first() {
        assert_eq!(entry.warped_pixels, 0);
    }
    assert_eq!(count_diff(&rendered, &image, (0, 0, W, H)), 0);
}

#[test]
fn g13_low_confidence_face_does_not_aggressive_slim() {
    let mut low = face();
    low.score = 0.28;
    let contour = analyze_face_contour(&low, W, H).unwrap();
    assert!(!contour.allow_aggressive);
    let (shape, notes) = plan_face_slim(&contour, SlimStyle::Advanced, 100.0);
    assert!(shape.slim <= 31.0, "低置信不得满档: {}", shape.slim);
    assert!(notes.iter().any(|n| n.contains("激进") || n.contains("降至")));
}

#[test]
fn g13_body_slim_changes_subject_edge_and_not_far_background() {
    let mut image = synthetic_body();
    let original = image.clone();
    let alpha = body_alpha();
    let analysis = analyze_body("s1", &alpha, W, H, &[]);
    assert!(analysis.detected);

    let (shape, notes) = plan_body_slim(&analysis, SlimStyle::Refined, 60.0);
    assert!(shape.waist < 0.0, "收腰为负: {shape:?} {notes:?}");
    assert!(precheck_body_plan(&analysis, &shape).is_ok());

    let mut adjust = BodyAdjust::new("s1");
    adjust.shape = shape;

    let mut masks = SubjectMasks::new(W, H);
    masks.global = alpha.clone();

    let reports = apply_body(&mut image, &mut masks, &[adjust], &[], &CancelToken::default())
        .expect("apply_body");
    assert_eq!(reports.len(), 1);
    let report = &reports[0];
    assert!(
        report.affected_pixels > 200,
        "瘦身应改轮廓像素: {}",
        report.affected_pixels
    );
    assert_eq!(report.folded_cells, 0, "折叠检测必须通过");
    assert!(report.notes.iter().all(|n| !n.contains("尺寸不一致")));

    // 背景远角（条纹）不应被改。
    assert_eq!(image.get_pixel(0, 0), original.get_pixel(0, 0));
    assert_eq!(
        image.get_pixel(W - 1, H - 1),
        original.get_pixel(W - 1, H - 1)
    );

    // 人物中部可能因轮廓带位移而变化。
    let edge_changed = count_diff(&image, &original, (130, 100, 270, 450));
    assert!(edge_changed > 0);
}

#[test]
fn g13_body_without_subject_is_neutral() {
    let analysis = analyze_body("s1", &vec![0.0; (W * H) as usize], W, H, &[]);
    let (shape, _) = plan_body_slim(&analysis, SlimStyle::Advanced, 90.0);
    assert!(shape.is_neutral());
    assert!(precheck_body_plan(&analysis, &shape).is_ok() || shape.is_neutral());
}
