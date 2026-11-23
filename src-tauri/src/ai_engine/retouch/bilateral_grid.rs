//! 多尺度双边网格平滑（S1 SkinRetouchEngine 核心）。
//!
//! 算法思想参考公开的 bilateral grid（Apache-2.0 参考实现的数学结构），
//! **独立 Rust 实现**，不移植原仓库代码：
//! splat（空间×亮度分箱，按皮肤蒙版置信度加权）→ 可分盒式模糊
//! （颜色累加与权重累加分离传播）→ 归一化（无效节点打标而非染黑）
//! → 三线性 slice（权重归一化 + 最近有效节点回退）→ 粗到细金字塔
//! （面积平均降采样 / Catmull-Rom 上采样融合）→ 细节层 soft-threshold 回注。
//!
//! 约束：
//! - 输入输出均为 16-bit RGB 平面（0..1），不降为 8-bit；
//! - 边界 pad + clip，禁止 0 填充造成条纹；
//! - 网格尺寸随图像与 σ 受控，避免 4K 爆内存；
//! - 全分辨率工作副本仅保留 1 份（`combined`），各尺度小图随算随弃。

use std::time::Instant;

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
    /// 细节回注地板 0..1：网格平滑后允许回注回来的比例下限。
    /// **只由 texture/detail 决定（纹理保护归纹理旋钮），不随 strength 变化**——
    /// 否则强度在 grid(s)、采纳(m·s)、回注地板(0.9−0.75s) 三处重复计效，
    /// 有效强度被压成非线性的小量（一键精修“几乎无效果”的根因之一）。
    pub detail_keep_floor: f32,
}

