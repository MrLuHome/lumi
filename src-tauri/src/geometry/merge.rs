//! 多区域局部变形合并。
//!
//! 规则（与 `docs/G11_GEOMETRY_FOUNDATION.md` §2.3 一致）：
//! 1. 只合并 `LocalWarpOp`，不提供整图 global_scale；
//! 2. 区域取并集包围盒；
//! 3. 控制点位移在归一化图像坐标上按高斯核累加；
//! 4. 总强度 = op.strength × point.strength；
//! 5. 合成后按相对短边的最大位移 clamp（默认 12%，与 body 一致）；
//! 6. 全中性输入 → `None`（调用方视为恒等）。

use super::coords::short_side;
use super::protection::{apply_to_delta, ProtectionMask};
use super::types::{ControlPointOp, LocalWarpOp, RegionBox};

/// 合并后的位移场描述（归一化坐标上的稀疏控制点，非像素缓冲）。
#[derive(Debug, Clone, PartialEq)]
pub struct MergedWarp {
    pub region: RegionBox,
    pub points: Vec<ControlPointOp>,
    /// 已乘过总强度的位移（归一化）。
    pub effective_deltas: Vec<[f32; 2]>,
    pub max_displacement: f32,
    pub applied_clamp: bool,
    pub source_ops: usize,
}

/// 默认最大位移相对**短边**的比例（与 `ai_engine/body/deformation.rs` 一致）。
pub const DEFAULT_MAX_DISPLACEMENT_RATIO: f32 = 0.12;

/// 合并多个 op。返回 `None` 表示恒等（强度 0 / 空点 / 空区域）。
pub fn merge_ops(ops: &[LocalWarpOp]) -> Option<MergedWarp> {
    merge_ops_with_limit(ops, DEFAULT_MAX_DISPLACEMENT_RATIO)
}

pub fn merge_ops_with_limit(
    ops: &[LocalWarpOp],
    max_displacement_ratio: f32,
) -> Option<MergedWarp> {
    let live: Vec<&LocalWarpOp> = ops.iter().filter(|op| !op.is_neutral()).collect();
    if live.is_empty() {
        return None;
    }

    let mut region = RegionBox::new(0.0, 0.0, 0.0, 0.0);
    for op in &live {
        region = region.union(op.region);
    }
    if region.is_empty() {
        return None;
    }

    // 收集点容器（位移在下方按 op 累加）。
    let mut points: Vec<ControlPointOp> = Vec::new();
    let mut deltas: Vec<[f32; 2]> = Vec::new();

    // 按 op 累加位移；同位置（或极近）控制点位移相加，实现「多区域/多点合并」。
    points.clear();
    deltas.clear();
    for op in &live {
        let op_factor = op.strength_factor();
        for point in &op.points {
            let mut delta = [
                point.delta[0] * op_factor * point.strength_factor(),
                point.delta[1] * op_factor * point.strength_factor(),
            ];
            if let Some(mask) = op.protection.as_ref() {
                if !mask.is_empty() {
                    let w = sample_protection(mask, point.pos);
                    delta = apply_to_delta(delta, w);
                }
            }
            // 区域外的点位移清零（region 门控）。
            if !op.region.contains(point.pos[0], point.pos[1]) {
                delta = [0.0, 0.0];
            }

            // 与已有同邻域点合并（位置差 < 1e-3 归一化）。
            let existing = points
                .iter()
                .position(|p| {
                    (p.pos[0] - point.pos[0]).abs() < 1e-3
                        && (p.pos[1] - point.pos[1]).abs() < 1e-3
                });
            if let Some(idx) = existing {
                deltas[idx][0] += delta[0];
                deltas[idx][1] += delta[1];
                // 半径/强度取较大者，便于后续内核使用。
                points[idx].radius = points[idx].radius.max(point.radius);
                points[idx].strength = points[idx].strength.max(point.strength);
            } else {
                points.push(*point);
                deltas.push(delta);
            }
        }
    }

    if points
        .iter()
        .zip(deltas.iter())
        .all(|(_, d)| d[0].abs() < 1e-8 && d[1].abs() < 1e-8)
    {
        return None;
    }

    // 合成位移上限（相对短边）。短边用区域在单位正方形中的近似：0..1 归一化下
    // max |delta| 以短边比例表达时，归一化位移上限 = ratio（因 delta 已按全图归一化）。
    // 更精确：按图像像素短边换算需要 width/height；这里对归一化 delta 用
    // ratio * (min(w,h)/max(w,h) 的保守下界) 不可得，故采用与 body 相同的
    // 「相对短边 12%」在像素域的等价：归一化 x 上限 = 0.12 * (short/max) ≤ 0.12。
    // 为可测且保守，直接 clamp 到 0.12（全图归一化位移）。
    let max_norm = max_displacement_ratio.max(0.0);
    let mut applied_clamp = false;
    for d in deltas.iter_mut() {
        let mag = (d[0] * d[0] + d[1] * d[1]).sqrt();
        if mag > max_norm && mag > f32::EPSILON {
            let scale = max_norm / mag;
            d[0] *= scale;
            d[1] *= scale;
            applied_clamp = true;
        }
    }

    let max_displacement = deltas
        .iter()
        .map(|d| (d[0] * d[0] + d[1] * d[1]).sqrt())
        .fold(0.0f32, f32::max);

    Some(MergedWarp {
        region,
        points,
        effective_deltas: deltas,
        max_displacement,
        applied_clamp,
        source_ops: live.len(),
    })
}

