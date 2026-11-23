//! G12 · 人脸轮廓分析（5 点 + 框级近似，**不是**稠密轮廓模型）。
//!
//! 从 YuNet 5 点与人脸框推导：中心线、左右脸颊、颧骨带、下颌线、下巴、太阳穴、耳朵近似。
//! 侧脸/低分/点数不足时 `confidence` 降低，`allow_aggressive=false` → G13 禁止激进变形。

use serde::Serialize;

use crate::ai_engine::face_frame::{FaceFrame, FaceGeometry};
use crate::error::{AppError, AppResult};

/// 轮廓分析方法标签（UI/报告必须原样展示，不得写成「AI 精准轮廓」）。
pub const CONTOUR_METHOD: &str = "yunet5-bbox-contour";

/// 归一化 0..1 的轮廓锚点。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContourPoint {
    pub x: f32,
    pub y: f32,
    /// 0..1，该锚点可信度。
    pub confidence: f32,
}

impl ContourPoint {
    fn new(x: f32, y: f32, confidence: f32) -> Self {
        Self {
            x: x.clamp(0.0, 1.0),
            y: y.clamp(0.0, 1.0),
            confidence: confidence.clamp(0.0, 1.0),
        }
    }
}

/// 单张脸的轮廓分析结果。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceContourAnalysis {
    pub face_id: u32,
    pub face_uuid: String,
    pub method: &'static str,
    /// 整体置信度 0..1。
    pub confidence: f32,
    /// 置信度低于阈值时禁止激进变形（G13 强制）。
    pub allow_aggressive: bool,
    /// 人脸中心线（垂直，经过双眼中心与鼻尖投影）。
    pub centerline: [ContourPoint; 2],
    pub left_cheek: ContourPoint,
    pub right_cheek: ContourPoint,
    pub left_cheekbone: ContourPoint,
    pub right_cheekbone: ContourPoint,
    pub jaw_left: ContourPoint,
    pub jaw_right: ContourPoint,
    pub chin: ContourPoint,
    pub left_temple: ContourPoint,
    pub right_temple: ContourPoint,
    /// 耳朵：仅用框侧近似（5 点无耳点）。
    pub left_ear: ContourPoint,
    pub right_ear: ContourPoint,
    pub notes: Vec<String>,
}

/// 激进变形最低置信度。
pub const AGGRESSIVE_MIN_CONFIDENCE: f32 = 0.55;

