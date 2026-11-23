//! 归一化坐标 ↔ 像素坐标映射。
//!
//! 禁止在调用方手写 `x * width`；统一走这里，避免 body / face / 手动三套方言。

use super::types::{Normalized, RegionBox};

/// 归一化 → 像素（像素中心对齐约定：返回连续坐标，采样时 +0.5 由内核决定）。
pub fn normalized_to_pixel(x: Normalized, y: Normalized, width: u32, height: u32) -> [f32; 2] {
    [
        x.clamp(0.0, 1.0) * width as f32,
        y.clamp(0.0, 1.0) * height as f32,
    ]
}

/// 像素 → 归一化。
pub fn pixel_to_normalized(x: f32, y: f32, width: u32, height: u32) -> [Normalized; 2] {
    if width == 0 || height == 0 {
        return [0.0, 0.0];
    }
    [
        (x / width as f32).clamp(0.0, 1.0),
        (y / height as f32).clamp(0.0, 1.0),
    ]
}

/// 区域盒 → 像素矩形 `(x0, y0, x1, y1)`，半开区间，已夹到图像内。
///
/// 浮点边界（如 0.1+0.2=0.3000…4）在 `ceil` 前减去微小 epsilon，
/// 避免本应贴齐的右/下边界多出 1 像素。
pub fn region_to_pixels(region: RegionBox, width: u32, height: u32) -> (i32, i32, i32, i32) {
    if region.is_empty() || width == 0 || height == 0 {
        return (0, 0, 0, 0);
    }
    const EPS: f32 = 1e-4;
    let x0 = (region.x * width as f32).floor().clamp(0.0, width as f32) as i32;
    let y0 = (region.y * height as f32).floor().clamp(0.0, height as f32) as i32;
    let x1 = ((region.x + region.width) * width as f32 - EPS)
        .ceil()
        .clamp(0.0, width as f32) as i32;
    let y1 = ((region.y + region.height) * height as f32 - EPS)
        .ceil()
        .clamp(0.0, height as f32) as i32;
    (x0, y0, x1.max(x0), y1.max(y0))
}

/// 短边长度（像素），用于半径归一化换算。
pub fn short_side(width: u32, height: u32) -> f32 {
    width.min(height) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_pixel_roundtrip_within_epsilon() {
        let (w, h) = (4000u32, 6000u32);
        for (x, y) in [(0.0, 0.0), (0.5, 0.5), (1.0, 1.0), (0.123, 0.987)] {
            let px = normalized_to_pixel(x, y, w, h);
            let back = pixel_to_normalized(px[0], px[1], w, h);
            assert!((back[0] - x).abs() < 1e-4, "x {x} -> {back:?}");
            assert!((back[1] - y).abs() < 1e-4, "y {y} -> {back:?}");
        }
    }

    #[test]
    fn region_pixels_clamped_to_image() {
        let region = RegionBox::new(0.9, 0.9, 0.5, 0.5);
        let (x0, y0, x1, y1) = region_to_pixels(region, 100, 80);
        assert_eq!(x0, 90);
        assert_eq!(y0, 72);
        assert!(x1 <= 100);
        assert!(y1 <= 80);
    }

    #[test]
    fn empty_region_is_zero_rect() {
        assert_eq!(
            region_to_pixels(RegionBox::new(0.2, 0.2, 0.0, 0.1), 100, 100),
            (0, 0, 0, 0)
        );
    }
}
