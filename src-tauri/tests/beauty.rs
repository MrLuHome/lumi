//! 第三轮美肤引擎的可重复算法测试。
//!
//! 全部使用合成人像（可控肤色、纹理、瑕疵、光照），不依赖主观判断：
//! 蒙版覆盖、五官保护、纹理保留率、瑕疵修复、肤色约束、多人脸参数独立、
//! 手动蒙版、取消与全分辨率输出都有明确数值断言。
#![allow(clippy::manual_range_contains, clippy::cloned_ref_to_slice_refs)]

use image::Rgba;

use lumi_retouch_lib::ai_engine::face_frame::{FaceFrame, FaceGeometry, NormPoint};
use lumi_retouch_lib::ai_engine::retouch::analyze::{analyze_face, preset};
use lumi_retouch_lib::ai_engine::retouch::lab::{chroma, srgb_to_lab};
use lumi_retouch_lib::ai_engine::retouch::apply_face_retouch;
use lumi_retouch_lib::ai_engine::skin_segmentation::{build_skin_mask, SkinMask};
use lumi_retouch_lib::error::AppError;
use lumi_retouch_lib::image_engine::buffer::{Image16, CHANNEL_MAX};
use lumi_retouch_lib::image_engine::cancel::CancelToken;
use lumi_retouch_lib::image_engine::params::{
    BeautyParams, EditParams, FaceBeauty, ManualMask, MaskPoint, MaskStroke, RepairPoint,
};
use lumi_retouch_lib::image_engine::pipeline;

const SKIN: [u16; 3] = [214 * 257, 170 * 257, 140 * 257];

fn skin_image(width: u32, height: u32) -> Image16 {
    Image16::from_pixel(
        width,
        height,
        Rgba([SKIN[0], SKIN[1], SKIN[2], 65535]),
    )
}

/// 合成人脸：双眼间距 d，中心 (cx, cy)，关键点沿用 YuNet 五点顺序。
fn face_frame(id: u32, cx: f32, cy: f32, d: f32, width: u32, height: u32) -> FaceFrame {
    let w = width as f32;
    let h = height as f32;
    let point = |x: f32, y: f32| NormPoint {
        x: x / w,
        y: y / h,
    };
    FaceFrame {
        id,
        uuid: String::new(),
        score: 0.95,
        x: (cx - 1.6 * d) / w,
        y: (cy - 1.4 * d) / h,
        width: 3.2 * d / w,
        height: 3.6 * d / h,
        landmarks: vec![
            point(cx - 0.5 * d, cy),
            point(cx + 0.5 * d, cy),
            point(cx, cy + 0.55 * d),
            point(cx - 0.35 * d, cy + 0.95 * d),
            point(cx + 0.35 * d, cy + 0.95 * d),
        ],
    }
}

fn luma(pixel: [u16; 4]) -> f32 {
    0.299 * pixel[0] as f32 / CHANNEL_MAX
        + 0.587 * pixel[1] as f32 / CHANNEL_MAX
        + 0.114 * pixel[2] as f32 / CHANNEL_MAX
}

fn lab(pixel: [u16; 4]) -> [f32; 3] {
    srgb_to_lab(
        pixel[0] as f32 / CHANNEL_MAX,
        pixel[1] as f32 / CHANNEL_MAX,
        pixel[2] as f32 / CHANNEL_MAX,
    )
}

/// 在给定窗口内统计“相对 5x5 均值的绝对偏差”，作为高频纹理能量。
fn texture_energy(image: &Image16, cx: u32, cy: u32, half: u32) -> f32 {
    let mut sum = 0f32;
    let mut count = 0f32;
    let width = image.width() as i64;
    let height = image.height() as i64;
    for y in cy as i64 - half as i64..=cy as i64 + half as i64 {
        for x in cx as i64 - half as i64..=cx as i64 + half as i64 {
            if x < 0 || y < 0 || x >= width || y >= height {
                continue;
            }
            let mut mean = 0f32;
            for dy in -2i64..=2 {
                for dx in -2i64..=2 {
                    let sx = (x + dx).clamp(0, width - 1) as u32;
                    let sy = (y + dy).clamp(0, height - 1) as u32;
                    mean += luma(image.get_pixel(sx, sy).0);
                }
            }
            mean /= 25.0;
            sum += (luma(image.get_pixel(x as u32, y as u32).0) - mean).abs();
            count += 1.0;
        }
    }
    if count > 0.0 {
        sum / count
    } else {
        0.0
    }
}

