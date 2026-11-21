//! ROUND 9：用户 ONNX 正常推理链路与 Worker 安全边界。
//!
//! 这些测试通过 `infer_isolated` 启动真实子进程（应用自身的 `--face-model-infer` 模式），
//! 验证的是 Worker 协议，而不是人脸模型精度。夹具模型标记 ProtocolTestOnly。

use std::path::{Path, PathBuf};
use std::time::Duration;

use lumi_retouch_lib::face_intelligence::model_hub::probe::{
    infer_isolated, probe_isolated, InferRequest,
};

const MODEL: &[u8] = include_bytes!("fixtures/onnx/protocol68.onnx");
const MODEL_NAN: &[u8] = include_bytes!("fixtures/onnx/protocol68_nan.onnx");
const MODEL_RANGE: &[u8] = include_bytes!("fixtures/onnx/protocol68_out_of_range.onnx");
const MODEL_BAD_OP: &[u8] = include_bytes!("fixtures/onnx/protocol68_bad_op.onnx");

fn worker() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_lumi-retouch"))
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lumi-onnx-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn manifest(model_file: &str) -> String {
    format!(
        r#"{{
  "schemaVersion": 1,
  "id": "lumi-protocol68",
  "name": "LUMI Protocol 68 (ProtocolTestOnly)",
  "version": "1.0.0",
  "author": "LUMI",
  "description": "协议验证模型，不是人脸关键点模型",
  "task": "faceLandmark",
  "topologyId": "face-68",
  "input": {{ "width": 64, "height": 64, "layout": "nchw", "normalization": "zeroToOne", "cropMargin": 0.1 }},
  "output": {{ "layout": "n3", "coordinate": "normalizedCrop" }},
  "license": {{ "name": "CC0-1.0", "url": "https://creativecommons.org/publicdomain/zero/1.0/", "commercialUseClaimed": true, "redistributionClaimed": true, "note": "本项目生成的协议夹具" }},
  "sha256": "",
  "fileName": "{model_file}",
  "sourceUrl": null,
  "sourcePath": "",
  "importedAt": "2026-11-21T00:00:00Z"
}}"#
    )
}

fn install(name: &str, bytes: &[u8]) -> PathBuf {
    let dir = scratch(name);
    std::fs::write(dir.join("model.onnx"), bytes).unwrap();
    std::fs::write(dir.join("manifest.json"), manifest("model.onnx")).unwrap();
    dir
}

