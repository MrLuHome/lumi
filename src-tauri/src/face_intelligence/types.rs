//! 统一人脸分析数据结构。
//!
//! 坐标约定：`x/y` 均为**原图左上角原点、按图像宽高归一化到 0..1**。
//! 每个关键点必须带语义 ID 与置信度；不允许下游依赖数组下标表达五官。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// 归一化人脸框。`x/y` 为左上角，`width/height` 为比例。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BoundingBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl BoundingBox {
    pub fn clamp_unit(self) -> Self {
        Self {
            x: clamp(self.x),
            y: clamp(self.y),
            width: clamp(self.width),
            height: clamp(self.height),
        }
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x + self.width * 0.5, self.y + self.height * 0.5)
    }

    pub fn area(&self) -> f32 {
        self.width.max(0.0) * self.height.max(0.0)
    }
}

fn clamp(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// 面部语义区域类型。Provider 必须按真实输出归类，不能用邻近点插值伪造。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RegionKind {
    FaceContour,
    LeftEye,
    RightEye,
    LeftEyebrow,
    RightEyebrow,
    Nose,
    NoseCrest,
    MedianLine,
    OuterLips,
    InnerLips,
    Iris,
    FaceMesh,
    Other,
}

impl RegionKind {
    /// 语义 ID 前缀。下游按前缀分组，不依赖编号。
    pub fn prefix(self) -> &'static str {
        match self {
            RegionKind::FaceContour => "face.contour",
            RegionKind::LeftEye => "eye.left.lid",
            RegionKind::RightEye => "eye.right.lid",
            RegionKind::LeftEyebrow => "eyebrow.left",
            RegionKind::RightEyebrow => "eyebrow.right",
            RegionKind::Nose => "nose.contour",
            RegionKind::NoseCrest => "nose.crest",
            RegionKind::MedianLine => "face.median",
            RegionKind::OuterLips => "mouth.outer",
            RegionKind::InnerLips => "mouth.inner",
            RegionKind::Iris => "eye.iris",
            RegionKind::FaceMesh => "mesh",
            RegionKind::Other => "other",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            RegionKind::FaceContour => "面部轮廓",
            RegionKind::LeftEye => "左眼轮廓",
            RegionKind::RightEye => "右眼轮廓",
            RegionKind::LeftEyebrow => "左眉",
            RegionKind::RightEyebrow => "右眉",
            RegionKind::Nose => "鼻部轮廓",
            RegionKind::NoseCrest => "鼻梁",
            RegionKind::MedianLine => "面部中线",
            RegionKind::OuterLips => "外唇轮廓",
            RegionKind::InnerLips => "内唇轮廓",
            RegionKind::Iris => "瞳孔",
            RegionKind::FaceMesh => "面部网格",
            RegionKind::Other => "其他",
        }
    }

    pub fn is_dense_feature(self) -> bool {
        matches!(
            self,
            RegionKind::FaceContour
                | RegionKind::LeftEye
                | RegionKind::RightEye
                | RegionKind::Nose
                | RegionKind::OuterLips
                | RegionKind::InnerLips
        )
    }
}

/// 单个语义关键点。`semantic_id` 形如 `eye.left.lid.2`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Landmark {
    pub x: f32,
    pub y: f32,
    pub confidence: f32,
    pub semantic_id: String,
}

impl Landmark {
    pub fn new(x: f32, y: f32, confidence: f32, semantic_id: impl Into<String>) -> Self {
        Self {
            x: clamp(x),
            y: clamp(y),
            confidence: confidence.clamp(0.0, 1.0),
            semantic_id: semantic_id.into(),
        }
    }

    pub fn is_finite(&self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.confidence.is_finite()
    }
}

/// 一条有序轮廓。`closed` 表示首尾相连（眼、唇、脸缘）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DenseRegion {
    pub kind: RegionKind,
    pub points: Vec<Landmark>,
    pub closed: bool,
}

impl DenseRegion {
    pub fn centroid(&self) -> Option<(f32, f32)> {
        if self.points.is_empty() {
            return None;
        }
        let count = self.points.len() as f32;
        let (mut x, mut y) = (0.0, 0.0);
        for point in &self.points {
            x += point.x;
            y += point.y;
        }
        Some((x / count, y / count))
    }

    pub fn confidence(&self) -> f32 {
        if self.points.is_empty() {
            return 0.0;
        }
        self.points
            .iter()
            .map(|point| point.confidence)
            .sum::<f32>()
            / self.points.len() as f32
    }
}

/// 一张人脸的稠密结果。挂到 `FaceFrame.dense` 上，随人脸 UUID 一起流转。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DenseFace {
    pub topology_id: String,
    pub provider_id: String,
    pub provider_version: String,
    pub image_width: u32,
    pub image_height: u32,
    pub bounding_box: BoundingBox,
    pub confidence: f32,
    pub regions: Vec<DenseRegion>,
}

impl DenseFace {
    pub fn region(&self, kind: RegionKind) -> Option<&DenseRegion> {
        self.regions.iter().find(|region| region.kind == kind)
    }

    pub fn point_count(&self) -> usize {
        self.regions.iter().map(|region| region.points.len()).sum()
    }

