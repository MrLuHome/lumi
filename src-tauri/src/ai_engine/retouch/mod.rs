//! AI 美肤操作集合。每个操作都要求皮肤蒙版或关键点几何，
//! 没有检测结果时不执行任何“全图美化”。

pub mod analyze;
pub mod blemish;
pub mod bilateral_grid;
pub mod eyes;
pub mod filter;
pub mod frequency;
pub mod fusion;
pub mod lab;
pub mod quality;
pub mod skin_engine;
pub mod smoothing;
pub mod texture_protect;
pub mod tone;

use crate::ai_engine::face_frame::FaceGeometry;
use crate::ai_engine::retouch::blemish::BlemishReport;
use crate::ai_engine::skin_segmentation::SkinMask;
use crate::error::AppResult;
use crate::image_engine::buffer::Image16;
use crate::image_engine::cancel::CancelToken;
use crate::image_engine::params::{BeautyParams, RepairPoint};

#[derive(Debug, Clone, Default)]
pub struct FaceRetouchReport {
    pub face_id: u32,
    pub skin_pixels: usize,
    pub blemish_detected: usize,
    pub blemish_repaired: usize,
    pub blemish_skipped: Option<String>,
    /// S1：磨皮核心是否执行。
    pub skin_engine_ran: bool,
    pub over_smooth_risk: f32,
    pub protected_leak: f32,
    pub notes: Vec<String>,
}

/// 对单张人脸执行完整美肤链：瑕疵 → **SkinRetouchEngine** → 肤色 → 眼周。
///
/// S1：平滑阶段由 `skin_engine::run_skin_retouch`（双边网格+频率分离+纹理保护）替代
/// 旧的单纯 guided 平滑；blemish / tone / eyes 仍为独立模块。
pub fn apply_face_retouch(
    image: &mut Image16,
    mask: &SkinMask,
    geometry: &FaceGeometry,
    beauty: &BeautyParams,
    legacy_strength: f32,
    repair_points: &[RepairPoint],
    cancel: &CancelToken,
) -> AppResult<FaceRetouchReport> {
    let mut report = FaceRetouchReport {
        face_id: geometry.id,
        skin_pixels: mask.active_pixels(),
        ..Default::default()
    };
    if beauty.is_neutral() && legacy_strength <= 0.0 {
        return Ok(report);
    }

    let blemish: BlemishReport = blemish::repair_blemishes(
        image,
        mask,
        beauty.blemish,
        beauty.spots,
        beauty.blackheads,
        repair_points,
        cancel,
    )?;
    report.blemish_detected = blemish.detected;
    report.blemish_repaired = blemish.repaired;
    report.blemish_skipped = blemish.skipped_reason;

    // S1 核心：双边网格 + 频率分离 + 纹理保护 + 融合
    let out = skin_engine::run_skin_retouch(
        image,
        mask,
        geometry,
        beauty,
        legacy_strength,
        cancel,
    )?;
    report.skin_engine_ran = true;
    report.over_smooth_risk = out.quality.over_smooth_risk;
    report.protected_leak = out.quality.protected_leak;
    report.notes = out.quality.notes.clone();
    if out.seam_risk {
        report.notes.push("蒙版边缘权重偏高，请检查过渡".into());
    }

    tone::even_skin_tone(image, mask, beauty, cancel)?;
    eyes::treat_eye_area(image, mask, geometry, beauty, cancel)?;
    Ok(report)
}
