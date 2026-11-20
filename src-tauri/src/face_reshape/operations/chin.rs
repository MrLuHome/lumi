//! 下巴：长度、宽度、尖度、下颌线。
//!
//! 全部作用在脸缘轮廓环的下半部分，与脸型/下颌线连续；
//! 外圈和颈部区域固定，不会出现下巴与脖子脱离或背景弯曲。

use crate::face_reshape::mesh::{ControlPoint, ControlTag};
use crate::face_reshape::operations::{face_norm, shift, smoothstep};
use crate::image_engine::params::ChinShape;

pub fn apply(points: &mut [ControlPoint], params: &ChinShape) {
    if params.is_neutral() {
        return;
    }
    for point in points.iter_mut() {
        if !matches!(point.tag, ControlTag::Face { .. }) || point.weight <= 0.0 {
            continue;
        }
        let norm = face_norm(point.local_to);
        let bottom = smoothstep(0.30, 0.95, norm[1]);
        if bottom <= 0.01 {
            continue;
        }
        let side = if norm[0] < 0.0 { -1.0 } else { 1.0 };
        let lateral = smoothstep(0.05, 0.45, norm[0].abs());
        let center_weight = 1.0 - smoothstep(0.08, 0.50, norm[0].abs());

        let mut du = 0.0;
        let mut dv = 0.0;
        if params.length != 0.0 {
            dv += 0.055 * (params.length / 100.0) * bottom;
        }
        if params.width != 0.0 {
            du -= side * 0.045 * (params.width / 100.0) * bottom * lateral;
        }
        if params.jawline != 0.0 {
            du -= side * 0.035 * (params.jawline / 100.0) * bottom * lateral;
        }
        if params.point != 0.0 {
            // 尖度：下巴中心相对两侧上提，形成更清晰的 V 形，位移很小。
            dv -= 0.030 * (params.point / 100.0) * bottom * center_weight;
        }
        shift(point, du, dv);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::face_reshape::mesh::ControlPoint;

    fn chin_point(u: f32, v: f32) -> ControlPoint {
        ControlPoint {
            local: [u, v],
            local_to: [u, v],
            from: [0.0, 0.0],
            to: [0.0, 0.0],
            tag: ControlTag::Face { ring: 3 },
            weight: 1.0,
        }
    }

    #[test]
    fn length_extends_chin_downward() {
        let mut points = vec![chin_point(0.0, 1.3)];
        apply(
            &mut points,
            &ChinShape {
                length: 100.0,
                ..Default::default()
            },
        );
        assert!(points[0].local_to[1] > 1.3);
    }

    #[test]
    fn forehead_is_untouched() {
        let mut points = vec![chin_point(0.0, -0.9)];
        apply(
            &mut points,
            &ChinShape {
                length: 100.0,
                width: 100.0,
                point: 100.0,
                jawline: 100.0,
            },
        );
        assert_eq!(points[0].local_to, points[0].local);
    }

    #[test]
    fn width_narrows_lower_face() {
        let mut points = vec![chin_point(-0.6, 1.1), chin_point(0.6, 1.1)];
        apply(
            &mut points,
            &ChinShape {
                width: 100.0,
                ..Default::default()
            },
        );
        assert!(points[0].local_to[0] > -0.6);
        assert!(points[1].local_to[0] < 0.6);
    }
}
