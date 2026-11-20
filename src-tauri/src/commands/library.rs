use std::path::PathBuf;

use serde::Serialize;
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::imaging::decode;
use crate::state::{encode_urls, CachedImage, ImageCache};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageItem {
    /// 以绝对路径作为图片 ID：同一张图被重复导入时天然去重。
    pub id: String,
    pub path: String,
    pub file_name: String,
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub file_size: u64,
    pub has_alpha: bool,
    pub thumb_url: String,
    pub preview_url: String,
    pub preview_width: u32,
    pub preview_height: u32,
}

fn to_item(cached: &CachedImage) -> AppResult<ImageItem> {
    let (thumb_url, preview_url) = encode_urls(cached)?;
    Ok(ImageItem {
        id: cached.path.to_string_lossy().into_owned(),
        path: cached.path.to_string_lossy().into_owned(),
        file_name: cached.file_name.clone(),
        format: cached.format.clone(),
        width: cached.width,
        height: cached.height,
        file_size: cached.file_size,
        has_alpha: cached.has_alpha,
        thumb_url,
        preview_url,
        preview_width: cached.preview.width(),
        preview_height: cached.preview.height(),
    })
}

fn load_paths(cache: &ImageCache, paths: Vec<PathBuf>) -> AppResult<Vec<ImageItem>> {
    let mut items = Vec::new();
    let mut last_error = None;

    for path in paths {
        if !decode::is_supported_extension(&path) {
            continue;
        }
        match cache.load(&path).and_then(|cached| to_item(&cached)) {
            Ok(item) => items.push(item),
            Err(error) => last_error = Some(error),
        }
    }

    if items.is_empty() {
        return Err(last_error.unwrap_or(AppError::EmptySelection));
    }
    Ok(items)
}

/// 打开系统文件选择器并导入照片。
#[tauri::command]
pub async fn pick_images(state: State<'_, ImageCache>) -> AppResult<Vec<ImageItem>> {
    let cache = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let selection = rfd::FileDialog::new()
            .set_title("导入照片")
            .add_filter("照片", &["jpg", "jpeg", "png", "webp"])
            .pick_files();
        match selection {
            // 用户取消不应弹错误提示。
            None => Ok(Vec::new()),
            Some(paths) => load_paths(&cache, paths),
        }
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

/// 通过拖拽或外部调用按路径导入照片。
#[tauri::command]
pub async fn load_images(state: State<'_, ImageCache>, paths: Vec<String>) -> AppResult<Vec<ImageItem>> {
    let cache = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        load_paths(&cache, paths)
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    #[test]
    fn import_reads_supported_files_and_skips_unsupported() {
        let dir = std::env::temp_dir().join(format!("lumi-import-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let photo = dir.join("人像样张.png");
        let image = RgbaImage::from_pixel(64, 40, Rgba([120, 90, 70, 255]));
        image.save(&photo).unwrap();
        let note = dir.join("备注.txt");
        std::fs::write(&note, b"not an image").unwrap();

        let cache = ImageCache::new(2);
        let items = load_paths(&cache, vec![note, photo.clone()]).unwrap();
        assert_eq!(items.len(), 1);
        let item = &items[0];
        assert_eq!((item.width, item.height), (64, 40));
        assert_eq!(item.format, "PNG");
        assert_eq!(item.file_name, "人像样张.png");
        assert!(item.preview_width <= 64 && item.preview_height <= 40);
        assert!(item.thumb_url.starts_with("data:image/"));
        assert!(item.preview_url.starts_with("data:image/"));

        // 同一路径重复导入命中缓存，仍返回完整数据。
        let again = load_paths(&cache, vec![photo]).unwrap();
        assert_eq!(again.len(), 1);

        let empty = load_paths(&cache, Vec::new());
        assert!(empty.is_err(), "空选择应返回错误而不是空列表");

        std::fs::remove_dir_all(&dir).ok();
    }
}
