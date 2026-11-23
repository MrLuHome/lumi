//! 智能祛痘祛斑。
//!
//! 保守策略：
//! - 只在皮肤蒙版内检测；
//! - 以“局部低频亮度残差”找候选，面积/对比度/紧凑度三重约束；
//! - 过暗的大块（痣）、密集小点（雀斑/毛孔）直接放弃自动修复；
//! - 修复用环形皮肤采样色 + 边缘羽化，不做整片模糊。

use std::collections::VecDeque;

use crate::ai_engine::retouch::filter::{box_mean, luma};
use crate::ai_engine::skin_segmentation::SkinMask;
use crate::error::AppResult;
use crate::image_engine::buffer::{Image16, CHANNEL_MAX};
use crate::image_engine::cancel::CancelToken;
use crate::image_engine::params::RepairPoint;

#[derive(Debug, Default, Clone)]
pub struct BlemishReport {
    pub detected: usize,
    pub repaired: usize,
    pub skipped_reason: Option<String>,
}

struct Component {
    pixels: Vec<u32>,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    mean_residual: f32,
}

pub fn repair_blemishes(
    image: &mut Image16,
    mask: &SkinMask,
    blemish: f32,
    spots: f32,
    blackheads: f32,
    repair_points: &[RepairPoint],
    cancel: &CancelToken,
) -> AppResult<BlemishReport> {
    let mut report = BlemishReport::default();
    let targets = repair_targets(image, repair_points);
    let amount = blemish.max(spots).clamp(0.0, 100.0);
    if amount > 0.0 {
        let result = repair_pass(
            image,
            mask,
            amount,
            0.030 + 0.022 * (1.0 - amount / 100.0),
            (mask.width as f32 * 0.045).powi(2).max(24.0) as usize,
            -0.22,
            40,
            &targets,
            cancel,
        )?;
        report.detected += result.detected;
        report.repaired += result.repaired;
        if result.skipped_reason.is_some() {
            report.skipped_reason = result.skipped_reason;
        }
    }
    if blackheads > 0.0 {
        let result = repair_pass(
            image,
            mask,
            blackheads.clamp(0.0, 100.0),
            0.045 + 0.020 * (1.0 - blackheads / 100.0),
            48,
            -0.30,
            120,
            &targets,
            cancel,
        )?;
        report.detected += result.detected;
        report.repaired += result.repaired;
        if report.skipped_reason.is_none() && result.skipped_reason.is_some() {
            report.skipped_reason = result.skipped_reason;
        }
    }
    Ok(report)
}

