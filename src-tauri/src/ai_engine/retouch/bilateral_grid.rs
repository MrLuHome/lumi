//! 多尺度双边网格平滑（S1 SkinRetouchEngine 核心）。
//!
//! 算法思想参考公开的 bilateral grid（Apache-2.0 参考实现的数学结构），
//! **独立 Rust 实现**，不移植原仓库代码：
//! splat（空间×亮度分箱）→ 可分盒式模糊 → 归一化 → 三线性 slice
//! → 粗到细金字塔 + 强度混合 + 细节层 soft-threshold 回注。
//!
//! 约束：
//! - 输入输出均为 16-bit RGB 平面（0..1），不降为 8-bit；
//! - 边界 pad + clip，禁止 0 填充造成条纹；
//! - 网格尺寸随图像与 σ 受控，避免 4K 爆内存。

use crate::error::AppResult;
use crate::image_engine::cancel::CancelToken;

/// 双边网格参数（0..1 强度域）。
#[derive(Debug, Clone, Copy)]
pub struct BilateralGridParams {
    /// 空间步长（像素）。
    pub sigma_s: f32,
    /// 亮度分箱宽度（归一化 0..1）。
    pub sigma_r: f32,
    /// 金字塔层数（0 = 单级）。
    pub levels: u32,
    /// 总强度 0..1。
    pub strength: f32,
    /// 细节 soft-threshold 0..1（越大越保护高频）。
    pub detail_threshold: f32,
}

impl Default for BilateralGridParams {
    fn default() -> Self {
        Self {
            sigma_s: 8.0,
            sigma_r: 0.12,
            levels: 3,
            strength: 0.5,
            detail_threshold: 0.25,
        }
    }
}

/// 从滑杆推导网格参数（与 S0 研究的映射同型，独立常数）。
pub fn params_from_strength(smoothing: f32, texture: f32, detail: f32) -> BilateralGridParams {
    let s = smoothing.clamp(0.0, 100.0) / 100.0;
    let texture = texture.clamp(0.0, 100.0) / 100.0;
    let detail = detail.clamp(0.0, 100.0) / 100.0;
    BilateralGridParams {
        sigma_s: 4.0 + 10.0 * s,
        sigma_r: 0.04 + 0.16 * (1.0 - texture * 0.5),
        levels: if s < 0.15 { 1 } else if s < 0.55 { 2 } else { 3 },
        strength: s,
        // 纹理保护越高，阈值越高，越少抹掉毛孔。
        detail_threshold: (0.10 + 0.35 * texture + 0.15 * detail).clamp(0.05, 0.7),
    }
}

