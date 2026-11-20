use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::face_reshape::identity::{self, ProjectFace};
use crate::image_engine::operations::estimate_white_balance;
use crate::image_engine::project::{self, ProjectDoc, PROJECT_EXTENSION};
use crate::state::{FaceRepository, ImageCache};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSaved {
    pub path: String,
    pub bytes: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectOpened {
    pub path: String,
    pub project: ProjectDoc,
}

/// 保存非破坏性项目：只写参数、蒙版、导出设置，不写预览图。
/// 同时写入源图哈希与当前检测出的人脸身份记录，供重开时匹配。
#[tauri::command]
pub async fn save_project(
    repo: State<'_, FaceRepository>,
    mut project: ProjectDoc,
    destination: Option<String>,
) -> AppResult<ProjectSaved> {
    let repo = repo.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        for photo in project.photos.iter_mut() {
            if let Some(entry) = repo.get(Path::new(&photo.path)) {
                if !entry.faces.is_empty() {
                    photo.faces = entry
                        .faces
                        .iter()
                        .filter(|face| face.is_usable())
                        .map(|face| {
                            let mut record =
                                ProjectFace::from_frame(face, identity::MATCH_MODEL_VERSION);
                            record.ensure_uuid();
                            record
                        })
                        .collect();
                }
            }
            if photo.source_hash.is_empty() {
                photo.source_hash =
                    identity::source_hash(Path::new(&photo.path)).unwrap_or_default();
            }
        }
        let target = match destination {
            Some(value) if !value.trim().is_empty() => PathBuf::from(value),
            _ => {
                let suggested = format!(
                    "lumi-project-{}.{}",
                    project.active_index + 1,
                    PROJECT_EXTENSION
                );
                match rfd::FileDialog::new()
                    .set_title("保存 LUMI 项目")
                    .set_file_name(&suggested)
                    .add_filter("LUMI 项目", &[PROJECT_EXTENSION])
                    .add_filter("JSON", &["json"])
                    .save_file()
                {
                    Some(mut path) => {
                        if path.extension().is_none() {
                            path.set_extension(PROJECT_EXTENSION);
                        }
                        path
                    }
                    None => return Err(AppError::Cancelled),
                }
            }
        };
        project::save(&target, &project)?;
        let bytes = std::fs::metadata(&target).map(|meta| meta.len()).unwrap_or(0);
        Ok(ProjectSaved {
            path: target.to_string_lossy().into_owned(),
            bytes,
        })
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

/// 选择并读取项目文件。返回参数与路径，图片由前端调用 load_images 重新载入。
#[tauri::command]
pub async fn open_project(destination: Option<String>) -> AppResult<ProjectOpened> {
    tauri::async_runtime::spawn_blocking(move || {
        let target = match destination {
            Some(value) if !value.trim().is_empty() => PathBuf::from(value),
            _ => match rfd::FileDialog::new()
                .set_title("打开 LUMI 项目")
                .add_filter("LUMI 项目", &[PROJECT_EXTENSION, "json"])
                .pick_file()
            {
                Some(path) => path,
                None => return Err(AppError::Cancelled),
            },
        };
        let project = project::load(&target)?;
        Ok(ProjectOpened {
            path: target.to_string_lossy().into_owned(),
            project,
        })
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LutPicked {
    pub path: String,
    pub name: String,
}

/// 选择一个 .cube 3D LUT 文件；路径会写进项目参数。
#[tauri::command]
pub async fn pick_lut() -> AppResult<Option<LutPicked>> {
    tauri::async_runtime::spawn_blocking(move || {
        match rfd::FileDialog::new()
            .set_title("选择 3D LUT")
            .add_filter("3D LUT", &["cube"])
            .pick_file()
        {
            Some(path) => {
                let name = path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "LUT".into());
                Ok(Some(LutPicked {
                    path: path.to_string_lossy().into_owned(),
                    name,
                }))
            }
            None => Ok(None),
        }
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhiteBalanceEstimate {
    pub temperature: f32,
    pub tint: f32,
}

/// 灰世界自动白平衡：基于原图像素统计，不改变原图，只返回建议偏移。
#[tauri::command]
pub async fn auto_white_balance(
    state: State<'_, ImageCache>,
    path: String,
) -> AppResult<WhiteBalanceEstimate> {
    let cache = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let cached = cache.load(Path::new(&path))?;
        let (temperature, tint) = estimate_white_balance(&cached.preview);
        Ok(WhiteBalanceEstimate {
            temperature,
            tint,
        })
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}
