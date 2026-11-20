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

fn push_point(
    points: &mut Vec<ControlPoint>,
    local: [f32; 2],
    tag: ControlTag,
    weight: f32,
) {
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

/// 构建单张脸的控制网格（未做质量降强度，质量检查在 `quality` 中完成）。
pub fn build(geometry: &FaceGeometry, params: &FaceReshape) -> ControlMesh {
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

/// 带缓存的构建：网格只依赖人脸几何与塑形参数，滑杆重复计算会命中缓存。
pub fn build_cached(geometry: &FaceGeometry, params: &FaceReshape) -> Arc<ControlMesh> {
    let key = format!(
        "{}|{:.4};{:.4};{:.4};{:.4};{:.3};{}",
        MESH_VERSION,
        geometry.origin.0,
        geometry.origin.1,
        geometry.axis_x.0,
        geometry.axis_x.1,
        geometry.eye_distance,
        serde_json::to_string(params).unwrap_or_default()
    );
    cached(key, || build(geometry, params))
}

/// 网格预览：真实控制点与三角形，用于前端“网格显示”。
pub fn preview(face: &FaceFrame, width: u32, height: u32, params: &FaceReshape) -> Option<MeshPreview> {
    let geometry = FaceGeometry::from_face(face, width, height).ok()?;
    let mesh = build_cached(&geometry, params);
    Some(MeshPreview {
        points: mesh.points.iter().map(|point| point.to).collect(),
        triangles: mesh
            .triangles
            .iter()
            .map(|triangle| [triangle[0] as u32, triangle[1] as u32, triangle[2] as u32])
            .collect(),
        dense_landmarks: false,
        source: "yunet-5".to_string(),
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
        }
    }

    #[test]
    fn build_creates_closed_mesh_and_fixed_background() {
        let geometry = FaceGeometry::from_face(&face(), 1000, 1000).unwrap();
        let mesh = build(&geometry, &FaceReshape::new(1));
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
        let mesh = build(&geometry, &params);
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
        let base = build(&geometry, &FaceReshape::new(1));
        let changed = build(
            &geometry,
            &FaceReshape {
                face: FaceShape {
                    jaw: 40.0,
                    ..Default::default()
                },
                ..FaceReshape::new(1)
            },
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
}
