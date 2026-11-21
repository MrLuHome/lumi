//! Provider 注册表、质量验证缓存与 LEVEL 3 门控的唯一入口。
//!
//! 约束：
//! - 能力只有在本机实测（重复稳定性 + 结构校验）后才升级为 `Verified`；
//! - 切换 Provider 不影响其他 Provider 的健康状态；
//! - 任何 Provider 失败都返回具体原因，不会静默回退到“看起来能用”的结果。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

use super::capability::{EvidenceLevel, ProviderDescriptor, ProviderState};
use super::dense;
use super::model_hub::{InstalledModel, ModelHub};
use super::provider::{AnalysisOptions, FaceError, FaceInput, FaceProvider, ProviderHealth};
use super::providers::{apple_vision, onnx, yunet};
use super::quality::{self, FaceQuality, Level3Gate, QualityThresholds, StabilityReport};
use super::types::{BoundingBox, DenseFace, FaceAnalysisResult};
use crate::image_engine::buffer::Image16;
use crate::image_engine::cancel::CancelToken;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzedFace {
    pub face_uuid: String,
    pub bounding_box: BoundingBox,
    pub confidence: f32,
    pub dense_points: usize,
    pub dense_fingerprint: Option<String>,
    pub quality: FaceQuality,
    pub gate: Level3Gate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceAnalysisRecord {
    pub photo_path: String,
    pub provider_id: String,
    pub provider_name: String,
    pub topology_id: String,
    pub processing_time_ms: u64,
    pub image_width: u32,
    pub image_height: u32,
    pub faces: Vec<AnalyzedFace>,
    pub stability: Option<StabilityReport>,
    pub warnings: Vec<String>,
    pub total_landmarks: usize,
    pub landmark_counts: BTreeMap<String, usize>,
    pub created_at_unix: u64,
}

impl FaceAnalysisRecord {
    pub fn gate_summary(&self) -> String {
        summary_for_gates(self.faces.iter().map(|face| &face.gate))
    }
}

fn summary_for_gates<'a>(gates: impl Iterator<Item = &'a Level3Gate>) -> String {
    let mut enabled = Vec::new();
    let mut disabled = Vec::new();
    for gate in gates {
        for (name, feature) in [
            ("眼部", &gate.eyes),
            ("鼻部", &gate.nose),
            ("唇部", &gate.mouth),
            ("面部轮廓", &gate.chin),
        ] {
            if feature.enabled {
                if !enabled.contains(&name) {
                    enabled.push(name);
                }
            } else if !disabled.contains(&name) {
                disabled.push(name);
            }
        }
    }
    if enabled.is_empty() {
        "LEVEL 3 未解锁（当前人脸没有通过质量门控的稠密区域）".into()
    } else {
        format!(
            "LEVEL 3 已解锁：{}；未解锁：{}",
            enabled.join("、"),
            if disabled.is_empty() {
                "无".into()
            } else {
                disabled.join("、")
            }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityReport {
    pub provider_id: String,
    pub provider_name: String,
    pub stability: StabilityReport,
    pub quality_sample: Vec<FaceQuality>,
    pub verified: bool,
    pub note: String,
    pub measured_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceIntelligenceStatus {
    pub active_provider: String,
    pub providers: Vec<ProviderDescriptor>,
    pub last_analysis: Option<FaceAnalysisRecord>,
    pub quality: Vec<QualityReport>,
    pub level3_summary: String,
    pub models_root: String,
    pub thresholds: QualityThresholds,
}

#[derive(Debug, Clone)]
struct Verification {
    stability: StabilityReport,
    verified: bool,
    measured_at_unix: u64,
    note: String,
}

pub struct FaceIntelligence {
    providers: RwLock<BTreeMap<String, Arc<dyn FaceProvider>>>,
    installed: RwLock<Vec<InstalledModel>>,
    active: RwLock<String>,
    verifications: RwLock<BTreeMap<String, Verification>>,
    last: RwLock<Option<FaceAnalysisRecord>>,
    model_hub: ModelHub,
    yunet_path: Option<PathBuf>,
    thresholds: QualityThresholds,
    models_root: PathBuf,
}

impl FaceIntelligence {
    pub fn new(
        yunet_path: Option<PathBuf>,
        models_root: PathBuf,
        executable: Option<PathBuf>,
    ) -> Self {
        let model_hub = ModelHub::new(models_root.clone(), executable);
        let mut providers: BTreeMap<String, Arc<dyn FaceProvider>> = BTreeMap::new();
        providers.insert(
            yunet::PROVIDER_ID.into(),
            Arc::new(yunet::YunetProvider::new(yunet_path.clone())),
        );
        if apple_vision::AppleVisionProvider::available() {
            providers.insert(
                apple_vision::PROVIDER_ID.into(),
                Arc::new(apple_vision::AppleVisionProvider::new()),
            );
        }
        let installed = model_hub.list();
        for model in &installed {
            if let Ok(provider) = onnx::OnnxLandmarkProvider::load(model, yunet_path.clone()) {
                providers.insert(provider.id().to_string(), Arc::new(provider));
            }
        }
        Self {
            providers: RwLock::new(providers),
            installed: RwLock::new(installed),
            active: RwLock::new(yunet::PROVIDER_ID.into()),
            verifications: RwLock::new(BTreeMap::new()),
            last: RwLock::new(None),
            model_hub,
            yunet_path,
            thresholds: QualityThresholds::default(),
            models_root,
        }
    }

    pub fn thresholds(&self) -> &QualityThresholds {
        &self.thresholds
    }

    pub fn model_hub(&self) -> &ModelHub {
        &self.model_hub
    }

    pub fn models_root(&self) -> &Path {
        &self.models_root
    }

    pub fn active_id(&self) -> String {
        read(&self.active).clone()
    }

    pub fn active_provider(&self) -> Result<Arc<dyn FaceProvider>, FaceError> {
        let active = self.active_id();
        read(&self.providers)
            .get(&active)
            .cloned()
            .ok_or_else(|| FaceError::Unavailable(format!("Provider `{active}` 不可用")))
    }

    /// 选择 Provider。不可用、未验证安装或许可不可用的模型会被拒绝。
    pub fn select(&self, id: &str) -> Result<ProviderDescriptor, FaceError> {
        let descriptor = self.descriptor(id)?;
        if !descriptor.available {
            return Err(FaceError::Unavailable(format!(
                "{} 当前不可用：{}",
                descriptor.display_name, descriptor.note
            )));
        }
        if !descriptor.enabled {
            return Err(FaceError::Unavailable(format!(
                "{} 未通过验证，不能启用",
                descriptor.display_name
            )));
        }
        if !descriptor.license.state.is_usable_locally() {
            return Err(FaceError::Unavailable(format!(
                "{} 的许可证状态为“{}”，不能启用",
                descriptor.display_name,
                descriptor.license.state.label()
            )));
        }
        *write(&self.active) = id.to_string();
        self.descriptor(id)
    }

    pub fn descriptor(&self, id: &str) -> Result<ProviderDescriptor, FaceError> {
        let provider = read(&self.providers)
            .get(id)
            .cloned()
            .ok_or_else(|| FaceError::Unavailable(format!("未知 Provider `{id}`")))?;
        let mut descriptor = provider.descriptor();
        if let Some(verification) = read(&self.verifications).get(id) {
            apply_verification(&mut descriptor, verification);
        }
        descriptor.state = if self.active_id() == id {
            if descriptor.available {
                ProviderState::Ready
            } else {
                descriptor.state
            }
        } else if descriptor.available {
            ProviderState::Idle
        } else {
            descriptor.state
        };
        Ok(descriptor)
    }

    pub fn descriptors(&self) -> Vec<ProviderDescriptor> {
        let ids: Vec<String> = read(&self.providers).keys().cloned().collect();
        ids.into_iter()
            .filter_map(|id| self.descriptor(&id).ok())
            .collect()
    }

    pub fn health(&self, id: &str) -> Option<ProviderHealth> {
        read(&self.providers)
            .get(id)
            .map(|provider| provider.health_check())
    }

    /// 分析一张照片。返回完整结果（供命令层写入 FaceRepository）与 UI 记录。
    pub fn analyze(
        &self,
        photo_path: &str,
        image: &Image16,
        cancel: &CancelToken,
    ) -> Result<(FaceAnalysisResult, FaceAnalysisRecord), FaceError> {
        let provider = self.active_provider()?;
        let options = AnalysisOptions::default();
        let result = provider.analyze(&FaceInput::new(image), &options, cancel)?;
        let descriptor = self.descriptor(provider.id())?;
        let stability = read(&self.verifications)
            .get(provider.id())
            .map(|verification| verification.stability.clone());
        let mut faces = Vec::with_capacity(result.faces.len());
        for face in &result.faces {
            let dense = dense::assemble_dense(face, &result);
            let quality = quality::evaluate_face_quality(&dense, &self.thresholds);
            let mut gate = if descriptor.capabilities.dense_landmarks {
                quality::level3_gate(
                    if dense.regions.is_empty() {
                        None
                    } else {
                        Some(&dense)
                    },
                    &descriptor,
                    stability.as_ref(),
                    &self.thresholds,
                )
            } else {
                // 稀疏 Provider 已经完成分析，必须说明“是能力不足”，
                // 不能误导用户去重复运行分析。
                Level3Gate::locked(
                    &descriptor.id,
                    &face.face_uuid,
                    &format!(
                        "{} 只输出 {} 个稀疏关键点，不提供眼睑/鼻翼/唇轮廓；LEVEL 3 保持锁定，LEVEL 1/2 正常可用",
                        descriptor.display_name,
                        face.landmarks.len()
                    ),
                )
            };
            gate.face_uuid = face.face_uuid.clone();
            gate.topology_id = result.topology_id.clone();
            faces.push(AnalyzedFace {
                face_uuid: face.face_uuid.clone(),
                bounding_box: face.bounding_box,
                confidence: face.confidence,
                dense_points: dense.point_count(),
                dense_fingerprint: if dense.regions.is_empty() {
                    None
                } else {
                    Some(dense.fingerprint())
                },
                quality,
                gate,
            });
        }
        let record = FaceAnalysisRecord {
            photo_path: photo_path.to_string(),
            provider_id: descriptor.id.clone(),
            provider_name: descriptor.display_name.clone(),
            topology_id: result.topology_id.clone(),
            processing_time_ms: result.processing_time_ms,
            image_width: result.image_width,
            image_height: result.image_height,
            faces,
            stability,
            warnings: result.warnings.clone(),
            total_landmarks: result.total_landmarks(),
            landmark_counts: result
                .faces
                .iter()
                .flat_map(|face| face.regions.iter())
                .fold(BTreeMap::new(), |mut counts, region| {
                    *counts
                        .entry(region.kind.label().to_string())
                        .or_insert(0usize) += region.points.len();
                    counts
                }),
            created_at_unix: now_unix(),
        };
        *write(&self.last) = Some(record.clone());
        Ok((result, record))
    }

    /// 重复分析质量探针。`runs` 次固定分析同一张照片，测量点位移与结构。
    pub fn probe_quality(
        &self,
        image: &Image16,
        cancel: &CancelToken,
        runs: usize,
    ) -> Result<QualityReport, FaceError> {
        let provider = self.active_provider()?;
        let descriptor = self.descriptor(provider.id())?;
        let runs = runs.clamp(2, 5);
        // 5 点 Provider 没有可测的稠密结果：不做无意义的重复推理，
        // 直接给出“不适用”的结论，避免 UI 显示假数字。
        if !descriptor.capabilities.dense_landmarks {
            let note = format!(
                "{} 无稠密关键点能力，质量探针不适用；LEVEL 3 不参与解锁",
                descriptor.display_name
            );
            let stability = StabilityReport {
                provider_id: descriptor.id.clone(),
                topology_id: String::new(),
                runs: 0,
                faces_per_run: Vec::new(),
                eye_distance: 0.0,
                max_deviation: 0.0,
                ratio: 1.0,
                passed: false,
                elapsed_ms: Vec::new(),
                note: note.clone(),
            };
            let report = QualityReport {
                provider_id: descriptor.id.clone(),
                provider_name: descriptor.display_name.clone(),
                stability: stability.clone(),
                quality_sample: Vec::new(),
                verified: false,
                note: note.clone(),
                measured_at_unix: now_unix(),
            };
            write(&self.verifications).insert(
                descriptor.id.clone(),
                Verification {
                    stability,
                    verified: false,
                    measured_at_unix: report.measured_at_unix,
                    note,
                },
            );
            return Ok(report);
        }
        let longest = image.width().max(image.height());
        let probe_image = if longest > 900 {
            let scale = 900.0 / longest as f32;
            image::imageops::thumbnail(
                image,
                ((image.width() as f32 * scale).round() as u32).max(1),
                ((image.height() as f32 * scale).round() as u32).max(1),
            )
        } else {
            image.clone()
        };
        let options = AnalysisOptions {
            max_edge: 900,
            ..Default::default()
        };
        let mut dense_runs: Vec<Vec<DenseFace>> = Vec::new();
        let mut elapsed = Vec::new();
        let mut first_result: Option<FaceAnalysisResult> = None;
        for _ in 0..runs {
            if cancel.is_cancelled() {
                return Err(FaceError::Cancelled);
            }
            let started = std::time::Instant::now();
            let result = provider.analyze(&FaceInput::new(&probe_image), &options, cancel)?;
            elapsed.push(started.elapsed().as_millis() as u64);
            let dense: Vec<DenseFace> = result
                .faces
                .iter()
                .map(|face| dense::assemble_dense(face, &result))
                .filter(|face| !face.regions.is_empty())
                .collect();
            dense_runs.push(dense);
            if first_result.is_none() {
                first_result = Some(result);
            }
        }
        let sample = dense_runs.first().cloned().unwrap_or_default();
        let quality_sample: Vec<FaceQuality> = sample
            .iter()
            .map(|face| quality::evaluate_face_quality(face, &self.thresholds))
            .collect();
        let quality_passed = quality_sample
            .iter()
            .filter(|quality| quality.passed)
            .count();
        let quality_total = quality_sample.len();
        let topology = first_result
            .as_ref()
            .map(|result| result.topology_id.clone())
            .unwrap_or_default();
        let mut stability = quality::measure_stability(
            provider.id(),
            &topology,
            &dense_runs,
            &elapsed,
            &self.thresholds,
        );
        if quality_total == 0 {
            stability.passed = false;
            stability.note = format!(
                "{} 没有稠密关键点输出，LEVEL 3 不参与解锁",
                descriptor.display_name
            );
        }
        let has_dense_capability = descriptor.capabilities.dense_landmarks;
        let verified = has_dense_capability
            && stability.passed
            && quality_total > 0
            && quality_passed == quality_total;
        let note = if !has_dense_capability {
            format!(
                "{} 无稠密关键点能力，不参与 LEVEL 3",
                descriptor.display_name
            )
        } else if verified {
            format!(
                "{} 次重复最大偏差 {:.3}% 眼距；{} 张人脸结构校验全部通过",
                runs,
                stability.ratio * 100.0,
                quality_total
            )
        } else if !stability.passed {
            format!("稳定性未达标：{}", stability.note)
        } else {
            format!(
                "稳定性达标，但 {}/{} 张人脸结构校验未通过",
                quality_total - quality_passed,
                quality_total
            )
        };
        let report = QualityReport {
            provider_id: descriptor.id.clone(),
            provider_name: descriptor.display_name.clone(),
            stability: stability.clone(),
            quality_sample,
            verified,
            note,
            measured_at_unix: now_unix(),
        };
        write(&self.verifications).insert(
            descriptor.id.clone(),
            Verification {
                stability,
                verified,
                measured_at_unix: report.measured_at_unix,
                note: report.note.clone(),
            },
        );
        Ok(report)
    }

    pub fn quality_reports(&self) -> Vec<QualityReport> {
        let providers = read(&self.providers);
        read(&self.verifications)
            .iter()
            .filter_map(|(id, verification)| {
                let descriptor = providers.get(id)?.descriptor();
                Some(QualityReport {
                    provider_id: id.clone(),
                    provider_name: descriptor.display_name,
                    stability: verification.stability.clone(),
                    quality_sample: Vec::new(),
                    verified: verification.verified,
                    note: verification.note.clone(),
                    measured_at_unix: verification.measured_at_unix,
                })
            })
            .collect()
    }

    pub fn status(&self) -> FaceIntelligenceStatus {
        let last = read(&self.last).clone();
        let level3_summary = match &last {
            Some(record) => record.gate_summary(),
            None => "尚未运行 AI 人像分析".into(),
        };
        FaceIntelligenceStatus {
            active_provider: self.active_id(),
            providers: self.descriptors(),
            last_analysis: last,
            quality: self.quality_reports(),
            level3_summary,
            models_root: self.models_root.to_string_lossy().into_owned(),
            thresholds: self.thresholds.clone(),
        }
    }

    pub fn last_analysis(&self) -> Option<FaceAnalysisRecord> {
        read(&self.last).clone()
    }

    /// 模型变化后重建 Provider 集合，保留验证缓存与当前选择（若仍存在）。
    pub fn reload_models(&self) {
        let installed = self.model_hub.list();
        let mut providers: BTreeMap<String, Arc<dyn FaceProvider>> = BTreeMap::new();
        providers.insert(
            yunet::PROVIDER_ID.into(),
            Arc::new(yunet::YunetProvider::new(self.yunet_path.clone())),
        );
        if apple_vision::AppleVisionProvider::available() {
            providers.insert(
                apple_vision::PROVIDER_ID.into(),
                Arc::new(apple_vision::AppleVisionProvider::new()),
            );
        }
        for model in &installed {
            if let Ok(provider) = onnx::OnnxLandmarkProvider::load(model, self.yunet_path.clone()) {
                providers.insert(provider.id().to_string(), Arc::new(provider));
            }
        }
        let active = self.active_id();
        if !providers.contains_key(&active) {
            *write(&self.active) = yunet::PROVIDER_ID.into();
        }
        *write(&self.providers) = providers;
        *write(&self.installed) = installed;
    }

    /// 最近一次分析的稠密结果（供切换到新 Provider 后做 UUID 映射）。
    pub fn installed_models(&self) -> Vec<InstalledModel> {
        read(&self.installed).clone()
    }

    /// 卸载指定 Provider 的模型缓存，不影响其他 Provider。
    pub fn unload(&self, id: &str) -> bool {
        if let Some(provider) = read(&self.providers).get(id).cloned() {
            provider.unload();
            true
        } else {
            false
        }
    }
}

fn apply_verification(descriptor: &mut ProviderDescriptor, verification: &Verification) {
    for evidence in descriptor.evidence.iter_mut() {
        if !matches!(
            evidence.capability.as_str(),
            "dense_landmarks"
                | "eye_landmarks"
                | "eyelid_landmarks"
                | "nose_landmarks"
                | "lip_landmarks"
        ) {
            continue;
        }
        if verification.verified {
            evidence.level = EvidenceLevel::Verified;
            evidence.measurement = Some(verification.note.clone());
            evidence.note = format!("本轮本机实测：{}", verification.note);
        } else if evidence.level == EvidenceLevel::Declared {
            evidence.level = EvidenceLevel::Unverified;
            evidence.note = format!("实测未通过：{}", verification.note);
        }
    }
    descriptor.verified_capabilities = descriptor.capabilities.verified_only(&descriptor.evidence);
}

fn read<T>(lock: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn write<T>(lock: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    lock.write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::face_intelligence::capability::LicenseState;

    #[test]
    fn gate_summary_mentions_locked_state() {
        let gate = Level3Gate::locked("p", "", "未分析");
        assert!(summary_for_gates(std::iter::once(&gate)).contains("未解锁"));
    }

    #[test]
    fn gate_summary_lists_unlocked_regions() {
        let mut gate = Level3Gate::locked("p", "", "x");
        gate.eyes.enabled = true;
        gate.eyes.reason = "ok".into();
        let summary = summary_for_gates(std::iter::once(&gate));
        assert!(summary.contains("眼部"));
        assert!(summary.contains("鼻部"));
    }

    fn user_installed_model() -> InstalledModel {
        use crate::face_intelligence::model_hub::{
            InputSpec, LicenseDeclaration, ModelManifest, ModelTask, Normalization,
            OutputCoordinate, OutputLayout, OutputSpec, TensorLayout,
        };
        InstalledModel {
            manifest: ModelManifest {
                schema_version: crate::face_intelligence::model_hub::manifest::SCHEMA_VERSION,
                id: "user-model".into(),
                name: "用户模型".into(),
                version: "1.0".into(),
                author: String::new(),
                description: String::new(),
                task: ModelTask::FaceLandmark,
                topology_id: "face-68".into(),
                input: InputSpec {
                    width: 256,
                    height: 256,
                    layout: TensorLayout::Nchw,
                    normalization: Normalization::ZeroToOne,
                    crop_margin: 0.15,
                },
                output: OutputSpec {
                    layout: OutputLayout::N2,
                    coordinate: OutputCoordinate::NormalizedCrop,
                },
                license: LicenseDeclaration {
                    name: "MIT".into(),
                    url: None,
                    commercial_use_claimed: true,
                    redistribution_claimed: true,
                    note: "用户声明".into(),
                },
                sha256: "0".repeat(64),
                file_name: "model.onnx".into(),
                source_url: None,
                source_path: String::new(),
                imported_at: "unix:0".into(),
                validation: None,
            },
            directory: String::new(),
            model_path: String::new(),
            present: true,
            sha256_matches: true,
            validation_ok: true,
            error: None,
        }
    }

    #[test]
    fn user_model_license_is_never_official() {
        // 即使 manifest 声明 MIT 且允许商用/再分发，仍必须是 UserUnverified。
        assert_eq!(
            ModelHub::license_state(&user_installed_model()),
            LicenseState::UserUnverified
        );
    }
}
