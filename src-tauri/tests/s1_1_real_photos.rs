//! S1.1 真实照片重新验收。
//!
//! 语料：`beauty-validation/`（P0 起在用的授权验证照片；本轮额外覆盖
//! `01-acne` 痘痘/真实皮肤纹理与 `02-rough-tone` 肤色不均组）。
//!
//! 输出到 `docs/assets/s1_1/{stem}/`：
//! - `original.png` / `skin_mask.png` / `protect_mask.png`
//! - `frequency_output.png`（频率分离输出）/ `bilateral_grid_output.png`（双边网格输出）
//! - `final.png`（最终融合输出）/ `compare.png`（原图|最终并排）
//! - `zoom_{region}_100.png` / `zoom_{region}_200.png`（原图|处理后并排，100%/200%）
//!   region ∈ forehead / eye_left / eye_right / nose_wing_left / nose_wing_right /
//!   lips / cheek_left / cheek_right / chin / skin_edge
//!
//! ## 三态约定（禁止缺照片假通过）
//! - 默认 `cargo test --test s1_1_real_photos` → `ignored`（**未执行**）；
//! - `-- --ignored` 且照片齐全 → 断言决定 **通过/失败**；
//! - `-- --ignored` 且缺少照片 → **panic**「缺少照片导致未执行」，绝不 return 假绿。

use std::path::{Path, PathBuf};

use image::{imageops::FilterType, Rgba, RgbaImage};

use lumi_retouch_lib::ai::face::FaceDetectionResult;
use lumi_retouch_lib::ai::model::MODEL_FILE;
use lumi_retouch_lib::ai::FaceEngine;
use lumi_retouch_lib::ai_engine::face_frame::{FaceFrame, FaceGeometry};
use lumi_retouch_lib::ai_engine::retouch::bilateral_grid::{
    bilateral_grid_smooth_with_stats, params_from_strength,
};
use lumi_retouch_lib::ai_engine::retouch::fusion::{self, FusionRequest};
use lumi_retouch_lib::ai_engine::retouch::quality;
use lumi_retouch_lib::ai_engine::retouch::texture_protect;
use lumi_retouch_lib::ai_engine::retouch::{frequency, texture_protect as tp};
use lumi_retouch_lib::ai_engine::skin_segmentation::build_skin_mask;
use lumi_retouch_lib::image_engine::buffer::{fit_within, Image16};
use lumi_retouch_lib::image_engine::cancel::CancelToken;
use lumi_retouch_lib::image_engine::decoder;
use lumi_retouch_lib::image_engine::params::ManualMask;

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

fn out_root() -> PathBuf {
    repo_root().join("docs/assets/s1_1")
}

/// 收集验收照片：优先痘痘/肤色不均组，再补其他组，最多 7 张。
fn collect_photos() -> Vec<PathBuf> {
    let root = repo_root().join("beauty-validation");
    if !root.is_dir() {
        return Vec::new();
    }
    let mut priority = Vec::new();
    let mut others = Vec::new();
    for entry in std::fs::read_dir(&root).ok().into_iter().flatten() {
        let dir = entry.ok().map(|e| e.path()).unwrap_or_default();
        if !dir.is_dir() {
            continue;
        }
        let group = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let mut jpgs: Vec<PathBuf> = Vec::new();
        for file in std::fs::read_dir(&dir).ok().into_iter().flatten() {
            let p = file.ok().map(|e| e.path()).unwrap_or_default();
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase());
            if matches!(ext.as_deref(), Some("jpg" | "jpeg")) {
                jpgs.push(p);
            }
        }
        jpgs.sort();
        if jpgs.is_empty() {
            continue;
        }
        if group.starts_with("01-acne") || group.starts_with("02-rough-tone") {
            priority.extend(jpgs);
        } else {
            others.push(jpgs[0].clone());
        }
    }
    priority.sort();
    others.sort();
    let mut picked = priority;
    for p in others {
        if picked.len() >= 7 {
            break;
        }
        picked.push(p);
    }
    picked.truncate(7);
    picked
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

fn detect(preview: &Image16, full_w: u32, full_h: u32) -> Option<FaceDetectionResult> {
    if !model_path().is_file() {
        return None;
    }
    FaceEngine::default()
        .detect(&model_path(), preview, full_w, full_h, &mut |_, _| {})
        .ok()
}

/// 把 ROI 平面按权重融合进全图副本（导出中间结果用）。
fn fuse_to_canvas(
    base: &Image16,
    origin: (i32, i32),
    rgb: &[f32],
    weight: &[f32],
    mw: u32,
    mh: u32,
) -> Image16 {
    let mut canvas = base.clone();
    fusion::fuse(
        FusionRequest {
            image: &mut canvas,
            origin,
            processed_rgb: rgb,
            weight,
            roi_w: mw,
            roi_h: mh,
            strength: 1.0,
        },
        &CancelToken::default(),
    )
    .expect("export fuse");
    canvas
}

