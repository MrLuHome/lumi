//! 精简版六项的**像素级契约测试**。
//!
//! 目的：证明「手动调整」的六项参数真的改变渲染像素，而不是界面上的假滑块。
//! 覆盖 TS 侧 `src/state/slim.ts` 的 `ITEM_PROFILE`（强度 100 的剖面），
//! 直接走 `pipeline::render`（预览与导出共用的同一条管线）。
//!
//! 特别是 ③ 眼睛美化与 ④ 暗沉（`dullness`）：仓库内此前**没有**任何像素断言，
//! 这里补上首次断言。
//!
//! 全部使用合成人像，数值可重复：

use image::Rgba;

use lumi_retouch_lib::ai_engine::face_frame::{FaceFrame, NormPoint};
use lumi_retouch_lib::image_engine::buffer::{Image16, CHANNEL_MAX};
use lumi_retouch_lib::image_engine::cancel::CancelToken;
use lumi_retouch_lib::image_engine::params::{BeautyParams, EditParams, FaceBeauty};
use lumi_retouch_lib::image_engine::pipeline;

const SKIN: [u16; 3] = [214 * 257, 170 * 257, 140 * 257];
const WIDTH: u32 = 400;
const HEIGHT: u32 = 500;

/// 合成人脸：双眼间距 80px，中心 (200, 180)，关键点沿用五点顺序。
fn face_frame() -> FaceFrame {
    let (cx, cy, d) = (200.0f32, 180.0f32, 80.0f32);
    let point = |x: f32, y: f32| NormPoint {
        x: x / WIDTH as f32,
        y: y / HEIGHT as f32,
    };
    FaceFrame {
        id: 0,
        uuid: "slim-face-0".into(),
        score: 0.95,
        x: (cx - 1.6 * d) / WIDTH as f32,
        y: (cy - 1.4 * d) / HEIGHT as f32,
        width: 3.2 * d / WIDTH as f32,
        height: 3.6 * d / HEIGHT as f32,
        landmarks: vec![
            point(cx - 0.5 * d, cy),
            point(cx + 0.5 * d, cy),
            point(cx, cy + 0.55 * d),
            point(cx - 0.35 * d, cy + 0.95 * d),
            point(cx + 0.35 * d, cy + 0.95 * d),
        ],
        dense: None,
        level3: None,
    }
}

/// 纯肤色底图。
fn skin_image() -> Image16 {
    Image16::from_pixel(WIDTH, HEIGHT, Rgba([SKIN[0], SKIN[1], SKIN[2], 65535]))
}

/// 皮肤纹理（毛孔）：确定性高频噪声，供 ① 磨皮使用。
fn with_texture(mut image: Image16) -> Image16 {
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let noise = (((x * 13 + y * 7) % 9) as i32 - 4) * 2600;
            let pixel = image.get_pixel_mut(x, y);
            for channel in 0..3 {
                pixel.0[channel] = (pixel.0[channel] as i32 + noise).clamp(0, 65535) as u16;
            }
        }
    }
    image
}

/// 三个暗斑（痘印），供 ② 祛痘使用。
fn with_spots(mut image: Image16) -> Image16 {
    for (cx, cy) in [(238i32, 212i32), (252, 228), (236, 240)] {
        for dy in -3i32..=3 {
            for dx in -3i32..=3 {
                if dx * dx + dy * dy <= 9 {
                    image.put_pixel(
                        (cx + dx) as u32,
                        (cy + dy) as u32,
                        Rgba([120 * 257, 96 * 257, 84 * 257, 65535]),
                    );
                }
            }
        }
    }
    image
}

/// 眼下暗沉椭圆（黑眼圈），位置对应 `under_eye_weight` 的中心。
fn with_under_eye(mut image: Image16) -> Image16 {
    for (ex, ey) in [(160i32, 204i32), (240, 204)] {
        for dy in -9i32..=9 {
            for dx in -16i32..=16 {
                let ratio = ((dx as f32 / 16.0).powi(2) + (dy as f32 / 9.0).powi(2)).sqrt();
                if ratio > 1.0 {
                    continue;
                }
                let pixel = image.get_pixel_mut((ex + dx) as u32, (ey + dy) as u32);
                for channel in 0..3 {
                    pixel.0[channel] = (pixel.0[channel] as f32 * 0.78) as u16;
                }
            }
        }
    }
    image
}

