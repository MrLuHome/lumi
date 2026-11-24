//! S1.2.2 伪影逐阶段定位（dump 阶段）。
//!
//! 对用户问题照片执行检测 + 分析，产出 `{stem}.input.json` 供真实
//! `buildOneClickPlan` 桥生成参数；随后 `phase_localize` 输出 15 张阶段图
//! 与 A–H 消融、伪影检测结果。
//!
//! 三态：默认 `#[ignore]`=未执行；缺照片 panic「缺少照片导致未执行」。

use std::path::PathBuf;
use std::time::Instant;

use serde_json::{json, Value};

use lumi_retouch_lib::ai::model::MODEL_FILE;
use lumi_retouch_lib::ai::FaceEngine;
use lumi_retouch_lib::image_engine::analysis::{self, RetouchMode};
use lumi_retouch_lib::ai_engine::face_frame::{FaceFrame, FaceGeometry};
use lumi_retouch_lib::ai_engine::retouch::analyze::analyze_face;
use lumi_retouch_lib::ai_engine::skin_segmentation::build_skin_mask;
use lumi_retouch_lib::image_engine::buffer::{fit_within, Image16};
use lumi_retouch_lib::image_engine::cancel::CancelToken;
use lumi_retouch_lib::image_engine::decoder;
use lumi_retouch_lib::image_engine::params::{EditParams, ManualMask};

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
    let dir = repo_root().join("docs/assets/s1_2_2");
    std::fs::create_dir_all(&dir).expect("创建输出目录");
    dir
}

/// 首要回归样本：用户出现问题的原始照片。
fn problem_photo() -> PathBuf {
    let path = PathBuf::from("/Users/lu/Desktop/微信图片_2026-11-22_210852_641.png");
    assert!(
        path.is_file(),
        "缺少照片导致未执行：问题原图 {} 不存在",
        path.display()
    );
    path
}

fn require_phase(expected: &str) {
    let phase = std::env::var("S1_2_2_PHASE").unwrap_or_default();
    assert_eq!(
        phase, expected,
        "S1_2_2_PHASE={phase:?} 与本测试阶段 {expected:?} 不符"
    );
}

fn detect_frames(preview: &Image16, full_w: u32, full_h: u32) -> Vec<FaceFrame> {
    let model = model_path();
    assert!(model.is_file(), "缺少人脸模型 {}", model.display());
    let result = FaceEngine::default()
        .detect(&model, preview, full_w, full_h, &mut |_, _| {})
        .expect("人脸检测执行失败");
    result
        .faces
        .iter()
        .map(|face| FaceFrame::from_detected(face, full_w, full_h))
        .collect()
}

#[test]
#[ignore = "S1.2.2 dump；由 scripts/s1_2_2_artifact.sh 驱动"]
fn phase_dump() {
    require_phase("dump");
    let path = problem_photo();
    let dir = out_dir();
    let cancel = CancelToken::default();

    let decoded = decoder::decode(&path).expect("问题照片解码失败");
    let full_w = decoded.image.width();
    let full_h = decoded.image.height();
    let preview = fit_within(&decoded.image, 1800);
    let frames = detect_frames(&preview, full_w, full_h);
    assert!(!frames.is_empty(), "问题照片未检测到人脸");

    let mut suggestions = Vec::new();
    for frame in &frames {
        let mask = build_skin_mask(&preview, frame, &ManualMask::default()).expect("蒙版");
        let geometry = FaceGeometry::from_face(frame, preview.width(), preview.height()).expect("几何");
        suggestions.push(analyze_face(&preview, &mask, &geometry, &cancel).expect("美肤分析"));
    }
    let beauty = json!({
        "faces": suggestions,
        "method": "diagnostic",
        "model": "YuNet + diagnostic",
        "elapsedMs": 0,
        "skinModel": Value::Null,
        "warnings": [],
    });
    let report = analysis::analyze(&preview, &frames, &EditParams::default(), RetouchMode::Standard);
    let report: Value = serde_json::to_value(&report).expect("report");

    // detection JSON 与 s1_1/s1_2 桥兼容（faceId/uuid 对齐）。
    let detection = json!({
        "faces": frames.iter().map(|f| json!({
            "id": f.id, "uuid": f.uuid, "score": f.score,
            "x": f.x, "y": f.y, "width": f.width, "height": f.height,
            "landmarks": f.landmarks.iter().map(|p| json!({"x": p.x, "y": p.y})).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "imageWidth": preview.width(),
        "imageHeight": preview.height(),
    });

    let input = json!({
        "photo": path.to_string_lossy(),
        "category": "user_problem",
        "mode": "natural",
        "overall": 100,
        "detection": detection,
        "beauty": beauty,
        "report": report,
    });
    let stem = "user_wechat";
    std::fs::write(
        dir.join(format!("{stem}.input.json")),
        serde_json::to_string_pretty(&input).expect("json"),
    )
    .expect("写 input.json");
    std::fs::write(
        dir.join("manifest.json"),
        serde_json::to_string_pretty(&json!([
            {"category": "user_problem", "stem": stem, "photo": path.to_string_lossy()}
        ]))
        .expect("json"),
    )
    .expect("写 manifest");
    println!(
        "[dump] {} faces={} → {}",
        path.display(),
        frames.len(),
        dir.display()
    );
}

// ─────────────────────── 阶段输出与消融（phase_localize） ───────────────────────

use image::Rgba;
use lumi_retouch_lib::ai_engine::retouch::bilateral_grid::{
    bilateral_grid_smooth_debug, params_from_strength, GridDebugCapture,
};
use lumi_retouch_lib::ai_engine::retouch::fusion::{self, FusionRequest};
use lumi_retouch_lib::ai_engine::retouch::texture_protect;
use lumi_retouch_lib::ai_engine::retouch::{apply_face_retouch, frequency};
use lumi_retouch_lib::face_reshape::ai_slim::{plan_face_slim, SlimStyle};
use lumi_retouch_lib::face_reshape::contour::analyze_face_contour;
use lumi_retouch_lib::image_engine::params::FaceReshape;
use lumi_retouch_lib::image_engine::pipeline;

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

fn rgb_to_image(rgb: &[f32], w: u32, h: u32) -> Image16 {
    let mut img = Image16::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            img.put_pixel(
                x,
                y,
                Rgba([
                    (rgb[i * 3].clamp(0.0, 1.0) * 65535.0).round() as u16,
                    (rgb[i * 3 + 1].clamp(0.0, 1.0) * 65535.0).round() as u16,
                    (rgb[i * 3 + 2].clamp(0.0, 1.0) * 65535.0).round() as u16,
                    65535,
                ]),
            );
        }
    }
    img
}

/// 把 ROI 平面贴回全画布（其他区域=原图，保证坐标一致可直接对比）。
fn paste_roi(base: &Image16, roi: &[f32], x0: i32, y0: i32, rw: u32, rh: u32) -> Image16 {
    let mut out = base.clone();
    for y in 0..rh {
        for x in 0..rw {
            let gx = x0 + x as i32;
            let gy = y0 + y as i32;
            if gx < 0 || gy < 0 || gx >= out.width() as i32 || gy >= out.height() as i32 {
                continue;
            }
            let i = (y * rw + x) as usize;
            out.put_pixel(
                gx as u32,
                gy as u32,
                Rgba([
                    (roi[i * 3].clamp(0.0, 1.0) * 65535.0).round() as u16,
                    (roi[i * 3 + 1].clamp(0.0, 1.0) * 65535.0).round() as u16,
                    (roi[i * 3 + 2].clamp(0.0, 1.0) * 65535.0).round() as u16,
                    65535,
                ]),
            );
        }
    }
    out
}

fn gray_image(values: &[f32], w: u32, h: u32) -> Image16 {
    let mut img = Image16::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let v = (values[(y * w + x) as usize].clamp(0.0, 1.0) * 65535.0).round() as u16;
            img.put_pixel(x, y, Rgba([v, v, v, 65535]));
        }
    }
    img
}

