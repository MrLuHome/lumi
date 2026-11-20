use crate::error::AppResult;
use crate::image_engine::buffer::Image16;
use crate::image_engine::cancel::CancelToken;
use crate::image_engine::operations::{detail, grade};
use crate::image_engine::params::EditParams;

pub type ProgressFn<'a> = &'a mut dyn FnMut(f32, &'static str);

/// 完整渲染管线：磨皮 → 白平衡/光影/HSL/曲线/LUT → 清晰度 → 锐化 → 颗粒 → 暗角。
///
/// 预览与导出共用本函数，唯一区别是输入缓冲区尺寸；因此同一组参数在
/// 任意分辨率下都走完全相同的算法路径。
pub fn render(image: &Image16, params: &EditParams, cancel: &CancelToken) -> AppResult<Image16> {
    render_with_progress(image, params, cancel, &mut |_, _| {})
}

pub fn render_with_progress(
    image: &Image16,
    params: &EditParams,
    cancel: &CancelToken,
    progress: ProgressFn<'_>,
) -> AppResult<Image16> {
    let params = params.clone().clamped();
    if params.is_neutral() {
        progress(1.0, "复用原图");
        return Ok(image.clone());
    }
    cancel.check()?;

    let mut output = image.clone();

    if params.skin_smooth > 0.0 {
        progress(0.08, "人像磨皮");
        output = detail::smooth_skin(&output, params.skin_smooth);
        cancel.check()?;
    }

    progress(0.24, "调色");
    let plan = grade::GradePlan::build(&params)?;
    plan.apply(&mut output, cancel)?;

    if params.clarity > 0.0 {
        progress(0.52, "清晰度");
        let radius = (output.width().min(output.height()) / 160).clamp(2, 18);
        output = detail::unsharp_mask(&output, radius, params.clarity / 100.0 * 0.6);
        cancel.check()?;
    }
    if params.sharpness > 0.0 {
        progress(0.68, "锐化");
        output = detail::unsharp_mask(&output, 1, params.sharpness / 100.0 * 1.5);
        cancel.check()?;
    }
    if params.grain > 0.0 {
        progress(0.82, "颗粒");
        detail::add_grain(&mut output, params.grain / 100.0);
        cancel.check()?;
    }
    if params.vignette > 0.0 {
        progress(0.9, "暗角");
        detail::add_vignette(&mut output, params.vignette / 100.0);
    }

    progress(0.96, "编码");
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn neutral_render_is_identity() {
        let image: Image16 = Image16::from_pixel(8, 8, Rgba([1000, 20000, 60000, 65535]));
        let rendered = render(&image, &EditParams::default(), &CancelToken::default()).unwrap();
        assert_eq!(rendered.into_raw(), image.into_raw());
    }

    #[test]
    fn pipeline_applies_multiple_stages_in_order() {
        let image: Image16 = Image16::from_pixel(16, 16, Rgba([30_000, 30_000, 30_000, 65535]));
        let params = EditParams {
            exposure: 25.0,
            saturation: 30.0,
            vignette: 20.0,
            ..Default::default()
        };
        let mut stages = Vec::new();
        let rendered = render_with_progress(&image, &params, &CancelToken::default(), &mut |value, label| {
            stages.push((value, label));
        })
        .unwrap();
        assert!(rendered.get_pixel(8, 8).0[0] > 30_000);
        assert!(stages.len() >= 3);
    }

    #[test]
    fn missing_lut_returns_clear_error() {
        use crate::image_engine::params::LutSettings;
        let image: Image16 = Image16::from_pixel(4, 4, Rgba([1, 2, 3, 65535]));
        let params = EditParams {
            lut: Some(LutSettings {
                path: "/nonexistent/lumi.cube".into(),
                name: "missing".into(),
                strength: 100.0,
            }),
            ..Default::default()
        };
        let error = render(&image, &params, &CancelToken::default()).unwrap_err();
        assert!(error.to_string().contains("lumi.cube") || error.to_string().contains("No such"));
    }
}
