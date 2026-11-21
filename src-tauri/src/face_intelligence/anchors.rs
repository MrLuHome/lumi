//! 从稠密语义区域推导 5 点锚点。
//!
//! 变形引擎的几何基座仍然需要双眼连线、鼻尖与左右嘴角这 5 个锚点。
//! 这里**只按区域语义**取点（眼中心、鼻部最下点、外唇最左/最右点），
//! 不依赖 Apple Vision / ONNX / iBUG 的任何点序号，也不把稠密点伪装成 5 点。

use crate::face_intelligence::types::{FaceResult, RegionKind};

/// 返回 `[右眼, 左眼, 鼻尖, 右嘴角, 左嘴角]`（图像坐标，右眼在图像左侧）。
pub fn five_point_anchors(face: &FaceResult) -> Option<[(f32, f32); 5]> {
    let center = |kind: RegionKind| {
        let region = face.regions.iter().find(|region| region.kind == kind)?;
        if region.points.is_empty() {
            return None;
        }
        let count = region.points.len() as f32;
        Some((
            region.points.iter().map(|point| point.x).sum::<f32>() / count,
            region.points.iter().map(|point| point.y).sum::<f32>() / count,
        ))
    };
    let right_eye = center(RegionKind::RightEye)?;
    let left_eye = center(RegionKind::LeftEye)?;
    let nose = face
        .regions
        .iter()
        .find(|region| region.kind == RegionKind::Nose)?;
    let nose_tip = nose
        .points
        .iter()
        .max_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal))?;
    let lips = face
        .regions
        .iter()
        .find(|region| region.kind == RegionKind::OuterLips)?;
    let right_corner = lips
        .points
        .iter()
        .min_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))?;
    let left_corner = lips
        .points
        .iter()
        .max_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))?;
    Some([
        right_eye,
        left_eye,
        (nose_tip.x, nose_tip.y),
        (right_corner.x, right_corner.y),
        (left_corner.x, left_corner.y),
    ])
}

/// 只有 5 点输出的 Provider 直接复用自身关键点；顺序必须与 FaceFrame 一致。
pub fn sparse_anchors(face: &FaceResult) -> Option<[(f32, f32); 5]> {
    if face.regions.is_empty() && face.landmarks.len() == 5 {
        Some([
            (face.landmarks[0].x, face.landmarks[0].y),
            (face.landmarks[1].x, face.landmarks[1].y),
            (face.landmarks[2].x, face.landmarks[2].y),
            (face.landmarks[3].x, face.landmarks[3].y),
            (face.landmarks[4].x, face.landmarks[4].y),
        ])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::face_intelligence::types::{BoundingBox, DenseRegion, Landmark};

    fn point(x: f32, y: f32) -> Landmark {
        Landmark::new(x, y, 1.0, "x.0")
    }

    fn face_with(regions: Vec<DenseRegion>) -> FaceResult {
        FaceResult {
            face_uuid: "f".into(),
            bounding_box: BoundingBox::default(),
            confidence: 1.0,
            landmarks: Vec::new(),
            regions,
            masks: Vec::new(),
            mesh: None,
        }
    }

    #[test]
    fn anchors_come_from_semantics_not_indices() {
        let face = face_with(vec![
            DenseRegion {
                kind: RegionKind::RightEye,
                points: vec![point(0.30, 0.30), point(0.34, 0.30)],
                closed: true,
            },
            DenseRegion {
                kind: RegionKind::LeftEye,
                points: vec![point(0.60, 0.30), point(0.64, 0.30)],
                closed: true,
            },
            DenseRegion {
                kind: RegionKind::Nose,
                points: vec![point(0.46, 0.40), point(0.48, 0.50)],
                closed: false,
            },
            DenseRegion {
                kind: RegionKind::OuterLips,
                points: vec![point(0.40, 0.60), point(0.60, 0.60)],
                closed: true,
            },
        ]);
        let anchors = five_point_anchors(&face).unwrap();
        assert!((anchors[0].0 - 0.32).abs() < 1e-6);
        assert!((anchors[1].0 - 0.62).abs() < 1e-6);
        assert!((anchors[2].1 - 0.50).abs() < 1e-6, "鼻尖取鼻部最下点");
        assert!(anchors[3].0 < anchors[4].0, "右嘴角在图像左侧");
    }

    #[test]
    fn missing_lips_returns_none() {
        let face = face_with(vec![DenseRegion {
            kind: RegionKind::RightEye,
            points: vec![point(0.3, 0.3)],
            closed: true,
        }]);
        assert!(five_point_anchors(&face).is_none());
    }
}