fn paste_gray(base: &Image16, values: &[f32], x0: i32, y0: i32, rw: u32, rh: u32) -> Image16 {
    let mut out = base.clone();
    for y in 0..rh {
        for x in 0..rw {
            let gx = x0 + x as i32;
            let gy = y0 + y as i32;
            if gx < 0 || gy < 0 || gx >= out.width() as i32 || gy >= out.height() as i32 {
                continue;
            }
            let v = (values[(y * rw + x) as usize].clamp(0.0, 1.0) * 65535.0).round() as u16;
            out.put_pixel(gx as u32, gy as u32, Rgba([v, v, v, 65535]));
        }
    }
    out
}

fn save_jpg(img: &Image16, path: &std::path::Path) {
    let mut rgb8 = image::RgbImage::new(img.width(), img.height());
    for y in 0..img.height() {
        for x in 0..img.width() {
            let p = img.get_pixel(x, y).0;
            rgb8.put_pixel(
                x,
                y,
                image::Rgb([(p[0] / 257) as u8, (p[1] / 257) as u8, (p[2] / 257) as u8]),
            );
        }
    }
    rgb8.save(path)
        .unwrap_or_else(|e| panic!("保存 {} 失败：{e}", path.display()));
}

/// |Δ| 热力图（×8，灰度）。
fn diff_heatmap(a: &Image16, b: &Image16) -> Image16 {
    let w = a.width();
    let h = a.height();
    let mut out = vec![0f32; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let pa = a.get_pixel(x, y).0;
            let pb = b.get_pixel(x, y).0;
            let d = ((pa[0] as f32 - pb[0] as f32).abs()
                + (pa[1] as f32 - pb[1] as f32).abs()
                + (pa[2] as f32 - pb[2] as f32).abs())
                / 3.0
                / 65535.0;
            out[(y * w + x) as usize] = (d * 8.0).min(1.0);
        }
    }
    gray_image(&out, w, h)
}

fn face_beauty_of(params: &EditParams) -> lumi_retouch_lib::image_engine::params::BeautyParams {
    params
        .faces
        .iter()
        .find(|e| e.enabled)
        .map(|e| e.beauty.clone())
        .unwrap_or_default()
}

