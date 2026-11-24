//! 频率分离：低频（颜色/光影）与高频（毛孔/纹理）分解与重组。
//!
//! 独立设计：低频用带 halo 的引导/盒式平滑得到 base；高频 = src − base。
//! 处理低频做局部均匀，高频按纹理保护比例回注；禁止把五官边缘当皮肤纹理抹掉。

use crate::ai_engine::retouch::filter::{box_mean, guided_filter, RgbPlanes};
use crate::error::AppResult;
use crate::image_engine::cancel::CancelToken;
use crate::ai_engine::skin_segmentation::SkinMask;

/// 频率分离结果（ROI 尺寸，0..1）。
pub struct FrequencySplit {
    pub low: [Vec<f32>; 3],
    pub high: [Vec<f32>; 3],
    pub width: u32,
    pub height: u32,
}

/// 分解：对 mask ROI（带 pad）做引导滤波得到低频。
pub fn split(
    image: &crate::image_engine::buffer::Image16,
    mask: &SkinMask,
    radius: u32,
    eps: f32,
    cancel: &CancelToken,
) -> AppResult<FrequencySplit> {
    let pad = (radius * 2 + 4).min(40);
    let padded = RgbPlanes::from_image_padded(image, mask, pad);
    let (fw, fh) = (padded.width, padded.height);
    let mut low_p: [Vec<f32>; 3] = [
        vec![0f32; (fw * fh) as usize],
        vec![0f32; (fw * fh) as usize],
        vec![0f32; (fw * fh) as usize],
    ];
    for c in 0..3 {
        cancel.check()?;
        low_p[c] = guided_filter(&padded.planes[c], fw, fh, radius, eps);
    }
    // 裁回 mask 尺寸
    let (mw, mh) = (mask.width, mask.height);
    let crop = |src: &[f32]| -> Vec<f32> {
        let mut out = vec![0f32; (mw * mh) as usize];
        let ox = ((fw - mw) / 2) as u32;
        let oy = ((fh - mh) / 2) as u32;
        for y in 0..mh {
            for x in 0..mw {
                out[(y * mw + x) as usize] = src[((y + oy) * fw + x + ox) as usize];
            }
        }
        out
    };
    let cropped: [Vec<f32>; 3] = [crop(&low_p[0]), crop(&low_p[1]), crop(&low_p[2])];
    let src = RgbPlanes::from_image(image, mask);
    let mut high: [Vec<f32>; 3] = [
        vec![0f32; (mw * mh) as usize],
        vec![0f32; (mw * mh) as usize],
        vec![0f32; (mw * mh) as usize],
    ];
    for c in 0..3 {
        for i in 0..(mw * mh) as usize {
            high[c][i] = src.planes[c][i] - cropped[c][i];
        }
    }
    Ok(FrequencySplit {
        low: cropped,
        high,
        width: mw,
        height: mh,
    })
}

/// 低频局部均匀：向 ROI 内皮肤均值轻微靠拢（保大面积光影方向）。
/// 低频均匀（ROI 版）：盒滤窗口先做**边缘复制外扩**再取均值，
/// 杜绝 ROI 边界截断窗口（S1.2.2 §4.1）。需要与全图完全一致的
/// 真实像素 Halo 时请用 [`even_low_frequency_padded`]（引擎路径）。
pub fn even_low_frequency_wh(
    low: &mut [Vec<f32>; 3],
    width: u32,
    height: u32,
    mask_alpha: &[f32],
    amount: f32,
    cancel: &CancelToken,
) -> AppResult<()> {
    let amount = amount.clamp(0.0, 1.0);
    if amount <= 0.0 || width < 3 || height < 3 {
        return Ok(());
    }
    let n = (width * height) as usize;
    if mask_alpha.len() != n || low[0].len() != n {
        return Err(crate::error::AppError::Unsupported(
            "频率分离缓冲尺寸不一致".into(),
        ));
    }
    let radius = ((width as f32 * 0.04).round() as u32).clamp(6, 40);
    // 边缘复制外扩 radius：窗口在 ROI 边界不再被截断。
    let pad = radius as usize;
    let pw = width as usize + pad * 2;
    let ph = height as usize + pad * 2;
    for c in 0..3 {
        cancel.check()?;
        let mut extended = vec![0f32; pw * ph];
        for y in 0..ph {
            let sy = (y as i64 - pad as i64).clamp(0, height as i64 - 1) as usize;
            for x in 0..pw {
                let sx = (x as i64 - pad as i64).clamp(0, width as i64 - 1) as usize;
                extended[y * pw + x] = low[c][sy * width as usize + sx];
            }
        }
        let means = box_mean(&extended, pw as u32, ph as u32, radius);
        for y in 0..height as usize {
            for x in 0..width as usize {
                let i = y * width as usize + x;
                let m = mask_alpha[i].clamp(0.0, 1.0);
                if m <= 0.02 {
                    continue;
                }
                let k = amount * m * 0.55;
                let mean = means[(y + pad) * pw + (x + pad)];
                low[c][i] += (mean - low[c][i]) * k;
            }
        }
    }
    Ok(())
}

