//! 身材能力与手动锚点建议。
//!
//! 锚点是「建议位置 + 用户确认/拖动」，不是 AI 人体关键点；
//! 只有手动蒙版存在时才会给出建议，避免在没有人物区域时输出猜测结果。

use std::path::Path;

use serde::Serialize;
use tauri::State;

use crate::ai_engine::body::landmarks::analyze;
use crate::ai_engine::body::pose::{self, PoseCapability};
use crate::ai_engine::segmentation;
use crate::error::{AppError, AppResult};
use crate::image_engine::buffer::fit_within;
use crate::image_engine::params::{BodyAnchorPart, SubjectMaskParams};
use crate::image_engine::PREVIEW_MAX_EDGE;
use crate::state::ImageCache;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BodyAnchorPreset {
    pub part: BodyAnchorPart,
    pub x: f32,
    pub y: f32,
}

#[tauri::command]
pub fn body_capability() -> PoseCapability {
    pose::capability()
}

/// 根据手动蒙版轮廓给出锚点建议。用户可以继续拖动、删除或锁定。
#[tauri::command]
pub async fn body_anchor_presets(
    state: State<'_, ImageCache>,
    path: String,
    mask: SubjectMaskParams,
) -> AppResult<Vec<BodyAnchorPreset>> {
    if !mask.has_content() {
        return Err(AppError::Unsupported(
            "请先绘制人物蒙版，再添加身体锚点".into(),
        ));
    }
    let cache = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let cached = cache.load(Path::new(&path))?;
        let working = fit_within(&cached.preview, PREVIEW_MAX_EDGE);
        let subject = segmentation::build_subject_mask(&working, &mask);
        if subject.is_empty() {
            return Err(AppError::Unsupported(
                "人物蒙版为空，无法给出锚点建议".into(),
            ));
        }
        let metrics = analyze(&subject.alpha, working.width(), working.height())
            .ok_or_else(|| AppError::Unsupported("蒙版内没有连续人物轮廓".into()))?;
        let mut presets = Vec::new();
        let plan = [
            (BodyAnchorPart::ShoulderLeft, 0.18),
            (BodyAnchorPart::ShoulderRight, 0.18),
            (BodyAnchorPart::WaistLeft, 0.42),
            (BodyAnchorPart::WaistRight, 0.42),
            (BodyAnchorPart::HipLeft, 0.55),
            (BodyAnchorPart::HipRight, 0.55),
            (BodyAnchorPart::ThighLeft, 0.66),
            (BodyAnchorPart::ThighRight, 0.66),
            (BodyAnchorPart::KneeLeft, 0.78),
            (BodyAnchorPart::KneeRight, 0.78),
            (BodyAnchorPart::CalfLeft, 0.88),
            (BodyAnchorPart::CalfRight, 0.88),
        ];
        for (part, fraction) in plan {
            let Some(row) = metrics.sampled(fraction) else {
                continue;
            };
            let left_side = matches!(
                part,
                BodyAnchorPart::ShoulderLeft
                    | BodyAnchorPart::WaistLeft
                    | BodyAnchorPart::HipLeft
                    | BodyAnchorPart::ThighLeft
                    | BodyAnchorPart::KneeLeft
                    | BodyAnchorPart::CalfLeft
            );
            let x = if left_side {
                row.left / working.width() as f32
            } else {
                row.right / working.width() as f32
            };
            presets.push(BodyAnchorPreset {
                part,
                x: x.clamp(0.0, 1.0),
                y: (row.y / working.height() as f32).clamp(0.0, 1.0),
            });
        }
        if presets.is_empty() {
            return Err(AppError::Unsupported("未找到可放置锚点的轮廓行".into()));
        }
        Ok(presets)
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}