#[test]
#[ignore = "S1.2.2 定位；由 scripts/s1_2_2_artifact.sh 驱动（需先 dump+桥）"]
fn phase_localize() {
    require_phase("localize");
    let dir = out_dir();
    let stem = "user_wechat";
    let params_path = dir.join(format!("{stem}.params.natural.json"));
    assert!(
        params_path.is_file(),
        "缺少 {}：请先运行 dump 与 TS 计划桥",
        params_path.display()
    );
    let params: EditParams = serde_json::from_str(
        &std::fs::read_to_string(&params_path).expect("读 params"),
    )
    .expect("解析 params");
    let beauty = face_beauty_of(&params);
    let strength = beauty.skin_smooth / 100.0;
    assert!(strength > 0.0, "skinSmooth=0，参数未写入");

    let photo = PathBuf::from(
        serde_json::from_str::<serde_json::Value>(
            &std::fs::read_to_string(dir.join(format!("{stem}.input.json"))).expect("读 input"),
        )
        .expect("json")[ "photo"]
            .as_str()
            .expect("photo")
            .to_string(),
    );
    let cancel = CancelToken::default();
    let decoded = decoder::decode(&photo).expect("解码");
    let full = decoded.image;
    let (fw, fh) = (full.width(), full.height());
    let preview = fit_within(&full, 1800);
    let frames = detect_frames(&preview, fw, fh);
    assert!(!frames.is_empty(), "未检测到人脸");
    let frame = &frames[0];
    let geometry = FaceGeometry::from_face(frame, fw, fh).expect("几何");

    // —— 蒙版三件套（全画布坐标）——
    let mask = build_skin_mask(&full, frame, &ManualMask::default()).expect("皮肤蒙版");
    let protect =
        texture_protect::build_texture_protection(&full, &mask, &geometry, &cancel).expect("保护");
    let process_w = texture_protect::combine_with_skin(&mask.alpha, &protect);
    let (mw, mh, mx0, my0) = (mask.width, mask.height, mask.x0, mask.y0);
    let roi_n = (mw * mh) as usize;

    let original_rgba_dir = dir.join(stem);
    let _ = std::fs::remove_dir_all(&original_rgba_dir);
    std::fs::create_dir_all(&original_rgba_dir).expect("建目录");
    let sub = |name: &str| original_rgba_dir.join(name);

    save_jpg(&full, &sub("01_original.jpg"));
    save_jpg(&paste_gray(&full, &mask.alpha, mx0, my0, mw, mh), &sub("02_skin_mask.png"));
    save_jpg(&paste_gray(&full, &protect, mx0, my0, mw, mh), &sub("03_protect_mask.png"));
    save_jpg(&paste_gray(&full, &process_w, mx0, my0, mw, mh), &sub("04_process_mask.png"));

    // 保护蒙版边缘热力图（|∇protect|）
    let mut pedge = vec![0f32; roi_n];
    for y in 1..mh.saturating_sub(1) {
        for x in 1..mw.saturating_sub(1) {
            let i = (y * mw + x) as usize;
            let gx = (protect[i + 1] - protect[i - 1]).abs();
            let gy = (protect[i + mw as usize] - protect[i - mw as usize]).abs();
            pedge[i] = ((gx + gy) * 4.0).min(1.0);
        }
    }
    save_jpg(&paste_gray(&full, &pedge, mx0, my0, mw, mh), &sub("03b_protect_edge_heat.png"));

    let before = fusion::extract_roi(&full, (mx0, my0), mw, mh);

    // —— 频率分离（与引擎同参：Halo split + even_low + recombine）——
    let radius = ((mw as f32 * 0.02).round() as u32).clamp(3, 20);
    let eps = 0.004 + 0.02 * (1.0 - beauty.texture / 100.0);
    let split = frequency::split(&full, &mask, radius, eps, &cancel).expect("freq split");
    let mut low = split.low;
    let high = split.high;

    // frequency_high 可视化：0.5 + 4·high（截断）。
    let mut high_vis = vec![0.5f32; roi_n * 3];
    for i in 0..roi_n {
        for c in 0..3 {
            high_vis[i * 3 + c] = (0.5 + high[c][i] * 4.0).clamp(0.0, 1.0);
        }
    }
    let high_gray: Vec<f32> = (0..roi_n)
        .map(|i| (high_vis[i * 3] + high_vis[i * 3 + 1] + high_vis[i * 3 + 2]) / 3.0)
        .collect();
    save_jpg(&paste_gray(&full, &high_gray, mx0, my0, mw, mh), &sub("06_frequency_high.png"));
    let low_img = rgb_to_image(&concat_low(&low), mw, mh);
    save_jpg(&paste_roi(&full, &concat_low(&low), mx0, my0, mw, mh), &sub("05_frequency_low.png"));

    // C 消融：仅 even_low，recombine keep=1（不平滑）
    let tone_even = (beauty.tone_even / 100.0) * 0.7;
    let mut low_even = low.clone();
    frequency::even_low_frequency_wh(&mut low_even, mw, mh, &process_w, tone_even, &cancel)
        .expect("even_low");
    save_jpg(
        &paste_roi(&full, &recombine(&low_even, &high, 1.0), mx0, my0, mw, mh),
        &sub("C_ablation_even_only.jpg"),
    );
    let _ = low_img;

    let high_keep = (0.20
        + 0.50 * beauty.texture / 100.0
        + 0.20 * beauty.detail / 100.0)
        .clamp(0.10, 0.90);
    let mut rgb = recombine(&low_even, &high, high_keep);
    let after_recomb = rgb.clone();
    save_jpg(
        &paste_roi(&full, &rgb, mx0, my0, mw, mh),
        &sub("08_after_recombine.jpg"),
    );

    // —— 双边网格（带逐尺度捕获）——
    let mut grid_params = params_from_strength(strength * 100.0, beauty.texture, beauty.detail);
    grid_params.strength = strength;
    let mut capture = GridDebugCapture::default();
    let stats = bilateral_grid_smooth_debug(
        &mut rgb,
        mw,
        mh,
        Some(&process_w),
        &grid_params,
        &cancel,
        &mut capture,
    )
    .expect("grid");
    println!(
        "[localize] grid: levels={} cells={} splat={}ms blur={}ms slice={}ms",
        capture.level_smoothed.len(),
        stats.max_cells,
        stats.splat_ms,
        stats.blur_ms,
        stats.slice_ms
    );
    for (level, lw, lh, data) in &capture.level_smoothed {
        if *level == 0 {
            save_jpg(
                &paste_roi(&full, data, mx0, my0, mw, mh),
                &sub("09_after_grid_level0.jpg"),
            );
        } else if *level == 1 {
            save_jpg(&rgb_to_image(data, *lw, *lh), &sub("10_after_grid_level1.jpg"));
        } else if *level == 2 {
            save_jpg(&rgb_to_image(data, *lw, *lh), &sub("11_after_grid_level2.jpg"));
        } else {
            save_jpg(&rgb_to_image(data, *lw, *lh), &sub("11b_after_grid_level3.jpg"));
        }
    }
    if let Some(pyramid) = &capture.pyramid {
        save_jpg(
            &paste_roi(&full, pyramid, mx0, my0, mw, mh),
            &sub("12_after_pyramid.jpg"),
        );
    }
    // rgb 现在 = 细节回注后（纹理保护结果）
    let after_texture = rgb.clone();
    save_jpg(
        &paste_roi(&full, &after_texture, mx0, my0, mw, mh),
        &sub("13_after_texture_protect.jpg"),
    );

    // —— D 消融：仅 level 0（带蒙版）——
    let mut rgb_d = recombine(&low_even, &high, high_keep);
    let mut p0 = grid_params;
    p0.levels = 0;
    bilateral_grid_smooth_debug(&mut rgb_d, mw, mh, Some(&process_w), &p0, &cancel, &mut {
        GridDebugCapture::default()
    })
    .expect("grid L0");
    save_jpg(
        &paste_roi(&full, &rgb_d, mx0, my0, mw, mh),
        &sub("D_ablation_grid_level0.jpg"),
    );

    // —— F/14：最终皮肤融合 ——
    let mut fused = full.clone();
    fusion::fuse(
        FusionRequest {
            image: &mut fused,
            origin: (mx0, my0),
            processed_rgb: &after_texture,
            weight: &process_w,
            roi_w: mw,
            roi_h: mh,
            strength: 1.0,
        },
        &cancel,
    )
    .expect("fuse");
    save_jpg(&fused, &sub("14_after_final_fusion.jpg"));
    save_jpg(&diff_heatmap(&full, &fused), &sub("14_diff_vs_original.png"));

    // —— G：完整美肤引擎（apply_face_retouch：祛痘+磨皮+肤色+眼周）——
    let mut g_img = full.clone();
    apply_face_retouch(
        &mut g_img,
        &mask,
        &geometry,
        &beauty,
        0.0,
        &[],
        &cancel,
    )
    .expect("G apply_face_retouch");
    save_jpg(&g_img, &sub("G_full_beauty_engine.jpg"));
    save_jpg(&diff_heatmap(&full, &g_img), &sub("G_diff_vs_original.png"));

    // —— H：完整一键精修管线（含瘦脸/调色/锐化，与导出一致）——
    let mut h_params = params.clone();
    merge_reshape(&mut h_params, &frames, fw, fh, "natural", 100.0);
    let (h_img, h_report) =
        pipeline::render_with_report(&full, &h_params, &frames, &cancel, &mut |_, _| {})
            .expect("H render_with_report");
    save_jpg(&h_img, &sub("H_full_oneclick.jpg"));
    save_jpg(&diff_heatmap(&full, &h_img), &sub("H_diff_vs_original.png"));
    println!(
        "[localize] H warnings={:?} reshape={}",
        h_report.warnings,
        h_report.reshape.len()
    );

    // —— B：仅频率分离（不修改 → 输出恒等；low/high 图即 B 的产物）——
    // A：仅蒙版（01_original 即 A）。

    // —— 阶段差异热力图 ——
    for name in [
        "08_after_recombine",
        "09_after_grid_level0",
        "12_after_pyramid",
        "13_after_texture_protect",
        "D_ablation_grid_level0",
    ] {
        let src = original_rgba_dir.join(format!("{name}.jpg"));
        if src.is_file() {
            let img = image::open(&src).expect("open").to_rgb8();
            let mut as16 = Image16::new(img.width(), img.height());
            for y in 0..img.height() {
                for x in 0..img.width() {
                    let p = img.get_pixel(x, y);
                    as16.put_pixel(
                        x,
                        y,
                        Rgba([
                            p[0] as u16 * 257,
                            p[1] as u16 * 257,
                            p[2] as u16 * 257,
                            65535,
                        ]),
                    );
                }
            }
            save_jpg(
                &diff_heatmap(&full, &as16),
                &original_rgba_dir.join(format!("{name}_diff.png")),
            );
        }
    }

    // —— E 消融记录（= 主线 levels=3，见09..13）——
    std::fs::write(
        sub("stages.json"),
        serde_json::to_string_pretty(&json!({
            "photo": photo.to_string_lossy(),
            "strength": strength,
            "skinSmooth": beauty.skin_smooth,
            "texture": beauty.texture,
            "detail": beauty.detail,
            "highKeep": high_keep,
            "radius": radius,
            "roi": {"x": mx0, "y": my0, "w": mw, "h": mh},
            "grid": {"levels": grid_params.levels, "cells": stats.max_cells,
                     "splatMs": stats.splat_ms, "blurMs": stats.blur_ms, "sliceMs": stats.slice_ms},
            "levelsCaptured": capture.level_smoothed.iter().map(|(l,w,h,_)| json!({"level":l,"w":w,"h":h})).collect::<Vec<_>>(),
            "hWarnings": h_report.warnings,
        }))
        .expect("json"),
    )
    .expect("写 stages.json");
    // —— 伪影检测（区分原图既有边缘 / 算法新边缘 / 纹理损失）——
    let skin_full = paste_gray_values(&full, &process_w, mx0, my0, mw, mh);
    let protect_full = paste_gray_values(&full, &protect, mx0, my0, mw, mh);
    let skin_v = image_to_rgb(&skin_full);
    let protect_v = image_to_rgb(&protect_full);
    // 灰度图三通道相同，取 R 即可
    let skin_r: Vec<f32> = skin_v.chunks_exact(3).map(|c| c[0]).collect();
    let protect_r: Vec<f32> = protect_v.chunks_exact(3).map(|c| c[0]).collect();
    let orig_v = image_to_rgb(&full);

    let mut recomb_canvas = paste_roi(&full, &after_recomb, mx0, my0, mw, mh);
    let mut d_canvas = paste_roi(&full, &rgb_d, mx0, my0, mw, mh);
    let mut pyramid_canvas = full.clone();
    if let Some(py) = &capture.pyramid {
        pyramid_canvas = paste_roi(&full, py, mx0, my0, mw, mh);
    }
    let mut texture_canvas = paste_roi(&full, &after_texture, mx0, my0, mw, mh);

    let stages: Vec<(&str, &Image16)> = vec![
        ("B_freq_only(recombine)", &recomb_canvas),
        ("D_grid_level0", &d_canvas),
        ("12_pyramid", &pyramid_canvas),
        ("13_texture_protect", &texture_canvas),
        ("14_final_fusion", &fused),
        ("G_beauty_engine", &g_img),
        ("H_full_oneclick", &h_img),
    ];
    let mut reports = Vec::new();
    for (name, img) in stages {
        let proc_v = image_to_rgb(img);
        let r = detect_artifacts(
            name,
            &orig_v,
            &proc_v,
            &skin_r,
            &protect_r,
            fw,
            fh,
            (mx0, my0, mw, mh),
        );
        println!(
            "[detect] {name}: roi_edge×{:.2} newH={:.4} newV={:.4} lostHF={:.4} gridH@8={:.3}@32={:.3} gridV@8={:.3} steps={} blurTiles={:.3} protEdge×{:.2}",
            r["roiEdgeRatio"].as_f64().unwrap_or(0.0),
            r["newEdgeH"].as_f64().unwrap_or(0.0),
            r["newEdgeV"].as_f64().unwrap_or(0.0),
            r["lostHf"].as_f64().unwrap_or(0.0),
            r["gridCorrH8"].as_f64().unwrap_or(0.0),
            r["gridCorrH32"].as_f64().unwrap_or(0.0),
            r["gridCorrV8"].as_f64().unwrap_or(0.0),
            r["blockSteps"].as_u64().unwrap_or(0),
            r["blurTileRatio"].as_f64().unwrap_or(0.0),
            r["protectEdgeRatio"].as_f64().unwrap_or(0.0),
        );
        reports.push(r);
    }
    // 热力图（检测对象）
    save_jpg(&diff_heatmap(&full, &recomb_canvas), &sub("08_diff.png"));
    save_jpg(&diff_heatmap(&full, &d_canvas), &sub("D_diff.png"));
    save_jpg(&diff_heatmap(&full, &texture_canvas), &sub("13_diff.png"));

    std::fs::write(
        sub("detect.json"),
        serde_json::to_string_pretty(&serde_json::Value::Array(reports)).expect("json"),
    )
    .expect("写 detect.json");
    let _ = (&mut recomb_canvas, &mut d_canvas);
    println!("[localize] 阶段输出 → {}", original_rgba_dir.display());
}