/// 确定性高频纹理（±amount/255），模拟毛孔与细节。
fn add_texture(image: &mut Image16, amount: i32) {
    let (width, height) = image.dimensions();
    for y in 0..height {
        for x in 0..width {
            let noise = (((x * 13 + y * 7) % 9) as i32 - 4) * amount;
            let pixel = image.get_pixel_mut(x, y);
            for channel in 0..3 {
                pixel.0[channel] =
                    (pixel.0[channel] as i32 + noise).clamp(0, 65535) as u16;
            }
        }
    }
}

fn beauty_params(beauty: BeautyParams) -> EditParams {
    EditParams {
        faces: vec![FaceBeauty {
            face_id: 0,
            face_uuid: String::new(),
            enabled: true,
            beauty,
        }],
        ..Default::default()
    }
}

fn apply_single_face(
    image: &mut Image16,
    face: &FaceFrame,
    beauty: BeautyParams,
    manual: ManualMask,
    repair_points: &[RepairPoint],
) -> (SkinMask, lumi_retouch_lib::ai_engine::retouch::FaceRetouchReport) {
    let mask = build_skin_mask(image, face, &manual).unwrap();
    let geometry = FaceGeometry::from_face(face, image.width(), image.height()).unwrap();
    let report = apply_face_retouch(
        image,
        &mask,
        &geometry,
        &beauty,
        0.0,
        repair_points,
        &CancelToken::default(),
    )
    .unwrap();
    (mask, report)
}

#[test]
fn skin_mask_covers_face_and_excludes_hair_eyes_mouth_and_clothes() {
    let (width, height) = (400u32, 500u32);
    let mut image = skin_image(width, height);
    // 两侧头发（深色）、底部衣服、右上背景。
    for y in 0..height {
        for x in 0..width {
            let hair = x < 60 || x > 340;
            let clothes = y > 400;
            if hair || clothes {
                image.put_pixel(x, y, Rgba([40 * 257, 38 * 257, 36 * 257, 65535]));
            }
        }
    }
    let face = face_frame(0, 200.0, 180.0, 80.0, width, height);
    let mask = build_skin_mask(&image, &face, &ManualMask::default()).unwrap();
    let geometry = FaceGeometry::from_face(&face, width, height).unwrap();

    let forehead = geometry.from_local(0.0, -0.85);
    let cheek = geometry.from_local(0.45, 0.35);
    assert!(
        mask.at(forehead.0 as u32, forehead.1 as u32) > 0.5,
        "额头应在皮肤蒙版内"
    );
    assert!(
        mask.at(cheek.0 as u32, cheek.1 as u32) > 0.5,
        "脸颊应在皮肤蒙版内"
    );

    for (eye_x, eye_y) in geometry.eyes {
        assert!(
            mask.at(eye_x as u32, eye_y as u32) < 0.05,
            "眼睛必须被保护"
        );
    }
    let mouth = geometry.from_local(0.0, 0.95);
    assert!(
        mask.at(mouth.0 as u32, mouth.1 as u32) < 0.05,
        "嘴唇必须被保护"
    );

    // 头发/衣服的深色像素不应被当成皮肤。
    assert!(mask.at(20, 180) < 0.1, "头发区域不应进入蒙版");
    assert!(mask.at(200, 450) < 0.1, "衣服区域不应进入蒙版");
}

