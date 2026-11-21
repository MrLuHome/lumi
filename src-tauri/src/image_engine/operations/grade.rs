use std::sync::Arc;

use crate::error::AppResult;
use crate::image_engine::buffer::{par_pixels_mut, Image16, CHANNEL_MAX};
use crate::image_engine::cancel::CancelToken;
use crate::image_engine::color::{luma, smoothstep, CurveSetLuts, Lut3D};
use crate::image_engine::params::{EditParams, HslChannel, HslParams};

struct HslPlan {
    base_hue: f32,
    channel: HslChannel,
}

/// 单次遍历完成：白平衡 → 曝光 → 明暗色阶 → HSL → 饱和度 → 曲线 → LUT。
/// 预览与导出都走这一个函数，参数相同则像素结果完全一致。
pub struct GradePlan {
    params: EditParams,
    curves: CurveSetLuts,
    curves_neutral: bool,
    lut: Option<Arc<Lut3D>>,
    hsl: Vec<HslPlan>,
}

impl GradePlan {
    pub fn build(params: &EditParams) -> AppResult<Self> {
        let lut = match &params.lut {
            Some(settings) if settings.strength > 0.0 => {
                Some(Lut3D::load(std::path::Path::new(&settings.path))?)
            }
            _ => None,
        };
        let hsl = HslParams::CHANNELS
            .iter()
            .filter_map(|(name, base_hue)| {
                let channel = match *name {
                    "red" => params.hsl.red,
                    "orange" => params.hsl.orange,
                    "yellow" => params.hsl.yellow,
                    "green" => params.hsl.green,
                    "aqua" => params.hsl.aqua,
                    "blue" => params.hsl.blue,
                    "purple" => params.hsl.purple,
                    "magenta" => params.hsl.magenta,
                    _ => return None,
                };
                if channel.hue == 0.0 && channel.saturation == 0.0 && channel.luminance == 0.0 {
                    return None;
                }
                Some(HslPlan {
                    base_hue: *base_hue,
                    channel,
                })
            })
            .collect();

        Ok(Self {
            params: params.clone(),
            curves: CurveSetLuts::build(&params.curves),
            curves_neutral: params.curves.is_neutral(),
            lut,
            hsl,
        })
    }

