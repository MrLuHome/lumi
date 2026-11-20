//! 变形质量保护：三角翻转、网格折叠、位移上限、纹理拉伸。
//!
//! 所有自动塑形在写入像素前必须经过这里。检测到异常时按 0.75 递减强度，
//! 直到网格重新有效；仍然失败则回退为“不变形”，绝不让损坏结果进入管线。

use serde::Serialize;

use crate::face_reshape::mesh::ControlMesh;

/// 面部整体位移上限（相对眼距）。
pub const MAX_FACE_DISPLACEMENT_RATIO: f32 = 0.26;
/// 局部五官位移上限（相对眼距）。
pub const MAX_FEATURE_DISPLACEMENT_RATIO: f32 = 0.14;
/// 三角形面积比允许范围，超出视为明显拉伸/压缩。
pub const MIN_AREA_RATIO: f32 = 0.52;
pub const MAX_AREA_RATIO: f32 = 1.80;

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct QualityReport {
    pub flipped_triangles: usize,
    pub min_area_ratio: f32,
    pub max_area_ratio: f32,
    pub max_displacement_ratio: f32,
    /// 实际采用的强度系数（1 表示未降强度）。
    pub applied_scale: f32,
    pub degraded: bool,
    pub notes: Vec<String>,
}

/// 检查并降强度。返回的报告会写入渲染结果，前端据此提示用户。
pub fn stabilize(mesh: &mut ControlMesh, max_displacement_ratio: f32) -> QualityReport {
    let base: Vec<[f32; 2]> = mesh.points.iter().map(|point| point.to).collect();
    let from: Vec<[f32; 2]> = mesh.points.iter().map(|point| point.from).collect();
    let mut notes = Vec::new();
    let mut scale = 1.0f32;
    let mut last = QualityReport::default();

    for attempt in 0..9 {
        for (index, point) in mesh.points.iter_mut().enumerate() {
            point.to = [
                from[index][0] + (base[index][0] - from[index][0]) * scale,
                from[index][1] + (base[index][1] - from[index][1]) * scale,
            ];
        }
        let flipped = mesh.flipped_triangles();
        let (min_ratio, max_ratio) = mesh.area_ratio();
        let displacement = mesh.max_displacement_ratio();
        last = QualityReport {
            flipped_triangles: flipped,
            min_area_ratio: min_ratio,
            max_area_ratio: max_ratio,
            max_displacement_ratio: displacement,
            applied_scale: scale,
            degraded: scale < 0.999,
            notes: notes.clone(),
        };
        let valid = flipped == 0
            && min_ratio >= MIN_AREA_RATIO
            && max_ratio <= MAX_AREA_RATIO
            && displacement <= max_displacement_ratio + 1e-4;
        if valid {
            if scale < 0.999 {
                notes.push(format!(
                    "参数超过安全范围，已将塑形强度降至 {:.0}%",
                    scale * 100.0
                ));
            }
            last.degraded = scale < 0.999;
            last.notes = notes;
            return last;
        }
        if attempt < 8 {
            notes.push(format!(
                "网格检查未通过（翻转 {}，拉伸 {:.2}×），降低强度重试",
                flipped, max_ratio
            ));
            scale *= 0.75;
        }
    }

    // 最终回退：所有控制点回到原位。
    for (index, point) in mesh.points.iter_mut().enumerate() {
        point.to = from[index];
    }
    notes.push("塑形参数超出可安全变形范围，本次未改变像素".into());
    last.flipped_triangles = 0;
    last.min_area_ratio = 1.0;
    last.max_area_ratio = 1.0;
    last.max_displacement_ratio = 0.0;
    last.applied_scale = 0.0;
    last.degraded = true;
    last.notes = notes;
    last
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::face_reshape::mesh::{ControlPoint, ControlTag};

    fn mesh(targets: Vec<[f32; 2]>, from: Vec<[f32; 2]>) -> ControlMesh {
        let points = from
            .iter()
            .zip(targets.iter())
            .map(|(source, target)| ControlPoint {
                local: [0.0, 0.0],
                local_to: [0.0, 0.0],
                from: *source,
                to: *target,
                tag: ControlTag::Fixed,
                weight: 0.0,
            })
            .collect();
        let mut mesh = ControlMesh {
            points,
            triangles: Vec::new(),
            eye_distance: 20.0,
            bbox: [0.0, 0.0, 20.0, 20.0],
            version: 1,
            signature: String::new(),
        };
        mesh.rebuild_triangles();
        mesh
    }

    #[test]
    fn safe_mesh_is_kept_as_is() {
        let mut mesh = mesh(
            vec![[0.0, 0.0], [10.0, 0.0], [0.0, 10.0], [2.0, 2.0]],
            vec![[0.0, 0.0], [10.0, 0.0], [0.0, 10.0], [2.0, 2.0]],
        );
        let report = stabilize(&mut mesh, MAX_FACE_DISPLACEMENT_RATIO);
        assert_eq!(report.applied_scale, 1.0);
        assert!(!report.degraded);
        assert_eq!(report.flipped_triangles, 0);
    }

    #[test]
    fn extreme_displacement_is_reduced() {
        let mut mesh = mesh(
            vec![[0.0, 0.0], [10.0, 0.0], [0.0, 10.0], [60.0, 2.0]],
            vec![[0.0, 0.0], [10.0, 0.0], [0.0, 10.0], [2.0, 2.0]],
        );
        let report = stabilize(&mut mesh, MAX_FACE_DISPLACEMENT_RATIO);
        assert!(report.degraded);
        assert!(report.applied_scale < 1.0);
        assert!(mesh.max_displacement_ratio() <= MAX_FACE_DISPLACEMENT_RATIO + 1e-3);
    }

    #[test]
    fn flipped_mesh_is_repaired_by_scaling() {
        let mut mesh = mesh(
            vec![[0.0, 0.0], [0.0, 10.0], [10.0, 0.0], [10.0, 10.0]],
            vec![[0.0, 0.0], [10.0, 0.0], [0.0, 10.0], [10.0, 10.0]],
        );
        let report = stabilize(&mut mesh, MAX_FACE_DISPLACEMENT_RATIO);
        assert!(report.flipped_triangles == 0);
        assert!(report.degraded);
    }
}
