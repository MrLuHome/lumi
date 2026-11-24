//! 纹理保护：五官、发际/轮廓、高对比细节（睫毛/发丝/毛孔边缘）的保护权重。
//!
//! 由**实际图像特征 + 区域蒙版**决定，禁止固定矩形排除区。
//! 输出 w∈[0,1]：1=允许改变，0=完全保护（与 geometry::ProtectionMask 语义一致）。

use crate::ai_engine::face_frame::FaceGeometry;
use crate::ai_engine::retouch::filter::box_mean;
use crate::ai_engine::skin_segmentation::{feature_protection, SkinMask};
use crate::error::AppResult;
use crate::image_engine::buffer::Image16;
use crate::image_engine::cancel::CancelToken;

/// 构建纹理保护权重（与 mask ROI 同尺寸）。
///
/// w = feature_protect × edge_keep × skin_soft_edge
/// - feature_protect：眼/眉/唇/鼻孔（几何）
/// - edge_keep：局部梯度过大处（发丝、强边缘）降权
/// - 蒙版外侧已是 0（由 skin mask 控制），这里补轮廓羽化
pub fn build_texture_protection(
    image: &Image16,
    mask: &SkinMask,
    geometry: &FaceGeometry,
    cancel: &CancelToken,
) -> AppResult<Vec<f32>> {
    let (w, h) = (mask.width, mask.height);
    let n = (w * h) as usize;
    if w < 3 || h < 3 {
        return Ok(vec![1.0; n]);
    }
    let feature = feature_protection(geometry, w, h, mask.x0, mask.y0);

    // 局部亮度梯度（3×3 近似）：高梯度 = 发丝/睫毛/强轮廓 → 保护。
    let mut luma = vec![0f32; n];
    for y in 0..h {
        for x in 0..w {
            let gx = mask.x0 as u32 + x;
            let gy = mask.y0 as u32 + y;
            if gx >= image.width() || gy >= image.height() {
                continue;
            }
            let p = image.get_pixel(gx, gy).0;
            luma[(y * w + x) as usize] =
                (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) / 65535.0;
        }
    }
    // S1.2.2 §4.3：边缘检测基于 3×3 模糊亮度——过渡天然摊到 ~3px，
    // 睫毛中心仍保持深度保护，同时蒙版空间连续（旧实现基于锐利亮度时
    // 一像素内 1→0.1，max_jump≈0.91，形成光晕/不规则色块边）。
    let luma_blur = {
        let mut out = luma.clone();
        for y in 1..h.saturating_sub(1) {
            for x in 1..w.saturating_sub(1) {
                let i = (y * w + x) as usize;
                let mut s = luma[i];
                let mut c = 1.0f32;
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        let xx = x as i32 + dx;
                        let yy = y as i32 + dy;
                        if xx < 0 || yy < 0 || xx >= w as i32 || yy >= h as i32 {
                            continue;
                        }
                        s += luma[(yy as usize) * w as usize + xx as usize];
                        c += 1.0;
                    }
                }
                out[i] = s / c;
            }
        }
        out
    };
    // S1.2.2 §4.3：feature（五官椭圆）先两遍 3×3——小半径椭圆（鼻/眉 rv≈0.12）
    // 在像素分辨率下1px 跨完整个 smoothstep 区间（max_jump≈0.91）；摊开后
    // 边界过渡 ~3px，椭圆内部的深度保护不受影响。
    let mut protect = feature.clone();
    let ws = w as usize;
    let hs = h as usize;
    for _ in 0..2 {
        let src = protect.clone();
        for y in 0..hs {
            for x in 0..ws {
                let mut sum = 0.0f32;
                let mut cnt = 0.0f32;
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let xx = x as i32 + dx;
                        let yy = y as i32 + dy;
                        if xx < 0 || yy < 0 || xx >= ws as i32 || yy >= hs as i32 {
                            continue;
                        }
                        sum += src[(yy as usize) * ws + xx as usize];
                        cnt += 1.0;
                    }
                }
                protect[y * ws + x] = sum / cnt.max(1.0);
            }
        }
    }
    // 再乘 edge_keep（基于模糊亮度，睫毛中心保护深度保留）。
    for y in 1..h.saturating_sub(1) {
        if y % 32 == 0 {
            cancel.check()?;
        }
        for x in 1..w.saturating_sub(1) {
            let i = (y * w + x) as usize;
            let gx = (luma_blur[i + 1] - luma_blur[i - 1]).abs();
            let gy = (luma_blur[i + w as usize] - luma_blur[i - w as usize]).abs();
            let edge = (gx + gy) * 0.5;
            // 只保护**很强**的边缘（发丝、睫毛、轮廓），普通皮肤纹理必须放行，
            // 否则 process 权重被压没，磨皮空转（S1 联调发现 mean_w≈0.03）。
            let edge_keep = 1.0 - 0.9 * crate::image_engine::color::smoothstep(0.08, 0.30, edge);
            protect[i] = (protect[i] * edge_keep).clamp(0.0, 1.0);
        }
    }
    // 边界一圈：跟随 feature，避免未初始化。
    for x in 0..w {
        let top = x as usize;
        let bot = (h - 1) as usize * w as usize + x as usize;
        protect[top] = feature[top] * 0.85;
        protect[bot] = feature[bot] * 0.85;
    }
    for y in 0..h {
        let left = (y * w) as usize;
        let right = (y * w + (w - 1)) as usize;
        protect[left] = feature[left] * 0.85;
        protect[right] = feature[right] * 0.85;
    }
    Ok(protect)
}

