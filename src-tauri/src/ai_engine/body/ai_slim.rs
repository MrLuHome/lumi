//! G13 · AI 瘦身计划：由 `BodyAnalysis` 生成 `BodyShape`（保守默认）。
//!
//! 约束（任务书）：
//! - 不整图横向缩小：只写轮廓带参数，走既有 `deformation::build_field`；
//! - 无对应身体部位检测 → 不强行写该部位；
//! - `detected=false` 或 confidence 过低 → 全中性；
//! - 手部/脚/脸由 `FaceGuard` 与质量折叠检测保护（既有内核）。

use crate::ai_engine::body::analysis::BodyAnalysis;
use crate::image_engine::params::BodyShape;

use crate::face_reshape::ai_slim::SlimStyle;

/// 身材分析最低置信度（低于则不自动瘦身）。
pub const BODY_MIN_CONFIDENCE: f32 = 0.25;

/// 由人体分析 + 风格 + 请求强度生成身材参数。
///
/// **正值放大、负值收窄**（与 `BodyShape` 文档一致）→ 瘦身写负值。
pub fn plan_body_slim(
    analysis: &BodyAnalysis,
    style: SlimStyle,
    requested_strength: f32,
) -> (BodyShape, Vec<String>) {
    let mut notes = Vec::new();

    if !analysis.detected {
        notes.push("未检测到人体，已跳过瘦身".into());
        return (BodyShape::default(), notes);
    }
    if analysis.confidence < BODY_MIN_CONFIDENCE {
        notes.push(format!(
            "人体置信度 {:.0}% 过低，已跳过自动瘦身",
            analysis.confidence * 100.0
        ));
        return (BodyShape::default(), notes);
    }
    if analysis.occlusion > 0.7 {
        notes.push("遮挡过高，已跳过自动瘦身".into());
        return (BodyShape::default(), notes);
    }

    let mut strength = requested_strength.clamp(0.0, 100.0).min(style.max_strength());
    // 遮挡越高越保守。
    let occ_penalty = 1.0 - analysis.occlusion.clamp(0.0, 0.7) * 0.5;
    if analysis.occlusion > 0.4 && strength > 40.0 {
        let cap = 40.0;
        if strength > cap {
            notes.push(format!("存在遮挡，瘦身强度封顶 {:.0}", cap));
            strength = cap;
        }
    }

    if strength <= f32::EPSILON {
        return (BodyShape::default(), notes);
    }

    let conf_scale = (0.5 + 0.5 * analysis.confidence).clamp(0.5, 1.0);
    let s = strength / 100.0 * conf_scale * occ_penalty;

    // 分区盒存在才写对应部位（任务书：没有检测到部位不得强行变形）。
    let regions = &analysis.regions;
    let has_waist = regions.waist[2] > 0.02 && regions.waist[3] > 0.01;
    let has_hips = regions.hips[2] > 0.02 && regions.hips[3] > 0.01;
    let has_legs = regions.legs[2] > 0.02 && regions.legs[3] > 0.01;
    let has_arms = !regions.arms.is_empty();

    // 肩膀：仅在上身足够宽时微调（避免把窄肩压没）。
    let shoulder_w = analysis.bbox[2];
    let has_shoulders = shoulder_w > 0.08;

    let shape = BodyShape {
        waist: if has_waist {
            (-55.0 * s).round().clamp(-100.0, 100.0)
        } else {
            0.0
        },
        hips: if has_hips {
            (-40.0 * s).round().clamp(-100.0, 100.0)
        } else {
            0.0
        },
        thighs: if has_legs {
            (-35.0 * s).round().clamp(-100.0, 100.0)
        } else {
            0.0
        },
        calves: if has_legs {
            (-22.0 * s).round().clamp(-100.0, 100.0)
        } else {
            0.0
        },
        arms: if has_arms {
            (-30.0 * s).round().clamp(-100.0, 100.0)
        } else {
            0.0
        },
        // 肩膀瘦身为轻微收窄（负），非扩肩。
        shoulders: if has_shoulders {
            (-15.0 * s).round().clamp(-100.0, 100.0)
        } else {
            0.0
        },
        // 腿部比例：中高强度才轻微调整，避免关节异常。
        legs: if has_legs && style == SlimStyle::Advanced && strength >= 50.0 {
            (-12.0 * s).round().clamp(-100.0, 100.0)
        } else {
            0.0
        },
    };

    if shape.is_neutral() {
        notes.push("未获得足够身体分区，保持原身材".into());
    } else {
        notes.push("轮廓级瘦身，非姿态模型驱动".into());
    }

    (shape, notes)
}

/// 质量摘要：交给既有 fold/位移检查前的预检。
pub fn precheck_body_plan(analysis: &BodyAnalysis, shape: &BodyShape) -> Result<(), String> {
    if shape.is_neutral() {
        return Ok(());
    }
    if !analysis.detected {
        return Err("无主体不可瘦身".into());
    }
    if analysis.confidence < BODY_MIN_CONFIDENCE {
        return Err("人体置信度不足".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_engine::body::analysis::analyze_body;

    fn analysis(detected: bool, confidence: f32, occlusion: f32) -> BodyAnalysis {
        let mut a = if detected {
            let alpha = {
                let mut v = vec![0.0f32; 60 * 120];
                for y in 20..100u32 {
                    for x in 15..45u32 {
                        v[(y * 60 + x) as usize] = 1.0;
                    }
                }
                v
            };
            analyze_body("s", &alpha, 60, 120, &[])
        } else {
            analyze_body("s", &vec![0.0; 60 * 120], 60, 120, &[])
        };
        a.confidence = confidence;
        a.occlusion = occlusion;
        a
    }

    #[test]
    fn undetected_body_is_neutral() {
        let a = analysis(false, 0.0, 1.0);
        let (shape, notes) = plan_body_slim(&a, SlimStyle::Advanced, 80.0);
        assert!(shape.is_neutral());
        assert!(notes.iter().any(|n| n.contains("未检测到人体")));
    }

    #[test]
    fn slim_writes_negative_values() {
        let a = analysis(true, 0.8, 0.2);
        let (shape, _) = plan_body_slim(&a, SlimStyle::Refined, 60.0);
        assert!(shape.waist < 0.0, "收腰应为负 {}", shape.waist);
        assert!(shape.hips <= 0.0);
        assert!(shape.thighs <= 0.0);
    }

    #[test]
    fn high_occlusion_skips_or_caps() {
        let a = analysis(true, 0.8, 0.85);
        let (shape, notes) = plan_body_slim(&a, SlimStyle::Advanced, 90.0);
        assert!(shape.is_neutral() || shape.waist.abs() <= 40.0);
        assert!(!notes.is_empty());
    }

    #[test]
    fn low_confidence_skips() {
        let a = analysis(true, 0.1, 0.2);
        let (shape, notes) = plan_body_slim(&a, SlimStyle::Advanced, 80.0);
        assert!(shape.is_neutral());
        assert!(notes.iter().any(|n| n.contains("过低")));
    }

    #[test]
    fn precheck_rejects_empty() {
        let a = analysis(false, 0.0, 1.0);
        let shape = BodyShape { waist: -30.0, ..Default::default() };
        assert!(precheck_body_plan(&a, &shape).is_err());
    }
}
