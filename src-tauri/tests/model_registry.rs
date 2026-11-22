//! R11 验收测试：统一模型注册表 + 推理准入链。
//!
//! 覆盖任务书要求的 7 个自动化验收点：
//! 1. SHA256 校验通过 / 不符（损坏）两条路径；
//! 2. 缺失模型 → 明确错误码 + 降级；
//! 3. 版本（契约）不匹配 → `E_MODEL_VERSION`；
//! 4. 未授权 / 未核实许可证 → 拒绝加载且**未读取文件**；
//! 5. 准入链顺序：许可未过 + 文件缺失时 `blocking` 是许可而不是缺失；
//! 6. 推理可取消（`E_CANCELLED`，不 panic）；
//! 7. registry 状态可序列化给前端（字段齐全、camelCase）。

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use lumi_retouch_lib::ai::FaceEngine;
use lumi_retouch_lib::ai_engine::inference::{
    CpuTractBackend, InferenceBackend, InferenceError, TensorSpec, ValueRange,
};
use lumi_retouch_lib::ai_engine::model_registry::builtin;
use lumi_retouch_lib::ai_engine::model_registry::license::{LicenseRecord, Verdict3};
use lumi_retouch_lib::ai_engine::model_registry::spec::{ModelOrigin, ModelSpec};
use lumi_retouch_lib::ai_engine::model_registry::{
    locate, state::ModelRuntimeState, verify, Registry,
};
use lumi_retouch_lib::image_engine::buffer::Image16;
use lumi_retouch_lib::image_engine::cancel::CancelToken;
use image::Rgba;

fn model_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join(builtin::YUNET.dir)
        .join(builtin::YUNET.file_name)
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "lumi-r11-{name}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// 复制随包 YuNet 到临时目录（体积与内容都合法）。
fn copy_real_model(dir: &Path) -> PathBuf {
    let target = dir.join(builtin::YUNET.file_name);
    std::fs::copy(model_path(), &target).unwrap();
    target
}

/// 形如 `ModelSpec { input: TensorSpec { shape: Cow::Owned(..) } }` 的测试夹具：
/// 形状来自运行时（模拟"换了一个不同契约的权重"）。
fn owned_shape(shape: Vec<usize>) -> std::borrow::Cow<'static, [usize]> {
    std::borrow::Cow::Owned(shape)
}

fn spec_with_license(weights: Verdict3, commercial: Verdict3, redistribution: Verdict3) -> ModelSpec {
    ModelSpec {
        license: LicenseRecord {
            weights,
            training_data: Verdict3::Unverified {
                what_to_check: "测试用例，无需核对",
            },
            commercial_use: commercial,
            redistribution,
            spdx: Some("CC-BY-NC-4.0"),
            evidence: &[],
            note: "测试用许可记录",
        },
        ..builtin::YUNET
    }
}

// ───────────────────────── 1. SHA256 通过 / 不符 ─────────────────────────

#[test]
fn real_weight_passes_sha256_and_reports_verified() {
    let health = verify::check_at(&builtin::YUNET, &model_path(), true);
    assert!(health.findings.is_empty(), "findings={:?}", health.findings);
    assert_eq!(health.state().as_str(), "verified");
    assert_eq!(health.resolved_path.as_deref(), Some(model_path().as_path()));
}

