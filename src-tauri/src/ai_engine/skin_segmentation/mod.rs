//! 皮肤区域蒙版。
//!
//! 本轮没有可确认商用许可的皮肤解析权重（详见 `model.rs` 与 ROUND3_AUDIT），
//! 因此实际生效的是可解释、可测试的几何方案：
//! 5 个关键点建立脸部坐标系 → 额头/双颊/鼻部/下巴/颈部候选区 →
//! 挖除眼睛、眉毛、鼻孔、嘴唇 → YCbCr 肤色似然降权 → 羽化。
//!
//! 该蒙版不是模型推理结果，报告中标注为 `landmark-geometry`，不冒充 AI 分割。

pub mod model;

use image::Rgba;

use crate::ai_engine::face_frame::{FaceFrame, FaceGeometry};
use crate::image_engine::buffer::{Image16, CHANNEL_MAX};
use crate::image_engine::color::smoothstep;
use crate::image_engine::params::{ManualMask, MaskStroke};
use crate::error::AppResult;

/// 局部蒙版，只保存人脸包围盒范围内的 alpha（0..1）。
#[derive(Debug, Clone)]
pub struct SkinMask {
    pub x0: i32,
    pub y0: i32,
    pub width: u32,
    pub height: u32,
    /// 与包围盒同尺寸的 alpha。
    pub alpha: Vec<f32>,
}

impl SkinMask {
    pub fn at(&self, x: u32, y: u32) -> f32 {
        let local_x = x as i32 - self.x0;
        let local_y = y as i32 - self.y0;
        if local_x < 0 || local_y < 0 || local_x >= self.width as i32 || local_y >= self.height as i32
        {
            return 0.0;
        }
        self.alpha[(local_y as u32 * self.width + local_x as u32) as usize]
    }

    pub fn iter(&self) -> impl Iterator<Item = (u32, u32, f32)> + '_ {
        self.alpha
            .iter()
            .enumerate()
            .map(move |(index, value)| {
                let local_x = index as u32 % self.width.max(1);
                let local_y = index as u32 / self.width.max(1);
                (self.x0 as u32 + local_x, self.y0 as u32 + local_y, *value)
            })
    }

    pub fn active_pixels(&self) -> usize {
        self.alpha.iter().filter(|value| **value > 0.02).count()
    }

    pub fn mean_alpha(&self) -> f32 {
        if self.alpha.is_empty() {
            0.0
        } else {
            self.alpha.iter().sum::<f32>() / self.alpha.len() as f32
        }
    }
}

/// 生成单张人脸的皮肤蒙版。`manual` 会把画笔/橡皮擦叠加到自动蒙版上。
pub fn build_skin_mask(
    image: &Image16,
    face: &FaceFrame,
    manual: &ManualMask,
) -> AppResult<SkinMask> {
    let geometry = FaceGeometry::from_face(face, image.width(), image.height())?;
    Ok(build_mask_from_geometry(image, &geometry, manual))
}

