//! 脸型：整体瘦脸、左右脸、下颌线、颧骨、宽度、长度、左右平衡。
//!
//! 以脸缘轮廓环为控制点，向脸内侧按环权重平滑衰减（0 / 18% / 52% / 100%），
//! 外圈与五官锚点固定，因此背景、眼睛、鼻、唇不会跟着移动。

use crate::face_reshape::mesh::{ControlPoint, ControlTag};
use crate::face_reshape::operations::{face_norm, shift, smoothstep};
use crate::image_engine::params::FaceShape;

/// 单侧最大位移约为 0.085 眼距；多个参数叠加后由质量模块统一降强度。
pub fn apply(points: &mut [ControlPoint], params: &FaceShape) {
    if params.is_neutral() {
        return;
    }
    for point in points.iter_mut() {
        if !matches!(point.tag, ControlTag::Face { .. }) || point.weight <= 0.0 {
            continue;
        }
        let (un, vn) = {
            let norm = face_norm(point.local_to);
            (norm[0], norm[1])
        };
        let side = if un < 0.0 { -1.0 } else { 1.0 };
        let lateral = smoothstep(0.05, 0.55, un.abs());
        let lower = smoothstep(-0.35, 0.75, vn);
        let cheek = (-((vn + 0.02) / 0.48).powi(2)).exp() * lateral;
        let jaw = smoothstep(0.05, 0.9, vn) * lateral;
        let bottom = smoothstep(0.0, 1.0, vn);

        let mut du = 0.0;
        let mut dv = 0.0;

        let slim = params.slim / 100.0;
        if slim != 0.0 {
            du -= side * 0.085 * slim * (0.30 + 0.70 * lower) * lateral;
        }
        let side_param = if side < 0.0 {
            params.left
        } else {
            params.right
        } / 100.0;
        if side_param != 0.0 {
            du -= side * 0.055 * side_param * (0.35 + 0.65 * lower) * lateral;
        }
        if params.width != 0.0 {
            du -= side * 0.045 * (params.width / 100.0) * lateral * (0.35 + 0.65 * lower);
        }
        if params.cheekbone != 0.0 {
            du -= side * 0.050 * (params.cheekbone / 100.0) * cheek;
        }
        if params.jaw != 0.0 {
            du -= side * 0.050 * (params.jaw / 100.0) * jaw;
        }
        if params.balance != 0.0 {
            du += 0.030 * (params.balance / 100.0) * lower;
        }
        if params.length != 0.0 {
            // 只有下半张脸向下延伸，额头与发际线不动。
            dv += 0.060 * (params.length / 100.0) * bottom;
        }

        shift(point, du, dv);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::face_reshape::mesh::ControlPoint;

    fn ring_point(u: f32, v: f32, weight: f32) -> ControlPoint {
        ControlPoint {
            local: [u, v],
            local_to: [u, v],
            from: [0.0, 0.0],
            to: [0.0, 0.0],
            tag: ControlTag::Face { ring: 3 },
            weight,
        }
    }

    #[test]
    fn slim_pushes_both_sides_inward() {
        let mut points = vec![ring_point(-0.9, 0.5, 1.0), ring_point(0.9, 0.5, 1.0)];
        apply(
            &mut points,
            &FaceShape {
                slim: 100.0,
                ..Default::default()
            },
        );
        assert!(points[0].local_to[0] > -0.9);
        assert!(points[1].local_to[0] < 0.9);
        assert!((points[0].local_to[1] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn inner_rings_and_top_are_protected() {
        let mut points = vec![ring_point(-0.9, 0.5, 0.0), ring_point(-0.9, -0.9, 1.0)];
        apply(
            &mut points,
            &FaceShape {
                slim: 100.0,
                jaw: 100.0,
                cheekbone: 100.0,
                ..Default::default()
            },
        );
        assert_eq!(points[0].local_to, points[0].local);
        // 额头仍有少量横向变化，但远小于脸缘。
        assert!(points[1].local_to[0] > -0.9);
        assert!(points[1].local_to[0] - points[1].local[0] > -0.03);
    }

    #[test]
    fn length_only_moves_lower_face() {
        let mut points = vec![ring_point(0.0, -1.0, 1.0), ring_point(0.0, 1.1, 1.0)];
        apply(
            &mut points,
            &FaceShape {
                length: 100.0,
                ..Default::default()
            },
        );
        assert!(points[0].local_to[1] - points[0].local[1] < 0.001);
        assert!(points[1].local_to[1] > points[1].local[1] + 0.02);
    }

    #[test]
    fn left_and_right_are_independent() {
        let mut points = vec![ring_point(-0.9, 0.5, 1.0), ring_point(0.9, 0.5, 1.0)];
        apply(
            &mut points,
            &FaceShape {
                right: 100.0,
                ..Default::default()
            },
        );
        assert!((points[0].local_to[0] - points[0].local[0]).abs() < 1e-5);
        assert!(points[1].local_to[0] < 0.9);
    }
}
