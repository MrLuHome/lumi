//! 关键点能力层与置信度评估。
//!
//! 当前只接入 YuNet 5 点（双眼、鼻尖、左右嘴角）。这里不伪造 68 点：
//! `source` 与 `dense` 字段会一路传到报告和前端，明确告诉用户网格是
//! 框级 + 锚点级近似，不是稠密五官轮廓。
//!
//! 低置信度（遮挡、侧脸、模糊）不会直接放弃，而是返回 `auto_scale < 1`，
//! 自动把塑形强度降到安全范围，并在报告里提示。

use crate::ai_engine::face_frame::{FaceFrame, FaceGeometry};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LandmarkSource {
    Yunet5,
}

impl LandmarkSource {
    pub fn id(&self) -> &'static str {
        match self {
            LandmarkSource::Yunet5 => "yunet-5",
        }
    }

    pub fn is_dense(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone)]
pub struct LandmarkSet {
    pub source: LandmarkSource,
    pub geometry: FaceGeometry,
    pub confidence: f32,
    /// 质量不足时用于自动降强度（0..1）。
    pub auto_scale: f32,
    pub notes: Vec<String>,
}

impl LandmarkSet {
    pub fn dense(&self) -> bool {
        self.source.is_dense()
    }
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// 解析并验证 5 点关键点。几何关系异常时返回明确错误，不进入网格变形。
pub fn analyze(face: &FaceFrame, image_width: u32, image_height: u32) -> AppResult<LandmarkSet> {
    let geometry = FaceGeometry::from_face(face, image_width, image_height)?;
    let eye_distance = geometry.eye_distance;
    if !eye_distance.is_finite() || eye_distance < 12.0 {
        return Err(AppError::Unsupported(
            "人脸双眼距离过小，无法建立塑形网格".into(),
        ));
    }

    let nose_point = geometry.to_local(geometry.nose.0, geometry.nose.1);
    let nose = [nose_point.0, nose_point.1];
    let mouth_left_point = geometry.to_local(geometry.mouth[0].0, geometry.mouth[0].1);
    let mouth_left = [mouth_left_point.0, mouth_left_point.1];
    let mouth_right_point = geometry.to_local(geometry.mouth[1].0, geometry.mouth[1].1);
    let mouth_right = [mouth_right_point.0, mouth_right_point.1];
    let mouth_center = [
        (mouth_left[0] + mouth_right[0]) * 0.5,
        (mouth_left[1] + mouth_right[1]) * 0.5,
    ];
    let mouth_width = mouth_left[0] - mouth_right[0];

    let mut checks = 0.0;
    let mut total = 0.0;
    let mut notes = Vec::new();

    total += 1.0;
    if (0.05..1.25).contains(&nose[1]) {
        checks += 1.0;
    } else {
        notes.push("鼻尖位置异常，可能为侧脸或遮挡".into());
    }
    total += 1.0;
    if (0.55..1.95).contains(&mouth_center[1]) {
        checks += 1.0;
    } else {
        notes.push("嘴角位置异常，已降低自动强度".into());
    }
    total += 1.0;
    if mouth_width.abs() > 0.25 && mouth_width.abs() < 2.4 {
        checks += 1.0;
    } else {
        notes.push("嘴宽比例异常，已降低自动强度".into());
    }
    total += 1.0;
    if nose[0].abs() < 0.40 {
        checks += 1.0;
    } else {
        notes.push("鼻尖偏离面部中线，可能为侧脸".into());
    }

    let geometric = checks / total;
    let score = face.score.clamp(0.0, 1.0);
    let confidence = (score * 0.55 + geometric * 0.45).clamp(0.0, 1.0);
    let auto_scale = if confidence >= 0.65 {
        1.0
    } else {
        smoothstep(0.22, 0.65, confidence).clamp(0.25, 1.0)
    };
    if confidence < 0.60 {
        notes.push("关键点置信度偏低，本次塑形自动降强度".into());
    }

    Ok(LandmarkSet {
        source: LandmarkSource::Yunet5,
        geometry,
        confidence,
        auto_scale,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_engine::face_frame::NormPoint;

    fn face(score: f32, nose_x: f32, nose_y: f32) -> FaceFrame {
        FaceFrame {
            id: 1,
            uuid: String::new(),
            score,
            x: 0.3,
            y: 0.25,
            width: 0.4,
            height: 0.4,
            landmarks: vec![
                NormPoint { x: 0.44, y: 0.40 },
                NormPoint { x: 0.56, y: 0.40 },
                NormPoint { x: nose_x, y: nose_y },
                NormPoint { x: 0.44, y: 0.58 },
                NormPoint { x: 0.56, y: 0.58 },
            ],
        }
    }

    #[test]
    fn normal_face_has_full_strength() {
        let set = analyze(&face(0.95, 0.50, 0.50), 1000, 1000).unwrap();
        assert!(!set.dense());
        assert!(set.confidence > 0.8);
        assert_eq!(set.auto_scale, 1.0);
        assert!(set.notes.is_empty());
    }

    #[test]
    fn low_score_reduces_strength() {
        let set = analyze(&face(0.2, 0.50, 0.50), 1000, 1000).unwrap();
        assert!(set.auto_scale < 0.9);
        assert!(set.notes.iter().any(|note| note.contains("置信度")));
    }

    #[test]
    fn abnormal_geometry_is_flagged() {
        let set = analyze(&face(0.6, 0.75, 0.95), 1000, 1000).unwrap();
        assert!(!set.notes.is_empty());
        assert!(set.auto_scale < 1.0);
    }

    #[test]
    fn too_small_face_is_rejected() {
        let mut face = face(0.9, 0.50, 0.50);
        for point in face.landmarks.iter_mut() {
            point.x = 0.5 + (point.x - 0.5) * 0.02;
        }
        assert!(analyze(&face, 4000, 4000).is_err());
    }
}
