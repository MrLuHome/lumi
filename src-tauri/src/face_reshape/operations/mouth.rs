//! 唇部：宽度、上/下唇厚度、嘴角位置、对称性、整体大小。
//!
//! 唇中心与左右嘴角来自 YuNet 关键点，作用域限制在唇部同心环内。
//! 默认参数全部为 0，不会改变微笑表情；强度超过质量上限时自动降强度。

use crate::face_reshape::mesh::{ControlPoint, ControlTag};
use crate::face_reshape::operations::{shift, smoothstep};
use crate::image_engine::params::MouthShape;

pub fn apply(points: &mut [ControlPoint], params: &MouthShape, center: [f32; 2]) {
    if params.is_neutral() {
        return;
    }
    for point in points.iter_mut() {
        if !matches!(point.tag, ControlTag::Mouth { .. }) || point.weight <= 0.0 {
            continue;
        }
        let rel = [point.local_to[0] - center[0], point.local_to[1] - center[1]];
        let size = params.size / 100.0;
        let sx = 1.0 + size * 0.09 + (params.width / 100.0) * 0.08;
        let vertical = if rel[1] < 0.0 {
            1.0 + (params.upper / 100.0) * 0.06
        } else {
            1.0 + (params.lower / 100.0) * 0.06
        };
        let mut du = rel[0] * (sx - 1.0);
        let mut dv = rel[1] * (vertical - 1.0);

        if params.corner != 0.0 {
            // 只在嘴角附近生效；正值微微上扬，负值下压。
            let corner_weight = smoothstep(0.55, 1.0, (rel[0] / 0.54).abs());
            dv -= (params.corner / 100.0) * 0.020 * corner_weight;
        }
        if params.symmetry != 0.0 {
            du += 0.020 * (params.symmetry / 100.0) * if rel[0] < 0.0 { -1.0 } else { 1.0 };
        }
        shift(point, du, dv);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::face_reshape::mesh::ControlPoint;

    fn mouth_point(u: f32, v: f32, weight: f32) -> ControlPoint {
        ControlPoint {
            local: [u, v],
            local_to: [u, v],
            from: [0.0, 0.0],
            to: [0.0, 0.0],
            tag: ControlTag::Mouth { ring: 2 },
            weight,
        }
    }

    #[test]
    fn width_scales_horizontally() {
        let mut points = vec![mouth_point(0.17, 0.0, 1.0)];
        apply(
            &mut points,
            &MouthShape {
                width: 100.0,
                ..Default::default()
            },
            [0.0, 1.0],
        );
        assert!(points[0].local_to[0] > 0.17);
    }

    #[test]
    fn upper_and_lower_are_independent() {
        let mut points = vec![mouth_point(0.0, 0.91, 1.0), mouth_point(0.0, 1.09, 1.0)];
        apply(
            &mut points,
            &MouthShape {
                upper: 100.0,
                ..Default::default()
            },
            [0.0, 1.0],
        );
        assert!(points[0].local_to[1] < 1.0 - 0.09);
        assert!((points[1].local_to[1] - 1.09).abs() < 1e-5);
    }

    #[test]
    fn corner_raises_only_corner_region() {
        let mut points = vec![mouth_point(0.5, 1.0, 1.0), mouth_point(0.0, 1.0, 1.0)];
        apply(
            &mut points,
            &MouthShape {
                corner: 100.0,
                ..Default::default()
            },
            [0.0, 1.0],
        );
        assert!(points[0].local_to[1] < 1.0);
        assert!((points[1].local_to[1] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn neutral_keeps_points() {
        let mut points = vec![mouth_point(0.3, 0.1, 1.0)];
        apply(&mut points, &MouthShape::default(), [0.0, 1.0]);
        assert_eq!(points[0].local_to, points[0].local);
    }
}
