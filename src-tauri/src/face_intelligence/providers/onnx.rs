//! 用户 ONNX 关键点 Provider。
//!
//! 检测复用随包 YuNet（已实测），关键点来自用户模型。用户模型的解析与推理
//! **只在应用自身的独立 Worker 子进程中执行**（`model_hub::probe::infer_isolated`），
//! 主进程只做裁剪/缩放/归一化与临时文件读写，绝不加载用户模型本体。
//!
//! 模型输出必须符合 manifest 的 68 点契约，否则整次分析失败并返回原因，
//! 而不是悄悄退化成 5 点。

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::face_intelligence::capability::{
    CapabilityEvidence, EvidenceLevel, LicenseInfo, ProviderCapabilities, ProviderDescriptor,
    ProviderKind, ProviderState,
};
use crate::face_intelligence::coordinate;
use crate::face_intelligence::model_hub::{
    inference, probe, InstalledModel, ModelHub, ModelManifest,
};
use crate::face_intelligence::provider::{
    sanitize_result, AnalysisOptions, FaceError, FaceInput, FaceProvider, ProviderHealth,
};
use crate::face_intelligence::topology::{group_face68, FACE68};
use crate::face_intelligence::types::{
    BoundingBox, DenseRegion, FaceAnalysisResult, FaceResult, Landmark, RegionKind,
};
use crate::image_engine::cancel::CancelToken;

use super::yunet::YunetProvider;

static INPUT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub struct OnnxLandmarkProvider {
    id: String,
    manifest: ModelManifest,
    model_path: PathBuf,
    detector: Arc<YunetProvider>,
    /// 应用自身可执行文件；没有它就没有 Worker，Provider 不可用。
    executable: Option<PathBuf>,
}

impl OnnxLandmarkProvider {
    pub fn load(
        model: &InstalledModel,
        yunet_path: Option<PathBuf>,
        executable: Option<PathBuf>,
    ) -> Result<Self, String> {
        if !model.present || !model.sha256_matches || !model.validation_ok {
            return Err(model
                .error
                .clone()
                .unwrap_or_else(|| "模型未通过验证".into()));
        }
        Ok(Self {
            id: format!("onnx:{}", model.manifest.id),
            manifest: model.manifest.clone(),
            model_path: PathBuf::from(&model.model_path),
            detector: Arc::new(YunetProvider::new(yunet_path)),
            executable,
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
                "SHA256 {}… · 输出契约 {} · 实验性：推理在隔离 Worker 中执行，模型仍来自用户，未审计",
                &installed.manifest.sha256[..installed.manifest.sha256.len().min(12)],
                installed.manifest.output.layout.label()
            ),
        }
    }

    /// 用户模型的真实推理：主进程预处理 → 写临时张量 → Worker 子进程加载并推理。
    fn infer_face(
        &self,
        image: &crate::image_engine::buffer::Image16,
        bbox: BoundingBox,
    ) -> Result<Vec<(f32, f32, f32)>, String> {
        let executable = self.executable.as_deref().ok_or_else(|| {
            "当前环境没有可用的 Worker 可执行文件，用户 ONNX 模型不会在主进程加载".to_string()
        })?;
        let prepared = inference::prepare_input(image, bbox, &self.manifest)?;
        if prepared.width > probe::MAX_INFER_EDGE as usize
            || prepared.height > probe::MAX_INFER_EDGE as usize
        {
            return Err(format!(
                "模型输入 {}×{} 超过 Worker 上限 {}",
                prepared.width,
                prepared.height,
                probe::MAX_INFER_EDGE
            ));
        }
        let dir = std::env::temp_dir().join(format!(
            "lumi-face-input-{}-{}",
            std::process::id(),
            INPUT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).map_err(|error| format!("创建推理输入目录失败：{error}"))?;
        let input_path = dir.join("input.f32");
        let mut bytes = Vec::with_capacity(prepared.data.len() * 4);
        for value in &prepared.data {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        let write_result = std::fs::write(&input_path, &bytes);
        drop(bytes);
        if let Err(error) = write_result {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(format!("写入推理输入失败：{error}"));
        }
        let request = probe::InferRequest {
            model_path: self.model_path.to_string_lossy().into_owned(),
            input_path: input_path.to_string_lossy().into_owned(),
            width: prepared.width as u32,
            height: prepared.height as u32,
            expected_points: 68,
            timeout_ms: probe::DEFAULT_INFER_TIMEOUT_MS,
        };
        let response = probe::infer_isolated(
            executable,
            &request,
            Duration::from_millis(probe::DEFAULT_INFER_TIMEOUT_MS),
        );
        let _ = std::fs::remove_dir_all(&dir);
        let response = response?;
        if !response.ok {
            return Err(response
                .error
                .unwrap_or_else(|| "Worker 未给出具体原因".into()));
        }
        if response.points.len() != 68 {
            return Err(format!(
                "Worker 返回 {} 点，face-68 需要 68 点",
                response.points.len()
            ));
        }
        Ok(response
            .points
            .into_iter()
            .map(|[x, y, confidence]| {
                let (ix, iy) = coordinate::crop_to_image(x, y, prepared.crop);
                (ix, iy, confidence)
            })
            .collect())
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
        if self.executable.is_none() {
            return Err(FaceError::Unsupported(
                "没有可用的 Worker 进程，用户 ONNX 模型不会在主进程加载".into(),
            ));
        }
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
                .infer_face(input.image, face.bounding_box)
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
            warnings: vec![
                "用户模型：推理在隔离 Worker 中执行，但模型未审计；语义与精度未经 LUMI 认证，仅通过结构/稳定性校验".into(),
            ],
        };
        Ok(sanitize_result(result, options))
    }

    fn health_check(&self) -> ProviderHealth {
        if self.executable.is_none() {
            return ProviderHealth::Unavailable;
        }
        if self.model_path.is_file() {
            ProviderHealth::Ready
        } else {
            ProviderHealth::Unavailable
        }
    }
}
