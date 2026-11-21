//! 手动推拉/放大/缩小/保护/恢复。
//!
//! 笔触用图像归一化坐标保存，实际作用在像素位移场上：
//! - 推动/拉动：沿拖动方向施加高斯衰减位移；
//! - 放大/缩小：以笔触中心做局部径向缩放；
//! - 保护：降低该区域的后续位移；
//! - 恢复：把该区域混合回本次塑形前的像素（不是原图文件）。
//!
//! 位移场用固定点迭代做反向映射，位移上限由眼距约束，避免撕裂。

use crate::error::AppResult;
use crate::face_reshape::boundary::{AlphaRegion, Rect};
use crate::image_engine::buffer::Image16;
use crate::image_engine::cancel::CancelToken;
use crate::image_engine::params::{WarpMode, WarpStroke};

#[derive(Debug, Default, Clone, Copy)]
pub struct WarpStats {
    pub pixels: u32,
    pub max_displacement: f32,
}

pub struct ManualEffect {
    pub rect: Rect,
    dx: Vec<f32>,
    dy: Vec<f32>,
    restore: Vec<f32>,
    pub max_displacement: f32,
}

impl ManualEffect {
    fn index(&self, x: i32, y: i32) -> Option<usize> {
        if x < self.rect.x0
            || y < self.rect.y0
            || x >= self.rect.x0 + self.rect.width as i32
            || y >= self.rect.y0 + self.rect.height as i32
        {
            return None;
        }
        Some((y - self.rect.y0) as usize * self.rect.width as usize + (x - self.rect.x0) as usize)
    }

    fn displacement(&self, x: f32, y: f32) -> (f32, f32) {
        // 双线性采样位移场，边界外返回 0。
        let fx = x.floor();
        let fy = y.floor();
        let tx = x - fx;
        let ty = y - fy;
        let sample = |ix: i32, iy: i32| -> (f32, f32) {
            match self.index(ix, iy) {
                Some(index) => (self.dx[index], self.dy[index]),
                None => (0.0, 0.0),
            }
        };
        let (dx00, dy00) = sample(fx as i32, fy as i32);
        let (dx10, dy10) = sample(fx as i32 + 1, fy as i32);
        let (dx01, dy01) = sample(fx as i32, fy as i32 + 1);
        let (dx11, dy11) = sample(fx as i32 + 1, fy as i32 + 1);
        (
            (dx00 * (1.0 - tx) + dx10 * tx) * (1.0 - ty) + (dx01 * (1.0 - tx) + dx11 * tx) * ty,
            (dy00 * (1.0 - tx) + dy10 * tx) * (1.0 - ty) + (dy01 * (1.0 - tx) + dy11 * tx) * ty,
        )
    }

    fn alpha_at(&self, values: &[f32], x: i32, y: i32) -> f32 {
        self.index(x, y).map(|index| values[index]).unwrap_or(0.0)
    }
}

fn falloff(distance: f32, radius: f32, feather: f32) -> f32 {
    crate::face_reshape::mask::feather_weight(distance, radius, feather)
}

