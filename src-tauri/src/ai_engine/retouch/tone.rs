//! 肤色均匀、自然美白、泛红与暗沉改善。
//!
//! 在 CIELAB 中调整：亮度只做温和抬升并保护高光；色度向局部均值靠拢，
//! 调整后限制色度幅度在原始值的 35%..125%，避免灰白蜡黄或过度饱和。

use crate::ai_engine::retouch::filter::box_mean;
use crate::ai_engine::retouch::lab::{chroma, lab_to_srgb, srgb_to_lab};
use crate::ai_engine::skin_segmentation::SkinMask;
use crate::image_engine::buffer::{Image16, CHANNEL_MAX};
use crate::image_engine::cancel::CancelToken;
use crate::image_engine::params::BeautyParams;
use crate::error::AppResult;

pub fn even_skin_tone(
    image: &mut Image16,
    mask: &SkinMask,
    beauty: &BeautyParams,
    cancel: &CancelToken,
) -> AppResult<()> {
    if beauty.tone_even == 0.0
        && beauty.whitening == 0.0
        && beauty.redness == 0.0
        && beauty.dullness == 0.0
    {
        return Ok(());
    }
    let width = mask.width;
    let height = mask.height;
    let count = (width * height) as usize;
    let mut lightness = vec![0f32; count];
    let mut green_red = vec![0f32; count];
    let mut blue_yellow = vec![0f32; count];
    let mut active = vec![false; count];

    for y in 0..height {
        if y % 64 == 0 {
            cancel.check()?;
        }
        for x in 0..width {
            let index = (y * width + x) as usize;
            if mask.alpha[index] <= 0.02 {
                continue;
            }
            let image_x = mask.x0 as u32 + x;
            let image_y = mask.y0 as u32 + y;
            if image_x >= image.width() || image_y >= image.height() {
                continue;
            }
            let pixel = image.get_pixel(image_x, image_y).0;
            let lab = srgb_to_lab(
                pixel[0] as f32 / CHANNEL_MAX,
                pixel[1] as f32 / CHANNEL_MAX,
                pixel[2] as f32 / CHANNEL_MAX,
            );
            lightness[index] = lab[0];
            green_red[index] = lab[1];
            blue_yellow[index] = lab[2];
            active[index] = true;
        }
    }

    let local_radius = ((width as f32 * 0.03).round() as u32).clamp(4, 40);
    let local_a = box_mean(&green_red, width, height, local_radius);
    let local_b = box_mean(&blue_yellow, width, height, local_radius);

    let tone_even = beauty.tone_even / 100.0;
    let whitening = beauty.whitening / 100.0;
    let redness = beauty.redness / 100.0;
    let dullness = beauty.dullness / 100.0;

    for y in 0..height {
        if y % 64 == 0 {
            cancel.check()?;
        }
        for x in 0..width {
            let index = (y * width + x) as usize;
            if !active[index] {
                continue;
            }
            let alpha = mask.alpha[index];
            if alpha <= 0.02 {
                continue;
            }
            let mut l = lightness[index];
            let mut a = green_red[index];
            let mut b = blue_yellow[index];
            let original_chroma = chroma(a, b);
            let original_l = l;

            if tone_even > 0.0 {
                a += (local_a[index] - a) * 0.55 * tone_even;
                b += (local_b[index] - b) * 0.55 * tone_even;
            }
            if redness > 0.0 {
                // 只压多余的偏红，保留自然血色。
                a -= (a - 12.0).max(0.0) * 0.55 * redness;
            }
            if dullness > 0.0 {
                let shadow = 1.0 - smoothstep(35.0, 72.0, l);
                l += (62.0 - l).max(0.0) * 0.28 * dullness * shadow;
            }
            if whitening > 0.0 {
                let midtone = smoothstep(18.0, 60.0, l) * (1.0 - smoothstep(88.0, 96.0, l));
                l += 7.0 * whitening * midtone;
                b -= 1.6 * whitening;
            }

            // 高光保护：不让 L 冲到死白。
            if l > 96.0 {
                l = 96.0 + (l - 96.0) * 0.35;
            }
            // 色度约束：保留自然肤色特征。
            let new_chroma = chroma(a, b);
            if original_chroma > 0.5 && new_chroma > 0.001 {
                let lower = original_chroma * 0.35;
                let upper = original_chroma * 1.25;
                let target = new_chroma.clamp(lower, upper);
                let scale = target / new_chroma;
                a *= scale;
                b *= scale;
            }

            // 变化过大时按蒙版强度回退，防止低置信度区域被强行改色。
            let delta_l = (l - original_l).abs();
            let blend = alpha * (1.0 - smoothstep(18.0, 30.0, delta_l) * 0.5);
            l = original_l + (l - original_l) * blend;
            a = green_red[index] + (a - green_red[index]) * blend;
            b = blue_yellow[index] + (b - blue_yellow[index]) * blend;

            let rgb = lab_to_srgb(l, a, b);
            let image_x = mask.x0 as u32 + x;
            let image_y = mask.y0 as u32 + y;
            let pixel = image.get_pixel_mut(image_x, image_y);
            for channel in 0..3 {
                pixel.0[channel] = (rgb[channel] * CHANNEL_MAX).round() as u16;
            }
        }
    }
    Ok(())
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
