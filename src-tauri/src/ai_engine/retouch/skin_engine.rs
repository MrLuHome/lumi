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
    // 从原图扩展真实像素 Halo 做引导滤波，再裁回 ROI（禁止 ROI 裁剪后直接滤波产生矩形接缝）。
    let (mut low, high) = split_planes(image, mask, &rgb, radius, eps, cancel)?;
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

    // 5) 质量：before = 原 ROI，after = 处理后 rgb；
    //    skin_w 传原始皮肤蒙版（非 process_w），否则 protect=0 的五官区
    //    会因 process_w=0 被排除在保护区之外，保护泄漏永远测不到。
    let before = fusion::extract_roi(image, (mask.x0, mask.y0), mw, mh);
    let split_stub = FrequencySplit {
        low,
        high,
        width: mw,
        height: mh,
    };
    let q = quality::evaluate(&before, &rgb, &mask.alpha, &protect, mw, mh);

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
///
/// **Halo 策略（S1.1 P1）**：不直接对裁剪 ROI 跑 `guided_filter`（ROI 边界窗口
/// 截断会在脸颊/额头/下巴边缘产生矩形接缝），而是：
/// 1. 从**原图**按 mask 包围盒外扩 `pad = min(2r+4, 40)` 像素读取真实像素
///    （图像四边界统一用边缘复制，禁止 0/黑色填充）；
/// 2. 在含 Halo 的平面上执行引导滤波；
/// 3. 裁回目标 ROI，`high = roi - low`。
///
/// `roi` 为与 mask ROI 同尺寸的当前像素（0..1 RGB，行优先）。
fn split_planes(
    image: &crate::image_engine::buffer::Image16,
    mask: &SkinMask,
    roi: &[f32],
    radius: u32,
    eps: f32,
    cancel: &CancelToken,
) -> AppResult<([Vec<f32>; 3], [Vec<f32>; 3])> {
    use crate::ai_engine::retouch::filter::{guided_filter, RgbPlanes};
    let (mw, mh) = (mask.width, mask.height);
    let n = (mw * mh) as usize;
    if mw < 2 || mh < 2 || roi.len() != n * 3 {
        return Err(crate::error::AppError::Unsupported(
            "频率分离缓冲尺寸与 ROI 不一致".into(),
        ));
    }
    // 引导滤波嵌套两次盒滤，输出像素依赖半径 2r；Halo ≥ 2r 即与全图滤波一致。
    let pad = (radius * 2 + 4).min(40);
    let padded = RgbPlanes::from_image_padded(image, mask, pad);
    let (fw, fh) = (padded.width, padded.height);
    let ox = (fw - mw) / 2;
    let oy = (fh - mh) / 2;
    let mut low: [Vec<f32>; 3] = [vec![0f32; n], vec![0f32; n], vec![0f32; n]];
    let mut high: [Vec<f32>; 3] = [vec![0f32; n], vec![0f32; n], vec![0f32; n]];
    for c in 0..3 {
        cancel.check()?;
        let filtered = guided_filter(&padded.planes[c], fw, fh, radius, eps);
        for y in 0..mh {
            for x in 0..mw {
                let l = filtered[((y + oy) * fw + (x + ox)) as usize];
                let i = (y * mw + x) as usize;
                low[c][i] = l;
                high[c][i] = roi[i * 3 + c] - l;
            }
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

    // —— S1.1 P1：频率分离真实 Halo ——

    fn ramp_image(w: u32, h: u32) -> Image16 {
        let mut img = Image16::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let r = (x as f32 / w as f32 * 60000.0) as u16;
                let g = (y as f32 / h as f32 * 50000.0) as u16;
                let b = 20_000u16;
                img.put_pixel(x, y, Rgba([r, g, b, 65535]));
            }
        }
        img
    }

    fn manual_mask(x0: i32, y0: i32, w: u32, h: u32) -> SkinMask {
        SkinMask {
            x0,
            y0,
            width: w,
            height: h,
            alpha: vec![1.0f32; (w * h) as usize],
        }
    }

    fn roi_of(img: &Image16, mask: &SkinMask) -> Vec<f32> {
        let mut out = vec![0f32; (mask.width * mask.height * 3) as usize];
        for y in 0..mask.height {
            for x in 0..mask.width {
                let gx = (mask.x0 + x as i32) as u32;
                let gy = (mask.y0 + y as i32) as u32;
                let p = img.get_pixel(gx, gy).0;
                let i = ((y * mask.width + x) as usize) * 3;
                out[i] = p[0] as f32 / 65535.0;
                out[i + 1] = p[1] as f32 / 65535.0;
                out[i + 2] = p[2] as f32 / 65535.0;
            }
        }
        out
    }

    #[test]
    fn split_planes_halo_matches_full_frame_guided_filter() {
        // 内部 ROI：带 Halo 的 split 应与「全图引导滤波后裁剪」一致（消除矩形接缝的定义）。
        use crate::ai_engine::retouch::filter::guided_filter;
        let img = ramp_image(100, 100);
        let mask = manual_mask(30, 30, 40, 40);
        let roi = roi_of(&img, &mask);
        let radius = 6u32;
        let eps = 0.004f32;
        let (low, high) = split_planes(&img, &mask, &roi, radius, eps, &CancelToken::default())
            .unwrap();

        // 参考：全图平面引导滤波 → 裁 ROI。
        for c in 0..3 {
            let mut plane = vec![0f32; 100 * 100];
            for y in 0..100u32 {
                for x in 0..100u32 {
                    plane[(y * 100 + x) as usize] =
                        img.get_pixel(x, y).0[c] as f32 / 65535.0;
                }
            }
            let full_low = guided_filter(&plane, 100, 100, radius, eps);
            let mut max_err = 0.0f32;
            for y in 0..40u32 {
                for x in 0..40u32 {
                    let got = low[c][(y * 40 + x) as usize];
                    let want = full_low[((y + 30) * 100 + (x + 30)) as usize];
                    max_err = max_err.max((got - want).abs());
                }
            }
            assert!(
                max_err < 1e-4,
                "Halo split 应与全图引导滤波一致 c={c} max_err={max_err}"
            );
        }
        // high = roi - low 校验
        for i in 0..40 * 40 {
            for c in 0..3 {
                let expect = roi[i * 3 + c] - low[c][i];
                assert!((high[c][i] - expect).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn split_planes_no_rectangular_seam_on_ramp() {
        // 纯渐变图：ROI 内 low 应保持线性，边界列不得被拉平（矩形接缝）。
        let img = ramp_image(96, 96);
        let mask = manual_mask(24, 24, 48, 48);
        let roi = roi_of(&img, &mask);
        let (low, _high) = split_planes(
            &img,
            &mask,
            &roi,
            6,
            0.004,
            &CancelToken::default(),
        )
        .unwrap();
        // R 通道沿 x 线性：low 应贴合理想斜坡。
        let k = 60000.0f32 / 65535.0;
        let r0 = 24.0f32 / 96.0;
        let r1 = 71.0f32 / 96.0;
        for y in 0..48u32 {
            for x in 0..48u32 {
                let t = x as f32 / 47.0;
                let want = (r0 + (r1 - r0) * t) * k;
                let got = low[0][(y * 48 + x) as usize];
                assert!(
                    (got - want).abs() < 0.02,
                    "ROI 边界不得出现矩形接缝 ({x},{y}) got={got:.4} want={want:.4}"
                );
            }
        }
        // 列间梯度连续：首/末列跳变不得显著大于内部中位差。
        let col_mean = |x: usize| -> f32 {
            (0..48).map(|y| low[0][y * 48 + x]).sum::<f32>() / 48.0
        };
        let mut jumps = Vec::new();
        for x in 1..48 {
            jumps.push((col_mean(x) - col_mean(x - 1)).abs());
        }
        let mut sorted = jumps.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = sorted[sorted.len() / 2];
        let limit = (median * 4.0).max(0.005);
        assert!(
            jumps[0] <= limit && jumps[jumps.len() - 1] <= limit,
            "列梯度在 ROI 边界不得突变 first={:?} last={:?} median={median} limit={limit}",
            jumps[0],
            jumps[jumps.len() - 1]
        );
    }

    #[test]
    fn split_planes_corner_roi_edge_policy_consistent() {
        // ROI 触及图像左上角：边缘复制策略，不得产生 NaN/黑洞，且保持渐变趋势。
        let img = ramp_image(64, 64);
        let mask = manual_mask(0, 0, 32, 32);
        let roi = roi_of(&img, &mask);
        let (low, high) = split_planes(
            &img,
            &mask,
            &roi,
            5,
            0.004,
            &CancelToken::default(),
        )
        .unwrap();
        for c in 0..3 {
            for v in low[c].iter().chain(high[c].iter()) {
                assert!(v.is_finite(), "角 ROI 输出必须有限");
            }
        }
        // R 沿 x 上升、G 沿 y 上升（斜坡不被拉平）
        let r_span = low[0][31] - low[0][0];
        let g_span = low[1][31 * 32] - low[1][0];
        assert!(r_span > 0.05, "R 斜坡应保留 span={r_span}");
        assert!(g_span > 0.05, "G 斜坡应保留 span={g_span}");
        // B 为常量：不得被填充为黑色
        let b_mean = low[2].iter().sum::<f32>() / low[2].len() as f32;
        let want_b = 20_000.0f32 / 65535.0;
        assert!(
            (b_mean - want_b).abs() < 0.02,
            "常量通道不得被黑填充 b_mean={b_mean} want={want_b}"
        );
    }
}