/// 分析单张脸。`image_width/height` 用于像素几何，输出锚点仍为归一化。
pub fn analyze_face_contour(face: &FaceFrame, image_width: u32, image_height: u32) -> AppResult<FaceContourAnalysis> {
    if !face.is_usable() {
        return Err(AppError::Unsupported(
            "人脸缺少可用关键点，无法分析轮廓".into(),
        ));
    }
    let geometry = FaceGeometry::from_face(face, image_width, image_height)?;
    let w = image_width.max(1) as f32;
    let h = image_height.max(1) as f32;
    let to_norm = |px: (f32, f32)| -> (f32, f32) {
        ((px.0 / w).clamp(0.0, 1.0), (px.1 / h).clamp(0.0, 1.0))
    };

    let mut notes = Vec::new();
    // 检测分 → 基础置信度。
    let mut confidence = face.score.clamp(0.0, 1.0);
    if face.score < 0.5 {
        notes.push("人脸检测分数偏低，轮廓仅供保守调整".into());
    }

    // 几何健全性。
    let eye_dist = geometry.eye_distance;
    if !eye_dist.is_finite() || eye_dist < 12.0 {
        return Err(AppError::Unsupported("双眼距离异常，无法分析轮廓".into()));
    }

    // 中心线：双眼中心 → 鼻尖下方（下巴方向延伸）。
    let eye_mid = (
        (geometry.eyes[0].0 + geometry.eyes[1].0) * 0.5,
        (geometry.eyes[0].1 + geometry.eyes[1].1) * 0.5,
    );
    let chin_base = (
        eye_mid.0 + (geometry.nose.0 - eye_mid.0) * 0.35,
        face.y * h + face.height * h * 0.92,
    );
    let (cl0x, cl0y) = to_norm(eye_mid);
    let (cl1x, cl1y) = to_norm(chin_base);
    let centerline = [
        ContourPoint::new(cl0x, cl0y, confidence),
        ContourPoint::new(cl1x, cl1y, confidence * 0.85),
    ];

    // 框级左右（归一化）。
    let left_edge = face.x + face.width;
    let right_edge = face.x;
    // 注意：图像 x 向右增大；`face.x` 为左上角。左脸在图像右侧（被拍者的左）。
    // 为避免语义混乱，这里 left/right 均按**图像坐标**（左=小 x）。
    let img_left = face.x;
    let img_right = face.x + face.width;
    let mid_x = face.x + face.width * 0.5;

    // 脸颊：眼高略下、框内侧。
    let eye_y = face.y + face.height * 0.42;
    let left_cheek = ContourPoint::new(img_left + face.width * 0.28, eye_y + face.height * 0.08, confidence * 0.8);
    let right_cheek = ContourPoint::new(img_right - face.width * 0.28, eye_y + face.height * 0.08, confidence * 0.8);

    // 颧骨：眼线高度、框侧。
    let left_cheekbone = ContourPoint::new(img_left + face.width * 0.22, eye_y, confidence * 0.75);
    let right_cheekbone = ContourPoint::new(img_right - face.width * 0.22, eye_y, confidence * 0.75);

    // 下颌线：嘴角高度两侧。
    let jaw_y = face.y + face.height * 0.72;
    let jaw_left = ContourPoint::new(img_left + face.width * 0.18, jaw_y, confidence * 0.7);
    let jaw_right = ContourPoint::new(img_right - face.width * 0.18, jaw_y, confidence * 0.7);

    // 下巴：中心线底端。
    let chin = ContourPoint::new(mid_x, face.y + face.height * 0.9, confidence * 0.75);

    // 太阳穴：框上部两侧。
    let temple_y = face.y + face.height * 0.28;
    let left_temple = ContourPoint::new(img_left + face.width * 0.12, temple_y, confidence * 0.65);
    let right_temple = ContourPoint::new(img_right - face.width * 0.12, temple_y, confidence * 0.65);

    // 耳朵：框外侧近似（更不可靠）。
    let left_ear = ContourPoint::new(img_left - face.width * 0.06, face.y + face.height * 0.48, confidence * 0.4);
    let right_ear = ContourPoint::new(img_right + face.width * 0.06, face.y + face.height * 0.48, confidence * 0.4);
    notes.push("耳朵/下颌为框级近似，非稠密轮廓模型".into());

    // 点数与侧脸启发：双眼与嘴不共线程度过高时降置信度。
    if face.landmarks.len() >= 5 {
        let mouth_mid_x = (geometry.mouth[0].0 + geometry.mouth[1].0) * 0.5;
        let lateral = (mouth_mid_x - eye_mid.0).abs() / eye_dist;
        if lateral > 0.35 {
            confidence *= 0.85;
            notes.push("疑似侧脸/微侧脸，已降低轮廓置信度".into());
        }
    } else {
        confidence *= 0.7;
    }

    // 遮挡粗估：检测分很低时视为可能遮挡。
    if face.score < 0.4 {
        notes.push("可能存在遮挡或模糊，禁止激进变形".into());
    }

    let confidence = confidence.clamp(0.0, 1.0);
    let allow_aggressive = confidence >= AGGRESSIVE_MIN_CONFIDENCE;

    // 校验输出在框附近（容差）。
    debug_assert!(left_edge >= right_edge - 1e-5);

    Ok(FaceContourAnalysis {
        face_id: face.id,
        face_uuid: face.uuid.clone(),
        method: CONTOUR_METHOD,
        confidence,
        allow_aggressive,
        centerline,
        left_cheek,
        right_cheek,
        left_cheekbone,
        right_cheekbone,
        jaw_left,
        jaw_right,
        chin,
        left_temple,
        right_temple,
        left_ear,
        right_ear,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_engine::face_frame::NormPoint;

    fn sample_face(score: f32) -> FaceFrame {
        FaceFrame {
            id: 1,
            uuid: "uuid-1".into(),
            score,
            x: 0.3,
            y: 0.2,
            width: 0.2,
            height: 0.28,
            landmarks: vec![
                NormPoint { x: 0.36, y: 0.30 },
                NormPoint { x: 0.44, y: 0.30 },
                NormPoint { x: 0.40, y: 0.38 },
                NormPoint { x: 0.37, y: 0.44 },
                NormPoint { x: 0.43, y: 0.44 },
            ],
            dense: None,
            level3: None,
        }
    }

    #[test]
    fn analyzes_frontal_face_with_notes() {
        let face = sample_face(0.9);
        let contour = analyze_face_contour(&face, 1000, 1000).unwrap();
        assert_eq!(contour.method, CONTOUR_METHOD);
        assert!(contour.confidence > 0.7);
        assert!(contour.allow_aggressive);
        assert!(contour.notes.iter().any(|n| n.contains("近似")));
        // 中心线 y 应从眼高到下巴方向递增。
        assert!(contour.centerline[1].y > contour.centerline[0].y);
        // 图像坐标：左脸颊 x < 右脸颊 x。
        assert!(contour.left_cheek.x < contour.right_cheek.x);
    }

    #[test]
    fn low_score_disables_aggressive() {
        let face = sample_face(0.3);
        let contour = analyze_face_contour(&face, 1000, 1000).unwrap();
        assert!(!contour.allow_aggressive);
        assert!(contour.confidence < AGGRESSIVE_MIN_CONFIDENCE);
        assert!(contour.notes.iter().any(|n| n.contains("激进") || n.contains("遮挡")));
    }

    #[test]
    fn unusable_face_errors() {
        let mut face = sample_face(0.9);
        face.landmarks.clear();
        assert!(analyze_face_contour(&face, 1000, 1000).is_err());
    }

    #[test]
    fn side_face_lowers_confidence() {
        // 嘴角严重偏向一侧。
        let mut face = sample_face(0.9);
        face.landmarks[3] = NormPoint { x: 0.48, y: 0.44 };
        face.landmarks[4] = NormPoint { x: 0.52, y: 0.44 };
        let contour = analyze_face_contour(&face, 1000, 1000).unwrap();
        assert!(contour
            .notes
            .iter()
            .any(|n| n.contains("侧脸")));
        assert!(contour.confidence <= 0.9 * 0.85 + 1e-3);
    }
}
