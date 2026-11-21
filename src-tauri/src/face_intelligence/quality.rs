//! 质量验证与 LEVEL 3 门控。
//!
//! 门控只承认**实测**证据：
//! 1. Provider 的稠密能力等级为 `Verified`；
//! 2. 同一张照片重复分析的稳定性 ≤ 2% 眼距；
//! 3. 稠密区域数量与几何关系合理；
//! 4. 人脸置信度达到阈值。
//!
//! 任何一项缺失，对应五官保持禁用并给出可读原因，而不是用无效滑块占位。

use serde::{Deserialize, Serialize};

use crate::face_intelligence::capability::ProviderDescriptor;
use crate::face_intelligence::types::{DenseFace, RegionKind};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityThresholds {
    pub min_confidence: f32,
    pub max_stability_ratio: f32,
    pub min_eye_width: f32,
    pub min_mouth_width: f32,
    pub min_dense_points: usize,
}

impl Default for QualityThresholds {
    fn default() -> Self {
        Self {
            min_confidence: 0.45,
            max_stability_ratio: 0.02,
            min_eye_width: 0.018,
            min_mouth_width: 0.02,
            min_dense_points: 20,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegionQuality {
    pub region: String,
    pub points: usize,
    pub passed: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceQuality {
    pub passed: bool,
    pub dense_points: usize,
    pub confidence: f32,
    pub eye_distance: f32,
    pub regions: Vec<RegionQuality>,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureGate {
    pub enabled: bool,
    pub reason: String,
}

impl FeatureGate {
    fn locked(reason: impl Into<String>) -> Self {
        Self {
            enabled: false,
            reason: reason.into(),
        }
    }

    fn open(reason: impl Into<String>) -> Self {
        Self {
            enabled: true,
            reason: reason.into(),
        }
    }
}

/// LEVEL 3 的逐项门控结果。前端只显示 enabled=true 的滑块。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Level3Gate {
    pub provider_id: String,
    pub topology_id: String,
    pub face_uuid: String,
    pub dense_landmarks: bool,
    pub eyes: FeatureGate,
    pub nose: FeatureGate,
    pub mouth: FeatureGate,
    pub chin: FeatureGate,
    pub summary: String,
    pub issues: Vec<String>,
}

impl Level3Gate {
    pub fn locked(provider: &str, face_uuid: &str, reason: &str) -> Self {
        Self {
            provider_id: provider.into(),
            topology_id: String::new(),
            face_uuid: face_uuid.into(),
            dense_landmarks: false,
            eyes: FeatureGate::locked(reason),
            nose: FeatureGate::locked(reason),
            mouth: FeatureGate::locked(reason),
            chin: FeatureGate::locked(reason),
            summary: reason.into(),
            issues: vec![reason.into()],
        }
    }

    pub fn any_enabled(&self) -> bool {
        self.eyes.enabled || self.nose.enabled || self.mouth.enabled || self.chin.enabled
    }
}

/// 同一输入重复分析的稳定性报告。由 Provider 探针生成并缓存。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StabilityReport {
    pub provider_id: String,
    pub topology_id: String,
    pub runs: usize,
    pub faces_per_run: Vec<usize>,
    pub eye_distance: f32,
    /// 所有重复点相对首轮的最大位移（归一化坐标）。
    pub max_deviation: f32,
    /// `max_deviation / eye_distance`。
    pub ratio: f32,
    pub passed: bool,
    pub elapsed_ms: Vec<u64>,
    pub note: String,
}

impl StabilityReport {
    pub fn pending(provider_id: &str) -> Self {
        Self {
            provider_id: provider_id.into(),
            topology_id: String::new(),
            runs: 0,
            faces_per_run: Vec::new(),
            eye_distance: 0.0,
            max_deviation: 0.0,
            ratio: 0.0,
            passed: false,
            elapsed_ms: Vec::new(),
            note: "尚未完成重复稳定性实测".into(),
        }
    }
}

/// 置信度分布，供文档与 UI 展示；不是主观描述。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConfidenceDistribution {
    pub samples: usize,
    pub min: f32,
    pub max: f32,
    pub mean: f32,
    pub below_threshold: usize,
}

pub fn confidence_distribution(faces: &[DenseFace], threshold: f32) -> ConfidenceDistribution {
    if faces.is_empty() {
        return ConfidenceDistribution::default();
    }
    let values: Vec<f32> = faces.iter().map(|face| face.confidence).collect();
    ConfidenceDistribution {
        samples: values.len(),
        min: values.iter().copied().fold(f32::INFINITY, f32::min),
        max: values.iter().copied().fold(f32::NEG_INFINITY, f32::max),
        mean: values.iter().sum::<f32>() / values.len() as f32,
        below_threshold: values.iter().filter(|value| **value < threshold).count(),
    }
}

fn mean_point(region: &crate::face_intelligence::types::DenseRegion) -> (f32, f32) {
    let count = region.points.len().max(1) as f32;
    let x = region.points.iter().map(|point| point.x).sum::<f32>() / count;
    let y = region.points.iter().map(|point| point.y).sum::<f32>() / count;
    (x, y)
}

fn width_of(region: &crate::face_intelligence::types::DenseRegion) -> f32 {
    let min = region
        .points
        .iter()
        .map(|point| point.x)
        .fold(f32::INFINITY, f32::min);
    let max = region
        .points
        .iter()
        .map(|point| point.x)
        .fold(f32::NEG_INFINITY, f32::max);
    (max - min).max(0.0)
}

/// 眼距：左右眼区域中心距离。用于把位移/几何阈值归一化，避免分辨率影响。
pub fn eye_distance(dense: &DenseFace) -> f32 {
    let left = dense
        .region(RegionKind::LeftEye)
        .or_else(|| dense.region(RegionKind::RightEye));
    let right = dense
        .region(RegionKind::RightEye)
        .or_else(|| dense.region(RegionKind::LeftEye));
    match (left, right) {
        (Some(left), Some(right)) if !left.points.is_empty() && !right.points.is_empty() => {
            let (lx, ly) = mean_point(left);
            let (rx, ry) = mean_point(right);
            ((lx - rx).powi(2) + (ly - ry).powi(2)).sqrt()
        }
        _ => 0.0,
    }
}

/// 单张人脸的稠密质量检查。
pub fn evaluate_face_quality(dense: &DenseFace, thresholds: &QualityThresholds) -> FaceQuality {
    let mut issues = Vec::new();
    let mut regions = Vec::new();

    if dense.confidence < thresholds.min_confidence {
        issues.push(format!(
            "人脸置信度 {:.3} 低于阈值 {:.2}",
            dense.confidence, thresholds.min_confidence
        ));
    }
    let points = dense.point_count();
    if points < thresholds.min_dense_points {
        issues.push(format!(
            "稠密关键点仅 {points} 个，低于 {}",
            thresholds.min_dense_points
        ));
    }

    let eye_left = dense.region(RegionKind::LeftEye);
    let eye_right = dense.region(RegionKind::RightEye);
    let eye_ok = match (eye_left, eye_right) {
        (Some(left), Some(right)) if left.points.len() >= 4 && right.points.len() >= 4 => {
            let width = width_of(left).min(width_of(right));
            if width < thresholds.min_eye_width {
                issues.push(format!(
                    "眼睑轮廓宽度 {:.4} 小于 {:.4}，不足以驱动精修",
                    width, thresholds.min_eye_width
                ));
                false
            } else {
                true
            }
        }
        _ => {
            issues.push("缺少左右眼睑轮廓（各至少 4 点）".into());
            false
        }
    };

    let nose = dense.region(RegionKind::Nose);
    let nose_ok = match nose {
        Some(region) if region.points.len() >= 6 => {
            let spread = width_of(region);
            if spread < thresholds.min_eye_width * 0.8 {
                issues.push("鼻部轮廓横向跨度不足，无法区分鼻翼".into());
                false
            } else {
                true
            }
        }
        _ => {
            issues.push("缺少鼻部轮廓（至少 6 点）".into());
            false
        }
    };

    let outer = dense.region(RegionKind::OuterLips);
    let inner = dense.region(RegionKind::InnerLips);
    let mouth_ok = match (outer, inner) {
        (Some(outer), Some(inner)) if outer.points.len() >= 8 && inner.points.len() >= 3 => {
            if width_of(outer) < thresholds.min_mouth_width {
                issues.push("唇部轮廓宽度不足".into());
                false
            } else {
                true
            }
        }
        _ => {
            issues.push("缺少完整外唇/内唇轮廓".into());
            false
        }
    };

    let contour = dense.region(RegionKind::FaceContour);
    let chin_ok = match contour {
        Some(region) if region.points.len() >= 9 => true,
        _ => {
            issues.push("缺少脸缘轮廓（至少 9 点）".into());
            false
        }
    };

    // 几何顺序：眼在鼻上、鼻在嘴上、嘴在脸缘上方。
    let axis_ok = if eye_ok {
        let eye_y = (mean_point(eye_left.unwrap()).1 + mean_point(eye_right.unwrap()).1) * 0.5;
        let nose_y = nose
            .map(mean_point)
            .map(|point| point.1)
            .unwrap_or(eye_y + 0.01);
        let mouth_y = outer
            .map(mean_point)
            .map(|point| point.1)
            .unwrap_or(nose_y + 0.01);
        let chin_y = contour
            .map(|region| {
                region
                    .points
                    .iter()
                    .map(|point| point.y)
                    .fold(f32::NEG_INFINITY, f32::max)
            })
            .unwrap_or(mouth_y + 0.01);
        if !(eye_y < nose_y && nose_y < mouth_y && mouth_y < chin_y) {
            issues.push("五官纵向顺序异常（可能坐标翻转或标签错误）".into());
            false
        } else {
            true
        }
    } else {
        false
    };

    for (kind, ok) in [
        (RegionKind::LeftEye, eye_ok),
        (RegionKind::RightEye, eye_ok),
        (RegionKind::Nose, nose_ok),
        (RegionKind::OuterLips, mouth_ok),
        (RegionKind::InnerLips, mouth_ok),
        (RegionKind::FaceContour, chin_ok),
    ] {
        let count = dense
            .region(kind)
            .map(|region| region.points.len())
            .unwrap_or(0);
        regions.push(RegionQuality {
            region: kind.label().into(),
            points: count,
            passed: ok,
            reason: if ok {
                "结构完整".into()
            } else {
                format!("{} 点数/几何不满足门控", kind.label())
            },
        });
    }

    let passed = issues.is_empty() && eye_ok && nose_ok && mouth_ok && chin_ok && axis_ok;
    FaceQuality {
        passed,
        dense_points: points,
        confidence: dense.confidence,
        eye_distance: eye_distance(dense),
        regions,
        issues,
    }
}

/// 逐项 LEVEL 3 门控。`stability` 必须是本机实测；`None` 一律禁用。
pub fn level3_gate(
    dense: Option<&DenseFace>,
    provider: &ProviderDescriptor,
    stability: Option<&StabilityReport>,
    thresholds: &QualityThresholds,
) -> Level3Gate {
    let Some(dense) = dense else {
        return Level3Gate::locked(
            &provider.id,
            "",
            "当前照片没有人脸分析结果，请先运行 AI 人像分析",
        );
    };
    let verified = provider.verified_capabilities;
    let cap_reason = if verified.dense_landmarks {
        None
    } else {
        Some(format!(
            "{} 的稠密关键点尚未通过实测验证",
            provider.display_name
        ))
    };
    if let Some(reason) = cap_reason {
        return Level3Gate::locked(&provider.id, "", &reason);
    }
    let Some(stability) = stability else {
        return Level3Gate::locked(
            &provider.id,
            "",
            "尚未完成重复分析稳定性实测，LEVEL 3 暂不可用",
        );
    };
    if !stability.passed {
        return Level3Gate {
            provider_id: provider.id.clone(),
            topology_id: dense.topology_id.clone(),
            face_uuid: String::new(),
            dense_landmarks: true,
            eyes: FeatureGate::locked(format!(
                "稳定性未达标：最大偏差 {:.2}% 眼距（阈值 {:.2}%）",
                stability.ratio * 100.0,
                thresholds.max_stability_ratio * 100.0
            )),
            nose: FeatureGate::locked("稳定性未达标"),
            mouth: FeatureGate::locked("稳定性未达标"),
            chin: FeatureGate::locked("稳定性未达标"),
            summary: "重复分析稳定性未达标，LEVEL 3 保持禁用".into(),
            issues: vec![stability.note.clone()],
        };
    }

    let quality = evaluate_face_quality(dense, thresholds);
    let eye_region_ok = quality
        .issues
        .iter()
        .all(|issue| !issue.contains("眼") && !issue.contains("顺序"));
    let nose_region_ok = quality.issues.iter().all(|issue| !issue.contains("鼻"));
    let mouth_region_ok = quality.issues.iter().all(|issue| !issue.contains("唇"));
    let chin_region_ok = quality
        .issues
        .iter()
        .all(|issue| !issue.contains("脸缘") && !issue.contains("顺序"));

    let stable_note = format!(
        "{} 次重复最大偏差 {:.3}% 眼距",
        stability.runs,
        stability.ratio * 100.0
    );
    let gate = |ok: bool, capability: bool, region: &str| {
        if !capability {
            FeatureGate::locked(format!("{region}：Provider 未声明并验证该区域能力"))
        } else if ok && quality.passed {
            FeatureGate::open(format!("{region}：{stable_note}，结构校验通过"))
        } else {
            FeatureGate::locked(format!(
                "{region}：{}",
                quality
                    .issues
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "质量未达标".into())
            ))
        }
    };

    let eyes = gate(
        eye_region_ok,
        verified.eye_landmarks && verified.eyelid_landmarks,
        "眼部",
    );
    let nose = gate(nose_region_ok, verified.nose_landmarks, "鼻部");
    let mouth = gate(mouth_region_ok, verified.lip_landmarks, "唇部");
    // 下巴依赖脸缘轮廓，不额外要求独立 chin 能力位。
    let chin = if quality.passed {
        FeatureGate::open(format!("面部轮廓：{stable_note}，结构校验通过"))
    } else if chin_region_ok {
        FeatureGate::locked(format!(
            "面部轮廓：{}",
            quality
                .issues
                .first()
                .cloned()
                .unwrap_or_else(|| "质量未达标".into())
        ))
    } else {
        FeatureGate::locked("面部轮廓：缺少可用脸缘点")
    };

    let any = eyes.enabled || nose.enabled || mouth.enabled || chin.enabled;
    Level3Gate {
        provider_id: provider.id.clone(),
        topology_id: dense.topology_id.clone(),
        face_uuid: String::new(),
        dense_landmarks: true,
        eyes,
        nose,
        mouth,
        chin,
        summary: if any {
            format!("LEVEL 3 已按实测质量部分解锁（{stable_note}）")
        } else {
            "LEVEL 3 保持禁用：稠密质量未通过".into()
        },
        issues: quality.issues,
    }
}

/// 由重复分析计算稳定性。按人脸顺序匹配；人脸数量变化直接判失败。
pub fn measure_stability(
    provider_id: &str,
    topology_id: &str,
    runs: &[Vec<DenseFace>],
    elapsed_ms: &[u64],
    thresholds: &QualityThresholds,
) -> StabilityReport {
    let counts: Vec<usize> = runs.iter().map(Vec::len).collect();
    let mut issues = Vec::new();
    if runs.len() < 2 {
        issues.push("至少需要 2 次重复分析".into());
    }
    if counts.iter().any(|count| *count != counts[0]) {
        issues.push(format!("重复分析人脸数量不一致：{counts:?}"));
    }
    let mut max_deviation = 0.0f32;
    let mut eye = 0.0f32;
    if !runs.is_empty() {
        let first = &runs[0];
        for (index, face) in first.iter().enumerate() {
            eye = eye.max(eye_distance(face));
            for run in runs.iter().skip(1) {
                let Some(other) = run.get(index) else {
                    continue;
                };
                for region in &face.regions {
                    let Some(other_region) = other.region(region.kind) else {
                        issues.push(format!("重复结果缺少 {}", region.kind.label()));
                        continue;
                    };
                    for (a, b) in region.points.iter().zip(other_region.points.iter()) {
                        max_deviation =
                            max_deviation.max(((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt());
                    }
                }
            }
        }
    }
    if eye <= 1e-6 {
        issues.push("无法计算眼距，稳定性不可用".into());
    }
    let ratio = if eye > 1e-6 {
        max_deviation / eye
    } else {
        // 没有可比较的稠密人脸（或无眼距）时不能给出真实比值；
        // 用 1.0 表示“完全不可信”，同时 `issues` 已说明原因，
        // 避免无穷大在 JSON 里变成 null。
        1.0
    };
    let passed = issues.is_empty() && ratio <= thresholds.max_stability_ratio;
    let note = if passed {
        format!(
            "{} 次重复，最大偏差 {:.3}% 眼距（阈值 {:.2}%）",
            runs.len(),
            ratio * 100.0,
            thresholds.max_stability_ratio * 100.0
        )
    } else if issues.is_empty() {
        format!(
            "最大偏差 {:.3}% 眼距，超过阈值 {:.2}%",
            ratio * 100.0,
            thresholds.max_stability_ratio * 100.0
        )
    } else {
        issues.join("；")
    };
    StabilityReport {
        provider_id: provider_id.into(),
        topology_id: topology_id.into(),
        runs: runs.len(),
        faces_per_run: counts,
        eye_distance: eye,
        max_deviation,
        ratio,
        passed,
        elapsed_ms: elapsed_ms.to_vec(),
        note,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::face_intelligence::capability::{
        CapabilityEvidence, EvidenceLevel, LicenseInfo, ProviderCapabilities, ProviderKind,
        ProviderState,
    };
    use crate::face_intelligence::types::{BoundingBox, DenseRegion, Landmark};

    fn region(kind: RegionKind, points: &[(f32, f32)]) -> DenseRegion {
        DenseRegion {
            kind,
            points: points
                .iter()
                .enumerate()
                .map(|(index, (x, y))| {
                    Landmark::new(*x, *y, 0.95, format!("{}.{}", kind.prefix(), index))
                })
                .collect(),
            closed: true,
        }
    }

    fn dense() -> DenseFace {
        DenseFace {
            topology_id: "apple-vision-landmarks".into(),
            provider_id: "apple-vision".into(),
            bounding_box: BoundingBox {
                x: 0.25,
                y: 0.2,
                width: 0.5,
                height: 0.6,
            },
            confidence: 0.98,
            regions: vec![
                region(
                    RegionKind::FaceContour,
                    &[
                        (0.30, 0.25),
                        (0.32, 0.42),
                        (0.35, 0.65),
                        (0.42, 0.72),
                        (0.50, 0.78),
                        (0.58, 0.72),
                        (0.65, 0.65),
                        (0.68, 0.42),
                        (0.70, 0.25),
                        (0.60, 0.22),
                        (0.40, 0.22),
                    ],
                ),
                region(
                    RegionKind::LeftEye,
                    &[
                        (0.36, 0.38),
                        (0.38, 0.36),
                        (0.42, 0.36),
                        (0.44, 0.38),
                        (0.42, 0.40),
                        (0.38, 0.40),
                    ],
                ),
                region(
                    RegionKind::RightEye,
                    &[
                        (0.56, 0.38),
                        (0.58, 0.36),
                        (0.62, 0.36),
                        (0.64, 0.38),
                        (0.62, 0.40),
                        (0.58, 0.40),
                    ],
                ),
                region(
                    RegionKind::Nose,
                    &[
                        (0.46, 0.40),
                        (0.47, 0.46),
                        (0.45, 0.50),
                        (0.48, 0.52),
                        (0.52, 0.52),
                        (0.55, 0.50),
                        (0.53, 0.46),
                    ],
                ),
                region(
                    RegionKind::OuterLips,
                    &[
                        (0.42, 0.58),
                        (0.45, 0.56),
                        (0.5, 0.57),
                        (0.55, 0.56),
                        (0.58, 0.58),
                        (0.55, 0.62),
                        (0.5, 0.64),
                        (0.45, 0.62),
                    ],
                ),
                region(
                    RegionKind::InnerLips,
                    &[
                        (0.45, 0.59),
                        (0.48, 0.585),
                        (0.52, 0.585),
                        (0.55, 0.59),
                        (0.52, 0.61),
                        (0.48, 0.61),
                    ],
                ),
            ],
            ..Default::default()
        }
    }

    fn provider(dense_verified: bool) -> ProviderDescriptor {
        let caps = ProviderCapabilities {
            face_detection: true,
            dense_landmarks: true,
            eye_landmarks: true,
            eyelid_landmarks: true,
            nose_landmarks: true,
            lip_landmarks: true,
            ..Default::default()
        };
        let evidence = vec![
            CapabilityEvidence::new(
                "dense_landmarks",
                if dense_verified {
                    EvidenceLevel::Verified
                } else {
                    EvidenceLevel::Declared
                },
                "test",
            ),
            CapabilityEvidence::new("eye_landmarks", EvidenceLevel::Verified, "test"),
            CapabilityEvidence::new("eyelid_landmarks", EvidenceLevel::Verified, "test"),
            CapabilityEvidence::new("nose_landmarks", EvidenceLevel::Verified, "test"),
            CapabilityEvidence::new("lip_landmarks", EvidenceLevel::Verified, "test"),
        ];
        ProviderDescriptor {
            id: "apple-vision".into(),
            display_name: "Apple Vision".into(),
            version: "1".into(),
            kind: ProviderKind::System,
            state: ProviderState::Ready,
            available: true,
            enabled: true,
            license: LicenseInfo::default(),
            capabilities: caps,
            verified_capabilities: caps.verified_only(&evidence),
            evidence,
            topology_id: Some("apple-vision-landmarks".into()),
            note: String::new(),
        }
    }

    fn stable(n: usize) -> Vec<Vec<DenseFace>> {
        vec![vec![dense()]; n]
    }

    #[test]
    fn synthetic_dense_passes_structure_checks() {
        let quality = evaluate_face_quality(&dense(), &QualityThresholds::default());
        assert!(quality.passed, "{:?}", quality.issues);
    }

    #[test]
    fn identical_runs_have_zero_deviation() {
        let report = measure_stability(
            "apple-vision",
            "apple-vision-landmarks",
            &stable(3),
            &[10, 9, 9],
            &QualityThresholds::default(),
        );
        assert!(report.passed);
        assert_eq!(report.max_deviation, 0.0);
    }

    #[test]
    fn jitter_above_threshold_fails() {
        let base = dense();
        let mut shifted = dense();
        shifted.regions[1].points[0].x += 0.02;
        let report = measure_stability(
            "apple-vision",
            "apple-vision-landmarks",
            &[vec![base], vec![shifted]],
            &[1, 1],
            &QualityThresholds::default(),
        );
        assert!(!report.passed);
        assert!(report.ratio > 0.02);
    }

    #[test]
    fn missing_stability_locks_everything() {
        let gate = level3_gate(
            Some(&dense()),
            &provider(true),
            None,
            &QualityThresholds::default(),
        );
        assert!(!gate.eyes.enabled && !gate.nose.enabled && !gate.mouth.enabled);
        assert!(gate.summary.contains("稳定性"));
    }

    #[test]
    fn declared_only_provider_is_locked() {
        let report = measure_stability(
            "apple-vision",
            "apple-vision-landmarks",
            &stable(3),
            &[1, 1, 1],
            &QualityThresholds::default(),
        );
        let gate = level3_gate(
            Some(&dense()),
            &provider(false),
            Some(&report),
            &QualityThresholds::default(),
        );
        assert!(!gate.eyes.enabled);
        assert!(gate.summary.contains("尚未通过实测验证"));
    }

    #[test]
    fn verified_provider_with_stability_unlocks_all_regions() {
        let report = measure_stability(
            "apple-vision",
            "apple-vision-landmarks",
            &stable(3),
            &[1, 1, 1],
            &QualityThresholds::default(),
        );
        let gate = level3_gate(
            Some(&dense()),
            &provider(true),
            Some(&report),
            &QualityThresholds::default(),
        );
        assert!(gate.eyes.enabled, "{}", gate.eyes.reason);
        assert!(gate.nose.enabled, "{}", gate.nose.reason);
        assert!(gate.mouth.enabled, "{}", gate.mouth.reason);
        assert!(gate.chin.enabled, "{}", gate.chin.reason);
    }

    #[test]
    fn flipped_eye_nose_mouth_order_is_rejected() {
        let mut broken = dense();
        for region in &mut broken.regions {
            if region.kind == RegionKind::Nose {
                for point in &mut region.points {
                    point.y = 0.2;
                }
            }
        }
        let quality = evaluate_face_quality(&broken, &QualityThresholds::default());
        assert!(!quality.passed);
        assert!(quality.issues.iter().any(|issue| issue.contains("顺序")));
    }

    #[test]
    fn confidence_distribution_is_numeric() {
        let mut faces = vec![dense()];
        let mut low = dense();
        low.confidence = 0.2;
        faces.push(low);
        let distribution = confidence_distribution(&faces, 0.45);
        assert_eq!(distribution.samples, 2);
        assert_eq!(distribution.below_threshold, 1);
        assert!(distribution.min < distribution.mean);
    }
}