/// ROI 灰度值贴回全画布（供检测器取 skin/protect 权重）。
fn paste_gray_values(base: &Image16, values: &[f32], x0: i32, y0: i32, rw: u32, rh: u32) -> Image16 {
    paste_gray(base, values, x0, y0, rw, rh)
}

/// 伪影检测：所有指标均以「原图同位置」为基线，区分天然边缘与算法新边缘。
#[allow(clippy::too_many_arguments)]
fn detect_artifacts(
    name: &str,
    orig: &[f32],
    proc: &[f32],
    skin: &[f32],
    protect: &[f32],
    w: u32,
    h: u32,
    roi: (i32, i32, u32, u32),
) -> serde_json::Value {
    let (rx0, ry0, rw, rh) = roi;
    let n = (w * h) as usize;
    let luma = |img: &[f32], i: usize| 0.299 * img[i * 3] + 0.587 * img[i * 3 + 1] + 0.114 * img[i * 3 + 2];
    let d_at = |i: usize| {
        ((proc[i * 3] - orig[i * 3]).abs()
            + (proc[i * 3 + 1] - orig[i * 3 + 1]).abs()
            + (proc[i * 3 + 2] - orig[i * 3 + 2]).abs())
            / 3.0
    };

    // 1) ROI 边缘差异：边界内 8px 带 vs 内部（≥40px）
    let mut band_sum = 0.0f64;
    let mut band_n = 0u32;
    let mut inner_sum = 0.0f64;
    let mut inner_n = 0u32;
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            if skin[i] <= 0.3 {
                continue;
            }
            let lx = x as i32 - rx0;
            let ly = y as i32 - ry0;
            let dist_edge = [
                lx,
                ly,
                rw as i32 - 1 - lx,
                rh as i32 - 1 - ly,
            ]
            .into_iter()
            .min()
            .unwrap_or(i32::MAX);
            if dist_edge >= 0 && dist_edge < 8 {
                band_sum += d_at(i) as f64;
                band_n += 1;
            } else if dist_edge >= 40 {
                inner_sum += d_at(i) as f64;
                inner_n += 1;
            }
        }
    }
    let band = if band_n > 0 { band_sum / band_n as f64 } else { 0.0 };
    let inner = if inner_n > 0 { inner_sum / inner_n as f64 } else { 1e-9 };
    let roi_edge_ratio = band / inner.max(1e-9);

    // 2) 新边缘（横/纵）与纹理损失：皮肤区，proc 梯度显著高于原图 = 新边缘；
    //    原图有梯度而 proc 大幅下降 = 纹理损失。
    let mut new_h = 0u32;
    let mut new_v = 0u32;
    let mut lost = 0u32;
    let mut edge_n = 0u32;
    let grad = |img: &[f32], a: usize, b: usize| (luma(img, a) - luma(img, b)).abs();
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let i = (y * w + x) as usize;
            if skin[i] <= 0.5 {
                continue;
            }
            edge_n += 1;
            let gh_o = grad(orig, i, i - 1);
            let gh_p = grad(proc, i, i - 1);
            let gv_o = grad(orig, i, i - w as usize);
            let gv_p = grad(proc, i, i - w as usize);
            if gh_p > gh_o * 1.5 + 0.015 {
                new_h += 1;
            }
            if gv_p > gv_o * 1.5 + 0.015 {
                new_v += 1;
            }
            let go = (gh_o + gv_o) * 0.5;
            let gp = (gh_p + gv_p) * 0.5;
            if go > 0.02 && gp < go * 0.5 {
                lost += 1;
            }
        }
    }
    let edge_n = edge_n.max(1);

    // 3) 网格周期性：差分场去均值后自相关（横向/纵向），与原图高频自相关基线对比。
    let mut diff: Vec<f32> = vec![0.0; n];
    for i in 0..n {
        if skin[i] > 0.5 {
            diff[i] = d_at(i);
        }
    }
    // 去局部均值（15×15 盒）
    let detrend = box15(&diff, w, h);
    let corr_at = |field: &[f32], lag: usize, horizontal: bool| -> f64 {
        let mut sxy = 0.0f64;
        let mut sxx = 0.0f64;
        let mut syy = 0.0f64;
        let mut cnt = 0.0f64;
        for y in 1..h.saturating_sub(1) {
            for x in 1..w.saturating_sub(1) {
                let i = (y * w + x) as usize;
                if skin[i] <= 0.5 {
                    continue;
                }
                let j = if horizontal { i + lag } else { i + lag * w as usize };
                if j >= n || skin[j] <= 0.5 {
                    continue;
                }
                let a = field[i] as f64;
                let b = field[j] as f64;
                sxy += a * b;
                sxx += a * a;
                syy += b * b;
                cnt += 1.0;
            }
        }
        if cnt < 100.0 || sxx < 1e-9 || syy < 1e-9 {
            return 0.0;
        }
        (sxy / cnt) / ((sxx / cnt).sqrt() * (syy / cnt).sqrt() + 1e-12)
    };
    // 原图亮度去均值场作为基线
    let mut lum = vec![0f32; n];
    for i in 0..n {
        lum[i] = luma(orig, i);
    }
    let lum_d = box15(&lum, w, h);
    let mut hp = vec![0f32; n];
    for i in 0..n {
        hp[i] = lum[i] - lum_d[i];
    }

    // 4) 局部色块：64×64 相邻块均值跳变（相对原图跳变的新增量）
    let tile = 64usize;
    let tw = (w as usize + tile - 1) / tile;
    let th = (h as usize + tile - 1) / tile;
    let mut block_steps = 0u32;
    let mut blur_tiles_num = 0u32;
    let mut blur_tiles_den = 0u32;
    let tile_stats = |img: &[f32], tx: usize, ty: usize| -> (f32, f32) {
        let mut s = 0.0f64;
        let mut sq = 0.0f64;
        let mut cnt = 0.0f64;
        for y in ty * tile..((ty + 1) * tile).min(h as usize) {
            for x in tx * tile..((tx + 1) * tile).min(w as usize) {
                let i = y * w as usize + x;
                if skin[i] <= 0.5 {
                    continue;
                }
                let v = luma(img, i) as f64;
                s += v;
                sq += v * v;
                cnt += 1.0;
            }
        }
        if cnt < 30.0 {
            return (f32::NAN, f32::NAN);
        }
        let m = s / cnt;
        let var = (sq / cnt - m * m).max(0.0);
        (m as f32, var.sqrt() as f32)
    };
    for ty in 0..th {
        for tx in 0..tw {
            let (mo, so) = tile_stats(orig, tx, ty);
            let (mp, sp) = tile_stats(proc, tx, ty);
            if mo.is_nan() || mp.is_nan() {
                continue;
            }
            if so > 0.01 {
                blur_tiles_den += 1;
                if sp < so * 0.6 {
                    blur_tiles_num += 1;
                }
            }
            // 右邻
            if tx + 1 < tw {
                let (mo2, _) = tile_stats(orig, tx + 1, ty);
                let (mp2, _) = tile_stats(proc, tx + 1, ty);
                if !mo2.is_nan() && !mp2.is_nan() {
                    let step_o = (mo - mo2).abs();
                    let step_p = (mp - mp2).abs();
                    if step_p > step_o + 0.010 {
                        block_steps += 1;
                    }
                }
            }
            // 下邻
            if ty + 1 < th {
                let (mo2, _) = tile_stats(orig, tx, ty + 1);
                let (mp2, _) = tile_stats(proc, tx, ty + 1);
                if !mo2.is_nan() && !mp2.is_nan() {
                    let step_o = (mo - mo2).abs();
                    let step_p = (mp - mp2).abs();
                    if step_p > step_o + 0.010 {
                        block_steps += 1;
                    }
                }
            }
        }
    }

    // 5) 保护边缘光晕：|∇protect| 处 proc/orig 梯度比
    let mut pe_p = 0.0f64;
    let mut pe_o = 0.0f64;
    let mut pe_n = 0u32;
    for y in 2..h.saturating_sub(2) {
        for x in 2..w.saturating_sub(2) {
            let i = (y * w + x) as usize;
            let gpx = (protect[i + 1] - protect[i - 1]).abs();
            let gpy = (protect[i + w as usize] - protect[i - w as usize]).abs();
            if gpx + gpy < 0.05 || skin[i] <= 0.3 {
                continue;
            }
            pe_o += grad(orig, i, i - 1) as f64 + grad(orig, i, i - w as usize) as f64;
            pe_p += grad(proc, i, i - 1) as f64 + grad(proc, i, i - w as usize) as f64;
            pe_n += 1;
        }
    }
    let protect_edge_ratio = if pe_n > 0 && pe_o > 1e-9 {
        (pe_p / pe_n as f64) / (pe_o / pe_n as f64)
    } else {
        1.0
    };

    json!({
        "stage": name,
        "roiEdgeRatio": roi_edge_ratio,
        "newEdgeH": new_h as f32 / edge_n as f32,
        "newEdgeV": new_v as f32 / edge_n as f32,
        "lostHf": lost as f32 / edge_n as f32,
        "gridCorrH4": corr_at(&detrend, 4, true),
        "gridCorrH8": corr_at(&detrend, 8, true),
        "gridCorrH16": corr_at(&detrend, 16, true),
        "gridCorrH32": corr_at(&detrend, 32, true),
        "gridCorrV8": corr_at(&detrend, 8, false),
        "gridCorrV32": corr_at(&detrend, 32, false),
        "baselineCorrH8": corr_at(&hp, 8, true),
        "baselineCorrH32": corr_at(&hp, 32, true),
        "blockSteps": block_steps,
        "blurTileRatio": if blur_tiles_den > 0 {
            blur_tiles_num as f32 / blur_tiles_den as f32
        } else { 0.0 },
        "protectEdgeRatio": protect_edge_ratio,
    })
}

