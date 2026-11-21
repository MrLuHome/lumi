use std::time::Instant;

use crate::ai_engine::background::{apply_background, BackgroundReport};
use crate::ai_engine::body::{self, BodyReport, SubjectMasks};
use crate::ai_engine::face_frame::{FaceFrame, FaceGeometry};
use crate::ai_engine::retouch::{apply_face_retouch, FaceRetouchReport};
use crate::ai_engine::segmentation;
use crate::ai_engine::skin_segmentation::{apply_ownership, build_skin_mask, SkinMask};
use crate::error::AppResult;
use crate::face_reshape::{self, FaceReshapeReport};
use crate::image_engine::buffer::Image16;
use crate::image_engine::cancel::CancelToken;
use crate::image_engine::operations::{detail, grade};
use crate::image_engine::params::{BeautyParams, EditParams};
use serde::Serialize;

pub type ProgressFn<'a> = &'a mut dyn FnMut(f32, &'static str);

/// 手动蒙版报告。`source` 固定为 `manual`，不含任何 AI 模型输出。
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MaskReport {
    pub source: String,
    pub model_version: String,
    pub coverage: f32,
    pub edge_ratio: f32,
    pub strokes: usize,
    pub elapsed_ms: u128,
}

/// 管线副产物：塑形质量报告与用户可见警告。预览/导出都使用它，
/// 前端据此显示“已自动降强度”等提示，不再静默输出异常结果。
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PipelineReport {
    pub reshape: Vec<FaceReshapeReport>,
    /// 第五轮：每个身材对象的处理结果。
    pub body: Vec<BodyReport>,
    pub background: Option<BackgroundReport>,
    pub mask: Option<MaskReport>,
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

    // 第五轮：手动人物蒙版 → 身材 → 背景。全部都只改变人物蒙版内/外的对应区域，
    // 没有相关参数时不会走到这里，旧像素路径保持不变。
    let needs_subject_work =
        params.has_body() || params.has_background() || params.has_subject_mask();
    let mut subject_masks: Option<SubjectMasks> = None;
    if needs_subject_work {
        progress(0.02, "人物蒙版");
        let started = Instant::now();
        let global = segmentation::build_subject_mask(&output, &params.subject_mask);
        let mut masks = SubjectMasks::new(output.width(), output.height());
        masks.global = global.alpha.clone();
        for entry in params.body.iter().filter(|entry| entry.enabled) {
            if !entry.has_own_mask() {
                continue;
            }
            let mask = segmentation::build_mask_from_strokes(
                &output,
                &entry.mask_strokes,
                &params.subject_mask,
            );
            masks.subjects.push((entry.subject_id.clone(), mask.alpha));
        }
        let combined = masks.combined();
        let covered = combined.iter().filter(|value| **value > 0.5).count();
        report.mask = Some(MaskReport {
            source: global.source.clone(),
            model_version: global.model_version.clone(),
            coverage: if combined.is_empty() {
                0.0
            } else {
                covered as f32 / combined.len() as f32
            },
            edge_ratio: global.edge_ratio(),
            strokes: params.subject_mask.strokes.len()
                + params
                    .body
                    .iter()
                    .map(|entry| entry.mask_strokes.len())
                    .sum::<usize>(),
            elapsed_ms: started.elapsed().as_millis(),
        });
        subject_masks = Some(masks);
        cancel.check()?;
    }

    if params.has_body() {
        let active: Vec<_> = params
            .body
            .iter()
            .filter(|entry| entry.enabled && entry.is_active())
            .cloned()
            .collect();
        let masks =
            subject_masks.get_or_insert_with(|| SubjectMasks::new(output.width(), output.height()));
        progress(0.10, "身材调整");
        let body_reports = body::apply_body(&mut output, masks, &active, faces, cancel)?;
        for entry in &body_reports {
            for note in &entry.notes {
                report
                    .warnings
                    .push(format!("人物 {}：{}", entry.subject_id, note));
            }
            if entry.degraded {
                report.warnings.push(format!(
                    "人物 {} 的变形已自动降强度以保护图像",
                    entry.subject_id
                ));
            }
        }
        if body_reports.iter().all(|entry| entry.affected_pixels == 0) {
            report
                .warnings
                .push("未检测到可用的身材蒙版，身材调整未改变像素".into());
        }
        report.body = body_reports;
        cancel.check()?;
    }

    if params.has_background() {
        let combined = subject_masks
            .as_ref()
            .map(|masks| masks.combined())
            .unwrap_or_default();
        let has_subject = combined.iter().any(|value| *value > 0.5);
        if has_subject {
            progress(0.18, "背景处理");
            let background = apply_background(&mut output, &combined, &params.background, cancel)?;
            for note in &background.notes {
                report.warnings.push(note.clone());
            }
            report.background = Some(background);
        } else {
            progress(0.18, "背景处理已跳过");
            report
                .warnings
                .push("背景处理需要人物蒙版：请先用蒙版画笔标记人物，已跳过背景".into());
        }
        cancel.check()?;
    }

    let wants_beauty = params.skin_smooth > 0.0
        || params
            .faces
            .iter()
            .any(|face| face.enabled && face.beauty.is_active());

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
            let reports =
                face_reshape::apply_face_reshape(&mut output, faces, &params.reshape, cancel)?;
            for entry in &reports {
                if entry.degraded {
                    report.warnings.push(format!(
                        "人脸 #{} 的塑形已自动降强度（{:.0}%），请检查参数",
                        entry.face_id,
                        entry.applied_scale * 100.0
                    ));
                }
                for note in &entry.notes {
                    if note.contains("置信度")
                        || note.contains("安全范围")
                        || note.contains("未改变")
                    {
                        report
                            .warnings
                            .push(format!("人脸 #{}：{}", entry.face_id, note));
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
        let rendered =
            render(&image, &EditParams::default(), &[], &CancelToken::default()).unwrap();
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
    fn background_replaces_only_outside_manual_mask() {
        use crate::image_engine::params::{
            BackgroundMode, BackgroundParams, MaskPoint, SubjectMaskParams, SubjectMaskStroke,
            SubjectMaskStrokeKind,
        };
        let image: Image16 = Image16::from_pixel(64, 64, Rgba([30_000, 30_000, 30_000, 65535]));
        let params = EditParams {
            subject_mask: SubjectMaskParams {
                enabled: true,
                feather: 0.0,
                smooth: 0.0,
                hair_protect: 0.0,
                strokes: vec![SubjectMaskStroke {
                    kind: SubjectMaskStrokeKind::Paint,
                    radius: 0.25,
                    hardness: 100.0,
                    strength: 100.0,
                    points: vec![MaskPoint { x: 0.5, y: 0.5 }],
                }],
                ..Default::default()
            },
            background: BackgroundParams {
                enabled: true,
                mode: BackgroundMode::SolidColor,
                color: "#FF0000".into(),
                decontaminate: 0.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let (rendered, report) = render_with_report(
            &image,
            &params,
            &[],
            &CancelToken::default(),
            &mut |_, _| {},
        )
        .unwrap();
        assert!(report.background.is_some());
        assert_eq!(rendered.get_pixel(0, 0).0[0], 0xFFFF);
        assert_eq!(rendered.get_pixel(0, 0).0[1], 0);
        assert_eq!(
            rendered.get_pixel(32, 32).0,
            [30_000, 30_000, 30_000, 65535]
        );
    }

    #[test]
    fn body_edit_changes_subject_not_corner() {
        use crate::image_engine::params::{
            BodyAdjust, BodyShape, MaskPoint, SubjectMaskParams, SubjectMaskStroke,
            SubjectMaskStrokeKind,
        };
        let image: Image16 = Image16::from_fn(64, 64, |x, y| {
            Rgba([
                (x as u16 * 800).min(60000),
                (y as u16 * 800).min(60000),
                20000,
                65535,
            ])
        });
        let corner_before = image.get_pixel(0, 0).0;
        let params = EditParams {
            subject_mask: SubjectMaskParams {
                enabled: true,
                feather: 0.0,
                smooth: 0.0,
                hair_protect: 0.0,
                strokes: vec![SubjectMaskStroke {
                    kind: SubjectMaskStrokeKind::Paint,
                    radius: 0.3,
                    hardness: 100.0,
                    strength: 100.0,
                    points: vec![MaskPoint { x: 0.5, y: 0.5 }],
                }],
                ..Default::default()
            },
            body: vec![BodyAdjust {
                subject_id: "subject-1".into(),
                enabled: true,
                shape: BodyShape {
                    waist: -90.0,
                    ..Default::default()
                },
                anchors: Vec::new(),
                strokes: Vec::new(),
                mask_strokes: Vec::new(),
            }],
            ..Default::default()
        };
        let (rendered, report) = render_with_report(
            &image,
            &params,
            &[],
            &CancelToken::default(),
            &mut |_, _| {},
        )
        .unwrap();
        assert_eq!(rendered.get_pixel(0, 0).0, corner_before);
        assert_eq!(report.body.len(), 1);
        assert!(report.body[0].affected_pixels > 0);
        assert_ne!(rendered.into_raw(), image.into_raw());
    }

    #[test]
    fn background_without_mask_warns_and_keeps_pixels() {
        use crate::image_engine::params::{BackgroundMode, BackgroundParams};
        let image: Image16 = Image16::from_pixel(16, 16, Rgba([1000, 2000, 3000, 65535]));
        let params = EditParams {
            background: BackgroundParams {
                enabled: true,
                mode: BackgroundMode::SolidColor,
                color: "#00FF00".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let (rendered, report) = render_with_report(
            &image,
            &params,
            &[],
            &CancelToken::default(),
            &mut |_, _| {},
        )
        .unwrap();
        assert_eq!(rendered.into_raw(), image.into_raw());
        assert!(report
            .warnings
            .iter()
            .any(|warning| warning.contains("人物蒙版")));
    }

    #[test]
    fn beauty_without_faces_does_not_touch_pixels() {
        use crate::image_engine::params::{BeautyParams, FaceBeauty};
        let image: Image16 = Image16::from_pixel(48, 48, Rgba([30_000, 24_000, 20_000, 65535]));
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
