//! G12 · 人体分析（轮廓级 + 分区，**姿态模型未启用时不伪造骨架关键点**）。
//!
//! 输入：人物 Alpha、可选人脸框。输出 `BodyAnalysis`：
//! - 人体检测（有无主体、包围盒、置信度）
//! - 轮廓关键「结构点」（肩/腰/胯/膝等高度上的左右边界，标注 method）
//! - 分区蒙版：手臂带、腰腹、胯、腿、衣服保护（可空）、背景保护
//! - 遮挡与总体置信度
//!
//! 与 `pose::capability()` 一致：`pose_model_available=false` 时
//! `PoseKeypoint.source = "silhouette"`，禁止 UI 宣称 AI 姿态识别。

use serde::Serialize;

use crate::ai_engine::face_frame::FaceFrame;
use crate::geometry::protection::ProtectionMask;

use super::landmarks::analyze;

/// 结构点来源。
pub const SOURCE_SILHOUETTE: &str = "silhouette";
/// 仅当未来接入已验证姿态模型时才可改（G12 恒为 silhouette）。
pub const SOURCE_POSE_MODEL: &str = "pose-model";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BodyPart {
    ShoulderLeft,
    ShoulderRight,
    ElbowLeft,
    ElbowRight,
    WristLeft,
    WristRight,
    Chest,
    Waist,
    HipLeft,
    HipRight,
    KneeLeft,
    KneeRight,
    AnkleLeft,
    AnkleRight,
    Center,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PoseKeypoint {
    pub part: BodyPart,
    /// 归一化图像坐标。
    pub x: f32,
    pub y: f32,
    pub confidence: f32,
    /// `silhouette` = 轮廓推导；`pose-model` = 姿态网络（当前不可用）。
    pub source: &'static str,
}

/// 人体结构点集合。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BodyStructure {
    pub keypoints: Vec<PoseKeypoint>,
    pub source: &'static str,
    pub pose_model_available: bool,
}

/// 分区：归一化区域列表（用于保护蒙版合成，不直接改像素）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BodyRegions {
    /// 手臂近似：上身左右外侧带。
    pub arms: Vec<[f32; 4]>,
    pub waist: [f32; 4],
    pub hips: [f32; 4],
    pub legs: [f32; 4],
    /// 衣服保护：无分割模型时为空（G12 不伪造）。
    pub clothes_protect: Vec<[f32; 4]>,
    pub confidence: f32,
}

/// 完整个人体分析结果。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BodyAnalysis {
    pub subject_id: String,
    pub method: &'static str,
    pub detected: bool,
    /// 0..1。
    pub confidence: f32,
    /// 0..1 包围盒 (x,y,w,h)。
    pub bbox: [f32; 4],
    pub structure: BodyStructure,
    pub regions: BodyRegions,
    /// 人物蒙版（归一化 alpha，与输入同尺寸）；背景保护 = 1 - mask。
    pub subject_alpha: Vec<f32>,
    pub width: u32,
    pub height: u32,
    /// 遮挡启发：顶部人脸缺失或覆盖率异常时升高。
    pub occlusion: f32,
    pub notes: Vec<String>,
}

pub const BODY_ANALYSIS_METHOD: &str = "alpha-silhouette-contour";

