//! 位移施加与限幅。
//!
//! 各部位参数先在局部坐标中产生位移，再统一经过这里做单点限幅，
//! 防止单个控制点被极端参数推到不可控位置。全局三角翻转检查在 quality 中。

use crate::face_reshape::mesh::ControlPoint;

/// 单点位移上限（局部坐标，1 = 一个眼距）。
pub const MAX_LOCAL_DISPLACEMENT: f32 = 0.32;

pub fn clamp_local(du: f32, dv: f32) -> (f32, f32) {
    let length = (du * du + dv * dv).sqrt();
    if length > MAX_LOCAL_DISPLACEMENT {
        let scale = MAX_LOCAL_DISPLACEMENT / length;
        (du * scale, dv * scale)
    } else {
        (du, dv)
    }
}

/// 按环权重施加局部位移。
pub fn apply(point: &mut ControlPoint, du: f32, dv: f32) {
    let (du, dv) = clamp_local(du, dv);
    point.local_to[0] += du * point.weight;
    point.local_to[1] += dv * point.weight;
}

/// 把已有位移整体缩放，用于低置信度降强度或质量回退。
pub fn scale_targets(points: &mut [ControlPoint], factor: f32) {
    let factor = factor.clamp(0.0, 1.0);
    for point in points.iter_mut() {
        let du = point.to[0] - point.from[0];
        let dv = point.to[1] - point.from[1];
        point.to = [point.from[0] + du * factor, point.from[1] + dv * factor];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::face_reshape::mesh::{ControlPoint, ControlTag};

    fn point() -> ControlPoint {
        ControlPoint {
            local: [0.0, 0.0],
            local_to: [0.0, 0.0],
            from: [100.0, 100.0],
            to: [100.0, 100.0],
            tag: ControlTag::Face { ring: 3 },
            weight: 1.0,
        }
    }

    #[test]
    fn displacement_is_clamped() {
        let (du, dv) = clamp_local(1.0, 0.0);
        assert!((du * du + dv * dv).sqrt() <= MAX_LOCAL_DISPLACEMENT + 1e-4);
    }

    #[test]
    fn weight_zero_blocks_movement() {
        let mut point = point();
        point.weight = 0.0;
        apply(&mut point, 0.2, 0.2);
        assert_eq!(point.local_to, point.local);
    }

    #[test]
    fn scaling_targets_keeps_direction() {
        let mut points = vec![point()];
        points[0].to = [110.0, 100.0];
        scale_targets(&mut points, 0.5);
        assert!((points[0].to[0] - 105.0).abs() < 1e-5);
    }
}
