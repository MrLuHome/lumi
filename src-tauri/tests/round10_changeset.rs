//! ROUND 10 阶段 A：ChangeSet 持久化、幂等、并发与迁移回归。
//!
//! 所有断言都基于真实磁盘文件，不比较内存对象。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};

use serde_json::{json, Value};

use lumi_retouch_lib::face_reshape::identity::{FaceBox, ProjectFace};
use lumi_retouch_lib::image_engine::changeset::{self, ChangeSet, ChangeSetError, Operation};
use lumi_retouch_lib::image_engine::export::ExportSettings;
use lumi_retouch_lib::image_engine::params::EditParams;
use lumi_retouch_lib::image_engine::project::{
    self, ProjectColor, ProjectDoc, ProjectPhoto, ProjectRaw, ProjectSource,
};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lumi-round10-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn photo(path: &str) -> ProjectPhoto {
    ProjectPhoto {
        path: path.into(),
        params: EditParams::default(),
        masks: Vec::new(),
        export: Some(ExportSettings::default()),
        source_hash: "source-hash".into(),
        faces: Vec::new(),
        subjects: Vec::new(),
        mask_meta: None,
        source: Some(ProjectSource {
            source_type: "raster".into(),
            path: path.into(),
            hash: "source-hash".into(),
            metadata: Some(json!({ "camera": "LUMI-TEST", "iso": 200 })),
        }),
        raw: Some(ProjectRaw {
            params: Default::default(),
            decoder_version: project::RAW_DECODER_VERSION.into(),
            extra: Default::default(),
        }),
        color: Some(ProjectColor {
            output: Default::default(),
            extra: Default::default(),
        }),
        extra: Default::default(),
    }
}

fn project(path: &str) -> ProjectDoc {
    let mut doc = ProjectDoc::new(
        vec![photo(path), photo("/photos/b.jpg")],
        0,
        ExportSettings::default(),
    );
    doc.project_uuid = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into();
    doc
}

fn write_project(dir: &Path, doc: &ProjectDoc) -> PathBuf {
    let path = dir.join("project.lumi");
    project::save(&path, doc).unwrap();
    path
}

fn disk_revision(path: &Path) -> u64 {
    let value: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    value.get("revision").and_then(Value::as_u64).unwrap_or(0)
}

fn request(rev: u64, request_id: &str, operations: Vec<Operation>) -> ChangeSet {
    ChangeSet {
        project_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into(),
        base_revision: rev,
        request_id: request_id.into(),
        operations,
        created_at: "2026-02-01T00:00:00Z".into(),
        rebase_on_conflict: false,
    }
}

#[test]
fn rebase_reapplies_increments_on_latest_state_without_overwriting() {
    let dir = temp_dir("rebase");
    let path = write_project(&dir, &project("/photos/a.jpg"));
    let base = disk_revision(&path);
    changeset::apply(
        &path,
        &request(base, "first", vec![set_param("/photos/a.jpg", "exposure", 10.0)]),
    )
    .unwrap();
    // 另一个窗口基于旧 revision 保存，但允许 rebase：只重放自己的增量。
    let mut second = request(base, "second", vec![set_param("/photos/a.jpg", "contrast", 20.0)]);
    second.rebase_on_conflict = true;
    let outcome = changeset::apply(&path, &second).unwrap();
    assert_eq!(outcome.revision, base + 2);
    let doc = project::load(&path).unwrap();
    assert_eq!(doc.photos[0].params.exposure, 10.0, "rebase 不得覆盖他人字段");
    assert_eq!(doc.photos[0].params.contrast, 20.0);
}

fn set_param(path: &str, key: &str, value: f32) -> Operation {
    Operation::UpdatePhotoParams {
        photo: path.into(),
        patch: json!({ key: value }),
    }
}