fn build_mask_from_geometry(
    image: &Image16,
    geometry: &FaceGeometry,
    manual: &ManualMask,
) -> SkinMask {
    let image_width = image.width().max(1);
    let image_height = image.height().max(1);
    let d = geometry.eye_distance;

    // 人脸包围盒：覆盖额头到颈部，并额外留出画笔范围。
    let corners = [
        geometry.from_local(-0.98, -1.62),
        geometry.from_local(0.98, -1.62),
        geometry.from_local(-0.98, 2.35),
        geometry.from_local(0.98, 2.35),
    ];
    let mut min_x = corners[0].0;
    let mut max_x = corners[0].0;
    let mut min_y = corners[0].1;
    let mut max_y = corners[0].1;
    for corner in corners.iter().skip(1) {
        min_x = min_x.min(corner.0);
        max_x = max_x.max(corner.0);
        min_y = min_y.min(corner.1);
        max_y = max_y.max(corner.1);
    }
    let margin = d * 0.35;
    let x0 = (min_x - margin).floor().max(0.0) as i32;
    let y0 = (min_y - margin).floor().max(0.0) as i32;
    let x1 = (max_x + margin).ceil().min(image_width as f32) as i32;
    let y1 = (max_y + margin).ceil().min(image_height as f32) as i32;
    let width = (x1 - x0).max(1) as u32;
    let height = (y1 - y0).max(1) as u32;

    let [eye_right, eye_left] = geometry.eyes;
    let eye_right_local = geometry.to_local(eye_right.0, eye_right.1);
    let eye_left_local = geometry.to_local(eye_left.0, eye_left.1);
    let nose_local = geometry.to_local(geometry.nose.0, geometry.nose.1);
    let mouth_center = (
        (geometry.mouth[0].0 + geometry.mouth[1].0) * 0.5,
        (geometry.mouth[0].1 + geometry.mouth[1].1) * 0.5,
    );
    let mouth_local = geometry.to_local(mouth_center.0, mouth_center.1);

    let mut alpha = vec![0f32; (width * height) as usize];
    for local_y in 0..height {
        for local_x in 0..width {
            let x = x0 as u32 + local_x;
            let y = y0 as u32 + local_y;
            let (u, v) = geometry.to_local(x as f32, y as f32);

            // 面部椭圆：覆盖额头、双颊、鼻、下巴。
            let radius = ((u / 0.92).powi(2) + (v / 1.58).powi(2)).sqrt();
            let face = 1.0 - smoothstep(0.80, 1.0, radius);

            // 颈部候选区：权重更低，靠肤色似然把关，避免误伤衣服。
            let neck = if v > 1.28 && v < 2.35 {
                let horizontal =
                    1.0 - smoothstep(0.42, 0.72, u.abs() / 0.72);
                let vertical = smoothstep(1.28, 1.58, v) * (1.0 - smoothstep(2.02, 2.35, v));
                horizontal * vertical * 0.55
            } else {
                0.0
            };
            let region = face.max(neck);
            if region <= 0.002 {
                continue;
            }

            // 五官保护：眼睛、眉毛、鼻孔、嘴唇必须接近 0。
            let eye_protect = ellipse(u, v, eye_right_local.0, eye_right_local.1, 0.30, 0.20)
                .max(ellipse(u, v, eye_left_local.0, eye_left_local.1, 0.30, 0.20));
            let brow_protect = ellipse(u, v, eye_right_local.0, -0.30, 0.36, 0.14)
                .max(ellipse(u, v, eye_left_local.0, -0.30, 0.36, 0.14));
            let mouth_protect = ellipse(u, v, mouth_local.0, mouth_local.1, 0.44, 0.24);
            let nostril_protect = ellipse(u, v, nose_local.0, nose_local.1, 0.17, 0.12);
            let protect = 1.0
                - eye_protect
                    .max(brow_protect)
                    .max(mouth_protect)
                    .max(nostril_protect);
            if protect <= 0.01 {
                continue;
            }

            let pixel = image.get_pixel(x, y).0;
            let chroma = skin_likelihood(pixel);
            let mut value = region * protect * chroma;
            // 高光裁切补偿：曝光过度时皮肤色度被压成纯白，色度似然会整片归零。
            // 这里只用几何区域给出有上限的置信度（≤0.5），仍低于正常皮肤；
            // 且裁切区没有可处理的纹理，后续操作不会产生虚假细节。
            if face > 0.5 && v < 1.05 {
                let clipped = smoothstep(0.93, 0.985, pixel_luma(pixel));
                value = value.max(face * protect * 0.5 * clipped);
            }
            alpha[(local_y * width + local_x) as usize] = value.clamp(0.0, 1.0);
        }
    }

    // 自动蒙版羽化：半径随人脸大小变化，避免硬边。
    let feather = (d * 0.020).round().clamp(1.0, 24.0) as u32;
    box_blur_alpha(&mut alpha, width, height, feather);

    // 手动蒙版：画笔叠加、橡皮擦减去，再按羽化参数柔化。
    if manual.has_content() {
        let mut add = vec![0f32; alpha.len()];
        let mut erase = vec![0f32; alpha.len()];
        for stroke in manual.strokes.iter() {
            stamp_stroke(
                stroke,
                &mut add,
                &mut erase,
                x0,
                y0,
                width,
                height,
                image_width,
                image_height,
            );
        }
        let manual_feather = (manual.feather / 100.0 * d * 0.10).round().clamp(0.0, 32.0) as u32;
        if manual_feather > 0 {
            box_blur_alpha(&mut add, width, height, manual_feather);
            box_blur_alpha(&mut erase, width, height, manual_feather);
        }
        let strength = (manual.strength / 100.0).clamp(0.0, 1.0);
        for (index, value) in alpha.iter_mut().enumerate() {
            let added = add[index] * strength;
            let removed = erase[index] * strength;
            let combined = *value * (1.0 - removed) + added * (1.0 - *value);
            *value = combined.clamp(0.0, 1.0);
        }
    }

    if manual.invert {
        let protect = feature_protection(
            &geometry,
            eye_right_local,
            eye_left_local,
            nose_local,
            mouth_local,
            width,
            height,
            x0,
            y0,
        );
        for (index, value) in alpha.iter_mut().enumerate() {
            *value = (1.0 - *value) * protect[index];
        }
    }

    // 低置信度区域直接归零，避免弱蒙版产生“糊一层”的假效果。
    for value in alpha.iter_mut() {
        if *value < 0.12 {
            *value = 0.0;
        }
    }

    SkinMask {
        x0,
        y0,
        width,
        height,
        alpha,
    }
}