    pub fn apply(&self, image: &mut Image16, cancel: &CancelToken) -> AppResult<()> {
        let params = &self.params;
        let exposure_gain = 2f32.powf(params.exposure / 100.0 * 2.0);
        let contrast_slope = 1.0 + params.contrast / 100.0 * 0.75;
        let shadow_shift = params.shadows / 100.0 * 0.34;
        let highlight_shift = params.highlights / 100.0 * 0.34;
        let white_shift = params.whites / 100.0 * 0.22;
        let black_shift = params.blacks / 100.0 * 0.22;
        let warm = params.temperature / 100.0 * 0.16;
        let tint = params.tint / 100.0 * 0.10;
        let saturation = params.saturation / 100.0;
        let vibrance = params.vibrance / 100.0;
        let lut_strength = self
            .lut
            .as_ref()
            .map(|_| {
                params
                    .lut
                    .as_ref()
                    .map(|s| s.strength / 100.0)
                    .unwrap_or(0.0)
            })
            .unwrap_or(0.0);

        par_pixels_mut(image, |_, _, pixel| {
            if cancel.is_cancelled() {
                return;
            }

            let mut rgb = [
                pixel[0] as f32 / CHANNEL_MAX,
                pixel[1] as f32 / CHANNEL_MAX,
                pixel[2] as f32 / CHANNEL_MAX,
            ];

            // 白平衡：色温推红/蓝，色调推品红/绿。
            if warm != 0.0 || tint != 0.0 {
                rgb[0] *= 1.0 + warm + tint * 0.35;
                rgb[1] *= 1.0 - tint;
                rgb[2] *= 1.0 - warm + tint * 0.35;
            }

            if exposure_gain != 1.0 {
                rgb[0] *= exposure_gain;
                rgb[1] *= exposure_gain;
                rgb[2] *= exposure_gain;
            }

            let light = luma(rgb[0], rgb[1], rgb[2]).clamp(0.0, 1.0);

            if shadow_shift != 0.0 {
                let mask = (1.0 - light).powi(2);
                for channel in rgb.iter_mut() {
                    *channel += shadow_shift * mask;
                }
            }
            if highlight_shift != 0.0 {
                let mask = light.powi(2);
                for channel in rgb.iter_mut() {
                    *channel += highlight_shift * mask;
                }
            }
            if white_shift != 0.0 {
                let mask = smoothstep(0.55, 1.0, light);
                for channel in rgb.iter_mut() {
                    *channel += white_shift * mask;
                }
            }
            if black_shift != 0.0 {
                let mask = 1.0 - smoothstep(0.0, 0.45, light);
                for channel in rgb.iter_mut() {
                    *channel += black_shift * mask;
                }
            }
            if contrast_slope != 1.0 {
                for channel in rgb.iter_mut() {
                    *channel = (*channel - 0.5) * contrast_slope + 0.5;
                }
            }

            if !self.hsl.is_empty() {
                rgb = apply_hsl(rgb, &self.hsl);
            }

            if saturation != 0.0 || vibrance != 0.0 {
                let current_light = luma(rgb[0], rgb[1], rgb[2]);
                let max_channel = rgb[0].max(rgb[1]).max(rgb[2]);
                let min_channel = rgb[0].min(rgb[1]).min(rgb[2]);
                let chroma = (max_channel - min_channel).clamp(0.0, 1.0);
                // 自然饱和度优先作用于低饱和区域，避免肤色被推成荧光色。
                let factor = (1.0 + saturation + vibrance * (1.0 - chroma)).max(0.0);
                for channel in rgb.iter_mut() {
                    *channel = current_light + (*channel - current_light) * factor;
                }
            }

            if !self.curves_neutral {
                rgb[0] = self.curves.red.apply(rgb[0].clamp(0.0, 1.0));
                rgb[1] = self.curves.green.apply(rgb[1].clamp(0.0, 1.0));
                rgb[2] = self.curves.blue.apply(rgb[2].clamp(0.0, 1.0));
                rgb[0] = self.curves.master.apply(rgb[0]);
                rgb[1] = self.curves.master.apply(rgb[1]);
                rgb[2] = self.curves.master.apply(rgb[2]);
            }

            if let Some(lut) = &self.lut {
                let graded = lut.sample(rgb[0], rgb[1], rgb[2]);
                for index in 0..3 {
                    rgb[index] = rgb[index] * (1.0 - lut_strength) + graded[index] * lut_strength;
                }
            }

            for index in 0..3 {
                pixel[index] = (rgb[index].clamp(0.0, 1.0) * CHANNEL_MAX).round() as u16;
            }
        });

        if cancel.is_cancelled() {
            return Err(crate::error::AppError::Cancelled);
        }
        Ok(())
    }
}

