use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::face_reshape::identity::ProjectFace;
use crate::image_engine::export::ExportSettings;
use crate::image_engine::params::{BodyShape, EditParams};

/// 版本 3：新增人物对象、蒙版元数据与身材/背景参数。
/// 版本 1/2 项目仍然可以打开，缺失字段使用默认值。
pub const PROJECT_VERSION: u32 = 3;
pub const PROJECT_EXTENSION: &str = "lumi";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MaskKind {
    #[default]
    Brush,
    Linear,
    Radial,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MaskPoint {
    pub x: f32,
    pub y: f32,
    #[serde(default = "default_radius")]
    pub radius: f32,
}

fn default_radius() -> f32 {
    0.08
}

/// 蒙版层只保存可重放的矢量数据，不保存像素结果，
/// 因此重新打开项目后仍可无损调整。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaskLayer {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub kind: MaskKind,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub invert: bool,
    #[serde(default = "default_feather")]
    pub feather: f32,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    #[serde(default)]
    pub points: Vec<MaskPoint>,
}

fn default_true() -> bool {
    true
}

fn default_feather() -> f32 {
    40.0
}

fn default_opacity() -> f32 {
    100.0
}

/// 第五轮人物记录。`subject_id` 与 `params.body[].subjectId` 一致，
/// 不复用人脸 UUID，避免身材与人脸参数在重开后错配。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectSubject {
    pub subject_id: String,
    pub mask_source: String,
    pub mask_model_version: String,
    pub mask_strokes: usize,
    pub body: BodyShape,
    pub anchors: usize,
}

/// 蒙版来源与版本。不保存 Alpha 像素，只保存可重放的笔触与哈希。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct MaskMeta {
    /// 当前固定为 `manual`；未来 AI 模型可用时写入模型 ID。
    pub source: String,
    pub model_id: String,
    pub model_version: String,
    /// 最近一次渲染的覆盖率。由前端或渲染报告回填，无法计算时为 0。
    pub coverage: f32,
    pub stroke_hash: String,
    pub revision: u32,
}

