//! 第八轮：真实 Provider 推理与能力门控的集成测试。
//!
//! 全部使用仓库内真实样张，不允许任何模拟输出；Apple Vision 测试仅限 macOS。

use std::path::{Path, PathBuf};

use lumi_retouch_lib::ai::model::MODEL_FILE;
use lumi_retouch_lib::face_intelligence::FaceIntelligence;
use lumi_retouch_lib::image_engine::buffer::fit_within;
use lumi_retouch_lib::image_engine::cancel::CancelToken;
use lumi_retouch_lib::image_engine::decoder;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/images")
        .join(name)
}

fn yunet_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("resources/models/yunet")
        .join(MODEL_FILE)
}

fn models_root() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lumi-round8-fi-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn load_preview(name: &str) -> lumi_retouch_lib::image_engine::buffer::Image16 {
    let decoded = decoder::decode(&fixture(name)).unwrap();
    fit_within(&decoded.image, 1400)
}

/// 5 点 Provider 必须保持 5 点，且不能凭空构造稠密区域或解锁 LEVEL 3。
#[test]
fn yunet_provider_stays_sparse_and_locks_level3() {
    let intelligence = FaceIntelligence::new(Some(yunet_path()), models_root(), None);
    intelligence.select("yunet-5").unwrap();
    let image = load_preview("portrait.jpg");
    let (result, record) = intelligence
        .analyze("fixture/portrait.jpg", &image, &CancelToken::default())
        .unwrap();

    assert_eq!(result.provider_id, "yunet-5");
    assert!(!result.faces.is_empty(), "真实样张应检测到人脸");
    for face in &result.faces {
        assert_eq!(face.landmarks.len(), 5, "YuNet 只能输出 5 点");
        assert!(face.regions.is_empty(), "不得把 5 点包装成稠密区域");
    }
    for face in &record.faces {
        assert_eq!(face.dense_points, 0);
        assert!(face.dense_fingerprint.is_none());
        assert!(!face.gate.any_enabled(), "5 点不得解锁 LEVEL 3");
        assert!(!face.gate.summary.is_empty(), "锁定必须给出可见原因");
        // 分析已经执行过，原因必须是“能力不足”，不能误导用户重复分析。
        assert!(
            face.gate.eyes.reason.contains("稀疏关键点"),
            "原因应说明稀疏能力：{}",
            face.gate.eyes.reason
        );
        assert!(!face.gate.eyes.reason.contains("请先运行 AI 人像分析"));
    }
    assert!(record.gate_summary().contains("未解锁"));
}

/// Apple Vision 真实推理：区域点数、坐标范围与耗时必须是实测值。
#[cfg(target_os = "macos")]
#[test]
fn apple_vision_returns_real_dense_regions() {
    let intelligence = FaceIntelligence::new(Some(yunet_path()), models_root(), None);
    intelligence.select("apple-vision").unwrap();
    let image = load_preview("portrait.jpg");
    let (result, record) = intelligence
        .analyze("fixture/portrait.jpg", &image, &CancelToken::default())
        .unwrap();

    assert_eq!(result.provider_id, "apple-vision");
    assert!(!result.faces.is_empty());
    let face = &result.faces[0];
    assert!(face.regions.len() >= 5, "应返回脸缘/眼/鼻/唇等多个区域");
    let total: usize = face.regions.iter().map(|region| region.points.len()).sum();
    assert!(total >= 30, "稠密关键点数量异常：{total}");
    for region in &face.regions {
        for point in &region.points {
            assert!(point.x >= 0.0 && point.x <= 1.0, "x 越界：{}", point.x);
            assert!(point.y >= 0.0 && point.y <= 1.0, "y 越界：{}", point.y);
            assert!(point.confidence.is_finite());
            assert!(!point.semantic_id.is_empty(), "必须携带语义 ID");
        }
    }
    // 语义顺序：左眼在图像右侧（观察者视角），必须来自真实几何而非硬编码。
    let eye_x = |kind| {
        face.regions
            .iter()
            .find(|region| region.kind == kind)
            .map(|region| {
                region.points.iter().map(|point| point.x).sum::<f32>() / region.points.len() as f32
            })
    };
    if let (Some(right), Some(left)) = (
        eye_x(lumi_retouch_lib::face_intelligence::RegionKind::RightEye),
        eye_x(lumi_retouch_lib::face_intelligence::RegionKind::LeftEye),
    ) {
        assert!(right < left, "右眼应位于左眼左侧：{right} vs {left}");
    }
    eprintln!(
        "[apple-vision] faces={} total_points={} inference={}ms",
        result.faces.len(),
        record.total_landmarks,
        result.processing_time_ms
    );
}