/// 将保护权重与皮肤蒙版相乘得到最终处理权重。
pub fn combine_with_skin(skin_alpha: &[f32], protect: &[f32]) -> Vec<f32> {
    skin_alpha
        .iter()
        .zip(protect.iter())
        .map(|(s, p)| (s.clamp(0.0, 1.0) * p.clamp(0.0, 1.0)))
        .collect()
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
            x: 0.25,
            y: 0.25,
            width: 0.5,
            height: 0.5,
            landmarks: vec![
                NormPoint { x: 0.35, y: 0.38 },
                NormPoint { x: 0.65, y: 0.38 },
                NormPoint { x: 0.5, y: 0.5 },
                NormPoint { x: 0.42, y: 0.62 },
                NormPoint { x: 0.58, y: 0.62 },
            ],
            dense: None,
            level3: None,
        }
    }

    #[test]
    fn protection_blocks_eye_region_and_allows_cheek() {
        let img: Image16 = Image16::from_pixel(80, 80, Rgba([45_000, 32_000, 28_000, 65535]));
        let mask = build_skin_mask(&img, &face(), &ManualMask::default()).unwrap();
        let geometry = FaceGeometry::from_face(&face(), 80, 80).unwrap();
        let protect = build_texture_protection(&img, &mask, &geometry, &CancelToken::default())
            .unwrap();
        assert_eq!(protect.len(), (mask.width * mask.height) as usize);
        // 眼睛中心应显著低于脸颊（feature protect）
        let eye = geometry.eyes[0];
        let ex = (eye.0 as i32 - mask.x0).clamp(0, mask.width as i32 - 1) as u32;
        let ey = (eye.1 as i32 - mask.y0).clamp(0, mask.height as i32 - 1) as u32;
        let eye_w = protect[(ey * mask.width + ex) as usize];
        let cheek = geometry.from_local(0.55, 0.35);
        let cx = (cheek.0 as i32 - mask.x0).clamp(0, mask.width as i32 - 1) as u32;
        let cy = (cheek.1 as i32 - mask.y0).clamp(0, mask.height as i32 - 1) as u32;
        let cheek_w = protect[(cy * mask.width + cx) as usize];
        assert!(
            eye_w < cheek_w,
            "眼保护应更严 eye={eye_w} cheek={cheek_w}"
        );
        assert!(cheek_w > 0.2, "脸颊应允许处理 cheek={cheek_w}");
    }

    /// S1.2.2 §4.3：保护蒙版必须空间连续（羽化，无硬跳变），且五官处显著低于脸颊。
    #[test]
    fn protect_mask_is_spatially_continuous() {
        let img: Image16 = Image16::from_pixel(120, 120, Rgba([42_000, 30_000, 26_000, 65535]));
        let mask = build_skin_mask(&img, &face(), &ManualMask::default()).unwrap();
        let geometry = FaceGeometry::from_face(&face(), 120, 120).unwrap();
        let protect = build_texture_protection(&img, &mask, &geometry, &CancelToken::default())
            .unwrap();
        let w = mask.width as usize;
        let h = mask.height as usize;
        // 相邻像素最大跳变：smoothstep 羽化下不得出现硬边（>0.35 视为断层）。
        let mut max_jump = 0.0f32;
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if x + 1 < w {
                    max_jump = max_jump.max((protect[i + 1] - protect[i]).abs());
                }
                if y + 1 < h {
                    max_jump = max_jump.max((protect[i + w] - protect[i]).abs());
                }
            }
        }
        assert!(
            max_jump < 0.35,
            "保护蒙版存在硬跳变（可能形成不规则色块边界）max_jump={max_jump}"
        );
        // 眼睛中心保护强度（1-allow）显著高于脸颊
        let eye = geometry.eyes[0];
        let ex = (eye.0 as i32 - mask.x0).clamp(0, mask.width as i32 - 1) as usize;
        let ey = (eye.1 as i32 - mask.y0).clamp(0, mask.height as i32 - 1) as usize;
        let eye_w = protect[ey * w + ex];
        let cheek_pt = geometry.from_local(0.6, 0.3);
        let cx = (cheek_pt.0 as i32 - mask.x0).clamp(0, mask.width as i32 - 1) as usize;
        let cy = (cheek_pt.1 as i32 - mask.y0).clamp(0, mask.height as i32 - 1) as usize;
        let cheek_w = protect[cy * w + cx];
        assert!(
            eye_w < cheek_w,
            "眼睛保护应更严 eye={eye_w} cheek={cheek_w}"
        );
    }

    #[test]
    fn combine_multiplies_masks() {
        let skin = vec![1.0, 0.5, 0.0];
        let prot = vec![0.5, 1.0, 1.0];
        let out = combine_with_skin(&skin, &prot);
        assert!((out[0] - 0.5).abs() < 1e-6);
        assert!((out[1] - 0.5).abs() < 1e-6);
        assert_eq!(out[2], 0.0);
    }
}