#[allow(clippy::too_many_arguments)]
fn feature_protection(
    geometry: &FaceGeometry,
    eye_right_local: (f32, f32),
    eye_left_local: (f32, f32),
    nose_local: (f32, f32),
    mouth_local: (f32, f32),
    width: u32,
    height: u32,
    x0: i32,
    y0: i32,
) -> Vec<f32> {
    let mut protect = vec![1f32; (width * height) as usize];
    for local_y in 0..height {
        for local_x in 0..width {
            let x = x0 as u32 + local_x;
            let y = y0 as u32 + local_y;
            let (u, v) = geometry.to_local(x as f32, y as f32);
            let eye = ellipse(u, v, eye_right_local.0, eye_right_local.1, 0.30, 0.20)
                .max(ellipse(u, v, eye_left_local.0, eye_left_local.1, 0.30, 0.20));
            let brow = ellipse(u, v, eye_right_local.0, -0.30, 0.36, 0.14)
                .max(ellipse(u, v, eye_left_local.0, -0.30, 0.36, 0.14));
            let mouth = ellipse(u, v, mouth_local.0, mouth_local.1, 0.44, 0.24);
            let nose = ellipse(u, v, nose_local.0, nose_local.1, 0.17, 0.12);
            protect[(local_y * width + local_x) as usize] =
                1.0 - eye.max(brow).max(mouth).max(nose);
        }
    }
    protect
}

fn ellipse(u: f32, v: f32, center_u: f32, center_v: f32, radius_u: f32, radius_v: f32) -> f32 {
    let distance = (((u - center_u) / radius_u).powi(2) + ((v - center_v) / radius_v).powi(2))
        .sqrt();
    1.0 - smoothstep(0.72, 1.0, distance)
}

fn pixel_luma(pixel: [u16; 4]) -> f32 {
    (0.299 * pixel[0] as f32 + 0.587 * pixel[1] as f32 + 0.114 * pixel[2] as f32) / CHANNEL_MAX
}

