use serde::{Deserialize, Serialize};

use crate::ai::face::DetectedFace;
use crate::error::{AppError, AppResult};

/// 归一化图像坐标（0..1）。人脸结果按此格式缓存，
/// 预览、高清预览、全尺寸导出都能复用到同一组检测结果。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NormPoint {
    pub x: f32,
    pub y: f32,
}

/// 供渲染管线使用的人脸几何。`id` 由后端跟踪器分配，跨检测保持稳定。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceFrame {
    pub id: u32,
    /// 稳定人脸 UUID。跨检测、跨项目重开都使用它作为真正的持久身份，
    /// `id` 只是 UUID 的稳定哈希，方便旧逻辑与前端索引。
    #[serde(default)]
    pub uuid: String,
    pub score: f32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// YuNet 5 点：右眼、左眼、鼻尖、右嘴角、左嘴角（图像上的左右）。
    pub landmarks: Vec<NormPoint>,
}

impl FaceFrame {
    pub fn from_detected(face: &DetectedFace, width: u32, height: u32) -> Self {
        let w = width.max(1) as f32;
        let h = height.max(1) as f32;
        Self {
            id: face.id as u32,
            uuid: String::new(),
            score: face.score,
            x: face.x / w,
            y: face.y / h,
            width: face.width / w,
            height: face.height / h,
            landmarks: face
                .landmarks
                .iter()
                .map(|point| NormPoint {
                    x: clamp_unit(point.x / w),
                    y: clamp_unit(point.y / h),
                })
                .collect(),
        }
    }

    pub fn is_usable(&self) -> bool {
        self.landmarks.len() >= 5 && self.width > 0.01 && self.height > 0.01
    }

    /// 持久身份：UUID 优先，旧数据回退到稳定哈希。
    pub fn stable_key(&self) -> String {
        if self.uuid.is_empty() {
            format!("legacy-{}", self.id)
        } else {
            self.uuid.clone()
        }
    }

    /// 补齐 UUID 并从 UUID 重新推导整型 ID，保证重开项目后仍指向同一张脸。
    pub fn ensure_identity(&mut self) {
        if self.uuid.is_empty() {
            self.uuid = new_face_uuid();
        }
        self.id = id_from_uuid(&self.uuid);
    }

    pub fn iou(&self, other: &FaceFrame) -> f32 {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);
        let intersection = (right - left).max(0.0) * (bottom - top).max(0.0);
        if intersection <= 0.0 {
            return 0.0;
        }
        let union = self.width * self.height + other.width * other.height - intersection;
        if union <= 0.0 {
            0.0
        } else {
            intersection / union
        }
    }
}

fn clamp_unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// 生成 UUID v4 形式的人脸 ID。不依赖额外 crate，时间 + 进程内计数器足够
/// 避免同一项目内碰撞；它不是可跨项目识别身份的 embedding。
pub fn new_face_uuid() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0);
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    let a = (nanos >> 32) as u32;
    let b = (nanos >> 16) as u16;
    let c = 0x4000 | ((nanos as u16) & 0x0fff);
    let d = 0x8000 | ((count as u16) & 0x3fff);
    let e = (nanos.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ count) & 0xffff_ffff_ffff;
    format!("{a:08x}-{b:04x}-{c:04x}-{d:04x}-{e:012x}")
}

/// 把 UUID 映射为稳定的 32 位正整数 ID（FNV-1a），检测顺序变化无关。
pub fn id_from_uuid(uuid: &str) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in uuid.as_bytes() {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    let id = hash & 0x7fff_ffff;
    if id == 0 {
        1
    } else {
        id
    }
}

/// 人脸 ID 跟踪器：优先按 IoU 匹配上一轮结果，匹配不上才分配新 ID。
/// 检测顺序变化、阈值抖动都不会让已有参数错配到另一张脸。
#[derive(Debug, Default)]
pub struct FaceTracker;

impl FaceTracker {
    pub fn assign(&mut self, previous: &[FaceFrame], mut current: Vec<FaceFrame>) -> Vec<FaceFrame> {
        let mut used = vec![false; previous.len()];
        let mut assigned: Vec<Option<usize>> = vec![None; current.len()];

        // 贪心匹配：重叠度最高的先配对。
        let mut pairs: Vec<(usize, usize, f32)> = Vec::new();
        for (current_index, face) in current.iter().enumerate() {
            for (previous_index, old) in previous.iter().enumerate() {
                let iou = face.iou(old);
                if iou >= 0.25 {
                    pairs.push((current_index, previous_index, iou));
                }
            }
        }
        pairs.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
        for (current_index, previous_index, _) in pairs {
            if used[previous_index] || assigned[current_index].is_some() {
                continue;
            }
            used[previous_index] = true;
            assigned[current_index] = Some(previous_index);
        }

        for (index, face) in current.iter_mut().enumerate() {
            match assigned[index] {
                Some(previous_index) => {
                    // UUID 跟着同一张脸走，整型 ID 由 UUID 重新推导。
                    face.uuid = if previous[previous_index].uuid.is_empty() {
                        new_face_uuid()
                    } else {
                        previous[previous_index].uuid.clone()
                    };
                }
                None => {
                    if face.uuid.is_empty() {
                        face.uuid = new_face_uuid();
                    }
                }
            }
            face.id = id_from_uuid(&face.uuid);
        }
        current.sort_by_key(|face| face.id);
        current
    }

}

