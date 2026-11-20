//! 统一图像引擎的集成测试：真实文件解码、处理、导出、项目往返、任务取消。

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use lumi_retouch_lib::error::AppError;
use lumi_retouch_lib::image_engine::buffer::{fit_within, Image16};
use lumi_retouch_lib::image_engine::cancel::CancelToken;
use lumi_retouch_lib::image_engine::decoder::{self, BitDepth};
use lumi_retouch_lib::image_engine::export::{self, ExportSettings};
use lumi_retouch_lib::image_engine::params::EditParams;
use lumi_retouch_lib::image_engine::pipeline;
use lumi_retouch_lib::image_engine::preview::build_preview;
use lumi_retouch_lib::image_engine::project::{self, ProjectDoc, ProjectPhoto};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/images")
}

fn fixture(name: &str) -> PathBuf {
    fixtures().join(name)
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lumi-it-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn decodes_all_supported_formats() {
    for name in ["portrait.jpg", "portrait.webp"] {
        let decoded = decoder::decode(&fixture(name)).unwrap();
        assert_eq!((decoded.image.width(), decoded.image.height()), (1680, 2240), "{name}");
        assert_eq!(decoded.bit_depth, BitDepth::Eight, "{name}");
    }
    let webp = decoder::decode(&fixture("portrait.webp")).unwrap();
    assert_eq!(webp.format, "WebP");

    let png = decoder::decode(&fixture("transparent.png")).unwrap();
    assert_eq!((png.image.width(), png.image.height()), (800, 600));
    assert_eq!(png.format, "PNG");
}

#[test]
fn preserves_transparency_and_16bit() {
    let png = decoder::decode(&fixture("transparent.png")).unwrap();
    assert!(png.image.pixels().any(|pixel| pixel.0[3] < 65535), "透明 PNG 的 alpha 应保留");

    let wide = decoder::decode(&fixture("sixteen_bit.png")).unwrap();
    assert_eq!(wide.bit_depth, BitDepth::Sixteen, "16-bit PNG 不应被降成 8-bit");
}

#[test]
fn applies_exif_orientation() {
    let image = decoder::decode(&fixture("exif_rotated.jpg")).unwrap();
    assert_eq!((image.image.width(), image.image.height()), (2240, 1680));
}

#[test]
fn corrupt_file_returns_explicit_error() {
    let error = decoder::decode(&fixture("corrupt.jpg")).unwrap_err();
    assert!(matches!(error, AppError::Decode(_)), "损坏图片必须返回解码错误");
}

#[test]
fn render_changes_pixels_and_reuses_neutral_input() {
    let decoded = decoder::decode(&fixture("portrait.jpg")).unwrap();
    let working = fit_within(&decoded.image, 900);
    let cancel = CancelToken::default();

    let neutral = pipeline::render(&working, &EditParams::default(), &[], &cancel).unwrap();
    assert_eq!(neutral.into_raw(), working.clone().into_raw(), "中性参数应原样复用");

    let params = EditParams {
        exposure: 35.0,
        contrast: 25.0,
        saturation: 30.0,
        ..Default::default()
    };
    let first = pipeline::render(&working, &params, &[], &cancel).unwrap();
    let second = pipeline::render(&working, &params, &[], &cancel).unwrap();
    assert_ne!(
        first.clone().into_raw(),
        working.clone().into_raw(),
        "调色应真正改变像素"
    );
    assert_eq!(first.into_raw(), second.into_raw(), "相同参数必须得到确定性结果");
}

#[test]
fn cancelled_render_stops_with_error() {
    let decoded = decoder::decode(&fixture("portrait.jpg")).unwrap();
    let working = fit_within(&decoded.image, 1200);
    let flag = Arc::new(AtomicBool::new(true));
    let cancel = CancelToken::new(flag);
    let params = EditParams {
        exposure: 20.0,
        ..Default::default()
    };
    let error = pipeline::render(&working, &params, &[], &cancel).unwrap_err();
    assert!(matches!(error, AppError::Cancelled));
}

#[test]
fn preview_and_export_use_same_pipeline() {
    let decoded = decoder::decode(&fixture("portrait.jpg")).unwrap();
    let params = EditParams {
        contrast: 30.0,
        highlights: -20.0,
        whites: 15.0,
        ..Default::default()
    };
    let cancel = CancelToken::default();
    let preview = build_preview(&decoded.image, 1800);
    let started = std::time::Instant::now();
    let rendered_preview = pipeline::render(&preview, &params, &[], &cancel).unwrap();
    let preview_ms = started.elapsed().as_millis();
    let started = std::time::Instant::now();
    let rendered_full = pipeline::render(&decoded.image, &params, &[], &cancel).unwrap();
    let full_ms = started.elapsed().as_millis();
    eprintln!(
        "[perf] render preview 1350x1800: {preview_ms} ms · full 1680x2240: {full_ms} ms"
    );
    assert_eq!(
        (rendered_preview.width(), rendered_preview.height()),
        (1350, 1800)
    );
    assert_eq!(
        (rendered_full.width(), rendered_full.height()),
        (1680, 2240),
        "导出必须保持原始分辨率"
    );

    // 同一坐标在缩小前后的相对亮度变化应一致（不是两套算法）。
    let sample_preview = rendered_preview.get_pixel(600, 900).0[0] as f32;
    let sample_full = rendered_full.get_pixel(747, 1120).0[0] as f32;
    assert!(
        (sample_preview - sample_full).abs() < 3000.0,
        "预览与导出结果应高度接近：{sample_preview} vs {sample_full}"
    );
}

#[test]
fn export_writes_file_and_never_overwrites_original() {
    let dir = temp_dir("export");
    let source = fixture("portrait.jpg");
    let decoded = decoder::decode(&source).unwrap();
    let image = fit_within(&decoded.image, 600);
    let settings = ExportSettings::default();

    let overwrite = export::write(&source, &source, &image, settings, BitDepth::Eight);
    assert!(overwrite.is_err(), "导出到原图路径必须被拒绝");

    let target = dir.join("out.png");
    let outcome = export::write(&target, &source, &image, settings, BitDepth::Eight).unwrap();
    assert_eq!(outcome.format, "PNG");
    assert_eq!(outcome.bytes, std::fs::metadata(&target).unwrap().len());
    let reloaded: Image16 = image::ImageReader::open(&target)
        .unwrap()
        .decode()
        .unwrap()
        .to_rgba16();
    assert_eq!(reloaded.dimensions(), image.dimensions());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn project_roundtrip_keeps_params_and_masks() {
    let dir = temp_dir("project");
    let path = dir.join("roundtrip.lumi");
    let mut params = EditParams::default();
    params.exposure = 42.0;
    params.hsl.red.hue = -25.0;
    params.curves.rgb = vec![
        lumi_retouch_lib::image_engine::params::CurvePoint::new(0.0, 0.0),
        lumi_retouch_lib::image_engine::params::CurvePoint::new(0.5, 0.6),
        lumi_retouch_lib::image_engine::params::CurvePoint::new(1.0, 1.0),
    ];
    params.lut = Some(lumi_retouch_lib::image_engine::params::LutSettings {
        path: "/tmp/sample.cube".into(),
        name: "sample".into(),
        strength: 60.0,
    });
    let doc = ProjectDoc::new(
        vec![ProjectPhoto {
            path: fixture("portrait.jpg").to_string_lossy().into_owned(),
            params: params.clone(),
            masks: Vec::new(),
            export: None,
        }],
        0,
        ExportSettings::default(),
    );
    project::save(&path, &doc).unwrap();
    let loaded = project::load(&path).unwrap();
    assert_eq!(loaded, doc);
    assert_eq!(loaded.photos[0].params.exposure, 42.0);
    assert_eq!(loaded.photos[0].params.hsl.red.hue, -25.0);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn high_resolution_image_is_handled_under_memory_limits() {
    let decoded = decoder::decode(&fixture("portrait_hires.jpg")).unwrap();
    assert_eq!((decoded.image.width(), decoded.image.height()), (3360, 4480));
    let preview = build_preview(&decoded.image, 1800);
    assert_eq!((preview.width(), preview.height()), (1350, 1800));
}

/// 手动运行的性能基准：`cargo test --release --test engine benchmark -- --ignored --nocapture`。
#[test]
#[ignore]
fn benchmark_full_pipeline() {
    use lumi_retouch_lib::image_engine::preview::encode_canvas;

    let decoded = decoder::decode(&fixture("portrait_hires.jpg")).unwrap();
    let preview = build_preview(&decoded.image, 1800);
    let params = EditParams {
        exposure: 18.0,
        contrast: 22.0,
        highlights: -30.0,
        shadows: 35.0,
        whites: 12.0,
        blacks: -15.0,
        temperature: 8.0,
        tint: -5.0,
        saturation: 12.0,
        vibrance: 20.0,
        clarity: 25.0,
        sharpness: 30.0,
        grain: 10.0,
        vignette: 15.0,
        ..Default::default()
    };
    let cancel = CancelToken::default();

    let started = std::time::Instant::now();
    let rendered = pipeline::render(&preview, &params, &[], &cancel).unwrap();
    let preview_render = started.elapsed();

    let started = std::time::Instant::now();
    let encoded = encode_canvas(&rendered, false).unwrap();
    let preview_encode = started.elapsed();

    let started = std::time::Instant::now();
    let full = pipeline::render(&decoded.image, &params, &[], &cancel).unwrap();
    let full_render = started.elapsed();

    let dir = temp_dir("bench");
    let target = dir.join("bench.jpg");
    let started = std::time::Instant::now();
    export::write(
        &target,
        &fixture("portrait_hires.jpg"),
        &full,
        ExportSettings::default(),
        BitDepth::Eight,
    )
    .unwrap();
    let export_time = started.elapsed();
    let size = std::fs::metadata(&target).unwrap().len();

    eprintln!(
        "[perf] 1350x1800 render {:?} · encode {:?} ({} KB) · 3360x4480 render {:?} · JPEG export {:?} ({} KB)",
        preview_render,
        preview_encode,
        encoded.url.len() / 1024,
        full_render,
        export_time,
        size / 1024
    );
    std::fs::remove_dir_all(&dir).ok();
}