#[test]
fn smoothing_reduces_texture_but_retains_grain() {
    let (width, height) = (400u32, 500u32);
    let mut image = skin_image(width, height);
    add_texture(&mut image, 2600);
    let face = face_frame(0, 200.0, 180.0, 80.0, width, height);
    let original = image.clone();

    let cheek = (236u32, 208u32);
    let before = texture_energy(&original, cheek.0, cheek.1, 12);

    let _ = apply_single_face(
        &mut image,
        &face,
        BeautyParams {
            skin_smooth: 70.0,
            texture: 60.0,
            detail: 30.0,
            ..Default::default()
        },
        ManualMask::default(),
        &[],
    );
    let after = texture_energy(&image, cheek.0, cheek.1, 12);

    assert!(after < before * 0.95, "磨皮应降低高频能量：{before} → {after}");
    assert!(
        after > before * 0.15,
        "纹理保护必须保留可见细节，不能磨成塑料：{before} → {after}"
    );
}

#[test]
fn blemish_repair_lifts_dark_spots_conservatively() {
    let (width, height) = (400u32, 500u32);
    let mut image = skin_image(width, height);
    let face = face_frame(0, 200.0, 180.0, 80.0, width, height);
    let spots = [(238u32, 212u32), (252, 228), (236, 240)];
    for (x, y) in spots {
        for dy in -3i32..=3 {
            for dx in -3i32..=3 {
                if dx * dx + dy * dy <= 9 {
                    image.put_pixel(
                        (x as i32 + dx) as u32,
                        (y as i32 + dy) as u32,
                        Rgba([120 * 257, 96 * 257, 84 * 257, 65535]),
                    );
                }
            }
        }
    }
    let before: Vec<f32> = spots
        .iter()
        .map(|(x, y)| luma(image.get_pixel(*x, *y).0))
        .collect();

    let (_, report) = apply_single_face(
        &mut image,
        &face,
        BeautyParams {
            blemish: 80.0,
            ..Default::default()
        },
        ManualMask::default(),
        &[],
    );

    assert!(report.blemish_detected >= 1, "应检测到明显暗斑");
    assert!(report.blemish_repaired >= 1, "应至少修复一个暗斑");
    for (index, (x, y)) in spots.iter().enumerate() {
        let after = luma(image.get_pixel(*x, *y).0);
        assert!(
            after > before[index] + 0.08,
            "修复后暗斑应明显提亮：{} → {}",
            before[index],
            after
        );
    }
    // 周围皮肤不应被整片漂白。
    let edge = luma(image.get_pixel(260, 280).0);
    let expected = luma(original_skin_pixel());
    assert!(
        (edge - expected).abs() < 0.02,
        "非瑕疵皮肤应基本保持不变：{edge} vs {expected}"
    );
}

fn original_skin_pixel() -> [u16; 4] {
    [SKIN[0], SKIN[1], SKIN[2], 65535]
}

#[test]
fn dark_moles_are_protected_from_automatic_repair() {
    // 痣的对比度远高于普通痘印，必须被保守策略排除，不能当瑕疵抹掉。
    let (width, height) = (400u32, 500u32);
    let mut image = skin_image(width, height);
    let face = face_frame(0, 200.0, 180.0, 80.0, width, height);
    let mole = (238u32, 212u32);
    for dy in -2i32..=2 {
        for dx in -2i32..=2 {
            if dx * dx + dy * dy <= 4 {
                image.put_pixel(
                    (mole.0 as i32 + dx) as u32,
                    (mole.1 as i32 + dy) as u32,
                    Rgba([40 * 257, 30 * 257, 28 * 257, 65535]),
                );
            }
        }
    }
    let before = luma(image.get_pixel(mole.0, mole.1).0);

    let (_, report) = apply_single_face(
        &mut image,
        &face,
        BeautyParams {
            blemish: 100.0,
            spots: 100.0,
            blackheads: 100.0,
            ..Default::default()
        },
        ManualMask::default(),
        &[],
    );

    let after = luma(image.get_pixel(mole.0, mole.1).0);
    assert!(
        (after - before).abs() < 0.02,
        "痣必须保留，不能被自动祛斑抹掉：{before} → {after}"
    );
    assert_eq!(report.blemish_repaired, 0, "不应把痣计入已修复瑕疵");
}

