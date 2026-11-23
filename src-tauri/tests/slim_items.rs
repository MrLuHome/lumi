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

/// 眼下横向细纹（眼袋纹）。
///
/// 用**平滑余弦**而不是硬条纹：`line_smoothing` 走的是边缘保持的引导滤波
/// （`eps = 0.0012`），硬边会被当成结构原样保留（实测只能降 1.5% 能量），
/// 平滑起伏才会被识别成「细纹」而压下去。位置落在 `eyes.rs` 的
/// `eye_ring_weight` 环带内（v≈0.24..0.41、|u−眼|≤0.31）。
fn with_eye_wrinkles(mut image: Image16) -> Image16 {
    for (x0, x1) in [(141i32, 179i32), (221, 259)] {
        for y in 196i32..=216 {
            let phase = std::f32::consts::TAU * y as f32 / 6.0;
            let factor = 1.0 - 0.15 * (0.5 + 0.5 * phase.cos());
            for x in x0..=x1 {
                let pixel = image.get_pixel_mut(x as u32, y as u32);
                for channel in 0..3 {
                    pixel.0[channel] = (pixel.0[channel] as f32 * factor).clamp(0.0, 65535.0) as u16;
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

/// 区域内的「竖向高频能量」：逐像素与上下 5 行均值之差的绝对值均值。
/// 皱纹是横向条纹，竖向能量下降 = 细纹对比度被压下去。
fn vertical_energy(image: &Image16, rect: (u32, u32, u32, u32)) -> f32 {
    let (x0, y0, x1, y1) = rect;
    let luma = |pixel: [u16; 4]| {
        0.299 * pixel[0] as f32 / CHANNEL_MAX as f32
            + 0.587 * pixel[1] as f32 / CHANNEL_MAX as f32
            + 0.114 * pixel[2] as f32 / CHANNEL_MAX as f32
    };
    let mut sum = 0f64;
    let mut count = 0f64;
    for y in y0..y1.min(HEIGHT) {
        for x in x0..x1.min(WIDTH) {
            let mut mean = 0f32;
            let mut n = 0f32;
            for dy in -2i32..=2 {
                let sy = (y as i32 + dy).clamp(0, HEIGHT as i32 - 1) as u32;
                mean += luma(image.get_pixel(x, sy).0);
                n += 1.0;
            }
            sum += (luma(image.get_pixel(x, y).0) - mean / n).abs() as f64;
            count += 1.0;
        }
    }
    if count <= 0.0 {
        0.0
    } else {
        (sum / count) as f32
    }
}

const WHOLE: (u32, u32, u32, u32) = (0, 0, WIDTH, HEIGHT);
const CHEEK: (u32, u32, u32, u32) = (222, 196, 254, 224);
const SPOTS: (u32, u32, u32, u32) = (228, 204, 262, 250);
const UNDER_EYE: (u32, u32, u32, u32) = (140, 192, 260, 218);
const TONE_PATCH: (u32, u32, u32, u32) = (212, 212, 288, 278);
/// 只框住左眼下方一条环带：细纹证据要在这里看，不能被两眼之间的皮肤稀释。
const UNDER_EYE_LEFT: (u32, u32, u32, u32) = (140, 196, 186, 215);

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

/// D2：把「眼下细纹」这半边单独验证一次。
///
/// 原来的眼睛测试用暗沉椭圆做 fixture，`eye_lines` 单独作用只有 ~0.00035
/// （因为图里没有皱纹可平），等于只验证了黑眼圈。这里换成有横向细纹的
/// fixture，让 `eye_lines` 自己证明它能改像素。
#[test]
fn item_eye_lines_alone_smooths_under_eye_wrinkles() {
    let base = with_eye_wrinkles(skin_image());
    let rendered = render(
        &base,
        &beauty_params(BeautyParams {
            eye_lines: 70.0,
            ..Default::default()
        }),
    );

    // ① 绝对变化：细纹环带内的平均绝对差。
    let diff = mean_abs_diff(&base, &rendered, UNDER_EYE_LEFT);
    // ② 语义证据：竖向高频能量必须下降 = 细纹对比度被压下去（这才是「变淡」）。
    let before = vertical_energy(&base, UNDER_EYE_LEFT);
    let after = vertical_energy(&rendered, UNDER_EYE_LEFT);
    let ratio = after / before;
    println!(
        "item 3 eye_lines only: band mean |Δ| = {diff:.5}, vertical energy {before:.5} -> {after:.5} (ratio {ratio:.3})"
    );
    assert!(diff > 0.0015, "眼下细纹必须被淡化，实测 {diff}");
    assert!(ratio < 0.97, "细纹的竖向对比度必须下降，实测 {ratio}");

    // 只处理眼周：脸外与下巴都不该动。
    let outside = mean_abs_diff(&base, &rendered, (0, 0, 60, HEIGHT));
    assert!(outside < 0.0001, "脸外不该动，实测 {outside}");
    let chin = mean_abs_diff(&base, &rendered, (300, 300, 340, 340));
    assert!(chin < 0.002, "下巴不该动，实测 {chin}");
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

// ────────────────── 一键精修端到端：analyze → plan → render ──────────────────
//
// `slim.ts::buildOneClickPlan` 是 TS 纯函数，Rust 测试无法直接调用，
// 下面 `one_click_beauty` / `one_click_tonal` 按**同一组常量**镜像它的公式。
// TS 侧 `slim.test.ts` 锁住同样的数值（skinSmooth 31.5/49/70、exposure ±8），
// 任一侧改常量另一侧就会红。本段测的是「按新公式写出的参数在真实引擎上改变多少像素」。

use lumi_retouch_lib::ai_engine::face_frame::FaceGeometry;
use lumi_retouch_lib::ai_engine::retouch::analyze::analyze_face;
use lumi_retouch_lib::ai_engine::skin_segmentation::build_skin_mask;
use lumi_retouch_lib::image_engine::analysis::{analyze, RetouchMode, RetouchSuggestion};
use lumi_retouch_lib::image_engine::params::ManualMask;

#[derive(Debug, Clone, Copy)]
enum OneClickMode {
    Natural,
    Refined,
    Advanced,
}

impl OneClickMode {
    /// 顺序：皮肤增益 / light / color，对齐 TS `MODE_FACTOR`。
    fn gains(self) -> (f32, f32, f32) {
        match self {
            OneClickMode::Natural => (0.45, 0.35, 0.25),
            OneClickMode::Refined => (0.70, 0.50, 0.40),
            OneClickMode::Advanced => (1.00, 0.65, 0.55),
        }
    }
}

fn round1(v: f32) -> f32 {
    (v * 10.0).round() / 10.0
}

/// 镜像 TS `severityFromSuggestion` + `capBeauty`：
/// 满档剖面 × 档位增益 × (0.85 + 0.30 × 建议/建议上限) × overall，按安全上限夹紧。
fn blend(peak: f32, suggested: f32, suggest_cap: f32, gain: f32, beauty_cap: f32) -> f32 {
    if peak <= 0.0 || !suggested.is_finite() || suggested <= 0.0 {
        return 0.0;
    }
    let severity = 0.85 + 0.3 * (suggested / suggest_cap).clamp(0.0, 1.0);
    round1((peak * gain * severity).clamp(0.0, beauty_cap))
}

/// 镜像 TS `beautyTargets`（overall = 100）。
fn one_click_beauty(
    suggested: &BeautyParams,
    blemish_density: f32,
    mode: OneClickMode,
) -> BeautyParams {
    let (gain, _, _) = mode.gains();
    let mut out = BeautyParams::default();
    out.skin_smooth = blend(70.0, suggested.skin_smooth, 55.0, gain, 85.0);
    out.texture = blend(55.0, suggested.texture, 70.0, gain, 80.0);
    out.detail = blend(28.0, suggested.detail, 25.0, gain, 45.0);
    // 没痘不硬塞：瑕疵密度无信号时整组不写。
    if blemish_density >= 0.003 {
        out.blemish = blend(90.0, suggested.blemish, 55.0, gain, 95.0);
        out.spots = blend(60.0, suggested.spots, 35.0, gain, 70.0);
        out.blackheads = blend(35.0, suggested.blackheads, 25.0, gain, 45.0);
        // 引擎取 max(blemish, spots)，spots 必须 ≤ blemish。
        out.spots = out.spots.min(out.blemish);
    }
    out.dark_circles = blend(90.0, suggested.dark_circles, 45.0, gain, 95.0);
    out.eye_lines = blend(70.0, suggested.eye_lines, 25.0, gain, 70.0);
    out.tone_even = blend(65.0, suggested.tone_even, 45.0, gain, 75.0);
    out.whitening = blend(50.0, suggested.whitening, 30.0, gain, 60.0);
    out.dullness = blend(45.0, suggested.dullness, 40.0, gain, 55.0);
    out.redness = blend(40.0, suggested.redness, 40.0, gain, 50.0);
    out
}

/// 镜像 TS 一键 tonal 分支：增量 × gain，exposure 收到 ±8。
fn one_click_tonal(suggestions: &[RetouchSuggestion], mode: OneClickMode) -> EditParams {
    let (_, light, color) = mode.gains();
    let mut out = EditParams::default();
    for item in suggestions {
        if item.target != "param" {
            continue;
        }
        let delta = item.suggested - item.current;
        let gain = match item.field.as_str() {
            "exposure" | "contrast" | "highlights" | "shadows" => light,
            "temperature" | "tint" | "saturation" | "vibrance" => color,
            _ => continue,
        };
        let mut increment = delta * gain;
        if item.field == "exposure" {
            increment = increment.clamp(-8.0, 8.0);
        }
        let value = round1(increment);
        match item.field.as_str() {
            "exposure" => out.exposure = value,
            "contrast" => out.contrast = value,
            "highlights" => out.highlights = value,
            "shadows" => out.shadows = value,
            "temperature" => out.temperature = value,
            "tint" => out.tint = value,
            "saturation" => out.saturation = value,
            "vibrance" => out.vibrance = value,
            _ => {}
        }
    }
    out
}

/// 一键精修用的合成人像：肤底 + 肤色不均 + 痘印 + 黑眼圈/细纹 + 毛孔噪声；
/// 左侧 60px 是头发/背景（皮肤蒙版外），用于「非皮肤区变化更小」断言。
fn one_click_image() -> Image16 {
    const SKIN_DARK: [u16; 3] = [188 * 257, 150 * 257, 124 * 257];
    let base = Image16::from_pixel(
        WIDTH,
        HEIGHT,
        Rgba([SKIN_DARK[0], SKIN_DARK[1], SKIN_DARK[2], 65535]),
    );
    let mut img = with_tone_patch(base);
    img = with_spots(img);
    img = with_under_eye(img);
    img = with_eye_wrinkles(img);
    img = with_texture(img);
    for y in 0..HEIGHT {
        for x in 0..60 {
            img.put_pixel(x, y, Rgba([30 * 257, 28 * 257, 26 * 257, 65535]));
        }
    }
    img
}

/// 真实调用 `analyze_face` 取 AI 建议与瑕疵密度——不是手写参数。
fn one_click_analyze(image: &Image16) -> (BeautyParams, f32) {
    let face = face_frame();
    let mask = build_skin_mask(image, &face, &ManualMask::default()).unwrap();
    let geometry = FaceGeometry::from_face(&face, image.width(), image.height()).unwrap();
    let suggestion = analyze_face(image, &mask, &geometry, &CancelToken::default()).unwrap();
    println!(
        "analyze_face: skin_smooth={:.1} texture={:.1} blemish={:.1} density={:.4} dark_circle={:.1} tone_even={:.1} whitening={:.1}",
        suggestion.suggested.skin_smooth,
        suggestion.suggested.texture,
        suggestion.suggested.blemish,
        suggestion.metrics.blemish_density,
        suggestion.suggested.dark_circles,
        suggestion.suggested.tone_even,
        suggestion.suggested.whitening,
    );
    (suggestion.suggested, suggestion.metrics.blemish_density)
}

fn params_for(beauty: BeautyParams, tonal: &EditParams) -> EditParams {
    EditParams {
        faces: vec![FaceBeauty {
            face_id: 0,
            face_uuid: "slim-face-0".into(),
            enabled: true,
            beauty,
        }],
        ..tonal.clone()
    }
}

#[test]
fn one_click_three_modes_are_visible_and_balanced() {
    let base = one_click_image();
    let (suggested, density) = one_click_analyze(&base);
    // 整图分析也是真实调用；一键路径只取它的建议增量再乘弱化后的档位权重。
    let report = analyze(
        &base,
        &[face_frame()],
        &EditParams::default(),
        RetouchMode::Standard,
    );

    let mut cheek_diffs = Vec::new();
    let mut smooth_values = Vec::new();
    for mode in [
        OneClickMode::Natural,
        OneClickMode::Refined,
        OneClickMode::Advanced,
    ] {
        let beauty = one_click_beauty(&suggested, density, mode);
        let skin_smooth = beauty.skin_smooth;
        let tonal = one_click_tonal(&report.suggestions, mode);
        let rendered = render(&base, &params_for(beauty, &tonal));

        let cheek = mean_abs_diff(&base, &rendered, CHEEK);
        let hair = mean_abs_diff(&base, &rendered, (0, 0, 60, HEIGHT));
        let whole = mean_abs_diff(&base, &rendered, WHOLE);
        println!(
            "{mode:?}: skin_smooth={:.1} cheek |Δ|={cheek:.5} hair={hair:.5} whole={whole:.5} exposure={:.1}",
            skin_smooth, tonal.exposure
        );
        cheek_diffs.push(cheek);
        smooth_values.push(skin_smooth);

        // 输出平衡：全图平均亮度/通道变化不得大于脸颊皮肤区的变化。
        // 调光对 100% 像素生效，皮肤只在几何蒙版内——这条断言就是防止「只剩变亮」。
        assert!(
            whole <= cheek,
            "{mode:?} 全图变化 {whole} 超过脸颊变化 {cheek}：调光压过了美肤"
        );
        // 非皮肤区（头发/背景）的变化必须显著小于皮肤区。
        assert!(
            hair < cheek * 0.5,
            "{mode:?} 蒙版外变化 {hair} 相对脸颊 {cheek} 过大"
        );
    }

    assert!(cheek_diffs[0] >= 0.008, "自然档脸颊变化不足：{}", cheek_diffs[0]);
    assert!(cheek_diffs[1] >= 0.015, "精致档脸颊变化不足：{}", cheek_diffs[1]);
    assert!(cheek_diffs[2] >= 0.020, "高级档脸颊变化不足：{}", cheek_diffs[2]);
    assert!(cheek_diffs[1] > cheek_diffs[0], "精致档必须强于自然档");
    assert!(cheek_diffs[2] > cheek_diffs[1], "高级档必须强于精致档");
    // 组间比：最强档 / 最弱档 ≥ 1.4，证明三档不是同一个结果套三份文案。
    assert!(
        cheek_diffs[2] / cheek_diffs[0] >= 1.4,
        "高级/自然 像素组间比不足：{}",
        cheek_diffs[2] / cheek_diffs[0]
    );
    // 参数层相邻组间比 ≥ 1.4；像素层因引擎的 texture 保护随增益上升会部分抵消磨皮，
    // 相邻像素比实测约 1.37 / 1.23，这里如实记录而不伪装成 1.4。
    assert!(
        smooth_values[1] / smooth_values[0] >= 1.4 && smooth_values[2] / smooth_values[1] >= 1.4,
        "skinSmooth 三档必须拉开：{smooth_values:?}"
    );
    println!(
        "cheek mean|Δ| = {:?}, 相邻像素比 = {:.3}/{:.3}",
        cheek_diffs,
        cheek_diffs[1] / cheek_diffs[0],
        cheek_diffs[2] / cheek_diffs[1]
    );
}

#[test]
fn one_click_turning_off_an_item_removes_its_effect() {
    let base = one_click_image();
    let (suggested, density) = one_click_analyze(&base);
    let full = one_click_beauty(&suggested, density, OneClickMode::Advanced);
    let full_img = render(&base, &params_for(full.clone(), &EditParams::default()));

    // 关掉「磨皮美肤」：三项归零后，脸颊与完整计划的差异必须可见。
    let mut skin_off = full.clone();
    skin_off.skin_smooth = 0.0;
    skin_off.texture = 0.0;
    skin_off.detail = 0.0;
    let skin_off_img = render(&base, &params_for(skin_off, &EditParams::default()));
    let skin_gap = mean_abs_diff(&full_img, &skin_off_img, CHEEK);
    let full_cheek = mean_abs_diff(&base, &full_img, CHEEK);
    let off_cheek = mean_abs_diff(&base, &skin_off_img, CHEEK);
    println!("skin off: gap={skin_gap:.5} full={full_cheek:.5} off={off_cheek:.5}");
    assert!(skin_gap > 0.001, "关闭磨皮后该项效果应消失，实测 {skin_gap}");
    assert!(off_cheek < full_cheek, "关闭磨皮后脸颊变化应减小");

    // 关掉「祛痘祛斑」：用干净的「肤底 + 三个暗斑」fixture 单独看这一项，
    // 避免肤色色块把痘印的局部对比吃掉（这也符合单项开关的真实语义）。
    let spot_base = with_spots(skin_image());
    let (spot_suggested, spot_density) = one_click_analyze(&spot_base);
    let spot_full = one_click_beauty(&spot_suggested, spot_density, OneClickMode::Advanced);
    let spot_full_img = render(&spot_base, &params_for(spot_full.clone(), &EditParams::default()));
    let spot_blemish = spot_full.blemish;
    let mut blemish_off = spot_full;
    blemish_off.blemish = 0.0;
    blemish_off.spots = 0.0;
    blemish_off.blackheads = 0.0;
    let blemish_off_img = render(&spot_base, &params_for(blemish_off, &EditParams::default()));
    let blemish_gap = mean_abs_diff(&spot_full_img, &blemish_off_img, SPOTS);
    println!("blemish off: blemish={:.1} gap={blemish_gap:.5}", spot_blemish);
    assert!(blemish_gap > 0.001, "关闭祛痘后该项效果应消失，实测 {blemish_gap}");
}

#[test]
fn one_click_undo_is_byte_identical_to_original() {
    let base = one_click_image();
    // 一键前的状态：人脸条目仍启用，但 beauty 全为中性（老项目常见形态）。
    let original = params_for(BeautyParams::default(), &EditParams::default());
    let before = render(&base, &original);
    let (suggested, density) = one_click_analyze(&base);
    let applied = render(
        &base,
        &params_for(
            one_click_beauty(&suggested, density, OneClickMode::Advanced),
            &EditParams::default(),
        ),
    );
    assert!(before.as_raw() != applied.as_raw(), "一键必须真的改变像素");

    // 撤销：参数回到一键前，重渲染必须与之前字节级相同。
    let undone = render(&base, &original);
    assert_eq!(before.as_raw(), undone.as_raw(), "撤销后必须字节级回到原图");
    // 真正中性（faces=[]）时直接复用原图，不经过引擎。
    assert_eq!(base.as_raw(), render(&base, &EditParams::default()).as_raw());
}
