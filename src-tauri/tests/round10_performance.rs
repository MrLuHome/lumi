//! ROUND 10 阶段 D：真实分辨率下的管线性能测量。
//!
//! 需要 `LUMI_TEST_PHOTOS` 指向测试照片目录；未设置时跳过。
//! 输出墙钟时间供 `docs/ROUND10_PERFORMANCE.md` 记录，并用宽松上限防止数量级回退。
//! 内存峰值请配合 `/usr/bin/time -l` 运行，见文档中的命令。

use std::path::{Path, PathBuf};
use std::time::Instant;

use lumi_retouch_lib::image_engine::buffer;
use lumi_retouch_lib::image_engine::cancel::CancelToken;
use lumi_retouch_lib::image_engine::changeset::{self, ChangeSet, Operation};
use lumi_retouch_lib::image_engine::decoder::{self, BitDepth};
use lumi_retouch_lib::image_engine::export::{self, ExportFormat, ExportSettings};
use lumi_retouch_lib::image_engine::params::EditParams;
use lumi_retouch_lib::image_engine::pipeline;
use lumi_retouch_lib::image_engine::preview::{build_preview, PREVIEW_MAX_EDGE};
use lumi_retouch_lib::image_engine::project::{self, ProjectDoc, ProjectPhoto};

