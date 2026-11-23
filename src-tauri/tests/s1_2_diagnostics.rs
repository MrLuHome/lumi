//! S1.2 一键精修端到端诊断（三阶段，由 `scripts/s1_2_one_click_diagnostics.sh` 驱动）。
//!
//! 阶段（缺照片时 panic，绝不静默假绿；默认 `#[ignore]` = 未执行态）：
//! 1. `phase_dump`   —— 真实检测 + 逐脸美肤分析 + 整图分析 → `{stem}.input.json`
//! 2. [vitest 桥]    —— 用**真实** `buildOneClickPlan`/`applyOneClickPlan` 生成三档参数
//! 3. `phase_render` —— 用真实 `pipeline::render_with_report` 渲染，输出逐环节诊断 +
//!                      皮肤/保护/整体像素统计 + 原图/结果/100%/200% 对比图
//!
//! 诊断数据只含数值与 UUID，不含照片内容。

use std::path::{Path, PathBuf};
use std::time::Instant;

use image::{imageops::FilterType, Rgba, RgbaImage};
use serde_json::{json, Value};

use lumi_retouch_lib::ai::face::FaceDetectionResult;
use lumi_retouch_lib::ai::model::MODEL_FILE;
use lumi_retouch_lib::ai::FaceEngine;
use lumi_retouch_lib::image_engine::analysis::{self, RetouchMode};
use lumi_retouch_lib::ai_engine::face_frame::{FaceFrame, FaceGeometry};
use lumi_retouch_lib::ai_engine::retouch::analyze::analyze_face;
use lumi_retouch_lib::ai_engine::retouch::quality;
use lumi_retouch_lib::ai_engine::retouch::texture_protect;
use lumi_retouch_lib::ai_engine::skin_segmentation::build_skin_mask;
use lumi_retouch_lib::face_reshape::ai_slim::{plan_face_slim, SlimStyle};
use lumi_retouch_lib::face_reshape::contour::analyze_face_contour;
use lumi_retouch_lib::image_engine::buffer::{fit_within, Image16};
use lumi_retouch_lib::image_engine::cancel::CancelToken;
use lumi_retouch_lib::image_engine::decoder;
use lumi_retouch_lib::image_engine::params::{
    BeautyParams, EditParams, FaceReshape, ManualMask,
};
use lumi_retouch_lib::image_engine::pipeline;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("parent")
        .to_path_buf()
}

fn model_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources/models/yunet")
        .join(MODEL_FILE)
}

fn out_dir() -> PathBuf {
    let dir = repo_root().join("docs/assets/s1_2");
    std::fs::create_dir_all(&dir).expect("创建诊断输出目录");
    dir
}

fn require_phase(expected: &str) {
    let phase = std::env::var("S1_2_PHASE").unwrap_or_default();
    assert_eq!(
        phase, expected,
        "S1_2_PHASE={phase:?} 与本测试阶段 {expected:?} 不符（诊断由驱动脚本按阶段调用）"
    );
}

/// 任务书要求的照片类别；缺任何一张 → panic「缺少照片导致未执行」。
fn required_photos() -> Vec<(&'static str, PathBuf)> {
    let root = repo_root().join("beauty-validation");
    let specs = [
        ("acne", "01-acne/DSCF0030.JPG"),
        ("rough", "02-rough-tone/DSCF0053.JPG"),
        ("uneven_tone", "04-skin-tone/DSCF0029.JPG"),
        ("good_skin", "03-good-skin/DSCF0057.JPG"),
    ];
    let mut out = Vec::new();
    for (category, rel) in specs {
        let path = root.join(rel);
        assert!(
            path.is_file(),
            "缺少照片导致未执行：{}（类别 {category}）",
            path.display()
        );
        out.push((category, path));
    }
    out
}

fn to_rgba(image: &Image16) -> RgbaImage {
    let mut out = RgbaImage::new(image.width(), image.height());
    for y in 0..image.height() {
        for x in 0..image.width() {
            let p = image.get_pixel(x, y).0;
            out.put_pixel(
                x,
                y,
                Rgba([
                    (p[0] / 257) as u8,
                    (p[1] / 257) as u8,
                    (p[2] / 257) as u8,
                    255,
                ]),
            );
        }
    }
    out
}