/// 从人物 Alpha（+ 可选人脸）构建分析。Alpha 为空/过碎时 `detected=false`。
pub fn analyze_body(
    subject_id: &str,
    alpha: &[f32],
    width: u32,
    height: u32,
    faces: &[FaceFrame],
) -> BodyAnalysis {
    let empty_notes = || vec!["轮廓级近似，非姿态识别模型".to_string()];

    let Some(metrics) = analyze(alpha, width, height) else {
        return BodyAnalysis {
            subject_id: subject_id.into(),
            method: BODY_ANALYSIS_METHOD,
            detected: false,
            confidence: 0.0,
            bbox: [0.0, 0.0, 0.0, 0.0],
            structure: BodyStructure {
                keypoints: Vec::new(),
                source: SOURCE_SILHOUETTE,
                pose_model_available: false,
            },
            regions: BodyRegions {
                arms: Vec::new(),
                waist: [0.0, 0.0, 0.0, 0.0],
                hips: [0.0, 0.0, 0.0, 0.0],
                legs: [0.0, 0.0, 0.0, 0.0],
                clothes_protect: Vec::new(),
                confidence: 0.0,
            },
            subject_alpha: alpha.to_vec(),
            width,
            height,
            occlusion: 1.0,
            notes: {
                let mut n = empty_notes();
                n.push("未检测到可用人体轮廓，禁止变形".into());
                n
            },
        };
    };

    let mut notes = empty_notes();
    let w = width.max(1) as f32;
    let h = height.max(1) as f32;
    let bbox = [
        metrics.left / w,
        metrics.top / h,
        (metrics.right - metrics.left).max(0.0) / w,
        (metrics.bottom - metrics.top).max(0.0) / h,
    ];

    // 结构点：在关键高度采样轮廓左右边界（与 deformation 使用的高度比例对齐）。
    let mut keypoints = Vec::new();
    let mut push_row = |part: BodyPart, fraction: f32, conf: f32| {
        if let Some(row) = metrics.sampled(fraction) {
            let y = row.y / h;
            keypoints.push(PoseKeypoint {
                part: match part {
                    BodyPart::Center => BodyPart::Center,
                    other => other,
                },
                x: (row.left + row.right) * 0.5 / w,
                y,
                confidence: conf,
                source: SOURCE_SILHOUETTE,
            });
            // 左右边界点用同一 part 的镜像约定：这里只推 Center + 明确左右肩腰胯膝踝。
            keypoints.push(PoseKeypoint {
                part: part,
                x: row.left / w,
                y,
                confidence: conf * 0.9,
                source: SOURCE_SILHOUETTE,
            });
            keypoints.push(PoseKeypoint {
                part: part,
                x: row.right / w,
                y,
                confidence: conf * 0.9,
                source: SOURCE_SILHOUETTE,
            });
        }
    };

    let conf = metrics.confidence;
    push_row(BodyPart::ShoulderLeft, 0.16, conf);
    push_row(BodyPart::Chest, 0.24, conf * 0.9);
    push_row(BodyPart::Waist, 0.40, conf);
    push_row(BodyPart::HipLeft, 0.53, conf);
    push_row(BodyPart::KneeLeft, 0.78, conf * 0.85);
    push_row(BodyPart::AnkleLeft, 0.94, conf * 0.7);

    // 手肘/手腕/另一侧肩膝踝：无姿态模型时用对称高度近似并降置信度，source 仍为 silhouette。
    for (part, fraction) in [
        (BodyPart::ElbowLeft, 0.30),
        (BodyPart::WristLeft, 0.42),
        (BodyPart::ShoulderRight, 0.16),
        (BodyPart::HipRight, 0.53),
        (BodyPart::KneeRight, 0.78),
        (BodyPart::AnkleRight, 0.94),
    ] {
        if let Some(row) = metrics.sampled(fraction) {
            // 肢体点放在轮廓内侧 25%，避免贴边。
            let inset = (row.right - row.left) * 0.25;
            let x = if matches!(
                part,
                BodyPart::ElbowLeft | BodyPart::WristLeft | BodyPart::ShoulderLeft | BodyPart::HipLeft | BodyPart::KneeLeft | BodyPart::AnkleLeft
            ) {
                row.left + inset
            } else {
                row.right - inset
            };
            keypoints.push(PoseKeypoint {
                part,
                x: (x / w).clamp(0.0, 1.0),
                y: (row.y / h).clamp(0.0, 1.0),
                confidence: conf * 0.55,
                source: SOURCE_SILHOUETTE,
            });
        }
    }
    notes.push("肩肘腕膝踝等为轮廓高度近似，source=silhouette，非姿态网络".into());

    // 分区盒（归一化）。
    let band = |f0: f32, f1: f32| -> [f32; 4] {
        let y0 = metrics.top + metrics.height() * f0;
        let y1 = metrics.top + metrics.height() * f1;
        let r0 = metrics.sampled(f0);
        let r1 = metrics.sampled(f1);
        let left = r0.map(|r| r.left).unwrap_or(metrics.left).min(metrics.left);
        let right = r0.map(|r| r.right).unwrap_or(metrics.right).max(metrics.right);
        let left2 = r1.map(|r| r.left).unwrap_or(left);
        let right2 = r1.map(|r| r.right).unwrap_or(right);
        [
            (left.min(left2) / w).clamp(0.0, 1.0),
            (y0 / h).clamp(0.0, 1.0),
            ((right.max(right2) - left.min(left2)).max(0.0) / w).clamp(0.0, 1.0),
            ((y1 - y0).max(0.0) / h).clamp(0.0, 1.0),
        ]
    };

    // 手臂：上身外侧两个窄带（框级近似）。
    let arm_band = band(0.18, 0.42);
    let arms = vec![
        [
            arm_band[0],
            arm_band[1],
            (arm_band[2] * 0.22).max(0.01),
            arm_band[3],
        ],
        [
            (arm_band[0] + arm_band[2] * 0.78).clamp(0.0, 1.0),
            arm_band[1],
            (arm_band[2] * 0.22).max(0.01),
            arm_band[3],
        ],
    ];

    let regions = BodyRegions {
        arms,
        waist: band(0.34, 0.48),
        hips: band(0.48, 0.60),
        legs: band(0.60, 0.98),
        clothes_protect: Vec::new(), // 无已验证衣服分割权重
        confidence: conf,
    };
    if regions.clothes_protect.is_empty() {
        notes.push("衣服保护区为空：无已验证分割权重，不伪造".into());
    }

    // 遮挡：有脸且脸在主体顶部附近 → 低；无脸的全身可能是背影/遮挡。
    let mut occlusion = 0.25;
    if faces.is_empty() {
        occlusion = 0.55;
        notes.push("未检测到人脸，可能是背影或头部遮挡".into());
    } else {
        let top_face = faces
            .iter()
            .map(|f| f.y)
            .fold(f32::MAX, f32::min);
        if top_face > metrics.top / h + bbox[3] * 0.35 {
            occlusion = 0.45;
            notes.push("人脸不在主体上部，姿态/遮挡存疑".into());
        }
    }

    // 总置信度：轮廓覆盖 × (1 - 遮挡/2)。
    let confidence = (metrics.confidence * (1.0 - occlusion * 0.5)).clamp(0.0, 1.0);

    BodyAnalysis {
        subject_id: subject_id.into(),
        method: BODY_ANALYSIS_METHOD,
        detected: true,
        confidence,
        bbox,
        structure: BodyStructure {
            keypoints,
            source: SOURCE_SILHOUETTE,
            pose_model_available: false,
        },
        regions,
        subject_alpha: alpha.to_vec(),
        width,
        height,
        occlusion,
        notes,
    }
}

