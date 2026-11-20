use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::ai_engine::face_frame::{FaceFrame, FaceTracker};
use crate::error::{AppError, AppResult};
use crate::image_engine::buffer::{estimated_bytes, has_alpha, Image16};
use crate::image_engine::decoder::{self, BitDepth};
use crate::image_engine::geometry::fit_within;
use crate::image_engine::preview::{encode_thumb, THUMB_MAX_EDGE};

/// 已经进入编辑会话的图片。`full` 保留原始像素供导出使用，
/// `preview` / `thumb` 是缩放后的派生图，滑杆交互只处理 preview。
pub struct CachedImage {
    pub path: PathBuf,
    pub file_name: String,
    pub format: String,
    pub depth: BitDepth,
    pub file_size: u64,
    pub width: u32,
    pub height: u32,
    pub has_alpha: bool,
    pub full: Arc<Image16>,
    pub preview: Arc<Image16>,
    pub thumb: Arc<Image16>,
    pub estimated_bytes: usize,
}

impl CachedImage {
    #[cfg(test)]
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

/// 进程内 LRU 图片缓存，同时受条目数与内存预算约束。
/// 24MP 图片的 16-bit RGBA 全图约 192MB，只看条目数会让内存失控。
#[derive(Clone)]
pub struct ImageCache {
    inner: Arc<Mutex<CacheInner>>,
}

struct CacheInner {
    capacity: usize,
    budget_bytes: usize,
    entries: VecDeque<(PathBuf, Arc<CachedImage>)>,
    total_bytes: usize,
}

pub const DEFAULT_MEMORY_BUDGET: usize = 1024 * 1024 * 1024;

impl ImageCache {
    pub fn new(capacity: usize) -> Self {
        Self::with_budget(capacity, DEFAULT_MEMORY_BUDGET)
    }

    pub fn with_budget(capacity: usize, budget_bytes: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(CacheInner {
                capacity: capacity.max(1),
                budget_bytes: budget_bytes.max(64 * 1024 * 1024),
                entries: VecDeque::new(),
                total_bytes: 0,
            })),
        }
    }

    pub fn load(&self, path: &Path) -> AppResult<Arc<CachedImage>> {
        let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());

        {
            let mut guard = self.lock()?;
            if let Some(index) = guard.entries.iter().position(|(key, _)| key == &canonical) {
                // 命中后移到队尾，保证 LRU 顺序。
                let entry = guard.entries.remove(index).expect("索引来自当前容器");
                let cached = entry.1.clone();
                guard.entries.push_back(entry);
                return Ok(cached);
            }
        }

        let decoded = decoder::decode(&canonical)?;
        let file_size = std::fs::metadata(&canonical).map(|meta| meta.len()).unwrap_or(0);
        let (width, height) = decoded.image.dimensions();
        let alpha = has_alpha(&decoded.image);
        let preview = Arc::new(crate::image_engine::preview::build_preview(
            &decoded.image,
            crate::image_engine::PREVIEW_MAX_EDGE,
        ));
        let thumb = Arc::new(fit_within(&decoded.image, THUMB_MAX_EDGE));
        let estimated = estimated_bytes(width, height)
            + estimated_bytes(preview.width(), preview.height())
            + estimated_bytes(thumb.width(), thumb.height());

        let cached = Arc::new(CachedImage {
            file_name: canonical
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "未命名".to_string()),
            format: decoded.format,
            depth: decoded.bit_depth,
            file_size,
            width,
            height,
            has_alpha: alpha,
            preview,
            thumb,
            full: Arc::new(decoded.image),
            estimated_bytes: estimated,
            path: canonical.clone(),
        });

        let mut guard = self.lock()?;
        guard.total_bytes += estimated;
        guard.entries.push_back((canonical, cached.clone()));
        while guard.entries.len() > guard.capacity
            || (guard.total_bytes > guard.budget_bytes && guard.entries.len() > 1)
        {
            if let Some((_, evicted)) = guard.entries.pop_front() {
                guard.total_bytes = guard.total_bytes.saturating_sub(evicted.estimated_bytes);
            }
        }
        Ok(cached)
    }

    pub fn memory_usage(&self) -> usize {
        self.inner
            .lock()
            .map(|guard| guard.total_bytes)
            .unwrap_or_default()
    }

    fn lock(&self) -> AppResult<std::sync::MutexGuard<'_, CacheInner>> {
        self.inner
            .lock()
            .map_err(|_| AppError::Task("图片缓存状态异常，请重启应用".to_string()))
    }
}

/// 单张照片的人脸检测结果缓存。归一化坐标可被预览、高清与导出复用；
/// `version` 用于渲染缓存键，检测结果变化时不会命中旧缓存。
#[derive(Debug, Clone)]
pub struct CachedFaces {
    pub faces: Vec<FaceFrame>,
    pub version: u64,
}

/// 每张照片的人脸结果仓库 + 稳定 ID 跟踪器。
#[derive(Clone, Default)]
pub struct FaceRepository {
    inner: Arc<Mutex<FaceRepositoryInner>>,
}

#[derive(Default)]
struct FaceRepositoryInner {
    entries: HashMap<PathBuf, CachedFaces>,
    trackers: HashMap<PathBuf, FaceTracker>,
    version: u64,
}

impl FaceRepository {
    pub fn get(&self, path: &Path) -> Option<CachedFaces> {
        let canonical = canonical_key(path);
        self.inner
            .lock()
            .ok()
            .and_then(|guard| guard.entries.get(&canonical).cloned())
    }

    /// 保存检测结果并用跟踪器维持 ID 稳定，返回分配好 ID 的人脸。
    pub fn store(&self, path: &Path, faces: Vec<FaceFrame>) -> CachedFaces {
        let canonical = canonical_key(path);
        let mut guard = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        let previous = guard
            .entries
            .get(&canonical)
            .map(|entry| entry.faces.clone())
            .unwrap_or_default();
        let tracker = guard.trackers.entry(canonical.clone()).or_default();
        let faces = tracker.assign(&previous, faces);
        guard.version = guard.version.saturating_add(1);
        let cached = CachedFaces {
            faces,
            version: guard.version,
        };
        guard.entries.insert(canonical, cached.clone());
        cached
    }

    pub fn signature(&self, path: &Path) -> String {
        self.get(path)
            .map(|entry| format!("{}:{}", entry.version, entry.faces.len()))
            .unwrap_or_else(|| "none".into())
    }
}

fn canonical_key(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// 缓存条目 -> 前端需要的缩略图 / 预览图 data URL。
pub fn encode_urls(cached: &CachedImage) -> AppResult<(String, String)> {
    let thumb = encode_thumb(&cached.thumb, cached.has_alpha)?;
    let preview = crate::image_engine::encoder::preview_data_url(&cached.preview, cached.has_alpha)?;
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
        let dir = std::env::temp_dir().join("lumi-cache-test-r2");
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
        let dir = std::env::temp_dir().join("lumi-cache-evict-r2");
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
        let reloaded = cache.load(&paths[0]).unwrap();
        assert_eq!(reloaded.dimensions(), (16, 16));
        for path in paths {
            std::fs::remove_file(path).ok();
        }
    }

    #[test]
    fn memory_budget_is_tracked() {
        let dir = std::env::temp_dir().join("lumi-cache-budget");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.png");
        write_png(&path, 32);
        let cache = ImageCache::with_budget(4, 64 * 1024 * 1024);
        cache.load(&path).unwrap();
        assert!(cache.memory_usage() > 0);
        std::fs::remove_file(&path).ok();
    }
}
