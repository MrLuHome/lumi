//! S1.1 P1：高分辨率内存与性能验收（3MP / 12MP / 24MP / 45MP）。
//!
//! 运行方式（必须 release，否则 debug 数学过慢）：
//! ```text
//! cargo test --release --test s1_1_performance -- --ignored --nocapture
//! ```
//!
//! 三态约定：
//! - 默认 `cargo test` → `ignored`（**未执行**，不会显示为通过）；
//! - `--ignored` 且照片存在 → 断言通过/失败；
//! - `--ignored` 且缺少 beauty-validation 照片 → **panic**（明确报「缺少照片」，禁止假通过）。
//!
//! 记录项：单个网格缓冲大小、总峰值内存（RSS）、各尺度耗时、Splat/Blur/Slice、
//! 总处理时间、CPU 时间。使用真实照片内容 + 与 `run_skin_retouch` 相同的处理管线
//! （检测 → 皮肤蒙版 → 纹理保护 → 频率分离 → 双边网格 → 质量 → 融合）。

use std::path::PathBuf;
use std::time::Instant;

use image::RgbaImage;

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
use lumi_retouch_lib::ai_engine::skin_segmentation::{build_skin_mask, SkinMask};
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

/// 真实照片源（beauty-validation 首张 JPG）。
fn source_photo() -> PathBuf {
    let root = repo_root().join("beauty-validation");
    let mut all: Vec<PathBuf> = Vec::new();
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
            if matches!(ext.as_deref(), Some("jpg" | "jpeg")) {
                all.push(p);
            }
        }
    }
    all.sort();
    if let Some(p) = all.iter().find(|p| p.to_string_lossy().contains("01-acne")) {
        return p.clone();
    }
    all.into_iter()
        .next()
        .unwrap_or_else(|| panic!("缺少照片导致未执行：beauty-validation 下未找到 JPG 真实照片"))
}

fn peak_rss_bytes() -> u64 {
    unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut usage) != 0 {
            return 0;
        }
        #[cfg(target_os = "macos")]
        {
            usage.ru_maxrss as u64
        }
        #[cfg(not(target_os = "macos"))]
        {
            (usage.ru_maxrss as u64) * 1024
        }
    }
}

fn cpu_seconds() -> f64 {
    unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut usage) != 0 {
            return 0.0;
        }
        usage.ru_utime.tv_sec as f64
            + usage.ru_utime.tv_usec as f64 / 1e6
            + usage.ru_stime.tv_sec as f64
            + usage.ru_stime.tv_usec as f64 / 1e6
    }
}

fn image16_to_rgba(img: &Image16) -> RgbaImage {
    let mut out = RgbaImage::new(img.width(), img.height());
    for y in 0..img.height() {
        for x in 0..img.width() {
            let p = img.get_pixel(x, y).0;
            out.put_pixel(
                x,
                y,
                image::Rgba([(p[0] / 257) as u8, (p[1] / 257) as u8, (p[2] / 257) as u8, 255]),
            );
        }
    }
    out
}

fn rgba_to_image16(img: &RgbaImage) -> Image16 {
    let mut out = Image16::new(img.width(), img.height());
    for y in 0..img.height() {
        for x in 0..img.width() {
            let p = img.get_pixel(x, y).0;
            out.put_pixel(
                x,
                y,
                image::Rgba([
                    p[0] as u16 * 257,
                    p[1] as u16 * 257,
                    p[2] as u16 * 257,
                    65535,
                ]),
            );
        }
    }
    out
}

fn resize_to(img: &Image16, w: u32, h: u32) -> Image16 {
    if img.width() == w && img.height() == h {
        return img.clone();
    }
    let rgba = image16_to_rgba(img);
    let resized = image::imageops::resize(&rgba, w, h, image::imageops::FilterType::Triangle);
    rgba_to_image16(&resized)
}

fn detect(preview: &Image16, full_w: u32, full_h: u32) -> Option<FaceDetectionResult> {
    if !model_path().is_file() {
        return None;
    }
    FaceEngine::default()
        .detect(&model_path(), preview, full_w, full_h, &mut |_, _| {})
        .ok()
}

fn face_frame(preview: &Image16, full: &Image16) -> FaceFrame {
    let det = detect(preview, full.width(), full.height())
        .expect("人脸检测执行失败");
    assert!(!det.faces.is_empty(), "真实照片未检测到人脸");
    FaceFrame::from_detected(&det.faces[0], det.image_width, det.image_height)
}

#[test]
#[ignore = "需要 beauty-validation 真实照片与 release 运行；cargo test --release --test s1_1_performance -- --ignored --nocapture"]
fn highres_memory_and_performance_3_12_24_45mp() {
    let photo = source_photo();
    eprintln!("source photo: {}", photo.display());
    let decoded = decoder::decode(&photo).expect("解码真实照片");
    let base = decoded.image;

    // 真实图像内容 → 目标分辨率（面积近似目标 MP，保持 4:3）。
    let targets: [(&str, u32, u32); 4] = [
        ("3MP", 2000, 1500),
        ("12MP", 4000, 3000),
        ("24MP", 6000, 4000),
        ("45MP", 9000, 5000),
    ];
    for (label, w, h) in targets {
        let t = Instant::now();
        let image = resize_to(&base, w, h);
        eprintln!("[{label}] resize {}x{} took {}ms", w, h, t.elapsed().as_millis());
        let mut img = image;
        run_pipeline_mutable(label, &mut img);
    }
}