#[test]
fn concurrent_writers_do_not_lose_updates() {
    let dir = temp_dir("concurrent");
    let path = write_project(&dir, &project("/photos/a.jpg"));
    let barrier = Arc::new(Barrier::new(8));
    let fields: Vec<(&str, f32)> = vec![
        ("exposure", 1.5),
        ("contrast", 2.5),
        ("highlights", 3.5),
        ("shadows", 4.5),
        ("whites", 5.5),
        ("blacks", 6.5),
        ("temperature", 7.5),
        ("tint", 8.5),
    ];
    let mut handles = Vec::new();
    for (index, (key, value)) in fields.iter().enumerate() {
        let path = path.clone();
        let barrier = barrier.clone();
        let key = (*key).to_string();
        let value = *value;
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            // 冲突时重新读取最新 revision 再合并，绝不重放旧快照。
            for attempt in 0..200 {
                let rev = disk_revision(&path);
                let result = changeset::apply(
                    &path,
                    &request(
                        rev,
                        &format!("writer-{index}-{attempt}"),
                        vec![set_param("/photos/a.jpg", &key, value)],
                    ),
                );
                match result {
                    Ok(_) => return,
                    Err(ChangeSetError::RevisionConflict { .. }) => {
                        std::thread::yield_now();
                    }
                    Err(error) => panic!("并发保存失败：{error}"),
                }
            }
            panic!("并发保存重试耗尽：{key}");
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }
    let doc = project::load(&path).unwrap();
    let params = &doc.photos[0].params;
    assert_eq!(params.exposure, 1.5);
    assert_eq!(params.contrast, 2.5);
    assert_eq!(params.highlights, 3.5);
    assert_eq!(params.shadows, 4.5);
    assert_eq!(params.whites, 5.5);
    assert_eq!(params.blacks, 6.5);
    assert_eq!(params.temperature, 7.5);
    assert_eq!(params.tint, 8.5);
    // 第二个照片未被任何操作触碰。
    assert_eq!(doc.photos[1].params.exposure, 0.0);
}

#[test]
fn same_base_revision_second_save_is_rejected() {
    let dir = temp_dir("same-base");
    let path = write_project(&dir, &project("/photos/a.jpg"));
    let rev = disk_revision(&path);
    changeset::apply(&path, &request(rev, "manual-1", vec![set_param("/photos/a.jpg", "exposure", 10.0)]))
        .unwrap();
    let error = changeset::apply(
        &path,
        &request(rev, "manual-2", vec![set_param("/photos/a.jpg", "contrast", 20.0)]),
    )
    .unwrap_err();
    assert_eq!(error.kind(), "REVISION_CONFLICT");
    let doc = project::load(&path).unwrap();
    assert_eq!(doc.photos[0].params.exposure, 10.0);
    assert_eq!(doc.photos[0].params.contrast, 0.0, "旧快照不得覆盖新数据");
}

#[test]
fn response_lost_retry_is_idempotent() {
    let dir = temp_dir("lost-response");
    let path = write_project(&dir, &project("/photos/a.jpg"));
    let rev = disk_revision(&path);
    let save = request(
        rev,
        "timeout-retry",
        vec![Operation::AddMask {
            photo: "/photos/a.jpg".into(),
            mask: json!({
                "id": "mask-1", "name": "蒙版", "kind": "brush",
                "enabled": true, "invert": false, "feather": 40.0, "opacity": 100.0,
                "points": []
            }),
        }],
    );
    let first = changeset::apply(&path, &save).unwrap();
    // IPC 超时后前端用同一 requestId 重试（携带旧 baseRevision）。
    let second = changeset::apply(&path, &save).unwrap();
    assert!(second.duplicate);
    assert_eq!(second.revision, first.revision);
    let doc = project::load(&path).unwrap();
    assert_eq!(doc.photos[0].masks.len(), 1, "重放不得重复新增蒙版");
    assert_eq!(doc.revision, first.revision);
}

#[test]
fn same_request_id_with_different_operations_is_not_treated_as_replay() {
    let dir = temp_dir("request-id-reuse");
    let path = write_project(&dir, &project("/photos/a.jpg"));
    let rev = disk_revision(&path);
    changeset::apply(&path, &request(rev, "dup", vec![set_param("/photos/a.jpg", "exposure", 1.0)]))
        .unwrap();
    let error = changeset::apply(
        &path,
        &request(rev, "dup", vec![set_param("/photos/a.jpg", "contrast", 1.0)]),
    )
    .unwrap_err();
    assert_eq!(error.kind(), "REVISION_CONFLICT");
}

#[test]
fn switching_projects_cannot_cross_write() {
    let dir_a = temp_dir("project-a");
    let dir_b = temp_dir("project-b");
    let path_a = write_project(&dir_a, &project("/photos/a.jpg"));
    let mut other = project("/photos/a.jpg");
    other.project_uuid = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into();
    let path_b = write_project(&dir_b, &other);

    let rev_b = disk_revision(&path_b);
    let error = changeset::apply(
        &path_b,
        &request(rev_b, "from-a", vec![set_param("/photos/a.jpg", "exposure", 9.0)]),
    )
    .unwrap_err();
    assert_eq!(error.kind(), "PROJECT_MISMATCH");
    assert_eq!(project::load(&path_a).unwrap().photos[0].params.exposure, 0.0);
    assert_eq!(project::load(&path_b).unwrap().photos[0].params.exposure, 0.0);
}

