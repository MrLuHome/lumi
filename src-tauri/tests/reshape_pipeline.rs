//! 塑形管线端到端测试：真实像素变化、背景保护、LEVEL 3 参数忽略。

use image::Rgba;
use lumi_retouch_lib::ai_engine::face_frame::{FaceFrame, NormPoint};
use lumi_retouch_lib::image_engine::buffer::Image16;
use lumi_retouch_lib::image_engine::cancel::CancelToken;
use lumi_retouch_lib::image_engine::params::{EditParams, FaceReshape};
use lumi_retouch_lib::image_engine::pipeline::render_with_report;

fn face() -> FaceFrame {
    FaceFrame {
        id: 7,
        uuid: String::new(),
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

/// 带纹理的合成人像：脸区为肤色渐变，背景为竖条纹，便于检测“背景是否被拉动”。
fn synthetic_portrait() -> Image16 {
    let width = 400u32;
    let height = 520u32;
    let mut image: Image16 = Image16::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let inside_face = x > 120 && x < 280 && y > 110 && y < 380;
            let pixel = if inside_face {
                let shade = ((x as f32 / width as f32) * 8000.0) as u16;
                Rgba([42_000 + shade, 32_000 + shade / 2, 26_000, 65_535])
            } else {
                // 高对比竖条纹，任何背景位移都会非常明显。
                let stripe = if (x / 8) % 2 == 0 { 12_000 } else { 52_000 };
                Rgba([stripe, stripe, stripe, 65_535])
            };
            image.put_pixel(x, y, pixel);
        }
    }
    image
}

#[test]
fn contour_reshape_changes_face_pixels_and_protects_background() {
    let image = synthetic_portrait();
    let params = EditParams {
        reshape: vec![{
            let mut entry = FaceReshape::new(7);
            entry.face.slim = 60.0;
            entry.face.jaw = 30.0;
            entry.face.length = -20.0;
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

    assert_eq!(report.reshape.len(), 1, "应有一张人脸的质量报告");
    let entry = &report.reshape[0];
    assert!(!entry.dense_landmarks);
    assert!(entry.control_points > 0);
    assert!(entry.triangles > 0);
    assert!(entry.warped_pixels > 0, "瘦脸必须在真实像素上生效");
    assert_eq!(entry.flipped_triangles, 0, "质量保护必须消除三角翻转");

    let mut changed = 0;
    for y in 140..340 {
        for x in 150..250 {
            if rendered.get_pixel(x, y) != image.get_pixel(x, y) {
                changed += 1;
            }
        }
    }
    assert!(changed > 500, "脸内应有大量像素发生位移（实际 {changed}）");

    // 四角背景远离人脸，必须逐像素不变。
    for (x, y) in [(0, 0), (399, 0), (0, 519), (399, 519)] {
        assert_eq!(
            rendered.get_pixel(x, y),
            image.get_pixel(x, y),
            "背景角落 ({x},{y}) 不应被面部变形影响"
        );
    }
}

#[test]
fn level3_parameters_are_not_applied_in_production() {
    let image = synthetic_portrait();
    let params = EditParams {
        reshape: vec![{
            let mut entry = FaceReshape::new(7);
            // 只包含 LEVEL 3 参数：必须被清零，不得改变像素。
            entry.eyes.size = 90.0;
            entry.nose.alar = 80.0;
            entry.mouth.width = 70.0;
            entry.chin.length = 60.0;
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

    assert_eq!(
        rendered.into_raw(),
        image.into_raw(),
        "LEVEL 3 参数必须被忽略"
    );
    if let Some(entry) = report.reshape.first() {
        assert_eq!(entry.warped_pixels, 0);
    }
}

/// 性能基线：默认忽略，手动运行
/// `cargo test --release --test reshape_pipeline -- --ignored --nocapture`
/// 输出真实耗时，用于报告中的数据，不作硬性时间断言（避免 CI 抖动）。
#[test]
#[ignore]
fn reshape_perf_baseline_12mp() {
    let width = 3000u32;
    let height = 4000u32;
    let mut image: Image16 = Image16::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let inset = x > 900 && x < 2100 && y > 800 && y < 2800;
            let pixel = if inset {
                let shade = ((x as f32 / width as f32) * 8000.0) as u16;
                Rgba([42_000 + shade, 32_000 + shade / 2, 26_000, 65_535])
            } else {
                let stripe = if (x / 16) % 2 == 0 { 12_000 } else { 52_000 };
                Rgba([stripe, stripe, stripe, 65_535])
            };
            image.put_pixel(x, y, pixel);
        }
    }
    let params = EditParams {
        reshape: vec![{
            let mut entry = FaceReshape::new(7);
            entry.face.slim = 50.0;
            entry.face.jaw = 25.0;
            entry
        }],
        ..Default::default()
    };
    let mut face = face();
    face.x = 0.30;
    face.y = 0.20;
    face.width = 0.40;
    face.height = 0.50;

    let started = std::time::Instant::now();
    let (_, report) = render_with_report(
        &image,
        &params,
        &[face],
        &CancelToken::default(),
        &mut |_, _| {},
    )
    .unwrap();
    let total = started.elapsed().as_secs_f32() * 1000.0;
    let entry = report.reshape.first().expect("reshape report");
    println!(
        "12MP reshape: total={total:.1}ms face={:.1}ms control_points={} triangles={} warped_pixels={} flipped={} scale={:.2}",
        entry.elapsed_ms,
        entry.control_points,
        entry.triangles,
        entry.warped_pixels,
        entry.flipped_triangles,
        entry.applied_scale
    );
    assert!(entry.warped_pixels > 0);
}

#[test]
fn manual_warp_stroke_changes_pixels_inside_region_only() {
    use lumi_retouch_lib::image_engine::params::{MaskPoint, WarpMode, WarpStroke};
    let image = synthetic_portrait();
    let params = EditParams {
        reshape: vec![{
            let mut entry = FaceReshape::new(7);
            entry.manual = lumi_retouch_lib::image_engine::params::ManualWarp {
                enabled: true,
                strokes: vec![WarpStroke {
                    mode: WarpMode::Push,
                    radius: 0.08,
                    strength: 80.0,
                    feather: 60.0,
                    points: vec![
                        MaskPoint { x: 0.38, y: 0.42 },
                        MaskPoint { x: 0.62, y: 0.42 },
                    ],
                }],
            };
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
    assert!(report.reshape[0].warped_pixels > 0);
    assert_eq!(
        rendered.get_pixel(5, 5),
        image.get_pixel(5, 5),
        "手动笔刷不得影响远离人脸的区域"
    );
}
