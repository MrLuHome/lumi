//! 由 5 点能力层生成可解释的控制网格。
//!
//! 没有稠密 68/106 点关键点时，网格由三层组成：
//! - 脸型轮廓环（框级近似，明确标注 `yunet-5`）；
//! - 双眼/鼻/唇的局部同心环，中心使用真实关键点锚定；
//! - 外圈固定环，负责把背景钉死。
//!
//! 这不是 68 点人脸网格，`dense_landmarks` 永远为 false，UI 会明确提示。

use std::sync::Arc;

use crate::ai_engine::face_frame::{FaceFrame, FaceGeometry};
use crate::face_intelligence::types::{DenseFace, RegionKind};
use crate::face_reshape::mesh::{
    cached, ControlMesh, ControlPoint, ControlTag, MeshPreview, MESH_VERSION,
};
use crate::face_reshape::operations;
use crate::image_engine::params::FaceReshape;

/// 脸型椭圆环：内到外，`weight` 决定该环跟随轮廓位移的比例。
const FACE_RINGS: [(f32, f32, f32, usize); 4] = [
    (0.30, 0.38, 0.0, 12),
    (0.55, 0.72, 0.18, 16),
    (0.78, 1.02, 0.52, 20),
    (0.97, 1.30, 1.0, 24),
];
const FACE_CENTER_V: f32 = 0.18;
const OUTER_RING: (f32, f32, usize) = (1.18, 1.55, 24);

struct RingSpec {
    rx: f32,
    ry: f32,
    weight: f32,
    count: usize,
}

const EYE_RINGS: [RingSpec; 3] = [
    RingSpec {
        rx: 0.46,
        ry: 0.30,
        weight: 0.0,
        count: 12,
    },
    RingSpec {
        rx: 0.30,
        ry: 0.20,
        weight: 0.45,
        count: 12,
    },
    RingSpec {
        rx: 0.14,
        ry: 0.09,
        weight: 1.0,
        count: 10,
    },
];
const NOSE_RINGS: [RingSpec; 3] = [
    RingSpec {
        rx: 0.40,
        ry: 0.34,
        weight: 0.0,
        count: 12,
    },
    RingSpec {
        rx: 0.26,
        ry: 0.24,
        weight: 0.45,
        count: 10,
    },
    RingSpec {
        rx: 0.12,
        ry: 0.12,
        weight: 1.0,
        count: 8,
    },
];
const MOUTH_RINGS: [RingSpec; 3] = [
    RingSpec {
        rx: 0.54,
        ry: 0.26,
        weight: 0.0,
        count: 14,
    },
    RingSpec {
        rx: 0.36,
        ry: 0.17,
        weight: 0.45,
        count: 12,
    },
    RingSpec {
        rx: 0.17,
        ry: 0.09,
        weight: 1.0,
        count: 10,
    },
];

fn to_pair(point: (f32, f32)) -> [f32; 2] {
    [point.0, point.1]
}

/// 保护级别：固定点 > 五官锚点环 > 脸型环。同一位置只保留级别最高的控制点。
fn protection(tag: ControlTag) -> u8 {
    match tag {
        ControlTag::Fixed => 3,
        ControlTag::Eye { .. } | ControlTag::Nose { .. } | ControlTag::Mouth { .. } => 2,
        ControlTag::Face { .. } => 1,
    }
}

/// 真实人脸的鼻子/嘴位置差异很大，脸型环可能与五官环在同一点重合。
/// 重合点若保留两种位移，任何塑形都会让相邻三角形折叠，因此按保护级别焊接。
const WELD_DISTANCE: f32 = 0.01;

fn push_point(points: &mut Vec<ControlPoint>, local: [f32; 2], tag: ControlTag, weight: f32) {
    if let Some(existing) = points.iter_mut().find(|point| {
        (point.local[0] - local[0]).powi(2) + (point.local[1] - local[1]).powi(2)
            < WELD_DISTANCE * WELD_DISTANCE
    }) {
        if protection(tag) > protection(existing.tag) {
            existing.local = local;
            existing.local_to = local;
            existing.tag = tag;
            existing.weight = weight;
        }
        return;
    }
    points.push(ControlPoint {
        local,
        local_to: local,
        from: [0.0, 0.0],
        to: [0.0, 0.0],
        tag,
        weight,
    });
}

fn push_ring(
    points: &mut Vec<ControlPoint>,
    local: [f32; 2],
    rx: f32,
    ry: f32,
    count: usize,
    weight: f32,
    tag: impl Fn(usize) -> ControlTag,
) {
    for index in 0..count {
        let angle = std::f32::consts::TAU * index as f32 / count as f32;
        push_point(
            points,
            [local[0] + rx * angle.cos(), local[1] + ry * angle.sin()],
            tag(index),
            weight,
        );
    }
}