#[test]
fn manual_repair_point_only_repairs_selected_blemish() {
    let (width, height) = (400u32, 500u32);
    let mut image = skin_image(width, height);
    let face = face_frame(0, 200.0, 180.0, 80.0, width, height);
    let selected = (238u32, 212u32);
    let untouched = (220u32, 250u32);
    for (x, y) in [selected, untouched] {
        for dy in -3i32..=3 {
            for dx in -3i32..=3 {
                if dx * dx + dy * dy <= 9 {
                    image.put_pixel(
                        (x as i32 + dx) as u32,
                        (y as i32 + dy) as u32,
                        Rgba([120 * 257, 96 * 257, 84 * 257, 65535]),
                    );
                }
            }
        }
    }
    let selected_before = luma(image.get_pixel(selected.0, selected.1).0);
    let untouched_before = luma(image.get_pixel(untouched.0, untouched.1).0);

    let points = [RepairPoint {
        x: selected.0 as f32 / width as f32,
        y: selected.1 as f32 / height as f32,
        radius: 0.04,
    }];
    let (_, report) = apply_single_face(
        &mut image,
        &face,
        BeautyParams {
            blemish: 80.0,
            ..Default::default()
        },
        ManualMask::default(),
        &points,
    );

    assert!(report.blemish_repaired >= 1);
    let selected_after = luma(image.get_pixel(selected.0, selected.1).0);
    let untouched_after = luma(image.get_pixel(untouched.0, untouched.1).0);
    assert!(
        selected_after > selected_before + 0.08,
        "点选瑕疵应被修复：{selected_before} → {selected_after}"
    );
    assert!(
        (untouched_after - untouched_before).abs() < 0.01,
        "点选范围外的瑕疵不应被改动：{untouched_before} → {untouched_after}"
    );
}

#[test]
fn skin_tone_keeps_natural_chroma_and_avoids_flat_white() {
    let (width, height) = (400u32, 500u32);
    let mut image = skin_image(width, height);
    // 一块偏红、偏暗的肤色不均区域。
    for y in 210..280 {
        for x in 210..290 {
            image.put_pixel(x, y, Rgba([228 * 257, 150 * 257, 132 * 257, 65535]));
        }
    }
    let face = face_frame(0, 200.0, 180.0, 80.0, width, height);
    let sample = (240u32, 240u32);
    let before = lab(image.get_pixel(sample.0, sample.1).0);
    let before_chroma = chroma(before[1], before[2]);

    apply_single_face(
        &mut image,
        &face,
        BeautyParams {
            tone_even: 60.0,
            whitening: 40.0,
            redness: 70.0,
            ..Default::default()
        },
        ManualMask::default(),
        &[],
    );

    let after = lab(image.get_pixel(sample.0, sample.1).0);
    assert!(after[1] < before[1], "泛红应被压低：{} → {}", before[1], after[1]);
    assert!(after[0] >= before[0] - 0.5, "美白不应让皮肤变暗");
    assert!(after[0] <= before[0] + 8.0, "美白必须温和，不能整体拉亮死白");
    let after_chroma = chroma(after[1], after[2]);
    assert!(
        after_chroma >= before_chroma * 0.34,
        "不能把肤色压成灰白：{before_chroma} → {after_chroma}"
    );
}