/// 一块偏红、偏暗的肤色不均区域：同时能触发均匀、美白、泛红与暗沉。
fn with_tone_patch(mut image: Image16) -> Image16 {
    for y in 210..280 {
        for x in 210..290 {
            image.put_pixel(x, y, Rgba([150 * 257, 110 * 257, 92 * 257, 65535]));
        }
    }
    image
}

fn beauty_params(beauty: BeautyParams) -> EditParams {
    EditParams {
        faces: vec![FaceBeauty {
            face_id: 0,
            face_uuid: "slim-face-0".into(),
            enabled: true,
            beauty,
        }],
        ..Default::default()
    }
}

fn render(image: &Image16, params: &EditParams) -> Image16 {
    // 与预览/导出完全同一条管线；faces 必须非空，否则美肤会被静默跳过。
    pipeline::render(image, params, &[face_frame()], &CancelToken::default()).unwrap()
}

/// 区域内逐像素三通道平均绝对差，单位是 0..1 的归一化亮度。
fn mean_abs_diff(a: &Image16, b: &Image16, rect: (u32, u32, u32, u32)) -> f32 {
    let (x0, y0, x1, y1) = rect;
    let mut sum = 0f64;
    let mut count = 0f64;
    for y in y0..y1.min(HEIGHT) {
        for x in x0..x1.min(WIDTH) {
            let left = a.get_pixel(x, y).0;
            let right = b.get_pixel(x, y).0;
            for channel in 0..3 {
                sum += ((left[channel] as f32 - right[channel] as f32).abs() / CHANNEL_MAX) as f64;
            }
            count += 3.0;
        }
    }
    if count <= 0.0 {
        return 0.0;
    }
    (sum / count) as f32
}

const WHOLE: (u32, u32, u32, u32) = (0, 0, WIDTH, HEIGHT);
const CHEEK: (u32, u32, u32, u32) = (222, 196, 254, 224);
const SPOTS: (u32, u32, u32, u32) = (228, 204, 262, 250);
const UNDER_EYE: (u32, u32, u32, u32) = (140, 192, 260, 218);
const TONE_PATCH: (u32, u32, u32, u32) = (212, 212, 288, 278);

#[test]
fn item_skin_smoothing_changes_cheek_pixels() {
    let base = with_texture(skin_image());
    let params = beauty_params(BeautyParams {
        skin_smooth: 70.0,
        texture: 55.0,
        detail: 28.0,
        ..Default::default()
    });
    let rendered = render(&base, &params);
    let diff = mean_abs_diff(&base, &rendered, CHEEK);
    println!("item 1 skin smoothing: ROI mean |Δ| = {diff:.5}");
    assert!(diff > 0.002, "磨皮美肤必须改变脸颊像素，实测 {diff}");
    // 皮肤蒙版之外（画面左侧头发/背景区）不应有任何变化。
    let outside = mean_abs_diff(&base, &rendered, (0, 0, 60, HEIGHT));
    assert!(outside < 0.0001, "蒙版外不应被改动，实测 {outside}");
}

#[test]
fn item_blemish_repair_changes_spot_pixels() {
    let base = with_spots(skin_image());
    let params = beauty_params(BeautyParams {
        blemish: 90.0,
        spots: 60.0,
        blackheads: 35.0,
        ..Default::default()
    });
    let rendered = render(&base, &params);
    let diff = mean_abs_diff(&base, &rendered, SPOTS);
    println!("item 2 blemish: ROI mean |Δ| = {diff:.5}");
    assert!(diff > 0.002, "祛痘祛斑必须改变痘印像素，实测 {diff}");
    let outside = mean_abs_diff(&base, &rendered, (0, 0, 60, HEIGHT));
    assert!(outside < 0.0001, "蒙版外不应被改动，实测 {outside}");
}