/// 把 Provider 输出的真实稠密轮廓加入控制网格。
///
/// 只有已通过质量门控的稠密数据才会出现在 `FaceFrame.dense` 中；
/// 这里按区域语义（而不是某个模型的点序号）选择控制标签：
/// - 脸缘 → 脸型外环；眼睑 → 对应眼的内环；鼻部 → 鼻内环；外唇 → 唇内环；
/// - 内唇/鼻梁/眉毛 → 固定点，保护牙齿、鼻梁与眉毛不被拖动。
fn push_dense_points(points: &mut Vec<ControlPoint>, geometry: &FaceGeometry, dense: &DenseFace) {
    for region in &dense.regions {
        let (tag, weight) = match region.kind {
            RegionKind::FaceContour => (ControlTag::Face { ring: 3 }, 1.0),
            RegionKind::RightEye => (ControlTag::Eye { side: 0, ring: 2 }, 1.0),
            RegionKind::LeftEye => (ControlTag::Eye { side: 1, ring: 2 }, 1.0),
            RegionKind::Nose => (ControlTag::Nose { ring: 2 }, 1.0),
            RegionKind::OuterLips => (ControlTag::Mouth { ring: 2 }, 1.0),
            RegionKind::InnerLips
            | RegionKind::NoseCrest
            | RegionKind::MedianLine
            | RegionKind::LeftEyebrow
            | RegionKind::RightEyebrow => (ControlTag::Fixed, 0.0),
            _ => continue,
        };
        if region.points.len() < 3 {
            continue;
        }
        let width = geometry.width as f32;
        let height = geometry.height as f32;
        for point in &region.points {
            let (u, v) = geometry.to_local(point.x * width, point.y * height);
            // 侧脸或异常输出可能落到很远的位置，越界点不进网格，避免拉坏背景。
            if !u.is_finite() || !v.is_finite() || u.abs() > 3.0 || v.abs() > 3.0 {
                continue;
            }
            push_point(points, [u, v], tag, weight);
        }
    }
}

