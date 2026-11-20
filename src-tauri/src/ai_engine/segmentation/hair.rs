//! 发丝保护。
//!
//! 人物蒙版在头发边缘常常把细碎发丝整段切掉。这里在半透明边缘带内，
//! 用原图亮度的高频成分调制 Alpha：亮于局部平均的细丝略保留，
//! 暗于局部平均的区域略收缩。它不会凭空生成发丝，只是让已有细节参与边缘形状。

use crate::image_engine::buffer::Image16;

use super::alpha;

pub fn apply_hair_detail(alpha: &mut [f32], image: &Image16, strength: f32) {
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 || alpha.len() != width as usize * height as usize {
        return;
    }
    let gain = (strength / 100.0).clamp(0.0, 1.0) * 0.6;
    if gain <= 0.0 {
        return;
    }
    let luma = alpha::luma_plane(image);
    // 局部均值半径至少 3px，避免小图上半径退化为 1px 而检测不到高频细节。
    let radius = ((width.min(height) as f32) * 0.006).round().max(3.0) as u32;
    let blurred = alpha::box_blur(&luma, width, height, radius, 2);
    for index in 0..alpha.len() {
        let a = alpha[index];
        if !(0.03..0.97).contains(&a) {
            continue;
        }
        let detail = luma[index] - blurred[index];
        let edge_weight = 4.0 * a * (1.0 - a);
        alpha[index] = (a + detail * gain * edge_weight * 1.8).clamp(0.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hair_detail_preserves_opaque_and_transparent_regions() {
        let image: Image16 = Image16::from_pixel(32, 32, image::Rgba([40000, 40000, 40000, 65535]));
        let mut alpha = vec![0.0f32; 32 * 32];
        alpha[16 * 32 + 16] = 1.0;
        alpha[16 * 32 + 15] = 0.5;
        apply_hair_detail(&mut alpha, &image, 80.0);
        assert_eq!(alpha[16 * 32 + 16], 1.0);
        assert_eq!(alpha[0], 0.0);
        assert!((alpha[16 * 32 + 15] - 0.5).abs() < 0.001);
    }

    #[test]
    fn hair_detail_reacts_to_luma_difference() {
        let mut image: Image16 = Image16::from_pixel(32, 1, image::Rgba([10000, 10000, 10000, 65535]));
        for x in 10..16 {
            image.put_pixel(x, 0, image::Rgba([50000, 50000, 50000, 65535]));
        }
        let mut alpha = vec![0.5f32; 32];
        let before = alpha.clone();
        apply_hair_detail(&mut alpha, &image, 100.0);
        assert!(alpha.iter().zip(before.iter()).any(|(a, b)| (a - b).abs() > 0.01));
        assert!(alpha[13] > before[13]);
    }
}
