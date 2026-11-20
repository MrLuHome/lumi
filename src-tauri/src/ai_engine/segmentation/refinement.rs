//! 蒙版后处理与前景去污染。
//!
//! 三个环节彼此独立、可单独测试：
//! 1. 平滑：抑制笔触抖动；
//! 2. 收缩/扩张 + 羽化：控制边缘位置与软硬；
//! 3. 去污染：替换背景时，把边缘像素里混入的旧背景颜色扣掉。

use crate::image_engine::buffer::Image16;
use crate::image_engine::params::SubjectMaskParams;

use super::alpha;

/// 按参数执行平滑 → 扩张/收缩 → 羽化。返回与输入同尺寸的 Alpha。
pub fn refine_alpha(values: &[f32], width: u32, height: u32, params: &SubjectMaskParams) -> Vec<f32> {
    let short = width.min(height) as f32;
    let mut current = values.to_vec();

    let smooth_radius = ((params.smooth / 100.0) * short * 0.012).round() as u32;
    if smooth_radius > 0 {
        current = alpha::box_blur(&current, width, height, smooth_radius, 2);
    }

    if params.expand.abs() > 0.5 {
        let blurred = alpha::box_blur(&current, width, height, (short * 0.006).max(1.0) as u32, 2);
        let threshold = 0.5 - (params.expand / 100.0) * 0.42;
        let softness = 0.08 + (params.smooth / 100.0) * 0.22;
        current = alpha::remap(&blurred, threshold, softness);
    }

    let feather_radius = ((params.feather / 100.0) * short * 0.02).round() as u32;
    if feather_radius > 0 {
        current = alpha::box_blur(&current, width, height, feather_radius, 2);
    }
    current
}

/// 前景去污染：边缘处观测颜色 = a·前景 + (1-a)·背景，
/// 已知背景估计时可反解前景，减少旧背景颜色在发丝/衣服边缘的残留。
///
/// `strength` 为 0..1，只有半透明边缘会被修改，内部与完全背景保持不变。
pub fn decontaminate_foreground(
    image: &mut Image16,
    alpha: &[f32],
    background: &Image16,
    strength: f32,
) {
    if strength <= 0.0 || image.dimensions() != background.dimensions() {
        return;
    }
    let width = image.width() as usize;
    let height = image.height() as usize;
    let strength = strength.clamp(0.0, 1.0);
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            let a = alpha.get(index).copied().unwrap_or(0.0);
            if !(0.02..0.98).contains(&a) {
                continue;
            }
            let edge_weight = 4.0 * a * (1.0 - a);
            let mix = (edge_weight * strength).clamp(0.0, 1.0);
            if mix <= 0.001 {
                continue;
            }
            let observed = image.get_pixel(x as u32, y as u32).0;
            let bg = background.get_pixel(x as u32, y as u32).0;
            let mut corrected = [0u16; 4];
            for channel in 0..3 {
                let obs = observed[channel] as f32 / 65535.0;
                let bg_value = bg[channel] as f32 / 65535.0;
                let estimated = ((obs - (1.0 - a) * bg_value) / a.max(0.001)).clamp(0.0, 1.0);
                let blended = obs * (1.0 - mix) + estimated * mix;
                corrected[channel] = (blended * 65535.0).round() as u16;
            }
            corrected[3] = observed[3];
            image.put_pixel(x as u32, y as u32, image::Rgba(corrected));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refine_keeps_uniform_alpha() {
        let alpha = vec![1.0f32; 32 * 32];
        let refined = refine_alpha(&alpha, 32, 32, &SubjectMaskParams::default());
        for value in refined {
            assert!((value - 1.0).abs() < 0.01);
        }
    }

    #[test]
    fn expand_grows_and_shrink_reduces() {
        let (width, height) = (64u32, 64u32);
        let mut alpha = vec![0.0f32; (width * height) as usize];
        for y in 24..40 {
            for x in 24..40 {
                alpha[y * width as usize + x] = 1.0;
            }
        }
        let grow = SubjectMaskParams {
            expand: 80.0,
            smooth: 0.0,
            feather: 0.0,
            ..Default::default()
        };
        let shrink = SubjectMaskParams {
            expand: -80.0,
            smooth: 0.0,
            feather: 0.0,
            ..Default::default()
        };
        let grown = refine_alpha(&alpha, width, height, &grow);
        let shrunk = refine_alpha(&alpha, width, height, &shrink);
        assert!(alpha::coverage(&grown) > alpha::coverage(&alpha));
        assert!(alpha::coverage(&shrunk) < alpha::coverage(&alpha));
    }

    #[test]
    fn decontamination_changes_only_soft_edges() {
        let mut image: Image16 = Image16::from_pixel(8, 8, image::Rgba([30000, 30000, 30000, 65535]));
        let background: Image16 =
            Image16::from_pixel(8, 8, image::Rgba([60000, 0, 0, 65535]));
        let interior_before = image.get_pixel(4, 4).0;
        let mut alpha = vec![0.0f32; 64];
        alpha[4 * 8 + 4] = 1.0;
        alpha[4 * 8 + 3] = 0.5;
        decontaminate_foreground(&mut image, &alpha, &background, 1.0);
        assert_eq!(image.get_pixel(4, 4).0, interior_before);
        assert_ne!(image.get_pixel(3, 4).0, [30000, 30000, 30000, 65535]);
    }
}