/// 对 RGB 平面（各通道 0..1，行优先）做双边网格平滑，原地写回 `rgb`。
///
/// `mask` 可选：长度 = w*h；None 表示全图。融合权重 = mask（调用方应已羽化）。
pub fn bilateral_grid_smooth(
    rgb: &mut [f32],
    width: u32,
    height: u32,
    mask: Option<&[f32]>,
    params: &BilateralGridParams,
    cancel: &CancelToken,
) -> AppResult<()> {
    let n = (width * height) as usize;
    if width < 4 || height < 4 || rgb.len() != n * 3 || params.strength <= 0.0 {
        return Ok(());
    }
    if let Some(m) = mask {
        if m.len() != n {
            return Err(crate::error::AppError::Unsupported(
                "bilateral mask 尺寸与图像不一致".into(),
            ));
        }
    }

    // 工作缓冲：原图副本 + 各尺度结果。
    let original: Vec<f32> = rgb.to_vec();
    let mut current = original.clone();
    let levels = params.levels.clamp(0, 4);
    let mut level_images: Vec<Vec<f32>> = Vec::with_capacity(levels as usize + 1);

    // 粗到细：先最粗，再上采样叠加。
    for level in (0..=levels).rev() {
        cancel.check()?;
        let scale = 1u32 << level.min(3);
        let (lw, lh) = ((width + scale - 1) / scale, (height + scale - 1) / scale);
        let (lw, lh) = (lw.max(4), lh.max(4));
        let low = if level == 0 {
            current.clone()
        } else {
            downsample_rgb(&current, width, height, lw, lh)
        };
        let sigma_s = (params.sigma_s / scale as f32).max(2.0);
        let strength_level = if level == 0 {
            params.strength
        } else {
            // 弱强度起步，避免粗层过糊。
            (params.strength * 16.0 / (level as f32 + levels as f32 + 15.0)).clamp(0.0, 1.0)
        };
        let mut smoothed = low.clone();
        grid_pass(
            &mut smoothed,
            lw,
            lh,
            sigma_s,
            params.sigma_r,
            strength_level,
            cancel,
        )?;
        // 再跑一次满强度（与参考管线双 pass 同型）。
        if strength_level > 0.01 {
            grid_pass(
                &mut smoothed,
                lw,
                lh,
                sigma_s,
                params.sigma_r,
                params.strength,
                cancel,
            )?;
        }
        if level > 0 {
            smoothed = upsample_rgb(&smoothed, lw, lh, width, height);
        }
        level_images.push(smoothed);
    }

    // level_images[0] = 最粗上采样结果… 最后一项 = 全分辨率单级。
    // 合成：从最细（最后一项）为主，向粗层加权混合（strength 已在各层应用）。
    let mut combined = level_images.pop().unwrap_or_else(|| original.clone());
    let w_fine = 1.0;
    let mut w_sum = w_fine;
    // 少量粗层贡献大尺度均匀，权重随层递减。
    for (i, level) in level_images.iter().enumerate().rev() {
        let w = 0.35 / (i as f32 + 1.0);
        for c in 0..3 {
            for p in 0..n {
                let idx = p * 3 + c;
                combined[idx] = (combined[idx] * w_sum + level[idx] * w) / (w_sum + w);
            }
        }
        w_sum += w;
        cancel.check()?;
    }

    // 细节层：original - combined，soft threshold 后按 strength 与 mask 回注。
    let thr = params.detail_threshold;
    let keep_base = 0.15 + 0.75 * (1.0 - params.strength); // 强度越大，回注越少
    for p in 0..n {
        let m = mask.map(|m| m[p].clamp(0.0, 1.0)).unwrap_or(1.0);
        if m <= 0.01 {
            continue;
        }
        let blend = m * params.strength;
        for c in 0..3 {
            let idx = p * 3 + c;
            let src = original[idx];
            let base = combined[idx];
            let detail = src - base;
            // soft threshold：低于 thr 的细节衰减，高于的保留。
            let ad = detail.abs();
            let gate = if ad <= thr {
                ad / thr.max(1e-4) // 线性 ramp
            } else {
                1.0
            };
            let keep = (keep_base + (1.0 - keep_base) * gate).clamp(0.0, 1.0);
            let target = base + detail * keep;
            let value = src + (target - src) * blend;
            rgb[idx] = value.clamp(0.0, 1.0);
        }
    }
    Ok(())
}

fn luma(r: f32, g: f32, b: f32) -> f32 {
    0.299 * r + 0.587 * g + 0.114 * b
}

