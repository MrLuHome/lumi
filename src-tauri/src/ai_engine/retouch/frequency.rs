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
    // 局部均值（大半径盒滤）
    let radius = ((width as f32 * 0.04).round() as u32).clamp(6, 40);
    let mut means: [Vec<f32>; 3] = [
        vec![0f32; n],
        vec![0f32; n],
        vec![0f32; n],
    ];
    for c in 0..3 {
        cancel.check()?;
        means[c] = box_mean(&low[c], width, height, radius);
    }
    for i in 0..n {
        let m = mask_alpha[i].clamp(0.0, 1.0);
        if m <= 0.02 {
            continue;
        }
        let k = amount * m * 0.55;
        for c in 0..3 {
            low[c][i] += (means[c][i] - low[c][i]) * k;
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