impl Default for BilateralGridParams {
    fn default() -> Self {
        Self {
            sigma_s: 8.0,
            sigma_r: 0.12,
            levels: 3,
            strength: 0.5,
            detail_threshold: 0.25,
            detail_keep_floor: 0.35,
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
        // 回注地板只看纹理/细节旋钮：texture 高 → 地板高 → 多回注（护毛孔）；
        // texture 低（肤质本身光滑/要求强磨皮）→ 地板低 → 少回注。
        // 强度 s 不再进入地板，净平滑 ≈ s × (1 − floor)，对 s 线性。
        detail_keep_floor: (0.15 + 0.45 * texture).clamp(0.10, 0.75),
    }
}

/// 双边网格阶段统计（性能/内存验收用）。
#[derive(Debug, Clone, Default)]
pub struct GridStats {
    /// 每尺度记录 `(level, 降采样ms, 网格处理ms, 上采样融合ms)`；level 0 无降采样/上采样。
    pub level_ms: Vec<(u32, u128, u128, u128)>,
    /// 所有 pass 的 Splat 累计耗时。
    pub splat_ms: u128,
    /// 所有 pass 的 Blur 累计耗时（含 normalize）。
    pub blur_ms: u128,
    /// 所有 pass 的 Slice 累计耗时。
    pub slice_ms: u128,
    /// 单个网格缓冲（4 平面 acc）峰值字节数。
    pub max_grid_bytes: u64,
    /// Blur 临时缓冲峰值字节数。
    pub max_blur_tmp_bytes: u64,
    /// 网格单元数峰值（nx*ny*nv）。
    pub max_cells: u64,
}

/// 对 RGB 平面（各通道 0..1，行优先）做双边网格平滑，原地写回 `rgb`。
///
/// `mask` 可选：长度 = w*h；None 表示全图。融合权重 = mask（调用方应已羽化）。
/// mask 全为 0 时恒等返回。
pub fn bilateral_grid_smooth(
    rgb: &mut [f32],
    width: u32,
    height: u32,
    mask: Option<&[f32]>,
    params: &BilateralGridParams,
    cancel: &CancelToken,
) -> AppResult<()> {
    bilateral_grid_smooth_with_stats(rgb, width, height, mask, params, cancel).map(|_| ())
}

/// 同 [`bilateral_grid_smooth`]，并返回各阶段耗时与网格缓冲统计。
pub fn bilateral_grid_smooth_with_stats(
    rgb: &mut [f32],
    width: u32,
    height: u32,
    mask: Option<&[f32]>,
    params: &BilateralGridParams,
    cancel: &CancelToken,
) -> AppResult<GridStats> {
    let n = (width * height) as usize;
    let mut stats = GridStats::default();
    if width < 4 || height < 4 || rgb.len() != n * 3 || params.strength <= 0.0 {
        return Ok(stats);
    }
    if let Some(m) = mask {
        if m.len() != n {
            return Err(crate::error::AppError::Unsupported(
                "bilateral mask 尺寸与图像不一致".into(),
            ));
        }
        // 蒙版全 0：不参与 splat、不回注 → 恒等，直接返回。
        if m.iter().all(|v| *v <= 1e-4) {
            return Ok(stats);
        }
    }

    let levels = params.levels.clamp(0, 4);

    // 累加器从最细层（level 0，全分辨率）开始，权重 1.0；
    // 各粗层面积降采样 → 网格平滑 → Catmull-Rom 上采样后按权重融合进来。
    // 与「先存全部尺度再合成」数学等价（加权平均可交换），但全分辨率缓冲只需 1 份。
    let mut combined = rgb.to_vec();
    let mut w_sum = 1.0f32;

    // —— level 0：全分辨率 ——
    cancel.check()?;
    let t_level = Instant::now();
    let t0 = Instant::now();
    grid_pass(
        &mut combined,
        width,
        height,
        mask,
        params.sigma_s,
        params.sigma_r,
        params.strength,
        cancel,
        &mut stats,
    )?;
    if params.strength > 0.01 {
        // 双 pass（与参考管线同型）：第二次仍为满强度。
        grid_pass(
            &mut combined,
            width,
            height,
            mask,
            params.sigma_s,
            params.sigma_r,
            params.strength,
            cancel,
            &mut stats,
        )?;
    }
    let level0_ms = t0.elapsed().as_millis();
    stats
        .level_ms
        .push((0, 0, level0_ms, t_level.elapsed().as_millis()));

    // —— 粗层：从最粗到次细依次融合（输入始终为原图 rgb）——
    for level in (1..=levels).rev() {
        cancel.check()?;
        let scale = 1u32 << level.min(3);
        let (lw, lh) = (
            ((width + scale - 1) / scale).max(4),
            ((height + scale - 1) / scale).max(4),
        );

        let t_down = Instant::now();
        let mut low = downsample_rgb(rgb, width, height, lw, lh);
        let low_mask: Option<Vec<f32>> = mask.map(|m| downsample_mask(m, width, height, lw, lh));
        let down_ms = t_down.elapsed().as_millis();

        let sigma_s = (params.sigma_s / scale as f32).max(2.0);
        // 弱强度起步，避免粗层过糊；第二 pass 满强度。
        let strength_level = (params.strength * 16.0 / (level as f32 + levels as f32 + 15.0))
            .clamp(0.0, 1.0);
        let t_proc = Instant::now();
        grid_pass(
            &mut low,
            lw,
            lh,
            low_mask.as_deref(),
            sigma_s,
            params.sigma_r,
            strength_level,
            cancel,
            &mut stats,
        )?;
        if strength_level > 0.01 {
            grid_pass(
                &mut low,
                lw,
                lh,
                low_mask.as_deref(),
                sigma_s,
                params.sigma_r,
                params.strength,
                cancel,
                &mut stats,
            )?;
        }
        let proc_ms = t_proc.elapsed().as_millis();

        // 融合权重（明确固定，不随实现漂移）：
        //   level 0（最细）w = 1.0；
        //   粗层 L（L = levels … 1）w = 0.35 / (levels - L + 1)，
        //   即最粗 0.35、次粗 0.175、…、最靠近全分辨率的粗层 0.35/levels。
        // combined = Σ w_i · up(I_i) / Σ w_i。
        let w_level = 0.35 / ((levels - level) as f32 + 1.0);
        let t_blend = Instant::now();
        upsample_blend_into(
            &mut combined,
            &low,
            lw,
            lh,
            width,
            height,
            w_level,
            w_sum,
            cancel,
        )?;
        w_sum += w_level;
        let blend_ms = t_blend.elapsed().as_millis();
        stats
            .level_ms
            .push((level, down_ms, proc_ms, blend_ms));
    }

    // 细节层：original(=rgb 尚未被改写) - combined，soft threshold 后按蒙版回注。
    // 逐像素先读后写、同址更新，因此无需再复制一份原图。
    //
    // 采纳系数只用蒙版 m（羽化），**不再乘 strength**：强度已在 grid_pass 内部
    // 生效一次，这里再乘会让有效强度变成 strength²（自然档 0.32 → 0.10），
    // 违反「强度与效果线性」的产品契约，表现为一键精修几乎无效果。
    // 回注地板同样与 strength 解耦（见 detail_keep_floor）。
    let thr = params.detail_threshold;
    let keep_base = params.detail_keep_floor;
    for p in 0..n {
        let m = mask.map(|m| m[p].clamp(0.0, 1.0)).unwrap_or(1.0);
        if m <= 0.01 {
            continue;
        }
        let blend = m;
        for c in 0..3 {
            let idx = p * 3 + c;
            let src = rgb[idx];
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
    Ok(stats)
}

fn luma(r: f32, g: f32, b: f32) -> f32 {
    0.299 * r + 0.587 * g + 0.114 * b
}

/// 单次双边网格 pass：原地平滑 `rgb`。
///
/// - Splat：像素按蒙版置信度 `m` 参与——颜色累加 `Σ c·m` 与权重累加 `Σ m` 分离；
///   蒙版外像素（m≈0）不进入网格，不会以完整权重污染皮肤网格。
/// - Blur：对 4 平面（R,G,B,w）做线性可分离模糊，稀疏网格下权重随颜色同步传播。
/// - Normalize：`w > eps` 的节点颜色 = 颜色累加/权重累加并标记有效；
///   无效节点标记为 0（忽略），**不**当作黑色有效节点。
/// - Slice：三线性只聚合有效节点并对权重和归一化；权重和过小时回退最近有效节点；
///   整个网格无有效节点时保持输入像素。
fn grid_pass(
    rgb: &mut [f32],
    width: u32,
    height: u32,
    mask: Option<&[f32]>,
    sigma_s: f32,
    sigma_r: f32,
    strength: f32,
    cancel: &CancelToken,
    stats: &mut GridStats,
) -> AppResult<()> {
    if strength <= 0.0 {
        return Ok(());
    }
    let w = width as usize;
    let h = height as usize;
    let n = w * h;
    if let Some(m) = mask {
        if m.len() != n {
            return Err(crate::error::AppError::Unsupported(
                "grid mask 尺寸与图像不一致".into(),
            ));
        }
    }
    let ss = sigma_s.max(1.0);
    let sr = sigma_r.max(0.01);

    // 网格尺寸：空间 + 亮度轴，各 +3 供模糊 halo。
    let nx = ((w as f32 / ss).ceil() as usize).max(2) + 3;
    let ny = ((h as f32 / ss).ceil() as usize).max(2) + 3;
    let nv = ((1.0 / sr).ceil() as usize).max(2) + 3;
    let cell = nx * ny * nv;
    // 4 平面：[0..3)=Σc·m 颜色累加，[3]=Σm 权重累加（累加器分离）。
    let mut acc = vec![0f32; cell * 4];
    stats.max_cells = stats.max_cells.max(cell as u64);
    stats.max_grid_bytes = stats.max_grid_bytes.max((cell * 4 * 4) as u64);

    // Splat
    let t_splat = Instant::now();
    for y in 0..h {
        if y % 64 == 0 {
            cancel.check()?;
        }
        for x in 0..w {
            let p = y * w + x;
            let m = mask.map(|m| m[p].clamp(0.0, 1.0)).unwrap_or(1.0);
            if m <= 1e-4 {
                continue;
            }
            let r = rgb[p * 3];
            let g = rgb[p * 3 + 1];
            let b = rgb[p * 3 + 2];
            let i = luma(r, g, b).clamp(0.0, 1.0);
            let gx = ((x as f32 / ss + 0.5) as usize + 1).min(nx - 1);
            let gy = ((y as f32 / ss + 0.5) as usize + 1).min(ny - 1);
            let gv = ((i / sr + 0.5) as usize + 1).min(nv - 1);
            let idx = ((gy * nx + gx) * nv + gv) * 4;
            acc[idx] += r * m;
            acc[idx + 1] += g * m;
            acc[idx + 2] += b * m;
            acc[idx + 3] += m;
        }
    }
    stats.splat_ms += t_splat.elapsed().as_millis();

    // 可分盒式模糊 (2c+n1+n2)/4 三轴；4 平面同步传播（线性 → 稀疏权重正确）。
    let t_blur = Instant::now();
    blur_grid(&mut acc, nx, ny, nv, stats, cancel)?;
    blur_grid(&mut acc, nx, ny, nv, stats, cancel)?;

    // 归一化：有效节点写出平均颜色；无效节点仅标 0，绝不染成“有效黑色”。
    for c in 0..cell {
        let wgt = acc[c * 4 + 3];
        if wgt > 1e-6 {
            acc[c * 4] /= wgt;
            acc[c * 4 + 1] /= wgt;
            acc[c * 4 + 2] /= wgt;
            acc[c * 4 + 3] = 1.0;
        } else {
            acc[c * 4] = 0.0;
            acc[c * 4 + 1] = 0.0;
            acc[c * 4 + 2] = 0.0;
            acc[c * 4 + 3] = 0.0; // 无效标记
        }
    }
    stats.blur_ms += t_blur.elapsed().as_millis();

    // Slice：三线性 + 强度混合（同址：先读后写）。
    let t_slice = Instant::now();
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
            if let Some((sr_, sg, sb)) = trilinear(&acc, nx, ny, nv, fx, fy, fv) {
                rgb[p * 3] = r + (sr_ - r) * strength;
                rgb[p * 3 + 1] = g + (sg - g) * strength;
                rgb[p * 3 + 2] = b + (sb - b) * strength;
            }
            // 无有效节点：保持输入像素（合法黑色/空蒙版都不会被误判为失败）。
        }
    }
    stats.slice_ms += t_slice.elapsed().as_millis();
    Ok(())
}

fn blur_grid(
    acc: &mut [f32],
    nx: usize,
    ny: usize,
    nv: usize,
    stats: &mut GridStats,
    cancel: &CancelToken,
) -> AppResult<()> {
    let cell = nx * ny * nv;
    // X 轴
    let mut tmp = vec![0f32; cell * 4];
    stats.max_blur_tmp_bytes = stats.max_blur_tmp_bytes.max((cell * 4 * 4) as u64);
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
    cancel.check()?;
    Ok(())
}

/// 三线性采样：只聚合有效节点（`acc[..+3] > 0.5`），按有效权重和归一化。
///
/// - 有效权重和 ≥ `MIN_W` → 归一化后的加权平均（合法黑色像素颜色为 0 但权重正常，
///   **不会**被当作插值失败）；
/// - 有效权重和过小 → 在采样点 3×3×3 邻域（Chebyshev 外壳 r=1..3）找最近有效节点；
/// - 仍找不到 → `None`，调用方保持输入像素。
///
/// 返回值恒为有限的 0..1 颜色。
fn trilinear(
    acc: &[f32],
    nx: usize,
    ny: usize,
    nv: usize,
    fx: f32,
    fy: f32,
    fv: f32,
) -> Option<(f32, f32, f32)> {
    if nx == 0 || ny == 0 || nv == 0 || !fx.is_finite() || !fy.is_finite() || !fv.is_finite() {
        return None;
    }
    let x0 = (fx.floor() as i64).clamp(0, nx as i64 - 1) as usize;
    let y0 = (fy.floor() as i64).clamp(0, ny as i64 - 1) as usize;
    let z0 = (fv.floor() as i64).clamp(0, nv as i64 - 1) as usize;
    let x1 = (x0 + 1).min(nx - 1);
    let y1 = (y0 + 1).min(ny - 1);
    let z1 = (z0 + 1).min(nv - 1);
    let tx = (fx - x0 as f32).clamp(0.0, 1.0);
    let ty = (fy - y0 as f32).clamp(0.0, 1.0);
    let tz = (fv - z0 as f32).clamp(0.0, 1.0);

    const MIN_W: f32 = 1e-4;
    let mut sum_w = 0.0f32;
    let mut r = 0.0f32;
    let mut g = 0.0f32;
    let mut b = 0.0f32;
    let mut any_valid = false;
    let mut fallback: Option<(f32, f32, f32)> = None;
    let mut fallback_w = -1.0f32;
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
                if acc[idx + 3] > 0.5 {
                    any_valid = true;
                    sum_w += w;
                    r += acc[idx] * w;
                    g += acc[idx + 1] * w;
                    b += acc[idx + 2] * w;
                    if w > fallback_w {
                        fallback_w = w;
                        fallback = Some((acc[idx], acc[idx + 1], acc[idx + 2]));
                    }
                }
            }
        }
    }

    if sum_w >= MIN_W {
        return Some((
            (r / sum_w).clamp(0.0, 1.0),
            (g / sum_w).clamp(0.0, 1.0),
            (b / sum_w).clamp(0.0, 1.0),
        ));
    }
    if any_valid {
        // 角点有有效节点但插值权重和过小（采样点恰落在权重为 0 的整数坐标上）。
        return fallback;
    }

    // 有效权重和为 0：在 3×3×3 邻域找最近有效节点。
    let cx = x0 as i64;
    let cy = y0 as i64;
    let cz = z0 as i64;
    for radius in 1i64..=3 {
        for dz in -radius..=radius {
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    if dx.abs().max(dy.abs()).max(dz.abs()) != radius {
                        continue;
                    }
                    let x = cx + dx;
                    let y = cy + dy;
                    let z = cz + dz;
                    if x < 0 || y < 0 || z < 0 || x >= nx as i64 || y >= ny as i64 || z >= nv as i64
                    {
                        continue;
                    }
                    let idx = (((y as usize) * nx + x as usize) * nv + z as usize) * 4;
                    if acc[idx + 3] > 0.5 {
                        return Some((acc[idx], acc[idx + 1], acc[idx + 2]));
                    }
                }
            }
        }
    }
    None
}

