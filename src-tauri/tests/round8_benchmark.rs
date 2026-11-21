//! 第八轮：可重复的质量基准。
//!
//! 这是测量程序，不是验收开关：只有显式运行 `--ignored --nocapture` 才会执行，
//! 输出真实的关键点稳定性、置信度分布、耗时与进程 RSS。
//!
//! 说明：
//! - 所有输入都来自仓库内真实样张；
//! - 分割边界准确性无法测量，因为当前没有任何 Provider 提供 face parsing
//!   （见输出中的 `face_parsing` 证据等级），文档如实标注“不支持”；
//! - 内存为当前进程 RSS（`ps -o rss=`），只反映本机本进程，不代表全部设备。

use std::path::{Path, PathBuf};

use lumi_retouch_lib::ai::model::MODEL_FILE;
use lumi_retouch_lib::face_intelligence::FaceIntelligence;
use lumi_retouch_lib::image_engine::buffer::{fit_within, Image16};
use lumi_retouch_lib::image_engine::cancel::CancelToken;
use lumi_retouch_lib::image_engine::decoder;

const FIXTURES: &[&str] = &[
    "portrait.jpg",
    "group.jpg",
    "portrait_bright.jpg",
    "portrait_dark.jpg",
    "portrait_hires.jpg",
];

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/images")
        .join(name)
}

fn load_preview(name: &str) -> Image16 {
    let decoded = decoder::decode(&fixture(name)).unwrap();
    fit_within(&decoded.image, 1400)
}

fn rss_kb() -> Option<u64> {
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u64>()
        .ok()
}

fn pct(values: &[f32], percent: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let index = ((sorted.len() - 1) as f32 * percent).round() as usize;
    sorted[index]
}

#[test]
#[ignore = "质量基准，需要显式 --ignored --nocapture 运行"]
fn round8_quality_benchmark() {
    let yunet = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("resources/models/yunet")
        .join(MODEL_FILE);
    let root = std::env::temp_dir().join(format!("lumi-round8-bench-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);

    let rss_start = rss_kb();
    let intelligence = FaceIntelligence::new(Some(yunet), root.clone(), None);
    let rss_loaded = rss_kb();

    println!("\n=== ROUND8 BENCHMARK ===");
    println!(
        "env: macos {} | pid {} ",
        std::env::consts::ARCH,
        std::process::id()
    );
    println!(
        "rss start: {:?} MB | after providers init: {:?} MB",
        rss_start.map(|kb| kb as f64 / 1024.0),
        rss_loaded.map(|kb| kb as f64 / 1024.0)
    );

    for descriptor in intelligence.descriptors() {
        println!(
            "\n--- provider {} ({}) state={:?} license={} available={}",
            descriptor.id,
            descriptor.display_name,
            descriptor.state,
            descriptor.license.state.label(),
            descriptor.available
        );
        println!(
            "capabilities: sparse={} dense={} eye={} eyelid={} nose={} lip={} mesh={} parsing={}",
            descriptor.capabilities.sparse_landmarks,
            descriptor.capabilities.dense_landmarks,
            descriptor.capabilities.eye_landmarks,
            descriptor.capabilities.eyelid_landmarks,
            descriptor.capabilities.nose_landmarks,
            descriptor.capabilities.lip_landmarks,
            descriptor.capabilities.face_mesh,
            descriptor.capabilities.face_parsing,
        );
        for evidence in &descriptor.evidence {
            println!(
                "  evidence {:<20} {:<10} {}{}",
                evidence.capability,
                evidence.level.label(),
                evidence.note,
                evidence
                    .measurement
                    .as_ref()
                    .map(|value| format!(" | {value}"))
                    .unwrap_or_default()
            );
        }
        if !descriptor.available || !descriptor.license.state.is_usable_locally() {
            println!("  skipped: unavailable or license not usable");
            continue;
        }
        if intelligence.select(&descriptor.id).is_err() {
            println!("  skipped: select failed");
            continue;
        }

        let mut confidences: Vec<f32> = Vec::new();
        let mut timings: Vec<u64> = Vec::new();
        let mut cold_ms: Option<u64> = None;
        for &name in FIXTURES {
            let image = load_preview(name);
            let (width, height) = (image.width(), image.height());
            let cancel = CancelToken::default();
            // 首次调用含模型/框架初始化，单独记录，避免混入后续推理耗时。
            let cold_started = std::time::Instant::now();
            let warmup = intelligence.analyze(name, &image, &cancel);
            if cold_ms.is_none() {
                cold_ms = Some(cold_started.elapsed().as_millis() as u64);
            }
            if let Err(error) = &warmup {
                println!("  {name:<20} {width}x{height} analyze failed: {error:?}");
                continue;
            }
            let mut runs = Vec::new();
            for _ in 0..3 {
                let started = std::time::Instant::now();
                let (_, record) = intelligence.analyze(name, &image, &cancel).unwrap();
                runs.push(started.elapsed().as_millis() as u64);
                for face in &record.faces {
                    confidences.push(face.confidence);
                }
            }
            let (_, record) = intelligence.analyze(name, &image, &cancel).unwrap();
            timings.extend(runs.iter().copied());
            let gates: Vec<String> = record
                .faces
                .iter()
                .map(|face| {
                    format!(
                        "points={} eyes={} nose={} mouth={} chin={}",
                        face.dense_points,
                        face.gate.eyes.enabled,
                        face.gate.nose.enabled,
                        face.gate.mouth.enabled,
                        face.gate.chin.enabled
                    )
                })
                .collect();
            println!(
                "  {name:<20} {width}x{height} faces={} ms={:?} total_landmarks={} {}",
                record.faces.len(),
                runs,
                record.total_landmarks,
                gates.join("; ")
            );

            if name == "portrait.jpg" {
                match intelligence.probe_quality(&image, &cancel, 3) {
                    Ok(report) => println!(
                        "  probe(portrait): verified={} runs={} max_deviation={:.6} ratio={:.6} elapsed_ms={:?} faces={:?}",
                        report.verified,
                        report.stability.runs,
                        report.stability.max_deviation,
                        report.stability.ratio,
                        report.stability.elapsed_ms,
                        report.stability.faces_per_run
                    ),
                    Err(error) => println!("  probe(portrait) failed: {error:?}"),
                }
            }
        }

        let sorted = {
            let mut values = confidences.clone();
            values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            values
        };
        println!(
            "  confidence: samples={} min={:.3} p50={:.3} p90={:.3} max={:.3} below0.45={}",
            sorted.len(),
            sorted.first().copied().unwrap_or(0.0),
            pct(&sorted, 0.5),
            pct(&sorted, 0.9),
            sorted.last().copied().unwrap_or(0.0),
            sorted.iter().filter(|value| **value < 0.45).count()
        );
        if !timings.is_empty() {
            println!(
                "  latency ms: cold={:?} min={} p50={} max={} (n={})",
                cold_ms,
                timings.iter().min().unwrap(),
                {
                    let mut values = timings.clone();
                    values.sort_unstable();
                    values[values.len() / 2]
                },
                timings.iter().max().unwrap(),
                timings.len()
            );
        }
    }

    let rss_end = rss_kb();
    println!(
        "\nrss after benchmark: {:?} MB (delta vs start: {:?} MB)",
        rss_end.map(|kb| kb as f64 / 1024.0),
        match (rss_start, rss_end) {
            (Some(start), Some(end)) => Some((end as f64 - start as f64) / 1024.0),
            _ => None,
        }
    );
    let _ = std::fs::remove_dir_all(&root);
}
