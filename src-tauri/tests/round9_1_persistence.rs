//! ROUND 9.1 持久化等价性与故障注入。
//!
//! 所有断言都基于真实磁盘文件（正式 `project::save` / `project::load`），
//! 不比较内存对象：保存 → 读回 → 再保存 → 读回，逐轮比较。

#![recursion_limit = "512"]

use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use lumi_retouch_lib::image_engine::project::{self, ProjectDoc, PROJECT_VERSION};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lumi-round91-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// 覆盖任务书要求的所有数据类别，并故意带上旧客户端不认识的字段。
fn rich_project() -> Value {
    let params = json!({
        "exposure": 24.0,
        "contrast": -8.0,
        "highlights": -20.0,
        "shadows": 15.0,
        "whites": 5.0,
        "blacks": -6.0,
        "temperature": 12.0,
        "tint": 3.0,
        "saturation": 9.0,
        "vibrance": 11.0,
        "clarity": 8.0,
        "sharpness": 12.0,
        "grain": 4.0,
        "vignette": 7.0,
        "skinSmooth": 5.0,
        "faces": [
            {
                "faceId": 101,
                "faceUuid": "18e9fd0f-9771-49b8-8001-df85c8fbfc19",
                "enabled": true,
                "beauty": {
                    "skinSmooth": 17.0, "texture": 33.0, "spots": 11.0,
                    "whitening": 8.0, "nasolabial": 9.0, "eyeBag": 3.0,
                    "teeth": 2.0, "lipColor": 4.0, "faceShape": 6.0
                }
            },
            {
                "faceId": 202,
                "faceUuid": "28e9fd0f-9771-49b8-8001-df85c8fbfc20",
                "enabled": true,
                "beauty": {
                    "skinSmooth": 44.0, "texture": 0.0, "spots": 0.0,
                    "whitening": 0.0, "nasolabial": 22.0, "eyeBag": 0.0,
                    "teeth": 0.0, "lipColor": 0.0, "faceShape": 0.0
                }
            }
        ],
        "reshape": [{
            "faceId": 101,
            "faceUuid": "18e9fd0f-9771-49b8-8001-df85c8fbfc19",
            "enabled": true,
            "face": {
                "slim": 22.0, "left": 0.0, "right": 0.0, "jaw": -11.0,
                "cheekbone": 0.0, "width": 0.0, "length": 0.0, "balance": 0.0
            },
            "eyes": {
                "size": 14.0, "left": 0.0, "right": 0.0, "width": 0.0,
                "height": 0.0, "distance": 0.0, "corner": 0.0
            },
            "nose": { "bridge": 0.0, "alar": 0.0, "tip": -6.0, "height": 0.0, "balance": 0.0 },
            "mouth": {
                "width": 5.0, "upper": 0.0, "lower": 0.0, "corner": 0.0,
                "symmetry": 0.0, "size": 0.0
            },
            "chin": { "length": 9.0, "width": 0.0, "point": 0.0, "jawline": 0.0 },
            "manual": { "enabled": false, "strokes": [] }
        }],
        "subjectMask": {
            "enabled": true,
            "source": "manual",
            "modelVersion": "manual-alpha-1",
            "feather": 22.0,
            "expand": -14.0,
            "strokes": [{
                "kind": "paint", "radius": 0.12, "hardness": 55.0, "strength": 90.0,
                "points": [{ "x": 0.32, "y": 0.44 }, { "x": 0.36, "y": 0.48 }]
            }]
        },
        "body": [{
            "subjectId": "subject-1",
            "enabled": true,
            "shape": {
                "waist": -30.0, "shoulders": 12.0, "hips": 8.0,
                "thighs": 0.0, "calves": 0.0, "arms": 0.0, "legs": 0.0
            },
            "anchors": [{
                "part": "waistLeft", "x": 0.3, "y": 0.6, "dx": -0.03,
                "dy": 0.01, "radius": 0.12, "locked": true
            }],
            "strokes": [],
            "maskStrokes": []
        }],
        "background": {
            "enabled": true, "mode": "solidColor", "blur": 62.0, "depth": 41.0,
            "brightness": 0.0, "temperature": 0.0, "saturation": 0.0,
            "color": "#102030",
            "gradient": { "angle": 0.0, "stops": [] },
            "imagePath": "", "imageFit": "cover", "imageScale": 100.0,
            "imageOffsetX": 0.0, "imageOffsetY": 0.0, "imageBlur": 0.0,
            "decontaminate": 60.0
        },
        "manualMask": {
            "enabled": true, "invert": false, "feather": 33.0, "strength": 88.0,
            "strokes": [
                { "erase": false, "radius": 0.05, "points": [{ "x": 0.5, "y": 0.5 }] },
                { "erase": true, "radius": 0.03, "points": [{ "x": 0.52, "y": 0.52 }] }
            ]
        },
        "repairPoints": [{ "x": 0.4, "y": 0.6, "radius": 0.02 }],
        "hsl": {
            "red": { "hue": 0.0, "saturation": 0.0, "luminance": 0.0 },
            "orange": { "hue": 12.0, "saturation": 18.0, "luminance": -4.0 },
            "yellow": { "hue": 0.0, "saturation": 0.0, "luminance": 0.0 },
            "green": { "hue": 0.0, "saturation": 0.0, "luminance": 0.0 },
            "aqua": { "hue": 0.0, "saturation": 0.0, "luminance": 0.0 },
            "blue": { "hue": 0.0, "saturation": 0.0, "luminance": 0.0 },
            "purple": { "hue": 0.0, "saturation": 0.0, "luminance": 0.0 },
            "magenta": { "hue": 0.0, "saturation": 0.0, "luminance": 0.0 }
        },
        "curves": {
            "rgb": [{ "x": 0.0, "y": 0.0 }, { "x": 0.5, "y": 0.55 }, { "x": 1.0, "y": 1.0 }],
            "red": [{ "x": 0.0, "y": 0.0 }, { "x": 1.0, "y": 1.0 }],
            "green": [{ "x": 0.0, "y": 0.0 }, { "x": 1.0, "y": 1.0 }],
            "blue": [{ "x": 0.0, "y": 0.0 }, { "x": 1.0, "y": 1.0 }]
        },
        "lut": { "path": "/luts/portrait.cube", "name": "portrait", "strength": 65.0 },
        // 旧客户端不认识的参数：必须原样保留。
        "futureParamField": { "enabled": true, "strength": 0.5 }
    });

    let dense_regions: Vec<Value> = ["faceContour", "leftEye", "rightEye", "nose", "outerLips"]
        .iter()
        .enumerate()
        .map(|(index, kind)| {
            json!({
                "kind": kind,
                "closed": true,
                "points": (0..17).map(|point| json!({
                    "x": 0.10 + (index as f32) * 0.01 + (point as f32) * 0.001,
                    "y": 0.20 + (point as f32) * 0.002,
                    "confidence": 0.93,
                    "semanticId": format!("{kind}.{point}")
                })).collect::<Vec<_>>()
            })
        })
        .collect();

    json!({
        "version": 3,
        "createdAt": "2026-11-20T10:00:00.000Z",
        "activeIndex": 0,
        "export": {
            "format": "jpeg", "quality": 92, "resize": null,
            "filenameTemplate": "", "watermark": null
        },
        "compute": { "preference": "cpu" },
        "projectUuid": "project-uuid-round91",
        "revision": 7,
        "updatedAt": "2026-11-20T11:00:00.000Z",
        "futureDocField": { "schemaHint": "v4" },
        "photos": [
            {
                "path": "/photos/group.jpg",
                "params": params,
                "masks": [
                    {
                        "id": "mask-1", "name": "面部提亮", "kind": "radial",
                        "enabled": true, "invert": false, "feather": 30.0,
                        "opacity": 80.0, "points": [{ "x": 0.5, "y": 0.4, "radius": 0.2 }]
                    },
                    {
                        "id": "mask-2", "name": "皮肤蒙版", "kind": "brush",
                        "enabled": true, "invert": true, "feather": 12.0,
                        "opacity": 60.0,
                        "points": [
                            { "x": 0.2, "y": 0.2, "radius": 0.05 },
                            { "x": 0.3, "y": 0.3, "radius": 0.05 }
                        ]
                    }
                ],
                "export": {
                    "format": "png", "quality": 88, "resize": null,
                    "filenameTemplate": "a", "watermark": null
                },
                "sourceHash": "sha256-a",
                "faces": [
                    {
                        "uuid": "18e9fd0f-9771-49b8-8001-df85c8fbfc19",
                        "bbox": { "x": 0.1, "y": 0.1, "width": 0.2, "height": 0.24 },
                        "landmarks": [
                            { "x": 0.15, "y": 0.2 }, { "x": 0.25, "y": 0.2 },
                            { "x": 0.2, "y": 0.25 }, { "x": 0.17, "y": 0.29 },
                            { "x": 0.23, "y": 0.29 }
                        ],
                        "score": 0.94,
                        "model": "yunet-5",
                        "confidence": 0.94,
                        "maskRef": "mask-1",
                        "dense": {
                            "topologyId": "face-68",
                            "providerId": "apple-vision",
                            "providerVersion": "vision-rev3",
                            "imageWidth": 4000,
                            "imageHeight": 3000,
                            "boundingBox": { "x": 0.1, "y": 0.1, "width": 0.2, "height": 0.24 },
                            "confidence": 0.91,
                            "regions": dense_regions
                        },
                        "level3": {
                            "providerId": "apple-vision",
                            "providerVersion": "vision-rev3",
                            "topologyId": "face-68",
                            "modelVersion": "macOS Vision revision 3",
                            "faceUuid": "18e9fd0f-9771-49b8-8001-df85c8fbfc19",
                            "denseLandmarks": true,
                            "eyes": { "enabled": true, "reason": "verified" },
                            "nose": { "enabled": true, "reason": "verified" },
                            "mouth": { "enabled": true, "reason": "verified" },
                            "chin": { "enabled": true, "reason": "verified" },
                            "summary": "LEVEL 3 unlocked",
                            "issues": []
                        },
                        "futureFaceField": { "note": "preserve" }
                    },
                    {
                        "uuid": "28e9fd0f-9771-49b8-8001-df85c8fbfc20",
                        "bbox": { "x": 0.6, "y": 0.1, "width": 0.18, "height": 0.2 },
                        "landmarks": [],
                        "score": 0.88,
                        "model": "yunet-5",
                        "confidence": 0.88,
                        "maskRef": null
                    }
                ],
                "subjects": [{
                    "subjectId": "subject-1",
                    "maskSource": "manual",
                    "maskModelVersion": "manual-alpha-1",
                    "maskStrokes": 0,
                    "body": {
                        "waist": -30.0, "shoulders": 12.0, "hips": 8.0,
                        "thighs": 0.0, "calves": 0.0, "arms": 0.0, "legs": 0.0
                    },
                    "anchors": 1
                }],
                "maskMeta": {
                    "source": "manual",
                    "modelId": "manual-alpha",
                    "modelVersion": "manual-alpha-1",
                    "coverage": 0.37,
                    "strokeHash": "abc123",
                    "revision": 4
                },
                "source": {
                    "sourceType": "raster",
                    "path": "/photos/group.jpg",
                    "hash": "sha256-a",
                    "metadata": { "make": "Canon", "model": "R5", "iso": 400 }
                },
                "raw": {
                    "params": {
                        "exposureEv": 0.75,
                        "highlights": -10.0,
                        "shadows": 4.0,
                        "wbMode": "custom",
                        "temperature": 5200.0,
                        "tint": 3.0,
                        "demosaic": "mhc",
                        "maxEdge": null,
                        "futureDevelopField": 7
                    },
                    "decoderVersion": "libraw-0.21.1+lumi-mhc-1",
                    "futureRawField": "keep-raw"
                },
                "color": {
                    "output": {
                        "profile": "displayP3", "customPath": null, "intent": "relative",
                        "blackPointCompensation": true, "embed": true,
                        "futureOutputField": { "x": 1 }
                    },
                    "futureColorField": true
                },
                "futurePhotoField": { "layers": [1, 2, 3] }
            },
            {
                "path": "/photos/b.jpg",
                "params": { "exposure": -5.0 },
                "masks": [],
                "export": {
                    "format": "jpeg", "quality": 70, "resize": null,
                    "filenameTemplate": "b", "watermark": null
                },
                "sourceHash": "sha256-b",
                "faces": [],
                "subjects": [],
                "maskMeta": null,
                "source": {
                    "sourceType": "raster", "path": "/photos/b.jpg",
                    "hash": "sha256-b", "metadata": { "make": "Sony", "model": "A7" }
                },
                "raw": null,
                "color": {
                    "output": {
                        "profile": "srgb", "customPath": null, "intent": "perceptual",
                        "blackPointCompensation": false, "embed": false
                    }
                }
            }
        ]
    })
}

