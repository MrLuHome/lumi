//! 从人物 Alpha 提取轮廓级身体几何。
//!
//! 这不是姿态识别，也不假装成骨架关键点：只扫描每一行 Alpha 的左右边界，
//! 得到肩、腰、胯、腿等高度上的近似宽度。UI 必须标注「轮廓级近似」。

use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SilhouetteRow {
    pub y: f32,
    pub left: f32,
    pub right: f32,
}

#[derive(Debug, Clone)]
pub struct BodyMetrics {
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
    /// 每一像素行的人物左右边界；`None` 表示该行没有人物。
    pub rows: Vec<Option<(f32, f32)>>,
    pub confidence: f32,
}

impl BodyMetrics {
    pub fn height(&self) -> f32 {
        (self.bottom - self.top).max(1.0)
    }

    pub fn row_at(&self, y: f32) -> Option<(f32, f32)> {
        let index = y.round() as isize;
        if index < 0 {
            return None;
        }
        self.rows.get(index as usize).copied().flatten()
    }

    pub fn width_at(&self, y: f32) -> Option<f32> {
        self.row_at(y).map(|(left, right)| (right - left).max(0.0))
    }

    pub fn sampled(&self, fraction: f32) -> Option<SilhouetteRow> {
        let y = self.top + self.height() * fraction.clamp(0.0, 1.0);
        self.row_at(y).map(|(left, right)| SilhouetteRow { y, left, right })
    }
}

/// Alpha 阈值低于此值视为背景；软边不参与轮廓宽度统计。
const ALPHA_THRESHOLD: f32 = 0.4;
/// 覆盖行数少于该比例时判定为不可用，避免把零碎笔触当成人。
const MIN_ROW_COVERAGE: f32 = 0.08;

pub fn analyze(alpha: &[f32], width: u32, height: u32) -> Option<BodyMetrics> {
    if width == 0 || height == 0 || alpha.len() != width as usize * height as usize {
        return None;
    }
    let mut rows = Vec::with_capacity(height as usize);
    let mut top = f32::MAX;
    let mut bottom = f32::MIN;
    let mut left = f32::MAX;
    let mut right = f32::MIN;
    let mut covered_rows = 0u32;

    for y in 0..height as usize {
        let row = &alpha[y * width as usize..(y + 1) * width as usize];
        let mut row_left = None;
        let mut row_right = None;
        for (x, value) in row.iter().enumerate() {
            if *value > ALPHA_THRESHOLD {
                if row_left.is_none() {
                    row_left = Some(x as f32);
                }
                row_right = Some(x as f32);
            }
        }
        match (row_left, row_right) {
            (Some(l), Some(r)) if r - l >= 2.0 => {
                rows.push(Some((l, r)));
                covered_rows += 1;
                top = top.min(y as f32);
                bottom = bottom.max(y as f32);
                left = left.min(l);
                right = right.max(r);
            }
            _ => rows.push(None),
        }
    }

    if covered_rows < 4 || top > bottom {
        return None;
    }
    let coverage = covered_rows as f32 / height as f32;
    if coverage < MIN_ROW_COVERAGE {
        return None;
    }
    Some(BodyMetrics {
        top,
        bottom,
        left,
        right,
        rows,
        confidence: coverage.clamp(0.0, 1.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha_with_rect(width: u32, height: u32, x0: u32, x1: u32, y0: u32, y1: u32) -> Vec<f32> {
        let mut alpha = vec![0.0f32; (width * height) as usize];
        for y in y0..y1 {
            for x in x0..x1 {
                alpha[y as usize * width as usize + x as usize] = 1.0;
            }
        }
        alpha
    }

    #[test]
    fn extracts_bounding_box_and_rows() {
        let alpha = alpha_with_rect(50, 100, 10, 40, 20, 80);
        let metrics = analyze(&alpha, 50, 100).expect("should analyze");
        assert_eq!(metrics.top, 20.0);
        assert_eq!(metrics.bottom, 79.0);
        assert_eq!(metrics.left, 10.0);
        assert_eq!(metrics.right, 39.0);
        assert_eq!(metrics.width_at(50.0), Some(29.0));
        assert!(metrics.confidence > 0.5);
    }

    #[test]
    fn rejects_empty_and_tiny_masks() {
        assert!(analyze(&vec![0.0; 40 * 40], 40, 40).is_none());
        let tiny = alpha_with_rect(40, 40, 10, 12, 10, 12);
        assert!(analyze(&tiny, 40, 40).is_none());
    }

    #[test]
    fn sampled_returns_row_near_fraction() {
        let alpha = alpha_with_rect(60, 120, 20, 40, 20, 100);
        let metrics = analyze(&alpha, 60, 120).unwrap();
        let waist = metrics.sampled(0.5).unwrap();
        assert!((waist.y - 60.0).abs() < 1.0);
        assert_eq!(waist.left, 20.0);
        assert_eq!(waist.right, 39.0);
    }
}
