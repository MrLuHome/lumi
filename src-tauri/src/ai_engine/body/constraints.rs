//! 变形质量保护：人脸保护、位移上限、折叠检测。
//!
//! 保护不是「尽量小心」，而是可量化的：
//! - 人脸框内位移为 0，框外 8% 范围内平滑恢复；
//! - 每个网格顶点位移有上限；
//! - 用有限差分检查反向映射的 Jacobian 行列式，检出折叠立即降强度。

use serde::Serialize;

use crate::ai_engine::face_frame::FaceFrame;
use crate::ai_engine::segmentation::alpha::smoothstep;

use super::mesh::GridField;

/// 人脸保护区域（归一化坐标）。只有检测到的人脸才参与，
/// 没有检测结果时不做任何假设。
#[derive(Debug, Clone, Default)]
pub struct FaceGuard {
    rects: Vec<(f32, f32, f32, f32)>,
}

impl FaceGuard {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn from_normalized(rects: &[(f32, f32, f32, f32)]) -> Self {
        Self {
            rects: rects.to_vec(),
        }
    }

    pub fn from_faces(faces: &[FaceFrame]) -> Self {
        let rects = faces
            .iter()
            .filter(|face| face.width > 0.01 && face.height > 0.01)
            .map(|face| (face.x, face.y, face.width, face.height))
            .collect();
        Self { rects }
    }

    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }

    /// 0 表示完全保护，1 表示完全不保护。多个脸取最小值。
    pub fn weight(&self, x: f32, y: f32) -> f32 {
        if self.rects.is_empty() {
            return 1.0;
        }
        let mut weight = 1.0f32;
        for (rx, ry, rw, rh) in &self.rects {
            // 人脸框本身完全保护，框外由 ramp 平滑恢复，不再额外扩张保护范围。
            let left = *rx;
            let top = *ry;
            let right = rx + rw;
            let bottom = ry + rh;
            let ramp = (rw.max(*rh) * 0.1).max(0.02);
            let dx = (left - x).max(x - right).max(0.0);
            let dy = (top - y).max(y - bottom).max(0.0);
            let distance = dx.max(dy);
            let region = if distance <= 0.0 {
                0.0
            } else {
                smoothstep((distance / ramp).clamp(0.0, 1.0))
            };
            weight = weight.min(region);
            if weight <= 0.0 {
                return 0.0;
            }
        }
        weight
    }
}

#[derive(Debug, Clone, Copy, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FoldReport {
    /// Jacobian 行列式 <= 0.05 的采样点数量。
    pub folded_cells: u32,
    pub sampled_cells: u32,
    pub min_determinant: f32,
    pub max_displacement: f32,
}

impl FoldReport {
    pub fn has_fold(&self) -> bool {
        self.folded_cells > 0
    }
}

/// 在 40×40 的规则采样点上检查反向映射 `s = p - d(p)` 的局部可逆性。
pub fn fold_report(field: &GridField) -> FoldReport {
    let steps = 40u32;
    let mut report = FoldReport {
        min_determinant: f32::MAX,
        ..Default::default()
    };
    let step_x = field.width as f32 / steps as f32;
    let step_y = field.height as f32 / steps as f32;
    for row in 0..steps {
        for col in 0..steps {
            let x = (col as f32 + 0.5) * step_x;
            let y = (row as f32 + 0.5) * step_y;
            let (dx, dy) = field.displacement_at(x, y);
            let (dx1, dy1) = field.displacement_at((x + step_x).min(field.width as f32), y);
            let (dx2, dy2) = field.displacement_at(x, (y + step_y).min(field.height as f32));
            // s = p - d ⇒ ∂s/∂x = 1 - ∂dx/∂x, ∂s/∂y = -∂dx/∂y, etc.
            let a = 1.0 - (dx1 - dx) / step_x.max(0.001);
            let b = -((dx2 - dx) / step_y.max(0.001));
            let c = -((dy1 - dy) / step_x.max(0.001));
            let d = 1.0 - (dy2 - dy) / step_y.max(0.001);
            let determinant = a * d - b * c;
            report.sampled_cells += 1;
            report.min_determinant = report.min_determinant.min(determinant);
            if determinant <= 0.05 {
                report.folded_cells += 1;
            }
            report.max_displacement = report.max_displacement.max((dx * dx + dy * dy).sqrt());
        }
    }
    if report.sampled_cells == 0 {
        report.min_determinant = 1.0;
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_guard_allows_everything() {
        let guard = FaceGuard::empty();
        assert_eq!(guard.weight(0.5, 0.5), 1.0);
    }

    #[test]
    fn guard_zero_inside_and_one_far_away() {
        let guard = FaceGuard::from_normalized(&[(0.3, 0.3, 0.3, 0.3)]);
        assert_eq!(guard.weight(0.45, 0.45), 0.0);
        assert_eq!(guard.weight(0.05, 0.05), 1.0);
        let edge = guard.weight(0.62, 0.45);
        assert!(edge > 0.0 && edge < 1.0);
    }

    #[test]
    fn zero_field_has_identity_jacobian() {
        let field = GridField::new(64, 64);
        let report = fold_report(&field);
        assert!(!report.has_fold());
        assert!((report.min_determinant - 1.0).abs() < 0.01);
    }

    #[test]
    fn severe_displacement_is_detected_as_fold() {
        let mut field = GridField::new(128, 128);
        // 左右各向外推 70px，超过半宽后反向映射发生折叠。
        for row in 0..field.rows {
            for col in 0..field.cols {
                let (x, _) = field.vertex(col, row);
                let direction = if x < 64.0 { -1.0 } else { 1.0 };
                field.add(col, row, direction * 80.0, 0.0);
            }
        }
        let report = fold_report(&field);
        assert!(report.has_fold());
    }
}
