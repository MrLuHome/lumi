//! Apple Vision 人脸关键点 Provider（macOS）。
//!
//! 这是本轮唯一接入的**真实稠密** Provider：
//! - 使用系统 `Vision.framework`，不随包分发任何权重；
//! - 输出图像左上角归一化坐标，已由 `vision_bridge.m` 完成翻转；
//! - 点按区域语义 ID 输出，下游不依赖数组下标；
//! - Apple Vision 不提供 Face Parsing / Matting / 三维网格，对应能力明确为不支持。

use std::collections::BTreeMap;
use std::time::Instant;

use serde::Deserialize;

use crate::face_intelligence::capability::{
    CapabilityEvidence, EvidenceLevel, LicenseInfo, LicenseState, ProviderCapabilities,
    ProviderDescriptor, ProviderKind, ProviderState,
};
use crate::face_intelligence::provider::{
    sanitize_result, AnalysisOptions, FaceError, FaceInput, FaceProvider, ProviderHealth,
};
use crate::face_intelligence::topology::APPLE_VISION;
use crate::face_intelligence::types::{
    BoundingBox, DenseRegion, FaceAnalysisResult, FaceResult, Landmark, RegionKind,
};
use crate::image_engine::buffer::Image16;
use crate::image_engine::cancel::CancelToken;

pub const PROVIDER_ID: &str = "apple-vision";

#[derive(Debug, Deserialize)]
struct RawPayload {
    #[serde(default)]
    faces: Vec<RawFace>,
    #[serde(default)]
    revision: u32,
}

#[derive(Debug, Deserialize)]
struct RawFace {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    #[serde(default = "default_confidence")]
    confidence: f32,
    #[serde(default)]
    regions: BTreeMap<String, Vec<RawPoint>>,
}

fn default_confidence() -> f32 {
    0.9
}

#[derive(Debug, Deserialize)]
struct RawPoint {
    x: f32,
    y: f32,
    #[serde(default = "default_confidence")]
    confidence: f32,
}

/// 区域键 → 语义区域类型。未知键不会被猜测成五官。
fn region_kind(key: &str) -> Option<RegionKind> {
    match key {
        "faceContour" => Some(RegionKind::FaceContour),
        "leftEye" => Some(RegionKind::LeftEye),
        "rightEye" => Some(RegionKind::RightEye),
        "leftEyebrow" => Some(RegionKind::LeftEyebrow),
        "rightEyebrow" => Some(RegionKind::RightEyebrow),
        "nose" => Some(RegionKind::Nose),
        "noseCrest" => Some(RegionKind::NoseCrest),
        "medianLine" => Some(RegionKind::MedianLine),
        "outerLips" => Some(RegionKind::OuterLips),
        "innerLips" => Some(RegionKind::InnerLips),
        _ => None,
    }
}

pub struct AppleVisionProvider;

impl AppleVisionProvider {
    pub fn new() -> Self {
        Self
    }

    pub fn available() -> bool {
        cfg!(target_os = "macos")
    }

    pub fn base_descriptor() -> ProviderDescriptor {
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
                "macOS 静态照片实测：单人/多人/深色/过曝样张均返回人脸框",
            ),
            CapabilityEvidence::new(
                "dense_landmarks",
                EvidenceLevel::Declared,
                "系统框架返回脸缘/眼睑/鼻/唇区域，等待重复稳定性与结构实测",
            ),
            CapabilityEvidence::new(
                "eye_landmarks",
                EvidenceLevel::Declared,
                "返回左右眼轮廓，待质量探针确认",
            ),
            CapabilityEvidence::new(
                "eyelid_landmarks",
                EvidenceLevel::Declared,
                "眼睑点为真实轮廓点，非插值",
            ),
            CapabilityEvidence::new(
                "nose_landmarks",
                EvidenceLevel::Declared,
                "返回鼻部轮廓与鼻梁，含鼻翼外侧实际点",
            ),
            CapabilityEvidence::new(
                "lip_landmarks",
                EvidenceLevel::Declared,
                "返回外唇与内唇轮廓",
            ),
            CapabilityEvidence::new(
                "face_parsing",
                EvidenceLevel::Unsupported,
                "Vision 关键点请求不输出逐像素语义分割",
            ),
            CapabilityEvidence::new(
                "face_matting",
                EvidenceLevel::Unsupported,
                "Vision 关键点请求不输出头发/皮肤 Matting",
            ),
            CapabilityEvidence::new(
                "face_mesh",
                EvidenceLevel::Unsupported,
                "没有三维拓扑网格输出",
            ),
        ];
        let available = Self::available();
        ProviderDescriptor {
            id: PROVIDER_ID.into(),
            display_name: "Apple Vision（系统）".into(),
            version: "VNDetectFaceLandmarksRequest".into(),
            kind: ProviderKind::System,
            state: if available {
                ProviderState::Idle
            } else {
                ProviderState::Unsupported
            },
            available,
            enabled: available,
            license: LicenseInfo {
                name: "Apple SDK / Vision.framework".into(),
                url: Some("https://developer.apple.com/documentation/vision".into()),
                state: if available {
                    LicenseState::SystemFramework
                } else {
                    LicenseState::Incompatible
                },
                note: "不随包分发模型权重；本地处理，不上传图像；商用发布需遵守 Apple SDK 条款。"
                    .into(),
            },
            capabilities,
            verified_capabilities: capabilities.verified_only(&evidence),
            evidence,
            topology_id: Some(APPLE_VISION.id.into()),
            note: if available {
                "本地系统框架，支持多人脸静态照片；不提供分割与网格".into()
            } else {
                "当前平台不支持 Apple Vision".into()
            },
        }
    }
}

