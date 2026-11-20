//! 各部位参数 → 控制点位移。
//!
//! 所有位移都在脸部局部坐标中进行，单位为眼距，再由 `face_mesh` 统一
//! 转换到原图像素。每个模块只改自己 tag 范围内的控制点，避免部位互相牵连。

use crate::face_reshape::deformation;
use crate::face_reshape::mesh::ControlPoint;

pub mod chin;
pub mod eyes;
pub mod face;
pub mod manual;
pub mod mouth;
pub mod nose;

pub(crate) fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    if (edge1 - edge0).abs() < 1e-6 {
        return if value >= edge1 { 1.0 } else { 0.0 };
    }
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// 脸部椭圆归一化坐标：`u` 约 -1..1，`v` 约 -1..1（眼睛在 -0.14，下巴在 0.86）。
pub(crate) fn face_norm(local: [f32; 2]) -> [f32; 2] {
    [local[0] / 0.97, (local[1] - 0.18) / 1.30]
}

pub(crate) fn shift(point: &mut ControlPoint, du: f32, dv: f32) {
    deformation::apply(point, du, dv);
}
