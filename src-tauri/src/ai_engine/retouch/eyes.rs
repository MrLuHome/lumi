//! 黑眼圈、眼下细纹、额头细纹、法令纹。
//!
//! 全部基于关键点局部蒙版：
//! - 黑眼圈只处理眼睛下方的椭圆，眼睛与眉毛权重为 0；
//! - 细纹只做小半径边缘保护平滑，并以蒙版权重限制强度；
//! - 默认参数为 0，用户开启后才生效。

use crate::ai_engine::face_frame::FaceGeometry;
use crate::ai_engine::retouch::filter::luma;
use crate::ai_engine::retouch::lab::{chroma, lab_to_srgb, srgb_to_lab};
use crate::ai_engine::retouch::smoothing::line_smoothing;
use crate::ai_engine::skin_segmentation::SkinMask;
use crate::image_engine::buffer::{Image16, CHANNEL_MAX};
use crate::image_engine::cancel::CancelToken;
use crate::image_engine::params::BeautyParams;
use crate::error::AppResult;

pub fn treat_eye_area(
    image: &mut Image16,
    mask: &SkinMask,
    geometry: &FaceGeometry,
    beauty: &BeautyParams,
    cancel: &CancelToken,
) -> AppResult<()> {
    if beauty.dark_circles == 0.0
        && beauty.eye_lines == 0.0
        && beauty.forehead_lines == 0.0
        && beauty.nasolabial == 0.0
    {
        return Ok(());
    }

    let width = mask.width;
    let height = mask.height;
    let count = (width * height) as usize;
    let eye_right = geometry.to_local(geometry.eyes[0].0, geometry.eyes[0].1);
    let eye_left = geometry.to_local(geometry.eyes[1].0, geometry.eyes[1].1);

    // 黑眼圈：先以脸颊亮度为目标做提亮，再轻微降低冷色偏。
    if beauty.dark_circles > 0.0 {
        let cheek_luma = cheek_reference_luma(image, mask, geometry, cancel)?;
        let amount = beauty.dark_circles / 100.0;
        for y in 0..height {
            if y % 64 == 0 {
                cancel.check()?;
            }
            for x in 0..width {
                let index = (y * width + x) as usize;
                if mask.alpha[index] <= 0.02 {
                    continue;
                }
                let (u, v) = geometry.to_local(
                    (mask.x0 + x as i32) as f32,
                    (mask.y0 + y as i32) as f32,
                );
                let weight = under_eye_weight(u, v, eye_right)
                    .max(under_eye_weight(u, v, eye_left))
                    * 0.85;
                if weight <= 0.02 {
                    continue;
                }
                let image_x = mask.x0 as u32 + x;
                let image_y = mask.y0 as u32 + y;
                if image_x >= image.width() || image_y >= image.height() {
                    continue;
                }
                let pixel = image.get_pixel(image_x, image_y).0;
                let r = pixel[0] as f32 / CHANNEL_MAX;
                let g = pixel[1] as f32 / CHANNEL_MAX;
                let b = pixel[2] as f32 / CHANNEL_MAX;
                let current = 0.299 * r + 0.587 * g + 0.114 * b;
                if current >= cheek_luma {
                    continue;
                }
                let mut lab = srgb_to_lab(r, g, b);
                let lift = (cheek_luma * 100.0 - lab[0]).max(0.0) * 0.45 * amount * weight;
                lab[0] += lift;
                lab[0] = lab[0].min(96.0);
                // 降低偏紫/偏蓝的色度，避免眼下发青。
                let original = chroma(lab[1], lab[2]);
                if original > 0.5 {
                    let scale = (1.0 - 0.18 * amount * weight).max(0.6);
                    lab[1] *= scale;
                    lab[2] *= scale;
                }
                let rgb = lab_to_srgb(lab[0], lab[1], lab[2]);
                let pixel = image.get_pixel_mut(image_x, image_y);
                for channel in 0..3 {
                    pixel.0[channel] = (rgb[channel] * CHANNEL_MAX).round() as u16;
                }
            }
        }
    }

    // 细纹：一次小半径平滑，权重按三种细纹的最大值合成。
    if beauty.eye_lines > 0.0 || beauty.forehead_lines > 0.0 || beauty.nasolabial > 0.0 {
        let mut plane = vec![0f32; count];
        let mut weights = vec![0f32; count];
        for y in 0..height {
            for x in 0..width {
                let index = (y * width + x) as usize;
                if mask.alpha[index] <= 0.05 {
                    continue;
                }
                let image_x = mask.x0 as u32 + x;
                let image_y = mask.y0 as u32 + y;
                if image_x >= image.width() || image_y >= image.height() {
                    continue;
                }
                plane[index] = luma(image.get_pixel(image_x, image_y).0);
                if plane[index] < 0.10 || plane[index] > 0.95 {
                    // 睫毛、高光不做平滑。
                    continue;
                }
                let (u, v) = geometry.to_local(image_x as f32, image_y as f32);
                let eye = eye_ring_weight(u, v, eye_right)
                    .max(eye_ring_weight(u, v, eye_left))
                    * (beauty.eye_lines / 100.0);
                let forehead = forehead_weight(u, v) * (beauty.forehead_lines / 100.0);
                let naso = nasolabial_weight(u, v) * (beauty.nasolabial / 100.0);
                weights[index] = eye.max(forehead).max(naso).clamp(0.0, 1.0);
            }
        }
        let smoothed = line_smoothing(&plane, width, height, &weights, 100.0, 2, cancel)?;
        for y in 0..height {
            for x in 0..width {
                let index = (y * width + x) as usize;
                if weights[index] <= 0.01 || plane[index] <= 0.001 {
                    continue;
                }
                let image_x = mask.x0 as u32 + x;
                let image_y = mask.y0 as u32 + y;
                let ratio = smoothed[index] / plane[index];
                let pixel = image.get_pixel_mut(image_x, image_y);
                for channel in 0..3 {
                    let value = pixel.0[channel] as f32 * ratio;
                    pixel.0[channel] = value.clamp(0.0, CHANNEL_MAX).round() as u16;
                }
            }
        }
    }

    Ok(())
}