impl Default for AppleVisionProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl FaceProvider for AppleVisionProvider {
    fn id(&self) -> &str {
        PROVIDER_ID
    }

    fn descriptor(&self) -> ProviderDescriptor {
        Self::base_descriptor()
    }

    fn analyze(
        &self,
        input: &FaceInput<'_>,
        options: &AnalysisOptions,
        cancel: &CancelToken,
    ) -> Result<FaceAnalysisResult, FaceError> {
        if !Self::available() {
            return Err(FaceError::Unavailable("当前平台没有 Apple Vision".into()));
        }
        if input.image.width() == 0 || input.image.height() == 0 {
            return Err(FaceError::InvalidInput("输入图像为空".into()));
        }
        if cancel.is_cancelled() {
            return Err(FaceError::Cancelled);
        }

        let started = Instant::now();
        let analysis = downscale(input.image, options.max_edge);
        let (width, height) = analysis.dimensions();
        let rgba = to_rgba8(&analysis);

        let raw = unsafe { call_bridge(&rgba, width, height)? };
        let payload: RawPayload = serde_json::from_str(&raw)
            .map_err(|error| FaceError::Inference(format!("Vision 结果解析失败：{error}")))?;

        let mut faces = Vec::with_capacity(payload.faces.len());
        for raw_face in payload.faces {
            if cancel.is_cancelled() {
                return Err(FaceError::Cancelled);
            }
            let bbox = BoundingBox {
                x: raw_face.x,
                y: raw_face.y,
                width: raw_face.width,
                height: raw_face.height,
            }
            .clamp_unit();
            let mut regions = Vec::new();
            for (key, points) in raw_face.regions {
                let Some(kind) = region_kind(&key) else {
                    continue;
                };
                let prefix = kind.prefix();
                let landmarks: Vec<Landmark> = points
                    .into_iter()
                    .enumerate()
                    .map(|(index, point)| {
                        Landmark::new(
                            point.x,
                            point.y,
                            point.confidence.max(0.0),
                            format!("{prefix}.{index}"),
                        )
                    })
                    .collect();
                if landmarks.is_empty() {
                    continue;
                }
                regions.push(DenseRegion {
                    kind,
                    points: landmarks,
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
            canonicalize_sides(&mut regions);
            let landmarks = regions
                .iter()
                .flat_map(|region| region.points.iter().cloned())
                .collect();
            faces.push(FaceResult {
                face_uuid: String::new(),
                bounding_box: bbox,
                confidence: raw_face.confidence.clamp(0.0, 1.0),
                landmarks,
                regions,
                masks: Vec::new(),
                mesh: None,
            });
        }
        // 固定排序，避免系统返回顺序抖动影响“重复分析”比较。
        faces.sort_by(|a, b| {
            let (ax, ay) = a.bounding_box.center();
            let (bx, by) = b.bounding_box.center();
            ax.partial_cmp(&bx)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(ay.partial_cmp(&by).unwrap_or(std::cmp::Ordering::Equal))
        });

        let mut warnings = Vec::new();
        for face in &faces {
            let dense = crate::face_intelligence::types::DenseFace {
                topology_id: APPLE_VISION.id.into(),
                provider_id: PROVIDER_ID.into(),
                provider_version: format!("vision-rev{}", payload.revision),
                image_width: width,
                image_height: height,
                bounding_box: face.bounding_box,
                confidence: face.confidence,
                regions: face.regions.clone(),
            };
            for issue in APPLE_VISION.validate(&dense) {
                warnings.push(issue);
            }
        }
        warnings.sort();
        warnings.dedup();

        let result = FaceAnalysisResult {
            faces,
            provider_id: PROVIDER_ID.into(),
            provider_version: format!("vision-rev{}", payload.revision),
            model_version: Some(format!("macOS Vision revision {}", payload.revision)),
            topology_id: APPLE_VISION.id.into(),
            processing_time_ms: started.elapsed().as_millis() as u64,
            image_width: input.image_width,
            image_height: input.image_height,
            warnings,
        };
        Ok(sanitize_result(result, options))
    }

    fn health_check(&self) -> ProviderHealth {
        if Self::available() {
            ProviderHealth::Ready
        } else {
            ProviderHealth::Unavailable
        }
    }
}

/// 左右侧规范化。
///
/// Vision 的 `leftEye` / `rightEye` 按观察者视角命名（图像左侧为 leftEye），
/// 而 YuNet / iBUG-68 与整形引擎统一按**被摄者的左右**（图像左侧 = 右眼）。
/// 归一化前先比较两眼的图像 x，确保所有 Provider 对同一只眼使用同一个语义 ID。
fn canonicalize_sides(regions: &mut [DenseRegion]) {
    for (a, b) in [
        (RegionKind::RightEye, RegionKind::LeftEye),
        (RegionKind::RightEyebrow, RegionKind::LeftEyebrow),
    ] {
        let mean_x = |kind: RegionKind| {
            regions
                .iter()
                .find(|region| region.kind == kind && !region.points.is_empty())
                .map(|region| {
                    region.points.iter().map(|point| point.x).sum::<f32>()
                        / region.points.len() as f32
                })
        };
        if let (Some(ax), Some(bx)) = (mean_x(a), mean_x(b)) {
            if ax > bx {
                for region in regions.iter_mut() {
                    if region.kind == a {
                        region.kind = b;
                    } else if region.kind == b {
                        region.kind = a;
                    }
                }
            }
        }
    }
    // 语义 ID 必须跟随最终 kind，否则下游会拿到自相矛盾的标签。
    for region in regions.iter_mut() {
        let prefix = region.kind.prefix();
        for (index, point) in region.points.iter_mut().enumerate() {
            point.semantic_id = format!("{prefix}.{index}");
        }
    }
}

/// 分析分辨率上限只影响推理成本；坐标始终是归一化值，不丢失与原图的对应关系。
fn downscale(image: &Image16, max_edge: u32) -> Image16 {
    let longest = image.width().max(image.height());
    if max_edge == 0 || longest <= max_edge {
        return image.clone();
    }
    let scale = max_edge as f32 / longest as f32;
    let width = ((image.width() as f32 * scale).round() as u32).max(1);
    let height = ((image.height() as f32 * scale).round() as u32).max(1);
    image::imageops::thumbnail(image, width, height)
}

fn to_rgba8(image: &Image16) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(image.width() as usize * image.height() as usize * 4);
    for pixel in image.pixels() {
        bytes.push((pixel.0[0] >> 8) as u8);
        bytes.push((pixel.0[1] >> 8) as u8);
        bytes.push((pixel.0[2] >> 8) as u8);
        bytes.push((pixel.0[3] >> 8) as u8);
    }
    bytes
}

#[cfg(target_os = "macos")]
mod bridge {
    use std::ffi::{c_char, CStr};

    unsafe extern "C" {
        fn lumi_vision_analyze(
            rgba: *const u8,
            width: i32,
            height: i32,
            error: *mut *mut c_char,
        ) -> *mut c_char;
        fn lumi_vision_free(pointer: *mut c_char);
    }

    pub unsafe fn analyze(rgba: &[u8], width: u32, height: u32) -> Result<String, String> {
        let mut error: *mut c_char = std::ptr::null_mut();
        let pointer = unsafe {
            lumi_vision_analyze(
                rgba.as_ptr(),
                width as i32,
                height as i32,
                &mut error as *mut *mut c_char,
            )
        };
        if pointer.is_null() {
            let message = if error.is_null() {
                "Vision 返回空结果".to_string()
            } else {
                let text = unsafe { CStr::from_ptr(error) }
                    .to_string_lossy()
                    .into_owned();
                unsafe { lumi_vision_free(error) };
                text
            };
            return Err(message);
        }
        let text = unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned();
        unsafe { lumi_vision_free(pointer) };
        Ok(text)
    }
}

#[cfg(target_os = "macos")]
unsafe fn call_bridge(rgba: &[u8], width: u32, height: u32) -> Result<String, FaceError> {
    bridge::analyze(rgba, width, height)
        .map_err(|message| FaceError::Inference(format!("Vision 桥接失败：{message}")))
}

#[cfg(not(target_os = "macos"))]
unsafe fn call_bridge(_rgba: &[u8], _width: u32, _height: u32) -> Result<String, FaceError> {
    Err(FaceError::Unavailable("当前平台没有 Apple Vision".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_keys_map_to_real_semantics() {
        assert_eq!(region_kind("leftEye"), Some(RegionKind::LeftEye));
        assert_eq!(region_kind("outerLips"), Some(RegionKind::OuterLips));
        assert_eq!(region_kind("unknownRegion"), None);
    }

    #[test]
    fn descriptor_never_claims_parsing_or_mesh() {
        let descriptor = AppleVisionProvider::base_descriptor();
        assert!(descriptor.capabilities.dense_landmarks);
        assert!(!descriptor.capabilities.face_parsing);
        assert!(!descriptor.capabilities.face_mesh);
        assert!(descriptor
            .evidence
            .iter()
            .any(|item| item.capability == "face_parsing"
                && item.level == EvidenceLevel::Unsupported));
    }

    #[test]
    fn raw_payload_parses_points() {
        let payload: RawPayload = serde_json::from_str(
            r#"{"revision":3,"faces":[{"x":0.1,"y":0.2,"width":0.3,"height":0.4,"confidence":0.9,
            "regions":{"leftEye":[{"x":0.2,"y":0.3,"confidence":0.8}]}}]}"#,
        )
        .unwrap();
        assert_eq!(payload.revision, 3);
        assert_eq!(payload.faces[0].regions["leftEye"][0].x, 0.2);
    }
}
