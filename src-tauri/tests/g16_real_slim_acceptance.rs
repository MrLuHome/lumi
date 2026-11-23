//! G16 · 真实照片验收：瘦脸像素对比与质量门。
//!
//! 素材：`beauty-validation/`（gitignore）。缺失时跳过，不伪造。
//! 输出：`docs/assets/g16_slim/{stem}_original|faceslim|compare.png`

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use image::{Rgba, RgbaImage};

use lumi_retouch_lib::ai::face::FaceDetectionResult;
use lumi_retouch_lib::ai::model::MODEL_FILE;
use lumi_retouch_lib::ai::FaceEngine;
use lumi_retouch_lib::ai_engine::face_frame::FaceFrame;
use lumi_retouch_lib::face_reshape::ai_slim::{plan_face_slim, SlimStyle};
use lumi_retouch_lib::face_reshape::contour::analyze_face_contour;
use lumi_retouch_lib::image_engine::buffer::{fit_within, Image16};
use lumi_retouch_lib::image_engine::cancel::CancelToken;
use lumi_retouch_lib::image_engine::decoder;
use lumi_retouch_lib::image_engine::params::{EditParams, FaceReshape};
use lumi_retouch_lib::image_engine::pipeline::render_with_report;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("manifest parent")
        .to_path_buf()
}

fn model_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("resources/models/yunet")
        .join(MODEL_FILE)
}

