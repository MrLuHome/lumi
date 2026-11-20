//! Alpha 合成与前景边缘去污染。
//!
//! 合成公式：`out = fg·a + bg·(1-a)`。只有半透明边缘同时包含两层信息；
//! 去污染先估计前景真实颜色，再做合成，避免旧背景色在发丝与衣服边缘残留。

use crate::ai_engine::segmentation::refinement::decontaminate_foreground;
use crate::image_engine::buffer::Image16;

pub fn composite(
    foreground: &Image16,
    background: &Image16,
    alpha: &[f32],
    decontaminate: f32,
) -> Image16 {
    if foreground.dimensions() != background.dimensions() {
        return foreground.clone();
    }
    let width = foreground.width();
    let height = foreground.height();
    if alpha.len() != width as usize * height as usize {
        return foreground.clone();
    }
    let mut source = foreground.clone();
    let strength = (decontaminate / 100.0).clamp(0.0, 1.0);
    if strength > 0.0 {
        decontaminate_foreground(&mut source, alpha, background, strength);
    }
    Image16::from_fn(width, height, |x, y| {
        let index = y as usize * width as usize + x as usize;
        let a = alpha[index].clamp(0.0, 1.0);
        let fg = source.get_pixel(x, y).0;
        let bg = background.get_pixel(x, y).0;
        let mut out = [0u16; 4];
        for channel in 0..3 {
            out[channel] = (fg[channel] as f32 * a + bg[channel] as f32 * (1.0 - a))
                .round()
                .clamp(0.0, 65535.0) as u16;
        }
        out[3] = fg[3];
        image::Rgba(out)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn opaque_area_keeps_foreground_exactly() {
        let fg: Image16 = Image16::from_pixel(4, 4, Rgba([10000, 20000, 30000, 65535]));
        let bg: Image16 = Image16::from_pixel(4, 4, Rgba([60000, 60000, 60000, 65535]));
        let alpha = vec![1.0f32; 16];
        let out = composite(&fg, &bg, &alpha, 0.0);
        assert_eq!(out.get_pixel(2, 2).0, [10000, 20000, 30000, 65535]);
    }

    #[test]
    fn transparent_area_uses_background() {
        let fg: Image16 = Image16::from_pixel(4, 4, Rgba([10000, 20000, 30000, 65535]));
        let bg: Image16 = Image16::from_pixel(4, 4, Rgba([50000, 40000, 30000, 65535]));
        let alpha = vec![0.0f32; 16];
        let out = composite(&fg, &bg, &alpha, 0.0);
        assert_eq!(out.get_pixel(2, 2).0[0], 50000);
    }

    #[test]
    fn half_alpha_blends_evenly() {
        let fg: Image16 = Image16::from_pixel(2, 2, Rgba([0, 0, 0, 65535]));
        let bg: Image16 = Image16::from_pixel(2, 2, Rgba([10000, 10000, 10000, 65535]));
        let alpha = vec![0.5f32; 4];
        let out = composite(&fg, &bg, &alpha, 0.0);
        assert!((out.get_pixel(0, 0).0[0] as i32 - 5000).abs() <= 1);
    }

    #[test]
    fn mismatched_alpha_returns_foreground() {
        let fg: Image16 = Image16::from_pixel(2, 2, Rgba([1, 2, 3, 65535]));
        let bg: Image16 = Image16::from_pixel(2, 2, Rgba([9, 9, 9, 65535]));
        let out = composite(&fg, &bg, &[0.0], 0.0);
        assert_eq!(out.get_pixel(1, 1).0, [1, 2, 3, 65535]);
    }
}
