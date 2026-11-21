//! 变形结果与原始像素的混合。
//!
//! 统一在 16-bit 线性缓冲上按 alpha 混合，边缘 alpha 来自脸部椭圆蒙版，
//! 因此脸缘、发际线与背景不会出现硬切边。

/// 按 alpha（0..1）混合原图像素与变形像素。
pub fn mix_pixel(original: &[u16; 4], warped: &[u16; 4], alpha: f32) -> [u16; 4] {
    let alpha = alpha.clamp(0.0, 1.0);
    let mut output = [0u16; 4];
    for channel in 0..4 {
        output[channel] = (original[channel] as f32 * (1.0 - alpha)
            + warped[channel] as f32 * alpha)
            .round()
            .clamp(0.0, 65535.0) as u16;
    }
    output
}

/// 两层级混合：先把手动变形与网格变形混合，再与保护/恢复结果混合。
pub fn mix_layers(base: [u16; 4], layer: [u16; 4], alpha: f32) -> [u16; 4] {
    mix_pixel(&base, &layer, alpha)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_alpha_keeps_original() {
        let original = [1000, 2000, 3000, 65535];
        let warped = [9000, 8000, 7000, 65535];
        assert_eq!(mix_pixel(&original, &warped, 0.0), original);
    }

    #[test]
    fn full_alpha_takes_warped() {
        let original = [1000, 2000, 3000, 65535];
        let warped = [9000, 8000, 7000, 40000];
        assert_eq!(mix_pixel(&original, &warped, 1.0), warped);
    }

    #[test]
    fn half_alpha_is_linear() {
        let original = [0, 0, 0, 0];
        let warped = [10000, 20000, 30000, 40000];
        assert_eq!(
            mix_pixel(&original, &warped, 0.5),
            [5000, 10000, 15000, 20000]
        );
    }
}