/// YCbCr 肤色似然：不依赖固定肤色值，用色度偏离 + 亮度门限适应不同肤色。
fn skin_likelihood(pixel: [u16; 4]) -> f32 {
    let r = pixel[0] as f32 / CHANNEL_MAX;
    let g = pixel[1] as f32 / CHANNEL_MAX;
    let b = pixel[2] as f32 / CHANNEL_MAX;
    let y = pixel_luma(pixel);
    let cb = -0.168_736 * r - 0.331_264 * g + 0.5 * b + 0.5;
    let cr = 0.5 * r - 0.418_688 * g - 0.081_312 * b + 0.5;
    // 暗部噪声与低对比会让肤色色度被压缩到接近中性，
    // 在低亮度区间适度放宽容差，避免暗光人像整片漏蒙版。
    let shadow = 1.0 - smoothstep(0.25, 0.55, y);
    let scale = 1.0 + 0.30 * shadow;
    let dcb = (cb - 0.402) / (0.125 * scale);
    let dcr = (cr - 0.600) / (0.095 * scale);
    let chroma = (-(dcb * dcb + dcr * dcr)).exp();
    let luma_gate = smoothstep(0.085, 0.20, y) * (1.0 - 0.7 * smoothstep(0.955, 0.995, y));
    (chroma * luma_gate).clamp(0.0, 1.0)
}

fn stamp_stroke(
    stroke: &MaskStroke,
    add: &mut [f32],
    erase: &mut [f32],
    x0: i32,
    y0: i32,
    width: u32,
    height: u32,
    image_width: u32,
    image_height: u32,
) {
    if stroke.points.is_empty() {
        return;
    }
    let radius = (stroke.radius * image_width as f32).max(1.0);
    let step = (radius * 0.35).max(1.0);
    let target = if stroke.erase { erase } else { add };

    let mut stamp = |point: (f32, f32)| {
        let center_x = point.0 * image_width as f32;
        let center_y = point.1 * image_height as f32;
        let low_x = ((center_x - radius).floor() as i32 - x0).max(0);
        let high_x = ((center_x + radius).ceil() as i32 - x0).min(width as i32 - 1);
        let low_y = ((center_y - radius).floor() as i32 - y0).max(0);
        let high_y = ((center_y + radius).ceil() as i32 - y0).min(height as i32 - 1);
        if high_x < low_x || high_y < low_y {
            return;
        }
        for local_y in low_y..=high_y {
            for local_x in low_x..=high_x {
                let px = x0 + local_x;
                let py = y0 + local_y;
                let distance = ((px as f32 - center_x).powi(2) + (py as f32 - center_y).powi(2))
                    .sqrt();
                let falloff = 1.0 - smoothstep(0.55, 1.0, distance / radius);
                if falloff <= 0.0 {
                    continue;
                }
                let index = (local_y as u32 * width + local_x as u32) as usize;
                target[index] = target[index].max(falloff);
            }
        }
    };

    if stroke.points.len() == 1 {
        let point = stroke.points[0];
        stamp((point.x, point.y));
        return;
    }
    for pair in stroke.points.windows(2) {
        let start = (pair[0].x, pair[0].y);
        let end = (pair[1].x, pair[1].y);
        let distance = (((end.0 - start.0) * image_width as f32).powi(2)
            + ((end.1 - start.1) * image_height as f32).powi(2))
        .sqrt();
        let count = (distance / step).ceil().max(1.0) as u32;
        for index in 0..=count {
            let t = index as f32 / count as f32;
            stamp((
                start.0 + (end.0 - start.0) * t,
                start.1 + (end.1 - start.1) * t,
            ));
        }
    }
}

fn box_blur_alpha(data: &mut [f32], width: u32, height: u32, radius: u32) {
    let w = width as usize;
    let h = height as usize;
    if radius == 0 || w == 0 || h == 0 {
        return;
    }
    let r = radius as usize;

    // 横向：前缀和滑动窗口。
    let mut temp = vec![0f32; data.len()];
    let mut prefix = vec![0f32; w + 1];
    for y in 0..h {
        let row = y * w;
        prefix[0] = 0.0;
        for x in 0..w {
            prefix[x + 1] = prefix[x] + data[row + x];
        }
        for x in 0..w {
            let low = x.saturating_sub(r);
            let high = (x + r).min(w - 1);
            temp[row + x] = (prefix[high + 1] - prefix[low]) / (high - low + 1) as f32;
        }
    }

    // 纵向。
    let mut prefix = vec![0f32; h + 1];
    for x in 0..w {
        prefix[0] = 0.0;
        for y in 0..h {
            prefix[y + 1] = prefix[y] + temp[y * w + x];
        }
        for y in 0..h {
            let low = y.saturating_sub(r);
            let high = (y + r).min(h - 1);
            data[y * w + x] = (prefix[high + 1] - prefix[low]) / (high - low + 1) as f32;
        }
    }
}

