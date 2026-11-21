//! 鼻部：鼻梁宽度、鼻翼宽度、鼻头大小、视觉高度、左右平衡。
//!
//! 以鼻尖关键点为锚，局部同心环限制在鼻部区域；
//! 鼻孔结构由内环保护，嘴唇与眼睛在作用域之外。

use crate::face_reshape::mesh::{ControlPoint, ControlTag};
use crate::face_reshape::operations::{shift, smoothstep};
use crate::image_engine::params::NoseShape;

pub fn apply(points: &mut [ControlPoint], params: &NoseShape, center: [f32; 2]) {
    if params.is_neutral() {
        return;
    }
    for point in points.iter_mut() {
        if !matches!(point.tag, ControlTag::Nose { .. }) || point.weight <= 0.0 {
            continue;
        }
        let rel = [point.local_to[0] - center[0], point.local_to[1] - center[1]];
        let mut du = 0.0;
        let mut dv = 0.0;

        let tip = params.tip / 100.0;
        if tip != 0.0 {
            du += rel[0] * 0.09 * tip;
            dv += rel[1] * 0.09 * tip;
        }
        if params.bridge != 0.0 {
            // 鼻梁在鼻尖上方（v 更小），越靠上影响越弱，避免动到眉心。
            let upper = 1.0 - smoothstep(-0.30, 0.10, rel[1]);
            du -= rel[0] * 0.05 * (params.bridge / 100.0) * upper;
        }
        if params.alar != 0.0 {
            du += rel[0] * 0.05 * (params.alar / 100.0);
        }
        if params.height != 0.0 {
            dv += rel[1] * 0.04 * (params.height / 100.0);
        }
        if params.balance != 0.0 {
            du += 0.020 * (params.balance / 100.0);
        }
        shift(point, du, dv);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::face_reshape::mesh::ControlPoint;

    fn nose_point(u: f32, v: f32, weight: f32) -> ControlPoint {
        ControlPoint {
            local: [u, v],
            local_to: [u, v],
            from: [0.0, 0.0],
            to: [0.0, 0.0],
            tag: ControlTag::Nose { ring: 2 },
            weight,
        }
    }

    #[test]
    fn tip_scales_outward() {
        let mut points = vec![nose_point(0.12, 0.55, 1.0)];
        apply(
            &mut points,
            &NoseShape {
                tip: 100.0,
                ..Default::default()
            },
            [0.0, 0.55],
        );
        assert!(points[0].local_to[0] > 0.12);
    }

    #[test]
    fn bridge_narrows_upper_nose_only() {
        let mut points = vec![nose_point(0.12, 0.38, 1.0), nose_point(0.12, 0.68, 1.0)];
        apply(
            &mut points,
            &NoseShape {
                bridge: 100.0,
                ..Default::default()
            },
            [0.0, 0.55],
        );
        assert!(points[0].local_to[0] < 0.12);
        assert!((points[1].local_to[0] - 0.12).abs() < 1e-5);
    }

    #[test]
    fn outer_ring_stays_fixed() {
        let mut points = vec![nose_point(0.4, 0.55, 0.0)];
        apply(
            &mut points,
            &NoseShape {
                tip: 100.0,
                alar: 100.0,
                height: 100.0,
                balance: 100.0,
                bridge: 100.0,
            },
            [0.0, 0.55],
        );
        assert_eq!(points[0].local_to, points[0].local);
    }
}
