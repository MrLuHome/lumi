//! G13 · AI 瘦脸计划：由 `FaceContourAnalysis` 生成 `FaceShape`（保守默认强度）。
//!
//! 约束：
//! - 不整脸横压：只写 `FaceShape` 进既有轮廓环内核；
//! - `allow_aggressive=false` 时强度封顶，禁止激进变形；
//! - 关键点不可靠（confidence 低）→ 更低上限；
//! - 质量闸门仍由 `face_reshape::quality::stabilize` 在渲染时执行。

use crate::face_reshape::contour::{FaceContourAnalysis, AGGRESSIVE_MIN_CONFIDENCE};
use crate::image_engine::params::FaceShape;

/// 自然 / 精致 / 高级 对应的整体强度上限（0..100）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlimStyle {
    Natural,
    Refined,
    Advanced,
}

impl SlimStyle {
    /// 未触碰质量门前的目标强度上限。
    pub fn max_strength(self) -> f32 {
        match self {
            Self::Natural => 35.0,
            Self::Refined => 55.0,
            Self::Advanced => 70.0,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Natural => "natural",
            Self::Refined => "refined",
            Self::Advanced => "advanced",
        }
    }
}

/// 低置信度时激进档进一步封顶。
const LOW_CONFIDENCE_CAP: f32 = 30.0;

/// 由轮廓分析 + 用户/一键强度（0..100）生成脸型参数。
///
/// 返回的 `FaceShape` 各字段均在 -100..100；`notes` 说明降级原因。
pub fn plan_face_slim(
    contour: &FaceContourAnalysis,
    style: SlimStyle,
    requested_strength: f32,
) -> (FaceShape, Vec<String>) {
    let mut notes = Vec::new();
    let mut strength = requested_strength.clamp(0.0, 100.0).min(style.max_strength());

    if !contour.allow_aggressive {
        let cap = if contour.confidence < AGGRESSIVE_MIN_CONFIDENCE * 0.7 {
            LOW_CONFIDENCE_CAP.min(strength)
        } else {
            40.0f32.min(strength)
        };
        if strength > cap {
            notes.push(format!(
                "轮廓置信度 {:.0}%，瘦脸强度由 {:.0} 降至 {:.0}，禁止激进变形",
                contour.confidence * 100.0,
                strength,
                cap
            ));
            strength = cap;
        }
    }

    if strength <= f32::EPSILON {
        return (FaceShape::default(), notes);
    }

    // 置信度缩放：低置信少写。
    let conf_scale = (0.55 + 0.45 * contour.confidence).clamp(0.5, 1.0);
    let s = strength / 100.0 * conf_scale;

    // 以整体瘦脸为主，下颌/颧骨为辅；宽度略收；长度默认 0（避免拉长脸）。
    let face = FaceShape {
        slim: (62.0 * s).round().clamp(-100.0, 100.0),
        jaw: (40.0 * s).round().clamp(-100.0, 100.0),
        cheekbone: (32.0 * s).round().clamp(-100.0, 100.0),
        width: (36.0 * s).round().clamp(-100.0, 100.0),
        left: (18.0 * s).round().clamp(-100.0, 100.0),
        right: (18.0 * s).round().clamp(-100.0, 100.0),
        length: 0.0,
        balance: 0.0,
    };

    if face.is_neutral() {
        notes.push("计算后的瘦脸强度过小，已保持原脸型".into());
    }

    (face, notes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::face_reshape::contour::analyze_face_contour;
    use crate::ai_engine::face_frame::{FaceFrame, NormPoint};

    fn face(score: f32) -> FaceFrame {
        FaceFrame {
            id: 1,
            uuid: "u".into(),
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
    fn advanced_high_confidence_stronger_than_natural() {
        let c = analyze_face_contour(&face(0.95), 1000, 1000).unwrap();
        let (nat, _) = plan_face_slim(&c, SlimStyle::Natural, 100.0);
        let (adv, _) = plan_face_slim(&c, SlimStyle::Advanced, 100.0);
        assert!(adv.slim > nat.slim);
        assert!(nat.slim > 0.0);
        assert_eq!(nat.length, 0.0);
    }

    #[test]
    fn low_confidence_caps_aggressive() {
        let c = analyze_face_contour(&face(0.35), 1000, 1000).unwrap();
        assert!(!c.allow_aggressive);
        let (shape, notes) = plan_face_slim(&c, SlimStyle::Advanced, 100.0);
        assert!(shape.slim <= LOW_CONFIDENCE_CAP + 1.0);
        assert!(notes.iter().any(|n| n.contains("禁止激进") || n.contains("降至")));
    }

    #[test]
    fn zero_strength_is_neutral() {
        let c = analyze_face_contour(&face(0.9), 1000, 1000).unwrap();
        let (shape, _) = plan_face_slim(&c, SlimStyle::Advanced, 0.0);
        assert!(shape.is_neutral());
    }

    #[test]
    fn no_global_squeeze_only_contour_fields() {
        // 不应写入与「整图缩放」相关的语义；FaceShape 仅脸型字段。
        let c = analyze_face_contour(&face(0.9), 1000, 1000).unwrap();
        let (shape, _) = plan_face_slim(&c, SlimStyle::Refined, 50.0);
        assert!(shape.slim > 0.0);
        assert!(shape.jaw >= 0.0);
        assert!(shape.cheekbone >= 0.0);
    }
}