/// 第七轮：来源信息（RAW/Raster）、RAW 显影参数、色彩输出设置与计算后端。
/// 字段全部可选，旧项目（v1/v2/v3）缺失时按默认值处理，不要求迁移。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectSource {
    /// `raw` 或 `raster`。
    pub source_type: String,
    pub path: String,
    pub hash: String,
    /// 相机/文件元数据快照（JSON，仅用于展示，不参与渲染）。
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectRaw {
    pub params: crate::image_engine::raw::RawDevelopParams,
    /// 解码器版本；变化时提示可能需要重新渲染。
    pub decoder_version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectColor {
    pub output: crate::color_management::ColorOutputSettings,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectCompute {
    pub preference: crate::gpu::BackendPreference,
}

/// 当前 RAW 解码/显影实现版本，写入项目用于提示重渲染。
pub const RAW_DECODER_VERSION: &str = "libraw-0.21.1+lumi-mhc-1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectPhoto {
    pub path: String,
    pub params: EditParams,
    pub masks: Vec<MaskLayer>,
    pub export: Option<ExportSettings>,
    /// 原图文件 SHA-256。重开时若变化，不自动套用任何人脸参数。
    pub source_hash: String,
    /// 持久化人脸记录：UUID、检测框、初始关键点、置信度、模型版本。
    pub faces: Vec<ProjectFace>,
    /// 第五轮人物记录（身材对象、独立蒙版笔触数量、锚点数量）。
    pub subjects: Vec<ProjectSubject>,
    /// 第五轮蒙版元数据。
    pub mask_meta: Option<MaskMeta>,
    /// 第七轮：来源文件信息（RAW 缺失时保留参数并提示重定位）。
    pub source: Option<ProjectSource>,
    /// 第七轮：RAW 显影参数。
    pub raw: Option<ProjectRaw>,
    /// 第七轮：色彩输出设置。
    pub color: Option<ProjectColor>,
}

impl ProjectPhoto {
    /// 保存前把第五轮记录与 `params` 同步，避免两份数据不一致。
    pub fn sync_subject_records(&mut self) {
        self.subjects = self
            .params
            .body
            .iter()
            .map(|entry| ProjectSubject {
                subject_id: entry.subject_id.clone(),
                mask_source: if entry.has_own_mask() {
                    "manual".into()
                } else {
                    self.params.subject_mask.source.clone()
                },
                mask_model_version: self.params.subject_mask.model_version.clone(),
                mask_strokes: entry.mask_strokes.len(),
                body: entry.shape,
                anchors: entry.anchors.len(),
            })
            .collect();
        let mask = &self.params.subject_mask;
        self.mask_meta = Some(MaskMeta {
            source: mask.source.clone(),
            model_id: "manual-alpha".into(),
            model_version: mask.model_version.clone(),
            coverage: 0.0,
            stroke_hash: mask.stroke_hash(),
            revision: 1,
        });
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDoc {
    pub version: u32,
    pub created_at: String,
    #[serde(default)]
    pub active_index: usize,
    #[serde(default)]
    pub export: ExportSettings,
    /// 第七轮：计算后端偏好（Auto/Gpu/Cpu）。
    #[serde(default)]
    pub compute: Option<ProjectCompute>,
    pub photos: Vec<ProjectPhoto>,
}

impl ProjectDoc {
    pub fn new(photos: Vec<ProjectPhoto>, active_index: usize, export: ExportSettings) -> Self {
        Self {
            version: PROJECT_VERSION,
            created_at: now_label(),
            active_index,
            export,
            compute: None,
            photos,
        }
    }
}

fn now_label() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("{seconds}")
}

/// 项目文件是纯 JSON（参数 + 蒙版 + 导出设置），不包含预览图。
/// 写入使用同目录临时文件 + rename，避免保存中断损坏已有项目。
pub fn save(path: &Path, project: &ProjectDoc) -> AppResult<()> {
    if project.version > PROJECT_VERSION {
        return Err(AppError::Unsupported(format!(
            "项目版本 {} 高于当前支持的 {}",
            project.version, PROJECT_VERSION
        )));
    }
    let mut project = project.clone();
    for photo in project.photos.iter_mut() {
        photo.sync_subject_records();
    }
    let json = serde_json::to_vec_pretty(&project)
        .map_err(|error| AppError::Task(format!("项目序列化失败：{error}")))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("lumi.tmp");
    std::fs::write(&temporary, json)?;
    std::fs::rename(&temporary, path)?;
    Ok(())
}

pub fn load(path: &Path) -> AppResult<ProjectDoc> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > 64 * 1024 * 1024 {
        return Err(AppError::Unsupported("项目文件超过 64MB".into()));
    }
    let bytes = std::fs::read(path)?;
    let project: ProjectDoc = serde_json::from_slice(&bytes)
        .map_err(|error| AppError::Decode(format!("项目文件格式错误：{error}")))?;
    if project.version > PROJECT_VERSION {
        return Err(AppError::Unsupported(format!(
            "项目版本 {} 高于当前支持的 {}",
            project.version, PROJECT_VERSION
        )));
    }
    if project.photos.is_empty() {
        return Err(AppError::Decode("项目中没有照片".into()));
    }
    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_engine::params::{HslChannel, HslParams};

    #[test]
    fn project_roundtrip_keeps_params_and_masks() {
        let dir = std::env::temp_dir().join("lumi-project-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("portrait.lumi");

        let params = EditParams {
            exposure: 24.0,
            hsl: HslParams {
                orange: HslChannel {
                    saturation: 18.0,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        let project = ProjectDoc::new(
            vec![ProjectPhoto {
                path: "/photos/portrait.jpg".into(),
                params,
                masks: vec![MaskLayer {
                    id: "m1".into(),
                    name: "面部提亮".into(),
                    kind: MaskKind::Radial,
                    enabled: true,
                    invert: false,
                    feather: 30.0,
                    opacity: 80.0,
                    points: vec![MaskPoint {
                        x: 0.5,
                        y: 0.4,
                        radius: 0.2,
                    }],
                }],
                export: Some(ExportSettings::default()),
                source_hash: "abc123".into(),
                faces: Vec::new(),
                subjects: Vec::new(),
                mask_meta: None,
                source: None,
                raw: None,
                color: None,
            }],
            0,
            ExportSettings::default(),
        );
        save(&path, &project).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.photos.len(), 1);
        assert_eq!(loaded.photos[0].params.exposure, 24.0);
        assert_eq!(loaded.photos[0].masks.len(), 1);
        assert_eq!(loaded.photos[0].masks[0].kind, MaskKind::Radial);
        assert_eq!(loaded.photos[0].source_hash, "abc123");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn project_v3_keeps_raw_color_and_compute() {
        let dir = std::env::temp_dir().join("lumi-project-v3-raw");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("raw.lumi");

        let mut photo = ProjectPhoto {
            path: "/photos/sample.CR3".into(),
            source_hash: "hash".into(),
            source: Some(ProjectSource {
                source_type: "raw".into(),
                path: "/photos/sample.CR3".into(),
                hash: "hash".into(),
                metadata: Some(serde_json::json!({"make": "Canon"})),
            }),
            ..Default::default()
        };
        photo.raw = Some(ProjectRaw {
            params: crate::image_engine::raw::RawDevelopParams {
                exposure_ev: 0.75,
                wb_mode: crate::image_engine::raw::WbMode::Custom,
                max_edge: None,
                ..Default::default()
            },
            decoder_version: RAW_DECODER_VERSION.into(),
        });
        photo.color = Some(ProjectColor {
            output: crate::color_management::ColorOutputSettings {
                profile: crate::color_management::ProfileKind::AdobeRgb,
                ..Default::default()
            },
        });
        let mut project = ProjectDoc::new(vec![photo], 0, ExportSettings::default());
        project.compute = Some(ProjectCompute {
            preference: crate::gpu::BackendPreference::Gpu,
        });

        save(&path, &project).unwrap();
        let loaded = load(&path).unwrap();
        let photo = &loaded.photos[0];
        assert_eq!(photo.source.as_ref().unwrap().source_type, "raw");
        let raw = photo.raw.as_ref().unwrap();
        assert!((raw.params.exposure_ev - 0.75).abs() < 1e-6);
        assert_eq!(raw.decoder_version, RAW_DECODER_VERSION);
        assert_eq!(
            photo.color.as_ref().unwrap().output.profile,
            crate::color_management::ProfileKind::AdobeRgb
        );
        assert_eq!(
            loaded.compute.as_ref().unwrap().preference,
            crate::gpu::BackendPreference::Gpu
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn project_without_round7_fields_still_opens() {
        let dir = std::env::temp_dir().join("lumi-project-legacy");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("legacy.lumi");
        // 模拟第六轮项目：没有 source/raw/color/compute。
        let json = serde_json::json!({
            "version": 2,
            "createdAt": "1",
            "activeIndex": 0,
            "export": {},
            "photos": [{
                "path": "/photos/a.jpg",
                "params": {},
                "masks": [],
                "export": null,
                "sourceHash": "abc",
                "faces": [],
                "subjects": [],
                "maskMeta": null
            }]
        });
        std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.photos.len(), 1);
        assert!(loaded.photos[0].source.is_none());
        assert!(loaded.photos[0].raw.is_none());
        assert!(loaded.photos[0].color.is_none());
        assert!(loaded.compute.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn project_roundtrip_keeps_face_identity_records() {
        use crate::ai_engine::face_frame::NormPoint;
        use crate::face_reshape::identity::FaceBox;

        let dir = std::env::temp_dir().join("lumi-project-faces");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("group.lumi");
        let face = ProjectFace {
            uuid: "11111111-2222-4333-8444-555555555555".into(),
            bbox: FaceBox {
                x: 0.1,
                y: 0.2,
                width: 0.2,
                height: 0.24,
            },
            landmarks: vec![NormPoint { x: 0.15, y: 0.3 }; 5],
            score: 0.94,
            model: "yunet-5".into(),
            confidence: 0.94,
            mask_ref: None,
            dense: None,
            level3: None,
        };
        let project = ProjectDoc::new(
            vec![ProjectPhoto {
                path: "/photos/group.jpg".into(),
                params: EditParams::default(),
                masks: Vec::new(),
                export: None,
                source_hash: "deadbeef".into(),
                faces: vec![face.clone()],
                subjects: Vec::new(),
                mask_meta: None,
                source: None,
                raw: None,
                color: None,
            }],
            0,
            ExportSettings::default(),
        );
        save(&path, &project).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.version, PROJECT_VERSION);
        assert_eq!(loaded.photos[0].faces, vec![face]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn v3_roundtrip_keeps_subject_body_and_mask_meta() {
        use crate::image_engine::params::{
            BackgroundMode, BackgroundParams, BodyAdjust, BodyShape, MaskPoint, SubjectMaskParams,
            SubjectMaskStroke, SubjectMaskStrokeKind,
        };
        let dir = std::env::temp_dir().join("lumi-project-v3");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("v3.lumi");
        let params = EditParams {
            subject_mask: SubjectMaskParams {
                enabled: true,
                strokes: vec![SubjectMaskStroke {
                    kind: SubjectMaskStrokeKind::Paint,
                    radius: 0.2,
                    hardness: 70.0,
                    strength: 100.0,
                    points: vec![MaskPoint { x: 0.5, y: 0.6 }],
                }],
                ..Default::default()
            },
            body: vec![BodyAdjust {
                subject_id: "subject-uuid-1".into(),
                enabled: true,
                shape: BodyShape {
                    waist: -30.0,
                    ..Default::default()
                },
                anchors: Vec::new(),
                strokes: Vec::new(),
                mask_strokes: Vec::new(),
            }],
            background: BackgroundParams {
                enabled: true,
                mode: BackgroundMode::SolidColor,
                color: "#102030".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let project = ProjectDoc::new(
            vec![ProjectPhoto {
                path: "/photos/body.jpg".into(),
                params,
                ..Default::default()
            }],
            0,
            ExportSettings::default(),
        );
        save(&path, &project).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.version, PROJECT_VERSION);
        let photo = &loaded.photos[0];
        assert_eq!(photo.params.body[0].subject_id, "subject-uuid-1");
        assert_eq!(photo.params.body[0].shape.waist, -30.0);
        assert_eq!(photo.params.background.mode, BackgroundMode::SolidColor);
        assert_eq!(photo.subjects.len(), 1);
        assert_eq!(photo.subjects[0].subject_id, "subject-uuid-1");
        let meta = photo.mask_meta.as_ref().unwrap();
        assert_eq!(meta.source, "manual");
        assert!(!meta.stroke_hash.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn v2_project_without_new_fields_loads_with_defaults() {
        let dir = std::env::temp_dir().join("lumi-project-v2");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("v2.lumi");
        std::fs::write(
            &path,
            br#"{"version":2,"createdAt":"0","activeIndex":0,"export":{"format":"jpeg","quality":92},"photos":[{"path":"/legacy.jpg","params":{"exposure":12}}]}"#,
        )
        .unwrap();
        let loaded = load(&path).unwrap();
        let photo = &loaded.photos[0];
        assert!(photo.params.body.is_empty());
        assert!(!photo.params.background.is_active());
        assert!(photo.subjects.is_empty());
        assert!(photo.mask_meta.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn future_version_is_rejected() {
        let dir = std::env::temp_dir().join("lumi-project-version");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("future.lumi");
        std::fs::write(
            &path,
            br#"{"version":99,"createdAt":"0","activeIndex":0,"export":{"format":"jpeg","quality":92},"photos":[{"path":"/a.jpg"}]}"#,
        )
        .unwrap();
        let error = load(&path).unwrap_err();
        assert!(error.to_string().contains("版本"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn empty_project_is_rejected() {
        let dir = std::env::temp_dir().join("lumi-project-empty");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("empty.lumi");
        std::fs::write(
            &path,
            br#"{"version":1,"createdAt":"0","activeIndex":0,"export":{"format":"jpeg","quality":92},"photos":[]}"#,
        )
        .unwrap();
        assert!(load(&path).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
