//! 用户 ONNX 关键点 Provider。
//!
//! 检测复用随包 YuNet（已实测），关键点来自用户模型；两者坐标都转换到原图
//! 归一化空间。模型输出必须符合 manifest 的 68 点契约，否则整次分析失败并
//! 返回原因，而不是悄悄退化成 5 点。

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::face_intelligence::capability::{
    CapabilityEvidence, EvidenceLevel, LicenseInfo, ProviderCapabilities, ProviderDescriptor,
    ProviderKind, ProviderState,
};
use crate::face_intelligence::model_hub::{InstalledModel, LandmarkModel, ModelHub};
use crate::face_intelligence::provider::{
    sanitize_result, AnalysisOptions, FaceError, FaceInput, FaceProvider, ProviderHealth,
};
use crate::face_intelligence::topology::{group_face68, FACE68};
use crate::face_intelligence::types::{
    DenseRegion, FaceAnalysisResult, FaceResult, Landmark, RegionKind,
};
use crate::image_engine::cancel::CancelToken;

use super::yunet::YunetProvider;

pub struct OnnxLandmarkProvider {
    id: String,
    manifest: crate::face_intelligence::model_hub::ModelManifest,
    model: Arc<LandmarkModel>,
    detector: Arc<YunetProvider>,
    model_path: PathBuf,
}

impl OnnxLandmarkProvider {
    pub fn load(model: &InstalledModel, yunet_path: Option<PathBuf>) -> Result<Self, String> {
        if !model.present || !model.sha256_matches || !model.validation_ok {
            return Err(model
                .error
                .clone()
                .unwrap_or_else(|| "模型未通过验证".into()));
        }
        let model_path = PathBuf::from(&model.model_path);
        let loaded = LandmarkModel::load(&model_path, &model.manifest)?;
        Ok(Self {
            id: format!("onnx:{}", model.manifest.id),
            manifest: model.manifest.clone(),
            model: Arc::new(loaded),
            detector: Arc::new(YunetProvider::new(yunet_path)),
            model_path,
        })
    }

    pub fn base_descriptor(installed: &InstalledModel) -> ProviderDescriptor {
        let license = ModelHub::license_state(installed);
        let capabilities = ProviderCapabilities {
            face_detection: true,
            sparse_landmarks: true,
            dense_landmarks: true,
            eye_landmarks: true,
            eyelid_landmarks: true,
            nose_landmarks: true,
            lip_landmarks: true,
            face_mesh: false,
            face_parsing: false,
            skin_segmentation: false,
            hair_segmentation: false,
            face_matting: false,
            multi_face: true,
            offline_inference: true,
            static_image: true,
            max_input_resolution: None,
        };
        let evidence = vec![
            CapabilityEvidence::new(
                "face_detection",
                EvidenceLevel::Verified,
                "检测复用随包 YuNet；关键点来自用户模型",
            ),
            CapabilityEvidence::new(
                "dense_landmarks",
                EvidenceLevel::Declared,
                "用户模型声明 68 点；需在本机照片上通过重复稳定性与结构校验后才解锁 LEVEL 3",
            ),
            CapabilityEvidence::new(
                "eye_landmarks",
                EvidenceLevel::Declared,
                "face-68 语义映射已固定，等待实测",
            ),
            CapabilityEvidence::new(
                "nose_landmarks",
                EvidenceLevel::Declared,
                "face-68 语义映射已固定，等待实测",
            ),
            CapabilityEvidence::new(
                "lip_landmarks",
                EvidenceLevel::Declared,
                "face-68 语义映射已固定，等待实测",
            ),
            CapabilityEvidence::new(
                "face_parsing",
                EvidenceLevel::Unsupported,
                "当前只支持关键点任务",
            ),
        ];
        ProviderDescriptor {
            id: format!("onnx:{}", installed.manifest.id),
            display_name: format!("{}（用户模型）", installed.manifest.name),
            version: installed.manifest.version.clone(),
            kind: ProviderKind::UserModel,
            state: if installed.validation_ok {
                ProviderState::LicenseUnverified
            } else {
                ProviderState::Failed
            },
            available: installed.present && installed.sha256_matches,
            enabled: installed.validation_ok,
            license: LicenseInfo {
                name: installed.manifest.license.name.clone(),
                url: installed.manifest.license.url.clone(),
                state: license,
                note: format!(
                    "{}；用户声明：商用={}，可再分发={}。LUMI 未核实。",
                    installed.manifest.license.note,
                    if installed.manifest.license.commercial_use_claimed {
                        "是"
                    } else {
                        "否/未知"
                    },
                    if installed.manifest.license.redistribution_claimed {
                        "是"
                    } else {
                        "否/未知"
                    }
                ),
            },
            capabilities,
            verified_capabilities: capabilities.verified_only(&evidence),
            evidence,
            topology_id: Some(FACE68.id.into()),
            note: format!(
                "SHA256 {}… · 输出契约 {}",
                &installed.manifest.sha256[..installed.manifest.sha256.len().min(12)],
                installed.manifest.output.layout.label()
            ),
        }
    }
}