fn side_by_side(a: &RgbaImage, b: &RgbaImage) -> RgbaImage {
    let gap = 6u32;
    let w = a.width() + b.width() + gap;
    let h = a.height().max(b.height());
    let mut out = RgbaImage::from_pixel(w, h, Rgba([18, 18, 18, 255]));
    for y in 0..a.height() {
        for x in 0..a.width() {
            out.put_pixel(x, y, *a.get_pixel(x, y));
        }
    }
    for y in 0..b.height() {
        for x in 0..b.width() {
            out.put_pixel(x + a.width() + gap, y, *b.get_pixel(x, y));
        }
    }
    out
}

fn zoom_crop(img: &RgbaImage, cx: u32, cy: u32, size: u32, factor: u32) -> RgbaImage {
    let half = size / 2;
    let x0 = cx.saturating_sub(half);
    let y0 = cy.saturating_sub(half);
    let x1 = (x0 + size).min(img.width());
    let y1 = (y0 + size).min(img.height());
    let x0 = x0.min(x1.saturating_sub(1));
    let y0 = y0.min(y1.saturating_sub(1));
    let crop = image::imageops::crop_imm(img, x0, y0, x1 - x0, y1 - y0).to_image();
    image::imageops::resize(
        &crop,
        (x1 - x0) * factor,
        (y1 - y0) * factor,
        FilterType::Nearest,
    )
}

