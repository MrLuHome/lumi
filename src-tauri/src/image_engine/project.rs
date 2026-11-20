use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::face_reshape::identity::ProjectFace;
use crate::image_engine::export::ExportSettings;
use crate::image_engine::params::EditParams;

/// 版本 2：新增源图哈希与持久化人脸记录（UUID / 检测框 / 关键点）。
/// 版本 1 项目仍然可以打开，缺失字段使用默认值。
pub const PROJECT_VERSION: u32 = 2;
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectPhoto {
    pub path: String,
    #[serde(default)]
    pub params: EditParams,
    #[serde(default)]
    pub masks: Vec<MaskLayer>,
    #[serde(default)]
    pub export: Option<ExportSettings>,
    /// 原图文件 SHA-256。重开时若变化，不自动套用任何人脸参数。
    #[serde(default)]
    pub source_hash: String,
    /// 持久化人脸记录：UUID、检测框、初始关键点、置信度、模型版本。
    #[serde(default)]
    pub faces: Vec<ProjectFace>,
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
    pub photos: Vec<ProjectPhoto>,
}

impl ProjectDoc {
    pub fn new(photos: Vec<ProjectPhoto>, active_index: usize, export: ExportSettings) -> Self {
        Self {
            version: PROJECT_VERSION,
            created_at: now_label(),
            active_index,
            export,
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
    let json = serde_json::to_vec_pretty(project)
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

    #[test]
    fn project_roundtrip_keeps_params_and_masks() {
        let dir = std::env::temp_dir().join("lumi-project-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("portrait.lumi");

        let mut params = EditParams::default();
        params.exposure = 24.0;
        params.hsl.orange.saturation = 18.0;
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
        };
        let project = ProjectDoc::new(
            vec![ProjectPhoto {
                path: "/photos/group.jpg".into(),
                params: EditParams::default(),
                masks: Vec::new(),
                export: None,
                source_hash: "deadbeef".into(),
                faces: vec![face.clone()],
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