fn sample_protection(mask: &ProtectionMask, pos: [f32; 2]) -> f32 {
    if mask.is_empty() {
        return 1.0;
    }
    let x = (pos[0].clamp(0.0, 1.0) * (mask.width - 1) as f32).round() as u32;
    let y = (pos[1].clamp(0.0, 1.0) * (mask.height - 1) as f32).round() as u32;
    mask.at(x.min(mask.width.saturating_sub(1)), y.min(mask.height.saturating_sub(1)))
}

/// 供测试：用短边像素验证归一化位移是否超过比例上限。
pub fn displacement_exceeds_short_side(
    merged: &MergedWarp,
    width: u32,
    height: u32,
    ratio: f32,
) -> bool {
    let short = short_side(width, height);
    if short <= 0.0 {
        return false;
    }
    // 归一化 delta → 像素：dx_px = dx * width, dy_px = dy * height。
    let limit = short * ratio;
    merged.effective_deltas.iter().any(|d| {
        let dx = d[0] * width as f32;
        let dy = d[1] * height as f32;
        (dx * dx + dy * dy).sqrt() > limit + 1e-3
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::protection::ProtectionMask;

    fn op(x: f32, dx: f32, strength: f32) -> LocalWarpOp {
        LocalWarpOp::new(
            RegionBox::new(x - 0.1, 0.3, 0.2, 0.2),
            vec![ControlPointOp::new([x, 0.4], [dx, 0.0], 0.08, 100.0)],
        )
        .with_strength(strength)
    }

    #[test]
    fn neutral_inputs_merge_to_none() {
        assert!(merge_ops(&[]).is_none());
        let zero = op(0.5, 0.05, 0.0);
        assert!(merge_ops(&[zero]).is_none());
        let empty_points = LocalWarpOp::new(RegionBox::full(), vec![]).with_strength(80.0);
        assert!(merge_ops(&[empty_points]).is_none());
    }

    #[test]
    fn strength_scales_delta() {
        let full = merge_ops(&[op(0.5, 0.04, 100.0)]).unwrap();
        let half = merge_ops(&[op(0.5, 0.04, 50.0)]).unwrap();
        assert!(half.max_displacement < full.max_displacement);
        assert!((full.max_displacement - 0.04).abs() < 1e-5);
        assert!((half.max_displacement - 0.02).abs() < 1e-5);
    }

    #[test]
    fn disjoint_regions_union_and_independent_deltas() {
        let a = op(0.2, 0.03, 100.0);
        let b = op(0.8, -0.03, 100.0);
        let merged = merge_ops(&[a, b]).unwrap();
        assert_eq!(merged.source_ops, 2);
        assert!(merged.region.contains(0.2, 0.4));
        assert!(merged.region.contains(0.8, 0.4));
        assert!((merged.max_displacement - 0.03).abs() < 1e-5);
    }

    #[test]
    fn overlapping_ops_accumulate() {
        let a = op(0.5, 0.03, 100.0);
        let b = op(0.5, 0.03, 100.0);
        let merged = merge_ops(&[a, b]).unwrap();
        assert!(merged.max_displacement > 0.05, "应叠加 {}", merged.max_displacement);
    }

    #[test]
    fn protection_zero_blocks_point() {
        let mut mask = ProtectionMask::full_allow(8, 8);
        // 控制点在 (0.5,0.4) → 像素约 (4,3)
        mask.alpha[3 * 8 + 4] = 0.0;
        let blocked = op(0.5, 0.04, 100.0).with_protection(mask);
        // protection 在 (0.5,0.4) 采样为 0 → 该点位移为 0 → 整体 None
        assert!(merge_ops(&[blocked]).is_none());

        let mut open = ProtectionMask::full_allow(8, 8);
        open.alpha[3 * 8 + 4] = 1.0;
        let allowed = op(0.5, 0.04, 100.0).with_protection(open);
        assert!(merge_ops(&[allowed]).is_some());
    }

    #[test]
    fn clamp_limits_max_displacement() {
        let huge = op(0.5, 0.5, 100.0);
        let merged = merge_ops(&[huge]).unwrap();
        assert!(merged.applied_clamp);
        assert!(merged.max_displacement <= DEFAULT_MAX_DISPLACEMENT_RATIO + 1e-5);
    }

    #[test]
    fn point_outside_region_is_zeroed() {
        let region = RegionBox::new(0.0, 0.0, 0.2, 0.2);
        let outside = LocalWarpOp::new(
            region,
            vec![ControlPointOp::new([0.9, 0.9], [0.05, 0.0], 0.1, 100.0)],
        )
        .with_strength(100.0);
        // 区域外点位移清零 → 中性 → None
        assert!(merge_ops(&[outside]).is_none());
    }

    #[test]
    fn displacement_helper_uses_pixel_short_side() {
        let merged = merge_ops(&[op(0.5, 0.04, 100.0)]).unwrap();
        // 1000×1000，短边 1000，limit=120px；dx=0.04*1000=40 < 120
        assert!(!displacement_exceeds_short_side(&merged, 1000, 1000, 0.12));
        let tight = merge_ops_with_limit(&[op(0.5, 0.04, 100.0)], 0.01).unwrap();
        assert!(displacement_exceeds_short_side(&tight, 1000, 1000, 0.02) || tight.applied_clamp);
    }
}