pub(crate) fn under_eye_weight(u: f32, v: f32, eye: (f32, f32)) -> f32 {
    let distance = (((u - eye.0) / 0.34).powi(2) + ((v - eye.1 - 0.30) / 0.20).powi(2)).sqrt();
    let ellipse = 1.0 - smoothstep(0.65, 1.0, distance);
    // 避开眼球与眉毛。
    let eye_guard = smoothstep(0.85, 1.05, (((u - eye.0) / 0.32).powi(2)
        + ((v - eye.1) / 0.22).powi(2))
    .sqrt());
    let brow_guard = smoothstep(-0.10, -0.02, v - eye.1);
    ellipse * eye_guard * brow_guard
}

fn eye_ring_weight(u: f32, v: f32, eye: (f32, f32)) -> f32 {
    let distance = (((u - eye.0) / 0.34).powi(2) + ((v - eye.1) / 0.24).powi(2)).sqrt();
    let band = smoothstep(1.02, 1.22, distance) * (1.0 - smoothstep(1.55, 2.05, distance));
    let below = smoothstep(-0.05, 0.05, v - eye.1);
    band * below
}

pub(crate) fn forehead_weight(u: f32, v: f32) -> f32 {
    let vertical = smoothstep(-1.15, -1.00, v) * (1.0 - smoothstep(-0.45, -0.30, v));
    let horizontal = 1.0 - smoothstep(0.42, 0.62, u.abs());
    vertical * horizontal
}

pub(crate) fn nasolabial_weight(u: f32, v: f32) -> f32 {
    let right = ((u - 0.52) / 0.18).powi(2) + ((v - 0.82) / 0.42).powi(2);
    let left = ((u + 0.52) / 0.18).powi(2) + ((v - 0.82) / 0.42).powi(2);
    let distance = right.min(left).sqrt();
    (1.0 - smoothstep(0.55, 1.0, distance)) * 0.8
}

fn cheek_reference_luma(
    image: &Image16,
    mask: &SkinMask,
    geometry: &FaceGeometry,
    cancel: &CancelToken,
) -> AppResult<f32> {
    let mut sum = 0f32;
    let mut count = 0f32;
    let step = 3u32;
    for y in (0..mask.height).step_by(step as usize) {
        if y % 96 == 0 {
            cancel.check()?;
        }
        for x in (0..mask.width).step_by(step as usize) {
            let index = (y * mask.width + x) as usize;
            if mask.alpha[index] <= 0.6 {
                continue;
            }
            let image_x = mask.x0 as u32 + x;
            let image_y = mask.y0 as u32 + y;
            if image_x >= image.width() || image_y >= image.height() {
                continue;
            }
            let (u, v) = geometry.to_local(image_x as f32, image_y as f32);
            if u.abs() > 0.30 && u.abs() < 0.75 && v > 0.20 && v < 0.75 {
                sum += luma(image.get_pixel(image_x, image_y).0);
                count += 1.0;
            }
        }
    }
    Ok(if count > 0.0 { sum / count } else { 0.6 })
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
