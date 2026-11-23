//! 统一局部变形描述类型（非破坏：只描述意图，不改像素）。

use super::protection::ProtectionMask;

/// 归一化图像坐标 0..1。
pub type Normalized = f32;

/// 强度 0..100。0 表示完全不变形。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WarpStrength(pub f32);

impl WarpStrength {
    pub fn new(value: f32) -> Self {
        Self(value.clamp(0.0, 100.0))
    }

    /// 0..1 乘子。
    pub fn factor(self) -> f32 {
        (self.0 / 100.0).clamp(0.0, 1.0)
    }

    pub fn is_neutral(self) -> bool {
        self.0 <= f32::EPSILON
    }
}

/// 归一化包围盒（x,y,w,h ∈ 0..1，相对图像宽高）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RegionBox {
    pub x: Normalized,
    pub y: Normalized,
    pub width: Normalized,
    pub height: Normalized,
}

impl RegionBox {
    pub fn new(x: Normalized, y: Normalized, width: Normalized, height: Normalized) -> Self {
        Self {
            x: x.clamp(0.0, 1.0),
            y: y.clamp(0.0, 1.0),
            width: width.clamp(0.0, 1.0),
            height: height.clamp(0.0, 1.0),
        }
    }

    /// 全图区域（仅用于保护/门控底图，不提供整图缩放变形）。
    pub fn full() -> Self {
        Self::new(0.0, 0.0, 1.0, 1.0)
    }

    pub fn is_empty(&self) -> bool {
        self.width <= f32::EPSILON || self.height <= f32::EPSILON
    }

    pub fn union(self, other: RegionBox) -> RegionBox {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        let x0 = self.x.min(other.x);
        let y0 = self.y.min(other.y);
        let x1 = (self.x + self.width).max(other.x + other.width);
        let y1 = (self.y + self.height).max(other.y + other.height);
        RegionBox::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
    }

    pub fn contains(&self, x: Normalized, y: Normalized) -> bool {
        !self.is_empty()
            && x >= self.x
            && y >= self.y
            && x <= self.x + self.width
            && y <= self.y + self.height
    }
}

/// 单控制点：位置 + 影响半径 + 位移 + 点强度。
///
/// `delta` 与 `BodyAnchor.dx/dy` 同量纲（相对图像宽/高的归一化位移）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControlPointOp {
    pub pos: [Normalized; 2],
    pub delta: [f32; 2],
    /// 短边归一化影响半径（与 BodyAnchor.radius 一致）。
    pub radius: Normalized,
    /// 0..100。
    pub strength: f32,
}

impl ControlPointOp {
    pub fn new(
        pos: [Normalized; 2],
        delta: [f32; 2],
        radius: Normalized,
        strength: f32,
    ) -> Self {
        Self {
            pos: [pos[0].clamp(0.0, 1.0), pos[1].clamp(0.0, 1.0)],
            delta,
            radius: radius.clamp(0.001, 1.0),
            strength: WarpStrength::new(strength).0,
        }
    }

    pub fn strength_factor(&self) -> f32 {
        WarpStrength::new(self.strength).factor()
    }
}

/// 一次局部变形请求。非破坏：写入项目前仍映射为既有 `BodyAnchor`/`FaceReshape` 或
/// 由 G13 内核消费；G11 只做合成与校验。
#[derive(Debug, Clone, PartialEq)]
pub struct LocalWarpOp {
    pub region: RegionBox,
    pub points: Vec<ControlPointOp>,
    /// 总强度 0..100。
    pub strength: f32,
    /// 边界羽化 0..100（0 = 硬边，100 = 最软）。
    pub feather: f32,
    /// 允许位移权重（与图像同尺寸）；None = 不额外限制（仍受 region 约束）。
    pub protection: Option<ProtectionMask>,
}

impl LocalWarpOp {
    pub fn new(region: RegionBox, points: Vec<ControlPointOp>) -> Self {
        Self {
            region,
            points,
            strength: 100.0,
            feather: 50.0,
            protection: None,
        }
    }

    pub fn with_strength(mut self, strength: f32) -> Self {
        self.strength = WarpStrength::new(strength).0;
        self
    }

    pub fn with_feather(mut self, feather: f32) -> Self {
        self.feather = feather.clamp(0.0, 100.0);
        self
    }

    pub fn with_protection(mut self, mask: ProtectionMask) -> Self {
        self.protection = Some(mask);
        self
    }

    pub fn strength_factor(&self) -> f32 {
        WarpStrength::new(self.strength).factor()
    }

    /// 强度为 0 或没有可位移控制点 → 恒等。
    pub fn is_neutral(&self) -> bool {
        self.strength_factor() <= f32::EPSILON
            || self.points.is_empty()
            || self
                .points
                .iter()
                .all(|p| p.delta[0].abs() < 1e-6 && p.delta[1].abs() < 1e-6)
            || self.region.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strength_zero_is_neutral() {
        let op = LocalWarpOp::new(
            RegionBox::new(0.2, 0.2, 0.3, 0.3),
            vec![ControlPointOp::new([0.3, 0.3], [0.05, 0.0], 0.1, 100.0)],
        )
        .with_strength(0.0);
        assert!(op.is_neutral());
    }

    #[test]
    fn empty_points_is_neutral() {
        let op = LocalWarpOp::new(RegionBox::full(), vec![]);
        assert!(op.is_neutral());
    }

    #[test]
    fn region_union_covers_both() {
        let a = RegionBox::new(0.0, 0.0, 0.2, 0.2);
        let b = RegionBox::new(0.5, 0.5, 0.2, 0.2);
        let u = a.union(b);
        assert!(u.contains(0.1, 0.1));
        assert!(u.contains(0.6, 0.6));
        // 并集包围盒会覆盖中间空隙；用盒外点验证。
        assert!(!u.contains(0.3, 0.8));
        assert!(!u.contains(0.9, 0.1));
        assert!((u.x - 0.0).abs() < 1e-6);
        assert!((u.width - 0.7).abs() < 1e-6);
    }

    #[test]
    fn strength_clamps_to_0_100() {
        assert_eq!(WarpStrength::new(-5.0).0, 0.0);
        assert_eq!(WarpStrength::new(150.0).0, 100.0);
        assert!((WarpStrength::new(50.0).factor() - 0.5).abs() < 1e-6);
    }
}
