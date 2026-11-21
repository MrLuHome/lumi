//! 稠密结果装配与几何派生。
//!
//! 派生点只做**分类**（例如把实际轮廓点中 x 最小/最大者标为眼角），
//! 不做插值。没有真实输出的点（例如鼻孔中心）不会被编造。

use crate::face_intelligence::types::{
    BoundingBox, DenseFace, DenseRegion, FaceAnalysisResult, FaceResult, Landmark, RegionKind,
};

/// 把 Provider 结果中的一张脸组装成可挂到 `FaceFrame` 的稠密记录。
pub fn assemble_dense(face: &FaceResult, result: &FaceAnalysisResult) -> DenseFace {
    DenseFace {
        topology_id: result.topology_id.clone(),
        provider_id: result.provider_id.clone(),
        provider_version: result.provider_version.clone(),
        image_width: result.image_width,
        image_height: result.image_height,
        bounding_box: face.bounding_box,
        confidence: face.confidence,
        regions: face.regions.clone(),
    }
}

/// 结构校验：点必须有限、在图像范围内、框非空。
pub fn validate_dense(dense: &DenseFace) -> Vec<String> {
    let mut issues = Vec::new();
    if dense.regions.is_empty() {
        issues.push("没有人脸语义区域".into());
    }
    if dense.bounding_box.width <= 0.0 || dense.bounding_box.height <= 0.0 {
        issues.push("人脸框为空".into());
    }
    if !dense.bounding_box.x.is_finite() || !dense.bounding_box.y.is_finite() {
        issues.push("人脸框坐标非法".into());
    }
    for region in &dense.regions {
        if region.points.is_empty() {
            issues.push(format!("{} 为空", region.kind.label()));
        }
        for point in &region.points {
            if !point.is_finite() {
                issues.push(format!("{} 含非法坐标", region.kind.label()));
                break;
            }
            if !(0.0..=1.0).contains(&point.x) || !(0.0..=1.0).contains(&point.y) {
                issues.push(format!("{} 有点越界", region.kind.label()));
                break;
            }
            if !point.semantic_id.starts_with(region.kind.prefix()) {
                issues.push(format!(
                    "{} 的语义 ID `{}` 前缀不匹配",
                    region.kind.label(),
                    point.semantic_id
                ));
                break;
            }
        }
    }
    issues
}

/// 左右眼轮廓。顺序按图像 x 排序，返回 `(image_left, image_right)`；
/// 不假设 Provider 的“left/right”命名与图像左右一致。
pub fn eye_contours(dense: &DenseFace) -> Option<(DenseRegion, DenseRegion)> {
    let mut regions: Vec<DenseRegion> = dense
        .regions
        .iter()
        .filter(|region| matches!(region.kind, RegionKind::LeftEye | RegionKind::RightEye))
        .filter(|region| region.points.len() >= 4)
        .cloned()
        .collect();
    if regions.len() < 2 {
        return None;
    }
    regions.sort_by(|a, b| {
        let ax = a.centroid().map(|point| point.0).unwrap_or(0.0);
        let bx = b.centroid().map(|point| point.0).unwrap_or(0.0);
        ax.partial_cmp(&bx).unwrap_or(std::cmp::Ordering::Equal)
    });
    Some((regions[0].clone(), regions[1].clone()))
}

/// 一条闭合轮廓中 x 最小/最大的两个点，作为眼角候选。
pub fn extreme_x(region: &DenseRegion) -> Option<(&Landmark, &Landmark)> {
    let left = region
        .points
        .iter()
        .min_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))?;
    let right = region
        .points
        .iter()
        .max_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))?;
    Some((left, right))
}

/// 把眼睑轮廓按两眼角连线分为上/下眼睑。属于分类，不是插值。
pub fn split_eyelids(region: &DenseRegion) -> (Vec<Landmark>, Vec<Landmark>) {
    let Some((left, right)) = extreme_x(region) else {
        return (Vec::new(), Vec::new());
    };
    let (x0, y0) = (left.x, left.y);
    let (x1, y1) = (right.x, right.y);
    let dx = x1 - x0;
    let denominator = (dx * dx + (y1 - y0).powi(2)).sqrt().max(1e-6);
    let mut upper = Vec::new();
    let mut lower = Vec::new();
    for point in &region.points {
        // 点到眼角连线的有符号距离；图像 y 向下，负值代表在上方。
        let signed = ((x1 - x0) * (point.y - y0) - (y1 - y0) * (point.x - x0)) / denominator;
        if signed <= 0.0 {
            upper.push(point.clone());
        } else {
            lower.push(point.clone());
        }
    }
    (upper, lower)
}

/// 鼻部轮廓中横向最外侧的两个实际点（鼻翼候选）与最下方的点（鼻尖候选）。
pub fn nose_landmarks(dense: &DenseFace) -> Option<(Landmark, Landmark, Landmark)> {
    let region = dense.region(RegionKind::Nose)?;
    if region.points.len() < 5 {
        return None;
    }
    let mean_y =
        region.points.iter().map(|point| point.y).sum::<f32>() / region.points.len() as f32;
    let lower: Vec<&Landmark> = region
        .points
        .iter()
        .filter(|point| point.y >= mean_y)
        .collect();
    let pool = if lower.len() >= 3 {
        lower
    } else {
        region.points.iter().collect()
    };
    let left = pool
        .iter()
        .copied()
        .min_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))?;
    let right = pool
        .iter()
        .copied()
        .max_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))?;
    let tip = region
        .points
        .iter()
        .max_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal))?;
    Some((left.clone(), right.clone(), tip.clone()))
}

