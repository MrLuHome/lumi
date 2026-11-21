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
    pub provider_version: String,
    pub topology_id: String,
    pub model_version: String,
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
            provider_version: String::new(),
            topology_id: String::new(),
            model_version: String::new(),
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

    /// 门控是否由同一 Provider / 拓扑 / 模型版本产生。
    /// 项目恢复或切换 Provider 后用它拒绝复用旧结论。
    pub fn matches_identity(
        &self,
        provider_id: &str,
        provider_version: &str,
        topology_id: &str,
        model_version: &str,
    ) -> bool {
        !self.provider_id.is_empty()
            && !self.topology_id.is_empty()
            && self.provider_id == provider_id
            && self.provider_version == provider_version
            && self.topology_id == topology_id
            && self.model_version == model_version
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

/// 分区域的结构问题。公共问题（置信度、总点数、五官顺序）单独保存，
/// 避免某个区域失败时连带锁死其他合格区域。
#[derive(Debug, Clone, Default)]
pub struct RegionIssues {
    pub eyes: Vec<String>,
    pub nose: Vec<String>,
    pub mouth: Vec<String>,
    pub chin: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct FaceChecks {
    pub common: Vec<String>,
    pub regions: RegionIssues,
}

impl FaceChecks {
    pub fn all_issues(&self) -> Vec<String> {
        let mut issues = self.common.clone();
        issues.extend(self.regions.eyes.iter().cloned());
        issues.extend(self.regions.nose.iter().cloned());
        issues.extend(self.regions.mouth.iter().cloned());
        issues.extend(self.regions.chin.iter().cloned());
        issues
    }

    pub fn common_passed(&self) -> bool {
        self.common.is_empty()
    }

    pub fn region_passed(&self, kind: RegionKind) -> bool {
        match kind {
            RegionKind::LeftEye | RegionKind::RightEye => self.regions.eyes.is_empty(),
            RegionKind::Nose => self.regions.nose.is_empty(),
            RegionKind::OuterLips | RegionKind::InnerLips => self.regions.mouth.is_empty(),
            RegionKind::FaceContour => self.regions.chin.is_empty(),
            _ => true,
        }
    }
}

/// 结构化检查：公共检查 + 四个可独立门控的区域。
pub fn evaluate_checks(dense: &DenseFace, thresholds: &QualityThresholds) -> FaceChecks {
    let mut checks = FaceChecks::default();

    if dense.confidence < thresholds.min_confidence {
        checks.common.push(format!(
            "人脸置信度 {:.3} 低于阈值 {:.2}",
            dense.confidence, thresholds.min_confidence
        ));
    }
    let points = dense.point_count();
    if points < thresholds.min_dense_points {
        checks.common.push(format!(
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
                checks.regions.eyes.push(format!(
                    "眼睑轮廓宽度 {:.4} 小于 {:.4}，不足以驱动精修",
                    width, thresholds.min_eye_width
                ));
                false
            } else {
                true
            }
        }
        _ => {
            checks
                .regions
                .eyes
                .push("缺少左右眼睑轮廓（各至少 4 点）".into());
            false
        }
    };

    let nose = dense.region(RegionKind::Nose);
    match nose {
        Some(region) if region.points.len() >= 6 => {
            if width_of(region) < thresholds.min_eye_width * 0.8 {
                checks
                    .regions
                    .nose
                    .push("鼻部轮廓横向跨度不足，无法区分鼻翼".into());
            }
        }
        _ => checks.regions.nose.push("缺少鼻部轮廓（至少 6 点）".into()),
    }

    let outer = dense.region(RegionKind::OuterLips);
    let inner = dense.region(RegionKind::InnerLips);
    match (outer, inner) {
        (Some(outer), Some(inner)) if outer.points.len() >= 8 && inner.points.len() >= 3 => {
            if width_of(outer) < thresholds.min_mouth_width {
                checks.regions.mouth.push("唇部轮廓宽度不足".into());
            }
        }
        _ => checks.regions.mouth.push("缺少完整外唇/内唇轮廓".into()),
    }

    let contour = dense.region(RegionKind::FaceContour);
    match contour {
        Some(region) if region.points.len() >= 9 => {}
        _ => checks.regions.chin.push("缺少脸缘轮廓（至少 9 点）".into()),
    }

    // 纵向顺序只有四大锚点都可用时才检查；缺少某区域由该区域自己报错，
    // 不用“顺序异常”连带锁定其他区域。
    if eye_ok {
        if let (Some(left), Some(right), Some(nose), Some(outer), Some(contour)) =
            (eye_left, eye_right, nose, outer, contour)
        {
            if !left.points.is_empty()
                && !right.points.is_empty()
                && !nose.points.is_empty()
                && !outer.points.is_empty()
                && !contour.points.is_empty()
            {
                let eye_y = (mean_point(left).1 + mean_point(right).1) * 0.5;
                let nose_y = mean_point(nose).1;
                let mouth_y = mean_point(outer).1;
                let chin_y = contour
                    .points
                    .iter()
                    .map(|point| point.y)
                    .fold(f32::NEG_INFINITY, f32::max);
                if !(eye_y < nose_y && nose_y < mouth_y && mouth_y < chin_y) {
                    checks
                        .common
                        .push("五官纵向顺序异常（可能坐标翻转或标签错误）".into());
                }
            }
        }
    }
    checks
}

/// 单张人脸的稠密质量检查。`passed` 仍表示整体通过，
/// 但 `issues` 不再用于推断某个区域是否合格；区域结论由 `evaluate_checks` 决定。
pub fn evaluate_face_quality(dense: &DenseFace, thresholds: &QualityThresholds) -> FaceQuality {
    let checks = evaluate_checks(dense, thresholds);
    let mut regions = Vec::new();
    for kind in [
        RegionKind::LeftEye,
        RegionKind::RightEye,
        RegionKind::Nose,
        RegionKind::OuterLips,
        RegionKind::InnerLips,
        RegionKind::FaceContour,
    ] {
        let count = dense
            .region(kind)
            .map(|region| region.points.len())
            .unwrap_or(0);
        let ok = checks.region_passed(kind);
        let reason = match kind {
            RegionKind::LeftEye | RegionKind::RightEye => checks.regions.eyes.first(),
            RegionKind::Nose => checks.regions.nose.first(),
            RegionKind::OuterLips | RegionKind::InnerLips => checks.regions.mouth.first(),
            RegionKind::FaceContour => checks.regions.chin.first(),
            _ => None,
        }
        .cloned()
        .unwrap_or_else(|| "结构完整".into());
        regions.push(RegionQuality {
            region: kind.label().into(),
            points: count,
            passed: ok,
            reason,
        });
    }

    let passed = checks.common_passed()
        && checks.regions.eyes.is_empty()
        && checks.regions.nose.is_empty()
        && checks.regions.mouth.is_empty()
        && checks.regions.chin.is_empty();
    FaceQuality {
        passed,
        dense_points: dense.point_count(),
        confidence: dense.confidence,
        eye_distance: eye_distance(dense),
        regions,
        issues: checks.all_issues(),
    }
}

/// 逐项 LEVEL 3 门控。`stability` 必须是本机实测；`None` 一律禁用。
pub fn level3_gate(
    dense: Option<&DenseFace>,
    provider: &ProviderDescriptor,
    stability: Option<&StabilityReport>,
    thresholds: &QualityThresholds,
    model_version: &str,
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
            provider_version: provider.version.clone(),
            topology_id: dense.topology_id.clone(),
            model_version: model_version.into(),
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

    let checks = evaluate_checks(dense, thresholds);
    let stable_note = format!(
        "{} 次重复最大偏差 {:.3}% 眼距",
        stability.runs,
        stability.ratio * 100.0
    );

    // 公共安全检查（置信度、总点数、五官顺序）失败时整体锁定。
    if !checks.common_passed() {
        let reason = checks.common.join("；");
        return Level3Gate {
            provider_id: provider.id.clone(),
            provider_version: provider.version.clone(),
            topology_id: dense.topology_id.clone(),
            model_version: model_version.into(),
            face_uuid: String::new(),
            dense_landmarks: true,
            eyes: FeatureGate::locked(format!("公共安全检查未通过：{reason}")),
            nose: FeatureGate::locked(format!("公共安全检查未通过：{reason}")),
            mouth: FeatureGate::locked(format!("公共安全检查未通过：{reason}")),
            chin: FeatureGate::locked(format!("公共安全检查未通过：{reason}")),
            summary: "公共安全检查未通过，LEVEL 3 保持禁用".into(),
            issues: checks.all_issues(),
        };
    }

    let verified = provider.verified_capabilities;
    let gate = |capability: bool, issues: &[String], region: &str| {
        if !capability {
            FeatureGate::locked(format!("{region}：Provider 未声明并验证该区域能力"))
        } else if issues.is_empty() {
            FeatureGate::open(format!("{region}：{stable_note}，结构校验通过"))
        } else {
            FeatureGate::locked(format!("{region}：{}", issues[0]))
        }
    };

    let eyes = gate(
        verified.eye_landmarks && verified.eyelid_landmarks,
        &checks.regions.eyes,
        "眼部",
    );
    let nose = gate(verified.nose_landmarks, &checks.regions.nose, "鼻部");
    let mouth = gate(verified.lip_landmarks, &checks.regions.mouth, "唇部");
    // 下巴依赖脸缘轮廓，不额外要求独立 chin 能力位。
    let chin = gate(true, &checks.regions.chin, "面部轮廓");

    let mut enabled = Vec::new();
    let mut locked = Vec::new();
    for (name, feature) in [
        ("眼部", &eyes),
        ("鼻部", &nose),
        ("唇部", &mouth),
        ("面部轮廓", &chin),
    ] {
        if feature.enabled {
            enabled.push(name);
        } else {
            locked.push(name);
        }
    }
    let any = !enabled.is_empty();
    Level3Gate {
        provider_id: provider.id.clone(),
        provider_version: provider.version.clone(),
        topology_id: dense.topology_id.clone(),
        model_version: model_version.into(),
        face_uuid: String::new(),
        dense_landmarks: true,
        eyes,
        nose,
        mouth,
        chin,
        summary: if any {
            format!(
                "LEVEL 3 已按实测质量解锁：{}；未解锁：{}（稳定性与结构已验证，定位精度未经人工标注验证）",
                enabled.join("、"),
                if locked.is_empty() {
                    "无".into()
                } else {
                    locked.join("、")
                }
            )
        } else {
            "LEVEL 3 保持禁用：各区域结构校验均未通过".into()
        },
        issues: checks.all_issues(),
    }
}

/// 一次重复分析的完整身份与结果。稳定性必须逐次核对身份，
/// 不能只比较点坐标。
#[derive(Debug, Clone)]
pub struct StabilityRun {
    pub provider_id: String,
    pub topology_id: String,
    pub model_version: String,
    pub faces: Vec<DenseFace>,
}

fn face_center(face: &DenseFace) -> (f32, f32) {
    (
        face.bounding_box.x + face.bounding_box.width * 0.5,
        face.bounding_box.y + face.bounding_box.height * 0.5,
    )
}

fn bbox_iou(a: &DenseFace, b: &DenseFace) -> f32 {
    let ax0 = a.bounding_box.x;
    let ay0 = a.bounding_box.y;
    let ax1 = ax0 + a.bounding_box.width;
    let ay1 = ay0 + a.bounding_box.height;
    let bx0 = b.bounding_box.x;
    let by0 = b.bounding_box.y;
    let bx1 = bx0 + b.bounding_box.width;
    let by1 = by0 + b.bounding_box.height;
    let inter = (ax1.min(bx1) - ax0.max(bx0)).max(0.0) * (ay1.min(by1) - ay0.max(by0)).max(0.0);
    let union = (ax1 - ax0).max(0.0) * (ay1 - ay0).max(0.0)
        + (bx1 - bx0).max(0.0) * (by1 - by0).max(0.0)
        - inter;
    if union <= 1e-9 {
        0.0
    } else {
        inter / union
    }
}

/// 一对一匹配：优先 IoU，其次中心距离。绝不按数组下标直接配对。
fn match_face(base: &DenseFace, candidates: &[DenseFace], used: &[bool]) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    let (bx, by) = face_center(base);
    for (index, candidate) in candidates.iter().enumerate() {
        if used[index] {
            continue;
        }
        let iou = bbox_iou(base, candidate);
        let (cx, cy) = face_center(candidate);
        let distance = ((bx - cx).powi(2) + (by - cy).powi(2)).sqrt();
        let acceptable = iou >= 0.30 || distance <= 0.15;
        if !acceptable {
            continue;
        }
        // 分数越大越好：IoU 为主，距离做微调。
        let score = iou - distance * 0.1;
        if best.map(|(_, current)| score > current).unwrap_or(true) {
            best = Some((index, score));
        }
    }
    best.map(|(index, _)| index)
}

fn points_valid(face: &DenseFace) -> bool {
    let bbox = &face.bounding_box;
    if !bbox.x.is_finite()
        || !bbox.y.is_finite()
        || !bbox.width.is_finite()
        || !bbox.height.is_finite()
        || bbox.width <= 0.0
        || bbox.height <= 0.0
    {
        return false;
    }
    face.regions.iter().all(|region| {
        region.points.iter().all(|point| {
            point.x.is_finite()
                && point.y.is_finite()
                && (-0.01..=1.01).contains(&point.x)
                && (-0.01..=1.01).contains(&point.y)
        })
    })
}

/// 由重复分析计算稳定性。
///
/// - 每次运行必须与期望的 Provider / 拓扑 / 模型版本一致；
/// - 人脸按 IoU + 中心距离一对一匹配，不依赖数组顺序；
/// - 每个语义区域必须存在且点数完全一致，点数不同直接失败；
/// - 所有点必须有限且在 [0,1] 附近，否则失败；
/// - 该报告只说明“重复一致性”，不代表定位精度。
pub fn measure_stability(
    expected_provider: &str,
    expected_topology: &str,
    expected_model: &str,
    runs: &[StabilityRun],
    elapsed_ms: &[u64],
    thresholds: &QualityThresholds,
) -> StabilityReport {
    let counts: Vec<usize> = runs.iter().map(|run| run.faces.len()).collect();
    let mut issues = Vec::new();
    if runs.len() < 2 {
        issues.push("至少需要 2 次重复分析".into());
    }
    if !counts.is_empty() && counts.iter().any(|count| *count != counts[0]) {
        issues.push(format!("重复分析人脸数量不一致：{counts:?}"));
    }
    for (index, run) in runs.iter().enumerate() {
        if run.provider_id != expected_provider {
            issues.push(format!(
                "第 {} 次重复 Provider 不一致：{} != {}",
                index + 1,
                run.provider_id,
                expected_provider
            ));
        }
        if run.topology_id != expected_topology || expected_topology.is_empty() {
            issues.push(format!(
                "第 {} 次重复拓扑不一致：{} != {}",
                index + 1,
                run.topology_id,
                expected_topology
            ));
        }
        if run.model_version != expected_model {
            issues.push(format!(
                "第 {} 次重复模型版本不一致：{} != {}",
                index + 1,
                run.model_version,
                expected_model
            ));
        }
        if !run.faces.iter().all(points_valid) {
            issues.push(format!("第 {} 次重复存在非有限或越界关键点", index + 1));
        }
    }

    let mut max_deviation = 0.0f32;
    let mut eye = 0.0f32;
    if let Some(first) = runs.first() {
        for face in &first.faces {
            eye = eye.max(eye_distance(face));
        }
        for (run_index, run) in runs.iter().enumerate().skip(1) {
            let mut used = vec![false; run.faces.len()];
            for (base_index, base) in first.faces.iter().enumerate() {
                let Some(other_index) = match_face(base, &run.faces, &used) else {
                    issues.push(format!(
                        "第 {} 次重复缺少可匹配的第 {} 张人脸（不按数组下标配对）",
                        run_index + 1,
                        base_index + 1
                    ));
                    continue;
                };
                used[other_index] = true;
                let other = &run.faces[other_index];

                for region in &base.regions {
                    let Some(other_region) = other.region(region.kind) else {
                        issues.push(format!(
                            "第 {} 次重复缺少 {} 区域",
                            run_index + 1,
                            region.kind.label()
                        ));
                        continue;
                    };
                    if region.points.len() != other_region.points.len() {
                        issues.push(format!(
                            "第 {} 次重复的 {} 点数不一致：{} != {}",
                            run_index + 1,
                            region.kind.label(),
                            other_region.points.len(),
                            region.points.len()
                        ));
                        continue;
                    }
                    for (a, b) in region.points.iter().zip(other_region.points.iter()) {
                        max_deviation =
                            max_deviation.max(((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt());
                    }
                }
                for region in &other.regions {
                    if base.region(region.kind).is_none() {
                        issues.push(format!(
                            "第 {} 次重复多出 {} 区域",
                            run_index + 1,
                            region.kind.label()
                        ));
                    }
                }
            }
            let unmatched = used.iter().filter(|used| !**used).count();
            if unmatched > 0 {
                issues.push(format!(
                    "第 {} 次重复有 {} 张人脸无法与首轮匹配",
                    run_index + 1,
                    unmatched
                ));
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
            "{} 次重复，最大偏差 {:.3}% 眼距（阈值 {:.2}%）；该指标只衡量重复一致性，不代表定位精度",
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
        provider_id: expected_provider.into(),
        topology_id: expected_topology.into(),
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

    fn stable(n: usize) -> Vec<StabilityRun> {
        vec![
            StabilityRun {
                provider_id: "apple-vision".into(),
                topology_id: "apple-vision-landmarks".into(),
                model_version: "system-1".into(),
                faces: vec![dense()],
            };
            n
        ]
    }

    fn gate_with(dense: &DenseFace, runs: &[StabilityRun]) -> Level3Gate {
        let stability = measure_stability(
            "apple-vision",
            "apple-vision-landmarks",
            "system-1",
            runs,
            &[1],
            &QualityThresholds::default(),
        );
        level3_gate(
            Some(dense),
            &provider(true),
            Some(&stability),
            &QualityThresholds::default(),
            "system-1",
        )
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
            "system-1",
            &stable(3),
            &[10, 9, 9],
            &QualityThresholds::default(),
        );
        assert!(report.passed);
        assert_eq!(report.max_deviation, 0.0);
    }

    #[test]
    fn jitter_above_threshold_fails() {
        let mut shifted = stable(2);
        shifted[1].faces[0].regions[1].points[0].x += 0.02;
        let report = measure_stability(
            "apple-vision",
            "apple-vision-landmarks",
            "system-1",
            &shifted,
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
            "system-1",
        );
        assert!(!gate.eyes.enabled && !gate.nose.enabled && !gate.mouth.enabled);
        assert!(gate.summary.contains("稳定性"));
    }

    #[test]
    fn declared_only_provider_is_locked() {
        let report = measure_stability(
            "apple-vision",
            "apple-vision-landmarks",
            "system-1",
            &stable(3),
            &[1, 1, 1],
            &QualityThresholds::default(),
        );
        let gate = level3_gate(
            Some(&dense()),
            &provider(false),
            Some(&report),
            &QualityThresholds::default(),
            "system-1",
        );
        assert!(!gate.eyes.enabled);
        assert!(gate.summary.contains("尚未通过实测验证"));
    }

    #[test]
    fn verified_provider_with_stability_unlocks_all_regions() {
        let gate = gate_with(&dense(), &stable(3));
        assert!(gate.eyes.enabled, "{}", gate.eyes.reason);
        assert!(gate.nose.enabled, "{}", gate.nose.reason);
        assert!(gate.mouth.enabled, "{}", gate.mouth.reason);
        assert!(gate.chin.enabled, "{}", gate.chin.reason);
        assert!(gate.summary.contains("定位精度未经人工标注验证"));
        assert_eq!(gate.model_version, "system-1");
        assert!(!gate.provider_version.is_empty());
    }

    #[test]
    fn single_region_failure_does_not_lock_others() {
        let mut broken = dense();
        broken
            .regions
            .retain(|region| region.kind != RegionKind::Nose);
        let gate = gate_with(&broken, &stable(2));
        assert!(!gate.nose.enabled, "鼻部应因缺区域锁定");
        assert!(gate.nose.reason.contains("鼻部"));
        assert!(
            gate.eyes.enabled,
            "眼部不应被连带锁定：{}",
            gate.eyes.reason
        );
        assert!(
            gate.mouth.enabled,
            "唇部不应被连带锁定：{}",
            gate.mouth.reason
        );
        assert!(
            gate.chin.enabled,
            "下巴不应被连带锁定：{}",
            gate.chin.reason
        );
        assert!(gate.summary.contains("未解锁：鼻部"));
    }

    #[test]
    fn multiple_region_failures_only_lock_those() {
        let mut broken = dense();
        broken.regions.retain(|region| {
            region.kind != RegionKind::Nose && region.kind != RegionKind::InnerLips
        });
        let gate = gate_with(&broken, &stable(2));
        assert!(!gate.nose.enabled);
        assert!(!gate.mouth.enabled);
        assert!(gate.eyes.enabled, "{}", gate.eyes.reason);
        assert!(gate.chin.enabled, "{}", gate.chin.reason);
    }

    #[test]
    fn common_confidence_failure_locks_all_regions() {
        let mut low = dense();
        low.confidence = 0.2;
        let gate = gate_with(&low, &stable(2));
        assert!(
            !gate.eyes.enabled && !gate.nose.enabled && !gate.mouth.enabled && !gate.chin.enabled
        );
        assert!(gate.eyes.reason.contains("公共安全检查"));
        assert!(gate.summary.contains("公共安全检查"));
    }

    #[test]
    fn face_order_change_matches_by_geometry() {
        let mut second = dense();
        second.bounding_box.x = 0.45;
        for region in &mut second.regions {
            for point in &mut region.points {
                point.x += 0.20;
            }
        }
        let mut first = stable(1);
        first[0].faces = vec![dense(), second.clone()];
        let mut reordered = first[0].clone();
        reordered.faces = vec![second, dense()];
        let report = measure_stability(
            "apple-vision",
            "apple-vision-landmarks",
            "system-1",
            &[first.remove(0), reordered],
            &[1, 1],
            &QualityThresholds::default(),
        );
        assert!(report.passed, "{}", report.note);
        assert_eq!(report.faces_per_run, vec![2, 2]);
        assert_eq!(report.max_deviation, 0.0);
    }

    #[test]
    fn region_point_count_change_fails() {
        let mut changed = stable(2);
        changed[1].faces[0].regions[3].points.pop();
        let report = measure_stability(
            "apple-vision",
            "apple-vision-landmarks",
            "system-1",
            &changed,
            &[1, 1],
            &QualityThresholds::default(),
        );
        assert!(!report.passed);
        assert!(report.note.contains("点数不一致"), "{}", report.note);
    }

    #[test]
    fn missing_region_fails() {
        let mut changed = stable(2);
        changed[1].faces[0]
            .regions
            .retain(|region| region.kind != RegionKind::OuterLips);
        let report = measure_stability(
            "apple-vision",
            "apple-vision-landmarks",
            "system-1",
            &changed,
            &[1, 1],
            &QualityThresholds::default(),
        );
        assert!(!report.passed);
        assert!(report.note.contains("缺少"), "{}", report.note);
    }

    #[test]
    fn topology_mismatch_fails() {
        let mut changed = stable(2);
        changed[1].topology_id = "face-68".into();
        let report = measure_stability(
            "apple-vision",
            "apple-vision-landmarks",
            "system-1",
            &changed,
            &[1, 1],
            &QualityThresholds::default(),
        );
        assert!(!report.passed);
        assert!(report.note.contains("拓扑不一致"), "{}", report.note);
    }

    #[test]
    fn model_version_mismatch_fails() {
        let mut changed = stable(2);
        changed[1].model_version = "system-2".into();
        let report = measure_stability(
            "apple-vision",
            "apple-vision-landmarks",
            "system-1",
            &changed,
            &[1, 1],
            &QualityThresholds::default(),
        );
        assert!(!report.passed);
        assert!(report.note.contains("模型版本不一致"), "{}", report.note);
    }

    #[test]
    fn non_finite_point_fails() {
        let mut changed = stable(2);
        changed[1].faces[0].regions[2].points[0].y = f32::NAN;
        let report = measure_stability(
            "apple-vision",
            "apple-vision-landmarks",
            "system-1",
            &changed,
            &[1, 1],
            &QualityThresholds::default(),
        );
        assert!(!report.passed);
        assert!(report.note.contains("非有限或越界"), "{}", report.note);
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
