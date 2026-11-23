//! P0 验收：真实照片磨皮前后对比 + 放大裁切。
//!
//! 输出到 `docs/assets/p0_skin/`：
//! - `{stem}_original.png` / `_smoothed.png` / `_compare50.png`
//! - `{stem}_zoom100.png` / `_zoom200.png`（脸颊 ROI）
//!
//! 无 beauty-validation 时 skip。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use image::{imageops::FilterType, Rgba, RgbaImage};

use lumi_retouch_lib::ai::face::FaceDetectionResult;
use lumi_retouch_lib::ai::model::MODEL_FILE;
use lumi_retouch_lib::ai::FaceEngine;
use lumi_retouch_lib::ai_engine::face_frame::FaceFrame;
use lumi_retouch_lib::image_engine::buffer::{fit_within, Image16};
use lumi_retouch_lib::image_engine::cancel::CancelToken;
use lumi_retouch_lib::image_engine::decoder;
use lumi_retouch_lib::image_engine::params::{BeautyParams, EditParams, FaceBeauty};
use lumi_retouch_lib::image_engine::pipeline::render_with_report;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("parent")
        .to_path_buf()
}

fn model_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("resources/models/yunet")
        .join(MODEL_FILE)
}

fn out_dir() -> PathBuf {
    let dir = repo_root().join("docs/assets/p0_skin");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn collect_photos() -> Vec<PathBuf> {
    let root = repo_root().join("beauty-validation");
    if !root.is_dir() {
        return Vec::new();
    }
    let mut all = Vec::new();
    for entry in std::fs::read_dir(&root).ok().into_iter().flatten() {
        let dir = entry.ok().map(|e| e.path()).unwrap_or_default();
        if !dir.is_dir() {
            continue;
        }
        for file in std::fs::read_dir(&dir).ok().into_iter().flatten() {
            let p = file.ok().map(|e| e.path()).unwrap_or_default();
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase());
            if matches!(ext.as_deref(), Some("jpg" | "jpeg" | "png")) {
                all.push(p);
            }
        }
    }
    all.sort();
    let mut per: HashMap<String, usize> = Default::default();
    let mut picked = Vec::new();
    for p in all {
        let key = p
            .parent()
            .and_then(|d| d.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let n = per.entry(key).or_insert(0);
        if *n >= 1 {
            continue;
        }
        *n += 1;
        picked.push(p);
        if picked.len() >= 4 {
            break;
        }
    }
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
    let scale = 0.5f32;
    let aw = ((a.width() as f32) * scale) as u32;
    let ah = ((a.height() as f32) * scale) as u32;
    let bw = ((b.width() as f32) * scale) as u32;
    let bh = ((b.height() as f32) * scale) as u32;
    let asmall = imageops_resize(a, aw, ah);
    let bsmall = imageops_resize(b, bw, bh);
    let w = aw + bw + 6;
    let h = ah.max(bh);
    let mut out = RgbaImage::from_pixel(w, h, Rgba([18, 18, 18, 255]));
    for y in 0..ah {
        for x in 0..aw {
            out.put_pixel(x, y, *asmall.get_pixel(x, y));
        }
    }
    for y in 0..bh {
        for x in 0..bw {
            out.put_pixel(x + aw + 6, y, *bsmall.get_pixel(x, y));
        }
    }
    out
}

fn imageops_resize(img: &RgbaImage, w: u32, h: u32) -> RgbaImage {
    image::imageops::resize(img, w, h, FilterType::Triangle)
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

#[test]
fn p0_real_skin_smooth_comparisons() {
    let photos = collect_photos();
    if photos.is_empty() {
        eprintln!("P0 SKIP：无 beauty-validation 真图");
        return;
    }

    let mut ok = 0usize;
    for path in &photos {
        let Ok(decoded) = decoder::decode(path) else {
            continue;
        };
        let preview = fit_within(&decoded.image, 1200);
        let Some(det) = detect(&preview, decoded.image.width(), decoded.image.height()) else {
            continue;
        };
        if det.faces.is_empty() {
            continue;
        }
        let faces: Vec<FaceFrame> = det
            .faces
            .iter()
            .map(|f| FaceFrame::from_detected(f, det.image_width, det.image_height))
            .collect();

        let mut params = EditParams::default();
        for face in &faces {
            let mut fb = FaceBeauty {
                face_id: face.id,
                face_uuid: face.uuid.clone(),
                enabled: true,
                beauty: BeautyParams::default(),
            };
            fb.beauty.skin_smooth = 55.0;
            fb.beauty.texture = 40.0;
            fb.beauty.detail = 35.0;
            fb.beauty.blemish = 45.0;
            fb.beauty.spots = 35.0;
            fb.beauty.blackheads = 20.0;
            fb.beauty.tone_even = 25.0;
            params.faces.push(fb);
        }

        let (smoothed, report) = render_with_report(
            &preview,
            &params,
            &faces,
            &CancelToken::default(),
            &mut |_, _| {},
        )
        .expect("render");
        let _ = report;

        let original = to_rgba(&preview);
        let result = to_rgba(&smoothed);

        let f0 = &faces[0];
        let cx = ((f0.x + f0.width * 0.5) * preview.width() as f32) as u32;
        let cy = ((f0.y + f0.height * 0.55) * preview.height() as f32) as u32;
        let size = ((f0.width * preview.width() as f32) * 0.45) as u32;
        let size = size.clamp(48, 240);

        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let _ = original.save(out_dir().join(format!("{stem}_original.png")));
        let _ = result.save(out_dir().join(format!("{stem}_smoothed.png")));
        let _ =
            side_by_side(&original, &result).save(out_dir().join(format!("{stem}_compare50.png")));
        let _ = zoom_crop(&original, cx, cy, size, 2)
            .save(out_dir().join(format!("{stem}_zoom200.png")));
        let _ = zoom_crop(&result, cx, cy, size, 2)
            .save(out_dir().join(format!("{stem}_zoom200_smoothed.png")));
        let _ = zoom_crop(&original, cx, cy, size, 1)
            .save(out_dir().join(format!("{stem}_zoom100.png")));
        let _ = zoom_crop(&result, cx, cy, size, 1)
            .save(out_dir().join(format!("{stem}_zoom100_smoothed.png")));

        let mut changed = 0u32;
        let total = (preview.width() * preview.height()).max(1);
        for y in 0..preview.height() {
            for x in 0..preview.width() {
                if preview.get_pixel(x, y) != smoothed.get_pixel(x, y) {
                    changed += 1;
                }
            }
        }
        let ratio = changed as f32 / total as f32;
        eprintln!("P0 {stem}: changed={ratio:.4}");
        assert!(
            ratio > 0.0001 && ratio < 0.85,
            "磨皮变化比例异常 {ratio}"
        );
        ok += 1;
    }
    assert!(ok > 0, "无真图通过磨皮对比");
    eprintln!("P0 输出目录 {}", out_dir().display());
}
