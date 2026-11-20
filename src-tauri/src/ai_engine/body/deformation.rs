//! 身材位移场构建。
//!
//! 形状参数只作用于「轮廓带」：每行按 Alpha 找到左右边界，
//! 边缘向内一定宽度内的像素平滑移动，内部纹理（五官、衣服细节）保持不动。
//! 这避免了对整张图做水平压缩导致的人脸/背景变形。

use crate::image_engine::params::{BodyAdjust, BodyShape, MaskPoint, WarpMode};

use super::landmarks::{analyze, BodyMetrics};
use super::mesh::GridField;

/// 轮廓带宽度：占该行半宽的比例。带内平滑衰减到 0。
const EDGE_BAND: f32 = 0.55;
/// 形状参数对边缘位置的最大影响比例。
const MAX_SHAPE_RATIO: f32 = 0.22;
/// 网格顶点最大位移（相对图像宽/高）。
const MAX_DISPLACEMENT_RATIO: f32 = 0.12;

pub fn build_field(
    width: u32,
    height: u32,
    alpha: &[f32],
    adjust: &BodyAdjust,
) -> (GridField, Option<BodyMetrics>) {
    let mut field = GridField::new(width, height);
    let metrics = analyze(alpha, width, height);
    if let Some(metrics) = metrics.as_ref() {
        apply_shape(&mut field, metrics, &adjust.shape);
        apply_legs(&mut field, metrics, &adjust.shape, height);
    }
    apply_anchors(&mut field, width, height, adjust);
    apply_strokes(&mut field, width, adjust);
    gate_by_alpha(&mut field, alpha, width, height);
    field.clamp_magnitude(
        width as f32 * MAX_DISPLACEMENT_RATIO,
        height as f32 * MAX_DISPLACEMENT_RATIO,
    );
    (field, metrics)
}

