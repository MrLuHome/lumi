//! 第五轮人物分割（BODY & BACKGROUND ENGINE 1.0）。
//!
//! 本轮仓库内没有可再分发的分割权重，因此 AI 分割以“接口 + 禁用 + 原因”交付；
//! 实际可用的是用户手动笔触 + 本地 Alpha 处理（羽化、收缩扩张、发丝细节、去污染），
//! UI 会明确标注为「手动蒙版（非 AI 分割）」。

pub mod alpha;
pub mod cache;
pub mod hair;
pub mod inference;
pub mod manual;
pub mod model;
pub mod refinement;

use serde::Serialize;

use crate::error::AppResult;
use crate::image_engine::buffer::Image16;
use crate::image_engine::params::SubjectMaskParams;

/// 与源图同尺寸的软 Alpha。0 表示背景，1 表示人物。
#[derive(Debug, Clone)]
pub struct SubjectMask {
    pub width: u32,
    pub height: u32,
    pub alpha: Vec<f32>,
    pub source: String,
    pub model_version: String,
    pub confidence: f32,
}

impl SubjectMask {
    pub fn empty(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            alpha: vec![0.0; width as usize * height as usize],
            source: "manual".into(),
            model_version: "manual-alpha-1".into(),
            confidence: 0.0,
        }
    }

    pub fn coverage(&self) -> f32 {
        alpha::coverage(&self.alpha)
    }

    pub fn edge_ratio(&self) -> f32 {
        alpha::edge_ratio(&self.alpha, self.width, self.height)
    }

    pub fn is_empty(&self) -> bool {
        self.alpha.iter().all(|value| *value <= 0.001)
    }

    pub fn inverted(&self) -> Self {
        let mut next = self.clone();
        for value in next.alpha.iter_mut() {
            *value = 1.0 - *value;
        }
        next.source = format!("{}-inverted", self.source);
        next
    }

    pub fn sample(&self, x: f32, y: f32) -> f32 {
        if self.width == 0 || self.height == 0 {
            return 0.0;
        }
        let px = x.clamp(0.0, (self.width - 1) as f32);
        let py = y.clamp(0.0, (self.height - 1) as f32);
        let index = py as usize * self.width as usize + px as usize;
        self.alpha.get(index).copied().unwrap_or(0.0)
    }

    pub fn to_image16(&self) -> Image16 {
        Image16::from_fn(self.width.max(1), self.height.max(1), |x, y| {
            let value = self
                .alpha
                .get(y as usize * self.width as usize + x as usize)
                .copied()
                .unwrap_or(0.0);
            let level = (value.clamp(0.0, 1.0) * 65535.0).round() as u16;
            image::Rgba([level, level, level, 65535])
        })
    }
}

/// 能力描述：前端据此启用或禁用按钮，并显示不可用原因。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaskProviderInfo {
    pub id: String,
    pub version: String,
    pub license: String,
    pub available: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentationCapability {
    pub ai_available: bool,
    pub manual_available: bool,
    pub providers: Vec<MaskProviderInfo>,
    pub note: String,
}

pub trait MaskProvider: Send + Sync {
    fn id(&self) -> &str;
    fn version(&self) -> &str;
    fn license(&self) -> &str;
    fn available(&self) -> bool;
    fn unavailable_reason(&self) -> &str;
    fn segment(&self, image: &Image16) -> AppResult<SubjectMask>;
}

pub fn capability() -> SegmentationCapability {
    let providers: Vec<Box<dyn MaskProvider>> =
        vec![Box::new(inference::AiMaskProvider::default())];
    SegmentationCapability {
        ai_available: providers.iter().any(|provider| provider.available()),
        manual_available: true,
        providers: providers
            .iter()
            .map(|provider| MaskProviderInfo {
                id: provider.id().into(),
                version: provider.version().into(),
                license: provider.license().into(),
                available: provider.available(),
                reason: provider.unavailable_reason().into(),
            })
            .collect(),
        note: "AI 人物分割未启用：仓库内没有经过再分发审查的权重。当前使用手动笔触 + 本地 Alpha 处理，非 AI 推理。"
            .into(),
    }
}

/// 构建人物蒙版。当前只走手动笔触路径；AI Provider 未授权时返回空蒙版，
/// 由调用方给出明确警告，而不是静默使用未授权模型。
pub fn build_subject_mask(image: &Image16, params: &SubjectMaskParams) -> SubjectMask {
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return SubjectMask::empty(width, height);
    }
    if !params.has_content() {
        return SubjectMask::empty(width, height);
    }
    let mut mask = manual::build_manual_mask(image, params);
    if params.hair_protect > 0.0 {
        hair::apply_hair_detail(&mut mask.alpha, image, params.hair_protect);
    }
    if params.invert {
        mask = mask.inverted();
    }
    mask
}

/// 由给定笔触构建独立人物蒙版（多人照片中每个人一条）。
pub fn build_mask_from_strokes(
    image: &Image16,
    strokes: &[crate::image_engine::params::SubjectMaskStroke],
    template: &SubjectMaskParams,
) -> SubjectMask {
    let params = SubjectMaskParams {
        enabled: true,
        invert: template.invert,
        feather: template.feather,
        expand: template.expand,
        smooth: template.smooth,
        hair_protect: template.hair_protect,
        decontaminate: template.decontaminate,
        strokes: strokes.to_vec(),
        source: template.source.clone(),
        model_version: template.model_version.clone(),
    };
    build_subject_mask(image, &params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_engine::params::{MaskPoint, SubjectMaskStroke, SubjectMaskStrokeKind};
    use image::Rgba;

    fn stroke(x: f32, y: f32) -> SubjectMaskStroke {
        SubjectMaskStroke {
            kind: SubjectMaskStrokeKind::Paint,
            radius: 0.2,
            hardness: 100.0,
            strength: 100.0,
            points: vec![MaskPoint { x, y }],
        }
    }

    #[test]
    fn manual_mask_covers_stroke_center() {
        let image: Image16 = Image16::from_pixel(64, 64, Rgba([10, 20, 30, 65535]));
        let params = SubjectMaskParams {
            enabled: true,
            strokes: vec![stroke(0.5, 0.5)],
            smooth: 0.0,
            feather: 0.0,
            hair_protect: 0.0,
            ..Default::default()
        };
        let mask = build_subject_mask(&image, &params);
        assert!(mask.alpha[32 * 64 + 32] > 0.9);
        assert_eq!(mask.alpha[0], 0.0);
        assert!(mask.coverage() > 0.0 && mask.coverage() < 1.0);
    }

    #[test]
    fn disabled_mask_is_empty() {
        let image: Image16 = Image16::from_pixel(8, 8, Rgba([0, 0, 0, 65535]));
        let params = SubjectMaskParams {
            enabled: false,
            strokes: vec![stroke(0.5, 0.5)],
            ..Default::default()
        };
        let mask = build_subject_mask(&image, &params);
        assert!(mask.is_empty());
    }

    #[test]
    fn ai_provider_reports_unavailable_with_reason() {
        let capability = capability();
        assert!(!capability.ai_available);
        assert!(capability.manual_available);
        assert!(capability.providers[0].reason.contains("权重"));
    }
}