/// 从笔触构建位移场；没有有效笔触时返回 None。
pub fn build(
    strokes: &[WarpStroke],
    image_width: u32,
    image_height: u32,
    eye_distance: f32,
) -> Option<ManualEffect> {
    let strokes: Vec<&WarpStroke> = strokes
        .iter()
        .filter(|stroke| !stroke.points.is_empty())
        .collect();
    if strokes.is_empty() {
        return None;
    }

    let width = image_width.max(1) as f32;
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for stroke in &strokes {
        let radius = stroke.radius * width * 1.3;
        for point in &stroke.points {
            let x = point.x * image_width as f32;
            let y = point.y * image_height as f32;
            min_x = min_x.min(x - radius);
            min_y = min_y.min(y - radius);
            max_x = max_x.max(x + radius);
            max_y = max_y.max(y + radius);
        }
    }
    if !min_x.is_finite() {
        return None;
    }
    let rect = Rect {
        x0: min_x.floor() as i32,
        y0: min_y.floor() as i32,
        width: (max_x - min_x).ceil().max(1.0) as u32,
        height: (max_y - min_y).ceil().max(1.0) as u32,
    }
    .clamp_to(image_width, image_height)?;

    let count = rect.width as usize * rect.height as usize;
    let mut dx = vec![0.0f32; count];
    let mut dy = vec![0.0f32; count];
    let mut protect = vec![0.0f32; count];
    let mut restore = vec![0.0f32; count];
    let max_total = (0.26 * eye_distance).max(8.0);
    let mut max_displacement = 0.0f32;

    for stroke in strokes {
        let radius = (stroke.radius * width).clamp(2.0, width * 0.25);
        let strength = stroke.strength / 100.0;
        let feather = stroke.feather / 100.0;
        let points: Vec<(f32, f32)> = stroke
            .points
            .iter()
            .map(|point| (point.x * image_width as f32, point.y * image_height as f32))
            .collect();

        match stroke.mode {
            WarpMode::Protect | WarpMode::Restore => {
                let target = if stroke.mode == WarpMode::Protect {
                    &mut protect
                } else {
                    &mut restore
                };
                for index in 0..points.len() {
                    stamp(target, &rect, points[index], radius, feather, 1.0);
                }
            }
            WarpMode::ZoomIn | WarpMode::ZoomOut => {
                let mut cx = 0.0;
                let mut cy = 0.0;
                for point in &points {
                    cx += point.0;
                    cy += point.1;
                }
                cx /= points.len() as f32;
                cy /= points.len() as f32;
                let sign = if stroke.mode == WarpMode::ZoomIn {
                    1.0
                } else {
                    -1.0
                };
                for point in &points {
                    stamp_pair_into(
                        &mut dx,
                        &mut dy,
                        &rect,
                        *point,
                        radius,
                        feather,
                        |x, y, weight| {
                            let ox = x - cx;
                            let oy = y - cy;
                            let length = (ox * ox + oy * oy).sqrt().max(1.0);
                            let amount = weight * strength * radius * 0.22 * sign;
                            (ox / length * amount, oy / length * amount)
                        },
                    );
                }
            }
            WarpMode::Push | WarpMode::Pull => {
                let sign = if stroke.mode == WarpMode::Push {
                    1.0
                } else {
                    -1.0
                };
                for index in 0..points.len() {
                    let previous = points[index.saturating_sub(1)];
                    let next = points[(index + 1).min(points.len() - 1)];
                    let mut vx = next.0 - previous.0;
                    let mut vy = next.1 - previous.1;
                    let length = (vx * vx + vy * vy).sqrt();
                    if length < 0.5 {
                        continue;
                    }
                    vx /= length;
                    vy /= length;
                    let spacing = ((next.0 - previous.0).powi(2) + (next.1 - previous.1).powi(2))
                        .sqrt()
                        * 0.5;
                    let step = (spacing / (radius * 0.8)).clamp(0.12, 1.0);
                    let amount = sign * strength * radius * 0.55 * step;
                    stamp_pair_into(
                        &mut dx,
                        &mut dy,
                        &rect,
                        points[index],
                        radius,
                        feather,
                        |_x, _y, weight| (vx * amount * weight, vy * amount * weight),
                    );
                }
            }
        }
    }

    // 保护笔刷叠加、总位移上限。
    for index in 0..count {
        let guard = (1.0 - protect[index]).clamp(0.0, 1.0);
        dx[index] *= guard;
        dy[index] *= guard;
        let magnitude = (dx[index] * dx[index] + dy[index] * dy[index]).sqrt();
        if magnitude > max_total {
            dx[index] *= max_total / magnitude;
            dy[index] *= max_total / magnitude;
        }
        max_displacement =
            max_displacement.max((dx[index] * dx[index] + dy[index] * dy[index]).sqrt());
    }

    Some(ManualEffect {
        rect,
        dx,
        dy,
        restore,
        max_displacement,
    })
}

