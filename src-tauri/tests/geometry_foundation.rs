//! G11 几何底座跨模块契约测试。
//!
//! 覆盖任务书验收 A1–A7 中依赖多类型协作的部分；
//! 单模块测试见各文件 `mod tests`。

use lumi_retouch_lib::geometry::{
    compose_protection, degrade_scale, merge_ops, normalized_to_pixel, pixel_to_normalized,
    region_to_pixels, apply_to_delta, ControlPointOp, GeometryFoundationCapability,
    GeometryQualityReport, LocalWarpOp, ProtectionLayer, ProtectionMask, RegionBox,
    WarpStrength, DEFAULT_MAX_DISPLACEMENT_RATIO,
};

#[test]
fn capability_reports_foundation_only() {
    let cap = GeometryFoundationCapability::current();
    assert!(cap.local_warp_ops && cap.multi_control_points);
    assert!(cap.strength_range && cap.protection_compose);
    assert!(cap.multi_region_merge && cap.coordinate_mapping);
    assert!(!cap.ai_face_body_slim, "G11 不得宣称 AI 瘦脸瘦身已完成");
}

#[test]
fn strength_zero_neutral_and_factor_bounds() {
    assert!(WarpStrength::new(0.0).is_neutral());
    assert!(!WarpStrength::new(1.0).is_neutral());
    assert_eq!(WarpStrength::new(-10.0).factor(), 0.0);
    assert_eq!(WarpStrength::new(200.0).factor(), 1.0);
}

#[test]
fn coordinate_roundtrip_and_region_pixels() {
    let (w, h) = (3000u32, 2000u32);
    let px = normalized_to_pixel(0.25, 0.75, w, h);
    assert!((px[0] - 750.0).abs() < 1e-3);
    assert!((px[1] - 1500.0).abs() < 1e-3);
    let back = pixel_to_normalized(px[0], px[1], w, h);
    assert!((back[0] - 0.25).abs() < 1e-4);
    assert!((back[1] - 0.75).abs() < 1e-4);

    let (x0, y0, x1, y1) = region_to_pixels(RegionBox::new(0.1, 0.1, 0.2, 0.2), w, h);
    assert_eq!(x0, 300);
    assert_eq!(y0, 200);
    assert_eq!(x1, 900);
    assert_eq!(y1, 600);
}

#[test]
fn protection_layer_ids_are_stable() {
    assert_eq!(ProtectionLayer::Background.id(), "background");
    assert_eq!(ProtectionLayer::Features.id(), "features");
    assert_eq!(ProtectionLayer::BodyEdge.id(), "bodyEdge");
    assert_eq!(ProtectionLayer::UserProtect.id(), "userProtect");
}

#[test]
fn compose_and_apply_block_strictest() {
    let mut face = ProtectionMask::full_allow(4, 4);
    face.alpha[0] = 0.0;
    let mut user = ProtectionMask::full_allow(4, 4);
    user.alpha[0] = 0.5;
    user.alpha[1] = 0.0;
    let composed = compose_protection(4, 4, &[face, user]);
    assert_eq!(composed.at(0, 0), 0.0, "min(0,0.5)=0");
    assert_eq!(composed.at(1, 0), 0.0, "min(1,0)=0");
    assert_eq!(composed.at(2, 0), 1.0);

    assert_eq!(apply_to_delta([1.0, 1.0], composed.at(0, 0)), [0.0, 0.0]);
    assert_eq!(apply_to_delta([1.0, -1.0], composed.at(2, 0)), [1.0, -1.0]);
}

#[test]
fn merge_scales_and_clamps_without_global_scale_api() {
    // 类型上不存在 global_scale_x：这里只验证合并语义。
    let make = |dx: f32, strength: f32| {
        LocalWarpOp::new(
            RegionBox::new(0.4, 0.4, 0.2, 0.2),
            vec![ControlPointOp::new([0.5, 0.5], [dx, 0.0], 0.1, 100.0)],
        )
        .with_strength(strength)
    };
    assert!(merge_ops(&[make(0.02, 0.0)]).is_none());

    let full = merge_ops(&[make(0.02, 100.0)]).unwrap();
    assert!((full.max_displacement - 0.02).abs() < 1e-5);

    let half = merge_ops(&[make(0.02, 50.0)]).unwrap();
    assert!((half.max_displacement - 0.01).abs() < 1e-5);

    let huge = merge_ops(&[make(0.9, 100.0)]).unwrap();
    assert!(huge.applied_clamp);
    assert!(huge.max_displacement <= DEFAULT_MAX_DISPLACEMENT_RATIO + 1e-5);
}

#[test]
fn quality_degrade_tool_matches_face_body_step() {
    assert!((degrade_scale(1.0) - 0.75).abs() < 1e-6);
    let mut report = GeometryQualityReport::identity();
    assert!(!report.is_blocking());
    report.flipped_triangles = 1;
    assert!(report.is_blocking());
}