fn run_pipeline_mutable(label: &str, image: &mut Image16) {
    // 与 run_pipeline 相同，但持有 &mut 以便融合原地写回（真实管线路径）。
    let cancel = CancelToken::default();
    let t_total = Instant::now();
    let cpu0 = cpu_seconds();

    let t = Instant::now();
    let preview = fit_within(image, 1200);
    let frame = face_frame(&preview, image);
    let detect_ms = t.elapsed().as_millis();

    let t = Instant::now();
    let mask: SkinMask = build_skin_mask(image, &frame, &ManualMask::default()).expect("mask");
    let mask_ms = t.elapsed().as_millis();

    let t = Instant::now();
    let geometry = FaceGeometry::from_face(&frame, image.width(), image.height()).expect("geom");
    let protect = texture_protect::build_texture_protection(image, &mask, &geometry, &cancel)
        .expect("protect");
    let process_w = tp::combine_with_skin(&mask.alpha, &protect);
    let protect_ms = t.elapsed().as_millis();

    let (mw, mh) = (mask.width, mask.height);
    let n = (mw * mh) as usize;
    assert!(n > 1000, "{label} ROI 过小 n={n}");

    let t = Instant::now();
    let before = fusion::extract_roi(image, (mask.x0, mask.y0), mw, mh);
    let mut rgb = before.clone();
    let extract_ms = t.elapsed().as_millis();

    let t = Instant::now();
    let radius = ((mw as f32 * 0.02).round() as u32).clamp(3, 20);
    let texture = 40.0f32;
    let detail = 35.0f32;
    let eps = 0.004 + 0.02 * (1.0 - texture / 100.0);
    let split = frequency::split(image, &mask, radius, eps, &cancel).expect("freq split");
    let mut low = split.low;
    let high = split.high;
    let tone_even = 25.0f32 / 100.0 * 0.7;
    frequency::even_low_frequency_wh(&mut low, mw, mh, &process_w, tone_even, &cancel)
        .expect("even low");
    let high_keep = (0.20 + 0.50 * texture / 100.0 + 0.20 * detail / 100.0).clamp(0.10, 0.90);
    for i in 0..n {
        for c in 0..3 {
            rgb[i * 3 + c] = (low[c][i] + high[c][i] * high_keep).clamp(0.0, 1.0);
        }
    }
    let freq_ms = t.elapsed().as_millis();

    let t = Instant::now();
    let strength = 0.55f32;
    let mut grid_params = params_from_strength(strength * 100.0, texture, detail);
    grid_params.strength = strength;
    let stats =
        bilateral_grid_smooth_with_stats(&mut rgb, mw, mh, Some(&process_w), &grid_params, &cancel)
            .expect("grid");
    let grid_ms = t.elapsed().as_millis();

    let t = Instant::now();
    let q = quality::evaluate(&before, &rgb, &mask.alpha, &protect, mw, mh);
    let quality_ms = t.elapsed().as_millis();

    let t = Instant::now();
    let report = fusion::fuse(
        FusionRequest {
            image,
            origin: (mask.x0, mask.y0),
            processed_rgb: &rgb,
            weight: &process_w,
            roi_w: mw,
            roi_h: mh,
            strength: 1.0,
        },
        &cancel,
    )
    .expect("fuse");
    let fuse_ms = t.elapsed().as_millis();

    for v in rgb.iter() {
        assert!(v.is_finite(), "{label} 网格输出含非有限值");
    }
    assert!(report.fused_pixels > 100, "{label} 融合像素过少");
    let mut changed = 0u64;
    for i in 0..n {
        for c in 0..3 {
            if (rgb[i * 3 + c] - before[i * 3 + c]).abs() > 1e-4 {
                changed += 1;
                break;
            }
        }
    }
    assert!(changed > 0, "{label} 管线未产生任何变化（空转）");

    let total_ms = t_total.elapsed().as_millis();
    let cpu_delta = cpu_seconds() - cpu0;
    let rss = peak_rss_bytes();

    eprintln!(
        "=== S1.1 PERF {label} ({}x{}, ROI {mw}x{mh}) ===",
        image.width(),
        image.height()
    );
    eprintln!(
        "  stages_ms: detect={detect_ms} mask={mask_ms} protect={protect_ms} extract={extract_ms} freq={freq_ms} grid={grid_ms} quality={quality_ms} fuse={fuse_ms} TOTAL={total_ms}"
    );
    eprintln!(
        "  grid_splat_ms={} grid_blur_ms={} grid_slice_ms={}",
        stats.splat_ms, stats.blur_ms, stats.slice_ms
    );
    for (level, down, proc, blend) in &stats.level_ms {
        eprintln!("  level L{level}: downsample={down}ms process={proc}ms blend={blend}ms");
    }
    eprintln!(
        "  grid_buffer: max_cells={} max_grid_bytes={} ({:.1} MiB) blur_tmp_bytes={} ({:.1} MiB)",
        stats.max_cells,
        stats.max_grid_bytes,
        stats.max_grid_bytes as f64 / (1024.0 * 1024.0),
        stats.max_blur_tmp_bytes,
        stats.max_blur_tmp_bytes as f64 / (1024.0 * 1024.0),
    );
    eprintln!(
        "  peak_rss_bytes={rss} ({:.2} GiB)",
        rss as f64 / (1024.0 * 1024.0 * 1024.0)
    );
    eprintln!("  cpu_seconds={cpu_delta:.3}");
    eprintln!(
        "  quality: skin_keep={:.3} over_smooth_risk={} protected_leak={} fused={}",
        q.skin.texture_keep_ratio, q.over_smooth_risk, q.protected_leak, report.fused_pixels
    );

    assert!(
        rss < 12 * 1024 * 1024 * 1024,
        "{label} 峰值内存异常：{} GiB",
        rss as f64 / (1024.0 * 1024.0 * 1024.0)
    );
    assert!(
        stats.max_grid_bytes < 512 * 1024 * 1024,
        "{label} 网格缓冲过大：{}",
        stats.max_grid_bytes
    );
}