fn tensor(dir: &Path, name: &str, fill: f32) -> String {
    let path = dir.join(name);
    let data = vec![fill; 3 * 64 * 64];
    let mut bytes = Vec::with_capacity(data.len() * 4);
    for value in data {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    std::fs::write(&path, bytes).unwrap();
    path.to_string_lossy().into_owned()
}

fn request(model_dir: &Path, input: &str) -> InferRequest {
    InferRequest {
        model_path: model_dir.join("model.onnx").to_string_lossy().into_owned(),
        input_path: input.to_string(),
        width: 64,
        height: 64,
        expected_points: 68,
        timeout_ms: 10_000,
    }
}

#[test]
fn probe_chain_validates_structure_and_smoke_in_worker() {
    let model_dir = install("probe", MODEL);
    let report = probe_isolated(&worker(), &model_dir.join("model.onnx"), Duration::from_secs(10))
        .expect("探针必须返回报告");
    assert!(report.probe.ok, "{:?}", report.probe.errors);
    assert_eq!(report.probe.input_width, Some(64));
    assert_eq!(report.probe.input_height, Some(64));
    assert!(report.smoke.ok, "{:?}", report.smoke.error);
    assert_eq!(report.smoke.output_elements, 68 * 3);
    println!(
        "probe: 输入 {}x{}，输出 {} 元素，有限值比例 {}，冒烟 {} ms",
        report.probe.input_width.unwrap_or(0),
        report.probe.input_height.unwrap_or(0),
        report.smoke.output_elements,
        report.smoke.finite_ratio,
        report.smoke.elapsed_ms
    );
    std::fs::remove_dir_all(&model_dir).ok();
}

#[test]
fn protocol_model_runs_real_inference_in_worker() {
    let dir = scratch("ok");
    let model_dir = install("ok", MODEL);
    let input = tensor(&dir, "input.bin", 0.25);
    let response = infer_isolated(&worker(), &request(&model_dir, &input), Duration::from_secs(10))
        .expect("Worker 必须返回结果");
    assert!(response.ok, "{:?}", response.error);
    assert_eq!(response.points.len(), 68);
    for point in &response.points {
        let [x, y, confidence] = *point;
        assert!(x.is_finite() && y.is_finite());
        assert!((0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y));
        assert!((confidence - 0.9).abs() < 1e-5);
    }
    println!(
        "worker 推理成功：{} 点，耗时 {} ms",
        response.points.len(),
        response.elapsed_ms
    );
    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(&model_dir).ok();
}

#[test]
fn worker_rejects_wrong_input_length_and_nan_input() {
    let dir = scratch("input");
    let model_dir = install("input", MODEL);
    let short = dir.join("short.bin");
    std::fs::write(&short, vec![0u8; 12]).unwrap();
    let response = infer_isolated(
        &worker(),
        &request(&model_dir, &short.to_string_lossy()),
        Duration::from_secs(10),
    )
    .unwrap();
    assert!(!response.ok);
    assert!(response.error.unwrap().contains("不符"));

    let nan = dir.join("nan.bin");
    let mut bytes = Vec::new();
    for index in 0..(3 * 64 * 64) {
        let value = if index == 7 { f32::NAN } else { 0.1 };
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    std::fs::write(&nan, bytes).unwrap();
    let response = infer_isolated(
        &worker(),
        &request(&model_dir, &nan.to_string_lossy()),
        Duration::from_secs(10),
    )
    .unwrap();
    assert!(!response.ok);
    assert!(response.error.unwrap().contains("NaN"));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(&model_dir).ok();
}

#[test]
fn worker_reports_corrupt_and_unsupported_models_without_crashing() {
    let dir = scratch("bad");
    let input = tensor(&dir, "input.bin", 0.2);

    let corrupt = scratch("corrupt");
    std::fs::write(corrupt.join("model.onnx"), b"not-an-onnx-model").unwrap();
    std::fs::write(corrupt.join("manifest.json"), manifest("model.onnx")).unwrap();
    let response = infer_isolated(&worker(), &request(&corrupt, &input), Duration::from_secs(10))
        .unwrap();
    assert!(!response.ok);

    let bad_op = install("bad-op", MODEL_BAD_OP);
    let response =
        infer_isolated(&worker(), &request(&bad_op, &input), Duration::from_secs(10)).unwrap();
    assert!(!response.ok, "未知算子必须被拒绝");

    println!(
        "损坏模型错误：{:?} / 不支持算子错误：{:?}",
        response.error, response.error
    );
    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(&corrupt).ok();
    std::fs::remove_dir_all(&bad_op).ok();
}

#[test]
fn worker_rejects_nan_and_out_of_range_outputs() {
    let dir = scratch("output");
    let input = tensor(&dir, "input.bin", 0.3);

    let nan = install("nan-model", MODEL_NAN);
    let response = infer_isolated(&worker(), &request(&nan, &input), Duration::from_secs(10)).unwrap();
    assert!(!response.ok);
    assert!(response.error.as_deref().unwrap_or_default().contains("NaN"));

    let range = install("range-model", MODEL_RANGE);
    let response =
        infer_isolated(&worker(), &request(&range, &input), Duration::from_secs(10)).unwrap();
    assert!(!response.ok);
    assert!(response.error.as_deref().unwrap_or_default().contains("越界"));

    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(&nan).ok();
    std::fs::remove_dir_all(&range).ok();
}

#[test]
fn worker_timeout_and_start_failure_are_contained() {
    let dir = scratch("timeout");
    let model_dir = install("timeout", MODEL);
    let input = tensor(&dir, "input.bin", 0.2);

    let error = infer_isolated(
        &worker(),
        &request(&model_dir, &input),
        Duration::from_nanos(1),
    )
    .unwrap_err();
    assert!(error.contains("超时"), "{error}");

    let error = infer_isolated(
        Path::new("/definitely/missing/lumi-worker"),
        &request(&model_dir, &input),
        Duration::from_secs(2),
    )
    .unwrap_err();
    assert!(error.contains("无法启动"), "{error}");

    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(&model_dir).ok();
}

#[test]
fn consecutive_inference_model_switch_and_multi_face_inputs() {
    let dir = scratch("repeat");
    let good = install("repeat", MODEL);
    let nan = install("repeat-nan", MODEL_NAN);
    let first = tensor(&dir, "face-a.bin", 0.15);
    let second = tensor(&dir, "face-b.bin", 0.85);

    for input in [&first, &second] {
        let response =
            infer_isolated(&worker(), &request(&good, input), Duration::from_secs(10)).unwrap();
        assert!(response.ok, "{:?}", response.error);
    }

    let response =
        infer_isolated(&worker(), &request(&nan, &first), Duration::from_secs(10)).unwrap();
    assert!(!response.ok, "切换后的异常模型必须被拒绝");

    let response =
        infer_isolated(&worker(), &request(&good, &second), Duration::from_secs(10)).unwrap();
    assert!(response.ok, "切回正常模型后必须恢复推理");

    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(&good).ok();
    std::fs::remove_dir_all(&nan).ok();
}
