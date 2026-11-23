//! RetouchQualityEvaluator：分区质量指标（非单一全图数值）。
//!
//! 至少分别报告：皮肤区、保护/边缘区、纹理保留、颜色偏移、过度磨皮风险。
//!
//! # 量纲与计算说明（验收口径）
//!
//! - `high_freq_*`：加权平均梯度能量
//!   `g = mean_c ( |I(x)-I(x')| + |I(y)-I(y')| ) / 2`，
//!   其中 x' / y' 为可用方向的邻居（右/下优先，边界回退左/上）。
//!   单位：**0..1 强度 / 像素**（三通道平均后仍是强度差量纲）。
//! - `high_freq2_*`：同一算子在 **2× 盒式降采样** 后的粗尺度上计算（多尺度纹理指标）。
//! - `texture_keep_ratio` / `texture_keep_ratio2`：after/before，**无量纲**；
//!   before ≈ 0（区域本无纹理）时定义为 1.0。
//! - `mean_abs_delta`：加权平均逐像素 `mean_c |after-before|`，单位 **0..1 强度**。
//! - `mean_color_delta`：区域均值向量偏移 `mean_c |mean(after)-mean(before)|`，
//!   衡量低频/整体颜色变化，单位 **0..1 强度**；与纹理无关，常量区偏移也能测到。
//!
//! # 区域权重（软权重，不硬阈值切分）
//!
//! | 区域 | 权重 | 语义 |
//! |---|---|---|
//! | skin | `skin_w` | 皮肤蒙版置信度 |
//! | protected | `skin_w × (1 - allow_change)` | 保护强度（`protect_w`=1 允许改变，0 完全保护） |
//! | edge | `4 · skin_w · (1 - skin_w)` | 蒙版过渡带（skin=0.5 处取峰值 1） |
//! | full | `1` | 全图参考 |
//!
//! # 防护
//!
//! - 输入长度不匹配 → 返回带 note 的默认报告（不 panic）。
//! - 任一参与计算的样本（中心/邻居、before/after）非有限值 → 该像素不计入。
//! - 某区域 `weight_sum ≈ 0` → `pixels = 0`、比率为 1、差值为 0，并写明 note；
//!   所有输出字段保持有限值（无 NaN/Infinity）。

use serde::Serialize;

