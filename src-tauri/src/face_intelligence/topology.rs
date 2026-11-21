//! 人脸拓扑注册表。
//!
//! 拓扑是“语义 + 邻接 + 坐标定义”的独立数据，不是某个模型的数组下标。
//! 变形引擎只消费语义 ID（例如 `eye.left.lid.0`），Provider 负责把模型输出
//! 映射到拓扑。没有真实输出的 68/126 点不会被任何 Provider 伪造。

use std::sync::LazyLock;

use serde::Serialize;

use crate::face_intelligence::types::{DenseFace, RegionKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CoordinateSpace {
    /// Provider 直接输出原图归一化坐标（0..1，左上角原点）。
    ImageNormalized,
    /// Provider 输出裁剪区域归一化坐标，由 Provider 转换为原图坐标后交付。
    CropNormalized,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegionSpec {
    pub kind: RegionKind,
    pub min_points: usize,
    pub max_points: usize,
    pub closed: bool,
    pub note: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Topology {
    pub id: &'static str,
    pub version: &'static str,
    /// 固定点数；运行时点数可能因模型 revision 变化时为 `None`。
    pub point_count: Option<usize>,
    pub dense: bool,
    pub coordinate_space: CoordinateSpace,
    pub source: &'static str,
    pub regions: Vec<RegionSpec>,
}

impl Topology {
    pub fn region(&self, kind: RegionKind) -> Option<&RegionSpec> {
        self.regions.iter().find(|spec| spec.kind == kind)
    }

    /// 校验实际输出是否落在该拓扑声明的范围内。
    /// 只检查结构与数量，不把“点数对”误当成“语义对”。
    pub fn validate(&self, dense: &DenseFace) -> Vec<String> {
        let mut issues = Vec::new();
        if dense.topology_id != self.id {
            issues.push(format!(
                "拓扑不匹配：期望 {}，实际 {}",
                self.id, dense.topology_id
            ));
        }
        for region in &dense.regions {
            match self.region(region.kind) {
                Some(spec) => {
                    if region.points.len() < spec.min_points
                        || region.points.len() > spec.max_points
                    {
                        issues.push(format!(
                            "{} 点数 {} 超出拓扑允许范围 {}..{}",
                            region.kind.label(),
                            region.points.len(),
                            spec.min_points,
                            spec.max_points
                        ));
                    }
                }
                None => issues.push(format!("拓扑未声明区域 {}", region.kind.label())),
            }
        }
        issues
    }
}

fn spec(
    kind: RegionKind,
    min_points: usize,
    max_points: usize,
    closed: bool,
    note: &'static str,
) -> RegionSpec {
    RegionSpec {
        kind,
        min_points,
        max_points,
        closed,
        note,
    }
}

/// 随包 YuNet：只有 5 个锚点，不参与稠密拓扑校验。
pub static YUNET5: LazyLock<Topology> = LazyLock::new(|| Topology {
    id: "yunet-5",
    version: "2023mar",
    point_count: Some(5),
    dense: false,
    coordinate_space: CoordinateSpace::ImageNormalized,
    source: "OpenCV Zoo YuNet 2023mar（MIT）",
    regions: Vec::new(),
});

/// Apple Vision 人脸关键点（macOS 系统框架）。
/// 区域点数在不同 revision 间会变化，因此使用范围而不是硬编码 76。
pub static APPLE_VISION: LazyLock<Topology> = LazyLock::new(|| Topology {
    id: "apple-vision-landmarks",
    version: "VNDetectFaceLandmarksRequest",
    point_count: None,
    dense: true,
    coordinate_space: CoordinateSpace::ImageNormalized,
    source: "Apple Vision（系统框架，无随包权重）",
    regions: vec![
        spec(
            RegionKind::FaceContour,
            15,
            20,
            true,
            "脸缘轮廓，含下颌与下巴",
        ),
        spec(RegionKind::LeftEye, 4, 9, true, "左眼上下眼睑与内外眼角"),
        spec(RegionKind::RightEye, 4, 9, true, "右眼上下眼睑与内外眼角"),
        spec(RegionKind::LeftEyebrow, 4, 9, false, "左眉轮廓"),
        spec(RegionKind::RightEyebrow, 4, 9, false, "右眉轮廓"),
        spec(RegionKind::Nose, 6, 11, false, "鼻部轮廓，含鼻翼外侧点"),
        spec(RegionKind::NoseCrest, 4, 8, false, "鼻梁中线"),
        spec(RegionKind::MedianLine, 8, 12, false, "面部中线"),
        spec(RegionKind::OuterLips, 12, 22, true, "外唇轮廓"),
        spec(
            RegionKind::InnerLips,
            5,
            10,
            true,
            "内唇轮廓（牙齿保护参考）",
        ),
    ],
});

/// iBUG 300-W 68 点拓扑定义（仅结构，不附带权重）。
pub static FACE68: LazyLock<Topology> = LazyLock::new(|| Topology {
    id: "face-68",
    version: "iBUG-300W",
    point_count: Some(68),
    dense: true,
    coordinate_space: CoordinateSpace::ImageNormalized,
    source: "iBUG 300-W 标注约定（仅拓扑定义，未随包权重）",
    regions: vec![
        spec(RegionKind::FaceContour, 17, 17, false, "0..16 下颌线"),
        spec(RegionKind::RightEyebrow, 5, 5, false, "17..21"),
        spec(RegionKind::LeftEyebrow, 5, 5, false, "22..26"),
        spec(RegionKind::Nose, 9, 9, false, "27..35 鼻梁与鼻底"),
        spec(RegionKind::RightEye, 6, 6, true, "36..41"),
        spec(RegionKind::LeftEye, 6, 6, true, "42..47"),
        spec(RegionKind::OuterLips, 12, 12, true, "48..59"),
        spec(RegionKind::InnerLips, 8, 8, true, "60..67"),
    ],
});

/// NVIDIA Maxine 126 点。没有取得官方逐点语义文档前，只登记结构，
/// 不把索引映射成眼睑/鼻翼等未经确认的语义。
pub static FACE126: LazyLock<Topology> = LazyLock::new(|| Topology {
    id: "face-126",
    version: "nvidia-maxine-ar",
    point_count: Some(126),
    dense: true,
    coordinate_space: CoordinateSpace::ImageNormalized,
    source: "NVIDIA Maxine AR 文档（未接入，逐点语义待供应商确认）",
    regions: Vec::new(),
});

static TOPOLOGIES: LazyLock<Vec<&'static Topology>> =
    LazyLock::new(|| vec![&YUNET5, &APPLE_VISION, &FACE68, &FACE126]);

pub fn topology(id: &str) -> Option<&'static Topology> {
    TOPOLOGIES.iter().copied().find(|item| item.id == id)
}

/// iBUG 68 点的索引区段。只在这里集中定义，避免推理代码散落魔术数字。
pub fn face68_regions() -> Vec<(RegionKind, std::ops::Range<usize>)> {
    use std::ops::Range;
    vec![
        (RegionKind::FaceContour, Range { start: 0, end: 17 }),
        (RegionKind::RightEyebrow, Range { start: 17, end: 22 }),
        (RegionKind::LeftEyebrow, Range { start: 22, end: 27 }),
        (RegionKind::Nose, Range { start: 27, end: 36 }),
        (RegionKind::RightEye, Range { start: 36, end: 42 }),
        (RegionKind::LeftEye, Range { start: 42, end: 48 }),
        (RegionKind::OuterLips, Range { start: 48, end: 60 }),
        (RegionKind::InnerLips, Range { start: 60, end: 68 }),
    ]
}

/// 68 点索引 → 语义 ID。越界返回 `None`，绝不猜测。
pub fn face68_semantic(index: usize) -> Option<(RegionKind, String)> {
    for (kind, range) in face68_regions() {
        if range.contains(&index) {
            let local = index - range.start;
            return Some((kind, format!("{}.{}", kind.prefix(), local)));
        }
    }
    None
}

/// 分组后的语义区域：区域类型 + 该区域的点（归一化坐标与置信度）。
pub type GroupedRegions = Vec<(RegionKind, Vec<(f32, f32, f32)>)>;

/// 把 68 个点按语义区域分组，供 Provider 输出统一 `DenseRegion`。
pub fn group_face68(points: &[(f32, f32, f32)]) -> Result<GroupedRegions, String> {
    if points.len() != 68 {
        return Err(format!("face-68 需要 68 个点，实际 {}", points.len()));
    }
    Ok(face68_regions()
        .into_iter()
        .map(|(kind, range)| (kind, points[range].to_vec()))
        .collect())
}

pub fn all() -> Vec<&'static Topology> {
    TOPOLOGIES.iter().copied().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::face_intelligence::types::{DenseRegion, Landmark};

    fn dense(topology_id: &str, points: usize) -> DenseFace {
        DenseFace {
            topology_id: topology_id.into(),
            provider_id: "test".into(),
            regions: vec![DenseRegion {
                kind: RegionKind::LeftEye,
                points: (0..points)
                    .map(|index| Landmark::new(0.4, 0.4, 0.9, format!("eye.left.lid.{index}")))
                    .collect(),
                closed: true,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn all_topology_ids_are_unique() {
        let mut ids = std::collections::BTreeSet::new();
        for item in all() {
            assert!(ids.insert(item.id), "{} 重复", item.id);
        }
    }

    #[test]
    fn apple_vision_accepts_six_point_eye_but_rejects_two() {
        let apple = topology("apple-vision-landmarks").unwrap();
        assert!(apple
            .validate(&dense("apple-vision-landmarks", 6))
            .is_empty());
        assert!(!apple
            .validate(&dense("apple-vision-landmarks", 2))
            .is_empty());
    }

    #[test]
    fn wrong_topology_id_is_reported() {
        let apple = topology("apple-vision-landmarks").unwrap();
        let issues = apple.validate(&dense("face-68", 6));
        assert!(issues.iter().any(|issue| issue.contains("拓扑不匹配")));
    }

    #[test]
    fn face68_declares_68_points_and_regions() {
        let face = topology("face-68").unwrap();
        assert_eq!(face.point_count, Some(68));
        let total: usize = face.regions.iter().map(|spec| spec.max_points).sum();
        assert_eq!(total, 68);
    }

    #[test]
    fn face126_has_no_fabricated_semantics() {
        let face = topology("face-126").unwrap();
        assert_eq!(face.point_count, Some(126));
        assert!(face.regions.is_empty());
    }
}
