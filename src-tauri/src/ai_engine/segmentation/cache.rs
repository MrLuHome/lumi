//! 进程内蒙版缓存。
//!
//! 键包含源图哈希、笔触哈希、蒙版版本与输出尺寸，任何一项变化都会重建。
//! 缓存只保留少量条目，避免高分辨率 Alpha 常驻导致内存膨胀。

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};

use super::SubjectMask;

const DEFAULT_CAPACITY: usize = 4;

pub struct MaskCache {
    entries: Mutex<VecDeque<(String, Arc<SubjectMask>)>>,
    capacity: usize,
}

impl MaskCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Mutex::new(VecDeque::new()),
            capacity: capacity.max(1),
        }
    }

    pub fn get(&self, key: &str) -> Option<Arc<SubjectMask>> {
        let mut entries = self.entries.lock().ok()?;
        if let Some(index) = entries.iter().position(|(entry_key, _)| entry_key == key) {
            let entry = entries.remove(index)?;
            let mask = Arc::clone(&entry.1);
            entries.push_front(entry);
            return Some(mask);
        }
        None
    }

    pub fn insert(&self, key: String, mask: Arc<SubjectMask>) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.retain(|(entry_key, _)| entry_key != &key);
            entries.push_front((key, mask));
            while entries.len() > self.capacity {
                entries.pop_back();
            }
        }
    }

    pub fn len(&self) -> usize {
        self.entries
            .lock()
            .map(|entries| entries.len())
            .unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
    }
}

pub fn global() -> &'static MaskCache {
    static CACHE: OnceLock<MaskCache> = OnceLock::new();
    CACHE.get_or_init(|| MaskCache::new(DEFAULT_CAPACITY))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mask(value: f32) -> Arc<SubjectMask> {
        let mut mask = SubjectMask::empty(2, 2);
        mask.alpha = vec![value; 4];
        Arc::new(mask)
    }

    #[test]
    fn cache_returns_inserted_mask_and_evicts_oldest() {
        let cache = MaskCache::new(2);
        cache.insert("a".into(), mask(0.1));
        cache.insert("b".into(), mask(0.2));
        assert!(cache.get("a").is_some());
        cache.insert("c".into(), mask(0.3));
        assert_eq!(cache.len(), 2);
        assert!(cache.get("b").is_none());
        assert!(cache.get("a").is_some());
        cache.clear();
        assert!(cache.is_empty());
    }
}
