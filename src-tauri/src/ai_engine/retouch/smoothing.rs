//! 自然皮肤平滑：多尺度自引导滤波 + 高频纹理回注 + 蒙版融合。
//!
//! 不使用单一高斯模糊：引导滤波在低反差皮肤区域趋于均值，在睫毛、唇线、
//! 痣等强边缘处保持梯度；高频细节按“纹理保护/细节恢复”比例回注，避免塑料感。

use crate::ai_engine::retouch::filter::{box_mean, guided_filter, RgbPlanes};
use crate::ai_engine::skin_segmentation::SkinMask;
use crate::error::AppResult;
use crate::image_engine::buffer::{Image16, CHANNEL_MAX};
use crate::image_engine::cancel::CancelToken;
use crate::image_engine::params::BeautyParams;

pub fn smooth_skin(
    image: &mut Image16,
    mask: &SkinMask,
    beauty: &BeautyParams,
    legacy_strength: f32,
    cancel: &CancelToken,
) -> AppResult<()> {
    let strength = beauty.skin_smooth.max(legacy_strength).clamp(0.0, 100.0) / 100.0;
    if strength <= 0.0001 || mask.alpha.is_empty() {
        return Ok(());
    }

    let width = mask.width;
    let height = mask.height;
    let radius = ((width as f32 * 0.012).round() as u32).clamp(2, 18);
    let texture = (beauty.texture / 100.0).clamp(0.0, 1.0);
    let detail_recovery = (beauty.detail / 100.0).clamp(0.0, 1.0);
    // 引导滤波的正则项：纹理保护越高，eps 越大，越把皮肤细节当作“边缘”保留。
    // 0.0015 已明显低于普通皮肤纹理方差，可产生可见平滑；纹理保护拉满时接近恒等。
    let eps = 0.0015 + 0.045 * (1.0 - texture);

    let planes = RgbPlanes::from_image(image, mask);
    let pixel_count = (width * height) as usize;
    let mut bases: [Vec<f32>; 3] = [
        vec![0f32; pixel_count],
        vec![0f32; pixel_count],
        vec![0f32; pixel_count],
    ];
    for channel in 0..3 {
        cancel.check()?;
        let small = guided_filter(&planes.planes[channel], width, height, radius, eps);
        let large = guided_filter(
            &planes.planes[channel],
            width,
            height,
            (radius as f32 * 2.2).round() as u32,
            eps * 1.8,
        );
        for index in 0..pixel_count {
            bases[channel][index] = (small[index] + large[index]) * 0.5;
        }
    }
    // 纹理保护越高，回注比例越大；细节恢复在此之上额外补高频。
    // 即使纹理保护拉满也保留 5% 的低频基底差异，避免完全平掉皮肤结构。
    let keep = (0.08 + 0.45 * texture + 0.22 * detail_recovery).clamp(0.08, 0.95);
    let extra = 0.35 * detail_recovery;
    let retention = (keep + extra).min(1.0);

    for y in 0..height {
        if y % 64 == 0 {
            cancel.check()?;
        }
        for x in 0..width {
            let index = (y * width + x) as usize;
            let alpha = mask.alpha[index];
            if alpha <= 0.01 {
                continue;
            }
            let mix = alpha * strength;
            let image_x = mask.x0 as u32 + x;
            let image_y = mask.y0 as u32 + y;
            if image_x >= image.width() || image_y >= image.height() {
                continue;
            }
            let pixel = image.get_pixel_mut(image_x, image_y);
            for channel in 0..3 {
                let source = planes.planes[channel][index];
                let base = bases[channel][index];
                let detail = source - base;
                let target = base + detail * retention;
                let value = source + (target - source) * mix;
                pixel.0[channel] = (value.clamp(0.0, 1.0) * CHANNEL_MAX).round() as u16;
            }
        }
    }
    Ok(())
}

/// 小半径局部平滑，用于眼下细纹、额头细纹、法令纹。只平滑亮度，保留色度。
pub fn line_smoothing(
    plane: &[f32],
    width: u32,
    height: u32,
    weights: &[f32],
    amount: f32,
    radius: u32,
    cancel: &CancelToken,
) -> AppResult<Vec<f32>> {
    if amount <= 0.0001 {
        return Ok(plane.to_vec());
    }
    let smoothed = guided_filter(plane, width, height, radius.max(1), 0.0012);
    let mut output = plane.to_vec();
    for y in 0..height {
        if y % 64 == 0 {
            cancel.check()?;
        }
        for x in 0..width {
            let index = (y * width + x) as usize;
            let weight = weights[index];
            if weight <= 0.01 {
                continue;
            }
            let base = smoothed[index];
            let detail = plane[index] - base;
            // 保留约一半高频，避免把皮肤纹理磨成塑料。
            let target = base + detail * 0.5;
            let mix = (amount / 100.0) * 0.55 * weight;
            output[index] = plane[index] + (target - plane[index]) * mix;
        }
    }
    Ok(output)
}

/// 估计局部皮肤均值亮度，供黑眼圈等操作做目标值。
pub fn mean_skin_luma(plane: &[f32], weights: &[f32]) -> f32 {
    let mut sum = 0f32;
    let mut count = 0f32;
    for (value, weight) in plane.iter().zip(weights.iter()) {
        if *weight > 0.6 {
            sum += value;
            count += 1.0;
        }
    }
    if count > 0.0 {
        sum / count
    } else {
        0.6
    }
}

/// 区域内高频能量（用于测试纹理保留率与皮肤分析）。
pub fn texture_energy(plane: &[f32], width: u32, height: u32, weights: &[f32]) -> f32 {
    if width == 0 || height == 0 {
        return 0.0;
    }
    let base = box_mean(plane, width, height, 3);
    let mut sum = 0f32;
    let mut count = 0f32;
    for index in 0..plane.len() {
        if weights[index] > 0.6 {
            sum += (plane[index] - base[index]).abs();
            count += 1.0;
        }
    }
    if count > 0.0 {
        sum / count
    } else {
        0.0
    }
}