#[test]
fn multi_face_parameters_do_not_leak_across_faces() {
    let (width, height) = (1200u32, 600u32);
    let mut image = skin_image(width, height);
    add_texture(&mut image, 2400);
    let left = face_frame(0, 300.0, 250.0, 80.0, width, height);
    let right = face_frame(1, 900.0, 250.0, 80.0, width, height);

    let left_probe = (336u32, 278u32);
    let right_probe = (936u32, 278u32);
    let left_before = texture_energy(&image, left_probe.0, left_probe.1, 10);
    let right_before = texture_energy(&image, right_probe.0, right_probe.1, 10);

    let params = EditParams {
        faces: vec![
            FaceBeauty {
                face_id: 0,
                face_uuid: String::new(),
                enabled: true,
                beauty: BeautyParams::default(),
            },
            FaceBeauty {
                face_id: 1,
                face_uuid: String::new(),
                enabled: true,
                beauty: BeautyParams {
                    skin_smooth: 90.0,
                    texture: 20.0,
                    ..Default::default()
                },
            },
        ],
        ..Default::default()
    };
    let rendered = pipeline::render(
        &image,
        &params,
        &[left.clone(), right.clone()],
        &CancelToken::default(),
    )
    .unwrap();

    let left_after = texture_energy(&rendered, left_probe.0, left_probe.1, 10);
    let right_after = texture_energy(&rendered, right_probe.0, right_probe.1, 10);
    assert!(
        (left_after - left_before).abs() < left_before * 0.05,
        "人脸 A 未开启磨皮，纹理不应变化：{left_before} → {left_after}"
    );
    assert!(
        right_after < right_before * 0.75,
        "人脸 B 独立参数应生效：{right_before} → {right_after}"
    );

    // 单独关闭人脸 B 后，两张脸都不应改变。
    let mut disabled = params.clone();
    disabled.faces[1].enabled = false;
    let untouched = pipeline::render(
        &image,
        &disabled,
        &[left, right],
        &CancelToken::default(),
    )
    .unwrap();
    assert_eq!(untouched.into_raw(), image.into_raw());
}

#[test]
fn manual_eraser_protects_selected_area() {
    let (width, height) = (400u32, 500u32);
    let image = skin_image(width, height);
    let face = face_frame(0, 200.0, 180.0, 80.0, width, height);

    let beauty = BeautyParams {
        whitening: 100.0,
        ..Default::default()
    };
    let plain = pipeline::render(
        &image,
        &beauty_params(beauty.clone()),
        &[face.clone()],
        &CancelToken::default(),
    )
    .unwrap();

    // 用橡皮擦盖住左脸颊（图像右侧），该区域应回到接近原图。
    let erase_center = FaceGeometry::from_face(&face, width, height)
        .unwrap()
        .from_local(0.5, 0.45);
    let manual = ManualMask {
        enabled: true,
        invert: false,
        feather: 0.0,
        strength: 100.0,
        strokes: vec![MaskStroke {
            erase: true,
            radius: 0.08,
            points: vec![MaskPoint {
                x: erase_center.0 / width as f32,
                y: erase_center.1 / height as f32,
            }],
        }],
    };
    let mut params = beauty_params(beauty);
    params.manual_mask = manual;
    let masked = pipeline::render(&image, &params, &[face], &CancelToken::default()).unwrap();

    let erase_x = erase_center.0 as u32;
    let erase_y = erase_center.1 as u32;
    let protected = luma(masked.get_pixel(erase_x, erase_y).0);
    let base = luma(image.get_pixel(erase_x, erase_y).0);
    let processed = luma(plain.get_pixel(erase_x, erase_y).0);
    assert!(
        (protected - base).abs() < (processed - base).abs() * 0.35,
        "橡皮擦区域应基本不被美肤修改：base {base} · plain {processed} · masked {protected}"
    );
}

#[test]
fn cancelled_beauty_render_stops_with_error() {
    let (width, height) = (400u32, 500u32);
    let mut image = skin_image(width, height);
    for y in 0..height {
        for x in 0..width {
            let noise = (((x * 13 + y * 7) % 9) as i32 - 4) * 2000;
            let pixel = image.get_pixel_mut(x, y);
            for channel in 0..3 {
                pixel.0[channel] = (pixel.0[channel] as i32 + noise).clamp(0, 65535) as u16;
            }
        }
    }
    let face = face_frame(0, 200.0, 180.0, 80.0, width, height);
    let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let cancel = CancelToken::new(flag);
    let params = EditParams {
        faces: vec![FaceBeauty {
            face_id: 0,
            face_uuid: String::new(),
            enabled: true,
            beauty: BeautyParams {
                skin_smooth: 50.0,
                ..Default::default()
            },
        }],
        ..Default::default()
    };
    let error = pipeline::render(&image, &params, &[face], &cancel).unwrap_err();
    assert!(matches!(error, AppError::Cancelled));
}

