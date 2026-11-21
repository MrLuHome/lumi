//! 人脸身份持久化与项目重开匹配。
//!
//! 检测数组下标不是身份：每次重新检测顺序都可能变化。项目保存的是
//! 稳定 UUID + 原始检测框 + 初始关键点 + 置信度 + 模型版本。
//! 重开时重新检测，再用 IoU、关键点距离和尺寸相似度做一一匹配：
//! 高置信恢复原 UUID，低置信交给用户人工确认，匹配不到的人脸记录保留，
//! 绝不把 A 的参数套到 B 上。
//!
//! 这里只保存几何信息，不保存任何可用于跨项目识别身份的 embedding。

use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ai_engine::face_frame::{new_face_uuid, FaceFrame, NormPoint};
use crate::error::AppResult;

/// 保存记录时使用的关键点能力版本，便于将来升级后判断兼容性。
pub const MATCH_MODEL_VERSION: &str = "yunet-5";
/// 高置信阈值：直接恢复原 UUID。
pub const HIGH_CONFIDENCE: f32 = 0.62;
/// 低置信阈值：进入人工确认列表；低于此值视为“未出现”。
pub const LOW_CONFIDENCE: f32 = 0.30;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FaceBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// 项目中持久化的一张人脸。`parameters` 不在这里重复保存：
/// 美肤与塑形参数已经在 `EditParams` 中按 `faceUuid` 归属。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFace {
    pub uuid: String,
    pub bbox: FaceBox,
    /// 初始关键点（归一化）。没有可靠关键点时为空，不伪造。
    #[serde(default)]
    pub landmarks: Vec<NormPoint>,
    #[serde(default)]
    pub score: f32,
    /// 检测模型版本，例如 `yunet-5`。
    #[serde(default)]
    pub model: String,
    /// 检测置信度（0..1）。
    #[serde(default)]
    pub confidence: f32,
    /// 人脸蒙版引用；当前按几何实时重建，保留字段以便后续蒙版缓存。
    #[serde(default)]
    pub mask_ref: Option<String>,
}

impl ProjectFace {
    pub fn from_frame(frame: &FaceFrame, model: &str) -> Self {
        Self {
            uuid: frame.stable_key(),
            bbox: FaceBox {
                x: frame.x,
                y: frame.y,
                width: frame.width,
                height: frame.height,
            },
            landmarks: frame.landmarks.clone(),
            score: frame.score,
            model: if model.is_empty() {
                MATCH_MODEL_VERSION.into()
            } else {
                model.into()
            },
            confidence: frame.score,
            mask_ref: None,
        }
    }

    /// 旧项目缺少 UUID 时补一个，保证后续保存稳定。
    pub fn ensure_uuid(&mut self) -> &str {
        if self.uuid.is_empty() {
            self.uuid = new_face_uuid();
        }
        &self.uuid
    }
}

