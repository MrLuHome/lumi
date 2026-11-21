//! 一键美肤的皮肤分析与参数建议。
//!
//! 不生成“重绘人脸”：分析只产出可编辑的 `BeautyParams` 与说明，
//! 前端必须展示并允许用户逐项修改后才会真正渲染。

use serde::Serialize;

use crate::ai_engine::face_frame::FaceGeometry;
use crate::ai_engine::retouch::eyes::{forehead_weight, nasolabial_weight, under_eye_weight};
use crate::ai_engine::retouch::filter::{box_mean, luma};
use crate::ai_engine::retouch::lab::{chroma, srgb_to_lab};
use crate::ai_engine::skin_segmentation::SkinMask;
use crate::error::AppResult;
use crate::image_engine::buffer::{Image16, CHANNEL_MAX};
use crate::image_engine::cancel::CancelToken;
use crate::image_engine::params::BeautyParams;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceMetrics {
    pub face_id: u32,
    /// 皮肤蒙版覆盖人脸框的比例，可用于判断侧脸/遮挡。
    pub skin_ratio: f32,
    pub mean_luma: f32,
    /// 平均高频能量，越大纹理越明显。
    pub texture: f32,
    /// 疑似瑕疵像素占皮肤比例。
    pub blemish_density: f32,
    /// 色度标准差，反映肤色不均。
    pub chroma_variation: f32,
    /// 平均 Lab a（偏红程度）。
    pub redness: f32,
    /// 暗部占皮肤比例。
    pub dark_ratio: f32,
    /// 高光占皮肤比例。
    pub highlight_ratio: f32,
    /// 眼下与脸颊的亮度差。
    pub dark_circle_delta: f32,
    pub eye_texture: f32,
    pub forehead_texture: f32,
    pub nasolabial_texture: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceSuggestion {
    pub face_id: u32,
    pub metrics: FaceMetrics,
    pub suggested: BeautyParams,
    pub notes: Vec<String>,
}

/// 分析单张人脸并生成保守建议。结果完全由像素统计决定，可重复。
pub fn analyze_face(
    image: &Image16,
    mask: &SkinMask,
    geometry: &FaceGeometry,
    cancel: &CancelToken,
) -> AppResult<FaceSuggestion> {
    let width = mask.width;
    let height = mask.height;
    let count = (width * height) as usize;
    let mut luma_plane = vec![0f32; count];
    let mut active = vec![false; count];

    let mut skin_pixels = 0f32;
    let mut sum_luma = 0f32;
    let mut sum_a = 0f32;
    let mut sum_b = 0f32;
    let mut sum_chroma_sq = 0f32;
    let mut dark = 0f32;
    let mut highlight = 0f32;

    for y in 0..height {
        if y % 64 == 0 {
            cancel.check()?;
        }
        for x in 0..width {
            let index = (y * width + x) as usize;
            if mask.alpha[index] <= 0.5 {
                continue;
            }
            let image_x = mask.x0 as u32 + x;
            let image_y = mask.y0 as u32 + y;
            if image_x >= image.width() || image_y >= image.height() {
                continue;
            }
            let pixel = image.get_pixel(image_x, image_y).0;
            let value = luma(pixel);
            luma_plane[index] = value;
            active[index] = true;
            skin_pixels += 1.0;
            sum_luma += value;
            let lab = srgb_to_lab(
                pixel[0] as f32 / CHANNEL_MAX,
                pixel[1] as f32 / CHANNEL_MAX,
                pixel[2] as f32 / CHANNEL_MAX,
            );
            sum_a += lab[1];
            sum_b += lab[2];
            sum_chroma_sq += chroma(lab[1], lab[2]).powi(2);
            if value < 0.35 {
                dark += 1.0;
            }
            if value > 0.90 {
                highlight += 1.0;
            }
        }
    }

    let total = (width * height) as f32;
    if skin_pixels < 50.0 {
        return Ok(FaceSuggestion {
            face_id: geometry.id,
            metrics: FaceMetrics {
                face_id: geometry.id,
                skin_ratio: 0.0,
                mean_luma: 0.0,
                texture: 0.0,
                blemish_density: 0.0,
                chroma_variation: 0.0,
                redness: 0.0,
                dark_ratio: 0.0,
                highlight_ratio: 0.0,
                dark_circle_delta: 0.0,
                eye_texture: 0.0,
                forehead_texture: 0.0,
                nasolabial_texture: 0.0,
            },
            suggested: BeautyParams::default(),
            notes: vec!["皮肤区域过小，无法可靠分析（可能侧脸或严重遮挡）".into()],
        });
    }

    let mean_luma = sum_luma / skin_pixels;
    let base = box_mean(&luma_plane, width, height, 4);
    let mut texture_sum = 0f32;
    let mut blemish = 0f32;
    let mut eye_texture = 0f32;
    let mut eye_count = 0f32;
    let mut forehead_texture = 0f32;
    let mut forehead_count = 0f32;
    let mut naso_texture = 0f32;
    let mut naso_count = 0f32;
    let mut under_luma = 0f32;
    let mut under_count = 0f32;
    let eye_right = geometry.to_local(geometry.eyes[0].0, geometry.eyes[0].1);
    let eye_left = geometry.to_local(geometry.eyes[1].0, geometry.eyes[1].1);

    for y in 0..height {
        for x in 0..width {
            let index = (y * width + x) as usize;
            if !active[index] {
                continue;
            }
            let detail = (luma_plane[index] - base[index]).abs();
            texture_sum += detail;
            if luma_plane[index] > 0.10 && luma_plane[index] - base[index] < -0.045 {
                blemish += 1.0;
            }
            let image_x = mask.x0 as u32 + x;
            let image_y = mask.y0 as u32 + y;
            let (u, v) = geometry.to_local(image_x as f32, image_y as f32);
            let eye_weight =
                under_eye_weight(u, v, eye_right).max(under_eye_weight(u, v, eye_left));
            if eye_weight > 0.4 {
                eye_texture += detail;
                eye_count += 1.0;
                under_luma += luma_plane[index];
                under_count += 1.0;
            }
            if forehead_weight(u, v) > 0.4 {
                forehead_texture += detail;
                forehead_count += 1.0;
            }
            if nasolabial_weight(u, v) > 0.4 {
                naso_texture += detail;
                naso_count += 1.0;
            }
        }
    }

    let texture = texture_sum / skin_pixels;
    let blemish_density = blemish / skin_pixels;
    let mean_a = sum_a / skin_pixels;
    let mean_b = sum_b / skin_pixels;
    let mean_chroma_sq = sum_chroma_sq / skin_pixels;
    let chroma_variation = (mean_chroma_sq - (mean_a * mean_a + mean_b * mean_b))
        .max(0.0)
        .sqrt()
        / 10.0;
    let dark_ratio = dark / skin_pixels;
    let highlight_ratio = highlight / skin_pixels;
    let skin_ratio = skin_pixels / total;
    let dark_circle_delta = if under_count > 0.0 {
        (mean_luma - under_luma / under_count).max(0.0)
    } else {
        0.0
    };
    let eye_texture = if eye_count > 0.0 {
        eye_texture / eye_count
    } else {
        0.0
    };
    let forehead_texture = if forehead_count > 0.0 {
        forehead_texture / forehead_count
    } else {
        0.0
    };
    let nasolabial_texture = if naso_count > 0.0 {
        naso_texture / naso_count
    } else {
        0.0
    };

    let mut suggested = BeautyParams {
        skin_smooth: (20.0 + 220.0 * texture + 25.0 * dark_ratio).clamp(10.0, 55.0),
        texture: (38.0 + 260.0 * (texture - 0.035).max(0.0)).clamp(30.0, 70.0),
        detail: 25.0,
        blemish: (blemish_density * 900.0).clamp(0.0, 55.0),
        spots: (blemish_density * 500.0).clamp(0.0, 35.0),
        blackheads: (blemish_density * 300.0).clamp(0.0, 25.0),
        tone_even: (18.0 + chroma_variation * 90.0).clamp(0.0, 45.0),
        whitening: (12.0 + (0.62 - mean_luma).max(0.0) * 80.0 - highlight_ratio * 40.0)
            .clamp(0.0, 30.0),
        redness: ((mean_a - 12.0).max(0.0) * 2.6).clamp(0.0, 40.0),
        dullness: (dark_ratio * 110.0).clamp(0.0, 40.0),
        dark_circles: (dark_circle_delta * 280.0).clamp(0.0, 45.0),
        eye_lines: (eye_texture * 240.0).clamp(0.0, 25.0),
        forehead_lines: (forehead_texture * 200.0).clamp(0.0, 20.0),
        nasolabial: (nasolabial_texture * 200.0).clamp(0.0, 20.0),
    };

    // 高光过曝时不再美白，避免死白。
    if highlight_ratio > 0.12 {
        suggested.whitening = (suggested.whitening * 0.4).clamp(0.0, 30.0);
    }

    let mut notes = Vec::new();
    if skin_ratio < 0.28 {
        notes.push("皮肤区域偏小（侧脸或遮挡），已降低整体强度".into());
        suggested.skin_smooth *= 0.7;
        suggested.blemish *= 0.6;
    }
    if blemish_density > 0.02 {
        notes.push("疑似密集小点，祛痘祛斑采用保守值".into());
        suggested.blemish = suggested.blemish.min(35.0);
        suggested.spots = suggested.spots.min(20.0);
    }
    if highlight_ratio > 0.12 {
        notes.push("检测到较多高光，美白已自动降权".into());
    }
    if dark_circle_delta > 0.05 {
        notes.push("眼下暗沉明显，建议开启黑眼圈淡化".into());
    }
    if mean_luma < 0.35 {
        notes.push("整体偏暗，建议同时调整光线".into());
    }
    if notes.is_empty() {
        notes.push("皮肤状态较稳定，建议使用自然档参数".into());
    }

    Ok(FaceSuggestion {
        face_id: geometry.id,
        metrics: FaceMetrics {
            face_id: geometry.id,
            skin_ratio,
            mean_luma,
            texture,
            blemish_density,
            chroma_variation,
            redness: mean_a,
            dark_ratio,
            highlight_ratio,
            dark_circle_delta,
            eye_texture,
            forehead_texture,
            nasolabial_texture,
        },
        suggested,
        notes,
    })
}

/// 平滑预设：轻/自然/强，供前端一键套用，仍然是可编辑参数。
pub fn preset(level: &str) -> BeautyParams {
    match level {
        "light" => BeautyParams {
            skin_smooth: 22.0,
            texture: 55.0,
            detail: 25.0,
            blemish: 20.0,
            tone_even: 12.0,
            ..Default::default()
        },
        "strong" => BeautyParams {
            skin_smooth: 48.0,
            texture: 40.0,
            detail: 30.0,
            blemish: 45.0,
            spots: 25.0,
            tone_even: 30.0,
            whitening: 12.0,
            dark_circles: 20.0,
            ..Default::default()
        },
        _ => BeautyParams {
            skin_smooth: 32.0,
            texture: 50.0,
            detail: 25.0,
            blemish: 30.0,
            spots: 15.0,
            tone_even: 18.0,
            dark_circles: 12.0,
            ..Default::default()
        },
    }
}