fn stem_of(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// 与 `commands/faces.rs::detect_faces` 同步骤：检测 → 归一化 Frame（UUID 留空，靠稳定 id 对齐）。
fn detect_frames(
    preview: &Image16,
    full_w: u32,
    full_h: u32,
) -> (FaceDetectionResult, Vec<FaceFrame>) {
    let model = model_path();
    assert!(model.is_file(), "缺少人脸模型 {}", model.display());
    let mut result = FaceEngine::default()
        .detect(&model, preview, full_w, full_h, &mut |_, _| {})
        .expect("人脸检测执行失败");
    let frames: Vec<FaceFrame> = result
        .faces
        .iter()
        .map(|face| FaceFrame::from_detected(face, full_w, full_h))
        .collect();
    // 与 detect_faces 一致：把稳定 id 写回检测结果，保证 TS 侧 faceId 对齐。
    for (index, face) in result.faces.iter_mut().enumerate() {
        if let Some(frame) = frames.get(index) {
            face.id = frame.id as usize;
            face.uuid = String::new();
        }
    }
    (result, frames)
}

fn analysis_json(
    photo: &Path,
    category: &str,
    detection: &FaceDetectionResult,
    beauty: &Value,
    report: &Value,
) -> Value {
    json!({
        "photo": photo.to_string_lossy(),
        "category": category,
        "mode": "natural",
        "overall": 100,
        "detection": detection,
        "beauty": beauty,
        "report": report,
    })
}

// ───────────────────────────── 阶段 1：dump ─────────────────────────────

#[test]
#[ignore = "S1.2 诊断阶段1；由 scripts/s1_2_one_click_diagnostics.sh 驱动（缺照片会 panic）"]
fn phase_dump() {
    require_phase("dump");
    let dir = out_dir();
    let cancel = CancelToken::default();
    let mut manifest = Vec::new();

    let mut photos = required_photos();

    // 多人照片：仓库既有测试素材（与其余类别同样要求真实检测到 ≥2 张脸）。
    let group = repo_root().join("src-tauri/tests/fixtures/images/group.jpg");
    assert!(
        group.is_file(),
        "缺少照片导致未执行：多人照片素材 {} 不存在",
        group.display()
    );
    {
        let decoded = decoder::decode(&group).expect("多人照片解码失败");
        let preview = fit_within(&decoded.image, 1800);
        let (_, frames) = detect_frames(&preview, decoded.image.width(), decoded.image.height());
        assert!(
            frames.len() >= 2,
            "多人照片素材未检测到 ≥2 张人脸（got {}）",
            frames.len()
        );
        println!("[dump] 多人照片：{}（{} 张脸）", group.display(), frames.len());
        photos.push(("group", group));
    }

    for (category, path) in &photos {
        let started = Instant::now();
        let stem = stem_of(path);
        let decoded = decoder::decode(path)
            .unwrap_or_else(|error| panic!("[dump] {stem} 解码失败：{error}"));
        let full_w = decoded.image.width();
        let full_h = decoded.image.height();
        let preview = fit_within(&decoded.image, 1800);

        let (detection, frames) = detect_frames(&preview, full_w, full_h);
        assert!(
            !frames.is_empty(),
            "[dump] {stem} 未检测到人脸（分析链无法继续）"
        );

        // 逐脸美肤分析（与 analyze_beauty 相同的 analyze_face 调用）。
        let mut suggestions = Vec::new();
        for frame in &frames {
            let mask = build_skin_mask(&preview, frame, &ManualMask::default())
                .unwrap_or_else(|error| panic!("[dump] {stem} 蒙版失败：{error}"));
            let geometry = FaceGeometry::from_face(frame, preview.width(), preview.height())
                .unwrap_or_else(|error| panic!("[dump] {stem} 几何失败：{error}"));
            suggestions.push(
                analyze_face(&preview, &mask, &geometry, &cancel)
                    .unwrap_or_else(|error| panic!("[dump] {stem} 美肤分析失败：{error}")),
            );
        }
        let beauty = json!({
            "faces": suggestions,
            "method": "diagnostic",
            "model": "YuNet + diagnostic",
            "elapsedMs": 0,
            "skinModel": Value::Null,
            "warnings": [],
        });

        // 整图光影/色调分析（与 ensureGlobalReport 相同的 analysis::analyze，standard）。
        let report = analysis::analyze(&preview, &frames, &EditParams::default(), RetouchMode::Standard);
        let report: Value = serde_json::to_value(&report).expect("分析报告序列化");

        let input = analysis_json(path, category, &detection, &beauty, &report);
        let file = dir.join(format!("{stem}.input.json"));
        std::fs::write(&file, serde_json::to_string_pretty(&input).expect("json"))
            .expect("写 input.json");
        manifest.push(json!({
            "category": category,
            "stem": stem,
            "photo": path.to_string_lossy(),
        }));
        println!(
            "[dump] {category}/{stem}: faces={} suggestions={} {}ms",
            frames.len(),
            suggestions.len(),
            started.elapsed().as_millis()
        );
    }

    std::fs::write(
        dir.join("manifest.json"),
        serde_json::to_string_pretty(&Value::Array(manifest)).expect("json"),
    )
    .expect("写 manifest.json");
    println!("[dump] 完成 → {}", dir.display());
}

// ───────────────────────────── 阶段 3：render ─────────────────────────────

fn image_to_rgb(image: &Image16) -> Vec<f32> {
    let mut out = vec![0f32; (image.width() * image.height() * 3) as usize];
    for y in 0..image.height() {
        for x in 0..image.width() {
            let p = image.get_pixel(x, y).0;
            let i = (y * image.width() + x) as usize * 3;
            out[i] = p[0] as f32 / 65535.0;
            out[i + 1] = p[1] as f32 / 65535.0;
            out[i + 2] = p[2] as f32 / 65535.0;
        }
    }
    out
}

/// 把 ROI 蒙版铺回全画布（0..1），用于全图口径的质量统计。
fn compose_full(roi: &[f32], x0: i32, y0: i32, rw: u32, rh: u32, w: u32, h: u32) -> Vec<f32> {
    let mut full = vec![0f32; (w * h) as usize];
    for y in 0..rh {
        for x in 0..rw {
            let gx = x0 + x as i32;
            let gy = y0 + y as i32;
            if gx < 0 || gy < 0 || gx >= w as i32 || gy >= h as i32 {
                continue;
            }
            full[(gy as u32 * w + gx as u32) as usize] = roi[(y * rw + x) as usize];
        }
    }
    full
}

fn zero_tonal(params: &mut EditParams) {
    params.exposure = 0.0;
    params.contrast = 0.0;
    params.highlights = 0.0;
    params.shadows = 0.0;
    params.temperature = 0.0;
    params.tint = 0.0;
    params.saturation = 0.0;
    params.vibrance = 0.0;
}

fn zero_beauty(params: &mut EditParams) {
    params.skin_smooth = 0.0;
    for entry in params.faces.iter_mut() {
        entry.beauty = BeautyParams::default();
    }
    for entry in params.reshape.iter_mut() {
        entry.enabled = false;
    }
}

fn slim_style(mode: &str) -> SlimStyle {
    match mode {
        "refined" => SlimStyle::Refined,
        "advanced" => SlimStyle::Advanced,
        _ => SlimStyle::Natural,
    }
}

/// 镜像 OneClickPanel 的瘦脸写入：plan_face_shape → 非中性才 upsert。
fn merge_reshape(
    params: &mut EditParams,
    frames: &[FaceFrame],
    image_w: u32,
    image_h: u32,
    mode: &str,
    strength: f32,
) -> u32 {
    let style = slim_style(mode);
    let mut wrote = 0u32;
    for frame in frames {
        let Ok(contour) = analyze_face_contour(frame, image_w, image_h) else {
            continue;
        };
        let (shape, _) = plan_face_slim(&contour, style, strength);
        if shape.is_neutral() {
            continue;
        }
        let face_id = contour.face_id;
        let face_uuid = contour.face_uuid.clone();
        if let Some(entry) = params
            .reshape
            .iter_mut()
            .find(|entry| entry.face_id == face_id)
        {
            entry.enabled = true;
            entry.face = shape;
        } else {
            params.reshape.push(FaceReshape {
                face_id,
                face_uuid,
                enabled: true,
                face: shape,
                eyes: Default::default(),
                nose: Default::default(),
                mouth: Default::default(),
                chin: Default::default(),
                manual: Default::default(),
            });
        }
        wrote += 1;
    }
    wrote
}

/// 一次渲染的像素级统计（全图口径 + 皮肤/保护分区）。
fn measure(
    before_rgb: &[f32],
    after_rgb: &[f32],
    skin_full: &[f32],
    protect_full: &[f32],
    w: u32,
    h: u32,
) -> Value {
    let n = (w * h) as usize;
    let mut changed = 0u32;
    let mut visible_changed = 0u32;
    let mut abs_sum = 0.0f64;
    let mut luma_before = 0.0f64;
    let mut luma_after = 0.0f64;
    for i in 0..n {
        let mut pixel_delta = 0.0f32;
        for c in 0..3 {
            pixel_delta += (after_rgb[i * 3 + c] - before_rgb[i * 3 + c]).abs();
        }
        pixel_delta /= 3.0;
        if pixel_delta > 1.0 / 65535.0 {
            changed += 1;
        }
        // 「肉眼可辨」口径：平均通道差 > 0.001（约 1/8 级灰阶以上）。
        if pixel_delta > 0.001 {
            visible_changed += 1;
        }
        abs_sum += pixel_delta as f64;
        luma_before += (0.299 * before_rgb[i * 3]
            + 0.587 * before_rgb[i * 3 + 1]
            + 0.114 * before_rgb[i * 3 + 2]) as f64;
        luma_after += (0.299 * after_rgb[i * 3]
            + 0.587 * after_rgb[i * 3 + 1]
            + 0.114 * after_rgb[i * 3 + 2]) as f64;
    }

    // 保护区域变化比例：保护权重（1-允许变化）高且 |Δ| 超阈值的像素占比。
    let mut prot_total = 0u32;
    let mut prot_changed = 0u32;
    for i in 0..n {
        let protection = 1.0 - protect_full[i].clamp(0.0, 1.0);
        if protection > 0.5 {
            prot_total += 1;
            let mut delta = 0.0f32;
            for c in 0..3 {
                delta += (after_rgb[i * 3 + c] - before_rgb[i * 3 + c]).abs();
            }
            if delta / 3.0 > 0.002 {
                prot_changed += 1;
            }
        }
    }

    let q = quality::evaluate(
        before_rgb,
        after_rgb,
        skin_full,
        protect_full,
        w,
        h,
    );
    json!({
        "changedRatio": changed as f32 / n as f32,
        "visibleChangedRatio": visible_changed as f32 / n as f32,
        "meanAbsDelta": (abs_sum / n as f64) as f32,
        "meanLumaDelta": ((luma_after - luma_before) / n as f64) as f32,
        "protectedChangedRatio": if prot_total > 0 {
            prot_changed as f32 / prot_total as f32
        } else { 0.0 },
        "quality": {
            "skinKeepRatio": q.skin.texture_keep_ratio,
            "skinHighFreqBefore": q.skin.high_freq_before,
            "skinHighFreqAfter": q.skin.high_freq_after,
            "skinMeanAbsDelta": q.skin.mean_abs_delta,
            "skinColorDelta": q.skin.mean_color_delta,
            "protectedMeanAbsDelta": q.protected.mean_abs_delta,
            "edgeMeanAbsDelta": q.edge.mean_abs_delta,
            "overSmoothRisk": q.over_smooth_risk,
            "protectedLeak": q.protected_leak,
            "notes": q.notes,
        },
    })
}

fn beauty_written(params: &EditParams) -> Value {
    let faces: Vec<Value> = params
        .faces
        .iter()
        .filter(|entry| entry.enabled)
        .map(|entry| {
            json!({
                "faceId": entry.face_id,
                "faceUuid": entry.face_uuid,
                "skinSmooth": entry.beauty.skin_smooth,
                "texture": entry.beauty.texture,
                "detail": entry.beauty.detail,
                "blemish": entry.beauty.blemish,
                "spots": entry.beauty.spots,
                "blackheads": entry.beauty.blackheads,
                "whitening": entry.beauty.whitening,
                "toneEven": entry.beauty.tone_even,
                "dullness": entry.beauty.dullness,
                "redness": entry.beauty.redness,
                "darkCircles": entry.beauty.dark_circles,
                "eyeLines": entry.beauty.eye_lines,
            })
        })
        .collect();
    json!({
        "faces": faces,
        "tonal": {
            "exposure": params.exposure,
            "contrast": params.contrast,
            "highlights": params.highlights,
            "shadows": params.shadows,
            "temperature": params.temperature,
            "tint": params.tint,
            "saturation": params.saturation,
            "vibrance": params.vibrance,
        },
        "reshape": params.reshape.iter().filter(|e| e.enabled).map(|e| json!({
            "faceId": e.face_id, "slim": e.face.slim, "jaw": e.face.jaw,
        })).collect::<Vec<_>>(),
    })
}

#[test]
#[ignore = "S1.2 诊断阶段3；由 scripts/s1_2_one_click_diagnostics.sh 驱动（需先跑 dump 与 TS 桥）"]
fn phase_render() {
    require_phase("render");
    let dir = out_dir();
    let manifest_path = dir.join("manifest.json");
    assert!(
        manifest_path.is_file(),
        "缺少 manifest.json：请先运行 phase_dump 与 TS 计划桥"
    );
    let manifest: Vec<Value> =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).expect("读 manifest"))
            .expect("解析 manifest");
    assert!(!manifest.is_empty(), "manifest 为空：缺少照片导致未执行");

    let cancel = CancelToken::default();
    let modes = ["natural", "refined", "advanced"];

    for entry in &manifest {
        let stem = entry["stem"].as_str().expect("stem").to_string();
        let category = entry["category"].as_str().unwrap_or("").to_string();
        let photo = PathBuf::from(entry["photo"].as_str().expect("photo"));
        let started = Instant::now();

        // 1) 输入与三档参数（TS 桥产物）
        let input_path = dir.join(format!("{stem}.input.json"));
        assert!(input_path.is_file(), "缺少 {}（phase_dump 未产出）", input_path.display());
        let input: Value =
            serde_json::from_str(&std::fs::read_to_string(&input_path).expect("读 input"))
                .expect("解析 input");
        let mut params_by_mode: Vec<(String, EditParams)> = Vec::new();
        for mode in modes {
            let path = dir.join(format!("{stem}.params.{mode}.json"));
            assert!(
                path.is_file(),
                "缺少 {}：TS 计划桥未运行或失败（三态：本阶段失败，不假绿）",
                path.display()
            );
            let params: EditParams = serde_json::from_str(
                &std::fs::read_to_string(&path).expect("读 params"),
            )
            .unwrap_or_else(|error| panic!("[{stem}/{mode}] 参数反序列化失败：{error}"));
            params_by_mode.push((mode.to_string(), params));
        }

        // 2) 真实检测（与 render_preview 相同：faces 来自检测结果）
        let decoded = decoder::decode(&photo)
            .unwrap_or_else(|error| panic!("[{stem}] 解码失败：{error}"));
        let full_w = decoded.image.width();
        let full_h = decoded.image.height();
        let preview = fit_within(&decoded.image, 1800);
        let (detection, frames) = detect_frames(&preview, full_w, full_h);
        assert!(!frames.is_empty(), "[{stem}] 渲染阶段检测不到人脸");
        assert_eq!(
            detection.faces.len(),
            frames.len(),
            "[{stem}] 检测结果与 Frame 数量不一致"
        );

        let before_rgb = image_to_rgb(&preview);
        let (pw, ph) = (preview.width(), preview.height());

        // 蒙版 + 保护（全画布口径）
        let mut skin_full = vec![0f32; (pw * ph) as usize];
        let mut protect_full = vec![1.0f32; (pw * ph) as usize];
        let mut coverage_pixels = 0u32;
        for frame in &frames {
            let mask = build_skin_mask(&preview, frame, &ManualMask::default())
                .expect("渲染阶段蒙版失败");
            let geometry = FaceGeometry::from_face(frame, pw, ph).expect("几何失败");
            let protect = texture_protect::build_texture_protection(&preview, &mask, &geometry, &cancel)
                .expect("保护权重失败");
            let skin_roi = compose_full(&mask.alpha, mask.x0, mask.y0, mask.width, mask.height, pw, ph);
            let protect_roi = compose_full(&protect, mask.x0, mask.y0, mask.width, mask.height, pw, ph);
            for i in 0..skin_full.len() {
                skin_full[i] = skin_full[i].max(skin_roi[i]);
                // 保护取最弱（允许变化权重取最小）
                protect_full[i] = protect_full[i].min(protect_roi[i]);
            }
            coverage_pixels += mask.alpha.iter().filter(|v| **v > 0.5).count() as u32;
        }
        let mask_coverage = coverage_pixels as f32 / (pw * ph) as f32;

        // 3) 三档渲染（含瘦脸计划合并，镜像 OneClickPanel）
        let mut pixels_by_mode: Vec<(String, Value)> = Vec::new();
        let mut chain_by_mode: Vec<(String, Value)> = Vec::new();
        let mut finals: Vec<(String, Image16)> = Vec::new();
        let mut natural_beauty_only: Option<Value> = None;
        let mut natural_tonal_only: Option<Value> = None;
        let mut skin_smooth_by_mode: Vec<(String, f32)> = Vec::new();

        for (mode, mut params) in params_by_mode {
            let reshape_wrote = merge_reshape(&mut params, &frames, full_w, full_h, &mode, input["overall"].as_f64().unwrap_or(100.0) as f32);
            let skin_smooth = params
                .faces
                .iter()
                .filter(|e| e.enabled)
                .map(|e| e.beauty.skin_smooth)
                .fold(0.0f32, f32::max);
            skin_smooth_by_mode.push((mode.clone(), skin_smooth));

            let (rendered, report) = pipeline::render_with_report(
                &preview,
                &params,
                &frames,
                &cancel,
                &mut |_, _| {},
            )
            .unwrap_or_else(|error| panic!("[{stem}/{mode}] 渲染失败：{error}"));
            let after_rgb = image_to_rgb(&rendered);
            let pixels = measure(&before_rgb, &after_rgb, &skin_full, &protect_full, pw, ph);
            let chain = json!({
                "reshapeWrote": reshape_wrote,
                "applyBeauty": !report.faces.is_empty(),
                "faces": report.faces,
                "warnings": report.warnings,
            });

            // 分解实验（仅 natural）：证明皮肤变化来自美肤算法而非提亮。
            if mode == "natural" {
                let mut beauty_only = params.clone();
                zero_tonal(&mut beauty_only);
                for entry in beauty_only.reshape.iter_mut() {
                    entry.enabled = false;
                }
                let (bimg, _) = pipeline::render_with_report(
                    &preview,
                    &beauty_only,
                    &frames,
                    &cancel,
                    &mut |_, _| {},
                )
                .expect("beauty-only 渲染");
                natural_beauty_only =
                    Some(measure(&before_rgb, &image_to_rgb(&bimg), &skin_full, &protect_full, pw, ph));

                let mut tonal_only = params.clone();
                zero_beauty(&mut tonal_only);
                let (timg, _) = pipeline::render_with_report(
                    &preview,
                    &tonal_only,
                    &frames,
                    &cancel,
                    &mut |_, _| {},
                )
                .expect("tonal-only 渲染");
                natural_tonal_only =
                    Some(measure(&before_rgb, &image_to_rgb(&timg), &skin_full, &protect_full, pw, ph));
            }

            pixels_by_mode.push((mode.clone(), pixels));
            chain_by_mode.push((mode.clone(), chain));
            finals.push((mode, rendered));
        }

        // 4) 断言（自然档主链路 + 三档可辨识 + 非提亮冒充）
        let natural_pixels = pixels_by_mode
            .iter()
            .find(|(m, _)| m == "natural")
            .map(|(_, v)| v.clone())
            .expect("natural 像素统计");
        let natural_chain = chain_by_mode
            .iter()
            .find(|(m, _)| m == "natural")
            .map(|(_, v)| v.clone())
            .expect("natural 链路统计");

        assert!(
            natural_chain["applyBeauty"].as_bool() == Some(true),
            "[{stem}] apply_beauty 未执行：warnings={:?}",
            natural_chain["warnings"]
        );
        let faces_report = natural_chain["faces"].as_array().expect("faces 数组");
        assert!(
            !faces_report.is_empty(),
            "[{stem}] 没有逐脸美肤报告（run_skin_retouch 未到达）"
        );
        for face in faces_report {
            assert_eq!(face["skinEngineRan"].as_bool(), Some(true), "[{stem}] run_skin_retouch 未执行");
            assert_eq!(
                face["bilateralGridRan"].as_bool(),
                Some(true),
                "[{stem}] 双边网格未执行"
            );
            assert!(
                face["fusedPixels"].as_u64().unwrap_or(0) > 0,
                "[{stem}] 融合像素为 0（没有像素被修改）"
            );
            assert!(
                face["processCoverage"].as_f64().unwrap_or(0.0) > 0.005,
                "[{stem}] process_w 覆盖率过低：{}",
                face["processCoverage"]
            );
        }

        let beauty_only = natural_beauty_only.as_ref().expect("beauty-only 统计");
        let tonal_only = natural_tonal_only.as_ref().expect("tonal-only 统计");
        let bo_keep = beauty_only["quality"]["skinKeepRatio"].as_f64().unwrap_or(1.0);
        let bo_delta = beauty_only["quality"]["skinMeanAbsDelta"]
            .as_f64()
            .unwrap_or(0.0);
        assert!(
            bo_keep < 0.98 && bo_delta > 0.0001,
            "[{stem}] 美肤（非提亮）未改变皮肤纹理：keep={bo_keep} delta={bo_delta}"
        );
        let bo_luma = beauty_only["meanLumaDelta"].as_f64().unwrap_or(0.0);
        assert!(
            bo_luma.abs() < 0.03,
            "[{stem}] beauty-only 的亮度漂移过大（疑似以提亮冒充美颜）：{bo_luma}"
        );
        // 对照：纯色调路径不应像美肤那样削掉皮肤高频（否则「变化」只是对比度）。
        let tonal_keep = tonal_only["quality"]["skinKeepRatio"].as_f64().unwrap_or(1.0);
        assert!(
            bo_keep < tonal_keep - 0.01,
            "[{stem}] 皮肤纹理变化可由纯色调解释（美肤未贡献）：beauty_keep={bo_keep} tonal_keep={tonal_keep}"
        );

        // 三档参数与像素效果必须可辨识（严格递增）
        let ss: Vec<f32> = modes
            .iter()
            .map(|m| {
                skin_smooth_by_mode
                    .iter()
                    .find(|(name, _)| name == m)
                    .map(|(_, v)| *v)
                    .unwrap_or(0.0)
            })
            .collect();
        assert!(
            ss[0] > 0.0 && ss[0] < ss[1] && ss[1] < ss[2],
            "[{stem}] 三档 skinSmooth 不可辨识：{:?}",
            ss
        );
        let skin_delta = |mode: &str| -> f64 {
            pixels_by_mode
                .iter()
                .find(|(m, _)| m == mode)
                .and_then(|(_, v)| v["quality"]["skinMeanAbsDelta"].as_f64())
                .unwrap_or(0.0)
        };
        let (nat_d, adv_d) = (skin_delta("natural"), skin_delta("advanced"));
        assert!(
            adv_d > nat_d * 1.05,
            "[{stem}] 高级档皮肤变化未明显强于自然档：natural={nat_d} advanced={adv_d}"
        );

        let nat_changed = natural_pixels["changedRatio"].as_f64().unwrap_or(0.0);
        let nat_visible = natural_pixels["visibleChangedRatio"].as_f64().unwrap_or(0.0);
        let nat_mean = natural_pixels["meanAbsDelta"].as_f64().unwrap_or(0.0);
        assert!(
            nat_changed > 0.00005 && nat_visible > 0.0005 && nat_mean > 0.0002,
            "[{stem}] 自然档变化不足：changed={nat_changed} visible={nat_visible} mean={nat_mean} pixels={natural_pixels}"
        );

        // 5) 导出：原图 / 三档结果 / 并排 / 区域放大
        let out = dir.join(&stem);
        let _ = std::fs::remove_dir_all(&out);
        std::fs::create_dir_all(&out).expect("创建照片输出目录");
        let original = to_rgba(&preview);
        original.save(out.join("original.png")).expect("save original");
        let mut natural_img = None;
        for (mode, img) in &finals {
            let rgba = to_rgba(img);
            rgba.save(out.join(format!("oneclick_{mode}.png")))
                .expect("save mode");
            if mode == "natural" {
                natural_img = Some(rgba.clone());
                side_by_side(&original, &rgba)
                    .save(out.join("compare.png"))
                    .expect("save compare");
            }
        }
        // 三档并排（原图 | 自然 | 精致 | 高级）
        let tier_images: Vec<RgbaImage> = finals.iter().map(|(_, img)| to_rgba(img)).collect();
        {
            let gap = 6u32;
            let mut w = 0u32;
            for img in std::iter::once(&original).chain(tier_images.iter()) {
                w += img.width() + gap;
            }
            let h = original.height();
            let mut canvas = RgbaImage::from_pixel(w, h, Rgba([18, 18, 18, 255]));
            let mut x = 0u32;
            for img in std::iter::once(&original).chain(tier_images.iter()) {
                for y in 0..img.height() {
                    for ix in 0..img.width() {
                        canvas.put_pixel(x + ix, y, *img.get_pixel(ix, y));
                    }
                }
                x += img.width() + gap;
            }
            canvas.save(out.join("tier_compare.png")).expect("save tiers");
        }

        // 区域放大（原图 | 自然档），100% / 200%
        let natural_rgba = natural_img.expect("natural 结果");
        let geometry = FaceGeometry::from_face(&frames[0], pw, ph).expect("几何");
        let regions: Vec<(&str, (f32, f32))> = vec![
            ("eye_left", (geometry.eyes[0].0, geometry.eyes[0].1)),
            ("eye_right", (geometry.eyes[1].0, geometry.eyes[1].1)),
            ("brow_left", geometry.from_local(-0.35, -0.30)),
            ("brow_right", geometry.from_local(0.35, -0.30)),
            ("nose_wing_left", geometry.from_local(-0.30, 0.18)),
            ("nose_wing_right", geometry.from_local(0.30, 0.18)),
            ("lips", geometry.from_local(0.0, 0.78)),
        ];
        let size = (geometry.eye_distance * 3.0).clamp(72.0, 200.0) as u32;
        for (name, (fx, fy)) in &regions {
            let cx = (*fx as i32).clamp(0, pw as i32 - 1) as u32;
            let cy = (*fy as i32).clamp(0, ph as i32 - 1) as u32;
            for factor in [1u32, 2u32] {
                let a = zoom_crop(&original, cx, cy, size, factor);
                let b = zoom_crop(&natural_rgba, cx, cy, size, factor);
                side_by_side(&a, &b)
                    .save(out.join(format!("zoom_{name}_{factor}00.png")))
                    .expect("save zoom");
            }
        }

        // 6) 诊断 JSON
        let written: Value = {
            let map: Vec<Value> = std::fs::read_dir(&dir)
                .unwrap_or_else(|_| panic!("读输出目录 {}", dir.display()))
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.starts_with(&format!("{stem}.params.")) && n.ends_with(".json"))
                        .unwrap_or(false)
                })
                .map(|p| {
                    let mode = p
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .to_string()
                        .trim_start_matches(&format!("{stem}.params."))
                        .trim_end_matches(".json")
                        .to_string();
                    let params: EditParams =
                        serde_json::from_str(&std::fs::read_to_string(&p).expect("读")).expect("解析");
                    json!({ "mode": mode, "beauty": beauty_written(&params) })
                })
                .collect();
            Value::Array(map)
        };

        let diag = json!({
            "category": category,
            "stem": stem,
            "photo": photo.to_string_lossy(),
            "mode": "natural",
            "overall": input["overall"],
            "facesDetected": frames.len(),
            "faceIds": frames.iter().map(|f| f.id).collect::<Vec<_>>(),
            "faceUuids": frames.iter().map(|f| f.uuid.clone()).collect::<Vec<_>>(),
            "uuidNote": "测试链未接 FaceTracker，UUID 为空；按稳定 face_id 对齐（应用内由跟踪器分配 UUID）",
            "maskCoverage": mask_coverage,
            "writtenParams": written,
            "chainByMode": chain_by_mode.iter().map(|(m, v)| json!({ "mode": m, "chain": v })).collect::<Vec<_>>(),
            "pixelsByMode": pixels_by_mode.iter().map(|(m, v)| json!({ "mode": m, "pixels": v })).collect::<Vec<_>>(),
            "decomposition": {
                "beautyOnly": natural_beauty_only,
                "tonalOnly": natural_tonal_only,
            },
            "skinSmoothByMode": skin_smooth_by_mode.iter().map(|(m, v)| json!([m, v])).collect::<Vec<_>>(),
            "elapsedMs": started.elapsed().as_millis(),
        });
        std::fs::write(
            dir.join(format!("{stem}.diag.json")),
            serde_json::to_string_pretty(&diag).expect("diag json"),
        )
        .expect("写 diag.json");

        println!(
            "[render] {category}/{stem}: faces={} maskCoverage={:.3} natural_changed={:.4} skin_keep(bo)={:.3} nat/adv_delta={:.4}/{:.4} {}",
            frames.len(),
            mask_coverage,
            nat_changed,
            bo_keep,
            nat_d,
            adv_d,
            started.elapsed().as_millis()
        );
    }

    println!("[render] 全部照片诊断完成 → {}", dir.display());
}
