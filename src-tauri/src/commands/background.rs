//! 背景图片选择与校验。图片只在本机读取，不复制、不上传。

use std::path::Path;

use serde::Serialize;

use crate::error::{AppError, AppResult};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundImageInfo {
    pub path: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
}

#[tauri::command]
pub fn pick_background_image() -> AppResult<Option<BackgroundImageInfo>> {
    let Some(path) = rfd::FileDialog::new()
        .set_title("选择背景图片")
        .add_filter("图片", &["jpg", "jpeg", "png", "webp", "tif", "tiff", "bmp"])
        .pick_file()
    else {
        return Ok(None);
    };
    Ok(Some(background_info(&path)?))
}

/// 重开项目时校验背景图片是否仍然可用，不做任何复制。
#[tauri::command]
pub fn background_image_info(path: String) -> AppResult<BackgroundImageInfo> {
    background_info(Path::new(&path))
}

fn background_info(path: &Path) -> AppResult<BackgroundImageInfo> {
    let (width, height) = image::image_dimensions(path)
        .map_err(|error| AppError::Decode(format!("无法读取背景图片：{error}")))?;
    Ok(BackgroundImageInfo {
        path: path.to_string_lossy().into_owned(),
        name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned()),
        width,
        height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_background_reports_clear_error() {
        let error = background_info(Path::new("/nonexistent/lumi-bg.png")).unwrap_err();
        assert!(error.to_string().contains("背景图片"));
    }
}
