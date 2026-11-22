//! ROUND 10 阶段 B：真实照片导入 → 渲染 → 导出的核心管线验证。
//!
//! 需要设置 `LUMI_TEST_PHOTOS` 指向本地测试照片目录（见
//! docs/ROUND10_REAL_PHOTO_WORKFLOW.md 的来源与授权记录）。
//! 未设置时跳过，不影响常规 CI。

use std::path::{Path, PathBuf};

use lumi_retouch_lib::ai::model::MODEL_FILE;
use lumi_retouch_lib::ai::FaceEngine;
use lumi_retouch_lib::ai_engine::face_frame::FaceFrame;
use lumi_retouch_lib::color_management::{embed::extract_icc, ColorOutputSettings};
use lumi_retouch_lib::face_reshape;
use lumi_retouch_lib::image_engine::buffer;
use lumi_retouch_lib::image_engine::cancel::CancelToken;
use lumi_retouch_lib::image_engine::decoder::{self, BitDepth};
use lumi_retouch_lib::image_engine::export::{self, ExportFormat, ExportSettings};
use lumi_retouch_lib::image_engine::params::{EditParams, FaceReshape, FaceShape};
use lumi_retouch_lib::image_engine::pipeline;

fn load(path: &Path) -> decoder::DecodedImage {
    decoder::decode(path).unwrap_or_else(|error| {
        panic!(
            "{} 解码失败：{error}",
            path.file_name().unwrap_or_default().to_string_lossy()
        )
    })
}

fn corpus() -> Option<PathBuf> {
    std::env::var_os("LUMI_TEST_PHOTOS").map(PathBuf::from)
}