fn write_json(path: &Path, value: &Value) {
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn mask_digest(project: &ProjectDoc) -> String {
    // 蒙版内容 hash：比较真实读回后的像素/笔触数据，而不是文件大小。
    let mut hasher = Sha256::new();
    for photo in &project.photos {
        hasher.update(serde_json::to_vec(&photo.masks).unwrap());
        hasher.update(serde_json::to_vec(&photo.params.manual_mask).unwrap());
        hasher.update(serde_json::to_vec(&photo.params.subject_mask).unwrap());
        if let Some(meta) = &photo.mask_meta {
            hasher.update(serde_json::to_vec(meta).unwrap());
        }
    }
    format!("{:x}", hasher.finalize())
}

/// 递归比较：键集合必须一致；浮点允许 1e-6 绝对误差。
fn assert_equivalent(left: &Value, right: &Value, at: &str) {
    match (left, right) {
        (Value::Object(a), Value::Object(b)) => {
            let mut keys: Vec<&String> = a.keys().collect();
            keys.sort();
            let mut other: Vec<&String> = b.keys().collect();
            other.sort();
            assert_eq!(keys, other, "对象键集合不一致：{at}");
            for key in keys {
                assert_equivalent(&a[key], &b[key], &format!("{at}.{key}"));
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "数组长度不一致：{at}");
            for (index, (x, y)) in a.iter().zip(b.iter()).enumerate() {
                assert_equivalent(x, y, &format!("{at}[{index}]"));
            }
        }
        (Value::Number(a), Value::Number(b)) => {
            let (x, y) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            assert!(
                (x - y).abs() <= 1e-6,
                "数值不一致：{at} 左={x} 右={y}"
            );
        }
        _ => assert_eq!(left, right, "值不一致：{at}"),
    }
}

fn snapshot(doc: &ProjectDoc) -> Value {
    let mut value = serde_json::to_value(doc).unwrap();
    // 保存代数与更新时间属于预期变化项，单独断言。
    if let Some(object) = value.as_object_mut() {
        object.remove("revision");
        object.remove("updatedAt");
    }
    value
}

#[test]
fn save_load_save_preserves_every_category() {
    let dir = temp_dir("equivalence");
    let path = dir.join("rich.lumi");
    let source = rich_project();
    write_json(&path, &source);
    let loaded = project::load(&path).unwrap();
    assert_eq!(loaded.version, PROJECT_VERSION);
    assert_eq!(loaded.created_at, "2026-11-20T10:00:00.000Z");
    assert_eq!(loaded.project_uuid, "project-uuid-round91");
    assert_eq!(loaded.revision, 7);
    assert_eq!(loaded.photos.len(), 2);
    assert_eq!(loaded.photos[0].faces.len(), 2);
    assert_eq!(loaded.photos[0].masks.len(), 2);
    let dense = loaded.photos[0].faces[0].dense.as_ref().unwrap();
    assert_eq!(dense.topology_id, "face-68");
    assert_eq!(dense.regions.iter().map(|region| region.points.len()).sum::<usize>(), 85);
    let level3 = loaded.photos[0].faces[0].level3.as_ref().unwrap();
    assert!(level3.dense_landmarks);
    assert!(level3.eyes.enabled);
    // ROUND 9.1 R9-SAVE-09：导出设置里的旧/新版本字段不能因反序列化丢失。
    let export = loaded.photos[0].export.as_ref().unwrap();
    assert_eq!(
        export.extra.get("filenameTemplate"),
        Some(&json!("a")),
        "导出设置的未知字段必须在加载后保留"
    );
    // ROUND 9.1：RAW/色彩记录内嵌的未知字段同样不能丢。
    assert_eq!(
        loaded.photos[0].raw.as_ref().unwrap().extra.get("futureRawField"),
        Some(&json!("keep-raw"))
    );
    assert_eq!(
        loaded.photos[0].color.as_ref().unwrap().extra.get("futureColorField"),
        Some(&json!(true))
    );
    assert_eq!(
        loaded.photos[0]
            .raw
            .as_ref()
            .unwrap()
            .params
            .extra
            .get("futureDevelopField"),
        Some(&json!(7))
    );
    assert_eq!(
        loaded.photos[0]
            .color
            .as_ref()
            .unwrap()
            .output
            .extra
            .get("futureOutputField"),
        Some(&json!({ "x": 1 }))
    );

    // 夹具里的 strokeHash 是占位值，先按真实笔触算法归一化，
    // 否则会被正确判定为“笔触已变化”并重置 coverage。
    let mut loaded = loaded;
    loaded.photos[0].mask_meta.as_mut().unwrap().stroke_hash =
        loaded.photos[0].params.subject_mask.stroke_hash();

    let digest_before = mask_digest(&loaded);
    let first = project::save_with_meta(&path, &loaded).unwrap();
    assert_eq!(first.project_uuid, "project-uuid-round91");
    assert_eq!(first.revision, 8);

    let reloaded = project::load(&path).unwrap();
    assert_eq!(reloaded.created_at, loaded.created_at);
    assert!(!reloaded.updated_at.is_empty());
    assert_eq!(mask_digest(&reloaded), digest_before);
    assert_equivalent(&snapshot(&reloaded), &snapshot(&loaded), "root");

    // 未知字段必须还在文件里。
    let text = std::fs::read_to_string(&path).unwrap();
    for needle in [
        "\"futureDocField\"",
        "\"futurePhotoField\"",
        "\"futureParamField\"",
        "\"futureFaceField\"",
        "\"schemaHint\"",
        "\"note\": \"preserve\"",
    ] {
        assert!(text.contains(needle), "未知字段 {needle} 被静默丢弃");
    }

    // 再保存一次：数据不变，仅代数递增。
    let second = project::save_with_meta(&path, &reloaded).unwrap();
    assert_eq!(second.revision, 9);
    let third = project::load(&path).unwrap();
    assert_equivalent(&snapshot(&third), &snapshot(&reloaded), "second");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn twenty_roundtrips_are_identical() {
    let dir = temp_dir("rounds20");
    let path = dir.join("loop.lumi");
    write_json(&path, &rich_project());

    let mut current = project::load(&path).unwrap();
    current.photos[0].mask_meta.as_mut().unwrap().stroke_hash =
        current.photos[0].params.subject_mask.stroke_hash();
    let baseline = snapshot(&current);
    let baseline_masks = mask_digest(&current);
    let baseline_faces = serde_json::to_value(&current.photos[0].faces).unwrap();

    for round in 0..20 {
        let outcome = project::save_with_meta(&path, &current).unwrap();
        assert_eq!(outcome.revision, 8 + round as u64);
        current = project::load(&path).unwrap();
        assert_equivalent(&snapshot(&current), &baseline, &format!("round-{round}"));
        assert_eq!(mask_digest(&current), baseline_masks, "第 {round} 轮蒙版变化");
        assert_eq!(
            serde_json::to_value(&current.photos[0].faces).unwrap(),
            baseline_faces,
            "第 {round} 轮人脸记录变化"
        );
        assert_eq!(current.created_at, "2026-11-20T10:00:00.000Z");
        assert_eq!(current.project_uuid, "project-uuid-round91");
    }

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn partial_edit_does_not_touch_other_photos() {
    let dir = temp_dir("partial");
    let path = dir.join("partial.lumi");
    write_json(&path, &rich_project());
    let mut loaded = project::load(&path).unwrap();
    let other_before = serde_json::to_value(&loaded.photos[1]).unwrap();

    loaded.photos[0].params.exposure = 55.0;
    project::save_with_meta(&path, &loaded).unwrap();
    let reloaded = project::load(&path).unwrap();

    assert_eq!(reloaded.photos[0].params.exposure, 55.0);
    assert_eq!(reloaded.photos[0].params.contrast, -8.0);
    assert_eq!(reloaded.photos[0].params.faces.len(), 2);
    assert_eq!(reloaded.photos[0].params.body[0].shape.waist, -30.0);
    assert_eq!(reloaded.photos[0].faces.len(), 2);
    assert_eq!(reloaded.photos[0].masks.len(), 2);
    // 非目标照片逐字段一致（只允许 maskMeta/subjects 的派生同步差异，这里二者本就一致）。
    assert_equivalent(
        &serde_json::to_value(&reloaded.photos[1]).unwrap(),
        &other_before,
        "photo-b",
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn stale_revision_and_foreign_project_are_rejected() {
    let dir = temp_dir("conflict");
    let path = dir.join("conflict.lumi");
    write_json(&path, &rich_project());
    let original = std::fs::read(&path).unwrap();

    let mut stale = project::load(&path).unwrap();
    stale.revision = 3; // 磁盘是 7
    let error = project::save_with_meta(&path, &stale).unwrap_err();
    assert!(
        format!("{error}").contains("revision"),
        "旧快照必须被拒绝：{error}"
    );
    assert_eq!(std::fs::read(&path).unwrap(), original, "拒绝时不得改动原文件");

    let mut foreign = project::load(&path).unwrap();
    foreign.project_uuid = "another-project".into();
    let error = project::save_with_meta(&path, &foreign).unwrap_err();
    assert!(
        format!("{error}").contains("另一个项目"),
        "不同项目不得互相覆盖：{error}"
    );
    assert_eq!(std::fs::read(&path).unwrap(), original, "拒绝时不得改动原文件");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn write_failure_keeps_original_and_leaves_no_partial_file() {
    let dir = temp_dir("fault");
    let path = dir.join("fault.lumi");
    write_json(&path, &rich_project());
    let original = std::fs::read(&path).unwrap();

    // 故障注入：把临时文件路径占成目录，写入必然失败。
    let temporary = path.with_extension("lumi.tmp");
    std::fs::create_dir(&temporary).unwrap();
    let loaded = project::load(&path).unwrap();
    let error = project::save_with_meta(&path, &loaded).unwrap_err();
    assert!(!format!("{error}").is_empty());

    assert_eq!(std::fs::read(&path).unwrap(), original, "保存失败必须保留原文件");
    assert!(temporary.is_dir(), "失败后不得把半成品留在临时路径");
    // 原文件仍然可读。
    let reread = project::load(&path).unwrap();
    assert_eq!(reread.photos.len(), 2);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn corrupt_file_is_rejected_without_touching_backup() {
    let dir = temp_dir("corrupt");
    let path = dir.join("broken.lumi");
    write_json(&path, &rich_project());
    let backup = path.with_extension("lumi.bak");
    std::fs::copy(&path, &backup).unwrap();

    // 截断成非法 JSON：必须以错误拒绝，而不是默认值加载。
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, &text[..text.len() / 2]).unwrap();
    assert!(project::load(&path).is_err(), "损坏项目不得静默回退默认值");

    // 备份仍可用。
    let recovered = project::load(&backup).unwrap();
    assert_eq!(recovered.photos.len(), 2);
    assert_eq!(recovered.project_uuid, "project-uuid-round91");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn legacy_project_without_identity_gets_uuid_and_keeps_created_at() {
    let dir = temp_dir("legacy");
    let path = dir.join("legacy.lumi");
    let mut legacy = rich_project();
    let object = legacy.as_object_mut().unwrap();
    object.remove("projectUuid");
    object.remove("revision");
    object.remove("updatedAt");
    object.remove("futureDocField");
    write_json(&path, &legacy);

    let loaded = project::load(&path).unwrap();
    assert!(loaded.project_uuid.is_empty());
    assert_eq!(loaded.revision, 0);
    let first = project::save_with_meta(&path, &loaded).unwrap();
    assert!(!first.project_uuid.is_empty(), "首次保存必须生成项目 UUID");
    assert_eq!(first.revision, 1);

    let reloaded = project::load(&path).unwrap();
    let second = project::save_with_meta(&path, &reloaded).unwrap();
    assert_eq!(second.project_uuid, first.project_uuid, "UUID 不得每次保存都变化");
    assert_eq!(second.revision, 2);
    assert_eq!(reloaded.created_at, "2026-11-20T10:00:00.000Z");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn project_version_newer_than_supported_is_rejected() {
    let dir = temp_dir("future");
    let path = dir.join("future.lumi");
    let mut future = rich_project();
    future["version"] = json!(99);
    write_json(&path, &future);
    // 加载阶段就拒绝未知版本，不会静默按旧结构读取。
    assert!(project::load(&path).is_err());
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("\"version\": 99"));
    std::fs::remove_dir_all(&dir).ok();
}

fn scaled_project(photos: usize) -> Value {
    let mut project = rich_project();
    let template = project["photos"][0].clone();
    let mut list = Vec::new();
    for index in 0..photos {
        let mut photo = template.clone();
        let path = format!("/photos/bulk-{index}.jpg");
        photo["path"] = json!(path);
        photo["source"]["path"] = json!(path);
        photo["source"]["hash"] = json!(format!("sha256-{index}"));
        photo["sourceHash"] = json!(format!("sha256-{index}"));
        list.push(photo);
    }
    project["photos"] = Value::Array(list);
    project
}

#[test]
fn save_and_load_performance() {
    use std::time::Instant;
    let dir = temp_dir("perf");
    for (label, photos) in [("small-2", 2usize), ("medium-8", 8), ("large-50", 50)] {
        let path = dir.join(format!("{label}.lumi"));
        write_json(&path, &scaled_project(photos));
        let loaded = project::load(&path).unwrap();
        let save_started = Instant::now();
        project::save_with_meta(&path, &loaded).unwrap();
        let save_ms = save_started.elapsed().as_secs_f64() * 1000.0;
        let load_started = Instant::now();
        let reloaded = project::load(&path).unwrap();
        let load_ms = load_started.elapsed().as_secs_f64() * 1000.0;
        let size = std::fs::metadata(&path).unwrap().len();
        eprintln!(
            "PERF {label}: photos={} bytes={} save={save_ms:.2}ms load={load_ms:.2}ms",
            reloaded.photos.len(),
            size
        );
        assert_eq!(reloaded.photos.len(), photos);
        assert!(save_ms < 3000.0, "{label} 保存耗时异常：{save_ms}ms");
    }
    std::fs::remove_dir_all(&dir).ok();
}