/// 背景保护蒙版：人物外 w=1 允许？——保护语义是「允许位移权重」：
/// 背景应完全保护 → w=0 在人物外，人物内 w=1（变形只在人内）。
/// 与 `geometry::ProtectionMask` 对齐：0=保护，1=允许。
pub fn background_protection(analysis: &BodyAnalysis) -> ProtectionMask {
    let alpha = analysis
        .subject_alpha
        .iter()
        .map(|a| if *a > 0.4 { 1.0 } else { 0.0 })
        .collect();
    ProtectionMask::from_alpha(analysis.width, analysis.height, alpha)
        .unwrap_or_else(|| ProtectionMask::full_block(analysis.width, analysis.height))
}

/// 人体边缘保护：轮廓带内侧允许、带外/背景保护（简化：距边界过远的背景已是 block）。
/// 这里返回「人体边缘 ramp」近似：|alpha-0.5| 小 → 更允许平滑过渡。
pub fn body_edge_protection(analysis: &BodyAnalysis) -> ProtectionMask {
    let alpha = analysis
        .subject_alpha
        .iter()
        .map(|a| {
            // 边缘（a≈0.5）附近允许位移过渡；深处主体 1；背景 0。
            let edge = 1.0 - ((a - 0.5).abs() * 2.0).clamp(0.0, 1.0);
            if *a > 0.45 {
                (0.55 + 0.45 * a).clamp(0.0, 1.0).max(edge * 0.3)
            } else {
                0.0
            }
        })
        .collect();
    ProtectionMask::from_alpha(analysis.width, analysis.height, alpha)
        .unwrap_or_else(|| ProtectionMask::full_block(analysis.width, analysis.height))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect_alpha(width: u32, height: u32) -> Vec<f32> {
        let mut alpha = vec![0.0f32; (width * height) as usize];
        for y in 20..100u32 {
            for x in 15..45u32 {
                alpha[(y * width + x) as usize] = 1.0;
            }
        }
        alpha
    }

    #[test]
    fn detects_body_and_marks_silhouette_source() {
        let alpha = rect_alpha(60, 120);
        let analysis = analyze_body("s1", &alpha, 60, 120, &[]);
        assert!(analysis.detected);
        assert_eq!(analysis.structure.source, SOURCE_SILHOUETTE);
        assert!(!analysis.structure.pose_model_available);
        assert!(analysis.structure.keypoints.iter().all(|k| k.source == SOURCE_SILHOUETTE));
        assert!(analysis.confidence > 0.0);
        assert!(analysis.notes.iter().any(|n| n.contains("轮廓")));
        // 无脸 → 遮挡升高。
        assert!(analysis.occlusion >= 0.5);
    }

    #[test]
    fn empty_alpha_not_detected() {
        let analysis = analyze_body("s1", &vec![0.0; 40 * 40], 40, 40, &[]);
        assert!(!analysis.detected);
        assert_eq!(analysis.confidence, 0.0);
        assert!(analysis.notes.iter().any(|n| n.contains("禁止变形")));
    }

    #[test]
    fn clothes_protect_stays_empty_without_model() {
        let alpha = rect_alpha(60, 120);
        let analysis = analyze_body("s1", &alpha, 60, 120, &[]);
        assert!(analysis.regions.clothes_protect.is_empty());
        assert!(analysis
            .notes
            .iter()
            .any(|n| n.contains("衣服保护区为空")));
    }

    #[test]
    fn background_protection_blocks_outside_subject() {
        let alpha = rect_alpha(60, 120);
        let analysis = analyze_body("s1", &alpha, 60, 120, &[]);
        let mask = background_protection(&analysis);
        // 背景 (0,0) 保护=0；主体中心允许=1。
        assert_eq!(mask.at(0, 0), 0.0);
        assert_eq!(mask.at(30, 50), 1.0);
    }

    #[test]
    fn region_boxes_are_normalized() {
        let alpha = rect_alpha(60, 120);
        let analysis = analyze_body("s1", &alpha, 60, 120, &[]);
        for box4 in [
            analysis.regions.waist,
            analysis.regions.hips,
            analysis.regions.legs,
        ] {
            assert!((0.0..=1.0).contains(&box4[0]));
            assert!((0.0..=1.0).contains(&box4[1]));
            assert!(box4[2] >= 0.0 && box4[2] <= 1.0);
            assert!(box4[3] > 0.0);
        }
        assert_eq!(analysis.regions.arms.len(), 2);
    }
}
