//! SkinRetouchEngine：S1 磨皮核心编排。
//!
//! 输入原图 + 蒙版 + 参数 → 输出精修图 + 过程蒙版 + 质量报告。
//! 内部：双边网格（多尺度）→ 频率分离低频均匀 → 纹理保护融合。
//! 瑕疵修复 / 肤色均匀仍走独立模块（blemish / tone），由 `apply_face_retouch` 调用。

use crate::ai_engine::face_frame::FaceGeometry;
use crate::ai_engine::retouch::bilateral_grid::{bilateral_grid_smooth, params_from_strength};
use crate::ai_engine::retouch::frequency::{self, FrequencySplit};
use crate::ai_engine::retouch::fusion::{self, FusionRequest};
use crate::ai_engine::retouch::quality::{self, QualityReport};
use crate::ai_engine::retouch::texture_protect;
use crate::ai_engine::skin_segmentation::SkinMask;
use crate::error::AppResult;
use crate::image_engine::buffer::Image16;
use crate::image_engine::cancel::CancelToken;
use crate::image_engine::params::BeautyParams;

/// S1 引擎输出。
pub struct SkinRetouchOutput {
    /// 最终处理蒙版（skin × protect，与 ROI 同尺寸）。
    pub process_mask: Vec<f32>,
    pub quality: QualityReport,
    /// 中间：频率分解（验收用；可为空表示未跑）。
    pub frequency: Option<FrequencySplit>,
    pub fused_pixels: u64,
    pub seam_risk: bool,
}

/// 执行皮肤精修（不含 blemish/tone/eyes——由上层串联）。
///
/// 原地修改 `image` 中 mask ROI。
pub fn run_skin_retouch(
    image: &mut Image16,
    mask: &SkinMask,
    geometry: &FaceGeometry,
    beauty: &BeautyParams,
    legacy_strength: f32,
    cancel: &CancelToken,
) -> AppResult<SkinRetouchOutput> {
    let strength = beauty.skin_smooth.max(legacy_strength).clamp(0.0, 100.0) / 100.0;
    if strength <= 0.0001 || mask.alpha.is_empty() {
        return Ok(SkinRetouchOutput {
            process_mask: mask.alpha.clone(),
            quality: QualityReport::default(),
            frequency: None,
            fused_pixels: 0,
            seam_risk: false,
        });
    }

    let (mw, mh) = (mask.width, mask.height);
    let n = (mw * mh) as usize;
    cancel.check()?;

    // 1) 提取 ROI 到 0..1 RGB
    let mut rgb = fusion::extract_roi(image, (mask.x0, mask.y0), mw, mh);

    // 2) 纹理保护 + 处理权重
    let protect = texture_protect::build_texture_protection(image, mask, geometry, cancel)?;
    let process_w = texture_protect::combine_with_skin(&mask.alpha, &protect);

    // 3) 频率分离（对当前 ROI）：先均匀低频，按保护比例回注高频
    let radius = ((mw as f32 * 0.02).round() as u32).clamp(3, 20);
    let eps = 0.004 + 0.02 * (1.0 - beauty.texture.clamp(0.0, 100.0) / 100.0);
    // split 需要 Image16；改为对 rgb 平面直接分解
    let (mut low, high) = split_planes(&rgb, mw, mh, radius, eps, cancel)?;
    let tone_even = (beauty.tone_even.clamp(0.0, 100.0) / 100.0) * 0.7;
    frequency::even_low_frequency_wh(&mut low, mw, mh, &process_w, tone_even, cancel)?;
    let high_keep = (0.20
        + 0.50 * beauty.texture.clamp(0.0, 100.0) / 100.0
        + 0.20 * beauty.detail.clamp(0.0, 100.0) / 100.0)
        .clamp(0.10, 0.90);
    // 重组：以平滑低频 + 受控高频为“基础皮肤”
    for i in 0..n {
        for c in 0..3 {
            rgb[i * 3 + c] = (low[c][i] + high[c][i] * high_keep).clamp(0.0, 1.0);
        }
    }

    // 4) 多尺度双边网格（在基础皮肤上继续平滑，带 process mask）
    let mut grid_params = params_from_strength(strength * 100.0, beauty.texture, beauty.detail);
    grid_params.strength = strength;
    bilateral_grid_smooth(&mut rgb, mw, mh, Some(&process_w), &grid_params, cancel)?;

    // 5) 质量：before = 原 ROI，after = 处理后 rgb
    let before = fusion::extract_roi(image, (mask.x0, mask.y0), mw, mh);
    let split_stub = FrequencySplit {
        low,
        high,
        width: mw,
        height: mh,
    };
    let q = quality::evaluate(&before, &rgb, &process_w, &protect, mw, mh);

    // 6) 融合回全图
    let report = fusion::fuse(
        FusionRequest {
            image,
            origin: (mask.x0, mask.y0),
            processed_rgb: &rgb,
            weight: &process_w,
            roi_w: mw,
            roi_h: mh,
            strength: 1.0,
        },
        cancel,
    )?;

    Ok(SkinRetouchOutput {
        process_mask: process_w,
        quality: q,
        frequency: Some(split_stub),
        fused_pixels: report.fused_pixels,
        seam_risk: report.seam_risk,
    })
}

