//! 手动人物蒙版相关命令。能力查询会明确说明 AI 分割未启用及原因。

use std::path::Path;

use serde::Serialize;
use tauri::State;

use crate::ai_engine::segmentation::{self, SegmentationCapability};
use crate::error::AppResult;
use crate::image_engine::params::SubjectMaskParams;
use crate::image_engine::preview::encode_canvas;
use crate::state::ImageCache;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubjectMaskPreview {
    pub url: String,
    pub width: u32,
    pub height: u32,
    pub coverage: f32,
    pub edge_ratio: f32,
    pub source: String,
    pub model_version: String,
    pub strokes: usize,
    pub note: String,
}

#[tauri::command]
pub fn segmentation_capability() -> SegmentationCapability {
    segmentation::capability()
}

/// 生成蒙版预览。`mode` 为 `overlay` 时输出红色遮罩（Alpha 保留在透明度里），
/// 为 `alpha` 时输出黑白蒙版。预览使用归一化笔触，和导出共用同一算法。
#[tauri::command]
pub async fn preview_subject_mask(
    state: State<'_, ImageCache>,
    path: String,
    params: SubjectMaskParams,
    mode: Option<String>,
) -> AppResult<SubjectMaskPreview> {
    let cache = state.inner().clone();
    let mode = mode.unwrap_or_else(|| "overlay".into());
    tauri::async_runtime::spawn_blocking(move || {
        let cached = cache.load(Path::new(&path))?;
        let working = fit_preview(&cached.preview);
        let mask = segmentation::build_subject_mask(&working, &params);
        let coverage = mask.coverage();
        let edge_ratio = mask.edge_ratio();
        let visualization = if mode == "alpha" {
            mask.to_image16()
        } else {
            overlay_image(&mask)
        };
        let preview = encode_canvas(&visualization, true)?;
        Ok(SubjectMaskPreview {
            url: preview.url,
            width: preview.width,
            height: preview.height,
            coverage,
            edge_ratio,
            source: mask.source.clone(),
            model_version: mask.model_version.clone(),
            strokes: params.strokes.len(),
            note: "手动人物蒙版：本地笔触 + Alpha 处理，非 AI 自动分割".into(),
        })
    })
    .await
    .map_err(|error| crate::error::AppError::Task(error.to_string()))?
}

fn fit_preview(image: &crate::image_engine::buffer::Image16) -> crate::image_engine::buffer::Image16 {
    // 预览已限制在 1800px；这里只做上限保护，避免异常缓存尺寸。
    crate::image_engine::buffer::fit_within(image, crate::image_engine::PREVIEW_MAX_EDGE)
}

fn overlay_image(mask: &segmentation::SubjectMask) -> crate::image_engine::buffer::Image16 {
    let width = mask.width.max(1);
    let height = mask.height.max(1);
    crate::image_engine::buffer::Image16::from_fn(width, height, |x, y| {
        let alpha = mask.alpha[y as usize * width as usize + x as usize].clamp(0.0, 1.0);
        image::Rgba([
            65535,
            12000,
            12000,
            (alpha * 65535.0 * 0.72).round() as u16,
        ])
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_engine::params::{
        MaskPoint, SubjectMaskParams, SubjectMaskStroke, SubjectMaskStrokeKind,
    };
    use image::Rgba;

    #[test]
    fn overlay_is_semi_transparent_and_tracks_alpha() {
        let image: crate::image_engine::buffer::Image16 =
            crate::image_engine::buffer::Image16::from_pixel(16, 16, Rgba([0, 0, 0, 65535]));
        let params = SubjectMaskParams {
            enabled: true,
            feather: 0.0,
            smooth: 0.0,
            hair_protect: 0.0,
            strokes: vec![SubjectMaskStroke {
                kind: SubjectMaskStrokeKind::Paint,
                radius: 0.2,
                hardness: 100.0,
                strength: 100.0,
                points: vec![MaskPoint { x: 0.5, y: 0.5 }],
            }],
            ..Default::default()
        };
        let mask = segmentation::build_subject_mask(&image, &params);
        let overlay = overlay_image(&mask);
        let center = overlay.get_pixel(8, 8).0;
        let corner = overlay.get_pixel(0, 0).0;
        assert!(center[3] > 30000);
        assert_eq!(corner[3], 0);
    }
}
