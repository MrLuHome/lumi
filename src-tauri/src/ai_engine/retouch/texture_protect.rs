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
    let mut protect = vec![1.0f32; n];
    for y in 1..h.saturating_sub(1) {
        if y % 32 == 0 {
            cancel.check()?;
        }
        for x in 1..w.saturating_sub(1) {
            let i = (y * w + x) as usize;
            let gx = (luma[i + 1] - luma[i - 1]).abs();
            let gy = (luma[i + w as usize] - luma[i - w as usize]).abs();
            let edge = (gx + gy) * 0.5;
            // 只保护**很强**的边缘（发丝、睫毛、轮廓），普通皮肤纹理必须放行，
            // 否则 process 权重被压没，磨皮空转（S1 联调发现 mean_w≈0.03）。
            let edge_keep = 1.0 - (edge / 0.28).clamp(0.0, 1.0) * 0.9;
            protect[i] = (feature[i] * edge_keep).clamp(0.0, 1.0);
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