fn apply_hsl(rgb: [f32; 3], plans: &[HslPlan]) -> [f32; 3] {
    let (hue, saturation, lightness) =
        crate::image_engine::color::rgb_to_hsl(rgb[0], rgb[1], rgb[2]);
    if saturation < 0.005 {
        return rgb;
    }

    let mut total_weight = 0.0;
    let mut hue_shift = 0.0;
    let mut sat_shift = 0.0;
    let mut lum_shift = 0.0;
    for plan in plans {
        let distance = (hue - plan.base_hue).abs();
        let distance = distance.min(360.0 - distance);
        let weight = (1.0 - distance / 60.0).clamp(0.0, 1.0);
        if weight <= 0.0 {
            continue;
        }
        total_weight += weight;
        // -100..100 映射到 ±30°，避免单通道拖动产生断裂色带。
        hue_shift += weight * plan.channel.hue / 100.0 * 30.0;
        sat_shift += weight * plan.channel.saturation / 100.0;
        lum_shift += weight * plan.channel.luminance / 100.0;
    }
    if total_weight <= 0.0 {
        return rgb;
    }

    let normalized = total_weight.max(1.0);
    let new_hue = (hue + hue_shift / normalized).rem_euclid(360.0);
    let new_saturation = (saturation * (1.0 + sat_shift / normalized)).clamp(0.0, 1.0);
    let new_lightness = (lightness
        + lum_shift / normalized * 0.22 * (1.0 - (lightness - 0.5).abs()))
    .clamp(0.0, 1.0);
    let (r, g, b) = crate::image_engine::color::hsl_to_rgb(new_hue, new_saturation, new_lightness);
    [r, g, b]
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn flat(value: u16, size: u32) -> Image16 {
        Image16::from_pixel(size, size, Rgba([value, value, value, 65535]))
    }

    fn pixel(image: &Image16) -> [u16; 4] {
        image.get_pixel(image.width() / 2, image.height() / 2).0
    }

    #[test]
    fn neutral_params_are_pixel_identical() {
        let image = flat(30_000, 8);
        let plan = GradePlan::build(&EditParams::default()).unwrap();
        let mut output = image.clone();
        plan.apply(&mut output, &CancelToken::default()).unwrap();
        assert_eq!(output.into_raw(), image.into_raw());
    }

    #[test]
    fn exposure_and_whites_raise_tones() {
        let image = flat(25_000, 8);
        let params = EditParams {
            exposure: 40.0,
            whites: 60.0,
            ..Default::default()
        };
        let mut output = image.clone();
        GradePlan::build(&params)
            .unwrap()
            .apply(&mut output, &CancelToken::default())
            .unwrap();
        assert!(pixel(&output)[0] > 30_000);
    }

    #[test]
    fn blacks_lower_shadows() {
        let image = flat(20_000, 8);
        let params = EditParams {
            blacks: -70.0,
            ..Default::default()
        };
        let mut output = image.clone();
        GradePlan::build(&params)
            .unwrap()
            .apply(&mut output, &CancelToken::default())
            .unwrap();
        assert!(pixel(&output)[0] < 18_500);
    }

    #[test]
    fn hsl_red_channel_shifts_hue() {
        let image: Image16 = Image16::from_pixel(8, 8, Rgba([60_000, 8_000, 8_000, 65535]));
        let mut params = EditParams::default();
        params.hsl.red.hue = 100.0;
        let mut output = image.clone();
        GradePlan::build(&params)
            .unwrap()
            .apply(&mut output, &CancelToken::default())
            .unwrap();
        let result = pixel(&output);
        assert!(
            result[1] > 20_000,
            "正方向偏移应把红色推向橙/黄色（绿通道升高）"
        );
    }

    #[test]
    fn curve_brightens_midtones() {
        use crate::image_engine::params::CurvePoint;
        let image = flat(32_000, 8);
        let mut params = EditParams::default();
        params.curves.rgb = vec![
            CurvePoint::new(0.0, 0.0),
            CurvePoint::new(0.5, 0.75),
            CurvePoint::new(1.0, 1.0),
        ];
        let mut output = image.clone();
        GradePlan::build(&params)
            .unwrap()
            .apply(&mut output, &CancelToken::default())
            .unwrap();
        assert!(pixel(&output)[0] > 40_000);
    }

    #[test]
    fn cancellation_stops_the_pass() {
        use std::sync::atomic::AtomicBool;
        use std::sync::Arc;
        let image = flat(32_000, 256);
        let flag = Arc::new(AtomicBool::new(true));
        let mut output = image.clone();
        let result = GradePlan::build(&EditParams::default())
            .unwrap()
            .apply(&mut output, &CancelToken::new(flag));
        assert!(result.is_err());
    }
}