#[test]
fn analysis_is_deterministic_and_bounded() {
    let (width, height) = (400u32, 500u32);
    let mut image = skin_image(width, height);
    add_texture(&mut image, 1800);
    let face = face_frame(0, 200.0, 180.0, 80.0, width, height);
    let mask = build_skin_mask(&image, &face, &ManualMask::default()).unwrap();
    let geometry = FaceGeometry::from_face(&face, width, height).unwrap();
    let cancel = CancelToken::default();

    let first = analyze_face(&image, &mask, &geometry, &cancel).unwrap();
    let second = analyze_face(&image, &mask, &geometry, &cancel).unwrap();
    assert_eq!(first.suggested, second.suggested, "分析结果必须可重复");
    assert_eq!(first.notes, second.notes);
    for value in first.suggested.fields() {
        assert!(
            (0.0..=100.0).contains(&value),
            "建议参数必须在 0..100：{value}"
        );
    }
    assert!(first.suggested.skin_smooth <= 55.0, "自然建议不应过强");
}

#[test]
fn presets_are_progressive_and_editable() {
    let light = preset("light");
    let natural = preset("natural");
    let strong = preset("strong");
    assert!(light.skin_smooth < natural.skin_smooth);
    assert!(natural.skin_smooth < strong.skin_smooth);
    for params in [&light, &natural, &strong] {
        for value in params.fields() {
            assert!((0.0..=100.0).contains(&value));
        }
    }
}

#[test]
fn full_resolution_beauty_output_keeps_dimensions_and_source() {
    let (width, height) = (1200u32, 1600u32);
    let mut image = skin_image(width, height);
    add_texture(&mut image, 1600);
    let face = face_frame(0, 600.0, 560.0, 180.0, width, height);
    let original_raw = image.clone().into_raw();

    let params = EditParams {
        faces: vec![FaceBeauty {
            face_id: 0,
            face_uuid: String::new(),
            enabled: true,
            beauty: BeautyParams {
                skin_smooth: 35.0,
                texture: 55.0,
                detail: 25.0,
                blemish: 25.0,
                tone_even: 15.0,
                ..Default::default()
            },
        }],
        ..Default::default()
    };
    let rendered = pipeline::render(&image, &params, &[face], &CancelToken::default()).unwrap();
    assert_eq!((rendered.width(), rendered.height()), (width, height));
    assert_eq!(image.into_raw(), original_raw, "渲染不得修改原始缓冲区");
}

/// 手动运行的性能基准：
/// `cargo test --release --test beauty benchmark_beauty -- --ignored --nocapture`
#[test]
#[ignore]
fn benchmark_beauty() {
    use std::time::Instant;
    let (width, height) = (1600u32, 1200u32);
    let mut image = skin_image(width, height);
    add_texture(&mut image, 2200);
    let face = face_frame(0, 800.0, 480.0, 220.0, width, height);

    let started = Instant::now();
    let mask = build_skin_mask(&image, &face, &ManualMask::default()).unwrap();
    let mask_ms = started.elapsed();

    let started = Instant::now();
    let geometry = FaceGeometry::from_face(&face, width, height).unwrap();
    let beauty = preset("natural");
    let report = apply_face_retouch(
        &mut image,
        &mask,
        &geometry,
        &beauty,
        0.0,
        &[],
        &CancelToken::default(),
    )
    .unwrap();
    let retouch_ms = started.elapsed();

    let params = EditParams {
        faces: vec![FaceBeauty {
            face_id: 0,
            face_uuid: String::new(),
            enabled: true,
            beauty,
        }],
        ..Default::default()
    };
    let started = Instant::now();
    let rendered = pipeline::render(&image, &params, &[face], &CancelToken::default()).unwrap();
    let render_ms = started.elapsed();

    let mask_pixels = mask.active_pixels();
    eprintln!(
        "beauty benchmark 1600x1200: mask {mask_ms:?} ({mask_pixels} px), retouch {retouch_ms:?}, pipeline {render_ms:?}, blemish {}/{}",
        report.blemish_repaired, report.blemish_detected
    );
    assert_eq!((rendered.width(), rendered.height()), (width, height));
}