/// 面积平均降采样（正确处理非整数缩放比）：目标像素覆盖的源区域按重叠加权平均，
/// 等价于理想低通 + 抽取，避免最近邻的混叠/摩尔纹。
fn downsample_rgb(src: &[f32], sw: u32, sh: u32, dw: u32, dh: u32) -> Vec<f32> {
    debug_assert_eq!(src.len(), (sw * sh * 3) as usize);
    let mut out = vec![0f32; (dw * dh * 3) as usize];
    if sw == 0 || sh == 0 || dw == 0 || dh == 0 {
        return out;
    }
    let swf = sw as f32;
    let shf = sh as f32;
    for y in 0..dh {
        let y0f = y as f32 * shf / dh as f32;
        let y1f = (((y + 1) as f32) * shf / dh as f32).min(shf);
        for x in 0..dw {
            let x0f = x as f32 * swf / dw as f32;
            let x1f = (((x + 1) as f32) * swf / dw as f32).min(swf);
            let sx0 = (x0f.floor() as u32).min(sw - 1);
            let sx1 = ((x1f.ceil() as u32).min(sw)).max(sx0 + 1);
            let sy0 = (y0f.floor() as u32).min(sh - 1);
            let sy1 = ((y1f.ceil() as u32).min(sh)).max(sy0 + 1);
            let mut wr = 0.0f32;
            let mut wg = 0.0f32;
            let mut wb = 0.0f32;
            let mut wsum = 0.0f32;
            for sy in sy0..sy1 {
                let cy = ((sy + 1) as f32).min(y1f) - (sy as f32).max(y0f);
                if cy <= 0.0 {
                    continue;
                }
                for sx in sx0..sx1 {
                    let cx = ((sx + 1) as f32).min(x1f) - (sx as f32).max(x0f);
                    if cx <= 0.0 {
                        continue;
                    }
                    let wgt = cx * cy;
                    let sp = ((sy * sw + sx) as usize) * 3;
                    wr += src[sp] * wgt;
                    wg += src[sp + 1] * wgt;
                    wb += src[sp + 2] * wgt;
                    wsum += wgt;
                }
            }
            let dp = ((y * dw + x) as usize) * 3;
            if wsum > 0.0 {
                out[dp] = wr / wsum;
                out[dp + 1] = wg / wsum;
                out[dp + 2] = wb / wsum;
            } else {
                // 理论不可达（覆盖权重恒正）；防御性回退取源中心，避免写 0 造成黑点。
                let sx = ((x0f as u32).min(sw - 1)) as usize;
                let sy = ((y0f as u32).min(sh - 1)) as usize;
                let sp = (sy * sw as usize + sx) * 3;
                out[dp] = src[sp];
                out[dp + 1] = src[sp + 1];
                out[dp + 2] = src[sp + 2];
            }
        }
    }
    out
}

