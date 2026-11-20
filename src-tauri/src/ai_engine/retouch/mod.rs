//! AI 美肤操作集合。每个操作都要求皮肤蒙版或关键点几何，
//! 没有检测结果时不执行任何“全图美化”。

pub mod analyze;
pub mod blemish;
pub mod eyes;
pub mod filter;
pub mod lab;
pub mod smoothing;
pub mod tone;

use crate::ai_engine::face_frame::FaceGeometry;
use crate::ai_engine::retouch::blemish::BlemishReport;
use crate::ai_engine::skin_segmentation::SkinMask;
use crate::image_engine::buffer::Image16;
use crate::image_engine::cancel::CancelToken;
use crate::image_engine::params::{BeautyParams, RepairPoint};
use crate::error::AppResult;

#[derive(Debug, Clone, Default)]
pub struct FaceRetouchReport {
    pub face_id: u32,
    pub skin_pixels: usize,
    pub blemish_detected: usize,
    pub blemish_repaired: usize,
    pub blemish_skipped: Option<String>,
}

/// 对单张人脸执行完整美肤链：瑕疵 → 平滑 → 肤色 → 眼周。
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

    smoothing::smooth_skin(image, mask, beauty, legacy_strength, cancel)?;
    tone::even_skin_tone(image, mask, beauty, cancel)?;
    eyes::treat_eye_area(image, mask, geometry, beauty, cancel)?;
    Ok(report)
}
