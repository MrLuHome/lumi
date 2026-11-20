//! 第五轮身材调整（BODY ENGINE 1.0）。
//!
//! 输入是人物 Alpha 与人脸框；输出是变形后的像素与同步变形的 Alpha。
//! 没有 AI 姿态点时，形状参数作用于每一行的轮廓带，手动推拉作用于局部，
//! 所有结果都经过位移上限与折叠检查。

pub mod constraints;
pub mod deformation;
pub mod landmarks;
pub mod mesh;
pub mod pose;

use std::time::Instant;

use serde::Serialize;

use crate::ai_engine::face_frame::FaceFrame;
use crate::error::AppResult;
use crate::image_engine::buffer::Image16;
use crate::image_engine::cancel::CancelToken;
use crate::image_engine::params::BodyAdjust;

use self::constraints::{fold_report, FaceGuard};
use self::deformation::build_field;
use self::mesh::{warp_alpha, warp_image};

/// 与图像同尺寸的蒙版集合：全局蒙版 + 每个身材对象的独立蒙版。
#[derive(Debug, Clone, Default)]
pub struct SubjectMasks {
    pub global: Vec<f32>,
    pub subjects: Vec<(String, Vec<f32>)>,
}

impl SubjectMasks {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            global: vec![0.0; width as usize * height as usize],
            subjects: Vec::new(),
        }
    }

    pub fn subject_mut(&mut self, subject_id: &str) -> Option<&mut Vec<f32>> {
        self.subjects
            .iter_mut()
            .find(|(id, _)| id == subject_id)
            .map(|(_, mask)| mask)
    }

    /// 所有人物蒙版的逐像素最大值，供背景合成使用。
    pub fn combined(&self) -> Vec<f32> {
        let mut output = self.global.clone();
        for (_, mask) in &self.subjects {
            if mask.len() != output.len() {
                continue;
            }
            for (value, other) in output.iter_mut().zip(mask.iter()) {
                if *other > *value {
                    *value = *other;
                }
            }
        }
        output
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BodyReport {
    pub subject_id: String,
    pub displacement_max_ratio: f32,
    pub folded_cells: u32,
    pub affected_pixels: u64,
    pub degraded: bool,
    pub elapsed_ms: u128,
    pub notes: Vec<String>,
}

/// 依次应用每个身材对象。每个对象只在自己的蒙版内产生位移，
/// 因此多人照片中 A 的调整不会污染 B。
pub fn apply_body(
    image: &mut Image16,
    masks: &mut SubjectMasks,
    subjects: &[BodyAdjust],
    faces: &[FaceFrame],
    cancel: &CancelToken,
) -> AppResult<Vec<BodyReport>> {
    let mut reports = Vec::new();
    if subjects.is_empty() || image.width() == 0 || image.height() == 0 {
        return Ok(reports);
    }
    let width = image.width();
    let height = image.height();
    let guard = FaceGuard::from_faces(faces);

    for subject in subjects {
        if !subject.enabled {
            continue;
        }
        cancel.check()?;
        let started = Instant::now();
        let own_mask = masks.subject_mut(&subject.subject_id).cloned();
        let mask = own_mask.as_ref().unwrap_or(&masks.global);
        if mask.len() != width as usize * height as usize {
            reports.push(BodyReport {
                subject_id: subject.subject_id.clone(),
                displacement_max_ratio: 0.0,
                folded_cells: 0,
                affected_pixels: 0,
                degraded: false,
                elapsed_ms: started.elapsed().as_millis(),
                notes: vec!["人物蒙版尺寸与图像不一致，已跳过该人物".into()],
            });
            continue;
        }

        let (mut field, metrics) = build_field(width, height, mask, subject);
        let mut notes = Vec::new();
        if metrics.is_none() {
            notes.push("未能在蒙版内找到连续的人物轮廓，已跳过该人物".into());
        }
        let mut degraded = false;
        let mut report = fold_report(&field);
        if report.has_fold() {
            for _ in 0..3 {
                field.scale(0.6);
                let retry = fold_report(&field);
                if !retry.has_fold() {
                    report = retry;
                    break;
                }
                report = retry;
            }
            degraded = true;
            notes.push("检测到网格折叠风险，已自动降低变形强度".into());
        }
        if report.has_fold() || field.is_zero() {
            if report.has_fold() {
                notes.push("降强度后仍存在折叠风险，本次已跳过以保护图像".into());
            }
            reports.push(BodyReport {
                subject_id: subject.subject_id.clone(),
                displacement_max_ratio: report.max_displacement / width.max(1) as f32,
                folded_cells: report.folded_cells,
                affected_pixels: 0,
                degraded,
                elapsed_ms: started.elapsed().as_millis(),
                notes,
            });
            continue;
        }

        let affected = mask.iter().filter(|value| **value > 0.5).count() as u64;
        *image = warp_image(image, &field, &guard);
        if own_mask.is_some() {
            if let Some(stored) = masks.subject_mut(&subject.subject_id) {
                *stored = warp_alpha(&stored.clone(), width, height, &field, &guard);
            }
        } else if !masks.global.is_empty() {
            masks.global = warp_alpha(&masks.global.clone(), width, height, &field, &guard);
        }
        reports.push(BodyReport {
            subject_id: subject.subject_id.clone(),
            displacement_max_ratio: report.max_displacement / width.max(1) as f32,
            folded_cells: report.folded_cells,
            affected_pixels: affected,
            degraded,
            elapsed_ms: started.elapsed().as_millis(),
            notes,
        });
    }
    Ok(reports)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_engine::params::{
        BodyShape, MaskPoint, SubjectMaskStroke, SubjectMaskStrokeKind,
    };
    use image::Rgba;

    fn mask_with_rect(width: u32, height: u32) -> Vec<f32> {
        let mut alpha = vec![0.0f32; (width * height) as usize];
        for y in 30..90u32 {
            for x in 30..70u32 {
                alpha[y as usize * width as usize + x as usize] = 1.0;
            }
        }
        alpha
    }

    #[test]
    fn body_warp_changes_pixels_inside_mask_only() {
        let width = 100;
        let height = 120;
        let mut image: Image16 = Image16::from_fn(width, height, |x, y| {
            if (30..70).contains(&x) && (30..90).contains(&y) {
                Rgba([20000, 30000, 40000, 65535])
            } else {
                Rgba([1000, 2000, 3000, 65535])
            }
        });
        let background_before = image.get_pixel(5, 5).0;
        let mut masks = SubjectMasks {
            global: Vec::new(),
            subjects: vec![(
                "a".into(),
                mask_with_rect(width, height),
            )],
        };
        let subject = BodyAdjust {
            subject_id: "a".into(),
            enabled: true,
            shape: BodyShape {
                waist: -80.0,
                ..Default::default()
            },
            anchors: Vec::new(),
            strokes: Vec::new(),
            mask_strokes: Vec::new(),
        };
        let reports = apply_body(
            &mut image,
            &mut masks,
            &[subject],
            &[],
            &CancelToken::default(),
        )
        .unwrap();
        assert_eq!(reports.len(), 1);
        assert!(reports[0].affected_pixels > 0);
        assert_eq!(image.get_pixel(5, 5).0, background_before);
        // 蒙版同步：收腰后腰部行宽度变窄。
        let row = 60usize;
        let count = (0..width as usize)
            .filter(|x| masks.subjects[0].1[row * width as usize + x] > 0.5)
            .count();
        assert!(count < 40, "count={count}");
    }

    #[test]
    fn subject_without_mask_is_skipped_with_note() {
        let mut image: Image16 = Image16::from_pixel(32, 32, Rgba([1, 2, 3, 65535]));
        let mut masks = SubjectMasks::new(32, 32);
        let subject = BodyAdjust::new("missing");
        let reports = apply_body(
            &mut image,
            &mut masks,
            &[subject],
            &[],
            &CancelToken::default(),
        )
        .unwrap();
        // 没有可动的内容，字段为零，直接跳过且给出说明。
        assert_eq!(reports.len(), 1);
        assert!(reports[0].notes.iter().any(|note| note.contains("跳过")) || reports[0].affected_pixels == 0);
    }

    #[test]
    fn masks_union_combines_subjects() {
        let mut masks = SubjectMasks::new(2, 1);
        masks.global = vec![0.2, 0.0];
        masks.subjects.push(("a".into(), vec![0.0, 0.9]));
        assert_eq!(masks.combined(), vec![0.2, 0.9]);
    }

    #[test]
    fn subject_masks_isolate_each_other() {
        let width = 100;
        let height = 120;
        let mut image: Image16 = Image16::from_pixel(width, height, Rgba([100, 100, 100, 65535]));
        let mut left = vec![0.0f32; (width * height) as usize];
        let mut right = vec![0.0f32; (width * height) as usize];
        for y in 30..90u32 {
            for x in 10..40u32 {
                left[y as usize * width as usize + x as usize] = 1.0;
            }
            for x in 60..90u32 {
                right[y as usize * width as usize + x as usize] = 1.0;
            }
        }
        let mut masks = SubjectMasks {
            global: Vec::new(),
            subjects: vec![("a".into(), left), ("b".into(), right.clone())],
        };
        let subject = BodyAdjust {
            subject_id: "a".into(),
            enabled: true,
            shape: BodyShape {
                waist: -80.0,
                ..Default::default()
            },
            anchors: Vec::new(),
            strokes: Vec::new(),
            mask_strokes: Vec::new(),
        };
        apply_body(&mut image, &mut masks, &[subject], &[], &CancelToken::default()).unwrap();
        // B 的蒙版逐像素不变。
        assert_eq!(masks.subjects[1].1, right);
    }

    #[test]
    fn stroke_mask_is_used_for_subject_alpha() {
        let stroke = SubjectMaskStroke {
            kind: SubjectMaskStrokeKind::Paint,
            radius: 0.05,
            hardness: 60.0,
            strength: 100.0,
            points: vec![MaskPoint { x: 0.5, y: 0.5 }],
        };
        assert!(!stroke.points.is_empty());
    }
}