/// 低频均匀（**真实像素 Halo 版**，S1.2.2 §4.1 引擎路径）。
///
/// `low_padded` 是频率分离保留下来的、已外扩 `pad` 的低频平面（ROI 原点为
/// `(roi_x, roi_y)`）；盒滤在整张 padded 平面上执行，ROI 像素的窗口完全落在
/// 真实 Halo 内 → 与「全图滤波后裁剪」一致。`mask` 为 ROI 尺寸处理权重。
pub fn even_low_frequency_padded(
    low_padded: &mut [Vec<f32>; 3],
    padded_w: u32,
    padded_h: u32,
    roi_x: u32,
    roi_y: u32,
    roi_w: u32,
    roi_h: u32,
    mask: &[f32],
    amount: f32,
    cancel: &CancelToken,
) -> AppResult<()> {
    let amount = amount.clamp(0.0, 1.0);
    if amount <= 0.0 || roi_w < 3 || roi_h < 3 {
        return Ok(());
    }
    let n = (roi_w * roi_h) as usize;
    if mask.len() != n {
        return Err(crate::error::AppError::Unsupported(
            "低频均匀 mask 尺寸与 ROI 不一致".into(),
        ));
    }
    if (roi_x + roi_w > padded_w) || (roi_y + roi_h > padded_h) {
        return Err(crate::error::AppError::Unsupported(
            "低频均匀 ROI 越出 padded 平面".into(),
        ));
    }
    let radius = ((roi_w as f32 * 0.04).round() as u32).clamp(6, 40);
    // ROI 距 padded 边界应 ≥ radius（split 的 pad = 2r+4）；不足时也安全：
    // box_mean 在 padded 边界的截断只影响 pad<radius 的极端构造。
    let mut means: [Vec<f32>; 3] = [
        vec![0f32; (padded_w * padded_h) as usize],
        vec![0f32; (padded_w * padded_h) as usize],
        vec![0f32; (padded_w * padded_h) as usize],
    ];
    for c in 0..3 {
        cancel.check()?;
        means[c] = box_mean(&low_padded[c], padded_w, padded_h, radius);
    }
    for y in 0..roi_h as usize {
        for x in 0..roi_w as usize {
            let i = y * roi_w as usize + x;
            let m = mask[i].clamp(0.0, 1.0);
            if m <= 0.02 {
                continue;
            }
            let k = amount * m * 0.55;
            let pi = ((y + roi_y as usize) * padded_w as usize) + (x + roi_x as usize);
            for c in 0..3 {
                let v = low_padded[c][pi];
                let mv = means[c][pi];
                low_padded[c][pi] = v + (mv - v) * k;
            }
        }
    }
    Ok(())
}

/// 重组：low' + high * high_keep（由 texture/detail 控制）。
pub fn recombine(
    split: &FrequencySplit,
    high_keep: f32,
    mask_alpha: &[f32],
) -> ([Vec<f32>; 3], Vec<f32>) {
    let n = split.width as usize * split.height as usize;
    let keep = high_keep.clamp(0.0, 1.0);
    let mut out: [Vec<f32>; 3] = [
        vec![0f32; n],
        vec![0f32; n],
        vec![0f32; n],
    ];
    let mut blend = vec![0f32; n];
    for i in 0..n {
        let m = mask_alpha.get(i).copied().unwrap_or(1.0).clamp(0.0, 1.0);
        blend[i] = m;
        for c in 0..3 {
            out[c][i] = split.low[c][i] + split.high[c][i] * keep;
        }
    }
    (out, blend)
}

