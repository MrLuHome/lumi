//! GPU 运算的数据结构与 CPU 参考实现。
//!
//! GPU 只替换“曝光/色彩矩阵”和“曲线/饱和度”两处；CPU 版本保持完全相同的
//! 数学定义，用于一致性测试与无 GPU 时的回退。

use rayon::prelude::*;

use crate::image_engine::buffer::Image16;

/// 线性/编码浮点 RGBA 图像（0..1），GPU 与 CPU 共用。
#[derive(Debug, Clone, PartialEq)]
pub struct RgbaF32Image {
    pub width: u32,
    pub height: u32,
    pub data: Vec<[f32; 4]>,
}

impl RgbaF32Image {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            data: vec![[0.0, 0.0, 0.0, 0.0]; width as usize * height as usize],
        }
    }

    pub fn from_image16(image: &Image16) -> Self {
        Self {
            width: image.width(),
            height: image.height(),
            data: image
                .pixels()
                .map(|pixel| {
                    [
                        pixel.0[0] as f32 / 65535.0,
                        pixel.0[1] as f32 / 65535.0,
                        pixel.0[2] as f32 / 65535.0,
                        pixel.0[3] as f32 / 65535.0,
                    ]
                })
                .collect(),
        }
    }

    pub fn to_image16(&self) -> Image16 {
        let mut out: Image16 = image::ImageBuffer::new(self.width, self.height);
        for (index, pixel) in self.data.iter().enumerate() {
            let x = index as u32 % self.width;
            let y = index as u32 / self.width;
            out.put_pixel(
                x,
                y,
                image::Rgba([
                    encode(pixel[0]),
                    encode(pixel[1]),
                    encode(pixel[2]),
                    encode(pixel[3]),
                ]),
            );
        }
        out
    }

    pub fn pixel_count(&self) -> usize {
        self.data.len()
    }
}

fn encode(value: f32) -> u16 {
    (value.clamp(0.0, 1.0) * 65535.0).round() as u16
}

pub fn srgb_to_linear(value: f32) -> f32 {
    let v = value.clamp(0.0, 1.0);
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

pub fn linear_to_srgb(value: f32) -> f32 {
    let v = value.clamp(0.0, 1.0);
    if v <= 0.0031308 {
        12.92 * v
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

/// CPU 曝光的精确参考实现。
pub fn cpu_exposure(
    image: &RgbaF32Image,
    exposure_ev: f32,
    matrix: Option<[[f32; 3]; 3]>,
) -> RgbaF32Image {
    let gain = 2.0f32.powf(exposure_ev);
    let data = image
        .data
        .par_iter()
        .map(|pixel| {
            let mut rgb = [
                srgb_to_linear(pixel[0]) * gain,
                srgb_to_linear(pixel[1]) * gain,
                srgb_to_linear(pixel[2]) * gain,
            ];
            if let Some(matrix) = matrix {
                rgb = [
                    matrix[0][0] * rgb[0] + matrix[0][1] * rgb[1] + matrix[0][2] * rgb[2],
                    matrix[1][0] * rgb[0] + matrix[1][1] * rgb[1] + matrix[1][2] * rgb[2],
                    matrix[2][0] * rgb[0] + matrix[2][1] * rgb[1] + matrix[2][2] * rgb[2],
                ];
            }
            [
                linear_to_srgb(rgb[0]),
                linear_to_srgb(rgb[1]),
                linear_to_srgb(rgb[2]),
                pixel[3],
            ]
        })
        .collect();
    RgbaF32Image {
        width: image.width,
        height: image.height,
        data,
    }
}

/// CPU 曲线/饱和度的精确参考实现（与 `curve.wgsl` 相同：最近邻 LUT）。
pub fn cpu_curve(image: &RgbaF32Image, lut: &[f32; 256], saturation: f32) -> RgbaF32Image {
    let data = image
        .data
        .par_iter()
        .map(|pixel| {
            let mut rgb = [0.0f32; 3];
            for channel in 0..3 {
                let index = (pixel[channel].clamp(0.0, 1.0) * 255.0).round() as usize;
                rgb[channel] = lut[index.min(255)];
            }
            let luma = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
            for value in rgb.iter_mut() {
                *value = (luma + (*value - luma) * (1.0 + saturation)).clamp(0.0, 1.0);
            }
            [rgb[0], rgb[1], rgb[2], pixel[3]]
        })
        .collect();
    RgbaF32Image {
        width: image.width,
        height: image.height,
        data,
    }
}

/// 生成一条身份曲线，便于 UI 与测试。
pub fn identity_lut() -> [f32; 256] {
    let mut lut = [0.0f32; 256];
    for (index, value) in lut.iter_mut().enumerate() {
        *value = index as f32 / 255.0;
    }
    lut
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_roundtrip_is_stable() {
        for value in [0.0, 0.02, 0.5, 0.9, 1.0] {
            let back = linear_to_srgb(srgb_to_linear(value));
            assert!((back - value).abs() < 1e-4, "{value} → {back}");
        }
    }

    #[test]
    fn exposure_matches_expected_gain() {
        let mut image = RgbaF32Image::new(1, 1);
        image.data[0] = [0.5, 0.5, 0.5, 1.0];
        let out = cpu_exposure(&image, 1.0, None);
        let expected = linear_to_srgb(srgb_to_linear(0.5) * 2.0);
        assert!((out.data[0][0] - expected).abs() < 1e-5);
        assert_eq!(out.data[0][3], 1.0);
    }

    #[test]
    fn identity_curve_is_noop() {
        let mut image = RgbaF32Image::new(2, 1);
        image.data[0] = [0.2, 0.6, 0.9, 1.0];
        image.data[1] = [1.0, 0.0, 0.5, 0.5];
        let out = cpu_curve(&image, &identity_lut(), 0.0);
        for (before, after) in image.data.iter().zip(out.data.iter()) {
            for channel in 0..3 {
                assert!((before[channel] - after[channel]).abs() < 0.01);
            }
        }
    }

    #[test]
    fn saturation_changes_color_but_keeps_gray() {
        let mut image = RgbaF32Image::new(2, 1);
        image.data[0] = [0.8, 0.2, 0.2, 1.0];
        image.data[1] = [0.5, 0.5, 0.5, 1.0];
        let out = cpu_curve(&image, &identity_lut(), 0.5);
        assert!(out.data[0][0] > image.data[0][0]);
        // LUT 为最近邻取样，灰度值会落在 128/255，允许一个量化步长误差。
        assert!((out.data[1][0] - 0.5).abs() < 1e-2);
    }
}
