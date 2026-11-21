//! 统一 Provider 接口。
//!
//! Provider 之间互相隔离：任何 SDK/模型的类型都不得出现在返回值里，
//! 失败只影响当前 Provider，不影响其他 Provider 与手动修图。

use std::fmt;

use serde::Serialize;

use crate::face_intelligence::capability::ProviderDescriptor;
use crate::face_intelligence::types::FaceAnalysisResult;
use crate::image_engine::buffer::Image16;
use crate::image_engine::cancel::CancelToken;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FaceError {
    /// 当前平台/安装包不提供该 Provider。
    Unavailable(String),
    /// Provider 尚未初始化或被卸载。
    NotInitialized(String),
    /// 输入不合法（空图、尺寸超出上限）。
    InvalidInput(String),
    /// 推理失败（模型损坏、算子错误、内存不足）。
    Inference(String),
    /// 推理超时，已放弃本次结果。
    Timeout(String),
    /// 能力不支持。
    Unsupported(String),
    /// 用户取消。
    Cancelled,
}

impl fmt::Display for FaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FaceError::Unavailable(message) => write!(formatter, "Provider 不可用：{message}"),
            FaceError::NotInitialized(message) => write!(formatter, "Provider 未初始化：{message}"),
            FaceError::InvalidInput(message) => write!(formatter, "人脸分析输入无效：{message}"),
            FaceError::Inference(message) => write!(formatter, "人脸推理失败：{message}"),
            FaceError::Timeout(message) => write!(formatter, "人脸推理超时：{message}"),
            FaceError::Unsupported(message) => write!(formatter, "不支持的人脸能力：{message}"),
            FaceError::Cancelled => write!(formatter, "人脸分析已取消"),
        }
    }
}

impl std::error::Error for FaceError {}

impl From<FaceError> for crate::error::AppError {
    fn from(error: FaceError) -> Self {
        match error {
            FaceError::Cancelled => crate::error::AppError::Cancelled,
            FaceError::InvalidInput(message) => crate::error::AppError::InvalidInput(message),
            FaceError::Unsupported(message) | FaceError::Unavailable(message) => {
                crate::error::AppError::Unsupported(message)
            }
            other => crate::error::AppError::Task(other.to_string()),
        }
    }
}

/// 分析输入。保留原图尺寸，Provider 自行决定缩放策略，但坐标必须回到原图。
pub struct FaceInput<'a> {
    pub image: &'a Image16,
    pub image_width: u32,
    pub image_height: u32,
}

impl<'a> FaceInput<'a> {
    pub fn new(image: &'a Image16) -> Self {
        Self {
            image,
            image_width: image.width(),
            image_height: image.height(),
        }
    }
}

/// 分析选项。默认静态照片、多人脸、不输出蒙版与网格。
#[derive(Debug, Clone)]
pub struct AnalysisOptions {
    pub max_faces: usize,
    pub min_confidence: f32,
    /// 是否请求语义蒙版；没有 Parsing 能力时 Provider 返回空。
    pub include_masks: bool,
    /// 是否请求三维网格；没有网格能力时返回 `None`。
    pub include_mesh: bool,
    /// 分析阶段的最长边；仅影响推理分辨率，坐标仍映射回原图。
    pub max_edge: u32,
}

impl Default for AnalysisOptions {
    fn default() -> Self {
        Self {
            max_faces: 16,
            min_confidence: 0.35,
            include_masks: false,
            include_mesh: false,
            max_edge: 2048,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderHealth {
    Ready,
    Degraded,
    Unavailable,
}

/// Provider 统一接口。实现方负责模型生命周期、坐标转换与取消检查。
pub trait FaceProvider: Send + Sync {
    fn id(&self) -> &str;

    /// 当前真实能力、证据与许可证状态。
    fn descriptor(&self) -> ProviderDescriptor;

    /// 对单张静态照片分析。支持多人脸；不允许联网上传。
    fn analyze(
        &self,
        input: &FaceInput<'_>,
        options: &AnalysisOptions,
        cancel: &CancelToken,
    ) -> Result<FaceAnalysisResult, FaceError>;

    fn health_check(&self) -> ProviderHealth;

    /// 卸载模型/释放资源。默认无操作。
    fn unload(&self) {}

    /// 是否已经加载（供 UI 显示“已加载/未安装”）。
    fn loaded(&self) -> bool {
        true
    }
}

/// 把 Provider 结果限制在调用方要求的范围内，并校验坐标与置信度。
pub fn sanitize_result(
    mut result: FaceAnalysisResult,
    options: &AnalysisOptions,
) -> FaceAnalysisResult {
    result.faces.retain(|face| {
        face.confidence >= options.min_confidence
            && face.bounding_box.width > 0.0
            && face.bounding_box.height > 0.0
    });
    result.faces.truncate(options.max_faces);
    for face in result.faces.iter_mut() {
        face.bounding_box = face.bounding_box.clamp_unit();
        face.landmarks.retain(|point| point.is_finite());
        for region in face.regions.iter_mut() {
            region.points.retain(|point| point.is_finite());
        }
        face.regions.retain(|region| !region.points.is_empty());
        if !options.include_masks {
            face.masks.clear();
        }
        if !options.include_mesh {
            face.mesh = None;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::face_intelligence::types::{
        BoundingBox, DenseRegion, FaceResult, Landmark, RegionKind,
    };

    fn result_with(faces: Vec<FaceResult>) -> FaceAnalysisResult {
        FaceAnalysisResult {
            faces,
            provider_id: "test".into(),
            provider_version: "1".into(),
            model_version: None,
            topology_id: "test".into(),
            processing_time_ms: 1,
            image_width: 100,
            image_height: 100,
            warnings: Vec::new(),
        }
    }

    fn face(confidence: f32) -> FaceResult {
        FaceResult {
            face_uuid: "u".into(),
            bounding_box: BoundingBox {
                x: 0.1,
                y: 0.1,
                width: 0.2,
                height: 0.2,
            },
            confidence,
            landmarks: vec![Landmark::new(0.2, 0.2, 0.9, "eye.left.lid.0")],
            regions: vec![DenseRegion {
                kind: RegionKind::LeftEye,
                points: vec![Landmark::new(0.2, 0.2, 0.9, "eye.left.lid.0")],
                closed: true,
            }],
            masks: Vec::new(),
            mesh: None,
        }
    }

    #[test]
    fn low_confidence_faces_are_dropped() {
        let options = AnalysisOptions::default();
        let result = sanitize_result(result_with(vec![face(0.1), face(0.9)]), &options);
        assert_eq!(result.faces.len(), 1);
        assert!(result.faces[0].confidence > 0.5);
    }

    #[test]
    fn faces_are_truncated_to_max() {
        let options = AnalysisOptions {
            max_faces: 1,
            ..Default::default()
        };
        let result = sanitize_result(result_with(vec![face(0.9), face(0.8)]), &options);
        assert_eq!(result.faces.len(), 1);
    }

    #[test]
    fn non_finite_points_are_removed() {
        let options = AnalysisOptions::default();
        let mut bad = face(0.9);
        bad.regions[0].points.push(Landmark {
            x: f32::NAN,
            y: 0.5,
            confidence: 0.9,
            semantic_id: "eye.left.lid.1".into(),
        });
        let result = sanitize_result(result_with(vec![bad]), &options);
        assert_eq!(result.faces[0].regions[0].points.len(), 1);
    }
}
