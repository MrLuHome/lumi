//! 手动人物蒙版：把归一化笔触栅格化为软 Alpha。
//!
//! 笔触使用「胶囊」距离场：点到折线段的距离决定权重，
//! 硬度控制从实心到柔边的过渡，强度控制最大覆盖值。
//! 坐标始终是图像归一化坐标，因此任意预览/导出分辨率下形状一致。

use rayon::prelude::*;

use crate::image_engine::buffer::Image16;
use crate::image_engine::params::{
    MaskPoint, SubjectMaskParams, SubjectMaskStroke, SubjectMaskStrokeKind,
};

use super::alpha;
use super::{refinement, SubjectMask};

pub fn build_manual_mask(image: &Image16, params: &SubjectMaskParams) -> SubjectMask {
    let (width, height) = image.dimensions();
    let mut alpha = vec![0.0f32; width as usize * height as usize];
    // 普通笔触先叠加；前景/背景标记后执行，优先级更高，用于局部修正误判。
    for stroke in params
        .strokes
        .iter()
        .filter(|stroke| matches!(stroke.kind, SubjectMaskStrokeKind::Paint | SubjectMaskStrokeKind::Erase))
    {
        rasterize_stroke(&mut alpha, width, height, stroke);
    }
    for stroke in params
        .strokes
        .iter()
        .filter(|stroke| stroke.kind == SubjectMaskStrokeKind::Foreground)
    {
        rasterize_stroke(&mut alpha, width, height, stroke);
    }
    for stroke in params
        .strokes
        .iter()
        .filter(|stroke| stroke.kind == SubjectMaskStrokeKind::Background)
    {
        rasterize_stroke(&mut alpha, width, height, stroke);
    }
    let mut refined = refinement::refine_alpha(&alpha, width, height, params);
    for value in refined.iter_mut() {
        *value = value.clamp(0.0, 1.0);
    }
    let mask = SubjectMask {
        width,
        height,
        alpha: refined,
        source: params.source.clone(),
        model_version: params.model_version.clone(),
        confidence: 1.0,
    };
    if params.invert {
        mask.inverted()
    } else {
        mask
    }
}

fn rasterize_stroke(alpha: &mut [f32], width: u32, height: u32, stroke: &SubjectMaskStroke) {
    if stroke.points.is_empty() || stroke.strength <= 0.0 {
        return;
    }
    let w = width as f32;
    let h = height as f32;
    let radius_px = (stroke.radius * w).max(0.75);
    let strength = (stroke.strength / 100.0).clamp(0.0, 1.0);
    let hardness = (stroke.hardness / 100.0).clamp(0.0, 0.999);

    // 包围盒：笔触所有点的范围外扩一个半径。
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for point in &stroke.points {
        min_x = min_x.min(point.x * w);
        max_x = max_x.max(point.x * w);
        min_y = min_y.min(point.y * h);
        max_y = max_y.max(point.y * h);
    }
    let pad = radius_px + 2.0;
    let x0 = ((min_x - pad).floor().max(0.0)) as u32;
    let x1 = ((max_x + pad).ceil().min((width - 1) as f32)) as u32;
    let y0 = ((min_y - pad).floor().max(0.0)) as u32;
    let y1 = ((max_y + pad).ceil().min((height - 1) as f32)) as u32;
    if x0 > x1 || y0 > y1 {
        return;
    }

    let row_start = y0 as usize * width as usize;
    let row_end = (y1 as usize + 1) * width as usize;
    alpha[row_start..row_end]
        .par_chunks_mut(width as usize)
        .enumerate()
        .for_each(|(offset, row)| {
            let y = y0 as usize + offset;
            for (index, value) in row.iter_mut().enumerate() {
                // 行切片从列 0 开始，因此 x 直接使用行内索引，不能用包围盒偏移。
                let x = index;
                if x > x1 as usize || x < x0 as usize {
                    continue;
                }
                let distance = distance_to_polyline(
                    x as f32,
                    y as f32,
                    &stroke.points,
                    w,
                    h,
                );
                let normalized = distance / radius_px;
                if normalized >= 1.0 {
                    continue;
                }
                let weight = if normalized <= hardness {
                    1.0
                } else {
                    let t = ((1.0 - normalized) / (1.0 - hardness).max(0.0001)).clamp(0.0, 1.0);
                    alpha::smoothstep(t)
                } * strength;
                match stroke.kind {
                    SubjectMaskStrokeKind::Paint => *value = value.max(weight),
                    SubjectMaskStrokeKind::Erase => *value *= 1.0 - weight,
                    SubjectMaskStrokeKind::Foreground => *value = value.max(weight),
                    SubjectMaskStrokeKind::Background => *value = value.min(1.0 - weight),
                }
            }
        });
}

