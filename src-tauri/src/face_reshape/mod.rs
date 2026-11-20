//! 第四轮：面部塑形引擎。
//!
//! 管线位置：美肤之后、调色之前。输入是已经完成美肤的图像、稳定人脸 ID、
//! 5 点关键点与每张脸的塑形参数；输出是真实改变像素的结果 + 质量报告。
//!
//! 能力边界（重要）：
//! - 没有打包任何 68/106 点稠密关键点权重；
//! - 网格 = 脸型轮廓环（框级近似）+ 真实锚点（双眼/鼻尖/嘴角）；
//! - 眼睑、唇轮廓、鼻翼等需要稠密轮廓的参数在 UI 中禁用并提示；
//! - 所有变形限制在脸部椭圆内，外圈控制点固定，背景不会被弯曲。

pub mod boundary;
pub mod blending;
pub mod deformation;
pub mod face_mesh;
pub mod identity;
pub mod landmark;
pub mod landmarker;
pub mod mask;
pub mod mesh;
pub mod operations;
pub mod presets;
pub mod quality;
pub mod triangulation;
pub mod warp;

use std::time::Instant;

use serde::Serialize;

use crate::ai_engine::face_frame::FaceFrame;
use crate::error::AppResult;
use crate::image_engine::buffer::Image16;
use crate::image_engine::cancel::CancelToken;
use crate::image_engine::params::FaceReshape;

