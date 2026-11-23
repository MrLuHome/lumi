//! G11 几何变形底座：统一局部控制点、保护蒙版、坐标映射与多区域合并。
//!
//! 设计与选型见 `docs/G11_GEOMETRY_FOUNDATION.md`。
//! 本模块**只提供描述与合成**，不直接改像素；像素内核仍是：
//! - `face_reshape::warp`（Delaunay 网格）
//! - `ai_engine::body`（GridField 位移场）
//!
//! 红线：
//! - 没有整图横向压缩入口；
//! - `strength = 0` 必须恒等；
//! - 保护层合成取 min（最严保护优先）。

pub mod coords;
pub mod merge;
pub mod protection;
pub mod quality;
pub mod types;

pub use coords::{normalized_to_pixel, pixel_to_normalized, region_to_pixels};
pub use merge::{merge_ops, merge_ops_with_limit, MergedWarp, DEFAULT_MAX_DISPLACEMENT_RATIO};
pub use protection::{
    apply_to_delta, compose_protection, ProtectionLayer, ProtectionMask, PROTECTION_FULL_ALLOW,
    PROTECTION_FULL_BLOCK,
};
pub use quality::{degrade_scale, GeometryQualityReport};
pub use types::{ControlPointOp, LocalWarpOp, RegionBox, WarpStrength};

/// 底座能力快照（供测试与后续 capability 命令复用；不等于已支持 AI 瘦脸瘦身）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeometryFoundationCapability {
    pub local_warp_ops: bool,
    pub multi_control_points: bool,
    pub strength_range: bool,
    pub protection_compose: bool,
    pub multi_region_merge: bool,
    pub coordinate_mapping: bool,
    /// AI 自动瘦脸/瘦身算法（G13）。G11 恒为 false。
    pub ai_face_body_slim: bool,
}

impl GeometryFoundationCapability {
    pub fn current() -> Self {
        Self {
            local_warp_ops: true,
            multi_control_points: true,
            strength_range: true,
            protection_compose: true,
            multi_region_merge: true,
            coordinate_mapping: true,
            ai_face_body_slim: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foundation_capability_does_not_claim_ai_slim() {
        let cap = GeometryFoundationCapability::current();
        assert!(cap.local_warp_ops);
        assert!(cap.protection_compose);
        assert!(!cap.ai_face_body_slim, "G11 不得宣称已完成 AI 瘦脸瘦身");
    }
}
