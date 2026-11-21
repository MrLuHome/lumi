//! 面部作用区域与多人脸归属。
//!
//! 变形只允许发生在脸部椭圆内，边缘按平滑曲线羽化到 0。
//! 椭圆之外（背景、耳朵、头发、脖子）alpha 恒为 0，即使网格有数值误差
//! 也不会改到背景像素。

use crate::ai_engine::face_frame::FaceGeometry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x0: i32,
    pub y0: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn from_bounds(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Self {
        Self {
            x0: min_x.floor() as i32,
            y0: min_y.floor() as i32,
            width: (max_x - min_x).ceil().max(1.0) as u32,
            height: (max_y - min_y).ceil().max(1.0) as u32,
        }
    }

    pub fn clamp_to(self, image_width: u32, image_height: u32) -> Option<Self> {
        let x0 = self.x0.max(0);
        let y0 = self.y0.max(0);
        let x1 = (self.x0 + self.width as i32).min(image_width as i32);
        let y1 = (self.y0 + self.height as i32).min(image_height as i32);
        if x1 <= x0 || y1 <= y0 {
            None
        } else {
            Some(Self {
                x0,
                y0,
                width: (x1 - x0) as u32,
                height: (y1 - y0) as u32,
            })
        }
    }

    pub fn union(self, other: Rect) -> Rect {
        let x0 = self.x0.min(other.x0);
        let y0 = self.y0.min(other.y0);
        let x1 = (self.x0 + self.width as i32).max(other.x0 + other.width as i32);
        let y1 = (self.y0 + self.height as i32).max(other.y0 + other.height as i32);
        Rect {
            x0,
            y0,
            width: (x1 - x0).max(1) as u32,
            height: (y1 - y0).max(1) as u32,
        }
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x0
            && y >= self.y0
            && x < self.x0 + self.width as i32
            && y < self.y0 + self.height as i32
    }
}

/// 与 ROI 对齐的浮点 alpha 蒙版。
#[derive(Clone)]
pub struct AlphaRegion {
    pub rect: Rect,
    alpha: Vec<f32>,
}

impl AlphaRegion {
    #[cfg(test)]
    pub(crate) fn from_alpha(rect: Rect, alpha: Vec<f32>) -> Self {
        Self { rect, alpha }
    }

    pub fn alpha_at(&self, x: i32, y: i32) -> f32 {
        if !self.rect.contains(x, y) {
            return 0.0;
        }
        let index =
            (y - self.rect.y0) as usize * self.rect.width as usize + (x - self.rect.x0) as usize;
        self.alpha.get(index).copied().unwrap_or(0.0)
    }

    pub fn is_empty(&self) -> bool {
        self.alpha.iter().all(|value| *value <= 0.001)
    }
}

/// 由人脸几何构建脸部椭圆蒙版。
pub fn build_region(
    geometry: &FaceGeometry,
    image_width: u32,
    image_height: u32,
) -> Option<AlphaRegion> {
    const RX: f32 = 1.06;
    const RY: f32 = 1.48;
    const CENTER_V: f32 = 0.18;

    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for index in 0..24 {
        let angle = std::f32::consts::TAU * index as f32 / 24.0;
        let point = geometry.from_local(RX * angle.cos(), CENTER_V + RY * angle.sin());
        min_x = min_x.min(point.0);
        min_y = min_y.min(point.1);
        max_x = max_x.max(point.0);
        max_y = max_y.max(point.1);
    }
    let rect = Rect::from_bounds(min_x - 2.0, min_y - 2.0, max_x + 2.0, max_y + 2.0)
        .clamp_to(image_width, image_height)?;
    let mut alpha = vec![0.0f32; rect.width as usize * rect.height as usize];
    for y in 0..rect.height as i32 {
        for x in 0..rect.width as i32 {
            let pixel = (rect.x0 + x) as f32 + 0.5;
            let py = (rect.y0 + y) as f32 + 0.5;
            let (u, v) = geometry.to_local(pixel, py);
            let distance = ((u / RX).powi(2) + ((v - CENTER_V) / RY).powi(2)).sqrt();
            let value = 1.0 - smoothstep(0.80, 1.0, distance);
            if value > 0.001 {
                alpha[y as usize * rect.width as usize + x as usize] = value;
            }
        }
    }
    let region = AlphaRegion { rect, alpha };
    if region.is_empty() {
        None
    } else {
        Some(region)
    }
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// 多人脸重叠时按 alpha 归一化，避免同一像素被两张脸重复叠加。
///
/// 先基于所有原始蒙版计算每个像素的缩放系数，再统一写回；
/// 不能在循环中边改边算，否则后处理的区域会被前面已缩放的结果影响。
pub fn apply_ownership(regions: &mut [AlphaRegion]) {
    if regions.len() < 2 {
        return;
    }
    let scales: Vec<Vec<f32>> = regions
        .iter()
        .map(|region| {
            let rect = region.rect;
            let mut weights = vec![1.0f32; rect.width as usize * rect.height as usize];
            for y in 0..rect.height as i32 {
                for x in 0..rect.width as i32 {
                    let px = rect.x0 + x;
                    let py = rect.y0 + y;
                    let mut total = 0.0;
                    for other in regions.iter() {
                        total += other.alpha_at(px, py);
                    }
                    if total > 1.0 {
                        weights[y as usize * rect.width as usize + x as usize] = 1.0 / total;
                    }
                }
            }
            weights
        })
        .collect();
    for (region, weights) in regions.iter_mut().zip(scales) {
        for (value, weight) in region.alpha.iter_mut().zip(weights) {
            *value *= weight;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_engine::face_frame::{FaceFrame, NormPoint};

    fn face() -> FaceFrame {
        FaceFrame {
            id: 1,
            uuid: String::new(),
            score: 0.95,
            x: 0.3,
            y: 0.25,
            width: 0.4,
            height: 0.4,
            landmarks: vec![
                NormPoint { x: 0.44, y: 0.40 },
                NormPoint { x: 0.56, y: 0.40 },
                NormPoint { x: 0.50, y: 0.50 },
                NormPoint { x: 0.44, y: 0.58 },
                NormPoint { x: 0.56, y: 0.58 },
            ],
            dense: None,
            level3: None,
        }
    }

    #[test]
    fn region_is_soft_at_edges_and_zero_outside() {
        let geometry = FaceGeometry::from_face(&face(), 1000, 1000).unwrap();
        let region = build_region(&geometry, 1000, 1000).unwrap();
        let center = geometry.center();
        assert!(region.alpha_at(center.0 as i32, center.1 as i32) > 0.9);
        assert_eq!(region.alpha_at(5, 5), 0.0);
        assert_eq!(region.alpha_at(995, 995), 0.0);
    }

    #[test]
    fn overlapping_regions_are_normalized() {
        let geometry = FaceGeometry::from_face(&face(), 1000, 1000).unwrap();
        let a = build_region(&geometry, 1000, 1000).unwrap();
        let b = a.clone();
        let mut regions = vec![a, b];
        apply_ownership(&mut regions);
        let total: f32 = regions.iter().map(|region| region.alpha_at(500, 450)).sum();
        assert!(total <= 1.001);
    }

    #[test]
    fn region_is_clipped_to_image() {
        let geometry = FaceGeometry::from_face(&face(), 1000, 1000).unwrap();
        let region = build_region(&geometry, 1000, 1000).unwrap();
        assert!(region.rect.x0 >= 0);
        assert!(region.rect.y0 >= 0);
        assert!(region.rect.x0 + region.rect.width as i32 <= 1000);
    }
}
