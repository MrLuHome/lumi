use crate::ai_engine::face_frame::{FaceFrame, FaceGeometry};
use crate::ai_engine::retouch::{apply_face_retouch, FaceRetouchReport};
use crate::ai_engine::skin_segmentation::{apply_ownership, build_skin_mask, SkinMask};
use crate::error::AppResult;
use crate::face_reshape::{self, FaceReshapeReport};
use crate::image_engine::buffer::Image16;
use crate::image_engine::cancel::CancelToken;
use crate::image_engine::operations::{detail, grade};
use crate::image_engine::params::{BeautyParams, EditParams};
use serde::Serialize;

pub type ProgressFn<'a> = &'a mut dyn FnMut(f32, &'static str);

/// 管线副产物：塑形质量报告与用户可见警告。预览/导出都使用它，
/// 前端据此显示“已自动降强度”等提示，不再静默输出异常结果。
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PipelineReport {
    pub reshape: Vec<FaceReshapeReport>,
    pub warnings: Vec<String>,
}

/// 完整渲染管线：
/// 皮肤蒙版 → 瑕疵 → 自然磨皮 → 肤色 → 眼周 → 白平衡/光影/HSL/曲线/LUT
/// → 清晰度 → 锐化 → 颗粒 → 暗角。
///
/// 预览与导出共用本函数，唯一区别是输入缓冲区尺寸；人脸结果使用归一化坐标，
/// 因此任意分辨率下都是同一套算法路径。没有检测结果时不会做任何皮肤处理。
pub fn render(
    image: &Image16,
    params: &EditParams,
    faces: &[FaceFrame],
    cancel: &CancelToken,
) -> AppResult<Image16> {
    render_with_progress(image, params, faces, cancel, &mut |_, _| {})
}

pub fn render_with_progress(
    image: &Image16,
    params: &EditParams,
    faces: &[FaceFrame],
    cancel: &CancelToken,
    progress: ProgressFn<'_>,
) -> AppResult<Image16> {
    render_with_report(image, params, faces, cancel, progress).map(|(image, _)| image)
}

/// 与 `render_with_progress` 相同，但额外返回塑形质量报告。
pub fn render_with_report(
    image: &Image16,
    params: &EditParams,
    faces: &[FaceFrame],
    cancel: &CancelToken,
    progress: ProgressFn<'_>,
) -> AppResult<(Image16, PipelineReport)> {
    let params = params.clone().clamped();
    let mut report = PipelineReport::default();
    if params.is_neutral() {
        progress(1.0, "复用原图");
        return Ok((image.clone(), report));
    }
    cancel.check()?;

    let mut output = image.clone();
    let wants_beauty = params.skin_smooth > 0.0
        || params.faces.iter().any(|face| face.enabled && face.beauty.is_active());

    if wants_beauty {
        if faces.is_empty() {
            // 不静默退化成全图磨皮：明确提示需要先检测人脸。
            progress(0.06, "未检测到人脸，已跳过美肤");
            report.warnings.push("未检测到人脸，已跳过美肤".into());
        } else {
            progress(0.06, "皮肤分割");
            apply_beauty(&mut output, &params, faces, cancel, &mut *progress)?;
        }
    }

    if params.has_reshape() {
        if faces.is_empty() {
            progress(0.38, "未检测到人脸，已跳过塑形");
            report.warnings.push("面部塑形需要先检测人脸".into());
        } else {
            progress(0.38, "面部塑形");
            let reports = face_reshape::apply_face_reshape(
                &mut output,
                faces,
                &params.reshape,
                cancel,
            )?;
            for entry in &reports {
                if entry.degraded {
                    report.warnings.push(format!(
                        "人脸 #{} 的塑形已自动降强度（{:.0}%），请检查参数",
                        entry.face_id,
                        entry.applied_scale * 100.0
                    ));
                }
                for note in &entry.notes {
                    if note.contains("置信度") || note.contains("安全范围") || note.contains("未改变") {
                        report.warnings.push(format!("人脸 #{}：{}", entry.face_id, note));
                    }
                }
            }
            report.reshape = reports;
        }
    }

    progress(0.42, "调色");
    let plan = grade::GradePlan::build(&params)?;
    plan.apply(&mut output, cancel)?;

    if params.clarity > 0.0 {
        progress(0.58, "清晰度");
        let radius = (output.width().min(output.height()) / 160).clamp(2, 18);
        output = detail::unsharp_mask(&output, radius, params.clarity / 100.0 * 0.6);
        cancel.check()?;
    }
    if params.sharpness > 0.0 {
        progress(0.70, "锐化");
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
    Ok((output, report))
}

fn apply_beauty(
    output: &mut Image16,
    params: &EditParams,
    faces: &[FaceFrame],
    cancel: &CancelToken,
    progress: ProgressFn<'_>,
) -> AppResult<Vec<FaceRetouchReport>> {
    struct Entry {
        mask: SkinMask,
        geometry: FaceGeometry,
        beauty: BeautyParams,
    }

    let mut entries = Vec::new();
    for face in faces {
        if !face.is_usable() {
            continue;
        }
        let beauty = params
            .face_beauty_match(&face.uuid, face.id)
            .filter(|entry| entry.enabled)
            .map(|entry| entry.beauty.clone())
            .unwrap_or_default();
        if !beauty.is_active() && params.skin_smooth <= 0.0 {
            continue;
        }
        let mask = match build_skin_mask(output, face, &params.manual_mask) {
            Ok(mask) => mask,
            Err(_) => continue,
        };
        let geometry = match FaceGeometry::from_face(face, output.width(), output.height()) {
            Ok(geometry) => geometry,
            Err(_) => continue,
        };
        entries.push(Entry {
            mask,
            geometry,
            beauty,
        });
    }

    if entries.is_empty() {
        progress(0.40, "无可用人脸");
        return Ok(Vec::new());
    }

    let centers: Vec<(f32, f32)> = entries
        .iter()
        .map(|entry| entry.geometry.center())
        .collect();
    let mut masks = Vec::with_capacity(entries.len());
    let mut usable = Vec::with_capacity(entries.len());
    for entry in entries {
        masks.push(entry.mask);
        usable.push((entry.geometry, entry.beauty));
    }
    apply_ownership(&mut masks, &centers);

    let mut reports = Vec::new();
    for (index, mask) in masks.iter().enumerate() {
        cancel.check()?;
        progress(
            0.08 + 0.30 * (index as f32 / masks.len().max(1) as f32),
            "自然磨皮",
        );
        let (geometry, beauty) = &usable[index];
        let report = apply_face_retouch(
            output,
            mask,
            geometry,
            beauty,
            params.skin_smooth,
            &params.repair_points,
            cancel,
        )?;
        reports.push(report);
    }
    Ok(reports)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn neutral_render_is_identity() {
        let image: Image16 = Image16::from_pixel(8, 8, Rgba([1000, 20000, 60000, 65535]));
        let rendered = render(&image, &EditParams::default(), &[], &CancelToken::default()).unwrap();
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
        let rendered = render_with_progress(
            &image,
            &params,
            &[],
            &CancelToken::default(),
            &mut |value, label| {
                stages.push((value, label));
            },
        )
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
        let error = render(&image, &params, &[], &CancelToken::default()).unwrap_err();
        assert!(error.to_string().contains("lumi.cube") || error.to_string().contains("No such"));
    }

    #[test]
    fn beauty_without_faces_does_not_touch_pixels() {
        use crate::image_engine::params::{BeautyParams, FaceBeauty};
        let image: Image16 = Image16::from_pixel(
            48,
            48,
            Rgba([30_000, 24_000, 20_000, 65535]),
        );
        let params = EditParams {
            faces: vec![FaceBeauty {
                face_id: 0,
                face_uuid: String::new(),
                enabled: true,
                beauty: BeautyParams {
                    skin_smooth: 100.0,
                    ..Default::default()
                },
            }],
            ..Default::default()
        };
        let mut stages = Vec::new();
        let rendered = render_with_progress(
            &image,
            &params,
            &[],
            &CancelToken::default(),
            &mut |_, label| stages.push(label),
        )
        .unwrap();
        assert_eq!(rendered.into_raw(), image.into_raw());
        assert!(stages.iter().any(|label| label.contains("未检测到人脸")));
    }
}