/// 15×15 盒均值（去趋势用）。
fn box15(src: &[f32], w: u32, h: u32) -> Vec<f32> {
    let r = 7i32;
    let mut out = vec![0f32; src.len()];
    for y in 0..h {
        for x in 0..w {
            let mut s = 0.0f32;
            let mut c = 0.0f32;
            for dy in -r..=r {
                let yy = y as i32 + dy;
                if yy < 0 || yy >= h as i32 {
                    continue;
                }
                for dx in -r..=r {
                    let xx = x as i32 + dx;
                    if xx < 0 || xx >= w as i32 {
                        continue;
                    }
                    s += src[(yy as u32 * w + xx as u32) as usize];
                    c += 1.0;
                }
            }
            out[(y * w + x) as usize] = s / c.max(1.0);
        }
    }
    out
}

fn concat_low(low: &[Vec<f32>; 3]) -> Vec<f32> {
    let n = low[0].len();
    let mut out = vec![0f32; n * 3];
    for c in 0..3 {
        for i in 0..n {
            out[i * 3 + c] = low[c][i];
        }
    }
    out
}

fn recombine(low: &[Vec<f32>; 3], high: &[Vec<f32>; 3], keep: f32) -> Vec<f32> {
    let n = low[0].len();
    let mut out = vec![0f32; n * 3];
    for c in 0..3 {
        for i in 0..n {
            out[i * 3 + c] = (low[c][i] + high[c][i] * keep).clamp(0.0, 1.0);
        }
    }
    out
}