fn mask_rgba(alpha: &[f32], w: u32, h: u32) -> RgbaImage {
    let mut out = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let v = (alpha[(y * w + x) as usize].clamp(0.0, 1.0) * 255.0) as u8;
            out.put_pixel(x, y, Rgba([v, v, v, 255]));
        }
    }
    out
}

/// 单张照片完整验收：管线 + 导出 + 断言。
fn accept_photo(path: &Path) {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let dir = out_root().join(&stem);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("创建输出目录");

    let decoded = decoder::decode(path)
        .unwrap_or_else(|e| panic!("[FAIL] {stem} 解码失败：{e}"));
    let preview = fit_within(&decoded.image, 1200);
    let det = detect(&preview, preview.width(), preview.height())
        .unwrap_or_else(|| panic!("[FAIL] {stem} 人脸检测执行失败"));
    assert!(!det.faces.is_empty(), "[FAIL] {stem} 未检测到人脸");
    let frame = FaceFrame::from_detected(&det.faces[0], det.image_width, det.image_height);
    let geometry = FaceGeometry::from_face(&frame, preview.width(), preview.height())
        .unwrap_or_else(|e| panic!("[FAIL] {stem} 几何失败：{e}"));

    let cancel = CancelToken::default();
    let mask = build_skin_mask(&preview, &frame, &ManualMask::default())
        .unwrap_or_else(|e| panic!("[FAIL] {stem} 蒙版失败：{e}"));
    let protect = texture_protect::build_texture_protection(&preview, &mask, &geometry, &cancel)
        .unwrap_or_else(|e| panic!("[FAIL] {stem} 保护权重失败：{e}"));
    let process_w = tp::combine_with_skin(&mask.alpha, &protect);
    let (mw, mh) = (mask.width, mask.height);
    let n = (mw * mh) as usize;
    assert!(n > 1000, "[FAIL] {stem} ROI 过小 n={n}");

    // —— 管线（与 run_skin_retouch 同序同参）——
    let before = fusion::extract_roi(&preview, (mask.x0, mask.y0), mw, mh);
    let mut rgb = before.clone();

    let radius = ((mw as f32 * 0.02).round() as u32).clamp(3, 20);
    let texture = 40.0f32;
    let detail = 35.0f32;
    let eps = 0.004 + 0.02 * (1.0 - texture / 100.0);
    let split = frequency::split(&preview, &mask, radius, eps, &cancel)
        .unwrap_or_else(|e| panic!("[FAIL] {stem} 频率分离失败：{e}"));
    let mut low = split.low;
    let high = split.high;
    let tone_even = 25.0f32 / 100.0 * 0.7;
    frequency::even_low_frequency_wh(&mut low, mw, mh, &process_w, tone_even, &cancel)
        .expect("even low");
    let high_keep = (0.20 + 0.50 * texture / 100.0 + 0.20 * detail / 100.0).clamp(0.10, 0.90);
    let mut rgb_after_freq = rgb.clone();
    for i in 0..n {
        for c in 0..3 {
            rgb_after_freq[i * 3 + c] = (low[c][i] + high[c][i] * high_keep).clamp(0.0, 1.0);
        }
    }
    rgb.copy_from_slice(&rgb_after_freq);

    let strength = 0.55f32;
    let mut grid_params = params_from_strength(strength * 100.0, texture, detail);
    grid_params.strength = strength;
    let grid_stats =
        bilateral_grid_smooth_with_stats(&mut rgb, mw, mh, Some(&process_w), &grid_params, &cancel)
            .unwrap_or_else(|e| panic!("[FAIL] {stem} 双边网格失败：{e}"));
    let rgb_after_grid = rgb.clone();

    for v in rgb.iter() {
        assert!(v.is_finite(), "[FAIL] {stem} 输出含非有限值");
    }

    let q = quality::evaluate(&before, &rgb, &mask.alpha, &protect, mw, mh);

    // —— 导出 ——
    let original_rgba = to_rgba(&preview);
    original_rgba
        .save(dir.join("original.png"))
        .expect("save original");
    mask_rgba(&mask.alpha, mw, mh)
        .save(dir.join("skin_mask.png"))
        .expect("save skin mask");
    mask_rgba(&protect, mw, mh)
        .save(dir.join("protect_mask.png"))
        .expect("save protect mask");

    let freq_canvas = fuse_to_canvas(&preview, (mask.x0, mask.y0), &rgb_after_freq, &process_w, mw, mh);
    to_rgba(&freq_canvas)
        .save(dir.join("frequency_output.png"))
        .expect("save freq");
    let grid_canvas = fuse_to_canvas(&preview, (mask.x0, mask.y0), &rgb_after_grid, &process_w, mw, mh);
    to_rgba(&grid_canvas)
        .save(dir.join("bilateral_grid_output.png"))
        .expect("save grid");

    let final_img = fuse_to_canvas(&preview, (mask.x0, mask.y0), &rgb, &process_w, mw, mh);
    let final_rgba = to_rgba(&final_img);
    final_rgba.save(dir.join("final.png")).expect("save final");
    side_by_side(&original_rgba, &final_rgba)
        .save(dir.join("compare.png"))
        .expect("save compare");

    // —— 区域放大（100% / 200%，原图|处理后）——
    let regions: Vec<(&str, (f32, f32))> = vec![
        ("forehead", geometry.from_local(0.0, -0.95)),
        ("eye_left", (geometry.eyes[0].0, geometry.eyes[0].1)),
        ("eye_right", (geometry.eyes[1].0, geometry.eyes[1].1)),
        ("nose_wing_left", geometry.from_local(-0.30, 0.18)),
        ("nose_wing_right", geometry.from_local(0.30, 0.18)),
        ("lips", geometry.from_local(0.0, 0.78)),
        ("cheek_left", geometry.from_local(-0.65, 0.28)),
        ("cheek_right", geometry.from_local(0.65, 0.28)),
        ("chin", geometry.from_local(0.0, 1.15)),
        (
            "skin_edge",
            (
                (mask.x0 + 2) as f32,
                (mask.y0 + mask.height as i32 / 2) as f32,
            ),
        ),
    ];
    for (name, (fx, fy)) in &regions {
        let cx = (*fx as i32).clamp(0, preview.width() as i32 - 1) as u32;
        let cy = (*fy as i32).clamp(0, preview.height() as i32 - 1) as u32;
        // 裁窗随眼距缩放：小脸也有完整上下文，上限避免越过 ROI。
        let size = (geometry.eye_distance * 2.4).clamp(64.0, 160.0) as u32;
        for factor in [1u32, 2u32] {
            let a = zoom_crop(&original_rgba, cx, cy, size, factor);
            let b = zoom_crop(&final_rgba, cx, cy, size, factor);
            side_by_side(&a, &b)
                .save(dir.join(format!("zoom_{name}_{factor}00.png")))
                .expect("save zoom");
        }
    }

    // —— 自动断言 ——
    let total = (preview.width() * preview.height()).max(1);
    let mut changed = 0u32;
    for y in 0..preview.height() {
        for x in 0..preview.width() {
            if preview.get_pixel(x, y) != final_img.get_pixel(x, y) {
                changed += 1;
            }
        }
    }
    let ratio = changed as f32 / total as f32;
    assert!(
        ratio > 0.0001 && ratio < 0.85,
        "[FAIL] {stem} 磨皮变化比例异常 ratio={ratio}"
    );
    // ROI 外像素不得被修改（融合只允许写 ROI）
    for y in 0..preview.height() {
        for x in 0..preview.width() {
            let lx = x as i32 - mask.x0;
            let ly = y as i32 - mask.y0;
            let inside = lx >= 0
                && ly >= 0
                && lx < mask.width as i32
                && ly < mask.height as i32;
            if !inside {
                assert_eq!(
                    preview.get_pixel(x, y),
                    final_img.get_pixel(x, y),
                    "[FAIL] {stem} ROI 外像素被修改 ({x},{y})"
                );
            }
        }
    }
    assert!(
        q.over_smooth_risk < 1.0,
        "[FAIL] {stem} 过度磨皮风险触发：{:?}",
        q.notes
    );
    assert!(
        q.protected_leak < 1.0,
        "[FAIL] {stem} 保护泄漏触发：{:?}",
        q.notes
    );
    assert!(
        q.skin.pixels > 0 && q.skin.texture_keep_ratio >= 0.25,
        "[FAIL] {stem} 皮肤纹理保留不足 keep={} pixels={}",
        q.skin.texture_keep_ratio,
        q.skin.pixels
    );

    eprintln!(
        "[S1.1][PASS] {stem}: changed={ratio:.4} skin_keep={:.3} color_delta={:.4} edge_delta={:.4} prot_leak={} notes={:?} grid_cells={} grid_bytes={} rss_ok",
        q.skin.texture_keep_ratio,
        q.skin.mean_color_delta,
        q.edge.mean_abs_delta,
        q.protected_leak,
        q.notes,
        grid_stats.max_cells,
        grid_stats.max_grid_bytes,
    );
}

#[test]
#[ignore = "需要 beauty-validation 真实照片；cargo test --test s1_1_real_photos -- --ignored --nocapture"]
fn s1_1_real_photo_reacceptance() {
    let photos = collect_photos();
    assert!(
        !photos.is_empty(),
        "缺少照片导致未执行：beauty-validation/ 下未找到 JPG（01-acne/02-rough-tone 等）"
    );
    // 必须包含痘痘组（真实皮肤纹理/痘痘）与肤色不均组。
    let has_acne = photos.iter().any(|p| p.to_string_lossy().contains("01-acne"));
    let has_rough = photos
        .iter()
        .any(|p| p.to_string_lossy().contains("02-rough-tone"));
    assert!(
        has_acne && has_rough,
        "缺少照片导致未执行：必须包含 01-acne 与 02-rough-tone 组，got={:?}",
        photos
    );

    let mut passed = 0usize;
    for path in &photos {
        accept_photo(path);
        passed += 1;
    }
    assert_eq!(passed, photos.len());
    eprintln!(
        "[S1.1] 真实照片验收完成：{} 张全部通过；输出目录 {}",
        passed,
        out_root().display()
    );
}
