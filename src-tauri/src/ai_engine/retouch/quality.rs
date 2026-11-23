//! RetouchQualityEvaluator：分区质量指标（非单一全图数值）。
//!
//! 至少分别报告：皮肤区、保护/边缘区、纹理保留、颜色偏移、过度磨皮风险。

use serde::Serialize;

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RegionQuality {
    /// 该区域像素数。
    pub pixels: u32,
    /// 高频能量（处理前）。
    pub high_freq_before: f32,
    /// 高频能量（处理后）。
    pub high_freq_after: f32,
    /// 保留率 after/before（before≈0 时为 1）。
    pub texture_keep_ratio: f32,
    /// 平均 |ΔRGB| 0..1。
    pub mean_abs_delta: f32,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct QualityReport {
    pub skin: RegionQuality,
    /// 保护权重高的区域（五官等）。
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
        if self.skin.pixels > 0 && self.skin.texture_keep_ratio < 0.25 {
            self.over_smooth_risk = 1.0;
            self.notes.push("皮肤高频保留率过低，疑似过度磨皮".into());
        } else if self.skin.pixels > 0 && self.skin.texture_keep_ratio < 0.45 {
            self.over_smooth_risk = 0.5;
            self.notes.push("皮肤纹理保留偏低".into());
        } else {
            self.over_smooth_risk = 0.0;
        }
        if self.protected.mean_abs_delta > 0.04 {
            self.protected_leak = 1.0;
            self.notes.push("保护区域变化过大".into());
        } else if self.protected.mean_abs_delta > 0.02 {
            self.protected_leak = 0.5;
            self.notes.push("保护区域有可见变化".into());
        } else {
            self.protected_leak = 0.0;
        }
        if self.edge.mean_abs_delta > 0.08 {
            self.notes.push("蒙版边缘过渡偏硬".into());
        }
    }
}

/// 计算处理前后质量（mask/protect 为 ROI 权重 0..1）。
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
    if before_rgb.len() != n * 3 || after_rgb.len() != n * 3 || skin_w.len() != n {
        return report;
    }
    let protect = if protect_w.len() == n {
        protect_w
    } else {
        &vec![1.0f32; n][..]
    };

    // 边缘带：skin 在 0.25..0.75
    let mut acc = |region: &mut RegionQuality, i: usize, skin: f32, prot: f32| {
        let mut hb = 0.0;
        let mut ha = 0.0;
        let mut d = 0.0;
        for c in 0..3 {
            hb += (before_rgb[i * 3 + c]
                - before_rgb[i * 3 + c].min(before_rgb[i * 3 + c]))
                .abs();
            // 邻域高频用水平差近似
            let x = i % width as usize;
            if x > 0 {
                hb += (before_rgb[i * 3 + c] - before_rgb[(i - 1) * 3 + c]).abs();
                ha += (after_rgb[i * 3 + c] - after_rgb[(i - 1) * 3 + c]).abs();
            }
            d += (after_rgb[i * 3 + c] - before_rgb[i * 3 + c]).abs();
        }
        region.pixels += 1;
        region.high_freq_before += hb / 3.0;
        region.high_freq_after += ha / 3.0;
        region.mean_abs_delta += d / 3.0;
        let _ = (skin, prot);
    };

    let mut skin = RegionQuality::default();
    let mut prot = RegionQuality::default();
    let mut edge = RegionQuality::default();
    let mut full = RegionQuality::default();

    for i in 0..n {
        let s = skin_w[i].clamp(0.0, 1.0);
        let p = protect[i].clamp(0.0, 1.0);
        acc(&mut full, i, 1.0, 1.0);
        if s > 0.4 {
            acc(&mut skin, i, s, p);
        }
        if p < 0.35 && s > 0.2 {
            acc(&mut prot, i, s, p);
        }
        if (0.25..=0.75).contains(&s) {
            acc(&mut edge, i, s, p);
        }
    }

    let finish = |r: &mut RegionQuality| {
        if r.pixels > 0 {
            r.high_freq_before /= r.pixels as f32;
            r.high_freq_after /= r.pixels as f32;
            r.mean_abs_delta /= r.pixels as f32;
            r.texture_keep_ratio = if r.high_freq_before < 1e-5 {
                1.0
            } else {
                (r.high_freq_after / r.high_freq_before).clamp(0.0, 2.0)
            };
        }
    };
    finish(&mut skin);
    finish(&mut prot);
    finish(&mut edge);
    finish(&mut full);
    report.skin = skin;
    report.protected = prot;
    report.edge = edge;
    report.full = full;
    report.summarize();
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perfect_keep_and_no_protect_leak() {
        let n = 8 * 8;
        let img = vec![0.5f32; n * 3];
        let skin = vec![1.0f32; n];
        let protect = vec![0.0f32; n];
        // 保护区内故意改一点，测 leak
        let mut after = img.clone();
        for i in 0..n {
            after[i * 3] = 0.5;
        }
        // 无变化
        let q = evaluate(&img, &after, &skin, &protect, 8, 8);
        assert!(q.protected_leak == 0.0, "{:?}", q);
        assert!(q.over_smooth_risk == 0.0);
        assert!((q.skin.texture_keep_ratio - 1.0).abs() < 1e-3);
    }

    #[test]
    fn detects_over_smooth() {
        let n = 8 * 8;
        let mut before = vec![0.5f32; n * 3];
        for i in 0..n {
            before[i * 3] = 0.5 + ((i % 2) as f32) * 0.2; // 高频
        }
        let after = vec![0.5f32; n * 3]; // 抹平
        let skin = vec![1.0f32; n];
        let protect = vec![1.0f32; n];
        let mut q = evaluate(&before, &after, &skin, &protect, 8, 8);
        assert!(
            q.over_smooth_risk > 0.4,
            "应报过度磨皮 risk={}",
            q.over_smooth_risk
        );
    }
}