/// 对 0..1 RGB 平面做引导滤波频率分离（不依赖 Image16）。
fn split_planes(
    rgb: &[f32],
    width: u32,
    height: u32,
    radius: u32,
    eps: f32,
    cancel: &CancelToken,
) -> AppResult<([Vec<f32>; 3], [Vec<f32>; 3])> {
    use crate::ai_engine::retouch::filter::guided_filter;
    let n = (width * height) as usize;
    let mut low: [Vec<f32>; 3] = [vec![0f32; n], vec![0f32; n], vec![0f32; n]];
    let mut high: [Vec<f32>; 3] = [vec![0f32; n], vec![0f32; n], vec![0f32; n]];
    for c in 0..3 {
        cancel.check()?;
        let mut plane = vec![0f32; n];
        for i in 0..n {
            plane[i] = rgb[i * 3 + c];
        }
        low[c] = guided_filter(&plane, width, height, radius, eps);
        for i in 0..n {
            high[c][i] = plane[i] - low[c][i];
        }
    }
    Ok((low, high))
}

/// 供验收导出中间结果：把 ROI 平面写成 Image16（灰度伪彩可选）。
pub fn planes_to_image(rgb: &[f32], width: u32, height: u32) -> Image16 {
    use image::Rgba;
    let mut img = Image16::new(width, height);
    let n = (width * height) as usize;
    for i in 0..n {
        let r = (rgb[i * 3].clamp(0.0, 1.0) * 65535.0).round() as u16;
        let g = (rgb[i * 3 + 1].clamp(0.0, 1.0) * 65535.0).round() as u16;
        let b = (rgb[i * 3 + 2].clamp(0.0, 1.0) * 65535.0).round() as u16;
        let x = i as u32 % width;
        let y = i as u32 / width;
        img.put_pixel(x, y, Rgba([r, g, b, 65535]));
    }
    img
}

/// 蒙版可视化（0..1 → 灰度）。
pub fn mask_to_image(alpha: &[f32], width: u32, height: u32) -> Image16 {
    use image::Rgba;
    let mut img = Image16::new(width, height);
    let n = (width * height) as usize;
    for i in 0..n.min(alpha.len()) {
        let v = (alpha[i].clamp(0.0, 1.0) * 65535.0).round() as u16;
        let x = i as u32 % width;
        let y = i as u32 / width;
        img.put_pixel(x, y, Rgba([v, v, v, 65535]));
    }
    img
}

/// 质量报告是否阻断（过度磨皮 / 保护泄漏严重）。
pub fn quality_blocks(report: &QualityReport) -> bool {
    report.over_smooth_risk >= 1.0 || report.protected_leak >= 1.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_engine::face_frame::{FaceFrame, NormPoint};
    use crate::ai_engine::skin_segmentation::build_skin_mask;
    use crate::image_engine::params::ManualMask;
    use image::Rgba;

    fn face() -> FaceFrame {
        FaceFrame {
            id: 1,
            uuid: "s1".into(),
            score: 0.95,
            x: 0.25,
            y: 0.2,
            width: 0.5,
            height: 0.55,
            landmarks: vec![
                NormPoint { x: 0.35, y: 0.35 },
                NormPoint { x: 0.65, y: 0.35 },
                NormPoint { x: 0.5, y: 0.48 },
                NormPoint { x: 0.4, y: 0.62 },
                NormPoint { x: 0.6, y: 0.62 },
            ],
            dense: None,
            level3: None,
        }
    }

    fn synthetic() -> Image16 {
        let mut img: Image16 = Image16::from_pixel(96, 96, Rgba([42_000, 30_000, 26_000, 65535]));
        for y in 15..80 {
            for x in 15..80 {
                let noise = (((x * 13 + y * 7) % 11) as i32 - 5) * 1200;
                let p = img.get_pixel_mut(x, y);
                for c in 0..3 {
                    p.0[c] = ((p.0[c] as i32 + noise).clamp(0, 65535)) as u16;
                }
            }
        }
        // 高对比“睫毛”条
        for x in 30..66 {
            img.put_pixel(x, 32, Rgba([8000, 6000, 6000, 65535]));
        }
        img
    }

    #[test]
    fn engine_runs_and_reduces_noise_without_killing_structure() {
        let mut img = synthetic();
        let before = img.clone();
        let mask = build_skin_mask(&img, &face(), &ManualMask::default()).unwrap();
        let geometry = FaceGeometry::from_face(&face(), img.width(), img.height()).unwrap();
        let beauty = BeautyParams {
            skin_smooth: 55.0,
            texture: 45.0,
            detail: 40.0,
            tone_even: 20.0,
            ..BeautyParams::default()
        };
        let out = run_skin_retouch(&mut img, &mask, &geometry, &beauty, 0.0, &CancelToken::default())
            .expect("engine");
        assert!(out.fused_pixels > 50, "应融合到像素");
        assert!(!out.frequency.is_none());
        // 皮肤区应有变化
        let mut changed = 0u32;
        for y in 25..70 {
            for x in 25..70 {
                if img.get_pixel(x, y) != before.get_pixel(x, y) {
                    changed += 1;
                }
            }
        }
        assert!(changed > 30, "皮肤区应变化 {changed}");
        // 睫毛高线条附近变化应受保护（保护权重）——至少不应整图抹掉
        let lash_before = before.get_pixel(40, 32);
        let lash_after = img.get_pixel(40, 32);
        let lash_delta = (lash_after.0[0] as i32 - lash_before.0[0] as i32).abs();
        assert!(
            lash_delta < 12_000,
            "强边缘不应被抹平 Δ={lash_delta}"
        );
        assert!(!quality_blocks(&out.quality) || out.quality.over_smooth_risk < 1.0);
    }

    #[test]
    fn zero_smooth_is_identity() {
        let mut img = synthetic();
        let before = img.clone();
        let mask = build_skin_mask(&img, &face(), &ManualMask::default()).unwrap();
        let geometry = FaceGeometry::from_face(&face(), img.width(), img.height()).unwrap();
        let beauty = BeautyParams::default();
        run_skin_retouch(&mut img, &mask, &geometry, &beauty, 0.0, &CancelToken::default()).unwrap();
        assert_eq!(img.into_raw(), before.into_raw());
    }
}