/// 稠密门控通过后，LEVEL 3 参数必须真正进入像素变形；未通过时必须被忽略并说明原因。
#[cfg(target_os = "macos")]
#[test]
fn dense_gate_unlocks_level3_warp() {
    use lumi_retouch_lib::ai_engine::face_frame::{FaceFrame, NormPoint};
    use lumi_retouch_lib::face_intelligence::{anchors, dense};
    use lumi_retouch_lib::face_reshape::apply_face_reshape;
    use lumi_retouch_lib::image_engine::buffer::fit_within;
    use lumi_retouch_lib::image_engine::params::{EyeShape, FaceReshape};

    let intelligence = FaceIntelligence::new(Some(yunet_path()), models_root(), None);
    intelligence.select("apple-vision").unwrap();
    let decoded = decoder::decode(&fixture("portrait.jpg")).unwrap();
    let image = fit_within(&decoded.image, 900);
    let report = intelligence
        .probe_quality(&image, &CancelToken::default(), 2)
        .unwrap();
    if !report.verified {
        eprintln!("[dense-gate] 质量未达标，跳过解锁断言：{}", report.note);
        return;
    }
    let (result, record) = intelligence
        .analyze("fixture/portrait.jpg", &image, &CancelToken::default())
        .unwrap();
    let mut frames = Vec::new();
    for face in &result.faces {
        let Some(points) = anchors::five_point_anchors(face) else {
            continue;
        };
        let analyzed = record
            .faces
            .iter()
            .find(|item| item.face_uuid == face.face_uuid)
            .unwrap();
        assert!(
            analyzed.gate.any_enabled(),
            "已验证 Provider 应解锁 LEVEL 3"
        );
        frames.push(FaceFrame {
            id: 1,
            uuid: face.face_uuid.clone(),
            score: face.confidence,
            x: face.bounding_box.x,
            y: face.bounding_box.y,
            width: face.bounding_box.width,
            height: face.bounding_box.height,
            landmarks: points
                .iter()
                .map(|(x, y)| NormPoint { x: *x, y: *y })
                .collect(),
            dense: Some(dense::assemble_dense(face, &result)),
            level3: Some(analyzed.gate.clone()),
        });
    }
    assert!(!frames.is_empty());

    let params = FaceReshape {
        eyes: EyeShape {
            size: 40.0,
            ..Default::default()
        },
        ..FaceReshape::new(1)
    };

    let mut unlocked = image.clone();
    let reports = apply_face_reshape(
        &mut unlocked,
        &frames,
        std::slice::from_ref(&params),
        &CancelToken::default(),
    )
    .unwrap();
    assert!(reports[0].dense_landmarks, "必须报告使用了稠密关键点");
    assert!(
        reports[0]
            .notes
            .iter()
            .any(|note| note.contains("稠密轮廓")),
        "报告需说明网格来源"
    );

    let mut locked_frames = frames.clone();
    for frame in &mut locked_frames {
        frame.level3 = None;
    }
    let mut locked = image.clone();
    let locked_reports = apply_face_reshape(
        &mut locked,
        &locked_frames,
        std::slice::from_ref(&params),
        &CancelToken::default(),
    )
    .unwrap();
    assert!(!locked_reports[0].dense_landmarks);
    assert!(
        locked_reports[0]
            .notes
            .iter()
            .any(|note| note.contains("LEVEL 3 参数已忽略")),
        "锁定必须给出原因：{:?}",
        locked_reports[0].notes
    );

    let changed = unlocked
        .as_raw()
        .chunks_exact(4)
        .zip(locked.as_raw().chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count();
    assert!(changed > 100, "解锁与锁定必须产生真实像素差异：{changed}");
    eprintln!(
        "[dense-gate] unlocked_points={} locked_points={} changed_pixels={} elapsed={}ms/{}ms",
        reports[0].control_points,
        locked_reports[0].control_points,
        changed,
        reports[0].elapsed_ms,
        locked_reports[0].elapsed_ms
    );
}

#[cfg(target_os = "macos")]
#[test]
fn quality_probe_measures_stability_and_drives_gate() {
    let intelligence = FaceIntelligence::new(Some(yunet_path()), models_root(), None);
    intelligence.select("apple-vision").unwrap();
    let image = load_preview("portrait.jpg");
    let report = intelligence
        .probe_quality(&image, &CancelToken::default(), 2)
        .unwrap();

    assert_eq!(report.stability.runs, 2);
    assert!(report.stability.max_deviation >= 0.0);
    assert!(report.stability.ratio.is_finite());
    assert!(!report.stability.elapsed_ms.is_empty());
    assert!(report.stability.elapsed_ms.iter().all(|value| *value > 0));
    eprintln!(
        "[quality] provider={} runs={} max_deviation={:.6} ratio={:.4}% elapsed={:?}ms verified={} note={}",
        report.provider_id,
        report.stability.runs,
        report.stability.max_deviation,
        report.stability.ratio * 100.0,
        report.stability.elapsed_ms,
        report.verified,
        report.note
    );

    // 探针之后描述符必须反映实测结论，而不是声明。
    let descriptor = intelligence.descriptor("apple-vision").unwrap();
    let dense_verified = descriptor.evidence.iter().any(|item| {
        item.capability == "dense_landmarks"
            && item.level
                == lumi_retouch_lib::face_intelligence::capability::EvidenceLevel::Verified
    });
    assert_eq!(
        dense_verified, report.verified,
        "证据等级必须与质量探针结论一致"
    );
    assert_eq!(
        descriptor.verified_capabilities.dense_landmarks,
        report.verified
    );

    // 解锁与否直接体现在每张人脸的 LEVEL 3 门控上。
    let (_, record) = intelligence
        .analyze("fixture/portrait.jpg", &image, &CancelToken::default())
        .unwrap();
    for face in &record.faces {
        assert_eq!(face.gate.eyes.enabled, report.verified);
        if !report.verified {
            assert!(!face.gate.summary.is_empty(), "未达标必须显示原因");
        }
    }
}

/// 项目持久化：稠密关键点与门控必须随人脸记录往返，5 点记录不写稠密字段。
#[cfg(target_os = "macos")]
#[test]
fn project_face_roundtrips_dense_analysis() {
    use lumi_retouch_lib::ai_engine::face_frame::{FaceFrame, NormPoint};
    use lumi_retouch_lib::face_intelligence::types::RegionKind;
    use lumi_retouch_lib::face_intelligence::{anchors, dense};
    use lumi_retouch_lib::face_reshape::identity::ProjectFace;

    let intelligence = FaceIntelligence::new(Some(yunet_path()), models_root(), None);
    intelligence.select("apple-vision").unwrap();
    let image = load_preview("portrait.jpg");
    let report = intelligence
        .probe_quality(&image, &CancelToken::default(), 2)
        .unwrap();
    let (result, record) = intelligence
        .analyze("fixture/portrait.jpg", &image, &CancelToken::default())
        .unwrap();
    let face = &result.faces[0];
    let points = anchors::five_point_anchors(face).unwrap();
    let analyzed = record
        .faces
        .iter()
        .find(|item| item.face_uuid == face.face_uuid)
        .unwrap();
    let frame = FaceFrame {
        id: 1,
        uuid: face.face_uuid.clone(),
        score: face.confidence,
        x: face.bounding_box.x,
        y: face.bounding_box.y,
        width: face.bounding_box.width,
        height: face.bounding_box.height,
        landmarks: points
            .iter()
            .map(|(x, y)| NormPoint { x: *x, y: *y })
            .collect(),
        dense: Some(dense::assemble_dense(face, &result)),
        level3: Some(analyzed.gate.clone()),
    };
    let project_face = ProjectFace::from_frame(&frame, "apple-vision");
    let json = serde_json::to_string(&project_face).unwrap();
    assert!(json.contains("dense"), "稠密数据必须随项目保存");
    let restored: ProjectFace = serde_json::from_str(&json).unwrap();
    assert_eq!(
        restored
            .dense
            .as_ref()
            .and_then(|value| value.region(RegionKind::FaceContour))
            .map(|region| region.points.len()),
        frame
            .dense
            .as_ref()
            .and_then(|value| value.region(RegionKind::FaceContour))
            .map(|region| region.points.len())
    );
    assert_eq!(
        restored.level3.as_ref().map(|gate| gate.summary.clone()),
        frame.level3.as_ref().map(|gate| gate.summary.clone())
    );
    if report.verified {
        assert!(restored
            .level3
            .as_ref()
            .map(|gate| gate.any_enabled())
            .unwrap_or(false));
    }

    // 5 点人脸不写稠密字段，旧项目兼容。
    let mut sparse = frame.clone();
    sparse.dense = None;
    sparse.level3 = None;
    let sparse_record = ProjectFace::from_frame(&sparse, "yunet-5");
    let sparse_json = serde_json::to_string(&sparse_record).unwrap();
    assert!(
        !sparse_json.contains("\"dense\""),
        "5 点记录不得出现 dense 字段"
    );
}