fn corpus() -> Option<PathBuf> {
    std::env::var_os("LUMI_TEST_PHOTOS").map(PathBuf::from)
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lumi-round10-perf-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn tuned_params() -> EditParams {
    EditParams {
        exposure: 15.0,
        contrast: 12.0,
        saturation: 8.0,
        sharpness: 20.0,
        vignette: 10.0,
        ..Default::default()
    }
}

fn measure<T>(label: &str, work: impl FnOnce() -> T) -> (T, u128) {
    let started = Instant::now();
    let value = work();
    let elapsed = started.elapsed().as_millis();
    eprintln!("[perf] {label}: {elapsed} ms");
    (value, elapsed)
}

#[test]
fn performance_24mp() {
    let Some(dir) = corpus() else {
        println!("跳过：未设置 LUMI_TEST_PHOTOS");
        return;
    };
    let path = dir.join("large-24mp.jpg");
    if !path.exists() {
        eprintln!("缺少 large-24mp.jpg，跳过");
        return;
    }
    let (decoded, decode_ms) = measure("24MP 解码", || decoder::decode(&path).unwrap());
    let (preview, preview_ms) = measure("24MP 预览缩放", || build_preview(&decoded.image, PREVIEW_MAX_EDGE));
    let (thumb, thumb_ms) = measure("24MP 缩略图", || {
        buffer::fit_within(&decoded.image, 640)
    });
    let params = tuned_params();
    let (_, render_ms) = measure("24MP 参数渲染（preview）", || {
        pipeline::render(&preview, &params, &[], &CancelToken::default()).unwrap()
    });
    let (_, full_render_ms) = measure("24MP 参数渲染（full）", || {
        pipeline::render(&decoded.image, &params, &[], &CancelToken::default()).unwrap()
    });

    let out = temp_dir("24mp");
    let settings = |format| ExportSettings {
        format,
        quality: 92,
        ..Default::default()
    };
    let (_, jpeg_ms) = measure("24MP JPEG 导出", || {
        export::write(
            &out.join("out.jpg"),
            &path,
            &decoded.image,
            settings(ExportFormat::Jpeg),
            BitDepth::Eight,
            None,
        )
        .unwrap()
    });
    let (_, png_ms) = measure("24MP PNG 导出", || {
        export::write(
            &out.join("out.png"),
            &path,
            &decoded.image,
            settings(ExportFormat::Png),
            BitDepth::Eight,
            None,
        )
        .unwrap()
    });
    let (_, tiff_ms) = measure("24MP TIFF 导出", || {
        export::write(
            &out.join("out.tiff"),
            &path,
            &decoded.image,
            settings(ExportFormat::Tiff),
            BitDepth::Eight,
            None,
        )
        .unwrap()
    });

    let project_path = out.join("project.lumi");
    let mut doc = ProjectDoc::new(
        vec![ProjectPhoto {
            path: path.to_string_lossy().into_owned(),
            params: EditParams::default(),
            masks: Vec::new(),
            export: Some(ExportSettings::default()),
            source_hash: "perf-source".into(),
            faces: Vec::new(),
            subjects: Vec::new(),
            mask_meta: None,
            source: None,
            raw: None,
            color: None,
            extra: Default::default(),
        }],
        0,
        ExportSettings::default(),
    );
    doc.project_uuid = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into();
    project::save(&project_path, &doc).unwrap();
    let changeset = ChangeSet {
        project_id: doc.project_uuid.clone(),
        base_revision: doc.revision,
        request_id: "perf-save".into(),
        operations: vec![Operation::UpdateDocMeta {
            patch: serde_json::json!({ "activeIndex": 0 }),
        }],
        created_at: String::new(),
        rebase_on_conflict: false,
    };
    let (_, save_ms) = measure("24MP 单字段 ChangeSet 保存", || {
        changeset::apply(&project_path, &changeset).unwrap()
    });

    eprintln!(
        "[perf] 24MP 摘要 decode={decode_ms} preview={preview_ms} thumb={thumb_ms} \
         render_preview={render_ms} render_full={full_render_ms} jpeg={jpeg_ms} png={png_ms} \
         tiff={tiff_ms} save={save_ms} thumb_size={}x{}",
        thumb.width(),
        thumb.height()
    );
    assert!(decode_ms < 15_000, "24MP 解码过慢：{decode_ms} ms");
    assert!(render_ms < 10_000, "24MP 预览渲染过慢：{render_ms} ms");
    assert!(full_render_ms < 30_000, "24MP 全尺寸渲染过慢：{full_render_ms} ms");
    assert!(jpeg_ms < 15_000, "24MP JPEG 导出过慢：{jpeg_ms} ms");
    assert!(save_ms < 3_000, "24MP 保存过慢：{save_ms} ms");
}

#[test]
fn performance_45mp() {
    let Some(dir) = corpus() else {
        println!("跳过：未设置 LUMI_TEST_PHOTOS");
        return;
    };
    let path = dir.join("large-45mp.jpg");
    if !path.exists() {
        eprintln!("缺少 large-45mp.jpg，跳过");
        return;
    }
    let (decoded, decode_ms) = measure("45MP 解码", || decoder::decode(&path).unwrap());
    let (preview, preview_ms) = measure("45MP 预览缩放", || build_preview(&decoded.image, PREVIEW_MAX_EDGE));
    let params = tuned_params();
    let (_, render_ms) = measure("45MP 参数渲染（preview）", || {
        pipeline::render(&preview, &params, &[], &CancelToken::default()).unwrap()
    });
    let out = temp_dir("45mp");
    let (_, jpeg_ms) = measure("45MP JPEG 导出", || {
        export::write(
            &out.join("out.jpg"),
            &path,
            &decoded.image,
            ExportSettings {
                format: ExportFormat::Jpeg,
                quality: 92,
                ..Default::default()
            },
            BitDepth::Eight,
            None,
        )
        .unwrap()
    });
    eprintln!(
        "[perf] 45MP 摘要 decode={decode_ms} preview={preview_ms} render_preview={render_ms} jpeg={jpeg_ms}"
    );
    assert!(decode_ms < 30_000, "45MP 解码过慢：{decode_ms} ms");
    assert!(render_ms < 15_000, "45MP 预览渲染过慢：{render_ms} ms");
    assert!(jpeg_ms < 30_000, "45MP JPEG 导出过慢：{jpeg_ms} ms");
}

/// 50 张批量导出的引擎侧吞吐（不含 GUI 调度）。使用同一张 24MP 照片重复，
/// 反映的是编码与写盘成本，不是磁盘顺序读缓存差异。
#[test]
fn batch_export_50_jpegs() {
    let Some(dir) = corpus() else {
        println!("跳过：未设置 LUMI_TEST_PHOTOS");
        return;
    };
    let path = dir.join("large-24mp.jpg");
    if !path.exists() {
        return;
    }
    let decoded = decoder::decode(&path).unwrap();
    let out = temp_dir("batch50");
    let mut total_ms = 0u128;
    let mut slowest = 0u128;
    for index in 0..50 {
        let destination = out.join(format!("batch-{index:02}.jpg"));
        let started = Instant::now();
        export::write(
            &destination,
            &path,
            &decoded.image,
            ExportSettings {
                format: ExportFormat::Jpeg,
                quality: 92,
                ..Default::default()
            },
            BitDepth::Eight,
            None,
        )
        .unwrap();
        let elapsed = started.elapsed().as_millis();
        total_ms += elapsed;
        slowest = slowest.max(elapsed);
    }
    let files = std::fs::read_dir(&out)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "jpg"))
        .count();
    eprintln!("[perf] 50 张 JPEG：total={total_ms} ms slowest={slowest} ms files={files}");
    assert_eq!(files, 50);
    assert!(total_ms < 300_000, "50 张导出总耗时异常：{total_ms} ms");
}

/// 防止性能测试文件在未设置语料时被误判为空跑。
#[test]
fn performance_corpus_path_is_documented() {
    if let Some(dir) = corpus() {
        assert!(Path::new(&dir).is_dir(), "LUMI_TEST_PHOTOS 不是目录");
    }
}
