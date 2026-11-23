//! 统一几何质量报告与降强度工具。
//!
//! 完整检查仍由既有内核执行：
//! - `face_reshape::quality::stabilize`（三角翻转 / 面积比）
//! - `ai_engine::body::constraints::fold_report`（Jacobian 折叠）
//!
//! G11 只提供跨内核的报告形状与 `degrade_scale`（0.75 递减，与两者一致）。

use serde::Serialize;

/// 降强度步长（与 face quality / body fold 重试一致）。
pub const DEGRADE_STEP: f32 = 0.75;

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GeometryQualityReport {
    pub flipped_triangles: usize,
    pub folded_cells: u32,
    pub max_displacement_ratio: f32,
    /// 实际采用的强度系数（1 = 未降）。
    pub applied_scale: f32,
    pub degraded: bool,
    pub notes: Vec<String>,
}

impl GeometryQualityReport {
    pub fn identity() -> Self {
        Self {
            flipped_triangles: 0,
            folded_cells: 0,
            max_displacement_ratio: 0.0,
            applied_scale: 1.0,
            degraded: false,
            notes: Vec::new(),
        }
    }

    pub fn is_blocking(&self) -> bool {
        self.flipped_triangles > 0 || self.folded_cells > 0
    }
}

/// 单次降强度：`scale * DEGRADE_STEP`，下限 0。
pub fn degrade_scale(scale: f32) -> f32 {
    (scale * DEGRADE_STEP).max(0.0)
}

/// 迭代降强度直到 `is_valid(scale)` 通过或次数用尽。
///
/// `is_valid` 返回 `(是否通过, 该次检查报告)`。历史 notes 会合并进最终结果。
pub fn stabilize_scale<F>(max_scale: f32, attempts: u32, mut is_valid: F) -> GeometryQualityReport
where
    F: FnMut(f32) -> (bool, GeometryQualityReport),
{
    let mut scale = max_scale.clamp(0.0, 1.0);
    let mut history: Vec<String> = Vec::new();
    let mut last = GeometryQualityReport::identity();
    let attempts = attempts.max(1);

    for attempt in 0..attempts {
        let (ok, mut report) = is_valid(scale);
        report.applied_scale = scale;
        report.degraded = scale < 0.999;
        for note in history.iter() {
            if !report.notes.contains(note) {
                report.notes.push(note.clone());
            }
        }
        last = report;

        if ok {
            if scale < 0.999 {
                let note = format!(
                    "参数超过安全范围，已将变形强度降至 {:.0}%",
                    scale * 100.0
                );
                if !last.notes.contains(&note) {
                    last.notes.push(note);
                }
            }
            last.degraded = scale < 0.999;
            return last;
        }

        if attempt + 1 < attempts {
            history.push(format!(
                "几何检查未通过，降低强度重试（当前 {:.0}%）",
                scale * 100.0
            ));
            scale = degrade_scale(scale);
            if scale <= 1e-4 {
                last.degraded = true;
                last.applied_scale = 0.0;
                for note in history.iter() {
                    if !last.notes.contains(note) {
                        last.notes.push(note.clone());
                    }
                }
                last.notes.push("强度已降至 0，放弃本区域变形".into());
                return last;
            }
        }
    }

    // 次数用尽仍未通过：放弃本区域变形（宁可少修）。
    last.degraded = true;
    for note in history.iter() {
        if !last.notes.contains(note) {
            last.notes.push(note.clone());
        }
    }
    if !last.notes.iter().any(|n| n.contains("放弃")) {
        last.notes
            .push("几何检查未通过，已放弃本区域变形".into());
    }
    last
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn degrade_step_matches_existing_strategy() {
        let s = degrade_scale(1.0);
        assert!((s - 0.75).abs() < 1e-6);
        assert_eq!(degrade_scale(0.0), 0.0);
    }

    #[test]
    fn stabilize_accepts_valid_first_try() {
        let report = stabilize_scale(1.0, 9, |scale| {
            (scale >= 0.5, GeometryQualityReport::identity())
        });
        assert!((report.applied_scale - 1.0).abs() < 1e-6);
        assert!(!report.degraded);
    }

    #[test]
    fn stabilize_degrades_until_valid() {
        let report = stabilize_scale(1.0, 9, |scale| {
            let mut r = GeometryQualityReport::identity();
            if scale > 0.4 {
                r.flipped_triangles = 2;
                (false, r)
            } else {
                (true, r)
            }
        });
        assert!(report.degraded);
        assert!(report.applied_scale <= 0.4 + 1e-6);
        assert!(!report.notes.is_empty());
    }

    #[test]
    fn stabilize_gives_up_at_zero() {
        let report = stabilize_scale(1.0, 8, |_| {
            (
                false,
                GeometryQualityReport {
                    flipped_triangles: 1,
                    ..GeometryQualityReport::identity()
                },
            )
        });
        assert!(report.applied_scale <= f32::EPSILON || report.degraded);
        assert!(report.notes.iter().any(|n| n.contains("放弃")));
    }

    #[test]
    fn blocking_flags() {
        let mut r = GeometryQualityReport::identity();
        assert!(!r.is_blocking());
        r.folded_cells = 1;
        assert!(r.is_blocking());
    }
}
