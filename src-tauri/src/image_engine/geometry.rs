use crate::image_engine::buffer::Image16;

/// 等比缩放到长边不超过 `max_edge`。
pub fn target_size(width: u32, height: u32, max_edge: u32) -> (u32, u32) {
    let longest = width.max(height);
    if longest <= max_edge || width == 0 || height == 0 {
        return (width, height);
    }
    let scale = max_edge as f32 / longest as f32;
    (
        ((width as f32 * scale).round() as u32).max(1),
        ((height as f32 * scale).round() as u32).max(1),
    )
}

pub fn fit_within(image: &Image16, max_edge: u32) -> Image16 {
    let (width, height) = image.dimensions();
    let (target_width, target_height) = target_size(width, height, max_edge);
    if (target_width, target_height) == (width, height) {
        image.clone()
    } else {
        crate::image_engine::buffer::resize(image, target_width, target_height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn portrait_keeps_aspect_ratio() {
        assert_eq!(target_size(2000, 4000, 1000), (500, 1000));
    }

    #[test]
    fn small_image_is_not_enlarged() {
        let image: Image16 = Image16::from_pixel(320, 200, Rgba([1, 2, 3, 65535]));
        assert_eq!(fit_within(&image, 1800).dimensions(), (320, 200));
    }
}