#[test]
fn rapid_edits_on_same_photo_keep_latest_value_and_increment_revision() {
    let dir = temp_dir("rapid");
    let path = write_project(&dir, &project("/photos/a.jpg"));
    let mut revision = disk_revision(&path);
    for step in 1..=20 {
        let outcome = changeset::apply(
            &path,
            &request(
                revision,
                &format!("rapid-{step}"),
                vec![set_param("/photos/a.jpg", "exposure", step as f32)],
            ),
        )
        .unwrap();
        revision = outcome.revision;
    }
    let doc = project::load(&path).unwrap();
    assert_eq!(doc.photos[0].params.exposure, 20.0);
    assert_eq!(doc.revision, revision);
    assert_eq!(
        doc.extra.get("lumiAppliedRequests").and_then(Value::as_array).map(Vec::len),
        Some(20)
    );
}

#[test]
fn failed_operation_keeps_original_file_untouched() {
    let dir = temp_dir("atomic-failure");
    let path = write_project(&dir, &project("/photos/a.jpg"));
    let before = std::fs::read(&path).unwrap();
    let rev = disk_revision(&path);
    let error = changeset::apply(
        &path,
        &request(
            rev,
            "invalid",
            vec![
                set_param("/photos/a.jpg", "exposure", 5.0),
                set_param("/photos/missing.jpg", "exposure", 5.0),
            ],
        ),
    )
    .unwrap_err();
    assert_eq!(error.kind(), "TARGET_NOT_FOUND");
    let after = std::fs::read(&path).unwrap();
    assert_eq!(before, after, "部分失败不得提交一半");
    assert_eq!(project::load(&path).unwrap().photos[0].params.exposure, 0.0);
}

#[test]
fn changeset_roundtrip_preserves_masks_faces_and_unknown_fields() {
    let dir = temp_dir("roundtrip");
    let mut doc = project("/photos/a.jpg");
    doc.photos[0].extra.insert("futurePhotoField".into(), json!({ "keep": 1 }));
    doc.photos[0]
        .params
        .extra
        .insert("futureParamField".into(), json!([1, 2, 3]));
    doc.photos[0].faces.push(ProjectFace {
        uuid: "face-1".into(),
        bbox: FaceBox { x: 0.1, y: 0.1, width: 0.2, height: 0.2 },
        landmarks: Vec::new(),
        score: 0.9,
        model: "yunet-5".into(),
        confidence: 0.9,
        mask_ref: Some("mask-1".into()),
        dense: None,
        level3: None,
        extra: Default::default(),
    });
    let path = write_project(&dir, &doc);
    let rev = disk_revision(&path);
    let outcome = changeset::apply(
        &path,
        &request(
            rev,
            "mask-add",
            vec![
                Operation::AddMask {
                    photo: "/photos/a.jpg".into(),
                    mask: json!({
                        "id": "mask-1", "name": "蒙版", "kind": "radial",
                        "enabled": true, "invert": false, "feather": 30.0, "opacity": 80.0,
                        "points": [{ "x": 0.5, "y": 0.5, "radius": 0.2 }]
                    }),
                },
                Operation::AddFace {
                    photo: "/photos/a.jpg".into(),
                    face: json!({
                        "faceId": 1,
                        "faceUuid": "face-1",
                        "enabled": true,
                        "beauty": { "skinSmooth": 0.0, "texture": 0.0, "spots": 0.0,
                            "whitening": 0.0, "nasolabial": 0.0, "eyeBag": 0.0,
                            "teeth": 0.0, "lipColor": 0.0, "faceShape": 0.0 }
                    }),
                },
                Operation::UpdateFaceParams {
                    photo: "/photos/a.jpg".into(),
                    face_uuid: "face-1".into(),
                    patch: json!({ "beauty": { "skinSmooth": 22.0 } }),
                },
                Operation::UpdateSource {
                    photo: "/photos/a.jpg".into(),
                    patch: json!({ "hash": "new-hash" }),
                },
            ],
        ),
    )
    .unwrap();
    assert_eq!(outcome.revision, rev + 1);

    let reloaded = project::load(&path).unwrap();
    let entry = &reloaded.photos[0];
    assert_eq!(entry.masks.len(), 1);
    assert_eq!(entry.masks[0].id, "mask-1");
    assert_eq!(entry.masks[0].points[0].radius, 0.2);
    assert_eq!(entry.faces.len(), 1);
    assert_eq!(entry.faces[0].mask_ref.as_deref(), Some("mask-1"));
    assert_eq!(entry.source.as_ref().unwrap().hash, "new-hash");
    // 未修改字段与未知字段保持不变。
    assert_eq!(
        entry.source.as_ref().unwrap().metadata,
        Some(json!({ "camera": "LUMI-TEST", "iso": 200 }))
    );
    assert_eq!(entry.extra.get("futurePhotoField"), Some(&json!({ "keep": 1 })));
    assert_eq!(
        entry.params.extra.get("futureParamField"),
        Some(&json!([1, 2, 3]))
    );
    assert_eq!(
        entry.params.faces[0].beauty.skin_smooth,
        22.0
    );
}

