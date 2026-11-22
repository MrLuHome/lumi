//! 第四轮人脸身份持久化的端到端测试：
//! 项目保存 → 重开 → 重新检测（顺序变化） → 恢复 UUID → 参数回到对应的人。

use std::path::{Path, PathBuf};

use lumi_retouch_lib::ai_engine::face_frame::{FaceFrame, NormPoint};
use lumi_retouch_lib::face_reshape::identity::{
    match_faces, source_hash, MatchConfidence, ProjectFace,
};
use lumi_retouch_lib::image_engine::export::ExportSettings;
use lumi_retouch_lib::image_engine::params::{EditParams, FaceBeauty, FaceReshape};
use lumi_retouch_lib::image_engine::project::{self, ProjectDoc, ProjectPhoto};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lumi-reshape-{name}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn frame(id: u32, x: f32, y: f32) -> FaceFrame {
    FaceFrame {
        id,
        uuid: String::new(),
        score: 0.95,
        x,
        y,
        width: 0.2,
        height: 0.24,
        landmarks: vec![
            NormPoint {
                x: x + 0.04,
                y: y + 0.06,
            },
            NormPoint {
                x: x + 0.16,
                y: y + 0.06,
            },
            NormPoint {
                x: x + 0.10,
                y: y + 0.12,
            },
            NormPoint {
                x: x + 0.07,
                y: y + 0.16,
            },
            NormPoint {
                x: x + 0.13,
                y: y + 0.16,
            },
        ],
        dense: None,
        level3: None,
    }
}

fn record(mut frame: FaceFrame) -> ProjectFace {
    let mut record = ProjectFace::from_frame(&frame, "yunet-5");
    let uuid = record.ensure_uuid().to_string();
    frame.uuid = uuid;
    record
}

fn reshape(face_id: u32, uuid: &str, slim: f32) -> FaceReshape {
    let mut entry = FaceReshape::new(face_id);
    entry.face_uuid = uuid.into();
    entry.face.slim = slim;
    entry
}

/// 模拟真实流程：项目保存三张脸与每脸参数，重开后检测顺序变化，
/// 匹配结果必须让参数回到原来的人。
#[test]
fn three_face_project_reopens_with_params_on_correct_people() {
    let dir = temp_dir("three");
    let image_path = dir.join("group.jpg");
    std::fs::write(&image_path, b"fake-group-photo-v1").unwrap();

    let left = frame(1, 0.05, 0.2);
    let middle = frame(2, 0.40, 0.2);
    let right = frame(3, 0.75, 0.2);
    let left_record = record(left.clone());
    let middle_record = record(middle.clone());
    let right_record = record(right.clone());

    let params = EditParams {
        faces: vec![
            FaceBeauty {
                face_id: left.id,
                face_uuid: left_record.uuid.clone(),
                enabled: true,
                beauty: Default::default(),
            },
            FaceBeauty {
                face_id: middle.id,
                face_uuid: middle_record.uuid.clone(),
                enabled: true,
                beauty: Default::default(),
            },
            FaceBeauty {
                face_id: right.id,
                face_uuid: right_record.uuid.clone(),
                enabled: true,
                beauty: Default::default(),
            },
        ],
        reshape: vec![
            reshape(left.id, &left_record.uuid, 25.0),
            reshape(middle.id, &middle_record.uuid, 55.0),
            reshape(right.id, &right_record.uuid, 10.0),
        ],
        ..Default::default()
    };

    let project_path = dir.join("group.lumi");
    let doc = ProjectDoc::new(
        vec![ProjectPhoto {
            path: image_path.to_string_lossy().into_owned(),
            params: params.clone(),
            masks: Vec::new(),
            export: Some(ExportSettings::default()),
            source_hash: source_hash(&image_path).unwrap(),
            faces: vec![
                left_record.clone(),
                middle_record.clone(),
                right_record.clone(),
            ],
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
    project::save(&project_path, &doc).unwrap();

    // —— 重开：读取记录与哈希，重新检测（顺序反转、id 重新分配）。
    let loaded = project::load(&project_path).unwrap();
    let photo = &loaded.photos[0];
    assert_eq!(photo.faces.len(), 3);
    assert_eq!(photo.source_hash, source_hash(&image_path).unwrap());

    let mut left_again = left.clone();
    left_again.id = 101;
    let mut middle_again = middle.clone();
    middle_again.id = 102;
    let mut right_again = right.clone();
    right_again.id = 103;
    let detections = vec![right_again, left_again, middle_again];

    let current_hash = source_hash(Path::new(&photo.path)).unwrap();
    let outcome = match_faces(&photo.faces, &detections, current_hash != photo.source_hash);
    assert_eq!(outcome.restored.len(), 3);
    assert!(outcome.pending.is_empty());
    assert!(outcome.missing.is_empty());

    for entry in &outcome.restored {
        assert_eq!(entry.confidence, MatchConfidence::High);
        let uuid = &entry.face_uuid;
        let expected_slim = if *uuid == left_record.uuid {
            25.0
        } else if *uuid == middle_record.uuid {
            55.0
        } else {
            10.0
        };
        let resolved = photo
            .params
            .face_reshape_match(uuid, entry.face_id)
            .expect("重开后必须能按 UUID 找到该脸参数");
        assert!((resolved.face.slim - expected_slim).abs() < 0.001);
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// 原图被替换：哈希变化，全部转人工确认，任何参数都不得自动套用。
#[test]
fn replaced_source_blocks_automatic_parameter_restore() {
    let dir = temp_dir("replaced");
    let image_path = dir.join("portrait.jpg");
    std::fs::write(&image_path, b"original-photo").unwrap();
    let saved_hash = source_hash(&image_path).unwrap();

    let face = frame(1, 0.3, 0.2);
    let saved_record = record(face.clone());
    let current_hash = {
        std::fs::write(&image_path, b"replaced-photo").unwrap();
        source_hash(&image_path).unwrap()
    };
    assert_ne!(saved_hash, current_hash);

    let outcome = match_faces(
        std::slice::from_ref(&saved_record),
        std::slice::from_ref(&face),
        saved_hash != current_hash,
    );
    assert!(outcome.source_changed);
    assert!(outcome.restored.is_empty());
    assert_eq!(outcome.pending.len(), 1);
    assert_eq!(outcome.pending[0].face_uuid, saved_record.uuid);
    std::fs::remove_dir_all(&dir).ok();
}

/// 同一照片重新导入：哈希一致，重新检测后仍能高置信恢复原 UUID。
#[test]
fn reimporting_same_photo_restores_identity() {
    let dir = temp_dir("reimport");
    let first = dir.join("shot.jpg");
    let second = dir.join("shot-copy.jpg");
    std::fs::write(&first, b"same-bytes").unwrap();
    std::fs::write(&second, b"same-bytes").unwrap();
    assert_eq!(source_hash(&first).unwrap(), source_hash(&second).unwrap());

    let face = frame(1, 0.25, 0.3);
    let saved = record(face.clone());
    let mut redetected = face;
    redetected.id = 77;
    let outcome = match_faces(
        std::slice::from_ref(&saved),
        std::slice::from_ref(&redetected),
        false,
    );
    assert_eq!(outcome.restored.len(), 1);
    assert_eq!(outcome.restored[0].face_uuid, saved.uuid);
    assert_eq!(outcome.restored[0].face_id, 77);
    std::fs::remove_dir_all(&dir).ok();
}