fn settings(format: ExportFormat) -> ExportSettings {
    ExportSettings {
        format,
        quality: 92,
        ..Default::default()
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lumi-round10-photos-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn real_photos_import_render_and_export() {
    let Some(dir) = corpus() else {
        println!("跳过：未设置 LUMI_TEST_PHOTOS");
        return;
    };
    let out = temp_dir("export");
    let names = [
        "portrait-a.jpg",
        "portrait-b.jpg",
        "portrait-group.jpg",
        "photo.png",
        "photo.tiff",
        "photo-p3.jpg",
        "photo-noicc.jpg",
        "large-24mp.jpg",
    ];
    let mut tested = 0;
    for name in names {
        let path = dir.join(name);
        if !path.exists() {
            eprintln!("缺少样张，跳过：{name}");
            continue;
        }
        let before = std::fs::read(&path).unwrap();
        let decoded = load(&path);
        let cached = &decoded.image;
        let (width, height) = cached.dimensions();
        assert!(width > 0 && height > 0, "{name} 尺寸为 0");

        let rendered = pipeline::render(cached, &EditParams::default(), &[], &CancelToken::default())
            .unwrap_or_else(|error| panic!("{name} 默认参数渲染失败：{error}"));
        assert_eq!(
            rendered.dimensions(),
            (width, height),
            "{name} 渲染改变了分辨率"
        );

        for format in [ExportFormat::Jpeg, ExportFormat::Png, ExportFormat::Tiff] {
            let destination = out.join(format!("{name}-out.{}", format.extension()));
            let outcome = export::write(
                &destination,
                &path,
                &rendered,
                settings(format),
                BitDepth::Eight,
                Some(&ColorOutputSettings::default()),
            )
            .unwrap_or_else(|error| panic!("{name} 导出 {format:?} 失败：{error}"));
            assert!(outcome.bytes > 0, "{name} 导出 {format:?} 为空文件");
            let exported = decoder::decode(&destination)
                .unwrap_or_else(|error| panic!("{name} 导出的 {format:?} 无法回读：{error}"));
            assert_eq!(
                exported.image.dimensions(),
                (width, height),
                "{name} 导出的 {format:?} 尺寸不一致"
            );
            if format == ExportFormat::Png {
                // PNG 无损：导出与管线渲染在同一像素上应一致（8-bit 往返容差）。
                let expected = rendered.get_pixel(width / 2, height / 2).0;
                let actual = exported.image.get_pixel(width / 2, height / 2).0;
                for channel in 0..3 {
                    let diff = (expected[channel] as i32 - actual[channel] as i32).abs();
                    assert!(
                        diff <= 400,
                        "{name} 预览/导出像素偏差过大：通道 {channel} 差 {diff}"
                    );
                }
            }
            if format == ExportFormat::Jpeg {
                let icc = extract_icc(&std::fs::read(&destination).unwrap()).unwrap();
                assert!(icc.is_some(), "{name} JPEG 导出未嵌入 ICC");
            }
        }

        // 不修改原图。
        assert_eq!(
            std::fs::read(&path).unwrap(),
            before,
            "{name} 源文件在导入/渲染/导出过程中被修改"
        );
        tested += 1;
    }
    assert!(tested >= 4, "实际测试的样张过少：{tested}");
}

#[test]
fn orientation_is_applied_on_import() {
    let Some(dir) = corpus() else {
        println!("跳过：未设置 LUMI_TEST_PHOTOS");
        return;
    };
    let portrait = dir.join("portrait-a.jpg");
    let oriented = dir.join("photo-orient6.jpg");
    if !portrait.exists() || !oriented.exists() {
        return;
    }
    let original = load(&portrait);
    let rotated = load(&oriented);
    assert!(
        original.image.height() > original.image.width(),
        "原始样张应为竖幅"
    );
    // 存储的是竖幅 + Orientation=6，显示时应交换宽高。
    assert_eq!(
        rotated.image.dimensions(),
        (original.image.height(), original.image.width()),
        "EXIF 方向未被正确应用"
    );
}

#[test]
fn corrupt_and_raw_files_fail_explicitly() {
    let Some(dir) = corpus() else {
        println!("跳过：未设置 LUMI_TEST_PHOTOS");
        return;
    };
    let corrupt = dir.join("corrupt.jpg");
    if corrupt.exists() {
        let error = decoder::decode(&corrupt).unwrap_err();
        assert!(
            error.to_string().contains("解码") || error.to_string().contains("损坏"),
            "损坏文件错误不明确：{error}"
        );
    }
    let raw = dir.join("sample-canon.cr2");
    if raw.exists() {
        let error = decoder::decode(&raw).unwrap_err();
        assert!(
            error.to_string().contains("未开放 RAW"),
            "RAW 应明确提示未开放，而不是假装成功：{error}"
        );
    }
    let fake = dir.join("fake-unsupported.cr3");
    if fake.exists() {
        let error = decoder::decode(&fake).unwrap_err();
        assert!(error.to_string().contains("未开放 RAW"));
    }
}

#[test]
fn dng_and_tiff_extensions_are_classified_honestly() {
    assert!(decoder::is_supported_extension(Path::new("/tmp/a.tif")));
    assert!(decoder::is_supported_extension(Path::new("/tmp/a.TIFF")));
    assert!(!decoder::is_supported_extension(Path::new("/tmp/a.cr2")));
    assert_eq!(decoder::format_label(Path::new("/tmp/a.TIFF")), "TIFF");
}

/// 阶段 B4：真实人像在中等强度轮廓塑形下必须有可测量的像素变化。
/// 允许安全策略降强度，但不允许“滑杆能拖动、像素完全不变”且只给 0% 的静默失效。
#[test]
fn real_portrait_reshape_at_moderate_strength_changes_pixels() {
    let Some(dir) = corpus() else {
        println!("跳过：未设置 LUMI_TEST_PHOTOS");
        return;
    };
    let path = dir.join("portrait-a.jpg");
    if !path.exists() {
        return;
    }
    let decoded = load(&path);
    let preview = buffer::fit_within(&decoded.image, 1800);
    let model = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("resources/models/yunet")
        .join(MODEL_FILE);
    let engine = FaceEngine::default();
    let result = engine
        .detect(
            &model,
            &preview,
            decoded.image.width(),
            decoded.image.height(),
            &mut |_, _| {},
        )
        .expect("人脸检测失败");
    assert!(!result.faces.is_empty(), "真实人像应检测到人脸");
    let faces: Vec<FaceFrame> = result
        .faces
        .iter()
        .map(|face| FaceFrame::from_detected(face, result.image_width, result.image_height))
        .collect();

    let mut image = decoded.image.clone();
    let params = vec![FaceReshape {
        face: FaceShape {
            slim: 50.0,
            ..Default::default()
        },
        ..FaceReshape::new(faces[0].id)
    }];
    let reports = face_reshape::apply_face_reshape(
        &mut image,
        &faces,
        &params,
        &CancelToken::default(),
    )
    .unwrap();
    eprintln!("[round10] 真实人像塑形报告：{reports:?}");
    assert_eq!(reports.len(), 1, "应处理 1 张人脸");
    assert!(
        reports[0].applied_scale > 0.0,
        "中等强度塑形被完全回退：{:?}",
        reports[0].notes
    );
    assert!(
        reports[0].warped_pixels > 0,
        "塑形未改变任何像素：{:?}",
        reports[0].notes
    );
}
