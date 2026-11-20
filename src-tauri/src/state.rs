use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use image::RgbaImage;

use crate::error::{AppError, AppResult};
use crate::imaging::{decode, encode};

/// 已经进入编辑会话的图片。`full` 保留原始像素供导出使用，
/// `preview` / `thumb` 是缩放后的派生图，所有滑杆交互都只处理 preview。
pub struct CachedImage {
    pub path: PathBuf,
    pub file_name: String,
    pub format: String,
    pub file_size: u64,
    pub width: u32,
    pub height: u32,
    pub has_alpha: bool,
    pub full: Arc<RgbaImage>,
    pub preview: Arc<RgbaImage>,
    pub thumb: Arc<RgbaImage>,
}

impl CachedImage {
    #[cfg(test)]
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

/// 进程内 LRU 图片缓存。滑杆每动一次都会请求渲染，
/// 如果每次都从磁盘解码，24MP 原图会把交互拖到不可用。
#[derive(Clone)]
pub struct ImageCache {
    inner: Arc<Mutex<CacheInner>>,
}

struct CacheInner {
    capacity: usize,
    entries: VecDeque<(PathBuf, Arc<CachedImage>)>,
}

impl ImageCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(CacheInner {
                capacity: capacity.max(1),
                entries: VecDeque::new(),
            })),
        }
    }

    pub fn load(&self, path: &Path) -> AppResult<Arc<CachedImage>> {
        let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());

        {
            let guard = self.lock()?;
            if let Some((_, cached)) = guard.entries.iter().find(|(key, _)| key == &canonical) {
                return Ok(cached.clone());
            }
        }

        let decoded = decode::decode(&canonical)?;
        let file_size = std::fs::metadata(&canonical).map(|meta| meta.len()).unwrap_or(0);
        let full = decoded.to_rgba8();
        let (width, height) = full.dimensions();
        let has_alpha = decode::has_alpha(&full);

        let cached = Arc::new(CachedImage {
            file_name: canonical
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "未命名".to_string()),
            format: decode::format_label(&canonical),
            file_size,
            width,
            height,
            has_alpha,
            preview: Arc::new(decode::fit_within(&full, decode::PREVIEW_MAX_EDGE)),
            thumb: Arc::new(decode::fit_within(&full, decode::THUMB_MAX_EDGE)),
            full: Arc::new(full),
            path: canonical.clone(),
        });

        let mut guard = self.lock()?;
        guard.entries.push_back((canonical, cached.clone()));
        while guard.entries.len() > guard.capacity {
            guard.entries.pop_front();
        }
        Ok(cached)
    }

    fn lock(&self) -> AppResult<std::sync::MutexGuard<'_, CacheInner>> {
        self.inner
            .lock()
            .map_err(|_| AppError::Task("图片缓存状态异常，请重启应用".to_string()))
    }
}

/// 缓存条目 -> 前端需要的缩略图 / 预览图 data URL。
pub fn encode_urls(cached: &CachedImage) -> AppResult<(String, String)> {
    let thumb = encode::thumb_data_url(&cached.thumb, cached.has_alpha)?;
    let preview = encode::preview_data_url(&cached.preview, cached.has_alpha)?;
    Ok((thumb, preview))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn write_png(path: &Path, size: u32) {
        let image = RgbaImage::from_pixel(size, size, Rgba([30, 60, 90, 255]));
        image.save(path).unwrap();
    }

    #[test]
    fn caches_decoded_image_and_reuses_it() {
        let dir = std::env::temp_dir().join("lumi-cache-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("portrait.png");
        write_png(&path, 64);

        let cache = ImageCache::new(2);
        let first = cache.load(&path).unwrap();
        let second = cache.load(&path).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.width, 64);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn evicts_when_capacity_exceeded() {
        let dir = std::env::temp_dir().join("lumi-cache-evict");
        std::fs::create_dir_all(&dir).unwrap();
        let cache = ImageCache::new(1);
        let mut paths = Vec::new();
        for index in 0..2 {
            let path = dir.join(format!("shot-{index}.png"));
            write_png(&path, 16);
            paths.push(path);
        }
        cache.load(&paths[0]).unwrap();
        cache.load(&paths[1]).unwrap();
        // 容量为 1，第一张被淘汰，重新加载应得到新的 Arc。
        let reloaded = cache.load(&paths[0]).unwrap();
        assert_eq!(reloaded.dimensions(), (16, 16));
        for path in paths {
            std::fs::remove_file(path).ok();
        }
    }
}