/// 构建单张脸的控制网格（未做质量降强度，质量检查在 `quality` 中完成）。
pub fn build(
    geometry: &FaceGeometry,
    params: &FaceReshape,
    dense: Option<&DenseFace>,
) -> ControlMesh {
    let mut points: Vec<ControlPoint> = Vec::with_capacity(180);

    // 脸型环与背景外圈。
    for (ring, (rx, ry, weight, count)) in FACE_RINGS.iter().enumerate() {
        push_ring(
            &mut points,
            [0.0, FACE_CENTER_V],
            *rx,
            *ry,
            *count,
            *weight,
            |_| ControlTag::Face { ring },
        );
    }
    push_ring(
        &mut points,
        [0.0, FACE_CENTER_V],
        OUTER_RING.0,
        OUTER_RING.1,
        OUTER_RING.2,
        0.0,
        |_| ControlTag::Fixed,
    );
    push_point(&mut points, [0.0, FACE_CENTER_V], ControlTag::Fixed, 0.0);

    // 五官锚点与局部环。中心全部来自 YuNet 的真实关键点。
    let eyes = [
        to_pair(geometry.to_local(geometry.eyes[0].0, geometry.eyes[0].1)),
        to_pair(geometry.to_local(geometry.eyes[1].0, geometry.eyes[1].1)),
    ];
    let nose = to_pair(geometry.to_local(geometry.nose.0, geometry.nose.1));
    let mouth = [
        to_pair(geometry.to_local(geometry.mouth[0].0, geometry.mouth[0].1)),
        to_pair(geometry.to_local(geometry.mouth[1].0, geometry.mouth[1].1)),
    ];
    let mouth_center = [
        (mouth[0][0] + mouth[1][0]) * 0.5,
        (mouth[0][1] + mouth[1][1]) * 0.5,
    ];

    for (side, center) in eyes.iter().enumerate() {
        for (ring, spec) in EYE_RINGS.iter().enumerate() {
            push_ring(
                &mut points,
                *center,
                spec.rx,
                spec.ry,
                spec.count,
                spec.weight,
                |_| ControlTag::Eye {
                    side: side as u8,
                    ring,
                },
            );
        }
        push_point(&mut points, *center, ControlTag::Fixed, 0.0);
    }

    for (ring, spec) in NOSE_RINGS.iter().enumerate() {
        push_ring(
            &mut points,
            nose,
            spec.rx,
            spec.ry,
            spec.count,
            spec.weight,
            |_| ControlTag::Nose { ring },
        );
    }
    push_point(&mut points, nose, ControlTag::Fixed, 0.0);

    for (ring, spec) in MOUTH_RINGS.iter().enumerate() {
        push_ring(
            &mut points,
            mouth_center,
            spec.rx,
            spec.ry,
            spec.count,
            spec.weight,
            |_| ControlTag::Mouth { ring },
        );
    }
    push_point(&mut points, mouth_center, ControlTag::Fixed, 0.0);

    if let Some(dense) = dense {
        push_dense_points(&mut points, geometry, dense);
    }

    // 参数 → 局部位移。
    operations::face::apply(&mut points, &params.face);
    operations::eyes::apply(&mut points, &params.eyes, &eyes);
    operations::nose::apply(&mut points, &params.nose, nose);
    operations::mouth::apply(&mut points, &params.mouth, mouth_center);
    operations::chin::apply(&mut points, &params.chin);

    // 局部坐标 → 原图像素。瞳孔/鼻尖/嘴角锚点保持 from == to。
    for point in points.iter_mut() {
        point.from = to_pair(geometry.from_local(point.local[0], point.local[1]));
        point.to = to_pair(geometry.from_local(point.local_to[0], point.local_to[1]));
    }

    let (mut min_x, mut min_y) = (f32::INFINITY, f32::INFINITY);
    let (mut max_x, mut max_y) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
    for point in &points {
        min_x = min_x.min(point.from[0]).min(point.to[0]);
        min_y = min_y.min(point.from[1]).min(point.to[1]);
        max_x = max_x.max(point.from[0]).max(point.to[0]);
        max_y = max_y.max(point.from[1]).max(point.to[1]);
    }

    let from: Vec<[f32; 2]> = points.iter().map(|point| point.from).collect();
    let to: Vec<[f32; 2]> = points.iter().map(|point| point.to).collect();
    let mut mesh = ControlMesh {
        points,
        triangles: Vec::new(),
        eye_distance: geometry.eye_distance,
        bbox: [min_x, min_y, max_x, max_y],
        version: MESH_VERSION,
        signature: ControlMesh::signature_of(&from, &to),
    };
    mesh.rebuild_triangles();
    mesh
}

/// 带缓存的构建：网格只依赖人脸几何、稠密轮廓与塑形参数，滑杆重复计算会命中缓存。
pub fn build_cached(
    geometry: &FaceGeometry,
    params: &FaceReshape,
    dense: Option<&DenseFace>,
) -> Arc<ControlMesh> {
    let key = format!(
        "{}|{:.4};{:.4};{:.4};{:.4};{:.3};{}|{}",
        MESH_VERSION,
        geometry.origin.0,
        geometry.origin.1,
        geometry.axis_x.0,
        geometry.axis_x.1,
        geometry.eye_distance,
        serde_json::to_string(params).unwrap_or_default(),
        dense.map(DenseFace::fingerprint).unwrap_or_default()
    );
    cached(key, || build(geometry, params, dense))
}

