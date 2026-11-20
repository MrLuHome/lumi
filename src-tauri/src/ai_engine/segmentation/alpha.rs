//! Alpha 基础运算：统计、可分离盒式模糊、阈值重映射、二值化。
//! 全部按 f32 参与计算，避免 8-bit 量化在多次羽化后累积阶梯。

/// 人物覆盖率：Alpha 的平均值，保留半透明边缘对覆盖率的贡献。
pub fn coverage(alpha: &[f32]) -> f32 {
    if alpha.is_empty() {
        return 0.0;
    }
    alpha.iter().map(|value| value.clamp(0.0, 1.0)).sum::<f32>() / alpha.len() as f32
}

/// 边缘带占比（0.05..0.95），用于判断蒙版是否需要继续羽化。
pub fn edge_ratio(alpha: &[f32], _width: u32, _height: u32) -> f32 {
    if alpha.is_empty() {
        return 0.0;
    }
    let edge = alpha
        .iter()
        .filter(|value| **value > 0.05 && **value < 0.95)
        .count() as f32;
    edge / alpha.len() as f32
}

/// 分离式盒式模糊（水平 + 垂直），执行 `passes` 次近似高斯。
/// 半径按像素给出，0 表示直接复制。
pub fn box_blur(values: &[f32], width: u32, height: u32, radius: u32, passes: u32) -> Vec<f32> {
    if radius == 0 || width == 0 || height == 0 || values.len() != width as usize * height as usize
    {
        return values.to_vec();
    }
    let mut current = values.to_vec();
    for _ in 0..passes.max(1) {
        current = blur_axis(&current, width, height, radius, true);
        current = blur_axis(&current, width, height, radius, false);
    }
    current
}

fn blur_axis(
    values: &[f32],
    width: u32,
    height: u32,
    radius: u32,
    horizontal: bool,
) -> Vec<f32> {
    let w = width as usize;
    let h = height as usize;
    let r = radius as isize;
    let mut output = vec![0.0f32; values.len()];
    let window = (radius * 2 + 1) as f32;

    if horizontal {
        for y in 0..h {
            let row = y * w;
            let mut sum = 0.0f32;
            for x in -r..=r {
                let index = x.clamp(0, w as isize - 1) as usize;
                sum += values[row + index];
            }
            for x in 0..w {
                output[row + x] = sum / window;
                let remove = (x as isize - r).clamp(0, w as isize - 1) as usize;
                let add = (x as isize + r + 1).clamp(0, w as isize - 1) as usize;
                sum += values[row + add] - values[row + remove];
            }
        }
    } else {
        for x in 0..w {
            let mut sum = 0.0f32;
            for y in -r..=r {
                let index = y.clamp(0, h as isize - 1) as usize;
                sum += values[index * w + x];
            }
            for y in 0..h {
                output[y * w + x] = sum / window;
                let remove = (y as isize - r).clamp(0, h as isize - 1) as usize;
                let add = (y as isize + r + 1).clamp(0, h as isize - 1) as usize;
                sum += values[add * w + x] - values[remove * w + x];
            }
        }
    }
    output
}

/// 阈值重映射：把模糊后的 Alpha 在 `threshold` 附近重新拉开，
/// 实现收缩/扩张，同时保留可调软边。
pub fn remap(values: &[f32], threshold: f32, softness: f32) -> Vec<f32> {
    let soft = softness.max(0.001);
    let low = (threshold - soft * 0.5).clamp(0.0, 1.0);
    let high = (threshold + soft * 0.5).clamp(0.0, 1.0);
    values
        .iter()
        .map(|value| {
            if *value <= low {
                0.0
            } else if *value >= high {
                1.0
            } else {
                smoothstep((value - low) / (high - low).max(0.0001))
            }
        })
        .collect()
}

pub fn binarize(values: &[f32], threshold: f32) -> Vec<f32> {
    values
        .iter()
        .map(|value| if *value >= threshold { 1.0 } else { 0.0 })
        .collect()
}

pub fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// 计算亮度（0..1），用于发丝细节保护。
pub fn luma_plane(image: &crate::image_engine::buffer::Image16) -> Vec<f32> {
    image
        .pixels()
        .map(|pixel| {
            let [r, g, b, _] = pixel.0;
            (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 65535.0
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_and_edge_ratio_are_consistent() {
        let alpha = vec![0.0, 0.0, 1.0, 1.0, 0.5, 0.5];
        assert_eq!(coverage(&alpha), 0.5);
        assert!(edge_ratio(&alpha, 3, 2) > 0.0);
        assert_eq!(edge_ratio(&vec![0.0, 1.0], 2, 1), 0.0);
    }

    #[test]
    fn blur_preserves_uniform_region() {
        let values = vec![0.7f32; 16 * 16];
        let blurred = box_blur(&values, 16, 16, 4, 2);
        for value in blurred {
            assert!((value - 0.7).abs() < 1e-4);
        }
    }

    #[test]
    fn blur_spreads_hard_edge() {
        let mut values = vec![0.0f32; 32];
        values[20] = 1.0;
        let blurred = box_blur(&values, 32, 1, 2, 1);
        assert!(blurred[19] > 0.0);
        assert!(blurred[20] < 1.0);
        assert_eq!(blurred[28], 0.0);
    }

    #[test]
    fn remap_threshold_expands_and_shrinks() {
        let values = vec![0.5f32; 4];
        let expanded = remap(&values, 0.35, 0.1);
        let shrunk = remap(&values, 0.65, 0.1);
        assert_eq!(expanded[0], 1.0);
        assert_eq!(shrunk[0], 0.0);
    }

    #[test]
    fn binarize_is_strict() {
        assert_eq!(binarize(&[0.49, 0.51], 0.5), vec![0.0, 1.0]);
    }
}
