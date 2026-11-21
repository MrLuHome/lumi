//! 第五轮背景引擎（BACKGROUND ENGINE 1.0）。
//!
//! 只改变人物 Alpha 之外的区域：虚化、景深模拟、纯色、渐变、本地图片。
//! 所有背景模式共用同一条合成路径，预览与导出保持一致。

pub mod blur;
pub mod composite;
pub mod depth;
pub mod params;
pub mod replace;

use std::time::Instant;

use serde::Serialize;

use crate::error::{AppError, AppResult};
use crate::image_engine::buffer::Image16;
use crate::image_engine::cancel::CancelToken;
use crate::image_engine::params::{BackgroundMode, BackgroundParams};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundReport {
    pub mode: String,
    pub masked_pixels: u64,
    pub elapsed_ms: u128,
    pub degraded: bool,
    pub notes: Vec<String>,
}

pub fn apply_background(
    image: &mut Image16,
    alpha: &[f32],
    settings: &BackgroundParams,
    cancel: &CancelToken,
) -> AppResult<BackgroundReport> {
    let started = Instant::now();
    let width = image.width();
    let height = image.height();
    let notes = Vec::new();
    let masked_pixels = alpha.iter().filter(|value| **value > 0.5).count() as u64;
    if !settings.is_active() {
        return Ok(BackgroundReport {
            mode: params::mode_label(settings.mode).into(),
            masked_pixels,
            elapsed_ms: started.elapsed().as_millis(),
            degraded: false,
            notes,
        });
    }
    if alpha.len() != width as usize * height as usize {
        return Err(AppError::Unsupported(
            "人物蒙版尺寸与图像不一致，已取消背景处理".into(),
        ));
    }
    if masked_pixels == 0 {
        return Ok(BackgroundReport {
            mode: params::mode_label(settings.mode).into(),
            masked_pixels,
            elapsed_ms: started.elapsed().as_millis(),
            degraded: false,
            notes: vec!["没有可用的人物蒙版，背景处理已跳过".into()],
        });
    }

    cancel.check()?;
    let background = match settings.mode {
        BackgroundMode::Original => image.clone(),
        BackgroundMode::Blur => {
            let radius = blur::radius_for(settings.blur, width, height);
            let strong = blur::blur(image, radius, 3);
            if settings.depth > 0.0 {
                let soft_radius = (radius as f32 * 0.35).round() as u32;
                let soft = blur::blur(image, soft_radius, 2);
                let weights = depth::weight_plane(width, height, alpha, settings.depth);
                blur::blend(&soft, &strong, &weights)
            } else {
                strong
            }
        }
        BackgroundMode::SolidColor => replace::solid(width, height, &settings.color),
        BackgroundMode::Gradient => replace::gradient(width, height, &settings.gradient),
        BackgroundMode::Image => {
            replace::load_background(&settings.image_path, settings, width, height)?
        }
    };
    cancel.check()?;
    let mut background = background;
    replace::color_adjust(
        &mut background,
        settings.brightness,
        settings.temperature,
        settings.saturation,
    );
    let composited = composite::composite(image, &background, alpha, settings.decontaminate);
    *image = composited;
    Ok(BackgroundReport {
        mode: params::mode_label(settings.mode).into(),
        masked_pixels,
        elapsed_ms: started.elapsed().as_millis(),
        degraded: false,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_engine::params::{BackgroundGradient, GradientStop};
    use image::Rgba;

    fn mask(width: u32, height: u32) -> Vec<f32> {
        let mut alpha = vec![0.0f32; (width * height) as usize];
        for y in 4..12u32 {
            for x in 4..12u32 {
                alpha[y as usize * width as usize + x as usize] = 1.0;
            }
        }
        alpha
    }

    #[test]
    fn solid_background_changes_only_background() {
        let width = 16;
        let height = 16;
        let mut image: Image16 =
            Image16::from_pixel(width, height, Rgba([10000, 12000, 14000, 65535]));
        let foreground_before = image.get_pixel(8, 8).0;
        let settings = BackgroundParams {
            enabled: true,
            mode: BackgroundMode::SolidColor,
            color: "#FF0000".into(),
            decontaminate: 0.0,
            ..Default::default()
        };
        let report = apply_background(
            &mut image,
            &mask(width, height),
            &settings,
            &CancelToken::default(),
        )
        .unwrap();
        assert_eq!(report.masked_pixels, 64);
        assert_eq!(image.get_pixel(8, 8).0, foreground_before);
        assert_eq!(image.get_pixel(0, 0).0[0], 0xFFFF);
        assert_eq!(image.get_pixel(0, 0).0[1], 0);
    }

    #[test]
    fn blur_mode_keeps_foreground_pixels() {
        let width = 120;
        let height = 120;
        let mut image: Image16 = Image16::from_fn(width, height, |x, _| {
            if x < 60 {
                Rgba([0, 0, 0, 65535])
            } else {
                Rgba([65535, 65535, 65535, 65535])
            }
        });
        let before = image.get_pixel(50, 50).0;
        let settings = BackgroundParams {
            enabled: true,
            mode: BackgroundMode::Blur,
            blur: 80.0,
            depth: 0.0,
            ..Default::default()
        };
        apply_background(
            &mut image,
            &mask(width, height),
            &settings,
            &CancelToken::default(),
        )
        .unwrap();
        assert_eq!(image.get_pixel(50, 50).0, before);
        let edge = image.get_pixel(64, 50).0[0];
        assert!(edge > 0 && edge < 65535, "edge={edge}");
    }

    #[test]
    fn transparent_mask_is_skipped_with_note() {
        let mut image: Image16 = Image16::from_pixel(8, 8, Rgba([1, 2, 3, 65535]));
        let before = image.get_pixel(0, 0).0;
        let settings = BackgroundParams {
            enabled: true,
            mode: BackgroundMode::SolidColor,
            ..Default::default()
        };
        let report = apply_background(
            &mut image,
            &vec![0.0; 64],
            &settings,
            &CancelToken::default(),
        )
        .unwrap();
        assert!(report.notes.iter().any(|note| note.contains("跳过")));
        assert_eq!(image.get_pixel(0, 0).0, before);
    }

    #[test]
    fn gradient_background_uses_stops() {
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
        let plane = replace::gradient(32, 8, &spec);
        assert!(plane.get_pixel(1, 4).0[0] < plane.get_pixel(30, 4).0[0]);
    }

    #[test]
    fn image_mode_without_path_is_inactive() {
        let settings = BackgroundParams {
            enabled: true,
            mode: BackgroundMode::Image,
            image_path: String::new(),
            ..Default::default()
        }
        .clone()
        .clamped_background();
        assert!(!settings.is_active());
    }

    trait ClampBackground {
        fn clamped_background(&self) -> BackgroundParams;
    }

    impl ClampBackground for BackgroundParams {
        fn clamped_background(&self) -> BackgroundParams {
            let params = crate::image_engine::params::EditParams {
                background: self.clone(),
                ..Default::default()
            }
            .clamped();
            params.background
        }
    }
}