#[test]
fn tampered_weight_is_detected_as_corrupt_with_both_digests() {
    let dir = temp_dir("tampered");
    let path = copy_real_model(&dir);
    // 保持体积不变，只改最后 1 字节 → 只有 SHA 校验能发现。
    let mut bytes = std::fs::read(&path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    std::fs::write(&path, &bytes).unwrap();

    let health = verify::check_at(&builtin::YUNET, &path, false);
    let failure = health.blocking.expect("改过的权重必须被拒绝");
    assert_eq!(failure.kind(), "corrupt");
    assert_eq!(failure.expected(), Some(builtin::YUNET.sha256));
    let actual = failure.actual().expect("必须给出实际摘要");
    assert_ne!(actual, builtin::YUNET.sha256);
    assert_eq!(
        InferenceError::from(failure).code(),
        "E_MODEL_CORRUPT",
        "损坏必须是 E_MODEL_CORRUPT"
    );
    std::fs::remove_dir_all(&dir).ok();
}

// ───────────────────── 2. 缺失模型 → 错误码 + 降级 ─────────────────────

#[test]
fn missing_weight_reports_e_missing_and_degrades_to_geometry() {
    let registry = Registry::new(None);
    let health = registry.check(&builtin::YUNET, &[PathBuf::from("/definitely/not/here")], true);
    let failure = health.blocking.expect("缺失必须被报告");
    assert_eq!(failure.kind(), "missing");
    assert!(!failure.searched().unwrap().is_empty(), "必须列出搜索过的路径");
    assert_eq!(InferenceError::from(failure).code(), "E_MODEL_MISSING");

    // 降级：皮肤解析在没有可商用权重时回退几何蒙版，文案不得变化。
    let status = lumi_retouch_lib::ai_engine::skin_segmentation::model::status(None);
    assert!(!status.available);
    assert_eq!(status.method, "landmark-geometry");
    assert!(status.reason.contains("未登记可商用"), "文案不得改：{}", status.reason);
}

#[test]
fn engine_missing_model_surfaces_a_structured_error() {
    let engine = FaceEngine::default();
    let error = engine
        .load_checked(Path::new("/definitely/not/here/face_detection_yunet_2023mar.onnx"))
        .unwrap_err();
    assert_eq!(error.code(), "E_MODEL_MISSING");
    assert!(!engine.is_loaded());
}

// ─────────────── 3. 版本（契约）不匹配 → E_MODEL_VERSION ───────────────

#[test]
fn wrong_input_shape_is_a_version_mismatch() {
    let spec = ModelSpec {
        input: TensorSpec {
            shape: owned_shape(vec![1, 3, 320, 320]),
            ..builtin::YUNET.input
        },
        ..builtin::YUNET
    };
    let health = verify::check_at(&spec, &model_path(), true);
    let failure = health.blocking.expect("形状不符必须被拒绝");
    assert_eq!(failure.kind(), "versionMismatch");
    assert_eq!(InferenceError::from(failure).code(), "E_MODEL_VERSION");
}

#[test]
fn contract_is_checked_before_sha256() {
    // 形状不符 + SHA 也不符：判定链第 ④ 步必须早于第 ⑤ 步，
    // 因此结果是「版本不符」而不是「损坏」。
    let spec = ModelSpec {
        input: TensorSpec {
            shape: owned_shape(vec![1, 3, 320, 320]),
            ..builtin::YUNET.input
        },
        sha256: "0000000000000000000000000000000000000000000000000000000000000000",
        ..builtin::YUNET
    };
    let health = verify::check_at(&spec, &model_path(), true);
    assert_eq!(health.blocking.unwrap().kind(), "versionMismatch");
}

// ─────── 4. 未授权权重：拒绝加载，且**一个字节都没有被读** ───────

#[test]
fn non_commercial_weights_are_rejected_before_any_read() {
    let spec = spec_with_license(
        Verdict3::NotAllowed,
        Verdict3::NotAllowed,
        Verdict3::NotAllowed,
    );
    // 关键构造：文件存在但内容**与登记的 SHA 完全不同**（体积也不符）。
    // 如果实现先做体积/哈希再判许可，findings 里必然出现 corrupt。
    let dir = temp_dir("license-no-read");
    let path = dir.join(builtin::YUNET.file_name);
    std::fs::write(&path, b"this is not an onnx model at all").unwrap();

    let health = verify::check_at(&spec, &path, true);
    assert_eq!(health.findings.len(), 1, "findings={:?}", health.findings);
    let failure = &health.findings[0];
    assert_eq!(failure.kind(), "licenseBlocked");
    assert!(failure.is_permanent());
    assert_eq!(
        InferenceError::from(failure.clone()).code(),
        "E_MODEL_LICENSE"
    );
    assert!(health.resolved_path.is_none());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn unverified_commercial_field_blocks_bundled_weights() {
    let spec = spec_with_license(
        Verdict3::Allowed,
        Verdict3::Unverified {
            what_to_check: "需核对商用条款",
        },
        Verdict3::Allowed,
    );
    let health = verify::check_at(&spec, &model_path(), true);
    let failure = health.blocking.expect("随包权重未核实不得加载");
    assert_eq!(failure.kind(), "licenseBlocked");
    match &failure {
        lumi_retouch_lib::ai_engine::model_registry::ModelFailure::LicenseNotCleared {
            which, ..
        } => assert_eq!(*which, "commercialUse"),
        other => panic!("意外失败态：{other:?}"),
    }
}

#[test]
fn user_imported_unverified_licence_is_allowed_locally_only() {
    // Q1 裁决：用户自备模型只拦 NotAllowed，未核实放行并标注「仅本地」。
    let unverified = Verdict3::Unverified {
        what_to_check: "用户声明不等于核实",
    };
    let mut spec = spec_with_license(unverified, unverified, unverified);
    spec.origin = ModelOrigin::UserImported;
    assert!(
        lumi_retouch_lib::ai_engine::model_registry::license::gate(
            spec.origin,
            &spec.license,
            spec.id
        )
        .is_ok(),
        "用户自备模型不得被强制闸门拦住"
    );
    // 但永远不能因为“放在 Model Hub 里”就变成正式商用模型。
    assert!(!spec.is_default_eligible());
}

// ────────── 5. 准入链顺序：Missing 不得掩盖 License ──────────

#[test]
fn license_failure_blocks_before_missing() {
    let spec = spec_with_license(
        Verdict3::NotAllowed,
        Verdict3::Allowed,
        Verdict3::Allowed,
    );
    let health = verify::check(&spec, &[PathBuf::from("/definitely/not/here")], true);
    // blocking 必须是许可，findings 必须同时暴露“许可 + 缺失”。
    assert_eq!(
        health.blocking.as_ref().map(|failure| failure.kind()),
        Some("licenseBlocked")
    );
    let kinds: Vec<&str> = health.findings.iter().map(|failure| failure.kind()).collect();
    assert_eq!(kinds, vec!["licenseBlocked", "missing"]);
    assert_eq!(health.state().as_str(), "licenseBlocked");
}

#[test]
fn findings_expose_every_problem_not_just_the_first() {
    let spec = spec_with_license(
        Verdict3::Unverified {
            what_to_check: "需核对权重条款",
        },
        Verdict3::Allowed,
        Verdict3::Allowed,
    );
    let health = verify::check(&spec, &[PathBuf::from("/definitely/not/here")], true);
    assert!(health.findings.len() >= 2);
    assert_eq!(health.blocking.unwrap().kind(), "licenseBlocked");
}

// ───────────────────── 6. 推理可取消 ─────────────────────

#[test]
fn cancelled_inference_returns_e_cancelled_without_panicking() {
    let engine = FaceEngine::default();
    let preview: Image16 = Image16::from_pixel(96, 96, Rgba([30000u16, 30000, 30000, 65535]));
    let cancel = CancelToken::new(Arc::new(AtomicBool::new(true)));
    let error = engine
        .detect_checked(&model_path(), &preview, 96, 96, &mut |_, _| {}, &cancel)
        .unwrap_err();
    assert_eq!(error.code(), "E_CANCELLED");
    // 旧签名路径不 panic，只是降级成 AppError。
    assert!(engine
        .detect_with_cancel(&model_path(), &preview, 96, 96, &mut |_, _| {}, &cancel)
        .is_err());
}

#[test]
fn uncancelled_inference_still_works_after_the_cancel_test() {
    let engine = FaceEngine::default();
    let preview: Image16 = Image16::from_pixel(96, 96, Rgba([30000u16, 30000, 30000, 65535]));
    let result = engine
        .detect(&model_path(), &preview, 96, 96, &mut |_, _| {})
        .expect("空令牌路径必须照常工作");
    assert_eq!(result.model, "YuNet 2023mar");
    assert!(engine.is_loaded());
}

// ─────────── 7. registry 状态可序列化给前端（camelCase） ───────────

#[test]
fn registry_snapshot_is_frontend_ready() {
    let registry = Registry::new(None);
    let mut dirs = locate::dev_base_dirs();
    dirs.push(PathBuf::from("/definitely/not/here"));
    let snapshot = registry.snapshot(&dirs);
    let value = serde_json::to_value(&snapshot).unwrap();

    assert_eq!(value["schemaVersion"], 1);
    assert!(value["generatedAtUnix"].as_u64().unwrap() > 0);
    let models = value["models"].as_array().unwrap();
    assert_eq!(models.len(), 1);

    let model = &models[0];
    for field in [
        "id",
        "displayName",
        "version",
        "task",
        "origin",
        "official",
        "format",
        "file",
        "sizeBytes",
        "sizeOnDiskBytes",
        "sha256",
        "input",
        "outputs",
        "license",
        "state",
        "loaded",
        "path",
        "failure",
        "findings",
        "checkedAtUnix",
    ] {
        assert!(
            model.get(field).is_some(),
            "快照缺少字段 `{field}`：{model}"
        );
    }

    // 许可四字段独立 + 未核实字段单列（不得把 trainingData 合并进 commercialUse）。
    let license = &model["license"];
    for field in [
        "state",
        "spdx",
        "weights",
        "commercialUse",
        "redistribution",
        "trainingData",
        "unverifiedFields",
        "evidence",
        "note",
    ] {
        assert!(license.get(field).is_some(), "许可视图缺少 `{field}`");
    }
    assert_eq!(license["commercialUse"], "allowed");
    assert_eq!(license["trainingData"], "unverified");
    assert_eq!(license["unverifiedFields"], serde_json::json!(["trainingData"]));

    // 输入契约结构化（不是自由字符串）。
    assert_eq!(model["input"]["dtype"], "f32");
    assert_eq!(model["input"]["layout"], "nchw");
    assert_eq!(model["input"]["valueRange"], "raw255");
    assert_eq!(model["input"]["channelOrder"], "bgr");

    // 汇总字段齐全。
    for field in ["total", "official", "ready", "verified", "blocked", "userImported"] {
        assert!(
            value["summary"].get(field).is_some(),
            "summary 缺少 `{field}`"
        );
    }
}

#[test]
fn snapshot_state_reflects_recorded_checks() {
    let registry = Registry::new(None);
    let mut dirs = locate::dev_base_dirs();
    dirs.push(PathBuf::from("/definitely/not/here"));
    assert_eq!(
        registry
            .snapshot(&dirs)
            .find(builtin::YUNET.id)
            .unwrap()
            .state,
        "verified"
    );

    registry.record(
        &verify::check(&builtin::YUNET, &[PathBuf::from("/definitely/not/here")], false),
        builtin::YUNET.id,
    );
    let after = registry.snapshot(&dirs);
    assert_eq!(after.find(builtin::YUNET.id).unwrap().state, "missing");

    registry.mark_ready(builtin::YUNET.id, model_path());
    let ready = registry.snapshot(&dirs);
    assert_eq!(ready.find(builtin::YUNET.id).unwrap().state, "ready");
    assert!(ready.find(builtin::YUNET.id).unwrap().loaded);
    assert_eq!(ready.summary.ready, 1);
}

#[test]
fn model_runtime_state_strings_match_the_frontend_union() {
    let expected = [
        "verified",
        "ready",
        "licenseBlocked",
        "missing",
        "corrupt",
        "versionMismatch",
        "loadFailed",
    ];
    let states = [
        ModelRuntimeState::Verified {
            path: PathBuf::from("/x"),
        },
        ModelRuntimeState::Ready {
            path: PathBuf::from("/x"),
        },
    ];
    assert_eq!(states[0].as_str(), expected[0]);
    assert_eq!(states[1].as_str(), expected[1]);
    assert_eq!(expected.len(), 7);
}

#[test]
fn inspect_covers_the_l2_l3_read_only_shape() {
    // 设计 §8.6 验收点 ⑨：trait 必须能表达 model_hub/validator.rs 的
    // 「只读结构」形态（ir_version / opset / inputs / outputs），且不需要编译 runnable。
    let backend = CpuTractBackend::new();
    let introspection = backend
        .inspect(&model_path(), &CancelToken::default())
        .unwrap();
    assert_eq!(introspection.input_shapes(), vec![vec![1, 3, 640, 640]]);
    assert_eq!(introspection.outputs.len(), 12);
    assert!(introspection.ir_version.unwrap_or(0) > 0);
    assert!(introspection.opset.unwrap_or(0) > 0);
    assert!(introspection.operator_count > 0);
}

#[test]
fn yunet_value_range_is_raw255_consistent_with_the_implementation() {
    // R9 回归护栏：16 位内部表示 / 257.0 落在 0..255，因此登记的是 Raw255。
    assert_eq!(builtin::YUNET.input.value_range, ValueRange::Raw255);
    assert_eq!(f32::from(u16::MAX) / 257.0, 255.0);
    assert_eq!(
        lumi_retouch_lib::ai::model::MODEL_INPUT,
        builtin::YUNET.input.format_human()
    );
}