/// 网格预览：真实控制点与三角形，用于前端“网格显示”。
pub fn preview(
    face: &FaceFrame,
    width: u32,
    height: u32,
    params: &FaceReshape,
) -> Option<MeshPreview> {
    let geometry = FaceGeometry::from_face(face, width, height).ok()?;
    let dense = face.dense.as_ref();
    let mesh = build_cached(&geometry, params, dense);
    let dense_unlocked = dense.is_some()
        && face
            .level3
            .as_ref()
            .map(|gate| gate.any_enabled())
            .unwrap_or(false);
    Some(MeshPreview {
        points: mesh.points.iter().map(|point| point.to).collect(),
        triangles: mesh
            .triangles
            .iter()
            .map(|triangle| [triangle[0] as u32, triangle[1] as u32, triangle[2] as u32])
            .collect(),
        dense_landmarks: dense_unlocked,
        source: if dense_unlocked {
            dense
                .map(|value| value.provider_id.clone())
                .unwrap_or_else(|| "yunet-5".into())
        } else {
            "yunet-5".to_string()
        },
        version: mesh.version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_engine::face_frame::{FaceFrame, NormPoint};
    use crate::image_engine::params::{FaceReshape, FaceShape};

    fn face() -> FaceFrame {
        FaceFrame {
            id: 1,
            uuid: String::new(),
            score: 0.95,
            x: 0.3,
            y: 0.25,
            width: 0.4,
            height: 0.4,
            landmarks: vec![
                NormPoint { x: 0.44, y: 0.40 },
                NormPoint { x: 0.56, y: 0.40 },
                NormPoint { x: 0.50, y: 0.50 },
                NormPoint { x: 0.44, y: 0.58 },
                NormPoint { x: 0.56, y: 0.58 },
            ],
            dense: None,
            level3: None,
        }
    }

    #[test]
    fn build_creates_closed_mesh_and_fixed_background() {
        let geometry = FaceGeometry::from_face(&face(), 1000, 1000).unwrap();
        let mesh = build(&geometry, &FaceReshape::new(1), None);
        assert!(mesh.points.len() > 90);
        assert!(mesh.triangles.len() > 100);
        assert_eq!(mesh.flipped_triangles(), 0);
        assert!(mesh.max_displacement() < 0.001, "默认参数不能改变任何点");
        let fixed = mesh
            .points
            .iter()
            .filter(|point| matches!(point.tag, ControlTag::Fixed))
            .count();
        assert!(fixed > 20, "必须保留足够的固定锚点保护背景");
    }

    #[test]
    fn slim_moves_outer_ring_inward_only() {
        let geometry = FaceGeometry::from_face(&face(), 1000, 1000).unwrap();
        let params = FaceReshape {
            face: FaceShape {
                slim: 80.0,
                ..Default::default()
            },
            ..FaceReshape::new(1)
        };
        let mesh = build(&geometry, &params, None);
        assert!(mesh.max_displacement() > 1.0);
        assert_eq!(mesh.flipped_triangles(), 0);
        for point in &mesh.points {
            if matches!(point.tag, ControlTag::Fixed) {
                assert_eq!(point.from, point.to);
            }
            // 眼睛中心、鼻尖、嘴角必须钉住。
            if matches!(point.tag, ControlTag::Face { .. }) && point.weight == 0.0 {
                assert!(point.displacement() < 0.01);
            }
        }
    }

    #[test]
    fn signature_changes_with_parameters() {
        let geometry = FaceGeometry::from_face(&face(), 1000, 1000).unwrap();
        let base = build(&geometry, &FaceReshape::new(1), None);
        let changed = build(
            &geometry,
            &FaceReshape {
                face: FaceShape {
                    jaw: 40.0,
                    ..Default::default()
                },
                ..FaceReshape::new(1)
            },
            None,
        );
        assert_ne!(base.signature, changed.signature);
    }

    #[test]
    fn preview_reports_5_point_capability() {
        let preview = preview(&face(), 1000, 1000, &FaceReshape::new(1)).unwrap();
        assert!(!preview.dense_landmarks);
        assert_eq!(preview.source, "yunet-5");
        assert!(!preview.triangles.is_empty());
    }

    /// 回归：真实近景人像里鼻尖可能低到让鼻部环与脸型环重合。
    /// 重合点在焊接前会让瘦脸把相邻三角形翻折，安全策略随即把强度降为 0。
    #[test]
    fn coincident_feature_and_face_rings_are_welded_before_deformation() {
        let mut sample = face();
        // 鼻部环 1 的底部与脸型环 2 的底部在局部坐标 (0, 1.2) 重合。
        sample.landmarks[2] = NormPoint {
            x: 0.5,
            y: 0.5152,
        };
        let geometry = FaceGeometry::from_face(&sample, 1000, 1000).unwrap();
        let params = FaceReshape {
            face: FaceShape {
                slim: 50.0,
                ..Default::default()
            },
            ..FaceReshape::new(1)
        };
        let mut mesh = build(&geometry, &params, None);

        for (index, first) in mesh.points.iter().enumerate() {
            for second in mesh.points.iter().skip(index + 1) {
                let distance = ((first.local[0] - second.local[0]).powi(2)
                    + (first.local[1] - second.local[1]).powi(2))
                .sqrt();
                assert!(
                    distance >= WELD_DISTANCE,
                    "控制点未焊接：{distance} < {WELD_DISTANCE}"
                );
            }
        }
        assert_eq!(mesh.flipped_triangles(), 0, "焊接后不应再有翻折");
        assert!(mesh.max_displacement() > 1.0);

        let quality = crate::face_reshape::quality::stabilize(
            &mut mesh,
            crate::face_reshape::quality::MAX_FACE_DISPLACEMENT_RATIO,
        );
        assert!(
            quality.applied_scale > 0.0,
            "中等强度瘦脸不应被完全回退：{:?}",
            quality.notes
        );
        assert_eq!(quality.flipped_triangles, 0);
    }
}
