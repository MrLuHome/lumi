//! 纯色 / 渐变 / 本地图片背景生成。
//!
//! 背景图片只从用户选择的本地路径读取，不复制、不上传、不写入项目缓存；
//! 项目文件只保存路径与参数，重开时重新读取。

use image::Rgba;

use crate::error::{AppError, AppResult};
use crate::image_engine::buffer::{self, Image16};
use crate::image_engine::params::{
    sanitize_hex_color, BackgroundFit, BackgroundGradient, BackgroundParams,
};

use super::blur;

pub fn hex_to_rgb(value: &str) -> [u8; 3] {
    let value = sanitize_hex_color(value, "#000000");
    let bytes = value.as_bytes();
    [
        u8::from_str_radix(&String::from_utf8_lossy(&bytes[1..3]), 16).unwrap_or(0),
        u8::from_str_radix(&String::from_utf8_lossy(&bytes[3..5]), 16).unwrap_or(0),
        u8::from_str_radix(&String::from_utf8_lossy(&bytes[5..7]), 16).unwrap_or(0),
    ]
}

fn rgb16(value: &str) -> [u16; 3] {
    let rgb = hex_to_rgb(value);
    [
        rgb[0] as u16 * 257,
        rgb[1] as u16 * 257,
        rgb[2] as u16 * 257,
    ]
}

pub fn solid(width: u32, height: u32, color: &str) -> Image16 {
    let [r, g, b] = rgb16(color);
    Image16::from_pixel(width.max(1), height.max(1), Rgba([r, g, b, 65535]))
}

pub fn gradient(width: u32, height: u32, gradient: &BackgroundGradient) -> Image16 {
    let width = width.max(1);
    let height = height.max(1);
    let angle = gradient.angle.to_radians();
    let (dx, dy) = (angle.cos(), angle.sin());
    let divisor = (dx.abs() + dy.abs()).max(0.0001);
    let stops: Vec<([f32; 3], f32)> = gradient
        .stops
        .iter()
        .map(|stop| {
            let [r, g, b] = hex_to_rgb(&stop.color);
            (
                [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0],
                (stop.position / 100.0).clamp(0.0, 1.0),
            )
        })
        .collect();
    let stops = if stops.is_empty() {
        vec![([0.0, 0.0, 0.0], 0.0), ([1.0, 1.0, 1.0], 1.0)]
    } else {
        stops
    };
    Image16::from_fn(width, height, |x, y| {
        let nx = x as f32 / width as f32 - 0.5;
        let ny = y as f32 / height as f32 - 0.5;
        let t = (0.5 + (dx * nx + dy * ny) / divisor).clamp(0.0, 1.0);
        let color = sample_stops(&stops, t);
        Rgba([
            (color[0] * 65535.0).round() as u16,
            (color[1] * 65535.0).round() as u16,
            (color[2] * 65535.0).round() as u16,
            65535,
        ])
    })
}

fn sample_stops(stops: &[([f32; 3], f32)], t: f32) -> [f32; 3] {
    if t <= stops[0].1 {
        return stops[0].0;
    }
    for pair in stops.windows(2) {
        let (low, low_pos) = pair[0];
        let (high, high_pos) = pair[1];
        if t <= high_pos {
            let span = (high_pos - low_pos).max(0.0001);
            let mut local = ((t - low_pos) / span).clamp(0.0, 1.0);
            local = local * local * (3.0 - 2.0 * local);
            return [
                low[0] * (1.0 - local) + high[0] * local,
                low[1] * (1.0 - local) + high[1] * local,
                low[2] * (1.0 - local) + high[2] * local,
            ];
        }
    }
    stops[stops.len() - 1].0
}