/// 单次双边网格 pass：原地平滑 `rgb`。
fn grid_pass(
    rgb: &mut [f32],
    width: u32,
    height: u32,
    sigma_s: f32,
    sigma_r: f32,
    strength: f32,
    cancel: &CancelToken,
) -> AppResult<()> {
    if strength <= 0.0 {
        return Ok(());
    }
    let w = width as usize;
    let h = height as usize;
    let n = w * h;
    let ss = sigma_s.max(1.0);
    let sr = sigma_r.max(0.01);

    // 网格尺寸：空间 + 亮度轴，各 +3 供模糊 halo。
    let nx = ((w as f32 / ss).ceil() as usize).max(2) + 3;
    let ny = ((h as f32 / ss).ceil() as usize).max(2) + 3;
    let nv = ((1.0 / sr).ceil() as usize).max(2) + 3;
    let cell = nx * ny * nv;
    // 4 平面：R,G,B,weight
    let mut acc = vec![0f32; cell * 4];

    // Splat
    for y in 0..h {
        if y % 64 == 0 {
            cancel.check()?;
        }
        for x in 0..w {
            let p = y * w + x;
            let r = rgb[p * 3];
            let g = rgb[p * 3 + 1];
            let b = rgb[p * 3 + 2];
            let i = luma(r, g, b).clamp(0.0, 1.0);
            let gx = ((x as f32 / ss + 0.5) as usize + 1).min(nx - 1);
            let gy = ((y as f32 / ss + 0.5) as usize + 1).min(ny - 1);
            let gv = ((i / sr + 0.5) as usize + 1).min(nv - 1);
            let idx = ((gy * nx + gx) * nv + gv) * 4;
            acc[idx] += r;
            acc[idx + 1] += g;
            acc[idx + 2] += b;
            acc[idx + 3] += 1.0;
        }
    }

    // 可分盒式模糊 (2c+n1+n2)/4 三轴
    blur_grid(&mut acc, nx, ny, nv, cancel)?;
    blur_grid(&mut acc, nx, ny, nv, cancel)?;

    // 归一化
    for c in 0..cell {
        let wgt = acc[c * 4 + 3];
        if wgt > 1e-6 {
            acc[c * 4] /= wgt;
            acc[c * 4 + 1] /= wgt;
            acc[c * 4 + 2] /= wgt;
        } else {
            acc[c * 4] = 0.0;
            acc[c * 4 + 1] = 0.0;
            acc[c * 4 + 2] = 0.0;
        }
        acc[c * 4 + 3] = if wgt > 1e-6 { 1.0 } else { 0.0 };
    }

    // Slice：三线性 + 强度混合
    let mut out = rgb.to_vec();
    for y in 0..h {
        if y % 64 == 0 {
            cancel.check()?;
        }
        for x in 0..w {
            let p = y * w + x;
            let r = rgb[p * 3];
            let g = rgb[p * 3 + 1];
            let b = rgb[p * 3 + 2];
            let i = luma(r, g, b).clamp(0.0, 1.0);
            // 连续坐标：中心在 (x/ss+1, y/ss+1, i/sr+1)
            let fx = x as f32 / ss + 1.0;
            let fy = y as f32 / ss + 1.0;
            let fv = i / sr + 1.0;
            let (sr, sg, sb) = trilinear(&acc, nx, ny, nv, fx, fy, fv);
            let m = if acc[0].is_finite() { 1.0 } else { 1.0 };
            let _ = m;
            out[p * 3] = r + (sr - r) * strength;
            out[p * 3 + 1] = g + (sg - g) * strength;
            out[p * 3 + 2] = b + (sb - b) * strength;
        }
    }
    rgb.copy_from_slice(&out);
    Ok(())
}

fn blur_grid(acc: &mut [f32], nx: usize, ny: usize, nv: usize, cancel: &CancelToken) -> AppResult<()> {
    let cell = nx * ny * nv;
    // X 轴
    let mut tmp = vec![0f32; cell * 4];
    for z in 0..nv {
        for y in 0..ny {
            for x in 0..nx {
                let i = (y * nx + x) * nv + z;
                for c in 0..4 {
                    let left = if x == 0 { i } else { (y * nx + (x - 1)) * nv + z };
                    let right = if x + 1 >= nx { i } else { (y * nx + x + 1) * nv + z };
                    tmp[i * 4 + c] = (2.0 * acc[i * 4 + c] + acc[left * 4 + c] + acc[right * 4 + c]) * 0.25;
                }
            }
        }
    }
    // Y 轴
    acc.copy_from_slice(&tmp);
    for z in 0..nv {
        for y in 0..ny {
            for x in 0..nx {
                let i = (y * nx + x) * nv + z;
                for c in 0..4 {
                    let up = if y == 0 { i } else { ((y - 1) * nx + x) * nv + z };
                    let down = if y + 1 >= ny { i } else { ((y + 1) * nx + x) * nv + z };
                    tmp[i * 4 + c] = (2.0 * acc[i * 4 + c] + acc[up * 4 + c] + acc[down * 4 + c]) * 0.25;
                }
            }
        }
    }
    // V 轴
    acc.copy_from_slice(&tmp);
    for z in 0..nv {
        for y in 0..ny {
            for x in 0..nx {
                let i = (y * nx + x) * nv + z;
                for c in 0..4 {
                    let lo = if z == 0 { i } else { (y * nx + x) * nv + (z - 1) };
                    let hi = if z + 1 >= nv { i } else { (y * nx + x) * nv + (z + 1) };
                    tmp[i * 4 + c] = (2.0 * acc[i * 4 + c] + acc[lo * 4 + c] + acc[hi * 4 + c]) * 0.25;
                }
            }
        }
    }
    acc.copy_from_slice(&tmp);
    Ok(())
}