fn gaussian(value: f32, center: f32, sigma: f32) -> f32 {
    let delta = (value - center) / sigma.max(0.0001);
    (-0.5 * delta * delta).exp()
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn shape_scale(shape: &BodyShape, fraction: f32) -> f32 {
    // 各部位高度权重。sigma 越小，作用范围越集中。
    let mut scale = 0.0;
    scale += shape.shoulders / 100.0 * gaussian(fraction, 0.16, 0.06);
    scale += shape.arms / 100.0 * gaussian(fraction, 0.28, 0.06);
    scale += shape.waist / 100.0 * gaussian(fraction, 0.40, 0.08);
    scale += shape.hips / 100.0 * gaussian(fraction, 0.53, 0.07);
    scale += shape.thighs / 100.0 * gaussian(fraction, 0.66, 0.06);
    scale += shape.calves / 100.0 * gaussian(fraction, 0.82, 0.05);
    scale.clamp(-1.0, 1.0)
}

fn apply_shape(field: &mut GridField, metrics: &BodyMetrics, shape: &BodyShape) {
    if shape.is_neutral() {
        return;
    }
    for row in 0..field.rows {
        let (_, y) = field.vertex(0, row);
        let Some((left, right)) = metrics.row_at(y) else {
            continue;
        };
        let half = (right - left) * 0.5;
        if half < 2.0 {
            continue;
        }
        let fraction = (y - metrics.top) / metrics.height();
        let scale = shape_scale(shape, fraction);
        if scale.abs() < 0.001 {
            continue;
        }
        let center = (left + right) * 0.5;
        let delta = half * scale * MAX_SHAPE_RATIO;
        let band = half * EDGE_BAND;
        for col in 0..field.cols {
            let (x, _) = field.vertex(col, row);
            if x < left - 2.0 || x > right + 2.0 || band <= 0.5 {
                continue;
            }
            // 轮廓带包含紧贴人物的外侧顶点，否则边缘位移无法插值到边界像素。
            let distance = if x < left {
                left - x
            } else if x > right {
                x - right
            } else {
                (x - left).min(right - x)
            };
            if distance > band {
                continue;
            }
            let falloff = smoothstep(1.0 - distance / band);
            let direction = if x < center { -1.0 } else { 1.0 };
            field.add(col, row, direction * delta * falloff, 0.0);
        }
    }
}

fn apply_legs(field: &mut GridField, metrics: &BodyMetrics, shape: &BodyShape, height: u32) {
    if shape.legs.abs() < 0.5 {
        return;
    }
    let hip_y = metrics.top + metrics.height() * 0.53;
    let max_dy = height as f32 * MAX_DISPLACEMENT_RATIO;
    for row in 0..field.rows {
        let (_, y) = field.vertex(0, row);
        if y <= hip_y {
            continue;
        }
        let ramp = smoothstep((y - hip_y) / (metrics.height() * 0.1).max(1.0));
        let dy = ((y - hip_y) * shape.legs / 100.0 * 0.06 * ramp).clamp(-max_dy, max_dy);
        if dy.abs() < 0.01 {
            continue;
        }
        for col in 0..field.cols {
            field.add(col, row, 0.0, dy);
        }
    }
}

fn apply_anchors(field: &mut GridField, width: u32, height: u32, adjust: &BodyAdjust) {
    if adjust.anchors.is_empty() {
        return;
    }
    let short = width.min(height) as f32;
    for anchor in &adjust.anchors {
        let cx = anchor.x * width as f32;
        let cy = anchor.y * height as f32;
        let radius = (anchor.radius * short).max(2.0);
        let sigma = radius * 0.5;
        let dx = anchor.dx * width as f32;
        let dy = anchor.dy * height as f32;
        let reach = radius * 3.0;
        for row in 0..field.rows {
            for col in 0..field.cols {
                let (x, y) = field.vertex(col, row);
                let distance = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
                if distance > reach {
                    continue;
                }
                let falloff = (-0.5 * (distance / sigma).powi(2)).exp();
                field.add(col, row, dx * falloff, dy * falloff);
            }
        }
    }
}

fn apply_strokes(field: &mut GridField, width: u32, adjust: &BodyAdjust) {
    if adjust.strokes.is_empty() {
        return;
    }
    let height = field.height;
    let mut protect = vec![1.0f32; field.dx.len()];
    for stroke in &adjust.strokes {
        if stroke.points.len() < 2 || stroke.strength <= 0.0 {
            continue;
        }
        let radius = (stroke.radius * width as f32).max(1.0);
        let strength = (stroke.strength / 100.0).clamp(0.0, 1.0);
        let (first, last) = (
            stroke.points.first().copied().unwrap_or(MaskPoint { x: 0.0, y: 0.0 }),
            stroke.points.last().copied().unwrap_or(MaskPoint { x: 0.0, y: 0.0 }),
        );
        let direction_x = (last.x - first.x) * width as f32;
        let direction_y = (last.y - first.y) * height as f32;
        let length = (direction_x * direction_x + direction_y * direction_y).sqrt();
        let (unit_x, unit_y) = if length > 0.001 {
            (direction_x / length, direction_y / length)
        } else {
            (0.0, 0.0)
        };
        let travel = (radius * 0.6 * strength).min(radius * 0.8);
        let (center_x, center_y) = stroke_center(stroke, width, height);

        for row in 0..field.rows {
            for col in 0..field.cols {
                let (x, y) = field.vertex(col, row);
                let distance = distance_to_points(x, y, &stroke.points, width, height);
                if distance > radius {
                    continue;
                }
                let falloff = smoothstep(1.0 - distance / radius);
                let index = field.index(col, row);
                let weight = falloff * strength;
                match stroke.mode {
                    WarpMode::Push => {
                        field.dx[index] += unit_x * travel * weight * protect[index];
                        field.dy[index] += unit_y * travel * weight * protect[index];
                    }
                    WarpMode::Pull => {
                        field.dx[index] -= unit_x * travel * weight * protect[index];
                        field.dy[index] -= unit_y * travel * weight * protect[index];
                    }
                    WarpMode::ZoomIn | WarpMode::ZoomOut => {
                        let sign = if stroke.mode == WarpMode::ZoomIn { 1.0 } else { -1.0 };
                        let vx = x - center_x;
                        let vy = y - center_y;
                        let factor = sign * 0.22 * weight;
                        field.dx[index] += vx * factor * protect[index];
                        field.dy[index] += vy * factor * protect[index];
                    }
                    WarpMode::Protect => {
                        protect[index] *= 1.0 - weight;
                        field.dx[index] *= 1.0 - weight;
                        field.dy[index] *= 1.0 - weight;
                    }
                    WarpMode::Restore => {
                        field.dx[index] *= 1.0 - weight;
                        field.dy[index] *= 1.0 - weight;
                    }
                }
            }
        }
    }
}

fn stroke_center(
    stroke: &crate::image_engine::params::WarpStroke,
    width: u32,
    height: u32,
) -> (f32, f32) {
    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;
    for point in &stroke.points {
        min_x = min_x.min(point.x * width as f32);
        max_x = max_x.max(point.x * width as f32);
        min_y = min_y.min(point.y * height as f32);
        max_y = max_y.max(point.y * height as f32);
    }
    ((min_x + max_x) * 0.5, (min_y + max_y) * 0.5)
}

fn distance_to_points(x: f32, y: f32, points: &[MaskPoint], width: u32, height: u32) -> f32 {
    if points.len() < 2 {
        return f32::MAX;
    }
    let mut best = f32::MAX;
    for pair in points.windows(2) {
        let ax = pair[0].x * width as f32;
        let ay = pair[0].y * height as f32;
        let bx = pair[1].x * width as f32;
        let by = pair[1].y * height as f32;
        let vx = bx - ax;
        let vy = by - ay;
        let length_sq = vx * vx + vy * vy;
        let t = if length_sq <= 0.001 {
            0.0
        } else {
            (((x - ax) * vx + (y - ay) * vy) / length_sq).clamp(0.0, 1.0)
        };
        let cx = ax + vx * t;
        let cy = ay + vy * t;
        best = best.min(((x - cx).powi(2) + (y - cy).powi(2)).sqrt());
    }
    best
}

fn gate_by_alpha(field: &mut GridField, alpha: &[f32], width: u32, height: u32) {
    if alpha.len() != width as usize * height as usize {
        return;
    }
    // 轮廓带需要把紧贴人物的背景一起带着移动，否则边缘只能移动不到 1px。
    // 因此使用 2 个网格单元半径的膨胀 Alpha 做门控，远处背景位移仍为 0。
    let cell_x = width as f32 / (field.cols.saturating_sub(1).max(1)) as f32;
    let cell_y = height as f32 / (field.rows.saturating_sub(1).max(1)) as f32;
    let reach_x = (cell_x * 2.0).round().max(1.0) as i32;
    let reach_y = (cell_y * 2.0).round().max(1.0) as i32;
    for row in 0..field.rows {
        for col in 0..field.cols {
            let (x, y) = field.vertex(col, row);
            let cx = (x.floor() as i32).clamp(0, width as i32 - 1);
            let cy = (y.floor() as i32).clamp(0, height as i32 - 1);
            let mut value = 0.0f32;
            for py in (cy - reach_y).max(0)..=(cy + reach_y).min(height as i32 - 1) {
                for px in (cx - reach_x).max(0)..=(cx + reach_x).min(width as i32 - 1) {
                    value = value.max(alpha[py as usize * width as usize + px as usize]);
                }
            }
            let value = value.clamp(0.0, 1.0);
            let index = field.index(col, row);
            field.dx[index] *= value;
            field.dy[index] *= value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_engine::params::{BodyAnchor, BodyShape, WarpStroke};
    use crate::image_engine::params::MaskPoint;

    fn body_alpha(width: u32, height: u32) -> Vec<f32> {
        let mut alpha = vec![0.0f32; (width * height) as usize];
        for y in 20..100u32 {
            for x in 20..60u32 {
                alpha[y as usize * width as usize + x as usize] = 1.0;
            }
        }
        alpha
    }

    #[test]
    fn neutral_adjust_produces_zero_field() {
        let alpha = body_alpha(100, 120);
        let (field, metrics) = build_field(100, 120, &alpha, &BodyAdjust::new("a"));
        assert!(metrics.is_some());
        assert!(field.is_zero());
    }

    #[test]
    fn slim_waist_moves_left_edge_inward() {
        let alpha = body_alpha(100, 120);
        let adjust = BodyAdjust {
            subject_id: "a".into(),
            enabled: true,
            shape: BodyShape {
                waist: -80.0,
                ..Default::default()
            },
            anchors: Vec::new(),
            strokes: Vec::new(),
            mask_strokes: Vec::new(),
        };
        let (field, metrics) = build_field(100, 120, &alpha, &adjust);
        let metrics = metrics.unwrap();
        let row = metrics.top + metrics.height() * 0.40;
        let left = metrics.row_at(row).unwrap().0;
        let (dx, _) = field.displacement_at(left + 2.0, row);
        // 左边缘向右（内）移动。
        assert!(dx > 0.5, "dx={dx}");
    }

    #[test]
    fn anchor_moves_grid_vertex() {
        let alpha = body_alpha(100, 120);
        let adjust = BodyAdjust {
            subject_id: "a".into(),
            enabled: true,
            shape: BodyShape::default(),
            anchors: vec![BodyAnchor {
                part: crate::image_engine::params::BodyAnchorPart::WaistLeft,
                x: 0.4,
                y: 0.5,
                dx: 0.04,
                dy: 0.0,
                radius: 0.2,
                locked: false,
            }],
            strokes: Vec::new(),
            mask_strokes: Vec::new(),
        };
        let (field, _) = build_field(100, 120, &alpha, &adjust);
        let (dx, _) = field.displacement_at(40.0, 60.0);
        assert!(dx > 1.0, "dx={dx}");
    }

    #[test]
    fn push_stroke_changes_field_inside_mask() {
        let alpha = body_alpha(100, 120);
        let adjust = BodyAdjust {
            subject_id: "a".into(),
            enabled: true,
            shape: BodyShape::default(),
            anchors: Vec::new(),
            strokes: vec![WarpStroke {
                mode: WarpMode::Push,
                radius: 0.2,
                strength: 100.0,
                feather: 60.0,
                points: vec![MaskPoint { x: 0.4, y: 0.5 }, MaskPoint { x: 0.55, y: 0.5 }],
            }],
            mask_strokes: Vec::new(),
        };
        let (field, _) = build_field(100, 120, &alpha, &adjust);
        let (dx, _) = field.displacement_at(45.0, 60.0);
        assert!(dx > 0.5, "dx={dx}");
    }

    #[test]
    fn displacement_never_exceeds_limit() {
        let alpha = body_alpha(100, 120);
        let adjust = BodyAdjust {
            subject_id: "a".into(),
            enabled: true,
            shape: BodyShape {
                waist: 100.0,
                ..Default::default()
            },
            anchors: vec![],
            strokes: Vec::new(),
            mask_strokes: Vec::new(),
        };
        let (field, _) = build_field(100, 120, &alpha, &adjust);
        assert!(field.max_magnitude() <= 100.0 * MAX_DISPLACEMENT_RATIO + 0.5);
    }
}