impl FaceProvider for OnnxLandmarkProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn descriptor(&self) -> ProviderDescriptor {
        let installed = InstalledModel {
            manifest: self.manifest.clone(),
            directory: self
                .model_path
                .parent()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default(),
            model_path: self.model_path.to_string_lossy().into_owned(),
            present: true,
            sha256_matches: true,
            validation_ok: true,
            error: None,
        };
        Self::base_descriptor(&installed)
    }

    fn analyze(
        &self,
        input: &FaceInput<'_>,
        options: &AnalysisOptions,
        cancel: &CancelToken,
    ) -> Result<FaceAnalysisResult, FaceError> {
        let started = Instant::now();
        let detection = self.detector.analyze(
            input,
            &AnalysisOptions {
                max_edge: options.max_edge.min(1280),
                ..options.clone()
            },
            cancel,
        )?;
        let mut faces = Vec::with_capacity(detection.faces.len());
        for (index, face) in detection.faces.iter().enumerate() {
            if cancel.is_cancelled() {
                return Err(FaceError::Cancelled);
            }
            let points = self
                .model
                .infer(input.image, face.bounding_box)
                .map_err(|error| {
                    FaceError::Inference(format!("第 {} 张人脸推理失败：{error}", index + 1))
                })?;
            let grouped = group_face68(&points)
                .map_err(|error| FaceError::Inference(format!("68 点分组失败：{error}")))?;
            let mut regions = Vec::new();
            let mut landmarks = Vec::new();
            for (kind, values) in grouped {
                let prefix = kind.prefix();
                let points: Vec<Landmark> = values
                    .into_iter()
                    .enumerate()
                    .map(|(local, (x, y, confidence))| {
                        Landmark::new(
                            x,
                            y,
                            (confidence * face.confidence).clamp(0.0, 1.0),
                            format!("{prefix}.{local}"),
                        )
                    })
                    .collect();
                landmarks.extend(points.iter().cloned());
                regions.push(DenseRegion {
                    kind,
                    points,
                    closed: matches!(
                        kind,
                        RegionKind::FaceContour
                            | RegionKind::LeftEye
                            | RegionKind::RightEye
                            | RegionKind::OuterLips
                            | RegionKind::InnerLips
                    ),
                });
            }
            faces.push(FaceResult {
                face_uuid: String::new(),
                bounding_box: face.bounding_box,
                confidence: face.confidence,
                landmarks,
                regions,
                masks: Vec::new(),
                mesh: None,
            });
        }
        let result = FaceAnalysisResult {
            faces,
            provider_id: self.id.clone(),
            provider_version: self.manifest.version.clone(),
            model_version: Some(format!("{} {}", self.manifest.name, self.manifest.version)),
            topology_id: FACE68.id.into(),
            processing_time_ms: started.elapsed().as_millis() as u64,
            image_width: input.image_width,
            image_height: input.image_height,
            warnings: vec!["用户模型：语义与精度未经 LUMI 认证，仅通过结构/稳定性校验".into()],
        };
        Ok(sanitize_result(result, options))
    }

    fn health_check(&self) -> ProviderHealth {
        if self.model_path.is_file() {
            ProviderHealth::Ready
        } else {
            ProviderHealth::Unavailable
        }
    }
}