use self::boundary::AlphaRegion;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisabledParam {
    pub key: &'static str,
    pub reason: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReshapeCapability {
    pub source: &'static str,
    pub dense_landmarks: bool,
    pub supported: Vec<&'static str>,
    pub disabled: Vec<DisabledParam>,
    pub note: &'static str,
}

/// 当前真实能力。UI 与报告都使用这一份描述，避免“面板有滑杆但算法没实现”。
pub fn capability() -> ReshapeCapability {
    ReshapeCapability {
        source: "yunet-5",
        dense_landmarks: false,
        supported: vec![
            "face.slim",
            "face.left",
            "face.right",
            "face.jaw",
            "face.cheekbone",
            "face.width",
            "face.length",
            "face.balance",
            "manual.push",
            "manual.pull",
            "manual.zoomIn",
            "manual.zoomOut",
            "manual.protect",
            "manual.restore",
        ],
        disabled: vec![
            DisabledParam {
                key: "eyes.*",
                reason: "LEVEL 3：需要 478/106 点稠密眼睑轮廓，当前不开放",
            },
            DisabledParam {
                key: "eyes.corner",
                reason: "眼角需要 106 点眼睑轮廓，当前 5 点模型不开放",
            },
            DisabledParam {
                key: "nose.*",
                reason: "LEVEL 3：鼻梁/鼻翼需要稠密关键点或商业 SDK",
            },
            DisabledParam {
                key: "nose.alar",
                reason: "鼻翼宽度需要鼻孔关键点，当前不开放",
            },
            DisabledParam {
                key: "nose.height",
                reason: "鼻梁视觉高度属于光影修饰，本轮不做几何变形",
            },
            DisabledParam {
                key: "mouth.*",
                reason: "LEVEL 3：唇线与嘴角需要稠密唇轮廓，当前不开放",
            },
            DisabledParam {
                key: "mouth.upper",
                reason: "上唇厚度需要上下唇轮廓线",
            },
            DisabledParam {
                key: "mouth.lower",
                reason: "下唇厚度需要上下唇轮廓线",
            },
            DisabledParam {
                key: "mouth.symmetry",
                reason: "唇形对称性需要完整唇轮廓",
            },
            DisabledParam {
                key: "chin.*",
                reason: "LEVEL 3：独立下巴塑形需要下巴轮廓链，本轮仅开放下颌线整体调整",
            },
            DisabledParam {
                key: "chin.point",
                reason: "下巴尖度需要下巴轮廓链，当前只开放长度/宽度/下颌线",
            },
        ],
        note: "正式交付 LEVEL 1 手动变形 + LEVEL 2 轮廓级保守调整；LEVEL 3 稠密五官精修未启用。",
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceReshapeReport {
    pub face_id: u32,
    pub source: String,
    pub dense_landmarks: bool,
    pub confidence: f32,
    pub auto_scale: f32,
    pub control_points: usize,
    pub triangles: usize,
    pub flipped_triangles: usize,
    pub min_area_ratio: f32,
    pub max_area_ratio: f32,
    pub max_displacement_ratio: f32,
    pub applied_scale: f32,
    pub degraded: bool,
    pub mesh_version: u32,
    pub warped_pixels: u32,
    pub elapsed_ms: f32,
    pub notes: Vec<String>,
}

struct Candidate {
    face_id: u32,
    landmarks: landmark::LandmarkSet,
    mesh: mesh::ControlMesh,
    quality: quality::QualityReport,
    region: AlphaRegion,
    params: FaceReshape,
    elapsed: f32,
}

/// LEVEL 3 稠密五官塑形当前不交付。即使项目文件里带有这些参数，
/// 渲染时也会被清零，避免“接口存在 = 功能已完成”的误判。
pub const LEVEL3_ENABLED: bool = false;

fn sanitize(entry: &FaceReshape) -> FaceReshape {
    let mut output = entry.clone();
    if !LEVEL3_ENABLED {
        output.eyes = Default::default();
        output.nose = Default::default();
        output.mouth = Default::default();
        output.chin = Default::default();
    }
    output
}

/// 对一张图里的所有人脸依次塑形。每张脸只改自己的椭圆区域，
/// 参数按稳定 face_uuid 匹配（旧数据回退 face_id）；检测顺序变化不会错配。
pub fn apply_face_reshape(
    image: &mut Image16,
    faces: &[FaceFrame],
    params: &[FaceReshape],
    cancel: &CancelToken,
) -> AppResult<Vec<FaceReshapeReport>> {
    let mut reports = Vec::new();
    if params.is_empty() {
        return Ok(reports);
    }

    let mut candidates: Vec<Candidate> = Vec::new();
    for face in faces {
        let Some(entry) = params.iter().find(|entry| {
            (!entry.face_uuid.is_empty() && entry.face_uuid == face.uuid)
                || entry.face_id == face.id
        }) else {
            continue;
        };
        if !entry.enabled || !entry.is_active() {
            continue;
        }
        let entry = sanitize(entry);
        cancel.check()?;
        let started = Instant::now();
        let landmarks = match landmark::analyze(face, image.width(), image.height()) {
            Ok(value) => value,
            Err(error) => {
                reports.push(FaceReshapeReport {
                    face_id: face.id,
                    source: "yunet-5".into(),
                    dense_landmarks: false,
                    confidence: 0.0,
                    auto_scale: 0.0,
                    control_points: 0,
                    triangles: 0,
                    flipped_triangles: 0,
                    min_area_ratio: 1.0,
                    max_area_ratio: 1.0,
                    max_displacement_ratio: 0.0,
                    applied_scale: 0.0,
                    degraded: true,
                    mesh_version: mesh::MESH_VERSION,
                    warped_pixels: 0,
                    elapsed_ms: started.elapsed().as_secs_f32() * 1000.0,
                    notes: vec![error.to_string()],
                });
                continue;
            }
        };

        let mut mesh = (*face_mesh::build_cached(&landmarks.geometry, &entry)).clone();
        // 低置信度先整体降强度，再做质量检查，避免“勉强可用”的极端参数。
        deformation::scale_targets(&mut mesh.points, landmarks.auto_scale);
        let quality = quality::stabilize(&mut mesh, quality::MAX_FACE_DISPLACEMENT_RATIO);
        let Some(region) = boundary::build_region(&landmarks.geometry, image.width(), image.height()) else {
            continue;
        };
        candidates.push(Candidate {
            face_id: face.id,
            landmarks,
            mesh,
            quality,
            region,
            params: entry,
            elapsed: started.elapsed().as_secs_f32() * 1000.0,
        });
    }

    if candidates.is_empty() {
        return Ok(reports);
    }

    let mut regions: Vec<AlphaRegion> = candidates.iter().map(|entry| entry.region.clone()).collect();
    boundary::apply_ownership(&mut regions);

    for (candidate, region) in candidates.into_iter().zip(regions.into_iter()) {
        cancel.check()?;
        let warp_started = Instant::now();
        let rect = region.rect;
        let original_roi = copy_roi(image, rect);
        let mut warped_pixels = 0u32;
        let mut extra_notes = Vec::new();

        if candidate.mesh.max_displacement() > 0.01 {
            let stats = warp::warp_mesh(image, &candidate.mesh, &region, cancel)?;
            warped_pixels += stats.pixels;
        }

        if candidate.params.manual.enabled && !candidate.params.manual.strokes.is_empty() {
            if let Some(effect) = operations::manual::build(
                &candidate.params.manual.strokes,
                image.width(),
                image.height(),
                candidate.landmarks.geometry.eye_distance,
            ) {
                let stats = operations::manual::apply(
                    image,
                    &effect,
                    &region,
                    &original_roi,
                    cancel,
                )?;
                warped_pixels += stats.pixels;
                extra_notes.push(format!(
                    "手动笔触 {} 笔，最大位移 {:.1}px",
                    candidate.params.manual.strokes.len(),
                    stats.max_displacement
                ));
            }
        }

        let mut notes = candidate.landmarks.notes.clone();
        notes.extend(candidate.quality.notes.iter().cloned());
        notes.extend(extra_notes);
        if candidate.quality.applied_scale <= 0.0 && warped_pixels == 0 {
            notes.push("参数超出安全范围，本次未改变像素".into());
        }

        reports.push(FaceReshapeReport {
            face_id: candidate.face_id,
            source: candidate.landmarks.source.id().to_string(),
            dense_landmarks: candidate.landmarks.dense(),
            confidence: candidate.landmarks.confidence,
            auto_scale: candidate.landmarks.auto_scale,
            control_points: candidate.mesh.points.len(),
            triangles: candidate.mesh.triangles.len(),
            flipped_triangles: candidate.quality.flipped_triangles,
            min_area_ratio: candidate.quality.min_area_ratio,
            max_area_ratio: candidate.quality.max_area_ratio,
            max_displacement_ratio: candidate.quality.max_displacement_ratio,
            applied_scale: candidate.quality.applied_scale,
            degraded: candidate.quality.degraded || candidate.landmarks.auto_scale < 0.999,
            mesh_version: candidate.mesh.version,
            warped_pixels,
            elapsed_ms: candidate.elapsed + warp_started.elapsed().as_secs_f32() * 1000.0,
            notes,
        });
    }

    Ok(reports)
}

fn copy_roi(image: &Image16, rect: boundary::Rect) -> Vec<u16> {
    let mut output = Vec::with_capacity(rect.width as usize * rect.height as usize * 4);
    for y in rect.y0..rect.y0 + rect.height as i32 {
        for x in rect.x0..rect.x0 + rect.width as i32 {
            output.extend_from_slice(&image.get_pixel(x as u32, y as u32).0);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_engine::face_frame::NormPoint;
    use crate::image_engine::params::{FaceReshape, FaceShape, WarpMode, WarpStroke};
    use crate::image_engine::params::MaskPoint;
    use image::Rgba;

    fn face(id: u32, center_x: f32) -> FaceFrame {
        FaceFrame {
            id,
            uuid: String::new(),
            score: 0.95,
            x: center_x - 0.18,
            y: 0.25,
            width: 0.36,
            height: 0.36,
            landmarks: vec![
                NormPoint {
                    x: center_x - 0.06,
                    y: 0.40,
                },
                NormPoint {
                    x: center_x + 0.06,
                    y: 0.40,
                },
                NormPoint {
                    x: center_x,
                    y: 0.50,
                },
                NormPoint {
                    x: center_x - 0.05,
                    y: 0.58,
                },
                NormPoint {
                    x: center_x + 0.05,
                    y: 0.58,
                },
            ],
        }
    }

    fn photo() -> Image16 {
        Image16::from_fn(240, 240, |x, y| {
            Rgba([
                (x * 180) as u16,
                (y * 180) as u16,
                30_000,
                65535,
            ])
        })
    }

    #[test]
    fn slim_really_changes_pixels_inside_face() {
        let mut image = photo();
        let before = image.clone();
        let params = vec![FaceReshape {
            face: FaceShape {
                slim: 100.0,
                jaw: 80.0,
                ..Default::default()
            },
            ..FaceReshape::new(1)
        }];
        let reports = apply_face_reshape(
            &mut image,
            &[face(1, 0.5)],
            &params,
            &CancelToken::default(),
        )
        .unwrap();
        assert_eq!(reports.len(), 1);
        assert!(reports[0].warped_pixels > 100);
        assert_eq!(reports[0].source, "yunet-5");
        assert!(!reports[0].dense_landmarks);
        let changed = image
            .pixels()
            .zip(before.pixels())
            .filter(|(a, b)| a != b)
            .count();
        assert!(changed > 200);
    }

    #[test]
    fn background_pixels_are_untouched() {
        let mut image = photo();
        let before = image.clone();
        let params = vec![FaceReshape {
            face: FaceShape {
                slim: 100.0,
                length: 100.0,
                ..Default::default()
            },
            ..FaceReshape::new(1)
        }];
        apply_face_reshape(&mut image, &[face(1, 0.5)], &params, &CancelToken::default())
            .unwrap();
        for y in 0..240u32 {
            for x in 0..240u32 {
                if x < 20 || y < 20 || x > 220 || y > 220 {
                    assert_eq!(image.get_pixel(x, y), before.get_pixel(x, y));
                }
            }
        }
    }

    #[test]
    fn manual_push_moves_pixels_and_restore_works() {
        let mut image = photo();
        let before = image.clone();
        let params = vec![FaceReshape {
            manual: crate::image_engine::params::ManualWarp {
                enabled: true,
                strokes: vec![WarpStroke {
                    mode: WarpMode::Push,
                    radius: 0.06,
                    strength: 100.0,
                    feather: 80.0,
                    points: vec![
                        MaskPoint { x: 0.45, y: 0.5 },
                        MaskPoint { x: 0.55, y: 0.5 },
                    ],
                }],
            },
            ..FaceReshape::new(1)
        }];
        let reports = apply_face_reshape(
            &mut image,
            &[face(1, 0.5)],
            &params,
            &CancelToken::default(),
        )
        .unwrap();
        assert!(reports[0].warped_pixels > 50);
        assert_ne!(image.get_pixel(120, 120), before.get_pixel(120, 120));
    }

    #[test]
    fn inactive_or_disabled_params_are_noops() {
        let mut image = photo();
        let before = image.clone();
        let mut params = vec![FaceReshape {
            face: FaceShape {
                slim: 100.0,
                ..Default::default()
            },
            ..FaceReshape::new(1)
        }];
        params[0].enabled = false;
        let reports = apply_face_reshape(&mut image, &[face(1, 0.5)], &params, &CancelToken::default())
            .unwrap();
        assert!(reports.is_empty());
        assert_eq!(image.into_raw(), before.into_raw());
    }

    #[test]
    fn cap_scale_is_full_for_good_face() {
        let report = capability();
        assert!(!report.dense_landmarks);
        assert!(report.disabled.iter().any(|entry| entry.key == "mouth.upper"));
        assert!(!report.supported.is_empty());
    }
}
