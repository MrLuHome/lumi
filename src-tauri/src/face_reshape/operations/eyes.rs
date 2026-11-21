//! 眼部：放大、左右独立、眼宽/眼高、眼距、眼角。
//!
//! 只有眼中心是可靠关键点，因此这里是“锚点级椭圆区域缩放”，
//! 不是眼睑轮廓网格。作用域是围绕眼中心的同心环，外环固定，
//! 眉毛、鼻梁、太阳穴都在外环之外。所有位移上限由质量模块统一约束。

use crate::face_reshape::mesh::{ControlPoint, ControlTag};
use crate::face_reshape::operations::{shift, smoothstep};
use crate::image_engine::params::EyeShape;

const MAX_SCALE: f32 = 0.11;

pub fn apply(points: &mut [ControlPoint], params: &EyeShape, eyes: &[[f32; 2]; 2]) {
    if params.is_neutral() {
        return;
    }
    for point in points.iter_mut() {
        let (side, ring) = match point.tag {
            ControlTag::Eye { side, ring } => (side as usize, ring),
            _ => continue,
        };
        if point.weight <= 0.0 || side > 1 {
            continue;
        }
        let center = eyes[side];
        let rel = [point.local_to[0] - center[0], point.local_to[1] - center[1]];

        let side_size = if side == 0 { params.left } else { params.right } / 100.0;
        let size = (params.size / 100.0 + side_size).clamp(-1.5, 1.5);
        let sx = 1.0 + size * MAX_SCALE + (params.width / 100.0) * 0.06;
        let sy = 1.0 + size * MAX_SCALE + (params.height / 100.0) * 0.06;

        let mut du = rel[0] * (sx - 1.0);
        let mut dv = rel[1] * (sy - 1.0);

        if params.distance != 0.0 {
            // 眼距：沿双眼连线向外/向内平移整个椭圆；靠近鼻梁的一侧减半。
            let sign = center[0].signum();
            let bridge_guard =
                0.35 + 0.65 * smoothstep(0.10, 0.34, center[0].abs() + rel[0].abs() * 0.25);
            du += sign * 0.055 * (params.distance / 100.0) * bridge_guard;
        }

        if params.corner != 0.0 {
            // 眼角：以外眼角方向做很小的垂直剪切，默认 0，不改变正常眼型。
            let rx = if ring >= 2 { 0.14 } else { 0.30 };
            let corner_weight = smoothstep(0.45, 1.0, (rel[0] / rx).abs());
            dv += (params.corner / 100.0)
                * 0.018
                * corner_weight
                * if rel[0] * center[0].signum() > 0.0 {
                    1.0
                } else {
                    -1.0
                };
        }

        shift(point, du, dv);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::face_reshape::mesh::ControlPoint;

    fn eye_point(side: u8, rel_u: f32, rel_v: f32, weight: f32) -> ControlPoint {
        let center = if side == 0 { -0.5 } else { 0.5 };
        ControlPoint {
            local: [center + rel_u, rel_v],
            local_to: [center + rel_u, rel_v],
            from: [0.0, 0.0],
            to: [0.0, 0.0],
            tag: ControlTag::Eye { side, ring: 2 },
            weight,
        }
    }

    const EYES: [[f32; 2]; 2] = [[-0.5, 0.0], [0.5, 0.0]];

    #[test]
    fn size_scales_around_eye_center() {
        let mut points = vec![eye_point(0, 0.14, 0.0, 1.0), eye_point(0, -0.14, 0.0, 1.0)];
        apply(
            &mut points,
            &EyeShape {
                size: 100.0,
                ..Default::default()
            },
            &EYES,
        );
        assert!(points[0].local_to[0] > -0.5 + 0.14);
        assert!(points[1].local_to[0] < -0.5 - 0.14);
    }

    #[test]
    fn left_and_right_are_independent() {
        let mut points = vec![eye_point(0, 0.14, 0.0, 1.0), eye_point(1, 0.14, 0.0, 1.0)];
        apply(
            &mut points,
            &EyeShape {
                right: 100.0,
                ..Default::default()
            },
            &EYES,
        );
        assert!((points[0].local_to[0] - (-0.36)).abs() < 0.001);
        assert!(points[1].local_to[0] > 0.64);
    }

    #[test]
    fn distance_moves_eyes_apart() {
        let mut points = vec![eye_point(0, 0.14, 0.0, 1.0), eye_point(1, 0.14, 0.0, 1.0)];
        apply(
            &mut points,
            &EyeShape {
                distance: 100.0,
                ..Default::default()
            },
            &EYES,
        );
        assert!(points[0].local_to[0] < -0.36);
        assert!(points[1].local_to[0] > 0.64);
    }

    #[test]
    fn outer_ring_is_fixed() {
        let mut points = vec![eye_point(0, 0.46, 0.0, 0.0)];
        apply(
            &mut points,
            &EyeShape {
                size: 100.0,
                distance: 100.0,
                corner: 100.0,
                ..Default::default()
            },
            &EYES,
        );
        assert_eq!(points[0].local_to, points[0].local);
    }
}
