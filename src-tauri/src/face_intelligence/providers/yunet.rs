//! 随包 YuNet 5 点 Provider。
//!
//! 只声明真实具备的能力：检测与 5 个几何锚点。眼睑、鼻翼、唇轮廓
//! 都没有输出，因此稠密能力为 `Unsupported`，绝不能解锁 LEVEL 3。

use std::path::PathBuf;
use std::time::Instant;

use crate::ai::face::FaceEngine;
use crate::face_intelligence::capability::{
    CapabilityEvidence, EvidenceLevel, LicenseInfo, LicenseState, ProviderCapabilities,
    ProviderDescriptor, ProviderKind, ProviderState,
};
use crate::face_intelligence::provider::{
    sanitize_result, AnalysisOptions, FaceError, FaceInput, FaceProvider, ProviderHealth,
};
use crate::face_intelligence::topology::YUNET5;
use crate::face_intelligence::types::{
    BoundingBox, DenseRegion, FaceAnalysisResult, FaceResult, Landmark, RegionKind,
};
use crate::image_engine::cancel::CancelToken;

pub const PROVIDER_ID: &str = "yunet-5";

pub struct YunetProvider {
    engine: FaceEngine,
    model_path: Option<PathBuf>,
}

impl YunetProvider {
    pub fn new(model_path: Option<PathBuf>) -> Self {
        Self {
            engine: FaceEngine::default(),
            model_path,
        }
    }

    pub fn base_descriptor(model_path: Option<&std::path::Path>) -> ProviderDescriptor {
        let available = model_path.map(|path| path.exists()).unwrap_or(false);
        let capabilities = ProviderCapabilities {
            face_detection: true,
            sparse_landmarks: true,
            dense_landmarks: false,
            face_mesh: false,
            face_parsing: false,
            skin_segmentation: false,
            hair_segmentation: false,
            face_matting: false,
            multi_face: true,
            offline_inference: true,
            static_image: true,
            ..Default::default()
        };
        let evidence = vec![
            CapabilityEvidence::new(
                "face_detection",
                EvidenceLevel::Verified,
                "第七轮起随包模型，集成测试覆盖真实样张",
            ),
            CapabilityEvidence::new(
                "sparse_landmarks",
                EvidenceLevel::Verified,
                "输出右眼/左眼/鼻尖/左右嘴角共 5 点",
            ),
            CapabilityEvidence::new(
                "dense_landmarks",
                EvidenceLevel::Unsupported,
                "模型只有 5 点，没有眼睑/鼻翼/唇轮廓输出",
            ),
            CapabilityEvidence::new(
                "face_parsing",
                EvidenceLevel::Unsupported,
                "没有逐像素分割输出",
            ),
        ];
        ProviderDescriptor {
            id: PROVIDER_ID.into(),
            display_name: "YuNet 2023mar（随包）".into(),
            version: "2023mar".into(),
            kind: ProviderKind::Builtin,
            state: if available {
                ProviderState::Idle
            } else {
                ProviderState::Missing
            },
            available,
            enabled: available,
            license: LicenseInfo {
                name: "MIT".into(),
                url: Some("https://github.com/opencv/opencv_zoo".into()),
                state: LicenseState::VerifiedCommercial,
                note: "随包 YuNet 模型为 MIT，自第七轮起 SHA256 固定校验。".into(),
            },
            capabilities,
            verified_capabilities: capabilities.verified_only(&evidence),
            evidence,
            topology_id: Some(YUNET5.id.into()),
            note: if available {
                "基础检测与 5 点锚点；不提供稠密五官，LEVEL 3 保持禁用".into()
            } else {
                "未找到随包 YuNet 模型文件".into()
            },
        }
    }
}

impl FaceProvider for YunetProvider {
    fn id(&self) -> &str {
        PROVIDER_ID
    }

    fn descriptor(&self) -> ProviderDescriptor {
        Self::base_descriptor(self.model_path.as_deref())
    }

    fn analyze(
        &self,
        input: &FaceInput<'_>,
        options: &AnalysisOptions,
        cancel: &CancelToken,
    ) -> Result<FaceAnalysisResult, FaceError> {
        let Some(model_path) = &self.model_path else {
            return Err(FaceError::Unavailable("未找到随包 YuNet 模型".into()));
        };
        if !model_path.exists() {
            return Err(FaceError::Unavailable(format!(
                "随包模型不存在：{}",
                model_path.display()
            )));
        }
        if cancel.is_cancelled() {
            return Err(FaceError::Cancelled);
        }
        let started = Instant::now();
        let longest = input.image.width().max(input.image.height());
        let analysis = if options.max_edge > 0 && longest > options.max_edge {
            let scale = options.max_edge as f32 / longest as f32;
            image::imageops::thumbnail(
                input.image,
                ((input.image.width() as f32 * scale).round() as u32).max(1),
                ((input.image.height() as f32 * scale).round() as u32).max(1),
            )
        } else {
            input.image.clone()
        };
        let detected = self
            .engine
            .detect(
                model_path,
                &analysis,
                input.image_width,
                input.image_height,
                &mut |_, _| {},
            )
            .map_err(|error| FaceError::Inference(error.to_string()))?;
        if cancel.is_cancelled() {
            return Err(FaceError::Cancelled);
        }

        let width = input.image_width.max(1) as f32;
        let height = input.image_height.max(1) as f32;
        let labels = [
            ("eye.right", RegionKind::RightEye),
            ("eye.left", RegionKind::LeftEye),
            ("nose.tip", RegionKind::Nose),
            ("mouth.right_corner", RegionKind::OuterLips),
            ("mouth.left_corner", RegionKind::OuterLips),
        ];
        let faces = detected
            .faces
            .into_iter()
            .map(|face| {
                let bounding_box = BoundingBox {
                    x: face.x / width,
                    y: face.y / height,
                    width: face.width / width,
                    height: face.height / height,
                }
                .clamp_unit();
                let landmarks: Vec<Landmark> = face
                    .landmarks
                    .iter()
                    .take(labels.len())
                    .enumerate()
                    .map(|(index, point)| {
                        Landmark::new(
                            point.x / width,
                            point.y / height,
                            face.score,
                            labels[index].0,
                        )
                    })
                    .collect();
                // 5 点不是轮廓，不构造 region，避免下游误以为有眼睑点列。
                FaceResult {
                    face_uuid: String::new(),
                    bounding_box,
                    confidence: face.score,
                    landmarks,
                    regions: Vec::<DenseRegion>::new(),
                    masks: Vec::new(),
                    mesh: None,
                }
            })
            .collect();

        let result = FaceAnalysisResult {
            faces,
            provider_id: PROVIDER_ID.into(),
            provider_version: "2023mar".into(),
            model_version: Some("YuNet 2023mar".into()),
            topology_id: YUNET5.id.into(),
            processing_time_ms: started.elapsed().as_millis() as u64,
            image_width: input.image_width,
            image_height: input.image_height,
            warnings: vec!["YuNet 仅输出 5 点，不提供稠密轮廓".into()],
        };
        Ok(sanitize_result(result, options))
    }

    fn health_check(&self) -> ProviderHealth {
        match &self.model_path {
            Some(path) if path.exists() => ProviderHealth::Ready,
            _ => ProviderHealth::Unavailable,
        }
    }

    fn loaded(&self) -> bool {
        self.engine.status(self.model_path.as_deref()).loaded
    }
}