/// 外唇轮廓的左右嘴角与上下唇中点。
pub fn mouth_landmarks(dense: &DenseFace) -> Option<(Landmark, Landmark, Landmark, Landmark)> {
    let region = dense.region(RegionKind::OuterLips)?;
    if region.points.len() < 8 {
        return None;
    }
    let (left, right) = extreme_x(region)?;
    let center_x = (left.x + right.x) * 0.5;
    let upper = region
        .points
        .iter()
        .filter(|point| point.x > left.x && point.x < right.x)
        .min_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal))?;
    let lower = region
        .points
        .iter()
        .filter(|point| point.x > left.x && point.x < right.x)
        .max_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal))?;
    let _ = center_x;
    Some((left.clone(), right.clone(), upper.clone(), lower.clone()))
}

/// 脸缘轮廓中最下方的实际点（下巴候选）。
pub fn chin_point(dense: &DenseFace) -> Option<Landmark> {
    dense
        .region(RegionKind::FaceContour)?
        .points
        .iter()
        .max_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal))
        .cloned()
}

/// 所有实际输出点的包围盒，用于检查 Provider 坐标是否偏离人脸框过多。
pub fn landmark_bounds(dense: &DenseFace) -> Option<BoundingBox> {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut any = false;
    for region in &dense.regions {
        for point in &region.points {
            any = true;
            min_x = min_x.min(point.x);
            min_y = min_y.min(point.y);
            max_x = max_x.max(point.x);
            max_y = max_y.max(point.y);
        }
    }
    if !any {
        return None;
    }
    Some(BoundingBox {
        x: min_x,
        y: min_y,
        width: (max_x - min_x).max(0.0),
        height: (max_y - min_y).max(0.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn landmark(x: f32, y: f32, id: &str) -> Landmark {
        Landmark::new(x, y, 0.9, id)
    }

    fn dense_with(regions: Vec<DenseRegion>) -> DenseFace {
        DenseFace {
            topology_id: "apple-vision-landmarks".into(),
            provider_id: "apple-vision".into(),
            bounding_box: BoundingBox {
                x: 0.2,
                y: 0.2,
                width: 0.6,
                height: 0.6,
            },
            confidence: 0.95,
            regions,
            ..Default::default()
        }
    }

    #[test]
    fn validation_rejects_unknown_prefix_and_out_of_range() {
        let dense = dense_with(vec![DenseRegion {
            kind: RegionKind::LeftEye,
            points: vec![landmark(0.4, 0.4, "nose.contour.0")],
            closed: true,
        }]);
        assert!(!validate_dense(&dense).is_empty());

        let dense = dense_with(vec![DenseRegion {
            kind: RegionKind::LeftEye,
            points: vec![Landmark {
                x: 1.4,
                y: 0.4,
                confidence: 0.9,
                semantic_id: "eye.left.lid.0".into(),
            }],
            closed: true,
        }]);
        assert!(validate_dense(&dense)
            .iter()
            .any(|issue| issue.contains("越界")));
    }

    #[test]
    fn eyelids_split_without_inventing_points() {
        let region = DenseRegion {
            kind: RegionKind::LeftEye,
            points: vec![
                landmark(0.40, 0.40, "eye.left.lid.0"),
                landmark(0.42, 0.38, "eye.left.lid.1"),
                landmark(0.46, 0.38, "eye.left.lid.2"),
                landmark(0.48, 0.40, "eye.left.lid.3"),
                landmark(0.46, 0.42, "eye.left.lid.4"),
                landmark(0.42, 0.42, "eye.left.lid.5"),
            ],
            closed: true,
        };
        let (upper, lower) = split_eyelids(&region);
        assert!(upper.len() >= 2);
        assert!(lower.len() >= 2);
        assert_eq!(upper.len() + lower.len(), 6);
        for point in &upper {
            assert!(point.y <= 0.40);
        }
    }

    #[test]
    fn eye_contours_are_sorted_by_image_x() {
        let left = DenseRegion {
            kind: RegionKind::RightEye,
            points: vec![
                landmark(0.28, 0.40, "eye.right.lid.0"),
                landmark(0.30, 0.38, "eye.right.lid.1"),
                landmark(0.34, 0.38, "eye.right.lid.2"),
                landmark(0.36, 0.40, "eye.right.lid.3"),
            ],
            closed: true,
        };
        let right = DenseRegion {
            kind: RegionKind::LeftEye,
            points: vec![
                landmark(0.58, 0.40, "eye.left.lid.0"),
                landmark(0.60, 0.38, "eye.left.lid.1"),
                landmark(0.64, 0.38, "eye.left.lid.2"),
                landmark(0.66, 0.40, "eye.left.lid.3"),
            ],
            closed: true,
        };
        let (first, second) = eye_contours(&dense_with(vec![right, left])).unwrap();
        assert!(first.centroid().unwrap().0 < second.centroid().unwrap().0);
    }

    #[test]
    fn chin_is_the_lowest_real_contour_point() {
        let dense = dense_with(vec![DenseRegion {
            kind: RegionKind::FaceContour,
            points: vec![
                landmark(0.3, 0.3, "face.contour.0"),
                landmark(0.5, 0.8, "face.contour.1"),
                landmark(0.7, 0.3, "face.contour.2"),
            ],
            closed: true,
        }]);
        assert!((chin_point(&dense).unwrap().y - 0.8).abs() < 1e-6);
    }
}