/// 读取源文件 SHA-256，用来判断项目重开时原图是否被替换。
pub fn source_hash(path: &Path) -> AppResult<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MatchConfidence {
    High,
    Low,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceMatch {
    pub record_index: usize,
    pub face_id: u32,
    pub face_uuid: String,
    pub score: f32,
    pub confidence: MatchConfidence,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FaceMatchOutcome {
    /// 高置信，自动恢复 UUID。
    pub restored: Vec<FaceMatch>,
    /// 中置信，需要用户确认；确认前不套用参数。
    pub pending: Vec<FaceMatch>,
    /// 本次检测中不存在，但项目里的编辑记录必须保留。
    pub missing: Vec<ProjectFace>,
    /// 源图哈希变化：原图可能被替换，任何匹配都不自动套用。
    pub source_changed: bool,
}

fn iou(a: &FaceBox, b: &FaceBox) -> f32 {
    let ax1 = a.x + a.width;
    let ay1 = a.y + a.height;
    let bx1 = b.x + b.width;
    let by1 = b.y + b.height;
    let ix0 = a.x.max(b.x);
    let iy0 = a.y.max(b.y);
    let ix1 = ax1.min(bx1);
    let iy1 = ay1.min(by1);
    let inter = (ix1 - ix0).max(0.0) * (iy1 - iy0).max(0.0);
    let union = a.width * a.height + b.width * b.height - inter;
    if union <= 1e-6 {
        0.0
    } else {
        (inter / union).clamp(0.0, 1.0)
    }
}

fn landmark_similarity(record: &ProjectFace, frame: &FaceFrame) -> Option<f32> {
    if record.landmarks.len() < 5 || frame.landmarks.len() < 5 {
        return None;
    }
    let count = record.landmarks.len().min(frame.landmarks.len());
    let scale = record
        .bbox
        .width
        .max(record.bbox.height)
        .max(frame.width)
        .max(frame.height)
        .max(1e-4);
    let mut total = 0.0;
    for index in 0..count {
        let a = record.landmarks[index];
        let b = frame.landmarks[index];
        total += ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt();
    }
    let mean = total / count as f32;
    // 关键点平均偏移超过脸部尺寸的 22% 视为完全不同的人脸。
    let tolerance = scale * 0.22;
    Some((1.0 - (mean / tolerance)).clamp(0.0, 1.0))
}

fn size_similarity(a: &FaceBox, b: &FaceBox) -> f32 {
    let width = 1.0 - (a.width - b.width).abs() / a.width.max(b.width).max(1e-4);
    let height = 1.0 - (a.height - b.height).abs() / a.height.max(b.height).max(1e-4);
    ((width + height) * 0.5).clamp(0.0, 1.0)
}

fn pair_score(record: &ProjectFace, frame: &FaceFrame) -> f32 {
    let bbox = FaceBox {
        x: frame.x,
        y: frame.y,
        width: frame.width,
        height: frame.height,
    };
    let overlap = iou(&record.bbox, &bbox);
    let size = size_similarity(&record.bbox, &bbox);
    match landmark_similarity(record, frame) {
        Some(landmarks) => overlap * 0.55 + landmarks * 0.30 + size * 0.15,
        // 没有可靠关键点时只用框几何，并降低整体上限，避免误配。
        None => overlap * 0.70 + size * 0.30,
    }
    .clamp(0.0, 1.0)
}

/// 一一匹配持久化记录与本次检测结果。
///
/// 贪心从最高分开始配对，每个记录和每张检测脸最多使用一次，
/// 因此两张脸靠近或重叠时也不会把同一张脸匹配给两个人。
pub fn match_faces(
    records: &[ProjectFace],
    frames: &[FaceFrame],
    source_changed: bool,
) -> FaceMatchOutcome {
    let mut pairs: Vec<(usize, usize, f32)> = Vec::new();
    for (record_index, record) in records.iter().enumerate() {
        if record.uuid.is_empty() {
            continue;
        }
        for (frame_index, frame) in frames.iter().enumerate() {
            let score = pair_score(record, frame);
            if score >= LOW_CONFIDENCE {
                pairs.push((record_index, frame_index, score));
            }
        }
    }
    pairs.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    let mut record_used = vec![false; records.len()];
    let mut frame_used = vec![false; frames.len()];
    let mut outcome = FaceMatchOutcome {
        source_changed,
        ..Default::default()
    };
    for (record_index, frame_index, score) in pairs {
        if record_used[record_index] || frame_used[frame_index] {
            continue;
        }
        record_used[record_index] = true;
        frame_used[frame_index] = true;
        let record = &records[record_index];
        let frame = &frames[frame_index];
        // 源图被替换时一律不自动套用，交人工确认。
        let confidence = if !source_changed && score >= HIGH_CONFIDENCE {
            MatchConfidence::High
        } else {
            MatchConfidence::Low
        };
        let entry = FaceMatch {
            record_index,
            face_id: frame.id,
            face_uuid: record.uuid.clone(),
            score,
            confidence,
        };
        if confidence == MatchConfidence::High {
            outcome.restored.push(entry);
        } else {
            outcome.pending.push(entry);
        }
    }

    for (index, record) in records.iter().enumerate() {
        if !record_used[index] {
            outcome.missing.push(record.clone());
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(id: u32, x: f32, y: f32, landmarks: [[f32; 2]; 5]) -> FaceFrame {
        FaceFrame {
            id,
            uuid: String::new(),
            score: 0.95,
            x,
            y,
            width: 0.2,
            height: 0.24,
            landmarks: landmarks
                .iter()
                .map(|point| NormPoint {
                    x: point[0],
                    y: point[1],
                })
                .collect(),
        }
    }

    fn marks(x: f32, y: f32) -> [[f32; 2]; 5] {
        [
            [x - 0.04, y],
            [x + 0.04, y],
            [x, y + 0.05],
            [x - 0.03, y + 0.08],
            [x + 0.03, y + 0.08],
        ]
    }

    fn record(frame: &FaceFrame) -> ProjectFace {
        let mut record = ProjectFace::from_frame(frame, MATCH_MODEL_VERSION);
        record.ensure_uuid();
        record
    }

    #[test]
    fn three_faces_survive_detection_reorder() {
        let a = frame(1, 0.05, 0.2, marks(0.15, 0.3));
        let b = frame(2, 0.40, 0.2, marks(0.50, 0.3));
        let c = frame(3, 0.75, 0.2, marks(0.85, 0.3));
        let records = vec![record(&a), record(&b), record(&c)];
        // 检测顺序反转，且 id 被重新分配。
        let mut a2 = a.clone();
        a2.id = 11;
        let mut b2 = b.clone();
        b2.id = 12;
        let mut c2 = c.clone();
        c2.id = 13;
        let outcome = match_faces(&records, &[c2, a2, b2], false);
        assert_eq!(outcome.restored.len(), 3);
        assert!(outcome.pending.is_empty());
        assert!(outcome.missing.is_empty());
        let by_uuid: Vec<(String, u32)> = outcome
            .restored
            .iter()
            .map(|entry| (entry.face_uuid.clone(), entry.face_id))
            .collect();
        assert!(by_uuid.contains(&(records[0].uuid.clone(), 11)));
        assert!(by_uuid.contains(&(records[1].uuid.clone(), 12)));
        assert!(by_uuid.contains(&(records[2].uuid.clone(), 13)));
    }

    #[test]
    fn missing_face_keeps_its_record() {
        let a = frame(1, 0.05, 0.2, marks(0.15, 0.3));
        let b = frame(2, 0.40, 0.2, marks(0.50, 0.3));
        let records = vec![record(&a), record(&b)];
        let outcome = match_faces(&records, std::slice::from_ref(&a), false);
        assert_eq!(outcome.restored.len(), 1);
        assert_eq!(outcome.missing.len(), 1);
        assert_eq!(outcome.missing[0].uuid, records[1].uuid);
    }

    #[test]
    fn new_person_does_not_steal_existing_parameters() {
        let a = frame(1, 0.05, 0.2, marks(0.15, 0.3));
        let records = vec![record(&a)];
        let stranger = frame(9, 0.70, 0.2, marks(0.80, 0.3));
        let outcome = match_faces(&records, &[stranger], false);
        assert!(outcome.restored.is_empty());
        assert!(outcome.pending.is_empty());
        assert_eq!(outcome.missing.len(), 1);
    }

    #[test]
    fn changed_source_requires_manual_confirmation() {
        let a = frame(1, 0.05, 0.2, marks(0.15, 0.3));
        let records = vec![record(&a)];
        let outcome = match_faces(&records, &[a], true);
        assert!(outcome.source_changed);
        assert!(outcome.restored.is_empty(), "源图变化不得自动套用参数");
        assert_eq!(outcome.pending.len(), 1);
    }

    #[test]
    fn overlapping_faces_are_matched_one_to_one() {
        let a = frame(1, 0.10, 0.2, marks(0.20, 0.3));
        let b = frame(2, 0.13, 0.2, marks(0.23, 0.3));
        let records = vec![record(&a), record(&b)];
        // 两个检测框几乎重叠。
        let mut a2 = a.clone();
        a2.id = 21;
        let mut b2 = b.clone();
        b2.id = 22;
        let outcome = match_faces(&records, &[a2, b2], false);
        assert_eq!(outcome.restored.len() + outcome.pending.len(), 2);
        let ids: Vec<u32> = outcome
            .restored
            .iter()
            .chain(outcome.pending.iter())
            .map(|entry| entry.face_id)
            .collect();
        assert_eq!(ids.len(), ids.iter().collect::<std::collections::HashSet<_>>().len());
    }

    #[test]
    fn source_hash_tracks_file_replacement() {
        let dir = std::env::temp_dir().join("lumi-face-hash");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("portrait.bin");
        std::fs::write(&path, b"first-version").unwrap();
        let first = source_hash(&path).unwrap();
        let again = source_hash(&path).unwrap();
        assert_eq!(first, again, "同一文件重复读取必须得到同一哈希");
        std::fs::write(&path, b"second-version").unwrap();
        let second = source_hash(&path).unwrap();
        assert_ne!(first, second, "原图替换后哈希必须变化");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn project_face_roundtrip_keeps_uuid_and_geometry() {
        let frame = frame(1, 0.05, 0.2, marks(0.15, 0.3));
        let mut saved = ProjectFace::from_frame(&frame, MATCH_MODEL_VERSION);
        let uuid = saved.ensure_uuid().to_string();
        let json = serde_json::to_string(&saved).unwrap();
        let loaded: ProjectFace = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.uuid, uuid);
        assert_eq!(loaded.bbox, saved.bbox);
        assert_eq!(loaded.landmarks.len(), 5);
        assert_eq!(loaded.model, MATCH_MODEL_VERSION);
    }
}
