use crate::ai::model::{NMS_THRESHOLD, SCORE_THRESHOLD};

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub score: f32,
    pub cx: f32,
    pub cy: f32,
    pub width: f32,
    pub height: f32,
    pub landmarks: [[f32; 2]; 5],
}

impl Candidate {
    pub fn left(&self) -> f32 {
        self.cx - self.width * 0.5
    }

    pub fn top(&self) -> f32 {
        self.cy - self.height * 0.5
    }

    fn iou(&self, other: &Candidate) -> f32 {
        let left = self.left().max(other.left());
        let top = self.top().max(other.top());
        let right = (self.left() + self.width).min(other.left() + other.width);
        let bottom = (self.top() + self.height).min(other.top() + other.height);
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

/// 解码单个 stride 的 5 个输出张量。模型输入坐标空间为 640×640。
pub fn decode_stride(
    cls: &[f32],
    obj: &[f32],
    bbox: &[f32],
    kps: &[f32],
    stride: u32,
    threshold: f32,
) -> Vec<Candidate> {
    let grid = 640 / stride;
    let count = (grid * grid) as usize;
    let mut candidates = Vec::new();
    for index in 0..count.min(cls.len()).min(obj.len()) {
        let score = (cls[index].clamp(0.0, 1.0) * obj[index].clamp(0.0, 1.0)).sqrt();
        if score < threshold {
            continue;
        }
        let cell_x = (index as u32 % grid) as f32;
        let cell_y = (index as u32 / grid) as f32;
        let b = index * 4;
        let k = index * 10;
        if b + 3 >= bbox.len() || k + 9 >= kps.len() {
            break;
        }
        let width = bbox[b + 2].exp().min(4096.0) * stride as f32;
        let height = bbox[b + 3].exp().min(4096.0) * stride as f32;
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            continue;
        }
        let mut landmarks = [[0f32; 2]; 5];
        for point in 0..5 {
            landmarks[point] = [
                (cell_x + kps[k + point * 2]) * stride as f32,
                (cell_y + kps[k + point * 2 + 1]) * stride as f32,
            ];
        }
        candidates.push(Candidate {
            score,
            cx: (cell_x + bbox[b]) * stride as f32,
            cy: (cell_y + bbox[b + 1]) * stride as f32,
            width,
            height,
            landmarks,
        });
    }
    candidates
}

/// 贪心 NMS，按分数从高到低保留互不重叠的人脸。
pub fn nms(mut candidates: Vec<Candidate>, threshold: f32) -> Vec<Candidate> {
    candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let mut kept: Vec<Candidate> = Vec::new();
    for candidate in candidates {
        if kept.iter().all(|other| candidate.iou(other) <= threshold) {
            kept.push(candidate);
        }
        if kept.len() >= 32 {
            break;
        }
    }
    kept
}

/// 把 640×640 模型坐标映射回原图坐标（模型输入是直接拉伸的）。
pub fn scale_to_original(candidate: &Candidate, scale_x: f32, scale_y: f32) -> Candidate {
    Candidate {
        score: candidate.score,
        cx: candidate.cx * scale_x,
        cy: candidate.cy * scale_y,
        width: candidate.width * scale_x,
        height: candidate.height * scale_y,
        landmarks: candidate
            .landmarks
            .map(|point| [point[0] * scale_x, point[1] * scale_y]),
    }
}

pub fn decode_all(
    outputs: &[Vec<f32>],
    threshold: f32,
    iou_threshold: f32,
) -> Vec<Candidate> {
    if outputs.len() < 12 {
        return Vec::new();
    }
    let mut candidates = Vec::new();
    // YuNet ONNX 输出顺序与 stride 分组固定：
    // cls_8/16/32, obj_8/16/32, bbox_8/16/32, kps_8/16/32。
    for (group, stride) in [8u32, 16, 32].into_iter().enumerate() {
        let cls = &outputs[group];
        let obj = &outputs[3 + group];
        let bbox = &outputs[6 + group];
        let kps = &outputs[9 + group];
        candidates.extend(decode_stride(cls, obj, bbox, kps, stride, threshold));
    }
    nms(candidates, iou_threshold)
}

pub fn default_thresholds() -> (f32, f32) {
    (SCORE_THRESHOLD, NMS_THRESHOLD)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(cx: f32, cy: f32, score: f32) -> Candidate {
        Candidate {
            score,
            cx,
            cy,
            width: 100.0,
            height: 100.0,
            landmarks: [[cx - 20.0, cy - 20.0]; 5],
        }
    }

    #[test]
    fn decodes_anchor_into_pixel_geometry() {
        let mut cls = vec![0f32; 6400];
        let mut obj = vec![0f32; 6400];
        let mut bbox = vec![0f32; 6400 * 4];
        let mut kps = vec![0f32; 6400 * 10];
        // stride 8 的第 0 个 anchor。
        cls[0] = 1.0;
        obj[0] = 1.0;
        bbox[0] = 0.5;
        bbox[1] = 0.5;
        bbox[2] = 0.0; // exp(0) * 8 = 8px
        bbox[3] = 0.0;
        for point in 0..5 {
            kps[point * 2] = 0.5;
            kps[point * 2 + 1] = 0.5;
        }
        let candidates = decode_stride(&cls, &obj, &bbox, &kps, 8, 0.6);
        assert_eq!(candidates.len(), 1);
        assert!((candidates[0].cx - 4.0).abs() < 1e-3);
        assert!((candidates[0].width - 8.0).abs() < 1e-3);
        assert!((candidates[0].landmarks[0][0] - 4.0).abs() < 1e-3);
    }

    #[test]
    fn threshold_filters_low_confidence() {
        let cls = vec![0.1f32; 6400];
        let obj = vec![0.1f32; 6400];
        let bbox = vec![0f32; 6400 * 4];
        let kps = vec![0f32; 6400 * 10];
        assert!(decode_stride(&cls, &obj, &bbox, &kps, 8, 0.6).is_empty());
    }

    #[test]
    fn nms_keeps_highest_scoring_overlap() {
        let kept = nms(
            vec![
                candidate(100.0, 100.0, 0.9),
                candidate(104.0, 102.0, 0.8),
                candidate(400.0, 400.0, 0.7),
            ],
            0.3,
        );
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].score, 0.9);
        assert_eq!(kept[1].score, 0.7);
    }

    #[test]
    fn scaling_maps_into_original_pixels() {
        let scaled = scale_to_original(&candidate(320.0, 320.0, 0.9), 4.0, 2.0);
        assert_eq!(scaled.cx, 1280.0);
        assert_eq!(scaled.cy, 640.0);
        assert_eq!(scaled.width, 400.0);
    }
}
