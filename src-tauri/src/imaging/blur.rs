use image::RgbaImage;
use rayon::prelude::*;

/// 可分离盒式模糊：先横向前缀和，再纵向滑动窗口，整体 O(w·h)。
///
/// 半径较小时连续做两次即可近似高斯模糊，足够支撑清晰度 / 磨皮这类
/// 局部对比操作，同时避免引入额外的高斯卷积开销。
pub fn box_blur(image: &RgbaImage, radius: u32) -> RgbaImage {
    if radius == 0 {
        return image.clone();
    }
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return image.clone();
    }

    let mut horizontal = vec![0f32; (width as usize) * (height as usize) * 3];
    horizontal
        .par_chunks_mut((width * 3) as usize)
        .enumerate()
        .for_each(|(y, row)| {
            let source = image.as_raw();
            let width = width as usize;
            let radius = radius as usize;
            let mut prefix = vec![[0f32; 3]; width + 1];
            for x in 0..width {
                let idx = (y * width + x) * 4;
                let previous = prefix[x];
                prefix[x + 1] = [
                    previous[0] + source[idx] as f32,
                    previous[1] + source[idx + 1] as f32,
                    previous[2] + source[idx + 2] as f32,
                ];
            }
            for x in 0..width {
                let low = x.saturating_sub(radius);
                let high = (x + radius).min(width - 1);
                let count = (high - low + 1) as f32;
                let left = prefix[low];
                let right = prefix[high + 1];
                let base = x * 3;
                row[base] = (right[0] - left[0]) / count;
                row[base + 1] = (right[1] - left[1]) / count;
                row[base + 2] = (right[2] - left[2]) / count;
            }
        });

    let mut output = image.clone();
    let mut accum = vec![[0f32; 3]; width as usize];
    let first_high = radius.min(height - 1);
    for y in 0..=first_high {
        for x in 0..width {
            let idx = ((y * width + x) * 3) as usize;
            accum[x as usize][0] += horizontal[idx];
            accum[x as usize][1] += horizontal[idx + 1];
            accum[x as usize][2] += horizontal[idx + 2];
        }
    }

    let stride = (width * 4) as usize;
    let raw: &mut [u8] = output.as_mut();
    for y in 0..height {
        let low = y.saturating_sub(radius);
        let high = (y + radius).min(height - 1);
        let count = (high - low + 1) as f32;
        let base_row = y as usize * stride;
        for x in 0..width {
            let idx = base_row + (x as usize) * 4;
            let value = accum[x as usize];
            raw[idx] = (value[0] / count).round().clamp(0.0, 255.0) as u8;
            raw[idx + 1] = (value[1] / count).round().clamp(0.0, 255.0) as u8;
            raw[idx + 2] = (value[2] / count).round().clamp(0.0, 255.0) as u8;
        }

        let add_row = y + radius + 1;
        if add_row < height {
            for x in 0..width {
                let idx = ((add_row * width + x) * 3) as usize;
                accum[x as usize][0] += horizontal[idx];
                accum[x as usize][1] += horizontal[idx + 1];
                accum[x as usize][2] += horizontal[idx + 2];
            }
        }
        if y >= radius {
            let remove_row = y - radius;
            for x in 0..width {
                let idx = ((remove_row * width + x) * 3) as usize;
                accum[x as usize][0] -= horizontal[idx];
                accum[x as usize][1] -= horizontal[idx + 1];
                accum[x as usize][2] -= horizontal[idx + 2];
            }
        }
    }

    output
}

/// 两次盒式模糊，近似高斯分布。
pub fn gaussian_blur(image: &RgbaImage, radius: u32) -> RgbaImage {
    if radius == 0 {
        return image.clone();
    }
    box_blur(&box_blur(image, radius), radius)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    #[test]
    fn zero_radius_is_identity() {
        let image = RgbaImage::from_pixel(4, 4, Rgba([12, 200, 30, 255]));
        assert_eq!(box_blur(&image, 0).into_raw(), image.into_raw());
    }

    #[test]
    fn flat_image_stays_flat() {
        let image = RgbaImage::from_pixel(9, 7, Rgba([80, 90, 100, 255]));
        let blurred = box_blur(&image, 3);
        assert_eq!(blurred.dimensions(), (9, 7));
        for pixel in blurred.pixels() {
            assert_eq!(pixel.0, [80, 90, 100, 255]);
        }
    }

    #[test]
    fn blur_spreads_bright_pixel_and_keeps_alpha() {
        let mut image = RgbaImage::from_pixel(11, 11, Rgba([0, 0, 0, 255]));
        image.put_pixel(5, 5, Rgba([255, 255, 255, 255]));
        let blurred = box_blur(&image, 2);
        let neighbor = blurred.get_pixel(6, 5).0;
        assert!(neighbor[0] > 0 && neighbor[0] < 255, "邻域应被提亮");
        assert_eq!(blurred.get_pixel(0, 0).0[3], 255);
    }

    #[test]
    fn uniform_region_survives_blur() {
        let mut image = RgbaImage::from_pixel(5, 5, Rgba([0, 0, 0, 255]));
        for y in 1..4 {
            for x in 0..5 {
                image.put_pixel(x, y, Rgba([100, 100, 100, 255]));
            }
        }
        // 单次盒式模糊：窗口完全落在均匀区内时结果必须精确保持。
        let blurred = box_blur(&image, 1);
        assert_eq!(blurred.get_pixel(2, 2).0, [100, 100, 100, 255]);
    }
}