/// 高频能量（用于质量报告）。
pub fn high_frequency_energy(high: &[Vec<f32>; 3]) -> f32 {
    let n = high[0].len();
    if n == 0 {
        return 0.0;
    }
    let mut sum = 0.0;
    for i in 0..n {
        let e = (high[0][i].abs() + high[1][i].abs() + high[2][i].abs()) / 3.0;
        sum += e;
    }
    sum / n as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_engine::face_frame::{FaceFrame, NormPoint};
    use crate::ai_engine::skin_segmentation::build_skin_mask;
    use crate::image_engine::buffer::Image16;
    use crate::image_engine::cancel::CancelToken;
    use crate::image_engine::params::ManualMask;
    use image::Rgba;

    fn face() -> FaceFrame {
        FaceFrame {
            id: 0,
            uuid: "t".into(),
            score: 0.9,
            x: 0.2,
            y: 0.2,
            width: 0.6,
            height: 0.6,
            landmarks: vec![
                NormPoint { x: 0.35, y: 0.35 },
                NormPoint { x: 0.65, y: 0.35 },
                NormPoint { x: 0.5, y: 0.48 },
                NormPoint { x: 0.4, y: 0.6 },
                NormPoint { x: 0.6, y: 0.6 },
            ],
            dense: None,
            level3: None,
        }
    }

    #[test]
    fn split_then_recombine_with_full_keep_is_close_to_source() {
        let mut img: Image16 = Image16::from_pixel(64, 64, Rgba([40_000, 30_000, 25_000, 65535]));
        // 加一点纹理
        for y in 10..50 {
            for x in 10..50 {
                let p = img.get_pixel_mut(x, y);
                let noise = (((x + y) % 3) as i32) * 800;
                p.0[0] = ((p.0[0] as i32 + noise).clamp(0, 65535)) as u16;
            }
        }
        let mask = build_skin_mask(&img, &face(), &ManualMask::default()).unwrap();
        let split = split(&img, &mask, 4, 0.004, &CancelToken::default()).unwrap();
        assert_eq!(split.width, mask.width);
        let alpha = mask.alpha.clone();
        let (out, blend) = recombine(&split, 1.0, &alpha);
        // keep=1 → out ≈ low+high = source（在 mask ROI 内）
        let src = crate::ai_engine::retouch::filter::RgbPlanes::from_image(&img, &mask);
        let mut max_err = 0.0f32;
        for i in 0..alpha.len() {
            if alpha[i] < 0.5 {
                continue;
            }
            for c in 0..3 {
                max_err = max_err.max((out[c][i] - src.planes[c][i]).abs());
            }
        }
        assert!(max_err < 0.02, "recombine(keep=1) 应接近原图 max_err={max_err}");
        assert!(blend.iter().any(|b| *b > 0.5));
    }

    #[test]
    fn zero_amount_even_is_noop() {
        let mut low = [vec![0.5f32; 16], vec![0.5f32; 16], vec![0.5f32; 16]];
        let before = low[0].clone();
        even_low_frequency_wh(&mut low, 4, 4, &[1.0; 16], 0.0, &CancelToken::default()).unwrap();
        assert_eq!(low[0], before);
    }

    /// 构造平滑低频场（供全图参考）。
    fn smooth_field(w: u32, h: u32) -> Vec<f32> {
        let mut v = vec![0f32; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                v[(y * w + x) as usize] = 0.5
                    + 0.10 * ((x as f32 / 13.0).sin())
                    + 0.08 * ((y as f32 / 11.0).cos())
                    + 0.05 * (x as f32 / w as f32);
            }
        }
        v
    }

    /// S1.2.2 §4.1：padded even_low 在 ROI 内部与 ROI 边缘都必须与
    /// 「全图滤波后裁剪」一致（真实 Halo，无窗口截断）。
    #[test]
    fn even_low_padded_matches_full_field_reference() {
        let (fw, fh) = (120u32, 120u32);
        let full = smooth_field(fw, fh);
        let radius = ((40u32 as f32 * 0.04).round() as u32).clamp(6, 40); // roi_w=40 → radius6
        // 全图参考：box_mean(full) 后按 k=0.55 混合。
        let means_full = box_mean(&full, fw, fh, radius);
        let k = 0.55f32;
        // ROI（内部，距边界 ≥ radius）
        let (rx, ry, rw, rh) = (40u32, 40u32, 40u32, 40u32);
        let pad = (radius as i32) + 4;
        let mut padded = [Vec::new(), Vec::new(), Vec::new()];
        for c in 0..3 {
            let mut plane = vec![0f32; ((rw as i32 + pad * 2) * (rh as i32 + pad * 2)) as usize];
            let pw = (rw as i32 + pad * 2) as u32;
            for y in 0..(rh as i32 + pad * 2) as u32 {
                for x in 0..(rw as i32 + pad * 2) as u32 {
                    let sx = (rx as i32 + x as i32 - pad).clamp(0, fw as i32 - 1) as u32;
                    let sy = (ry as i32 + y as i32 - pad).clamp(0, fh as i32 - 1) as u32;
                    plane[(y * pw + x) as usize] = full[(sy * fw + sx) as usize];
                }
            }
            padded[c] = plane;
        }
        let pw = rw + (pad as u32) * 2;
        let ph = rh + (pad as u32) * 2;
        let mask = vec![1.0f32; (rw * rh) as usize];
        even_low_frequency_padded(
            &mut padded,
            pw,
            ph,
            pad as u32,
            pad as u32,
            rw,
            rh,
            &mask,
            1.0,
            &CancelToken::default(),
        )
        .unwrap();
        let mut max_err = 0.0f32;
        for y in 0..rh {
            for x in 0..rw {
                let pi = ((y + pad as u32) * pw + (x + pad as u32)) as usize;
                let gi = ((ry + y) * fw + (rx + x)) as usize;
                let got = padded[0][pi];
                let want = full[gi] + (means_full[gi] - full[gi]) * k;
                max_err = max_err.max((got - want).abs());
            }
        }
        assert!(
            max_err < 1e-4,
            "padded even_low 应与全图滤波裁剪一致（含 ROI 边缘）max_err={max_err}"
        );
    }

    /// 修复前后对比：旧实现（截断窗口）在 ROI 边缘有偏差，新实现（Halo）没有。
    #[test]
    fn even_low_halo_fixes_roi_edge_truncation() {
        let (fw, fh) = (120u32, 120u32);
        let full = smooth_field(fw, fh);
        let radius = ((40u32 as f32 * 0.04).round() as u32).clamp(6, 40);
        let means_full = box_mean(&full, fw, fh, radius);
        let k = 0.55f32;
        let (rx, ry, rw, rh) = (40u32, 40u32, 40u32, 40u32);
        // 旧路径：直接裁剪 ROI 后 box_mean（截断）
        let mut roi_old = [Vec::new(), Vec::new(), Vec::new()];
        for c in 0..3 {
            let mut plane = vec![0f32; (rw * rh) as usize];
            for y in 0..rh {
                for x in 0..rw {
                    plane[(y * rw + x) as usize] = full[((ry + y) * fw + (rx + x)) as usize];
                }
            }
            roi_old[c] = plane;
        }
        let mask = vec![1.0f32; (rw * rh) as usize];
        even_low_frequency_wh(&mut roi_old, rw, rh, &mask, 1.0, &CancelToken::default()).unwrap();
        let mut old_err = 0.0f32;
        for y in 0..rh {
            for x in 0..rw {
                let i = (y * rw + x) as usize;
                let gi = ((ry + y) * fw + (rx + x)) as usize;
                let want = full[gi] + (means_full[gi] - full[gi]) * k;
                old_err = old_err.max((roi_old[0][i] - want).abs());
            }
        }
        // 新路径（Halo）误差上限1e-4（上一测试），旧截断路径必须明显更差
        // （在本平滑场上边缘偏差 >10×）。
        assert!(
            old_err > 1e-3,
            "旧截断路径应有可见 ROI 边缘误差（对比基线失效？old_err={old_err}）"
        );
    }

    /// 图像四边界（ROI 贴角）：padded 路径与「边缘复制扩展画布上的盒滤」一致。
    #[test]
    fn even_low_padded_corner_roi_matches_replicated_reference() {
        let (fw, fh) = (60u32, 60u32);
        let full = smooth_field(fw, fh);
        let (rw, rh) = (30u32, 30u32);
        let radius = ((rw as f32 * 0.04).round() as u32).clamp(6, 40);
        let pad = radius as i32 + 4;
        // padded 源：真实值 + 图像边界外边缘复制
        let pw = rw + pad as u32 * 2;
        let ph = rh + pad as u32 * 2;
        let mut plane = vec![0f32; (pw * ph) as usize];
        for y in 0..ph {
            for x in 0..pw {
                let sx = (x as i32 - pad).clamp(0, fw as i32 - 1) as u32;
                let sy = (y as i32 - pad).clamp(0, fh as i32 - 1) as u32;
                plane[(y * pw + x) as usize] = full[(sy * fw + sx) as usize];
            }
        }
        let mut padded = [plane.clone(), plane.clone(), plane.clone()];
        let mask = vec![1.0f32; (rw * rh) as usize];
        even_low_frequency_padded(
            &mut padded,
            pw,
            ph,
            pad as u32,
            pad as u32,
            rw,
            rh,
            &mask,
            1.0,
            &CancelToken::default(),
        )
        .unwrap();
        // 独立参考：手写边缘复制盒滤（同一策略）取均值后混合
        let k = 0.55f32;
        let mut max_err = 0.0f32;
        for y in 0..rh {
            for x in 0..rw {
                let mut sum = 0.0f32;
                let mut cnt = 0.0f32;
                for dy in -(radius as i32)..=radius as i32 {
                    for dx in -(radius as i32)..=radius as i32 {
                        let sx = (x as i32 + dx).clamp(0, fw as i32 - 1) as u32;
                        let sy = (y as i32 + dy).clamp(0, fh as i32 - 1) as u32;
                        sum += full[(sy * fw + sx) as usize];
                        cnt += 1.0;
                    }
                }
                let mean = sum / cnt;
                let gi = (y * fw + x) as usize;
                let want = full[gi] + (mean - full[gi]) * k;
                let pi = ((y + pad as u32) * pw + (x + pad as u32)) as usize;
                max_err = max_err.max((padded[0][pi] - want).abs());
            }
        }
        assert!(
            max_err < 1e-4,
            "贴角 ROI 应与边缘复制策略的全图参考一致 max_err={max_err}"
        );
    }

    #[test]
    fn split_halo_matches_full_frame_guided_filter() {
        // frequency::split 的 Halo 路径应与全图引导滤波裁剪一致（内部 ROI）。
        use crate::ai_engine::retouch::filter::guided_filter;
        let mut img: Image16 = Image16::from_pixel(96, 96, Rgba([30_000, 28_000, 26_000, 65535]));
        for y in 8..88 {
            for x in 8..88 {
                let noise = (((x * 5 + y * 3) % 7) as i32 - 3) * 1500;
                let p = img.get_pixel_mut(x, y);
                p.0[0] = ((p.0[0] as i32 + noise).clamp(0, 65535)) as u16;
            }
        }
        let mask = build_skin_mask(&img, &face(), &ManualMask::default()).unwrap();
        let radius = 4u32;
        let split = split(&img, &mask, radius, 0.004, &CancelToken::default()).unwrap();
        let (mw, mh) = (mask.width, mask.height);
        let (ox, oy) = (mask.x0 as u32, mask.y0 as u32);
        for c in 0..3 {
            let mut plane = vec![0f32; 96 * 96];
            for y in 0..96u32 {
                for x in 0..96u32 {
                    plane[(y * 96 + x) as usize] = img.get_pixel(x, y).0[c] as f32 / 65535.0;
                }
            }
            let full_low = guided_filter(&plane, 96, 96, radius, 0.004);
            let mut max_err = 0.0f32;
            for y in 0..mh {
                for x in 0..mw {
                    let got = split.low[c][(y * mw + x) as usize];
                    let want = full_low[((y + oy) * 96 + (x + ox)) as usize];
                    max_err = max_err.max((got - want).abs());
                }
            }
            assert!(max_err < 1e-4, "split Halo 应匹配全图滤波 c={c} err={max_err}");
        }
    }
}
