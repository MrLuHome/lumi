use rayon::prelude::*;

use crate::image_engine::buffer::Image16;

/// 可分离盒式模糊，16-bit 精度，O(w·h)。
pub fn box_blur(image: &Image16, radius: u32) -> Image16 {
    if radius == 0 {
        return image.clone();
    }
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return image.clone();
    }

    let width_usize = width as usize;
    let radius_usize = radius as usize;
    let mut horizontal = vec![0f32; width_usize * height as usize * 3];
    let source = image.as_raw();

    horizontal
        .par_chunks_mut(width_usize * 3)
        .enumerate()
        .for_each(|(y, row)| {
            let mut prefix = vec![[0f32; 3]; width_usize + 1];
            for x in 0..width_usize {
                let idx = (y * width_usize + x) * 4;
                let previous = prefix[x];
                prefix[x + 1] = [
                    previous[0] + source[idx] as f32,
                    previous[1] + source[idx + 1] as f32,
                    previous[2] + source[idx + 2] as f32,
                ];
            }
            for x in 0..width_usize {
                let low = x.saturating_sub(radius_usize);
                let high = (x + radius_usize).min(width_usize - 1);
                let count = (high - low + 1) as f32;
                let base = x * 3;
                // 16-bit 通道和可达 65535 × 宽，f32 整数精度足够。
                row[base] = (prefix[high + 1][0] - prefix[low][0]) / count;
                row[base + 1] = (prefix[high + 1][1] - prefix[low][1]) / count;
                row[base + 2] = (prefix[high + 1][2] - prefix[low][2]) / count;
            }
        });

    let mut output = image.clone();
    let mut accum = vec![[0f32; 3]; width_usize];
    let first_high = radius.min(height - 1);
    for y in 0..=first_high {
        for x in 0..width {
            let idx = ((y * width + x) * 3) as usize;
            accum[x as usize][0] += horizontal[idx];
            accum[x as usize][1] += horizontal[idx + 1];
            accum[x as usize][2] += horizontal[idx + 2];
        }
    }

    let raw: &mut [u16] = output.as_mut();
    for y in 0..height {
        let low = y.saturating_sub(radius);
        let high = (y + radius).min(height - 1);
        let count = (high - low + 1) as f32;
        let base_row = y as usize * width_usize * 4;
        for x in 0..width {
            let idx = base_row + (x as usize) * 4;
            let value = accum[x as usize];
            raw[idx] = (value[0] / count).round().clamp(0.0, 65535.0) as u16;
            raw[idx + 1] = (value[1] / count).round().clamp(0.0, 65535.0) as u16;
            raw[idx + 2] = (value[2] / count).round().clamp(0.0, 65535.0) as u16;
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

pub fn gaussian_blur(image: &Image16, radius: u32) -> Image16 {
    if radius == 0 {
        return image.clone();
    }
    box_blur(&box_blur(image, radius), radius)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn flat_image_stays_flat_in_sixteen_bit() {
        let image: Image16 = Image16::from_pixel(9, 7, Rgba([20000, 30000, 40000, 65535]));
        let blurred = gaussian_blur(&image, 2);
        for pixel in blurred.pixels() {
            assert_eq!(pixel.0, [20000, 30000, 40000, 65535]);
        }
    }

    #[test]
    fn blur_spreads_bright_pixel() {
        let mut image: Image16 = Image16::from_pixel(11, 11, Rgba([0, 0, 0, 65535]));
        image.put_pixel(5, 5, Rgba([65535, 65535, 65535, 65535]));
        let blurred = box_blur(&image, 2);
        let neighbor = blurred.get_pixel(6, 5).0;
        assert!(neighbor[0] > 0 && neighbor[0] < 65535);
    }
}
