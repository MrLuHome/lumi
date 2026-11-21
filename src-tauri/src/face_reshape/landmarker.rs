//! 统一关键点提供者接口。
//!
//! 变形引擎只消费 `LandmarkDetection`（归一化点 + 置信度 + 能力元数据），
//! 不依赖任何具体模型的关键点编号。将来接入 MediaPipe 478 点、商业 SDK
//! 或自研模型时，只需实现本 trait，变形核心与参数系统无需重写。

use serde::Serialize;

use crate::ai_engine::face_frame::{FaceFrame, NormPoint};
use crate::error::{AppError, AppResult};
use crate::image_engine::buffer::Image16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LicenseStatus {
    /// 已核实允许商业使用。
    Commercial,
    /// 已核实不允许商业使用。
    NonCommercial,
    /// 尚未核实，按不可用处理。
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LandmarkModelInfo {
    pub id: String,
    pub display_name: String,
    pub version: String,
    pub point_count: usize,
    pub dense: bool,
    pub license: String,
    pub license_status: LicenseStatus,
    /// 权重 SHA-256；未打包权重时为空。
    pub sha256: String,
    pub source: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LandmarkDetection {
    /// 归一化图像坐标（0..1），由 provider 负责从模型坐标转换。
    pub points: Vec<NormPoint>,
    pub confidence: f32,
    /// 0..1，低置信度时调用方应降低自动变形强度。
    pub quality: f32,
    pub dense: bool,
    pub model: String,
}

/// 模型生命周期与检测接口。
pub trait LandmarkerProvider: Send + Sync {
    fn info(&self) -> LandmarkModelInfo;

    /// 加载模型（无外部权重时只做可用性校验）。
    fn initialize(&mut self) -> AppResult<()>;

    /// 对单张已检测人脸输出关键点；实现方自行决定裁剪与坐标转换。
    fn detect(&self, image: &Image16, face: &FaceFrame) -> AppResult<LandmarkDetection>;

    /// 归一化坐标 → 原图像素坐标。禁止返回屏幕坐标。
    fn to_image(&self, point: NormPoint, width: u32, height: u32) -> (f32, f32) {
        (
            (point.x * width as f32).clamp(0.0, width as f32 - 1.0),
            (point.y * height as f32).clamp(0.0, height as f32 - 1.0),
        )
    }

    /// 置信度输出，便于 UI 显示质量状态。
    fn confidence(&self) -> f32 {
        0.0
    }

    /// 卸载模型，释放内存。
    fn unload(&mut self);
}

/// 当前随包发布的 YuNet 5 点提供者。
///
/// 只作为 LEVEL 1/2 的几何锚点；`dense=false`，不能用于眼睑、唇线、
/// 鼻翼等稠密五官网格。
pub struct Yunet5Provider {
    loaded: bool,
}

impl Default for Yunet5Provider {
    fn default() -> Self {
        Self { loaded: false }
    }
}

impl LandmarkerProvider for Yunet5Provider {
    fn info(&self) -> LandmarkModelInfo {
        LandmarkModelInfo {
            id: "yunet-5".into(),
            display_name: "YuNet 2023mar（5 点）".into(),
            version: "2023mar".into(),
            point_count: 5,
            dense: false,
            license: "MIT（OpenCV Zoo）".into(),
            license_status: LicenseStatus::Commercial,
            sha256: "8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4".into(),
            source: "OpenCV Zoo / face_detection_yunet_2023mar.onnx".into(),
            note: "仅检测框与 5 个锚点，不支持稠密五官网格".into(),
        }
    }

    fn initialize(&mut self) -> AppResult<()> {
        self.loaded = true;
        Ok(())
    }

    fn detect(&self, _image: &Image16, face: &FaceFrame) -> AppResult<LandmarkDetection> {
        if !self.loaded {
            return Err(AppError::Unsupported("YuNet 关键点提供者尚未初始化".into()));
        }
        if face.landmarks.len() < 5 {
            return Err(AppError::Unsupported("检测结果缺少 5 点关键点".into()));
        }
        Ok(LandmarkDetection {
            points: face.landmarks.clone(),
            confidence: face.score,
            quality: face.score,
            dense: false,
            model: "yunet-5".into(),
        })
    }

    fn unload(&mut self) {
        self.loaded = false;
    }
}

/// LEVEL 3 占位：授权未确认前保持禁用，任何调用都明确报错。
pub struct DisabledDenseProvider {
    pub reason: String,
}

impl LandmarkerProvider for DisabledDenseProvider {
    fn info(&self) -> LandmarkModelInfo {
        LandmarkModelInfo {
            id: "dense-disabled".into(),
            display_name: "稠密关键点（未启用）".into(),
            version: "n/a".into(),
            point_count: 0,
            dense: true,
            license: "未确认".into(),
            license_status: LicenseStatus::Unknown,
            sha256: String::new(),
            source: "n/a".into(),
            note: self.reason.clone(),
        }
    }

    fn initialize(&mut self) -> AppResult<()> {
        Err(AppError::Unsupported(self.reason.clone()))
    }

    fn detect(&self, _image: &Image16, _face: &FaceFrame) -> AppResult<LandmarkDetection> {
        Err(AppError::Unsupported(self.reason.clone()))
    }

    fn unload(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face() -> FaceFrame {
        FaceFrame {
            id: 1,
            uuid: "u".into(),
            score: 0.9,
            x: 0.2,
            y: 0.2,
            width: 0.2,
            height: 0.24,
            landmarks: vec![
                NormPoint { x: 0.26, y: 0.3 },
                NormPoint { x: 0.34, y: 0.3 },
                NormPoint { x: 0.30, y: 0.36 },
                NormPoint { x: 0.27, y: 0.4 },
                NormPoint { x: 0.33, y: 0.4 },
            ],
            dense: None,
            level3: None,
        }
    }

    #[test]
    fn yunet_provider_reports_5_points_and_requires_init() {
        let image: Image16 = Image16::new(10, 10);
        let mut provider = Yunet5Provider::default();
        assert!(provider.detect(&image, &face()).is_err());
        provider.initialize().unwrap();
        let detection = provider.detect(&image, &face()).unwrap();
        assert_eq!(detection.points.len(), 5);
        assert!(!detection.dense);
        assert_eq!(provider.info().license_status, LicenseStatus::Commercial);
        provider.unload();
        assert!(provider.detect(&image, &face()).is_err());
    }

    #[test]
    fn disabled_dense_provider_explains_reason() {
        let mut provider = DisabledDenseProvider {
            reason: "稠密关键点模型授权未确认".into(),
        };
        let error = provider.initialize().unwrap_err();
        assert!(error.to_string().contains("授权未确认"));
        assert_eq!(provider.info().license_status, LicenseStatus::Unknown);
    }

    #[test]
    fn coordinates_map_to_image_pixels() {
        let provider = Yunet5Provider::default();
        let (x, y) = provider.to_image(NormPoint { x: 0.5, y: 0.25 }, 1000, 500);
        assert!((x - 500.0).abs() < 0.01);
        assert!((y - 125.0).abs() < 0.01);
    }
}
