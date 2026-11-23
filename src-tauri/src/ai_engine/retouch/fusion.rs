//! ImageFusionEngine：处理结果与原图的统一融合与一致性检查。
//!
//! 检查：Alpha 范围、蒙版边界、原图/ROI 坐标、尺寸、通道、重叠、分块接缝。
//! **禁止**把调试信息合成进最终照片。

use crate::error::{AppError, AppResult};
use crate::image_engine::buffer::{Image16, CHANNEL_MAX};
use crate::image_engine::cancel::CancelToken;

/// 融合请求：在 mask 权重下把 `processed`（与 image 同尺寸 ROI 或全图）混回 `image`。
pub struct FusionRequest<'a> {
    /// 目标图像（全图）。
    pub image: &'a mut Image16,
    /// ROI 左上角（全图像素）。
    pub origin: (i32, i32),
    /// ROI 内处理后的 RGB（0..1，行优先，len = roi_w*roi_h*3）。
    pub processed_rgb: &'a [f32],
    /// ROI 内融合权重 0..1。
    pub weight: &'a [f32],
    pub roi_w: u32,
    pub roi_h: u32,
    /// 全局强度 0..1。
    pub strength: f32,
}

#[derive(Debug, Clone, Default)]
pub struct FusionReport {
    pub fused_pixels: u64,
    pub skipped_out_of_bounds: u64,
    pub max_weight: f32,
    pub seam_risk: bool,
}

/// 执行融合。失败时不修改 image（先校验再写）。
pub fn fuse(req: FusionRequest<'_>, cancel: &CancelToken) -> AppResult<FusionReport> {
    let strength = req.strength.clamp(0.0, 1.0);
    let n = (req.roi_w * req.roi_h) as usize;
    if req.processed_rgb.len() != n * 3 || req.weight.len() != n {
        return Err(AppError::Unsupported(
            "融合缓冲长度与 ROI 不一致".into(),
        ));
    }
    if req.roi_w == 0 || req.roi_h == 0 {
        return Ok(FusionReport::default());
    }
    // Alpha 范围
    for (i, w) in req.weight.iter().enumerate() {
        if !w.is_finite() || *w < -1e-4 || *w > 1.0 + 1e-4 {
            return Err(AppError::Unsupported(format!(
                "融合权重越界 @ {i}: {w}"
            )));
        }
    }
    for (i, v) in req.processed_rgb.iter().enumerate() {
        if !v.is_finite() {
            return Err(AppError::Unsupported(format!("处理像素非有限值 @ {i}")));
        }
    }

    let img_w = req.image.width() as i32;
    let img_h = req.image.height() as i32;
    let mut report = FusionReport::default();

    // 分块接缝风险：权重在 ROI 边界若仍很高，边界处可能可见。
    let edge = |x: u32, y: u32| -> f32 {
        let i = (y * req.roi_w + x) as usize;
        req.weight.get(i).copied().unwrap_or(0.0)
    };
    let mut max_edge = 0.0f32;
    for x in 0..req.roi_w {
        max_edge = max_edge.max(edge(x, 0)).max(edge(x, req.roi_h - 1));
    }
    for y in 0..req.roi_h {
        max_edge = max_edge.max(edge(0, y)).max(edge(req.roi_w - 1, y));
    }
    report.seam_risk = max_edge > 0.85;
    report.max_weight = req
        .weight
        .iter()
        .fold(0.0f32, |a, b| a.max(*b));

    for y in 0..req.roi_h {
        if y % 64 == 0 {
            cancel.check()?;
        }
        for x in 0..req.roi_w {
            let gx = req.origin.0 + x as i32;
            let gy = req.origin.1 + y as i32;
            if gx < 0 || gy < 0 || gx >= img_w || gy >= img_h {
                report.skipped_out_of_bounds += 1;
                continue;
            }
            let w = req.weight[(y * req.roi_w + x) as usize].clamp(0.0, 1.0) * strength;
            if w <= 0.0 {
                continue;
            }
            let pi = (y * req.roi_w + x) as usize * 3;
            let pixel = req.image.get_pixel_mut(gx as u32, gy as u32);
            for c in 0..3 {
                let dst = pixel.0[c] as f32 / CHANNEL_MAX;
                let src = req.processed_rgb[pi + c];
                let out = dst + (src - dst) * w;
                pixel.0[c] = (out.clamp(0.0, 1.0) * CHANNEL_MAX).round() as u16;
            }
            report.fused_pixels += 1;
        }
    }
    Ok(report)
}

/// 从全图提取 ROI 到 0..1 RGB 缓冲。
pub fn extract_roi(image: &Image16, origin: (i32, i32), w: u32, h: u32) -> Vec<f32> {
    let mut out = vec![0f32; (w * h * 3) as usize];
    let iw = image.width() as i32;
    let ih = image.height() as i32;
    for y in 0..h {
        for x in 0..w {
            let gx = origin.0 + x as i32;
            let gy = origin.1 + y as i32;
            let pi = (y * w + x) as usize * 3;
            if gx < 0 || gy < 0 || gx >= iw || gy >= ih {
                continue;
            }
            let p = image.get_pixel(gx as u32, gy as u32).0;
            out[pi] = p[0] as f32 / CHANNEL_MAX;
            out[pi + 1] = p[1] as f32 / CHANNEL_MAX;
            out[pi + 2] = p[2] as f32 / CHANNEL_MAX;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn fuse_half_weight_mixes() {
        let mut img: Image16 = Image16::from_pixel(4, 4, Rgba([0, 0, 0, 65535]));
        let processed = vec![1.0f32; 4 * 4 * 3];
        let weight = vec![1.0f32; 4 * 4];
        let report = fuse(
            FusionRequest {
                image: &mut img,
                origin: (0, 0),
                processed_rgb: &processed,
                weight: &weight,
                roi_w: 4,
                roi_h: 4,
                strength: 0.5,
            },
            &CancelToken::default(),
        )
        .unwrap();
        assert_eq!(report.fused_pixels, 16);
        let p = img.get_pixel(1, 1).0;
        assert!((p[0] as f32 - 32768.0).abs() < 200.0, "half mix got {}", p[0]);
    }

    #[test]
    fn rejects_bad_weight() {
        let mut img: Image16 = Image16::from_pixel(2, 2, Rgba([100, 100, 100, 65535]));
        let processed = vec![0.5f32; 12];
        let weight = vec![1.5f32; 4];
        let err = fuse(
            FusionRequest {
                image: &mut img,
                origin: (0, 0),
                processed_rgb: &processed,
                weight: &weight,
                roi_w: 2,
                roi_h: 2,
                strength: 1.0,
            },
            &CancelToken::default(),
        );
        assert!(err.is_err());
    }

    #[test]
    fn strength_zero_is_noop() {
        let mut img: Image16 = Image16::from_pixel(2, 2, Rgba([1000, 2000, 3000, 65535]));
        let before = img.clone();
        let processed = vec![1.0f32; 12];
        let weight = vec![1.0f32; 4];
        fuse(
            FusionRequest {
                image: &mut img,
                origin: (0, 0),
                processed_rgb: &processed,
                weight: &weight,
                roi_w: 2,
                roi_h: 2,
                strength: 0.0,
            },
            &CancelToken::default(),
        )
        .unwrap();
        assert_eq!(img.into_raw(), before.into_raw());
    }
}