#[test]
fn migration_keeps_unknown_fields_dense_raw_and_color() {
    let dir = temp_dir("migration");
    let path = dir.join("legacy.lumi");
    let mut value = serde_json::to_value(project("/photos/a.jpg")).unwrap();
    value["version"] = json!(1);
    value["legacyTopLevel"] = json!({ "aiNotes": "keep-me" });
    value["photos"][0]["legacyPhotoField"] = json!("keep");
    value["photos"][0]["faces"] = json!([{
        "uuid": "face-dense",
        "bbox": { "x": 0.1, "y": 0.1, "width": 0.2, "height": 0.2 },
        "landmarks": [{ "x": 0.2, "y": 0.2 }],
        "score": 0.9,
        "model": "yunet-5",
        "confidence": 0.9,
        "maskRef": "mask-1",
        "dense": {
            "topologyId": "face-68",
            "providerId": "apple-vision",
            "providerVersion": "rev1",
            "imageWidth": 100,
            "imageHeight": 100,
            "boundingBox": { "x": 0.1, "y": 0.1, "width": 0.2, "height": 0.2 },
            "confidence": 0.9,
            "regions": []
        },
        "futureFaceField": { "keep": true }
    }]);
    value["photos"][0]["raw"]["decoderVersion"] = json!("libraw-test");
    value["photos"][0]["color"]["output"]["profile"] = json!("displayP3");
    let original = serde_json::to_vec_pretty(&value).unwrap();
    std::fs::write(&path, &original).unwrap();

    let (loaded, log) = project::load_with_report(&path).unwrap();
    assert_eq!(loaded.version, 1);
    assert!(!log.is_empty(), "旧项目必须产生迁移日志");
    assert_eq!(
        loaded.extra.get("legacyTopLevel"),
        Some(&json!({ "aiNotes": "keep-me" }))
    );
    assert_eq!(
        loaded.photos[0].extra.get("legacyPhotoField"),
        Some(&json!("keep"))
    );
    let face = &loaded.photos[0].faces[0];
    assert_eq!(
        face.dense.as_ref().expect("稠密关键点不得清空").topology_id,
        "face-68"
    );
    assert_eq!(face.mask_ref.as_deref(), Some("mask-1"));
    assert_eq!(
        face.extra.get("futureFaceField"),
        Some(&json!({ "keep": true }))
    );
    assert_eq!(
        loaded.photos[0].raw.as_ref().unwrap().decoder_version,
        "libraw-test",
        "迁移不得重置 RAW 解码器版本"
    );
    assert_eq!(
        serde_json::to_value(loaded.photos[0].color.as_ref().unwrap().output.profile).unwrap(),
        json!("displayP3"),
        "迁移不得重置 ICC/色彩配置"
    );
    // 迁移不修改磁盘文件。
    assert_eq!(std::fs::read(&path).unwrap(), original);
}

#[test]
fn higher_version_project_is_rejected() {
    let dir = temp_dir("future-version");
    let path = dir.join("future.lumi");
    let mut value = serde_json::to_value(project("/photos/a.jpg")).unwrap();
    value["version"] = json!(99);
    std::fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    let error = project::load(&path).unwrap_err();
    assert!(error.to_string().contains("高于当前支持"));
}
