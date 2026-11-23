//! G14 · 一键精修的瘦脸 / 瘦身计划命令。
//!
//! 返回保守的 `FaceShapePlan` / `BodyShapePlan`，由前端写入既有
//! `FaceReshape` / `BodyAdjust` 参数（非破坏，可撤销）。

use std::path::Path;

use serde::Serialize;
use tauri::State;

use crate::ai_engine::body::ai_slim::{plan_body_slim, precheck_body_plan};
use crate::ai_engine::body::analysis::analyze_body;
use crate::ai_engine::face_frame::FaceFrame;
use crate::ai_engine::segmentation;
use crate::error::{AppError, AppResult};
use crate::face_reshape::ai_slim::{plan_face_slim, SlimStyle};
use crate::face_reshape::contour::analyze_face_contour;
use crate::image_engine::buffer::fit_within;
use crate::image_engine::params::{BodyShape, FaceShape, SubjectMaskParams};
use crate::image_engine::PREVIEW_MAX_EDGE;
use crate::state::ImageCache;

fn parse_style(style: &str) -> SlimStyle {
    match style {
        "refined" => SlimStyle::Refined,
        "advanced" => SlimStyle::Advanced,
        _ => SlimStyle::Natural,
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FaceShapePlan {
    pub face_id: u32,
    pub face_uuid: String,
    pub face: FaceShape,
    pub confidence: f32,
    pub allow_aggressive: bool,
    pub notes: Vec<String>,
    pub skipped: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BodyShapePlan {
    pub subject_id: String,
    pub shape: BodyShape,
    pub confidence: f32,
    pub notes: Vec<String>,
    pub skipped: bool,
}

/// 由人脸检测结果 + 档位/强度生成瘦脸计划。
#[tauri::command]
pub fn plan_face_shape(
    face: FaceFrame,
    image_width: u32,
    image_height: u32,
    style: String,
    strength: f32,
) -> AppResult<FaceShapePlan> {
    let style = parse_style(&style);
    let contour = match analyze_face_contour(&face, image_width.max(1), image_height.max(1)) {
        Ok(value) => value,
        Err(error) => {
            return Ok(FaceShapePlan {
                face_id: face.id,
                face_uuid: face.uuid.clone(),
                face: FaceShape::default(),
                confidence: 0.0,
                allow_aggressive: false,
                notes: vec![error.to_string()],
                skipped: true,
            });
        }
    };
    let (shape, notes) = plan_face_slim(&contour, style, strength);
    Ok(FaceShapePlan {
        face_id: contour.face_id,
        face_uuid: contour.face_uuid.clone(),
        face: shape,
        confidence: contour.confidence,
        allow_aggressive: contour.allow_aggressive,
        notes,
        skipped: shape.is_neutral(),
    })
}

/// 由照片 + 人物蒙版生成瘦身计划（与锚点建议同一套蒙版构建）。
#[tauri::command]
pub async fn plan_body_shape(
    state: State<'_, ImageCache>,
    path: String,
    mask: SubjectMaskParams,
    subject_id: String,
    style: String,
    strength: f32,
) -> AppResult<BodyShapePlan> {
    if !mask.has_content() {
        return Ok(BodyShapePlan {
            subject_id,
            shape: BodyShape::default(),
            confidence: 0.0,
            notes: vec!["未绘制人物蒙版，已跳过自动瘦身".into()],
            skipped: true,
        });
    }
    let style = parse_style(&style);
    let cache = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let cached = cache.load(Path::new(&path))?;
        let working = fit_within(&cached.preview, PREVIEW_MAX_EDGE);
        let subject = segmentation::build_subject_mask(&working, &mask);
        let alpha = subject.alpha.clone();
        let analysis = analyze_body(
            &subject_id,
            &alpha,
            working.width(),
            working.height(),
            &[],
        );
        if precheck_body_plan(&analysis, &BodyShape::default()).is_err() && !analysis.detected {
            return Ok(BodyShapePlan {
                subject_id,
                shape: BodyShape::default(),
                confidence: analysis.confidence,
                notes: analysis.notes,
                skipped: true,
            });
        }
        let (shape, mut notes) = plan_body_slim(&analysis, style, strength);
        if shape.is_neutral() {
            notes.extend(analysis.notes.iter().cloned());
        }
        Ok(BodyShapePlan {
            subject_id: analysis.subject_id.clone(),
            shape,
            confidence: analysis.confidence,
            notes,
            skipped: shape.is_neutral(),
        })
    })
    .await
    .map_err(|error| AppError::Unsupported(error.to_string()))?
}
