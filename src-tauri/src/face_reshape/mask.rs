//! 笔刷蒙版核心：羽化权重与圆形笔刷栅格化。
//!
//! 手动推拉、保护、恢复共用同一套羽化曲线，保证笔刷边缘连续。
//! 坐标由调用方从归一化图像坐标换算成像素，屏幕缩放不影响结果。

/// 圆形笔刷权重：内圈为 1，外圈按 smoothstep 衰减到 0。
pub fn feather_weight(distance: f32, radius: f32, feather: f32) -> f32 {
    if radius <= 0.5 || distance >= radius {
        return 0.0;
    }
    let inner = radius * (1.0 - 0.85 * feather.clamp(0.0, 1.0));
    if distance <= inner {
        return 1.0;
    }
    let t = (radius - distance) / (radius - inner).max(1e-3);
    t * t * (3.0 - 2.0 * t)
}

/// 单次圆形落笔的覆盖权重，写入调用方提供的 ROI（宽度、高度、左上角）。
pub fn stamp_circle(
    target: &mut [f32],
    roi: (i32, i32, u32, u32),
    center: (f32, f32),
    radius: f32,
    feather: f32,
    strength: f32,
) -> u32 {
    let (x0, y0, width, height) = roi;
    let mut touched = 0;
    let start_x = (center.0 - radius).floor() as i32;
    let end_x = (center.0 + radius).ceil() as i32;
    let start_y = (center.1 - radius).floor() as i32;
    let end_y = (center.1 + radius).ceil() as i32;
    for y in start_y..=end_y {
        for x in start_x..=end_x {
            if x < x0 || y < y0 || x >= x0 + width as i32 || y >= y0 + height as i32 {
                continue;
            }
            let distance = ((x as f32 - center.0).powi(2) + (y as f32 - center.1).powi(2)).sqrt();
            let weight = feather_weight(distance, radius, feather) * strength;
            if weight <= 0.001 {
                continue;
            }
            let index = (y - y0) as usize * width as usize + (x - x0) as usize;
            if index < target.len() {
                target[index] = (target[index] + weight).min(1.0);
                touched += 1;
            }
        }
    }
    touched
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feather_falls_off_to_zero() {
        assert_eq!(feather_weight(20.0, 10.0, 0.5), 0.0);
        assert_eq!(feather_weight(0.0, 10.0, 0.5), 1.0);
        let edge = feather_weight(9.9, 10.0, 0.5);
        assert!(edge > 0.0 && edge < 0.2);
    }

    #[test]
    fn stamp_stays_inside_roi() {
        let mut mask = vec![0.0f32; 10 * 10];
        stamp_circle(&mut mask, (0, 0, 10, 10), (0.0, 0.0), 6.0, 1.0, 1.0);
        assert!(mask.iter().any(|value| *value > 0.5));
        assert!(!mask.iter().any(|value| *value > 1.0));
    }

    #[test]
    fn protect_is_clamped_to_one() {
        let mut mask = vec![0.0f32; 20 * 20];
        for _ in 0..4 {
            stamp_circle(&mut mask, (0, 0, 20, 20), (10.0, 10.0), 8.0, 0.0, 1.0);
        }
        assert!(mask.iter().all(|value| *value <= 1.0));
        assert!(mask[10 * 20 + 10] >= 0.99);
    }
}
