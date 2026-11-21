use crate::image_engine::buffer::{par_pixels_mut, Image16, CHANNEL_MAX};
use crate::image_engine::color::{luma, smoothstep};
use crate::image_engine::operations::blur;

pub fn unsharp_mask(image: &Image16, radius: u32, amount: f32) -> Image16 {
    if amount <= 0.0 || radius == 0 {
        return image.clone();
    }
    let blurred = blur::gaussian_blur(image, radius);
    let mut output = image.clone();
    par_pixels_mut(&mut output, |x, y, pixel| {
        let low = blurred.get_pixel(x, y).0;
        for channel in 0..3 {
            let original = pixel[channel] as f32;
            let detail = original - low[channel] as f32;
            pixel[channel] = (original + detail * amount).clamp(0.0, CHANNEL_MAX).round() as u16;
        }
    });
    output
}

// 磨皮已迁移到 `ai_engine::retouch`：必须带皮肤蒙版与关键点保护，
// 这里不再保留全图模糊实现。

fn hash_noise(x: u32, y: u32) -> f32 {
    let mut hash = x
        .wrapping_mul(374_761_393)
        .wrapping_add(y.wrapping_mul(668_265_263));
    hash = (hash ^ (hash >> 13)).wrapping_mul(1_274_126_177);
    let unit = ((hash ^ (hash >> 16)) & 0xFFFF) as f32 / 65_535.0;
    unit * 2.0 - 1.0
}

pub fn add_grain(image: &mut Image16, strength: f32) {
    let amplitude = strength.clamp(0.0, 1.0) * 0.055 * CHANNEL_MAX;
    par_pixels_mut(image, |x, y, pixel| {
        let noise = hash_noise(x, y) * amplitude;
        for channel in 0..3 {
            pixel[channel] = (pixel[channel] as f32 + noise)
                .clamp(0.0, CHANNEL_MAX)
                .round() as u16;
        }
    });
}

pub fn add_vignette(image: &mut Image16, strength: f32) {
    let strength = strength.clamp(0.0, 1.0);
    let width = image.width().max(1) as f32;
    let height = image.height().max(1) as f32;
    let center_x = width / 2.0;
    let center_y = height / 2.0;
    let max_distance = (center_x * center_x + center_y * center_y).sqrt().max(1.0);

    par_pixels_mut(image, |x, y, pixel| {
        let dx = x as f32 - center_x;
        let dy = y as f32 - center_y;
        let distance = (dx * dx + dy * dy).sqrt() / max_distance;
        let falloff = smoothstep(0.42, 1.0, distance);
        let factor = 1.0 - strength * 0.62 * falloff;
        for channel in 0..3 {
            pixel[channel] = (pixel[channel] as f32 * factor)
                .clamp(0.0, CHANNEL_MAX)
                .round() as u16;
        }
    });
}

/// 灰世界白平衡估计：忽略过暗/过亮像素，返回可映射到色温/色调的偏移。
pub fn estimate_white_balance(image: &Image16) -> (f32, f32) {
    let mut sum = [0f64; 3];
    let mut count = 0u64;
    let step = ((image.width() * image.height()) / 200_000).max(1);
    for (index, pixel) in image.pixels().enumerate() {
        if index as u32 % step != 0 {
            continue;
        }
        let rgb = [
            pixel.0[0] as f32 / CHANNEL_MAX,
            pixel.0[1] as f32 / CHANNEL_MAX,
            pixel.0[2] as f32 / CHANNEL_MAX,
        ];
        let light = luma(rgb[0], rgb[1], rgb[2]);
        if !(0.08..=0.92).contains(&light) {
            continue;
        }
        for channel in 0..3 {
            sum[channel] += rgb[channel] as f64;
        }
        count += 1;
    }
    if count == 0 {
        return (0.0, 0.0);
    }
    let mean = [
        sum[0] / count as f64,
        sum[1] / count as f64,
        sum[2] / count as f64,
    ];
    let green = mean[1].max(1e-6);
    // 红蓝相对绿通道的偏差 → 色温；红蓝均值相对绿 → 色调。
    let temperature = ((mean[0] - mean[2]) / (green * 0.32) * 100.0) as f32;
    let tint = (((mean[0] + mean[2]) * 0.5 - green) / (green * 0.24) * 100.0) as f32;
    (temperature.clamp(-100.0, 100.0), tint.clamp(-100.0, 100.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn flat(value: u16, size: u32) -> Image16 {
        Image16::from_pixel(size, size, Rgba([value, value, value, 65535]))
    }

    #[test]
    fn sharpening_increases_edge_contrast() {
        let size = 32u32;
        let mut image: Image16 =
            Image16::from_pixel(size, size, Rgba([20_000, 20_000, 20_000, 65535]));
        for y in 0..size {
            for x in size / 2..size {
                image.put_pixel(x, y, Rgba([40_000, 40_000, 40_000, 65535]));
            }
        }
        let sharpened = unsharp_mask(&image, 1, 1.2);
        let left = sharpened.get_pixel(size / 2 - 1, size / 2).0[0] as i32;
        let right = sharpened.get_pixel(size / 2, size / 2).0[0] as i32;
        assert!(right - left > 20_000);
    }

    #[test]
    fn grain_is_deterministic() {
        let mut first = flat(30_000, 16);
        let mut second = flat(30_000, 16);
        add_grain(&mut first, 0.5);
        add_grain(&mut second, 0.5);
        assert_eq!(first.into_raw(), second.into_raw());
    }

    #[test]
    fn vignette_darkens_corners() {
        let mut image = flat(40_000, 32);
        add_vignette(&mut image, 1.0);
        let center = image.get_pixel(16, 16).0[0];
        let corner = image.get_pixel(0, 0).0[0];
        assert!(corner < center);
    }

    #[test]
    fn white_balance_estimate_is_neutral_for_gray() {
        let image = flat(32_000, 32);
        let (temperature, tint) = estimate_white_balance(&image);
        assert!(temperature.abs() < 1.0);
        assert!(tint.abs() < 1.0);
    }
}