fn merge_reshape(
    params: &mut EditParams,
    frames: &[FaceFrame],
    image_w: u32,
    image_h: u32,
    mode: &str,
    strength: f32,
) -> u32 {
    let style = match mode {
        "refined" => SlimStyle::Refined,
        "advanced" => SlimStyle::Advanced,
        _ => SlimStyle::Natural,
    };
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
        if let Some(entry) = params.reshape.iter_mut().find(|e| e.face_id == face_id) {
            entry.enabled = true;
            entry.face = shape;
        } else {
            params.reshape.push(FaceReshape {
                face_id,
                face_uuid: contour.face_uuid.clone(),
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

// ─────────────────── §4.2 网格实验：点 Splat vs 三线性 Splat ───────────────────

fn mean_grad(rgb: &[f32], w: u32, h: u32, skin: &[f32]) -> f64 {
    let mut s = 0.0f64;
    let mut n = 0.0f64;
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let i = (y * w + x) as usize;
            if skin[i] <= 0.5 {
                continue;
            }
            let g = ((rgb[i * 3] - rgb[(i - 1) * 3]).abs()
                + rgb[i * 3]
                    - rgb[(i - w as usize) * 3])
                .abs();
            let g2 = ((rgb[i * 3] - rgb[(i - 1) * 3]).abs()
                + (rgb[i * 3] - rgb[(i - w as usize) * 3]).abs())
                * 0.5;
            let _ = g;
            s += g2 as f64;
            n += 1.0;
        }
    }
    s / n.max(1.0)
}

fn long_straight_score(out: &[f32], base: &[f32], w: u32, h: u32, skin: &[f32]) -> u32 {
    // (out-base) 差分场中 ≥32px 的轴对齐长直台阶（方块边界特征）。
    let mut total = 0u32;
    let d = |i: usize| {
        ((out[i * 3] - base[i * 3]).abs()
            + (out[i * 3 + 1] - base[i * 3 + 1]).abs()
            + (out[i * 3 + 2] - base[i * 3 + 2]).abs())
            / 3.0
    };
    // 垂直边：列方向 |Δ_x d| 连续段
    for x in 1..w.saturating_sub(1) {
        let mut run = 0u32;
        for y in 1..h.saturating_sub(1) {
            let i = (y * w + x) as usize;
            let j = i - 1;
            let edge = skin[i] > 0.5
                && ((d(i) - d(j)).abs() > 0.012)
                && d(i).max(d(j)) > 0.02;
            if edge {
                run += 1;
                if run == 32 {
                    total += 1;
                }
            } else {
                run = 0;
            }
        }
    }
    // 水平边：行方向
    for y in 1..h.saturating_sub(1) {
        let mut run = 0u32;
        for x in 1..w.saturating_sub(1) {
            let i = (y * w + x) as usize;
            let j = i - w as usize;
            let edge = skin[i] > 0.5
                && ((d(i) - d(j)).abs() > 0.012)
                && d(i).max(d(j)) > 0.02;
            if edge {
                run += 1;
                if run == 32 {
                    total += 1;
                }
            } else {
                run = 0;
            }
        }
    }
    total
}

#[test]
#[ignore = "S1.2.2 §4.2 网格实验；由 scripts/s1_2_2_artifact.sh 驱动"]
fn phase_grid_experiment() {
    require_phase("gridexp");
    use lumi_retouch_lib::ai_engine::retouch::bilateral_grid::{
        bilateral_grid_smooth_splat_experiment, bilateral_grid_smooth_with_stats,
        BilateralGridParams, SplatMode,
    };

    let photo = problem_photo();
    let cancel = CancelToken::default();
    let decoded = decoder::decode(&photo).expect("解码");
    let preview = fit_within(&decoded.image, 1800);
    let frames = detect_frames(&preview, preview.width(), preview.height());
    assert!(!frames.is_empty());
    let frame = &frames[0];
    let geometry = FaceGeometry::from_face(frame, preview.width(), preview.height()).expect("几何");
    let mask = build_skin_mask(&preview, frame, &ManualMask::default()).expect("蒙版");
    let protect =
        texture_protect::build_texture_protection(&preview, &mask, &geometry, &cancel).expect("protect");
    let process_w = texture_protect::combine_with_skin(&mask.alpha, &protect);
    let (mw, mh) = (mask.width, mask.height);
    let mut rgb = fusion::extract_roi(&preview, (mask.x0, mask.y0), mw, mh);

    // 统一基底：频率分离 + even + recombine（与引擎参数一致，natural）
    let beauty_skinsmooth = {
        let p: EditParams = serde_json::from_str(
            &std::fs::read_to_string(out_dir().join("user_wechat.params.natural.json"))
                .expect("params"),
        )
        .expect("parse");
        p.faces
            .iter()
            .find(|e| e.enabled)
            .map(|e| e.beauty.clone())
            .unwrap_or_default()
    };
    let radius = ((mw as f32 * 0.02).round() as u32).clamp(3, 20);
    let eps = 0.004 + 0.02 * (1.0 - beauty_skinsmooth.texture / 100.0);
    let split = frequency::split(&preview, &mask, radius, eps, &cancel).expect("split");
    let mut low = split.low;
    let high = split.high;
    let tone_even = (beauty_skinsmooth.tone_even / 100.0) * 0.7;
    frequency::even_low_frequency_wh(&mut low, mw, mh, &process_w, tone_even, &cancel)
        .expect("even");
    let high_keep = (0.20
        + 0.50 * beauty_skinsmooth.texture / 100.0
        + 0.20 * beauty_skinsmooth.detail / 100.0)
        .clamp(0.10, 0.90);
    for c in 0..3 {
        for i in 0..mw as usize * mh as usize {
            rgb[i * 3 + c] = (low[c][i] + high[c][i] * high_keep).clamp(0.0, 1.0);
        }
    }
    let base = rgb.clone();
    let skin: Vec<f32> = {
        let mut s = vec![0f32; (mw * mh) as usize];
        for y in 0..mh {
            for x in 0..mw {
                let gx = mask.x0 + x as i32;
                let gy = mask.y0 + y as i32;
                if gx >= 0 && gy >= 0 && gx < preview.width() as i32 && gy < preview.height() as i32 {
                    s[(y * mw + x) as usize] = process_w[(y * mw + x) as usize];
                }
            }
        }
        s
    };

    let strength = beauty_skinsmooth.skin_smooth / 100.0;
    let mut rows = Vec::new();
    for splat_point in [true, false] {
        for sigma_s in [4.0f32, 8.0, 14.0] {
            for sigma_r in [0.08f32, 0.15] {
                for levels in [0u32, 3] {
                    let mut p = BilateralGridParams {
                        sigma_s,
                        sigma_r,
                        levels,
                        strength,
                        detail_threshold: (0.10
                            + 0.35 * beauty_skinsmooth.texture / 100.0
                            + 0.15 * beauty_skinsmooth.detail / 100.0)
                            .clamp(0.05, 0.7),
                        detail_keep_floor: (0.15
                            + 0.45 * (beauty_skinsmooth.texture / 100.0))
                            .clamp(0.10, 0.75),
                    };
                    p.strength = strength;
                    let mut out = base.clone();
                    let started = Instant::now();
                    if splat_point {
                        bilateral_grid_smooth_with_stats(
                            &mut out,
                            mw,
                            mh,
                            Some(&process_w),
                            &p,
                            &cancel,
                        )
                        .expect("grid point");
                    } else {
                        bilateral_grid_smooth_splat_experiment(
                            &mut out,
                            mw,
                            mh,
                            Some(&process_w),
                            &p,
                            &cancel,
                        )
                        .expect("grid bilinear");
                    }
                    let ms = started.elapsed().as_millis();
                    let hf_keep = mean_grad(&out, mw, mh, &skin) / mean_grad(&base, mw, mh, &skin).max(1e-9);
                    let blocks = long_straight_score(&out, &base, mw, mh, &skin);
                    let mode = if splat_point { "point" } else { "bilinear" };
                    println!(
                        "[gridexp] splat={mode:8} ss={sigma_s} sr={sigma_r} lv={levels} hf_keep={hf_keep:.3} blocks={blocks} {ms}ms"
                    );
                    rows.push(json!({
                        "splat": mode, "sigmaS": sigma_s, "sigmaR": sigma_r, "levels": levels,
                        "hfKeep": hf_keep, "blocks": blocks, "ms": ms,
                    }));
                }
            }
        }
    }
    let _ = SplatMode::Point;
    let count = rows.len();
    std::fs::write(
        out_dir().join("grid_experiment.json"),
        serde_json::to_string_pretty(&serde_json::Value::Array(rows)).expect("json"),
    )
    .expect("写 grid_experiment.json");
    println!("[gridexp] {count} 组完成 → grid_experiment.json");
}