fn out_dir() -> PathBuf {
    let dir = repo_root().join("docs/assets/g16_slim");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn collect_jpegs() -> Vec<PathBuf> {
    let root = repo_root().join("beauty-validation");
    if !root.is_dir() {
        return Vec::new();
    }
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(&root).ok().into_iter().flatten() {
        let path = entry.ok().map(|e| e.path()).unwrap_or_default();
        if !path.is_dir() {
            continue;
        }
        for file in std::fs::read_dir(&path).ok().into_iter().flatten() {
            let p = file.ok().map(|e| e.path()).unwrap_or_default();
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase());
            if matches!(ext.as_deref(), Some("jpg" | "jpeg" | "png")) {
                paths.push(p);
            }
        }
    }
    paths.sort();
    let mut per_dir: HashMap<String, usize> = Default::default();
    let mut picked = Vec::new();
    for p in paths {
        let parent = p
            .parent()
            .and_then(|d| d.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let n = per_dir.entry(parent).or_insert(0);
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
    let w = a.width() + b.width() + 8;
    let h = a.height().max(b.height());
    let mut out = RgbaImage::from_pixel(w, h, Rgba([24, 24, 24, 255]));
    for y in 0..a.height() {
        for x in 0..a.width() {
            out.put_pixel(x, y, *a.get_pixel(x, y));
        }
    }
    for y in 0..b.height() {
        for x in 0..b.width() {
            out.put_pixel(x + a.width() + 8, y, *b.get_pixel(x, y));
        }
    }
    out
}

fn detect_faces(preview: &Image16, full_w: u32, full_h: u32) -> Option<FaceDetectionResult> {
    if !model_path().is_file() {
        return None;
    }
    FaceEngine::default()
        .detect(&model_path(), preview, full_w, full_h, &mut |_, _| {})
        .ok()
}

fn frames(result: &FaceDetectionResult) -> Vec<FaceFrame> {
    result
        .faces
        .iter()
        .map(|face| FaceFrame::from_detected(face, result.image_width, result.image_height))
        .collect()
}

fn changed_ratio(a: &Image16, b: &Image16) -> f32 {
    let mut changed = 0u32;
    let total = (a.width() * a.height()).max(1);
    for y in 0..a.height() {
        for x in 0..a.width() {
            if a.get_pixel(x, y) != b.get_pixel(x, y) {
                changed += 1;
            }
        }
    }
    changed as f32 / total as f32
}

#[test]
fn g16_real_portrait_face_slim_writes_comparisons() {
    let photos = collect_jpegs();
    if photos.is_empty() {
        eprintln!("G16 SKIP：未找到 beauty-validation 真图。合成契约见 g13_slim_algorithms。");
        return;
    }

    let mut accepted = 0usize;
    let mut side_cases = 0usize;

    for path in &photos {
        let Ok(decoded) = decoder::decode(path) else {
            continue;
        };
        // 验收跑在预览分辨率，避免 24MP 拖垮测试。
        let preview = fit_within(&decoded.image, 1200);
        let Some(detection) = detect_faces(
            &preview,
            decoded.image.width(),
            decoded.image.height(),
        ) else {
            eprintln!("G16 无模型或检测失败：{}", path.display());
            continue;
        };
        if detection.faces.is_empty() {
            eprintln!("G16 无人脸：{}", path.display());
            continue;
        }
        // from_detected 用 full 尺寸得到归一化坐标，与 preview 尺寸兼容。
        let faces = frames(&detection);

        let mut params = EditParams::default();
        let mut any = false;
        for face in &faces {
            let Ok(contour) =
                analyze_face_contour(face, preview.width(), preview.height())
            else {
                continue;
            };
            if contour.notes.iter().any(|n| n.contains("侧脸")) {
                side_cases += 1;
            }
            let (shape, notes) = plan_face_slim(&contour, SlimStyle::Advanced, 70.0);
            if shape.is_neutral() {
                continue;
            }
            let mut entry = FaceReshape::new(face.id);
            entry.face_uuid = face.uuid.clone();
            entry.face = shape;
            let slim_value = entry.face.slim;
            params.reshape.push(entry);
            any = true;
            eprintln!(
                "G16 {} face={} slim={} notes={:?}",
                path.file_name().unwrap_or_default().to_string_lossy(),
                face.id,
                slim_value,
                notes
            );
        }
        if !any {
            continue;
        }

        let (rendered, report) = render_with_report(
            &preview,
            &params,
            &faces,
            &CancelToken::default(),
            &mut |_, _| {},
        )
        .expect("render");

        for r in &report.reshape {
            assert_eq!(
                r.flipped_triangles, 0,
                "真图瘦脸不得出现三角翻转：{}",
                path.display()
            );
        }

        let ratio = changed_ratio(&preview, &rendered);
        assert!(
            ratio > 0.0001 && ratio < 0.5,
            "变化比例异常 {:.4}：{}",
            ratio,
            path.display()
        );

        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "photo".into());
        let original = to_rgba(&preview);
        let result = to_rgba(&rendered);
        let _ = original.save(out_dir().join(format!("{stem}_original.png")));
        let _ = result.save(out_dir().join(format!("{stem}_faceslim.png")));
        let _ =
            side_by_side(&original, &result).save(out_dir().join(format!("{stem}_compare.png")));
        accepted += 1;
    }

    assert!(
        accepted > 0,
        "beauty-validation 存在但没有任何真图通过瘦脸验收"
    );
    eprintln!(
        "G16 通过 {accepted} 张真图瘦脸；侧脸标记 {side_cases}；输出 {}",
        out_dir().display()
    );
}

#[test]
fn g16_output_readme() {
    let dir = out_dir();
    let readme = dir.join("README.md");
    if !readme.exists() {
        let _ = std::fs::write(
            &readme,
            "# G16 真图瘦脸对比\n\n\
             - `{stem}_original.png` 原图（预览分辨率）\n\
             - `{stem}_faceslim.png` 瘦脸后\n\
             - `{stem}_compare.png` 左右对比\n\n\
             素材 `beauty-validation/`（不入库）。质量门：无三角翻转、变化比例 ∈ (0, 0.5)。\n\
             身体瘦身需人物蒙版；无分割权重时不伪造，见 `tests/g13_slim_algorithms.rs`。\n",
        );
    }
    assert!(readme.exists());
}