fn stamp(
    target: &mut [f32],
    rect: &Rect,
    center: (f32, f32),
    radius: f32,
    feather: f32,
    strength: f32,
) {
    let x0 = (center.0 - radius).floor() as i32;
    let x1 = (center.0 + radius).ceil() as i32;
    let y0 = (center.1 - radius).floor() as i32;
    let y1 = (center.1 + radius).ceil() as i32;
    for y in y0..=y1 {
        for x in x0..=x1 {
            if x < rect.x0
                || y < rect.y0
                || x >= rect.x0 + rect.width as i32
                || y >= rect.y0 + rect.height as i32
            {
                continue;
            }
            let distance = ((x as f32 - center.0).powi(2) + (y as f32 - center.1).powi(2)).sqrt();
            let weight = falloff(distance, radius, feather) * strength;
            if weight <= 0.001 {
                continue;
            }
            let index = (y - rect.y0) as usize * rect.width as usize + (x - rect.x0) as usize;
            target[index] = (target[index] + weight).min(1.0);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn stamp_pair_into(
    target_x: &mut [f32],
    target_y: &mut [f32],
    rect: &Rect,
    center: (f32, f32),
    radius: f32,
    feather: f32,
    value: impl Fn(f32, f32, f32) -> (f32, f32),
) {
    let x0 = (center.0 - radius).floor() as i32;
    let x1 = (center.0 + radius).ceil() as i32;
    let y0 = (center.1 - radius).floor() as i32;
    let y1 = (center.1 + radius).ceil() as i32;
    for y in y0..=y1 {
        for x in x0..=x1 {
            if x < rect.x0
                || y < rect.y0
                || x >= rect.x0 + rect.width as i32
                || y >= rect.y0 + rect.height as i32
            {
                continue;
            }
            let distance = ((x as f32 - center.0).powi(2) + (y as f32 - center.1).powi(2)).sqrt();
            let weight = falloff(distance, radius, feather);
            if weight <= 0.001 {
                continue;
            }
            let (vx, vy) = value(x as f32, y as f32, weight);
            let index = (y - rect.y0) as usize * rect.width as usize + (x - rect.x0) as usize;
            target_x[index] += vx;
            target_y[index] += vy;
        }
    }
}

/// 在已经完成网格变形的图像上应用手动位移场。
///
/// `region` 是允许改动的面部区域；位移场超出该区域的部分会被忽略。
/// `original_roi` 是塑形前的 ROI 像素，用于“恢复”笔刷。
pub fn apply(
    image: &mut Image16,
    effect: &ManualEffect,
    region: &AlphaRegion,
    original_roi: &[u16],
    cancel: &CancelToken,
) -> AppResult<WarpStats> {
    let rect = effect.rect.clamp_to(image.width(), image.height());
    let Some(rect) = rect else {
        return Ok(WarpStats::default());
    };
    let mut stats = WarpStats::default();
    for y in rect.y0..rect.y0 + rect.height as i32 {
        if y % 32 == 0 {
            cancel.check()?;
        }
        for x in rect.x0..rect.x0 + rect.width as i32 {
            if region.alpha_at(x, y) <= 0.001 {
                continue;
            }
            let (mut sx, mut sy) = (x as f32 + 0.5, y as f32 + 0.5);
            for _ in 0..3 {
                let (dx, dy) = effect.displacement(sx, sy);
                sx = x as f32 + 0.5 - dx;
                sy = y as f32 + 0.5 - dy;
            }
            let mut pixel = bilinear(image, sx, sy);
            let restore = effect.alpha_at(&effect.restore, x, y);
            if restore > 0.001 {
                let original = original_pixel(original_roi, region.rect, x, y);
                for channel in 0..4 {
                    pixel[channel] = (pixel[channel] as f32 * (1.0 - restore)
                        + original[channel] as f32 * restore)
                        .round()
                        .clamp(0.0, 65535.0) as u16;
                }
            }
            image.put_pixel(x as u32, y as u32, image::Rgba(pixel));
            stats.pixels += 1;
        }
    }
    stats.max_displacement = effect.max_displacement;
    Ok(stats)
}

fn original_pixel(roi: &[u16], region: Rect, x: i32, y: i32) -> [u16; 4] {
    let index = ((y - region.y0) as usize * region.width as usize + (x - region.x0) as usize) * 4;
    if index + 3 < roi.len() {
        [roi[index], roi[index + 1], roi[index + 2], roi[index + 3]]
    } else {
        [0, 0, 0, 65535]
    }
}

fn bilinear(image: &Image16, x: f32, y: f32) -> [u16; 4] {
    let max_x = (image.width().saturating_sub(1)) as f32;
    let max_y = (image.height().saturating_sub(1)) as f32;
    let x = x.clamp(0.0, max_x);
    let y = y.clamp(0.0, max_y);
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(max_x as u32);
    let y1 = (y0 + 1).min(max_y as u32);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let p00 = image.get_pixel(x0, y0).0;
    let p10 = image.get_pixel(x1, y0).0;
    let p01 = image.get_pixel(x0, y1).0;
    let p11 = image.get_pixel(x1, y1).0;
    let mut output = [0u16; 4];
    for channel in 0..4 {
        let top = p00[channel] as f32 * (1.0 - tx) + p10[channel] as f32 * tx;
        let bottom = p01[channel] as f32 * (1.0 - tx) + p11[channel] as f32 * tx;
        output[channel] = (top * (1.0 - ty) + bottom * ty).round().clamp(0.0, 65535.0) as u16;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_engine::params::MaskPoint;
    use image::{GenericImageView, Rgba};

    fn image() -> Image16 {
        Image16::from_fn(64, 64, |x, y| {
            Rgba([(x * 900) as u16, (y * 900) as u16, 20_000, 65535])
        })
    }

    fn stroke(mode: WarpMode, points: Vec<(f32, f32)>, radius: f32) -> WarpStroke {
        WarpStroke {
            mode,
            radius,
            strength: 100.0,
            feather: 100.0,
            points: points
                .into_iter()
                .map(|(x, y)| MaskPoint { x, y })
                .collect(),
        }
    }

    #[test]
    fn push_changes_pixels_and_stays_in_region() {
        let mut image = image();
        let before = image.clone();
        let effect = build(
            &[stroke(WarpMode::Push, vec![(0.5, 0.5), (0.6, 0.5)], 0.12)],
            64,
            64,
            20.0,
        )
        .unwrap();
        let rect = Rect {
            x0: 20,
            y0: 20,
            width: 30,
            height: 30,
        };
        let region = AlphaRegion::from_alpha(rect, vec![1.0; 30 * 30]);
        let roi = image
            .view(20, 20, 30, 30)
            .pixels()
            .flat_map(|(_, _, pixel)| pixel.0)
            .collect::<Vec<u16>>();
        apply(&mut image, &effect, &region, &roi, &CancelToken::default()).unwrap();
        let changed = image
            .pixels()
            .zip(before.pixels())
            .filter(|(a, b)| a != b)
            .count();
        assert!(changed > 10);
        // 区域外像素必须完全不变。
        for y in 0..64u32 {
            for x in 0..64u32 {
                if x < 20 || y < 20 || x >= 50 || y >= 50 {
                    assert_eq!(image.get_pixel(x, y), before.get_pixel(x, y));
                }
            }
        }
    }

    #[test]
    fn protect_suppresses_displacement() {
        let push = stroke(WarpMode::Push, vec![(0.5, 0.5), (0.6, 0.5)], 0.12);
        let unprotected = build(std::slice::from_ref(&push), 64, 64, 20.0).unwrap();
        let protected = build(
            &[
                push,
                stroke(WarpMode::Protect, vec![(0.5, 0.5), (0.6, 0.5)], 0.12),
            ],
            64,
            64,
            20.0,
        )
        .unwrap();
        assert!(protected.max_displacement < unprotected.max_displacement * 0.8);
    }

    #[test]
    fn restore_brings_back_original_pixels() {
        let mut image = image();
        let original = image.clone();
        let effect = build(
            &[
                stroke(WarpMode::Push, vec![(0.5, 0.5), (0.6, 0.5)], 0.12),
                stroke(WarpMode::Restore, vec![(0.55, 0.5)], 0.12),
            ],
            64,
            64,
            20.0,
        )
        .unwrap();
        let rect = Rect {
            x0: 0,
            y0: 0,
            width: 64,
            height: 64,
        };
        let region = AlphaRegion::from_alpha(rect, vec![1.0; 64 * 64]);
        let roi = original.as_raw().clone();
        apply(&mut image, &effect, &region, &roi, &CancelToken::default()).unwrap();
        assert_eq!(image.get_pixel(35, 32), original.get_pixel(35, 32));
    }

    #[test]
    fn empty_strokes_produce_no_effect() {
        assert!(build(&[], 64, 64, 20.0).is_none());
    }
}