fn trilinear(
    acc: &[f32],
    nx: usize,
    ny: usize,
    nv: usize,
    fx: f32,
    fy: f32,
    fv: f32,
) -> (f32, f32, f32) {
    let x0 = (fx.floor() as i32).clamp(0, nx as i32 - 1) as usize;
    let y0 = (fy.floor() as i32).clamp(0, ny as i32 - 1) as usize;
    let z0 = (fv.floor() as i32).clamp(0, nv as i32 - 1) as usize;
    let x1 = (x0 + 1).min(nx - 1);
    let y1 = (y0 + 1).min(ny - 1);
    let z1 = (z0 + 1).min(nv - 1);
    let tx = (fx - x0 as f32).clamp(0.0, 1.0);
    let ty = (fy - y0 as f32).clamp(0.0, 1.0);
    let tz = (fv - z0 as f32).clamp(0.0, 1.0);

    let mut r = 0.0;
    let mut g = 0.0;
    let mut b = 0.0;
    for dz in 0..2 {
        for dy in 0..2 {
            for dx in 0..2 {
                let x = if dx == 0 { x0 } else { x1 };
                let y = if dy == 0 { y0 } else { y1 };
                let z = if dz == 0 { z0 } else { z1 };
                let w = (if dx == 0 { 1.0 - tx } else { tx })
                    * (if dy == 0 { 1.0 - ty } else { ty })
                    * (if dz == 0 { 1.0 - tz } else { tz });
                let idx = ((y * nx + x) * nv + z) * 4;
                let wgt = acc[idx + 3];
                if wgt > 0.5 {
                    r += acc[idx] * w;
                    g += acc[idx + 1] * w;
                    b += acc[idx + 2] * w;
                }
            }
        }
    }
    // 若邻域权重和不足，回退最近有效 cell 的颜色（避免黑洞）。
    if r + g + b == 0.0 {
        let idx = ((y0 * nx + x0) * nv + z0) * 4;
        return (acc[idx], acc[idx + 1], acc[idx + 2]);
    }
    (r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0))
}

fn downsample_rgb(src: &[f32], sw: u32, sh: u32, dw: u32, dh: u32) -> Vec<f32> {
    let mut out = vec![0f32; (dw * dh * 3) as usize];
    for y in 0..dh {
        for x in 0..dw {
            let sx = ((x as f32 + 0.5) * sw as f32 / dw as f32) as u32;
            let sy = ((y as f32 + 0.5) * sh as f32 / dh as f32) as u32;
            let sp = (sy.min(sh - 1) * sw + sx.min(sw - 1)) as usize;
            let dp = (y * dw + x) as usize;
            for c in 0..3 {
                out[dp * 3 + c] = src[sp * 3 + c];
            }
        }
    }
    out
}