pub fn load_background(
    path: &str,
    params: &BackgroundParams,
    width: u32,
    height: u32,
) -> AppResult<Image16> {
    let path = path.trim();
    if path.is_empty() {
        return Err(AppError::Unsupported("未选择背景图片".into()));
    }
    let decoded = image::open(path)
        .map_err(|error| AppError::Decode(format!("无法读取背景图片 {path}：{error}")))?;
    let source = buffer::from_dynamic(&decoded);
    let (sw, sh) = source.dimensions();
    if sw == 0 || sh == 0 {
        return Err(AppError::Unsupported("背景图片尺寸无效".into()));
    }
    let scale = (params.image_scale / 100.0).clamp(0.5, 2.0);
    let fit_scale = match params.image_fit {
        BackgroundFit::Cover => (width as f32 / sw as f32).max(height as f32 / sh as f32),
        BackgroundFit::Contain => (width as f32 / sw as f32).min(height as f32 / sh as f32),
        BackgroundFit::Stretch => 1.0,
    } * scale;
    let target_w = ((sw as f32 * fit_scale).round() as u32).max(1);
    let target_h = ((sh as f32 * fit_scale).round() as u32).max(1);
    let fitted = if params.image_fit == BackgroundFit::Stretch {
        buffer::resize(
            &source,
            (width as f32 * scale).round() as u32,
            (height as f32 * scale).round() as u32,
        )
    } else {
        buffer::resize(&source, target_w, target_h)
    };
    let (fw, fh) = fitted.dimensions();
    let [r, g, b] = rgb16(&params.color);
    let mut canvas = Image16::from_pixel(width.max(1), height.max(1), Rgba([r, g, b, 65535]));
    let offset_x = (width as f32 - fw as f32) * 0.5 + params.image_offset_x / 100.0 * width as f32;
    let offset_y =
        (height as f32 - fh as f32) * 0.5 + params.image_offset_y / 100.0 * height as f32;
    image::imageops::overlay(
        &mut canvas,
        &fitted,
        offset_x.round() as i64,
        offset_y.round() as i64,
    );
    if params.image_blur > 0.0 {
        let radius = blur::radius_for(params.image_blur, width, height);
        canvas = blur::blur(&canvas, radius, 2);
    }
    Ok(canvas)
}

/// 背景颜色调整：亮度、色温、饱和度。只作用于背景平面。
pub fn color_adjust(image: &mut Image16, brightness: f32, temperature: f32, saturation: f32) {
    let brightness = brightness.clamp(-100.0, 100.0) / 100.0;
    let temperature = temperature.clamp(-100.0, 100.0) / 100.0;
    let saturation = 1.0 + saturation.clamp(-100.0, 100.0) / 100.0;
    if brightness == 0.0 && temperature == 0.0 && (saturation - 1.0).abs() < 0.001 {
        return;
    }
    for pixel in image.pixels_mut() {
        let mut rgb = [
            pixel.0[0] as f32 / 65535.0,
            pixel.0[1] as f32 / 65535.0,
            pixel.0[2] as f32 / 65535.0,
        ];
        rgb[0] = (rgb[0] + brightness * 0.5 + temperature * 0.08).clamp(0.0, 1.0);
        rgb[2] = (rgb[2] + brightness * 0.5 - temperature * 0.08).clamp(0.0, 1.0);
        rgb[1] = (rgb[1] + brightness * 0.5).clamp(0.0, 1.0);
        let luma = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
        for channel in rgb.iter_mut() {
            *channel = (luma + (*channel - luma) * saturation).clamp(0.0, 1.0);
        }
        pixel.0[0] = (rgb[0] * 65535.0).round() as u16;
        pixel.0[1] = (rgb[1] * 65535.0).round() as u16;
        pixel.0[2] = (rgb[2] * 65535.0).round() as u16;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_engine::params::GradientStop;

    #[test]
    fn parses_hex_colors_with_fallback() {
        assert_eq!(hex_to_rgb("#FF8000"), [255, 128, 0]);
        assert_eq!(hex_to_rgb("bad"), [0, 0, 0]);
        assert_eq!(hex_to_rgb("#00ff00"), [0, 255, 0]);
    }

    #[test]
    fn solid_background_has_requested_color() {
        let image = solid(8, 8, "#112233");
        assert_eq!(image.get_pixel(3, 3).0, [0x1111, 0x2222, 0x3333, 65535]);
    }

    #[test]
    fn gradient_varies_along_angle() {
        let spec = BackgroundGradient {
            angle: 0.0,
            stops: vec![
                GradientStop {
                    position: 0.0,
                    color: "#000000".into(),
                },
                GradientStop {
                    position: 100.0,
                    color: "#FFFFFF".into(),
                },
            ],
        };
        let image = gradient(64, 8, &spec);
        assert!(image.get_pixel(1, 4).0[0] < image.get_pixel(62, 4).0[0]);
    }

    #[test]
    fn color_adjust_moves_temperature_and_brightness() {
        let mut image: Image16 = Image16::from_pixel(4, 4, Rgba([30000, 30000, 30000, 65535]));
        color_adjust(&mut image, 30.0, 60.0, 0.0);
        let pixel = image.get_pixel(0, 0).0;
        assert!(pixel[0] > 30000);
        assert!(pixel[2] > 30000);
        assert!(pixel[0] > pixel[2]);
    }

    #[test]
    fn missing_background_file_returns_error() {
        let params = BackgroundParams::default();
        assert!(load_background("/nonexistent/lumi-bg.png", &params, 32, 32).is_err());
    }
}