/// 蒙版面积平均降采样（与 `downsample_rgb` 同一覆盖模型）。
fn downsample_mask(src: &[f32], sw: u32, sh: u32, dw: u32, dh: u32) -> Vec<f32> {
    debug_assert_eq!(src.len(), (sw * sh) as usize);
    let mut out = vec![0f32; (dw * dh) as usize];
    if sw == 0 || sh == 0 || dw == 0 || dh == 0 {
        return out;
    }
    let swf = sw as f32;
    let shf = sh as f32;
    for y in 0..dh {
        let y0f = y as f32 * shf / dh as f32;
        let y1f = (((y + 1) as f32) * shf / dh as f32).min(shf);
        for x in 0..dw {
            let x0f = x as f32 * swf / dw as f32;
            let x1f = (((x + 1) as f32) * swf / dw as f32).min(swf);
            let sx0 = (x0f.floor() as u32).min(sw - 1);
            let sx1 = ((x1f.ceil() as u32).min(sw)).max(sx0 + 1);
            let sy0 = (y0f.floor() as u32).min(sh - 1);
            let sy1 = ((y1f.ceil() as u32).min(sh)).max(sy0 + 1);
            let mut sum = 0.0f32;
            let mut wsum = 0.0f32;
            for sy in sy0..sy1 {
                let cy = ((sy + 1) as f32).min(y1f) - (sy as f32).max(y0f);
                if cy <= 0.0 {
                    continue;
                }
                for sx in sx0..sx1 {
                    let cx = ((sx + 1) as f32).min(x1f) - (sx as f32).max(x0f);
                    if cx <= 0.0 {
                        continue;
                    }
                    let wgt = cx * cy;
                    sum += src[(sy * sw + sx) as usize] * wgt;
                    wsum += wgt;
                }
            }
            out[(y * dw + x) as usize] = if wsum > 0.0 {
                (sum / wsum).clamp(0.0, 1.0)
            } else {
                src[((y0f as u32).min(sh - 1) * sw + (x0f as u32).min(sw - 1)) as usize]
                    .clamp(0.0, 1.0)
            };
        }
    }
    out
}

/// Catmull-Rom 三次权重（分区和为 1 → 常量颜色上采样后保持不变）。
fn cubic_weights(t: f32) -> [f32; 4] {
    let t2 = t * t;
    let t3 = t2 * t;
    [
        -0.5 * t3 + t2 - 0.5 * t,
        1.0 - 2.5 * t2 + 1.5 * t3,
        0.5 * t + 2.0 * t2 - 1.5 * t3,
        -0.5 * t2 + 0.5 * t3,
    ]
}

/// 高质量（Catmull-Rom 双三次）采样，坐标为连续像素中心系，边界 clamp。
fn bicubic_sample(src: &[f32], sw: u32, sh: u32, fx: f32, fy: f32, channel: usize) -> f32 {
    let x0 = fx.floor() as i64;
    let y0 = fy.floor() as i64;
    let tx = fx - x0 as f32;
    let ty = fy - y0 as f32;
    let wx = cubic_weights(tx);
    let wy = cubic_weights(ty);
    let clamp_x = |v: i64| v.clamp(0, sw as i64 - 1) as usize;
    let clamp_y = |v: i64| v.clamp(0, sh as i64 - 1) as usize;
    let mut acc = 0.0f32;
    for j in 0..4 {
        let sy = clamp_y(y0 + j as i64 - 1);
        let mut row = 0.0f32;
        for i in 0..4 {
            let sx = clamp_x(x0 + i as i64 - 1);
            row += src[(sy * sw as usize + sx) * 3 + channel] * wx[i];
        }
        acc += row * wy[j];
    }
    acc.clamp(0.0, 1.0)
}