fn upsample_rgb(src: &[f32], sw: u32, sh: u32, dw: u32, dh: u32) -> Vec<f32> {
    let mut out = vec![0f32; (dw * dh * 3) as usize];
    for y in 0..dh {
        for x in 0..dw {
            let fx = (x as f32 + 0.5) * sw as f32 / dw as f32 - 0.5;
            let fy = (y as f32 + 0.5) * sh as f32 / dh as f32 - 0.5;
            let x0 = (fx.floor() as i32).clamp(0, sw as i32 - 1) as u32;
            let y0 = (fy.floor() as i32).clamp(0, sh as i32 - 1) as u32;
            let x1 = (x0 + 1).min(sw - 1);
            let y1 = (y0 + 1).min(sh - 1);
            let tx = (fx - x0 as f32).clamp(0.0, 1.0);
            let ty = (fy - y0 as f32).clamp(0.0, 1.0);
            let dp = (y * dw + x) as usize;
            for c in 0..3 {
                let p00 = (y0 * sw + x0) as usize * 3 + c;
                let p01 = (y0 * sw + x1) as usize * 3 + c;
                let p10 = (y1 * sw + x0) as usize * 3 + c;
                let p11 = (y1 * sw + x1) as usize * 3 + c;
                out[dp * 3 + c] = src[p00] * (1.0 - tx) * (1.0 - ty)
                    + src[p01] * tx * (1.0 - ty)
                    + src[p10] * (1.0 - tx) * ty
                    + src[p11] * tx * ty;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_engine::cancel::CancelToken;

    fn gradient_rgb(w: u32, h: u32) -> Vec<f32> {
        let mut v = vec![0f32; (w * h * 3) as usize];
        for y in 0..h {
            for x in 0..w {
                let p = (y * w + x) as usize;
                let t = x as f32 / w as f32;
                v[p * 3] = t;
                v[p * 3 + 1] = 0.4 + 0.2 * t;
                v[p * 3 + 2] = 0.7 - 0.3 * t;
            }
        }
        v
    }

    #[test]
    fn zero_strength_is_identity() {
        let mut rgb = gradient_rgb(32, 32);
        let orig = rgb.clone();
        let p = BilateralGridParams {
            strength: 0.0,
            ..Default::default()
        };
        bilateral_grid_smooth(&mut rgb, 32, 32, None, &p, &CancelToken::default()).unwrap();
        assert_eq!(rgb, orig);
    }

    #[test]
    fn smooth_reduces_high_freq_but_keeps_dc() {
        let mut rgb = gradient_rgb(48, 48);
        // 加高频噪
        for p in 0..48 * 48 {
            let n = ((p % 7) as f32 - 3.0) * 0.02;
            rgb[p * 3] = (rgb[p * 3] + n).clamp(0.0, 1.0);
        }
        let before = high_freq_energy(&rgb, 48, 48);
        let p = BilateralGridParams {
            sigma_s: 6.0,
            sigma_r: 0.15,
            levels: 2,
            strength: 0.85,
            detail_threshold: 0.15,
        };
        bilateral_grid_smooth(&mut rgb, 48, 48, None, &p, &CancelToken::default()).unwrap();
        let after = high_freq_energy(&rgb, 48, 48);
        assert!(
            after < before,
            "高频应下降 before={before:.4} after={after:.4}"
        );
        // 左右 DC 大致保持（梯度方向）
        let left = rgb[0];
        let right = rgb[(47 * 3) as usize];
        assert!((right - left) > 0.5, "不应把整图抹平 left={left} right={right}");
    }

    #[test]
    fn mask_zero_leaves_region_unchanged() {
        let mut rgb = gradient_rgb(24, 24);
        let orig = rgb.clone();
        let mask = vec![0f32; 24 * 24];
        let p = BilateralGridParams {
            strength: 0.9,
            ..Default::default()
        };
        bilateral_grid_smooth(&mut rgb, 24, 24, Some(&mask), &p, &CancelToken::default()).unwrap();
        // mask 全 0：blend=0 → 应保持 original（细节回注 blend 也为 0）
        for i in 0..rgb.len() {
            assert!(
                (rgb[i] - orig[i]).abs() < 1e-6,
                "mask=0 必须恒等 idx={i}"
            );
        }
    }

    #[test]
    fn wrong_mask_len_errors() {
        let mut rgb = gradient_rgb(8, 8);
        let p = BilateralGridParams::default();
        let err = bilateral_grid_smooth(&mut rgb, 8, 8, Some(&[0.0; 3]), &p, &CancelToken::default());
        assert!(err.is_err());
    }

    fn high_freq_energy(rgb: &[f32], w: u32, h: u32) -> f32 {
        let mut sum = 0.0f32;
        let mut count = 0.0f32;
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let p = (y * w + x) as usize;
                let c = (y * w + x - 1) as usize;
                sum += (rgb[p * 3] - rgb[c * 3]).abs();
                count += 1.0;
            }
        }
        sum / count.max(1.0)
    }
}