#[test]
fn item_eye_area_changes_under_eye_pixels() {
    let base = with_under_eye(skin_image());
    let params = beauty_params(BeautyParams {
        dark_circles: 90.0,
        eye_lines: 70.0,
        ..Default::default()
    });
    let rendered = render(&base, &params);
    let diff = mean_abs_diff(&base, &rendered, UNDER_EYE);
    println!("item 3 eyes: ROI mean |Δ| = {diff:.5}");
    assert!(diff > 0.002, "眼睛美化必须改变眼下像素，实测 {diff}");
    // 眼睛美化只处理眼周：脸颊与蒙版外都不该动。
    let cheek = mean_abs_diff(&base, &rendered, (300, 300, 340, 340));
    assert!(cheek < 0.002, "眼周处理不应扩散到下巴区域，实测 {cheek}");
}

#[test]
fn item_skin_tone_changes_patch_pixels_including_dullness() {
    let base = with_tone_patch(skin_image());
    let params = beauty_params(BeautyParams {
        whitening: 50.0,
        tone_even: 65.0,
        dullness: 45.0,
        redness: 40.0,
        ..Default::default()
    });
    let rendered = render(&base, &params);
    let diff = mean_abs_diff(&base, &rendered, TONE_PATCH);
    println!("item 4 tone: ROI mean |Δ| = {diff:.5}");
    assert!(diff > 0.002, "肤色提亮必须改变肤色不均区域，实测 {diff}");

    // 单独验证 dullness（此前仓库内没有断言）：暗沉区域必须被提亮。
    let dull_only = render(
        &base,
        &beauty_params(BeautyParams {
            dullness: 55.0,
            ..Default::default()
        }),
    );
    let dull_diff = mean_abs_diff(&base, &dull_only, TONE_PATCH);
    println!("item 4 dullness only: ROI mean |Δ| = {dull_diff:.5}");
    assert!(dull_diff > 0.0005, "暗沉改善必须改变像素，实测 {dull_diff}");
}

#[test]
fn item_light_and_color_change_whole_image() {
    let base = with_texture(skin_image());
    let light = render(
        &base,
        &EditParams {
            exposure: 18.0,
            contrast: -8.0,
            highlights: -12.0,
            shadows: 20.0,
            ..Default::default()
        },
    );
    let light_diff = mean_abs_diff(&base, &light, WHOLE);
    println!("item 5 light: whole-image mean |Δ| = {light_diff:.5}");
    assert!(light_diff > 0.005, "光影调整必须改变整图，实测 {light_diff}");

    let color = render(
        &base,
        &EditParams {
            temperature: 15.0,
            tint: 8.0,
            saturation: -10.0,
            vibrance: 12.0,
            ..Default::default()
        },
    );
    let color_diff = mean_abs_diff(&base, &color, WHOLE);
    println!("item 6 color: whole-image mean |Δ| = {color_diff:.5}");
    assert!(color_diff > 0.002, "照片色调必须改变整图，实测 {color_diff}");
}

#[test]
fn neutral_params_leave_pixels_untouched() {
    // 强度 0 必须真的不改像素：这是「回到中性 → 预览复用原图」的前提。
    let base = with_texture(skin_image());
    let rendered = render(&base, &EditParams::default());
    assert_eq!(mean_abs_diff(&base, &rendered, WHOLE), 0.0);
}

#[test]
fn forehead_lines_and_nasolabial_are_not_part_of_the_six_items() {
    // 主理人裁决 Q1：一键精修/手动六项都不写这两项，值为 0 时像素必须完全不变。
    let base = with_texture(skin_image());
    let rendered = render(
        &base,
        &beauty_params(BeautyParams {
            forehead_lines: 0.0,
            nasolabial: 0.0,
            ..Default::default()
        }),
    );
    assert_eq!(mean_abs_diff(&base, &rendered, WHOLE), 0.0);
}