/// 手动修复点转换到图像像素坐标；为空表示全自动模式。
fn repair_targets(image: &Image16, points: &[RepairPoint]) -> Vec<(f32, f32, f32)> {
    let width = image.width().max(1) as f32;
    let height = image.height().max(1) as f32;
    points
        .iter()
        .map(|point| {
            (
                point.x * width,
                point.y * height,
                (point.radius * width).max(2.0),
            )
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn repair_pass(
    image: &mut Image16,
    mask: &SkinMask,
    amount: f32,
    threshold: f32,
    max_area: usize,
    dark_cut: f32,
    max_components: usize,
    targets: &[(f32, f32, f32)],
    cancel: &CancelToken,
) -> AppResult<BlemishReport> {
    let width = mask.width;
    let height = mask.height;
    // 全 ROI 真实亮度（非 0 占位），避免 box_mean 被 0 拉偏产生条纹/假候选。
    let mut luma_plane = vec![0f32; (width * height) as usize];
    let mut skin_pixels = 0usize;
    for y in 0..height {
        for x in 0..width {
            let index = (y * width + x) as usize;
            let image_x = mask.x0 as u32 + x;
            let image_y = mask.y0 as u32 + y;
            if image_x >= image.width() || image_y >= image.height() {
                continue;
            }
            let value = luma(image.get_pixel(image_x, image_y).0);
            luma_plane[index] = value;
            if mask.alpha[index] > 0.3 {
                skin_pixels += 1;
            }
        }
    }
    if skin_pixels < 200 {
        return Ok(BlemishReport::default());
    }

    let radius = ((width as f32 * 0.010).round() as u32).clamp(3, 12);
    let base = box_mean(&luma_plane, width, height, radius);

    let mut candidate = vec![false; luma_plane.len()];
    let mut candidate_count = 0usize;
    for index in 0..luma_plane.len() {
        if mask.alpha[index] <= 0.35 {
            continue;
        }
        let residual = luma_plane[index] - base[index];
        if residual < -threshold && residual > dark_cut && luma_plane[index] > 0.08 {
            candidate[index] = true;
            candidate_count += 1;
        }
    }
    // 密集小点更像雀斑/毛孔：自动模式直接放弃；手动点选时由目标区域过滤。
    if targets.is_empty() && candidate_count > (skin_pixels as f32 * 0.012) as usize {
        return Ok(BlemishReport {
            detected: candidate_count,
            repaired: 0,
            skipped_reason: Some("检测到密集小点（可能是雀斑或毛孔），保守模式跳过自动修复".into()),
        });
    }

    let mut visited = vec![false; candidate.len()];
    let mut components: Vec<Component> = Vec::new();
    for start in 0..candidate.len() {
        if !candidate[start] || visited[start] {
            continue;
        }
        let mut queue = VecDeque::new();
        queue.push_back(start);
        visited[start] = true;
        let mut pixels = Vec::new();
        let mut residual_sum = 0f32;
        let mut x0 = i32::MAX;
        let mut y0 = i32::MAX;
        let mut x1 = i32::MIN;
        let mut y1 = i32::MIN;
        while let Some(index) = queue.pop_front() {
            pixels.push(index as u32);
            residual_sum += luma_plane[index] - base[index];
            let x = (index as u32 % width) as i32;
            let y = (index as u32 / width) as i32;
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let nx = x + dx;
                    let ny = y + dy;
                    if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                        continue;
                    }
                    let neighbor = (ny as u32 * width + nx as u32) as usize;
                    if candidate[neighbor] && !visited[neighbor] {
                        visited[neighbor] = true;
                        queue.push_back(neighbor);
                    }
                }
            }
        }
        let area = pixels.len();
        let box_width = (x1 - x0 + 1).max(1);
        let box_height = (y1 - y0 + 1).max(1);
        let aspect = box_width.max(box_height) as f32 / box_width.min(box_height).max(1) as f32;
        if area < 4 || area > max_area || aspect > 3.2 {
            continue;
        }
        let mean_residual = residual_sum / area as f32;
        if mean_residual > -threshold * 0.8 {
            continue;
        }
        if !targets.is_empty() {
            let center_x = mask.x0 as f32 + (x0 + x1) as f32 * 0.5;
            let center_y = mask.y0 as f32 + (y0 + y1) as f32 * 0.5;
            let hit = targets.iter().any(|(target_x, target_y, radius)| {
                let dx = center_x - target_x;
                let dy = center_y - target_y;
                dx * dx + dy * dy <= radius * radius
            });
            if !hit {
                continue;
            }
        }
        components.push(Component {
            pixels,
            x0,
            y0,
            x1,
            y1,
            mean_residual,
        });
    }

    if targets.is_empty() && components.len() > max_components {
        return Ok(BlemishReport {
            detected: components.len(),
            repaired: 0,
            skipped_reason: Some("候选瑕疵过多，保守模式跳过自动修复".into()),
        });
    }
    if components.is_empty() {
        return Ok(BlemishReport::default());
    }

    // 对比度更高的先修，强度决定修复比例。
    components.sort_by(|a, b| {
        a.mean_residual
            .partial_cmp(&b.mean_residual)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let repair_count =
        ((components.len() as f32 * amount / 100.0).ceil() as usize).clamp(1, components.len());

    let mut member = vec![false; luma_plane.len()];
    let mut repaired = 0usize;
    for component in components.iter().take(repair_count) {
        cancel.check()?;
        for pixel in component.pixels.iter() {
            member[*pixel as usize] = true;
        }

        let area = component.pixels.len() as f32;
        let ring = (area.sqrt() as i32).clamp(3, 14);
        let ex0 = (component.x0 - ring).max(0);
        let ey0 = (component.y0 - ring).max(0);
        let ex1 = (component.x1 + ring).min(width as i32 - 1);
        let ey1 = (component.y1 + ring).min(height as i32 - 1);

        let mut sum = [0f32; 3];
        let mut count = 0f32;
        for y in ey0..=ey1 {
            for x in ex0..=ex1 {
                let index = (y as u32 * width + x as u32) as usize;
                if member[index] || mask.alpha[index] <= 0.3 {
                    continue;
                }
                let image_x = mask.x0 as u32 + x as u32;
                let image_y = mask.y0 as u32 + y as u32;
                if image_x >= image.width() || image_y >= image.height() {
                    continue;
                }
                let pixel = image.get_pixel(image_x, image_y).0;
                for channel in 0..3 {
                    sum[channel] += pixel[channel] as f32;
                }
                count += 1.0;
            }
        }
        if count < 4.0 {
            for pixel in component.pixels.iter() {
                member[*pixel as usize] = false;
            }
            continue;
        }
        let fill = [sum[0] / count, sum[1] / count, sum[2] / count];
        // 降低平色填充强度，并用到边界的软衰减替代 1 像素硬边，
        // 避免连通域被填成「多边形色块」。
        let base_strength = 0.55 * (amount / 100.0).clamp(0.30, 1.0);

        // 到组件外缘的近似距离（多次扫描膨胀），用于软过渡。
        let mut edge_dist = vec![0u8; luma_plane.len()];
        let mut queue: VecDeque<u32> = VecDeque::new();
        for pixel in component.pixels.iter() {
            let index = *pixel as usize;
            let x = (index as u32 % width) as i32;
            let y = (index as u32 / width) as i32;
            let mut is_edge = false;
            for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                let nx = x + dx;
                let ny = y + dy;
                if nx < 0
                    || ny < 0
                    || nx >= width as i32
                    || ny >= height as i32
                    || !member[(ny as u32 * width + nx as u32) as usize]
                {
                    is_edge = true;
                    break;
                }
            }
            if is_edge {
                edge_dist[index] = 1;
                queue.push_back(*pixel);
            }
        }
        let max_ring = ring.min(10) as u8;
        while let Some(pixel) = queue.pop_front() {
            let index = pixel as usize;
            let d = edge_dist[index];
            if d >= max_ring {
                continue;
            }
            let x = (index as u32 % width) as i32;
            let y = (index as u32 / width) as i32;
            for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                let nx = x + dx;
                let ny = y + dy;
                if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                    continue;
                }
                let ni = (ny as u32 * width + nx as u32) as usize;
                if member[ni] && edge_dist[ni] == 0 {
                    edge_dist[ni] = d + 1;
                    queue.push_back(ni as u32);
                }
            }
        }

        for pixel in component.pixels.iter() {
            let index = *pixel as usize;
            let x = (index as u32 % width) as i32;
            let y = (index as u32 / width) as i32;
            let image_x = mask.x0 as u32 + x as u32;
            let image_y = mask.y0 as u32 + y as u32;
            if image_x >= image.width() || image_y >= image.height() {
                continue;
            }
            // 距外缘越近混合越弱；内部充分填充，边缘平滑过渡。
            let d = edge_dist[index] as f32;
            let t = (d / (max_ring as f32).max(1.0)).clamp(0.0, 1.0);
            let soft = t * t * (3.0 - 2.0 * t);
            let mix = base_strength * soft;
            if mix <= 0.01 {
                continue;
            }
            let pixel = image.get_pixel_mut(image_x, image_y);
            for channel in 0..3 {
                let source = pixel.0[channel] as f32;
                let value = source + (fill[channel] - source) * mix;
                pixel.0[channel] = value.clamp(0.0, CHANNEL_MAX).round() as u16;
            }
        }
        repaired += 1;
        for pixel in component.pixels.iter() {
            member[*pixel as usize] = false;
        }
    }

    Ok(BlemishReport {
        detected: components.len(),
        repaired,
        skipped_reason: None,
    })
}