/// 单张人脸的像素几何，皮肤蒙版与局部操作都基于这个坐标系。
#[derive(Debug, Clone)]
pub struct FaceGeometry {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    /// 双眼中点。
    pub origin: (f32, f32),
    /// 沿双眼连线方向的单位向量（指向左眼）。
    pub axis_x: (f32, f32),
    /// 垂直向下方向的单位向量。
    pub axis_y: (f32, f32),
    /// 双眼间距（像素）。
    pub eye_distance: f32,
    pub eyes: [(f32, f32); 2],
    pub nose: (f32, f32),
    pub mouth: [(f32, f32); 2],
}

impl FaceGeometry {
    pub fn from_face(face: &FaceFrame, image_width: u32, image_height: u32) -> AppResult<Self> {
        if !face.is_usable() {
            return Err(AppError::Unsupported(
                "人脸结果缺少关键点，无法生成皮肤蒙版".into(),
            ));
        }
        let w = image_width.max(1) as f32;
        let h = image_height.max(1) as f32;
        let point = |index: usize| -> (f32, f32) {
            let value = face.landmarks[index];
            (value.x * w, value.y * h)
        };
        let eye_right = point(0);
        let eye_left = point(1);
        let dx = eye_left.0 - eye_right.0;
        let dy = eye_left.1 - eye_right.1;
        let distance = (dx * dx + dy * dy).sqrt();
        if !distance.is_finite() || distance < 4.0 {
            return Err(AppError::Unsupported("双眼关键点距离异常".into()));
        }
        let ux = dx / distance;
        let uy = dy / distance;
        // 图像坐标 y 向下，选择朝向下方的一侧作为面部 v 轴。
        let (vx, vy) = if ux >= 0.0 { (-uy, ux) } else { (uy, -ux) };
        Ok(Self {
            id: face.id,
            width: image_width.max(1),
            height: image_height.max(1),
            origin: ((eye_right.0 + eye_left.0) * 0.5, (eye_right.1 + eye_left.1) * 0.5),
            axis_x: (ux, uy),
            axis_y: (vx, vy),
            eye_distance: distance,
            eyes: [eye_right, eye_left],
            nose: point(2),
            mouth: [point(3), point(4)],
        })
    }

    /// 把像素坐标转换为人脸局部坐标：u 沿双眼方向，v 垂直向下，单位为眼距。
    pub fn to_local(&self, x: f32, y: f32) -> (f32, f32) {
        let dx = x - self.origin.0;
        let dy = y - self.origin.1;
        let u = (dx * self.axis_x.0 + dy * self.axis_x.1) / self.eye_distance;
        let v = (dx * self.axis_y.0 + dy * self.axis_y.1) / self.eye_distance;
        (u, v)
    }

    pub fn from_local(&self, u: f32, v: f32) -> (f32, f32) {
        (
            self.origin.0 + (self.axis_x.0 * u + self.axis_y.0 * v) * self.eye_distance,
            self.origin.1 + (self.axis_x.1 * u + self.axis_y.1 * v) * self.eye_distance,
        )
    }

    pub fn center(&self) -> (f32, f32) {
        self.from_local(0.0, 0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::face::FacePoint;

    fn face(id: usize, x: f32) -> DetectedFace {
        DetectedFace {
            id,
            uuid: String::new(),
            score: 0.9,
            x,
            y: 100.0,
            width: 200.0,
            height: 200.0,
            landmarks: (0..5)
                .map(|index| FacePoint {
                    x: x + 60.0 + index as f32 * 20.0,
                    y: 180.0,
                })
                .collect(),
        }
    }

    #[test]
    fn tracker_keeps_ids_when_order_changes() {
        let mut tracker = FaceTracker::default();
        let first = tracker.assign(&[], vec![
            FaceFrame::from_detected(&face(0, 400.0), 1000, 1000),
            FaceFrame::from_detected(&face(1, 100.0), 1000, 1000),
        ]);
        assert_eq!(first.len(), 2);
        let id_at = |faces: &[FaceFrame], x: f32| {
            faces
                .iter()
                .find(|face| (face.x * 1000.0 - x).abs() < 0.5)
                .map(|face| face.id)
                .expect("找不到指定位置的人脸")
        };
        let left_id = id_at(&first, 400.0);
        let right_id = id_at(&first, 100.0);
        assert_ne!(left_id, right_id);

        // 顺序反转后再跟踪，ID 必须跟着人脸走。
        let mut reversed = first.clone();
        reversed.reverse();
        let second = tracker.assign(&first, reversed);
        assert_eq!(id_at(&second, 400.0), left_id);
        assert_eq!(id_at(&second, 100.0), right_id);
    }

    #[test]
    fn geometry_uses_eye_distance_as_unit() {
        let detected = DetectedFace {
            id: 0,
            uuid: String::new(),
            score: 0.9,
            x: 100.0,
            y: 100.0,
            width: 300.0,
            height: 300.0,
            landmarks: vec![
                FacePoint { x: 200.0, y: 200.0 },
                FacePoint { x: 300.0, y: 200.0 },
                FacePoint { x: 250.0, y: 260.0 },
                FacePoint { x: 220.0, y: 300.0 },
                FacePoint { x: 280.0, y: 300.0 },
            ],
        };
        let frame = FaceFrame::from_detected(&detected, 1000, 1000);
        let geometry = FaceGeometry::from_face(&frame, 1000, 1000).unwrap();
        assert!((geometry.eye_distance - 100.0).abs() < 0.01);
        let (u, v) = geometry.to_local(250.0, 260.0);
        assert!(u.abs() < 1e-3);
        assert!((v - 0.6).abs() < 1e-3);
    }
}
