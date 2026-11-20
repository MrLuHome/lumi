//! 模拟景深。
//!
//! 这不是 3D 深度估计：没有深度模型可用，只用画面纵向位置与人物蒙版
//! 构造一个平滑权重，使背景不同区域的虚化程度略有差异。UI 用「景深模拟」标注。

/// 返回每个像素的远景模糊权重（0..1）。
/// 顶部略远、底部略近，人物边缘附近的背景权重保持不变。
pub fn weight_plane(width: u32, height: u32, alpha: &[f32], depth: f32) -> Vec<f32> {
    let strength = (depth / 100.0).clamp(0.0, 1.0);
    let mut weights = vec![0.0f32; width as usize * height as usize];
    if strength <= 0.0 {
        return weights;
    }
    for y in 0..height as usize {
        let vertical = 1.0 - y as f32 / height.max(1) as f32;
        for x in 0..width as usize {
            let index = y * width as usize + x;
            let background = 1.0 - alpha.get(index).copied().unwrap_or(0.0).clamp(0.0, 1.0);
            weights[index] = strength * vertical * background;
        }
    }
    weights
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_depth_has_no_effect() {
        let weights = weight_plane(8, 8, &vec![0.0; 64], 0.0);
        assert!(weights.iter().all(|value| *value == 0.0));
    }

    #[test]
    fn top_background_blurs_more_than_bottom() {
        let weights = weight_plane(4, 10, &vec![0.0; 40], 100.0);
        assert!(weights[0] > weights[9 * 4]);
        assert!(weights[0] <= 1.0);
    }

    #[test]
    fn subject_pixels_are_never_far() {
        let alpha = vec![1.0f32; 100];
        let weights = weight_plane(10, 10, &alpha, 100.0);
        assert!(weights.iter().all(|value| *value == 0.0));
    }
}
