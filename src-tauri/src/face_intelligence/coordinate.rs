//! 坐标转换。所有 Provider 必须在交付前转换到原图归一化坐标（左上角原点）。

use crate::face_intelligence::types::BoundingBox;

/// 像素坐标 → 原图归一化坐标。
pub fn from_pixels(x: f32, y: f32, width: u32, height: u32) -> (f32, f32) {
    let w = width.max(1) as f32;
    let h = height.max(1) as f32;
    (clamp_unit(x / w), clamp_unit(y / h))
}

/// 原图归一化坐标 → 像素坐标。
pub fn to_pixels(x: f32, y: f32, width: u32, height: u32) -> (f32, f32) {
    (
        (clamp_unit(x) * width.max(1) as f32).clamp(0.0, width.saturating_sub(1) as f32),
        (clamp_unit(y) * height.max(1) as f32).clamp(0.0, height.saturating_sub(1) as f32),
    )
}

/// 裁剪区域内的归一化坐标 → 原图归一化坐标。
///
/// 裁剪框在 `crop_rect` 中按整数像素落地，误差小于 1px，不回写归一化框。
pub fn crop_to_image(x: f32, y: f32, crop: BoundingBox) -> (f32, f32) {
    (
        clamp_unit(crop.x + clamp_unit(x) * crop.width),
        clamp_unit(crop.y + clamp_unit(y) * crop.height),
    )
}

/// 把裁剪框向外扩展并限制在 0..1，供 ONNX 模型输入使用。
pub fn expand_crop(bbox: BoundingBox, margin: f32) -> BoundingBox {
    let margin = margin.clamp(0.0, 0.75);
    let bbox = bbox.clamp_unit();
    let grow_x = bbox.width * margin;
    let grow_y = bbox.height * margin;
    let x = (bbox.x - grow_x).max(0.0);
    let y = (bbox.y - grow_y).max(0.0);
    let right = (bbox.x + bbox.width + grow_x).min(1.0);
    let bottom = (bbox.y + bbox.height + grow_y).min(1.0);
    BoundingBox {
        x,
        y,
        width: (right - x).max(0.0),
        height: (bottom - y).max(0.0),
    }
}

/// 把裁剪框转成原图像素矩形（x, y, width, height），至少 1px。
pub fn crop_rect(bbox: BoundingBox, image_width: u32, image_height: u32) -> (u32, u32, u32, u32) {
    let bbox = bbox.clamp_unit();
    let x = (bbox.x * image_width as f32).floor() as u32;
    let y = (bbox.y * image_height as f32).floor() as u32;
    let width = (bbox.width * image_width as f32).round().max(1.0) as u32;
    let height = (bbox.height * image_height as f32).round().max(1.0) as u32;
    let x = x.min(image_width.saturating_sub(1));
    let y = y.min(image_height.saturating_sub(1));
    (
        x,
        y,
        width.min(image_width - x).max(1),
        height.min(image_height - y).max(1),
    )
}

pub fn clamp_unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_and_unit_roundtrip() {
        let (x, y) = from_pixels(840.0, 1120.0, 1680, 2240);
        assert!((x - 0.5).abs() < 1e-6);
        assert!((y - 0.5).abs() < 1e-6);
        let (px, py) = to_pixels(x, y, 1680, 2240);
        assert!((px - 840.0).abs() < 0.01);
        assert!((py - 1120.0).abs() < 0.01);
    }

    #[test]
    fn crop_is_clamped_to_image() {
        let crop = expand_crop(
            BoundingBox {
                x: 0.05,
                y: 0.05,
                width: 0.2,
                height: 0.2,
            },
            0.5,
        );
        assert_eq!(crop.x, 0.0);
        assert_eq!(crop.y, 0.0);
        assert!(crop.x + crop.width <= 1.0);
        assert!(crop.y + crop.height <= 1.0);
    }

    #[test]
    fn crop_rect_never_leaves_image() {
        let (x, y, w, h) = crop_rect(
            BoundingBox {
                x: 0.95,
                y: 0.95,
                width: 0.2,
                height: 0.2,
            },
            100,
            100,
        );
        assert!(x + w <= 100);
        assert!(y + h <= 100);
        assert!(w >= 1 && h >= 1);
    }

    #[test]
    fn non_finite_values_become_zero() {
        // 非有限值不猜测位置：NaN 与 ±Infinity 都回落到 0，而不是贴到图像边缘。
        assert_eq!(clamp_unit(f32::NAN), 0.0);
        assert_eq!(clamp_unit(f32::INFINITY), 0.0);
        assert_eq!(clamp_unit(f32::NEG_INFINITY), 0.0);
    }
}