/// 单区域统计（见模块文档的量纲说明）。
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RegionQuality {
    /// 有效贡献像素数（权重大于阈值且样本有限）。
    pub pixels: u32,
    /// 权重和（归一化分母）。
    pub weight_sum: f32,
    /// 高频能量（处理前），0..1 强度/像素。
    pub high_freq_before: f32,
    /// 高频能量（处理后），0..1 强度/像素。
    pub high_freq_after: f32,
    /// 纹理保留率 after/before（无量纲；before≈0 时为 1）。
    pub texture_keep_ratio: f32,
    /// 多尺度（2× 降采样）高频能量（处理前）。
    pub high_freq2_before: f32,
    /// 多尺度（2× 降采样）高频能量（处理后）。
    pub high_freq2_after: f32,
    /// 多尺度纹理保留率（无量纲）。
    pub texture_keep_ratio2: f32,
    /// 平均 |ΔRGB|（0..1 强度）。
    pub mean_abs_delta: f32,
    /// 区域均值颜色偏移（0..1 强度，低频变化）。
    pub mean_color_delta: f32,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct QualityReport {
    pub skin: RegionQuality,
    /// 保护权重高的区域（五官等；`protect_w` 低 = 保护强）。
    pub protected: RegionQuality,
    /// 蒙版边缘带。
    pub edge: RegionQuality,
    /// 全图（参考）。
    pub full: RegionQuality,
    /// 皮肤区高频掉太多 → 过度磨皮风险 0..1。
    pub over_smooth_risk: f32,
    /// 保护区内变化过大 → 0..1（应接近 0）。
    pub protected_leak: f32,
    pub notes: Vec<String>,
}

impl QualityReport {
    pub fn summarize(&mut self) {
        self.notes.clear();
        // —— 过度磨皮 ——
        if self.skin.pixels == 0 {
            self.over_smooth_risk = 0.0;
            self.notes.push("皮肤区无有效像素，未评估纹理保留".into());
        } else if self.skin.texture_keep_ratio < 0.25 {
            self.over_smooth_risk = 1.0;
            self.notes.push("皮肤高频保留率过低，疑似过度磨皮".into());
        } else if self.skin.texture_keep_ratio < 0.45 {
            self.over_smooth_risk = 0.5;
            self.notes.push("皮肤纹理保留偏低".into());
        } else {
            self.over_smooth_risk = 0.0;
        }
        // —— 保护泄漏 ——
        if self.protected.pixels == 0 {
            self.protected_leak = 0.0;
            self.notes.push("保护区无有效像素，未评估保护泄漏".into());
        } else if self.protected.mean_abs_delta > 0.04 {
            self.protected_leak = 1.0;
            self.notes.push("保护区域变化过大".into());
        } else if self.protected.mean_abs_delta > 0.02 {
            self.protected_leak = 0.5;
            self.notes.push("保护区域有可见变化".into());
        } else {
            self.protected_leak = 0.0;
        }
        // —— 边缘断层 ——
        if self.edge.pixels > 0 && self.edge.mean_abs_delta > 0.08 {
            self.notes.push("蒙版边缘过渡偏硬".into());
        }
        // —— 低频颜色偏移 ——
        if self.skin.pixels > 0 && self.skin.mean_color_delta > 0.03 {
            self.notes.push("皮肤低频颜色偏移".into());
        }
        // —— NaN/Infinity 防线：任何字段异常都显式记为 0 ——
        let fix = |v: &mut f32| {
            if !v.is_finite() {
                *v = 0.0;
            }
        };
        for r in [
            &mut self.skin,
            &mut self.protected,
            &mut self.edge,
            &mut self.full,
        ] {
            fix(&mut r.high_freq_before);
            fix(&mut r.high_freq_after);
            fix(&mut r.texture_keep_ratio);
            fix(&mut r.high_freq2_before);
            fix(&mut r.high_freq2_after);
            fix(&mut r.texture_keep_ratio2);
            fix(&mut r.mean_abs_delta);
            fix(&mut r.mean_color_delta);
            fix(&mut r.weight_sum);
        }
        fix(&mut self.over_smooth_risk);
        fix(&mut self.protected_leak);
    }
}

/// 区域累加器（内部）。
#[derive(Default, Clone)]
struct Acc {
    weight_sum: f64,
    pixels: u32,
    hf_b: f64,
    hf_a: f64,
    hf2_b: f64,
    hf2_a: f64,
    abs_delta: f64,
    sum_b: [f64; 3],
    sum_a: [f64; 3],
}

impl Acc {
    fn push(
        &mut self,
        w: f32,
        g1b: f32,
        g1a: f32,
        g2b: f32,
        g2a: f32,
        delta: f32,
        cb: [f32; 3],
        ca: [f32; 3],
    ) {
        let w = w as f64;
        self.weight_sum += w;
        self.pixels += 1;
        self.hf_b += w * g1b as f64;
        self.hf_a += w * g1a as f64;
        self.hf2_b += w * g2b as f64;
        self.hf2_a += w * g2a as f64;
        self.abs_delta += w * delta as f64;
        for c in 0..3 {
            self.sum_b[c] += w * cb[c] as f64;
            self.sum_a[c] += w * ca[c] as f64;
        }
    }

    fn finish(self) -> RegionQuality {
        let mut r = RegionQuality::default();
        if self.weight_sum <= 1e-9 {
            // 有效像素为零：明确归零 + 比率取 1（无纹理可保留 / 无变化可报）。
            r.pixels = 0;
            r.weight_sum = 0.0;
            r.high_freq_before = 0.0;
            r.high_freq_after = 0.0;
            r.texture_keep_ratio = 1.0;
            r.high_freq2_before = 0.0;
            r.high_freq2_after = 0.0;
            r.texture_keep_ratio2 = 1.0;
            r.mean_abs_delta = 0.0;
            r.mean_color_delta = 0.0;
            return r;
        }
        let inv = 1.0 / self.weight_sum;
        r.pixels = self.pixels;
        r.weight_sum = self.weight_sum as f32;
        r.high_freq_before = (self.hf_b * inv) as f32;
        r.high_freq_after = (self.hf_a * inv) as f32;
        r.high_freq2_before = (self.hf2_b * inv) as f32;
        r.high_freq2_after = (self.hf2_a * inv) as f32;
        r.mean_abs_delta = (self.abs_delta * inv) as f32;
        let mut color_delta = 0.0f64;
        for c in 0..3 {
            let mb = self.sum_b[c] * inv;
            let ma = self.sum_a[c] * inv;
            color_delta += (ma - mb).abs();
        }
        r.mean_color_delta = (color_delta / 3.0) as f32;
        r.texture_keep_ratio = ratio(r.high_freq_after, r.high_freq_before);
        r.texture_keep_ratio2 = ratio(r.high_freq2_after, r.high_freq2_before);
        r
    }
}

fn ratio(after: f32, before: f32) -> f32 {
    if !before.is_finite() || before < 1e-5 {
        1.0
    } else {
        (after / before).clamp(0.0, 2.0)
    }
}

/// 单像素梯度能量：`( |Ix| + |Iy| ) / 2` 的三通道平均；邻居缺失返回 None。
fn grad_energy(img: &[f32], w: usize, h: usize, x: usize, y: usize) -> Option<f32> {
    let i = y * w + x;
    let mut sum = 0.0f32;
    for c in 0..3 {
        let center = img[i * 3 + c];
        if !center.is_finite() {
            return None;
        }
        let ix = if x + 1 < w {
            let r = img[(i + 1) * 3 + c];
            if !r.is_finite() {
                return None;
            }
            (center - r).abs()
        } else if x > 0 {
            let l = img[(i - 1) * 3 + c];
            if !l.is_finite() {
                return None;
            }
            (center - l).abs()
        } else {
            0.0
        };
        let iy = if y + 1 < h {
            let d = img[(i + w) * 3 + c];
            if !d.is_finite() {
                return None;
            }
            (center - d).abs()
        } else if y > 0 {
            let u = img[(i - w) * 3 + c];
            if !u.is_finite() {
                return None;
            }
            (center - u).abs()
        } else {
            0.0
        };
        sum += (ix + iy) * 0.5;
    }
    Some(sum / 3.0)
}

/// 2× 盒式降采样（单通道/权重共用），奇数边回退单样本。
fn downsample2(src: &[f32], w: usize, h: usize) -> (Vec<f32>, usize, usize) {
    let dw = (w + 1) / 2;
    let dh = (h + 1) / 2;
    let mut out = vec![0f32; dw * dh];
    for dy in 0..dh {
        for dx in 0..dw {
            let x0 = dx * 2;
            let y0 = dy * 2;
            let x1 = (x0 + 2).min(w);
            let y1 = (y0 + 2).min(h);
            let mut s = 0.0f32;
            let mut n = 0.0f32;
            for y in y0..y1 {
                for x in x0..x1 {
                    let v = src[y * w + x];
                    if v.is_finite() {
                        s += v;
                        n += 1.0;
                    }
                }
            }
            out[dy * dw + dx] = if n > 0.0 { s / n } else { f32::NAN };
        }
    }
    (out, dw, dh)
}

fn downsample2_rgb(src: &[f32], w: usize, h: usize) -> (Vec<f32>, usize, usize) {
    let dw = (w + 1) / 2;
    let dh = (h + 1) / 2;
    let mut out = vec![0f32; dw * dh * 3];
    for c in 0..3 {
        let mut plane = vec![0f32; w * h];
        for i in 0..w * h {
            plane[i] = src[i * 3 + c];
        }
        let (d, _, _) = downsample2(&plane, w, h);
        for i in 0..dw * dh {
            out[i * 3 + c] = d[i];
        }
    }
    (out, dw, dh)
}

/// 计算处理前后质量（`skin_w` = 皮肤蒙版置信度；`protect_w` = 允许变化权重 1=允许，0=完全保护）。
///
/// 输入尺寸不一致、含非法长度时返回带 note 的默认报告。
pub fn evaluate(
    before_rgb: &[f32],
    after_rgb: &[f32],
    skin_w: &[f32],
    protect_w: &[f32],
    width: u32,
    height: u32,
) -> QualityReport {
    let n = (width * height) as usize;
    let mut report = QualityReport::default();
    if width < 2 || height < 2 {
        report.notes.push("图像过小，未评估".into());
        return report;
    }
    if before_rgb.len() != n * 3 || after_rgb.len() != n * 3 || skin_w.len() != n {
        report.notes.push("质量评估输入尺寸不匹配".into());
        return report;
    }
    let protect: Vec<f32> = if protect_w.len() == n {
        protect_w.to_vec()
    } else {
        vec![1.0; n]
    };

    // —— 预计算：有效标记、尺度1 梯度、2× 降采样与尺度2 梯度 ——
    let w = width as usize;
    let h = height as usize;
    let mut valid = vec![false; n];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let s = skin_w[i];
            let center_ok = before_rgb[i * 3..i * 3 + 3].iter().all(|v| v.is_finite())
                && after_rgb[i * 3..i * 3 + 3].iter().all(|v| v.is_finite())
                && s.is_finite();
            valid[i] = center_ok
                && grad_energy(before_rgb, w, h, x, y).is_some()
                && grad_energy(after_rgb, w, h, x, y).is_some();
        }
    }

    let (cb, cw2, ch2) = downsample2_rgb(before_rgb, w, h);
    let (ca, _, _) = downsample2_rgb(after_rgb, w, h);

    // 尺度 2 有效性：块内至少一个有效细像素，且块均值有限。
    let mut valid2 = vec![false; cw2 * ch2];
    let mut count2 = vec![0u32; cw2 * ch2];
    for y in 0..h {
        for x in 0..w {
            if valid[y * w + x] {
                count2[(y / 2) * cw2 + (x / 2)] += 1;
            }
        }
    }
    for j in 0..cw2 * ch2 {
        valid2[j] = count2[j] > 0
            && cb[j * 3..j * 3 + 3].iter().all(|v| v.is_finite())
            && ca[j * 3..j * 3 + 3].iter().all(|v| v.is_finite());
    }

    let grad2 = |img: &[f32], x: usize, y: usize| -> Option<f32> {
        grad_energy(img, cw2, ch2, x, y)
    };

    let mut skin_acc = Acc::default();
    let mut prot_acc = Acc::default();
    let mut edge_acc = Acc::default();
    let mut full_acc = Acc::default();

    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if !valid[i] {
                continue;
            }
            let s = skin_w[i].clamp(0.0, 1.0);
            let p = protect[i].clamp(0.0, 1.0);
            let g1b = grad_energy(before_rgb, w, h, x, y).unwrap_or(0.0);
            let g1a = grad_energy(after_rgb, w, h, x, y).unwrap_or(0.0);
            let (g2b, g2a) = match valid2.get((y / 2) * cw2 + (x / 2)).copied() {
                Some(true) => (
                    grad2(&cb, x / 2, y / 2).unwrap_or(0.0),
                    grad2(&ca, x / 2, y / 2).unwrap_or(0.0),
                ),
                _ => (0.0, 0.0),
            };
            let mut delta = 0.0f32;
            for c in 0..3 {
                delta += (after_rgb[i * 3 + c] - before_rgb[i * 3 + c]).abs();
            }
            delta /= 3.0;
            let cb_px = [
                before_rgb[i * 3],
                before_rgb[i * 3 + 1],
                before_rgb[i * 3 + 2],
            ];
            let ca_px = [
                after_rgb[i * 3],
                after_rgb[i * 3 + 1],
                after_rgb[i * 3 + 2],
            ];

            full_acc.push(1.0, g1b, g1a, g2b, g2a, delta, cb_px, ca_px);
            if s > 1e-4 {
                skin_acc.push(s, g1b, g1a, g2b, g2a, delta, cb_px, ca_px);
                let w_prot = s * (1.0 - p);
                if w_prot > 1e-4 {
                    prot_acc.push(w_prot, g1b, g1a, g2b, g2a, delta, cb_px, ca_px);
                }
                let w_edge = 4.0 * s * (1.0 - s);
                if w_edge > 1e-4 {
                    edge_acc.push(w_edge, g1b, g1a, g2b, g2a, delta, cb_px, ca_px);
                }
            }
        }
    }
    report.skin = skin_acc.finish();
    report.protected = prot_acc.finish();
    report.edge = edge_acc.finish();
    report.full = full_acc.finish();
    report.summarize();
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造测试图：w×h，三通道同值便于断言。
    fn solid(w: u32, h: u32, v: f32) -> Vec<f32> {
        vec![v; (w * h * 3) as usize]
    }

    fn set_px(img: &mut [f32], w: u32, x: u32, y: u32, v: [f32; 3]) {
        let i = (y * w + x) as usize * 3;
        img[i] = v[0];
        img[i + 1] = v[1];
        img[i + 2] = v[2];
    }

    #[test]
    fn identical_before_after_is_perfect() {
        let (w, h) = (16u32, 16u32);
        let mut img = solid(w, h, 0.5);
        // 加一点纹理
        for y in 0..h {
            for x in 0..w {
                if (x + y) % 2 == 0 {
                    set_px(&mut img, w, x, y, [0.55, 0.5, 0.45]);
                }
            }
        }
        let after = img.clone();
        let skin = vec![1.0f32; (w * h) as usize];
        let protect = vec![1.0f32; (w * h) as usize];
        let q = evaluate(&img, &after, &skin, &protect, w, h);
        assert!((q.skin.texture_keep_ratio - 1.0).abs() < 1e-3, "{:?}", q.skin);
        assert!(q.skin.mean_abs_delta < 1e-6);
        assert!(q.skin.mean_color_delta < 1e-6);
        assert_eq!(q.over_smooth_risk, 0.0);
        assert_eq!(q.protected_leak, 0.0);
        assert!(q.full.pixels == w * h);
        // 多尺度指标同样为 1
        assert!((q.skin.texture_keep_ratio2 - 1.0).abs() < 1e-3);
    }

    #[test]
    fn detects_total_texture_loss() {
        let (w, h) = (16u32, 16u32);
        let mut before = solid(w, h, 0.5);
        for y in 0..h {
            for x in 0..w {
                let v = if x % 2 == 0 { 0.7 } else { 0.3 };
                set_px(&mut before, w, x, y, [v, v, v]);
            }
        }
        let after = solid(w, h, 0.5); // 高频全部消失
        let skin = vec![1.0f32; (w * h) as usize];
        let protect = vec![1.0f32; (w * h) as usize];
        let q = evaluate(&before, &after, &skin, &protect, w, h);
        assert!(
            q.skin.texture_keep_ratio < 0.25,
            "高频全灭应触发过度磨皮 keep={}",
            q.skin.texture_keep_ratio
        );
        assert!(q.over_smooth_risk >= 1.0, "{:?}", q.notes);
        // 水平方向纹理（竖条纹）被测到
        assert!(q.skin.high_freq_before > 0.05, "hf={}", q.skin.high_freq_before);
    }

    #[test]
    fn detects_low_freq_color_shift() {
        let (w, h) = (16u32, 16u32);
        let mut before = solid(w, h, 0.5);
        for y in 0..h {
            for x in 0..w {
                if (x + y) % 2 == 0 {
                    set_px(&mut before, w, x, y, [0.55, 0.5, 0.45]);
                }
            }
        }
        // 整体 +0.06 低频偏移，纹理结构不变
        let mut after = before.clone();
        for v in after.iter_mut() {
            *v = (*v + 0.06).min(1.0);
        }
        let skin = vec![1.0f32; (w * h) as usize];
        let protect = vec![1.0f32; (w * h) as usize];
        let q = evaluate(&before, &after, &skin, &protect, w, h);
        assert!(
            q.skin.mean_color_delta > 0.04,
            "低频偏移应被测到 delta={}",
            q.skin.mean_color_delta
        );
        assert!(
            q.skin.texture_keep_ratio > 0.9,
            "纹理结构未变 keep={}",
            q.skin.texture_keep_ratio
        );
        assert!(q.notes.iter().any(|n| n.contains("颜色偏移")), "{:?}", q.notes);
    }

    #[test]
    fn detects_protected_region_modification() {
        let (w, h) = (16u32, 16u32);
        let before = solid(w, h, 0.5);
        let mut after = before.clone();
        // 全图修改 0.1
        for v in after.iter_mut() {
            *v += 0.1;
        }
        let skin = vec![1.0f32; (w * h) as usize];
        // protect_w = 0 → 完全保护（允许变化权重为 0）
        let protect = vec![0.0f32; (w * h) as usize];
        let q = evaluate(&before, &after, &skin, &protect, w, h);
        assert!(q.protected.pixels > 0);
        assert!(
            q.protected.mean_abs_delta > 0.04,
            "保护区被修改应测到 delta={}",
            q.protected.mean_abs_delta
        );
        assert!(q.protected_leak >= 1.0, "{:?}", q.notes);
    }

    #[test]
    fn detects_mask_edge_discontinuity() {
        let (w, h) = (16u32, 16u32);
        let before = solid(w, h, 0.5);
        let mut after = before.clone();
        // 在 skin=0.5 的过渡列制造断层：左 0.5 → 右 0.7，且过渡恰在蒙版半权重带
        let mut skin = vec![0.0f32; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) as usize;
                skin[i] = if x < w / 2 {
                    if x + 1 == w / 2 {
                        0.5 // 边缘带
                    } else {
                        1.0
                    }
                } else if x == w / 2 {
                    0.5
                } else {
                    0.0
                };
                if x >= w / 2 {
                    after[i * 3] = 0.7;
                    after[i * 3 + 1] = 0.7;
                    after[i * 3 + 2] = 0.7;
                }
            }
        }
        let protect = vec![1.0f32; (w * h) as usize];
        let q = evaluate(&before, &after, &skin, &protect, w, h);
        assert!(q.edge.pixels > 0, "应有边缘带像素");
        assert!(
            q.edge.mean_abs_delta > 0.08,
            "蒙版边缘断层应被测到 delta={}",
            q.edge.mean_abs_delta
        );
        assert!(q.notes.iter().any(|n| n.contains("边缘")), "{:?}", q.notes);
    }

    #[test]
    fn only_skin_changes_reports_skin_not_protected() {
        let (w, h) = (16u32, 16u32);
        let before = solid(w, h, 0.5);
        let mut after = before.clone();
        let mut skin = vec![0.0f32; (w * h) as usize];
        let mut protect = vec![1.0f32; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) as usize;
                if x < w / 2 {
                    // 左半：皮肤，允许改变，被修改
                    skin[i] = 1.0;
                    protect[i] = 1.0;
                    after[i * 3] += 0.08;
                    after[i * 3 + 1] += 0.08;
                    after[i * 3 + 2] += 0.08;
                } else {
                    // 右半：保护强度高（允许变化 0），未被修改
                    skin[i] = 1.0;
                    protect[i] = 0.0;
                }
            }
        }
        let q = evaluate(&before, &after, &skin, &protect, w, h);
        assert!(q.skin.mean_abs_delta > 0.03, "{}", q.skin.mean_abs_delta);
        assert!(
            q.protected.mean_abs_delta < 1e-5,
            "保护区不应有变化 {}",
            q.protected.mean_abs_delta
        );
        assert_eq!(q.protected_leak, 0.0);
    }

    #[test]
    fn vertical_stripes_detected_by_horizontal_gradient() {
        // 竖条纹：沿 x 变化 → 水平梯度。
        let (w, h) = (16u32, 16u32);
        let mut before = solid(w, h, 0.5);
        for y in 0..h {
            for x in 0..w {
                let v = if x % 2 == 0 { 0.8 } else { 0.2 };
                set_px(&mut before, w, x, y, [v, v, v]);
            }
        }
        let after = solid(w, h, 0.5);
        let skin = vec![1.0f32; (w * h) as usize];
        let protect = vec![1.0f32; (w * h) as usize];
        let q = evaluate(&before, &after, &skin, &protect, w, h);
        assert!(
            q.skin.high_freq_before > 0.15,
            "竖条纹高频应被测到 {}",
            q.skin.high_freq_before
        );
        assert!(q.skin.texture_keep_ratio < 0.25);
    }

    #[test]
    fn horizontal_stripes_detected_by_vertical_gradient() {
        // 横条纹：沿 y 变化 → 必须靠垂直梯度才能测到。
        let (w, h) = (16u32, 16u32);
        let mut before = solid(w, h, 0.5);
        for y in 0..h {
            for x in 0..w {
                let v = if y % 2 == 0 { 0.8 } else { 0.2 };
                set_px(&mut before, w, x, y, [v, v, v]);
            }
        }
        let after = solid(w, h, 0.5);
        let skin = vec![1.0f32; (w * h) as usize];
        let protect = vec![1.0f32; (w * h) as usize];
        let q = evaluate(&before, &after, &skin, &protect, w, h);
        assert!(
            q.skin.high_freq_before > 0.15,
            "横条纹高频必须被垂直梯度测到 {}",
            q.skin.high_freq_before
        );
        assert!(q.skin.texture_keep_ratio < 0.25);
    }

    #[test]
    fn empty_region_and_invalid_input_are_explicit() {
        // 皮肤蒙版全 0 → 皮肤区无有效像素，字段有限、note 明确。
        let (w, h) = (8u32, 8u32);
        let before = solid(w, h, 0.5);
        let after = solid(w, h, 0.4);
        let skin = vec![0.0f32; (w * h) as usize];
        let protect = vec![0.0f32; (w * h) as usize];
        let q = evaluate(&before, &after, &skin, &protect, w, h);
        assert_eq!(q.skin.pixels, 0);
        assert_eq!(q.skin.texture_keep_ratio, 1.0);
        assert_eq!(q.skin.mean_abs_delta, 0.0);
        assert_eq!(q.over_smooth_risk, 0.0);
        assert!(q.notes.iter().any(|n| n.contains("皮肤区无有效像素")));
        assert!(q.notes.iter().any(|n| n.contains("保护区无有效像素")));
        assert!(q.skin.texture_keep_ratio.is_finite());

        // 尺寸不匹配 → 默认报告 + note，不 panic。
        let q2 = evaluate(&before, &after, &[1.0; 3], &protect, w, h);
        assert!(q2.notes.iter().any(|n| n.contains("尺寸不匹配")));
        assert_eq!(q2.over_smooth_risk, 0.0);
    }

    #[test]
    fn nan_and_infinity_inputs_are_guarded() {
        let (w, h) = (8u32, 8u32);
        let mut before = solid(w, h, 0.5);
        let mut after = solid(w, h, 0.5);
        before[0] = f32::NAN;
        after[10] = f32::INFINITY;
        after[20] = f32::NEG_INFINITY;
        let skin = vec![1.0f32; (w * h) as usize];
        let protect = vec![1.0f32; (w * h) as usize];
        let q = evaluate(&before, &after, &skin, &protect, w, h);
        for v in [
            q.skin.high_freq_before,
            q.skin.high_freq_after,
            q.skin.texture_keep_ratio,
            q.skin.mean_abs_delta,
            q.skin.mean_color_delta,
            q.full.mean_abs_delta,
            q.over_smooth_risk,
            q.protected_leak,
        ] {
            assert!(v.is_finite(), "输出必须有限 {v}");
        }
        // 受污染像素被剔除，其余像素仍参与统计
        assert!(q.full.pixels > (w * h) / 2);
    }
}