fn distance_to_polyline(
    px: f32,
    py: f32,
    points: &[MaskPoint],
    width: f32,
    height: f32,
) -> f32 {
    if points.len() == 1 {
        let dx = px - points[0].x * width;
        let dy = py - points[0].y * height;
        return (dx * dx + dy * dy).sqrt();
    }
    let mut best = f32::MAX;
    for pair in points.windows(2) {
        let ax = pair[0].x * width;
        let ay = pair[0].y * height;
        let bx = pair[1].x * width;
        let by = pair[1].y * height;
        best = best.min(distance_to_segment(px, py, ax, ay, bx, by));
    }
    best
}

fn distance_to_segment(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let vx = bx - ax;
    let vy = by - ay;
    let length_sq = vx * vx + vy * vy;
    if length_sq <= 0.0001 {
        let dx = px - ax;
        let dy = py - ay;
        return (dx * dx + dy * dy).sqrt();
    }
    let t = (((px - ax) * vx + (py - ay) * vy) / length_sq).clamp(0.0, 1.0);
    let cx = ax + vx * t;
    let cy = ay + vy * t;
    let dx = px - cx;
    let dy = py - cy;
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn params(stroke: SubjectMaskStroke) -> SubjectMaskParams {
        SubjectMaskParams {
            enabled: true,
            feather: 0.0,
            smooth: 0.0,
            expand: 0.0,
            hair_protect: 0.0,
            strokes: vec![stroke],
            ..Default::default()
        }
    }

    fn paint(x: f32, y: f32) -> SubjectMaskStroke {
        SubjectMaskStroke {
            kind: SubjectMaskStrokeKind::Paint,
            radius: 0.1,
            hardness: 100.0,
            strength: 100.0,
            points: vec![MaskPoint { x, y }],
        }
    }

    #[test]
    fn stroke_center_is_opaque_and_outside_is_transparent() {
        let image: Image16 = Image16::from_pixel(100, 100, Rgba([0, 0, 0, 65535]));
        let mask = build_manual_mask(&image, &params(paint(0.5, 0.5)));
        assert!(mask.alpha[50 * 100 + 50] > 0.99);
        assert_eq!(mask.alpha[0], 0.0);
        assert!(mask.coverage() > 0.0);
    }

    #[test]
    fn erase_stroke_removes_painted_area() {
        let image: Image16 = Image16::from_pixel(100, 100, Rgba([0, 0, 0, 65535]));
        let mut paint_stroke = paint(0.5, 0.5);
        paint_stroke.radius = 0.5;
        let erase = SubjectMaskStroke {
            kind: SubjectMaskStrokeKind::Erase,
            radius: 0.1,
            hardness: 100.0,
            strength: 100.0,
            points: vec![MaskPoint { x: 0.5, y: 0.5 }],
        };
        let mut settings = params(paint_stroke);
        settings.strokes.push(erase);
        let mask = build_manual_mask(&image, &settings);
        assert!(mask.alpha[50 * 100 + 50] < 0.01);
        assert!(mask.alpha[50 * 100 + 10] > 0.5);
    }

    #[test]
    fn line_stroke_fills_between_points() {
        let image: Image16 = Image16::from_pixel(100, 100, Rgba([0, 0, 0, 65535]));
        let stroke = SubjectMaskStroke {
            kind: SubjectMaskStrokeKind::Paint,
            radius: 0.05,
            hardness: 50.0,
            strength: 100.0,
            points: vec![
                MaskPoint { x: 0.2, y: 0.5 },
                MaskPoint { x: 0.8, y: 0.5 },
            ],
        };
        let mask = build_manual_mask(&image, &params(stroke));
        assert!(mask.alpha[50 * 100 + 50] > 0.9);
        assert_eq!(mask.alpha[10 * 100 + 50], 0.0);
    }

    #[test]
    fn invert_flips_alpha() {
        let image: Image16 = Image16::from_pixel(64, 64, Rgba([0, 0, 0, 65535]));
        let mut settings = params(paint(0.5, 0.5));
        settings.invert = true;
        let mask = build_manual_mask(&image, &settings);
        assert!(mask.alpha[32 * 64 + 32] < 0.01);
        assert!(mask.alpha[0] > 0.99);
    }
}