/// 多张脸重叠时按最近脸中心划分归属，避免同一像素被两套参数重复处理。
pub fn apply_ownership(masks: &mut [SkinMask], centers: &[(f32, f32)]) {
    if masks.len() <= 1 {
        return;
    }
    for (index, mask) in masks.iter_mut().enumerate() {
        let width = mask.width as usize;
        for position in 0..mask.alpha.len() {
            if mask.alpha[position] <= 0.0 {
                continue;
            }
            let x = mask.x0 + (position % width) as i32;
            let y = mask.y0 + (position / width) as i32;
            let mut nearest = index;
            let mut best = f32::MAX;
            for (other, center) in centers.iter().enumerate() {
                let distance = (x as f32 - center.0).powi(2) + (y as f32 - center.1).powi(2);
                if distance < best {
                    best = distance;
                    nearest = other;
                }
            }
            if nearest != index {
                mask.alpha[position] = 0.0;
            }
        }
    }
}

/// 生成用于“显示蒙版”的叠加图：原图压暗，蒙版区域显示为青绿色。
pub fn visualization(image: &Image16, masks: &[SkinMask], invert: bool) -> Image16 {
    let mut output = image.clone();
    for (x, y, alpha) in masks.iter().flat_map(|mask| mask.iter()) {
        if x >= output.width() || y >= output.height() {
            continue;
        }
        let value = if invert { 1.0 - alpha } else { alpha };
        if value <= 0.01 {
            continue;
        }
        let pixel = output.get_pixel(x, y).0;
        let dim = 0.25;
        let tint = [0.1, 0.85, 0.72];
        let mixed = [
            pixel[0] as f32 * (1.0 - value) * dim + tint[0] * value * CHANNEL_MAX,
            pixel[1] as f32 * (1.0 - value) * dim + tint[1] * value * CHANNEL_MAX,
            pixel[2] as f32 * (1.0 - value) * dim + tint[2] * value * CHANNEL_MAX,
        ];
        output.put_pixel(
            x,
            y,
            Rgba([
                mixed[0].clamp(0.0, CHANNEL_MAX) as u16,
                mixed[1].clamp(0.0, CHANNEL_MAX) as u16,
                mixed[2].clamp(0.0, CHANNEL_MAX) as u16,
                pixel[3],
            ]),
        );
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_engine::face_frame::{FaceFrame, NormPoint};
    use image::Rgba;

    fn skin_image(width: u32, height: u32) -> Image16 {
        Image16::from_pixel(width, height, Rgba([214 * 257, 170 * 257, 140 * 257, 65535]))
    }

    fn frame(width: u32, height: u32) -> FaceFrame {
        let d = width as f32 * 0.2;
        let center_x = width as f32 * 0.5;
        let center_y = height as f32 * 0.4;
        let to_norm = |x: f32, y: f32| NormPoint {
            x: x / width as f32,
            y: y / height as f32,
        };
        FaceFrame {
            id: 0,
            score: 0.95,
            x: (center_x - d * 1.6) / width as f32,
            y: (center_y - d * 1.4) / height as f32,
            width: d * 3.2 / width as f32,
            height: d * 3.6 / height as f32,
            landmarks: vec![
                to_norm(center_x - d * 0.5, center_y),
                to_norm(center_x + d * 0.5, center_y),
                to_norm(center_x, center_y + d * 0.55),
                to_norm(center_x - d * 0.35, center_y + d * 0.95),
                to_norm(center_x + d * 0.35, center_y + d * 0.95),
            ],
        }
    }

    #[test]
    fn mask_covers_skin_and_excludes_eyes_and_mouth() {
        let image = skin_image(400, 400);
        let face = frame(400, 400);
        let mask = build_skin_mask(&image, &face, &ManualMask::default()).unwrap();
        let geometry = FaceGeometry::from_face(&face, 400, 400).unwrap();

        let forehead = geometry.from_local(0.0, -0.85);
        let cheek = geometry.from_local(0.45, 0.35);
        assert!(
            mask.at(forehead.0 as u32, forehead.1 as u32) > 0.5,
            "额头必须在皮肤蒙版内"
        );
        assert!(
            mask.at(cheek.0 as u32, cheek.1 as u32) > 0.5,
            "脸颊必须在皮肤蒙版内"
        );

        for (eye_x, eye_y) in geometry.eyes {
            assert!(
                mask.at(eye_x as u32, eye_y as u32) < 0.05,
                "眼睛区域必须被排除"
            );
        }
        let mouth = geometry.from_local(0.0, 0.95);
        assert!(
            mask.at(mouth.0 as u32, mouth.1 as u32) < 0.05,
            "嘴唇区域必须被排除"
        );
    }

    #[test]
    fn shadowed_desaturated_skin_stays_in_mask() {
        // 暗光下人像的肤色色度被压缩：仍应得到中等以上置信度，
        // 而不是因为偏离标准肤色中心被整片丢弃。
        let image = Image16::from_pixel(400, 400, Rgba([100 * 257, 88 * 257, 82 * 257, 65535]));
        let face = frame(400, 400);
        let mask = build_skin_mask(&image, &face, &ManualMask::default()).unwrap();
        let geometry = FaceGeometry::from_face(&face, 400, 400).unwrap();

        let cheek = geometry.from_local(0.45, 0.35);
        assert!(
            mask.at(cheek.0 as u32, cheek.1 as u32) > 0.4,
            "暗光肤色应保留足够置信度"
        );
    }

    #[test]
    fn clipped_highlight_keeps_bounded_geometry_confidence() {
        // 过曝到纯白的皮肤没有色度信息：几何区域内保留有限置信度，
        // 但不得超过 0.5，且人脸以外的白色区域必须为 0。
        let image = Image16::from_pixel(400, 400, Rgba([65535, 65535, 65535, 65535]));
        let face = frame(400, 400);
        let mask = build_skin_mask(&image, &face, &ManualMask::default()).unwrap();
        let geometry = FaceGeometry::from_face(&face, 400, 400).unwrap();

        let forehead = geometry.from_local(0.0, -0.85);
        let value = mask.at(forehead.0 as u32, forehead.1 as u32);
        assert!(
            value > 0.3 && value <= 0.55,
            "裁切高光应保留有上限的几何置信度，实际 {value}"
        );
        assert!(mask.at(4, 4) < 0.05, "人脸外的白色背景不得进入蒙版");
    }

    #[test]
    fn mask_edges_are_feathered() {
        let image = skin_image(400, 400);
        let face = frame(400, 400);
        let mask = build_skin_mask(&image, &face, &ManualMask::default()).unwrap();
        let soft = mask
            .alpha
            .iter()
            .filter(|value| **value > 0.03 && **value < 0.9)
            .count();
        assert!(soft > 200, "羽化边缘应存在大量中间值，实际 {soft}");
    }

    #[test]
    fn manual_brush_adds_and_eraser_removes() {
        let image = skin_image(400, 400);
        let face = frame(400, 400);
        let geometry = FaceGeometry::from_face(&face, 400, 400).unwrap();
        let point = geometry.from_local(0.0, 0.2);

        let brush = ManualMask {
            enabled: true,
            invert: false,
            feather: 0.0,
            strength: 100.0,
            strokes: vec![MaskStroke {
                erase: false,
                radius: 0.05,
                points: vec![crate::image_engine::params::MaskPoint {
                    x: point.0 / 400.0,
                    y: point.1 / 400.0,
                }],
            }],
        };
        let painted = build_skin_mask(&image, &face, &brush).unwrap();
        assert!(painted.at(point.0 as u32, point.1 as u32) > 0.9);

        let mut eraser = brush.clone();
        eraser.strokes[0].erase = true;
        let erased = build_skin_mask(&image, &face, &eraser).unwrap();
        assert!(erased.at(point.0 as u32, point.1 as u32) < 0.1);
    }
}