    pub fn has(&self, kind: RegionKind, minimum: usize) -> bool {
        self.region(kind)
            .map(|region| region.points.len() >= minimum)
            .unwrap_or(false)
    }

    /// 稳定指纹：用于渲染缓存键与 Provider 切换对比。
    pub fn fingerprint(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(self.topology_id.as_bytes());
        hasher.update(self.provider_id.as_bytes());
        for region in &self.regions {
            hasher.update(region.kind.prefix().as_bytes());
            for point in &region.points {
                hasher.update(point.x.to_le_bytes());
                hasher.update(point.y.to_le_bytes());
            }
        }
        format!("{:x}", hasher.finalize())[..16].to_string()
    }

    /// 摊平为带语义 ID 的关键点列表，供 API 与日志使用。
    pub fn flat_landmarks(&self) -> Vec<Landmark> {
        self.regions
            .iter()
            .flat_map(|region| region.points.iter().cloned())
            .collect()
    }

    pub fn points_by_prefix(&self) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for region in &self.regions {
            *counts
                .entry(region.kind.prefix().to_string())
                .or_insert(0usize) += region.points.len();
        }
        counts
    }
}

/// 语义分割蒙版。当前没有合规 Parsing Provider 时不会产生任何实例。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticMask {
    /// 类别标签，例如 `skin` / `left_eye` / `lips`。
    pub class: String,
    pub width: u32,
    pub height: u32,
    /// 0..255 连续 Alpha；不是二值蒙版。
    pub alpha: Vec<u8>,
    pub confidence: f32,
}

impl SemanticMask {
    pub fn validate(&self) -> bool {
        self.width > 0
            && self.height > 0
            && self.alpha.len() == (self.width as usize * self.height as usize)
            && self.alpha.iter().any(|value| *value > 0)
    }
}

/// 可选的拓扑网格。没有真实三角网格的 Provider 必须返回 `None`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceMesh {
    pub topology_id: String,
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

/// 单张人脸的统一结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceResult {
    pub face_uuid: String,
    pub bounding_box: BoundingBox,
    pub confidence: f32,
    pub landmarks: Vec<Landmark>,
    pub regions: Vec<DenseRegion>,
    #[serde(default)]
    pub masks: Vec<SemanticMask>,
    #[serde(default)]
    pub mesh: Option<FaceMesh>,
}

impl FaceResult {
    pub fn dense(&self, provider: &str) -> DenseFace {
        DenseFace {
            topology_id: String::new(),
            provider_id: provider.to_string(),
            provider_version: String::new(),
            image_width: 0,
            image_height: 0,
            bounding_box: self.bounding_box,
            confidence: self.confidence,
            regions: self.regions.clone(),
        }
    }
}

/// Provider 的统一输出。`image_width/height` 是坐标归一化所依据的原图尺寸。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceAnalysisResult {
    pub faces: Vec<FaceResult>,
    pub provider_id: String,
    pub provider_version: String,
    pub model_version: Option<String>,
    pub topology_id: String,
    pub processing_time_ms: u64,
    pub image_width: u32,
    pub image_height: u32,
    pub warnings: Vec<String>,
}

impl FaceAnalysisResult {
    pub fn total_landmarks(&self) -> usize {
        self.faces
            .iter()
            .map(|face| {
                face.regions
                    .iter()
                    .map(|region| region.points.len())
                    .sum::<usize>()
            })
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn landmark(x: f32, y: f32, id: &str) -> Landmark {
        Landmark::new(x, y, 0.9, id)
    }

    #[test]
    fn dense_fingerprint_changes_with_points() {
        let mut dense = DenseFace {
            topology_id: "apple-vision".into(),
            provider_id: "apple-vision".into(),
            regions: vec![DenseRegion {
                kind: RegionKind::LeftEye,
                points: vec![landmark(0.3, 0.4, "eye.left.lid.0")],
                closed: true,
            }],
            ..Default::default()
        };
        let first = dense.fingerprint();
        dense.regions[0].points[0].x = 0.31;
        assert_ne!(first, dense.fingerprint());
    }

    #[test]
    fn region_prefixes_are_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for kind in [
            RegionKind::FaceContour,
            RegionKind::LeftEye,
            RegionKind::RightEye,
            RegionKind::LeftEyebrow,
            RegionKind::RightEyebrow,
            RegionKind::Nose,
            RegionKind::NoseCrest,
            RegionKind::MedianLine,
            RegionKind::OuterLips,
            RegionKind::InnerLips,
            RegionKind::Iris,
            RegionKind::FaceMesh,
            RegionKind::Other,
        ] {
            assert!(seen.insert(kind.prefix()), "{} 重复", kind.prefix());
        }
    }

    #[test]
    fn semantic_mask_validates_exact_size() {
        let mask = SemanticMask {
            class: "skin".into(),
            width: 2,
            height: 2,
            alpha: vec![0, 1, 2, 3],
            confidence: 0.5,
        };
        assert!(mask.validate());
        let broken = SemanticMask {
            alpha: vec![1],
            ..mask
        };
        assert!(!broken.validate());
    }
}