/// 把 `src`（sw×sh）双三次上采样到 dst（dw×dh）尺寸，并按加权平均融合进 dst：
/// `dst = (dst·w_dst + up(src)·w_src) / (w_dst + w_src)`。
/// 不分配全分辨率临时缓冲（内存优化：全程仅 1 份全分辨率工作副本）。
fn upsample_blend_into(
    dst: &mut [f32],
    src: &[f32],
    sw: u32,
    sh: u32,
    dw: u32,
    dh: u32,
    w_src: f32,
    w_dst: f32,
    cancel: &CancelToken,
) -> AppResult<()> {
    let w_new = w_dst + w_src;
    if w_new <= 0.0 {
        return Ok(());
    }
    let inv = 1.0 / w_new;
    for y in 0..dh {
        if y % 64 == 0 {
            cancel.check()?;
        }
        for x in 0..dw {
            let fx = (x as f32 + 0.5) * sw as f32 / dw as f32 - 0.5;
            let fy = (y as f32 + 0.5) * sh as f32 / dh as f32 - 0.5;
            let di = ((y * dw + x) as usize) * 3;
            for c in 0..3 {
                let up = bicubic_sample(src, sw, sh, fx, fy, c);
                dst[di + c] = (dst[di + c] * w_dst + up * w_src) * inv;
            }
        }
    }
    Ok(())
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

    fn const_rgb(w: u32, h: u32, r: f32, g: f32, b: f32) -> Vec<f32> {
        let mut v = Vec::with_capacity((w * h * 3) as usize);
        for _ in 0..w * h {
            v.push(r);
            v.push(g);
            v.push(b);
        }
        v
    }

    fn default_params() -> BilateralGridParams {
        BilateralGridParams {
            sigma_s: 6.0,
            sigma_r: 0.1,
            levels: 2,
            strength: 0.8,
            detail_threshold: 0.2,
            ..Default::default()
        }
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

    fn region_mean(rgb: &[f32], w: u32, h: u32, pred: impl Fn(u32, u32) -> bool) -> [f32; 3] {
        let mut acc = [0.0f32; 3];
        let mut n = 0.0f32;
        for y in 0..h {
            for x in 0..w {
                if pred(x, y) {
                    let p = (y * w + x) as usize;
                    for c in 0..3 {
                        acc[c] += rgb[p * 3 + c];
                    }
                    n += 1.0;
                }
            }
        }
        if n > 0.0 {
            for c in 0..3 {
                acc[c] /= n;
            }
        }
        acc
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
        bilateral_grid_smooth(&mut rgb, 48, 48, None, &default_params(), &CancelToken::default())
            .unwrap();
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

    // —— S1.1 P0：trilinear / 蒙版 Splat ——

    #[test]
    fn all_black_image_runs_without_anomaly() {
        // 全黑图像：合法黑色，不得被当作“插值失败”，不得产生 NaN/黑洞扩散。
        let mut rgb = const_rgb(32, 32, 0.0, 0.0, 0.0);
        let p = default_params();
        bilateral_grid_smooth(&mut rgb, 32, 32, None, &p, &CancelToken::default()).unwrap();
        for (i, v) in rgb.iter().enumerate() {
            assert!(v.is_finite(), "idx={i} 非有限值 {v}");
            assert!(*v <= 1e-6, "全黑图像应保持全黑 idx={i} v={v}");
        }
    }

    #[test]
    fn constant_color_image_unchanged() {
        // 常量颜色（含近黑与中灰）处理前后颜色不变。
        for (r, g, b) in [(0.0, 0.0, 0.0), (0.04, 0.04, 0.05), (0.5, 0.45, 0.4), (1.0, 1.0, 1.0)] {
            let mut rgb = const_rgb(40, 40, r, g, b);
            let orig = rgb.clone();
            let p = BilateralGridParams {
                strength: 1.0,
                ..default_params()
            };
            bilateral_grid_smooth(&mut rgb, 40, 40, None, &p, &CancelToken::default()).unwrap();
            for i in 0..rgb.len() {
                assert!(
                    (rgb[i] - orig[i]).abs() < 1e-3,
                    "常量色 ({r},{g},{b}) 应不变 idx={i} got={} want={}",
                    rgb[i],
                    orig[i]
                );
            }
        }
    }

    #[test]
    fn sparse_grid_does_not_darken() {
        // 蒙版稀疏：有效节点权重和不完整时必须归一化，不得变暗。
        let (w, h) = (40u32, 40u32);
        let mut rgb = const_rgb(w, h, 0.5, 0.5, 0.5);
        let mut mask = vec![0f32; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                if x % 5 == 0 && y % 5 == 0 {
                    mask[(y * w + x) as usize] = 1.0;
                }
            }
        }
        let p = BilateralGridParams {
            sigma_s: 4.0,
            sigma_r: 0.1,
            levels: 1,
            strength: 1.0,
            detail_threshold: 0.1,
            ..Default::default()
        };
        bilateral_grid_smooth(&mut rgb, w, h, Some(&mask), &p, &CancelToken::default()).unwrap();
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) as usize;
                if mask[i] > 0.5 {
                    for c in 0..3 {
                        let v = rgb[i * 3 + c];
                        assert!(
                            (v - 0.5).abs() < 5e-3,
                            "稀疏网格不得变暗 ({x},{y}) c={c} v={v}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn trilinear_normalizes_valid_weights() {
        // 单个有效节点、插值权重和远小于 1 → 归一化后应返回节点颜色本身。
        let nx = 2;
        let ny = 2;
        let nv = 2;
        let mut acc = vec![0f32; nx * ny * nv * 4];
        let idx = 0; // (0,0,0)
        acc[idx * 4] = 0.8;
        acc[idx * 4 + 1] = 0.7;
        acc[idx * 4 + 2] = 0.6;
        acc[idx * 4 + 3] = 1.0;
        // 采样点靠近 (0,0,0) 但权重和 = 0.9^3 ≈ 0.729；再测权重极小的情形。
        let (r, g, b) = trilinear(&acc, nx, ny, nv, 0.0, 0.0, 0.0).unwrap();
        assert!((r - 0.8).abs() < 1e-5 && (g - 0.7).abs() < 1e-5 && (b - 0.6).abs() < 1e-5);
        // 采样点在角点远端：有效权重和 = 0.1^3 = 1e-3，未归一化会得到 ~8e-4。
        let (r2, g2, b2) = trilinear(&acc, nx, ny, nv, 1.0, 1.0, 1.0).unwrap();
        assert!(
            (r2 - 0.8).abs() < 1e-3,
            "权重和过小也必须归一化 r2={r2}"
        );
        assert!((g2 - 0.7).abs() < 1e-3 && (b2 - 0.6).abs() < 1e-3);
    }

    #[test]
    fn trilinear_black_pixel_is_valid_not_failure() {
        // 合法黑色节点：RGB 总和为 0 但权重有效 → 返回黑色，而不是回退路径误判。
        let nx = 2;
        let ny = 2;
        let nv = 2;
        let mut acc = vec![0f32; nx * ny * nv * 4];
        acc[3] = 1.0; // (0,0,0) 有效、颜色 (0,0,0)
        let out = trilinear(&acc, nx, ny, nv, 0.0, 0.0, 0.0);
        assert_eq!(out, Some((0.0, 0.0, 0.0)));
    }

    #[test]
    fn trilinear_falls_back_to_nearest_valid_node() {
        // 采样点周围 8 角点全无效，但半径 ≤3 内有有效节点 → 回退到该节点颜色。
        let nx = 6;
        let ny = 6;
        let nv = 6;
        let mut acc = vec![0f32; nx * ny * nv * 4];
        let node = ((3 * nx + 3) * nv + 3) * 4;
        acc[node] = 0.33;
        acc[node + 1] = 0.44;
        acc[node + 2] = 0.55;
        acc[node + 3] = 1.0;
        // 采样 (0.5,0.5,0.5)：角点 (0/1,0/1,0/1) 全无效；(3,3,3) 距离 3 → 应找到。
        let out = trilinear(&acc, nx, ny, nv, 0.5, 0.5, 0.5).unwrap();
        assert!((out.0 - 0.33).abs() < 1e-6 && (out.1 - 0.44).abs() < 1e-6);
        // 完全空网格 → None（调用方保持输入像素）。
        let empty = vec![0f32; nx * ny * nv * 4];
        assert!(trilinear(&empty, nx, ny, nv, 2.5, 2.5, 2.5).is_none());
    }

    #[test]
    fn trilinear_rejects_non_finite_coords() {
        let acc = vec![0f32; 2 * 2 * 2 * 4];
        assert!(trilinear(&acc, 2, 2, 2, f32::NAN, 0.0, 0.0).is_none());
        assert!(trilinear(&acc, 2, 2, 2, 0.0, f32::INFINITY, 0.0).is_none());
    }

    #[test]
    fn mask_one_actively_smooths() {
        let (w, h) = (32u32, 32u32);
        let mut rgb = gradient_rgb(w, h);
        for p in 0..(w * h) as usize {
            let n = ((p % 5) as f32 - 2.0) * 0.03;
            rgb[p * 3] = (rgb[p * 3] + n).clamp(0.0, 1.0);
        }
        let before = high_freq_energy(&rgb, w, h);
        let mask = vec![1.0f32; (w * h) as usize];
        bilateral_grid_smooth(
            &mut rgb,
            w,
            h,
            Some(&mask),
            &default_params(),
            &CancelToken::default(),
        )
        .unwrap();
        let after = high_freq_energy(&rgb, w, h);
        assert!(after < before, "mask=1 应产生平滑 {before} → {after}");
        assert!(rgb.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn black_background_does_not_darken_skin() {
        // 皮肤区（左）旁放纯黑背景（右）：皮肤内部不得变暗。
        let (w, h) = (64u32, 48u32);
        let mut rgb = const_rgb(w, h, 0.6, 0.5, 0.45);
        for y in 0..h {
            for x in w / 2..w {
                let p = (y * w + x) as usize;
                rgb[p * 3] = 0.0;
                rgb[p * 3 + 1] = 0.0;
                rgb[p * 3 + 2] = 0.0;
            }
        }
        // 蒙版：皮肤侧羽化（完全落在皮肤内），背景 m=0。
        let mut mask = vec![0f32; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                let m = if x < w / 2 - 6 {
                    1.0
                } else if x < w / 2 {
                    (w / 2 - x) as f32 / 6.0
                } else {
                    0.0
                };
                mask[(y * w + x) as usize] = m;
            }
        }
        let p = BilateralGridParams {
            sigma_s: 5.0,
            sigma_r: 0.1,
            levels: 2,
            strength: 1.0,
            detail_threshold: 0.1,
            ..Default::default()
        };
        let pred = |x: u32, _y: u32| x < w / 2 - 6;
        let before = region_mean(&rgb, w, h, pred);
        bilateral_grid_smooth(&mut rgb, w, h, Some(&mask), &p, &CancelToken::default()).unwrap();
        let after = region_mean(&rgb, w, h, pred);
        for c in 0..3 {
            assert!(
                after[c] >= before[c] - 0.01,
                "黑背景不得让皮肤变暗 c={c} before={} after={}",
                before[c],
                after[c]
            );
        }
    }

    #[test]
    fn white_background_does_not_brighten_skin() {
        let (w, h) = (64u32, 48u32);
        let mut rgb = const_rgb(w, h, 0.45, 0.38, 0.34);
        for y in 0..h {
            for x in w / 2..w {
                let p = (y * w + x) as usize;
                rgb[p * 3] = 1.0;
                rgb[p * 3 + 1] = 1.0;
                rgb[p * 3 + 2] = 1.0;
            }
        }
        let mut mask = vec![0f32; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                let m = if x < w / 2 - 6 {
                    1.0
                } else if x < w / 2 {
                    (w / 2 - x) as f32 / 6.0
                } else {
                    0.0
                };
                mask[(y * w + x) as usize] = m;
            }
        }
        let p = BilateralGridParams {
            sigma_s: 5.0,
            sigma_r: 0.1,
            levels: 2,
            strength: 1.0,
            detail_threshold: 0.1,
            ..Default::default()
        };
        let pred = |x: u32, _y: u32| x < w / 2 - 6;
        let before = region_mean(&rgb, w, h, pred);
        bilateral_grid_smooth(&mut rgb, w, h, Some(&mask), &p, &CancelToken::default()).unwrap();
        let after = region_mean(&rgb, w, h, pred);
        for c in 0..3 {
            assert!(
                after[c] <= before[c] + 0.01,
                "白背景不得让皮肤变亮 c={c} before={} after={}",
                before[c],
                after[c]
            );
        }
    }

    #[test]
    fn saturated_red_background_does_not_bleed_into_skin() {
        let (w, h) = (64u32, 48u32);
        let mut rgb = const_rgb(w, h, 0.55, 0.48, 0.42);
        for y in 0..h {
            for x in w / 2..w {
                let p = (y * w + x) as usize;
                rgb[p * 3] = 0.9;
                rgb[p * 3 + 1] = 0.05;
                rgb[p * 3 + 2] = 0.05;
            }
        }
        let mut mask = vec![0f32; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                let m = if x < w / 2 - 6 {
                    1.0
                } else if x < w / 2 {
                    (w / 2 - x) as f32 / 6.0
                } else {
                    0.0
                };
                mask[(y * w + x) as usize] = m;
            }
        }
        let p = BilateralGridParams {
            sigma_s: 5.0,
            sigma_r: 0.1,
            levels: 2,
            strength: 1.0,
            detail_threshold: 0.1,
            ..Default::default()
        };
        let pred = |x: u32, _y: u32| x < w / 2 - 6;
        let before = region_mean(&rgb, w, h, pred);
        bilateral_grid_smooth(&mut rgb, w, h, Some(&mask), &p, &CancelToken::default()).unwrap();
        let after = region_mean(&rgb, w, h, pred);
        // 皮肤色度 (R-B) 不得被红背景拉高。
        let chroma_before = before[0] - before[2];
        let chroma_after = after[0] - after[2];
        assert!(
            chroma_after <= chroma_before + 0.02,
            "红背景不得污染皮肤 chroma {chroma_before} → {chroma_after}"
        );
        assert!(
            after[0] <= before[0] + 0.02,
            "皮肤 R 通道不得被红背景拉高 {} → {}",
            before[0],
            after[0]
        );
    }

    #[test]
    fn eye_and_lip_regions_do_not_pollute_cheek() {
        // 全脸尺寸合成：皮肤底 + 眼睛/嘴唇暗块（mask=0），脸颊（mask=1）不得被拉暗。
        let (w, h) = (96u32, 96u32);
        let mut rgb = const_rgb(w, h, 0.62, 0.5, 0.44);
        let mut mask = vec![1.0f32; (w * h) as usize];
        // 眼睛两块
        for (ex0, ex1) in [(16u32, 40u32), (56u32, 80u32)] {
            for y in 24..40 {
                for x in ex0..ex1 {
                    let p = (y * w + x) as usize;
                    rgb[p * 3] = 0.08;
                    rgb[p * 3 + 1] = 0.07;
                    rgb[p * 3 + 2] = 0.09;
                    mask[p] = 0.0;
                }
            }
        }
        // 嘴唇块
        for y in 66..80 {
            for x in 34..62 {
                let p = (y * w + x) as usize;
                rgb[p * 3] = 0.45;
                rgb[p * 3 + 1] = 0.12;
                rgb[p * 3 + 2] = 0.14;
                mask[p] = 0.0;
            }
        }
        let pred = |x: u32, y: u32| (30..66).contains(&x) && (44..62).contains(&y); // 脸颊带
        let before = region_mean(&rgb, w, h, pred);
        let p = BilateralGridParams {
            sigma_s: 5.0,
            sigma_r: 0.1,
            levels: 2,
            strength: 1.0,
            detail_threshold: 0.1,
            ..Default::default()
        };
        bilateral_grid_smooth(&mut rgb, w, h, Some(&mask), &p, &CancelToken::default()).unwrap();
        let after = region_mean(&rgb, w, h, pred);
        for c in 0..3 {
            assert!(
                (after[c] - before[c]).abs() < 0.02,
                "眼睛/嘴唇不得污染脸颊 c={c} before={} after={}",
                before[c],
                after[c]
            );
        }
    }

    #[test]
    fn distinct_luma_regions_keep_their_colors() {
        // 左右不同亮度/色温区域：σ_r 足够小时互不串色。
        let (w, h) = (64u32, 48u32);
        let mut rgb = const_rgb(w, h, 0.2, 0.2, 0.25);
        for y in 0..h {
            for x in w / 2..w {
                let p = (y * w + x) as usize;
                rgb[p * 3] = 0.8;
                rgb[p * 3 + 1] = 0.75;
                rgb[p * 3 + 2] = 0.7;
            }
        }
        let p = BilateralGridParams {
            sigma_s: 5.0,
            sigma_r: 0.05,
            levels: 1,
            strength: 1.0,
            detail_threshold: 0.1,
            ..Default::default()
        };
        let left = |x: u32, _y: u32| x < w / 2 - 4;
        let right = |x: u32, _y: u32| x >= w / 2 + 4;
        let lb = region_mean(&rgb, w, h, left);
        let rb = region_mean(&rgb, w, h, right);
        bilateral_grid_smooth(&mut rgb, w, h, None, &p, &CancelToken::default()).unwrap();
        let la = region_mean(&rgb, w, h, left);
        let ra = region_mean(&rgb, w, h, right);
        for c in 0..3 {
            assert!(
                (la[c] - lb[c]).abs() < 0.03,
                "左区不得被右区污染 c={c} {} → {}",
                lb[c],
                la[c]
            );
            assert!(
                (ra[c] - rb[c]).abs() < 0.03,
                "右区不得被左区污染 c={c} {} → {}",
                rb[c],
                ra[c]
            );
        }
    }

    #[test]
    fn image_boundary_has_no_brightness_step() {
        // 横向渐变 + 噪声：处理后行均值在图像上下边界不得出现断层。
        let (w, h) = (64u32, 64u32);
        let mut rgb = gradient_rgb(w, h);
        for p in 0..(w * h) as usize {
            let n = ((p % 3) as f32 - 1.0) * 0.01;
            rgb[p * 3] = (rgb[p * 3] + n).clamp(0.0, 1.0);
        }
        let row_means = |img: &[f32]| -> Vec<f32> {
            (0..h)
                .map(|y| {
                    let mut s = 0.0;
                    for x in 0..w {
                        s += img[((y * w + x) as usize) * 3];
                    }
                    s / w as f32
                })
                .collect()
        };
        let before = row_means(&rgb);
        bilateral_grid_smooth(
            &mut rgb,
            w,
            h,
            None,
            &default_params(),
            &CancelToken::default(),
        )
        .unwrap();
        let after = row_means(&rgb);
        let max_jump = |rows: &[f32]| -> f32 {
            rows.windows(2)
                .map(|w| (w[1] - w[0]).abs())
                .fold(0.0f32, f32::max)
        };
        let jump_before = max_jump(&before);
        let jump_after = max_jump(&after);
        assert!(
            jump_after <= jump_before + 0.02,
            "行均值不得在边界产生断层 before_max_jump={jump_before:.4} after={jump_after:.4}"
        );
        // 四角不得出现黑点/异常：与输入角点值保持接近（渐变图左缘 R 本就为 0）。
        let orig = gradient_rgb(w, h);
        for (x, y) in [(0u32, 0u32), (w - 1, 0), (0, h - 1), (w - 1, h - 1)] {
            let i = (y * w + x) as usize;
            for c in 0..3 {
                let v = rgb[i * 3 + c];
                let want = orig[i * 3 + c];
                assert!(
                    v.is_finite() && (v - want).abs() < 0.1,
                    "角点异常 ({x},{y}) c={c} v={v} want={want}"
                );
            }
        }
    }

    #[test]
    fn multi_scale_fusion_weights_documented_and_dc_preserved() {
        // 融合权重公式：level0 w=1；粗层 w=0.35/(levels-L+1)。
        // 常量图在任意层数下融合后仍为常量（各尺度均为同常量，权重平均不变）。
        let mut rgb = const_rgb(48, 48, 0.42, 0.4, 0.38);
        let p = BilateralGridParams {
            levels: 3,
            strength: 1.0,
            ..default_params()
        };
        let stats = bilateral_grid_smooth_with_stats(
            &mut rgb,
            48,
            48,
            None,
            &p,
            &CancelToken::default(),
        )
        .unwrap();
        assert_eq!(stats.level_ms.len(), 4, "levels=3 应有 4 个尺度记录");
        for v in rgb.iter() {
            assert!((v - 0.42).abs() < 1e-3 || (v - 0.4).abs() < 1e-3 || (v - 0.38).abs() < 1e-3);
        }
        // 各通道仍各自保持
        for i in 0..48 * 48 {
            assert!((rgb[i * 3] - 0.42).abs() < 1e-3);
            assert!((rgb[i * 3 + 1] - 0.4).abs() < 1e-3);
            assert!((rgb[i * 3 + 2] - 0.38).abs() < 1e-3);
        }
    }

    #[test]
    fn downsample_area_average_preserves_constant_and_dc() {
        // 非整数缩放（100 → 13）：常量保持；均值不漂移。
        let src = const_rgb(100, 60, 0.6, 0.55, 0.5);
        let out = downsample_rgb(&src, 100, 60, 13, 8);
        for v in out.iter() {
            assert!((v - 0.6).abs() < 1e-4 || (v - 0.55).abs() < 1e-4 || (v - 0.5).abs() < 1e-4);
        }
        // 源为渐变：降采样均值应接近源均值（面积平均无偏）。
        let src = gradient_rgb(64, 32);
        let out = downsample_rgb(&src, 64, 32, 17, 9);
        let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
        let mb = mean(&src);
        let ma = mean(&out);
        assert!(
            (mb - ma).abs() < 0.02,
            "面积平均降采样均值漂移 {mb} → {ma}"
        );
    }

    #[test]
    fn downsample_is_lowpass_not_nearest_alias() {
        // 高频棋盘（最近邻会产生混叠条纹）：面积平均后应接近区域均值。
        let (w, h) = (64u32, 64u32);
        let mut src = vec![0f32; (w * h * 3) as usize];
        for y in 0..h {
            for x in 0..w {
                let p = (y * w + x) as usize;
                let v = if (x + y) % 2 == 0 { 1.0 } else { 0.0 };
                for c in 0..3 {
                    src[p * 3 + c] = v;
                }
            }
        }
        let out = downsample_rgb(&src, w, h, 16, 16);
        for v in out.iter() {
            // 4×4 均匀棋盘平均 = 0.5；允许少量边缘偏差。
            assert!(
                (v - 0.5).abs() < 0.15,
                "棋盘降采样应接近 0.5（低通），got {v}"
            );
        }
    }

    /// 多尺度 vs 单尺度实验：同参数下，levels=3 对大尺度不均匀的去除应优于 levels=0，
    /// 同时高频纹理保留不劣化超过容差（证明保留金字塔混合有实际收益）。
    #[test]
    fn multi_scale_beats_single_scale_on_large_scale_unevenness() {
        let (w, h) = (96u32, 96u32);
        let mut rgb = vec![0f32; (w * h * 3) as usize];
        for y in 0..h {
            for x in 0..w {
                let p = (y * w + x) as usize;
                // 大尺度斑块（周期32，振幅 0.08）+ 细纹理（±0.03）
                let blotch = 0.08
                    * (((x as f32 / 32.0) * std::f32::consts::PI * 2.0).sin()
                        + ((y as f32 / 40.0) * std::f32::consts::PI * 2.0).sin());
                let fine = (((x * 13 + y * 7) % 5) as f32 - 2.0) * 0.015;
                let base = 0.5 + blotch + fine;
                for c in 0..3 {
                    rgb[p * 3 + c] = base.clamp(0.0, 1.0);
                }
            }
        }
        let orig = rgb.clone();

        let low_freq_std = |img: &[f32]| -> f32 {
            // 8×8 盒滤后的行/块均值标准差（衡量大尺度不均匀）。
            let mut block_means = Vec::new();
            for by in 0..h / 8 {
                for bx in 0..w / 8 {
                    let mut s = 0.0;
                    let mut n = 0.0;
                    for y in by * 8..(by + 1) * 8 {
                        for x in bx * 8..(bx + 1) * 8 {
                            s += img[((y * w + x) as usize) * 3];
                            n += 1.0;
                        }
                    }
                    block_means.push(s / n);
                }
            }
            let mean = block_means.iter().sum::<f32>() / block_means.len() as f32;
            let var = block_means
                .iter()
                .map(|m| (m - mean) * (m - mean))
                .sum::<f32>()
                / block_means.len() as f32;
            var.sqrt()
        };

        let mk = |levels: u32| BilateralGridParams {
            sigma_s: 6.0,
            sigma_r: 0.12,
            levels,
            strength: 0.7,
            detail_threshold: 0.2,
            ..Default::default()
        };

        let mut single = orig.clone();
        bilateral_grid_smooth(&mut single, w, h, None, &mk(0), &CancelToken::default()).unwrap();
        let mut multi = orig.clone();
        bilateral_grid_smooth(&mut multi, w, h, None, &mk(3), &CancelToken::default()).unwrap();

        let lf_orig = low_freq_std(&orig);
        let lf_single = low_freq_std(&single);
        let lf_multi = low_freq_std(&multi);
        assert!(
            lf_multi < lf_single - 1e-4,
            "多尺度应更擅长去除大尺度不均匀：single={lf_single:.5} multi={lf_multi:.5} orig={lf_orig:.5}"
        );
        // 高频保留：多尺度不得比单尺度差太多（细节回注机制下应接近）。
        let hf_orig = high_freq_energy(&orig, w, h);
        let hf_single = high_freq_energy(&single, w, h);
        let hf_multi = high_freq_energy(&multi, w, h);
        assert!(
            hf_multi >= hf_single * 0.8,
            "多尺度高频保留不应明显劣于单尺度 single={hf_single:.4} multi={hf_multi:.4} orig={hf_orig:.4}"
        );
    }
}
