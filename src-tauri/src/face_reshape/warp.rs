//! 网格重采样。
//!
//! 采用 Delaunay 三角网格的分片仿射反向映射：
//! 对每个目标三角形做重心坐标光栅化，再从原图双线性采样。
//! 反向映射不会产生空洞；三角翻转在质量模块中已提前拦截。
//! 只写回面部 alpha 区域，并按 alpha 与原图混合，保证背景与边缘连续。

use crate::error::AppResult;
use crate::face_reshape::boundary::AlphaRegion;
use crate::face_reshape::mesh::ControlMesh;
use crate::image_engine::buffer::Image16;
use crate::image_engine::cancel::CancelToken;
use crate::image_engine::params::WarpMode;
use image::Rgba;

#[derive(Debug, Default, Clone, Copy)]
pub struct WarpStats {
    pub pixels: u32,
    pub max_displacement: f32,
}

/// 网格变形：把 `mesh.points.to` 当作目标位置，**从 `source` 快照采样**写回 `image`。
///
/// `source` 必须是本次变形前的原始图像（pristine）。早期实现直接从正在写回的
/// `image` 采样：行序自上而下 + 三角形互相覆盖导致「读到已写入的变形像素」，
/// 产生梳状条纹、鬼影与矩形台阶（S1.2.2 伪影根因）。
pub fn warp_mesh(
    image: &mut Image16,
    source: &Image16,
    mesh: &ControlMesh,
    region: &AlphaRegion,
    cancel: &CancelToken,
) -> AppResult<WarpStats> {
    debug_assert_eq!(
        (image.width(), image.height()),
        (source.width(), source.height()),
        "快照与目标尺寸必须一致"
    );
    let mut stats = WarpStats::default();
    if mesh.triangles.is_empty() || mesh.max_displacement() < 0.01 {
        return Ok(stats);
    }
    let width = image.width() as i32;
    let height = image.height() as i32;

    for triangle in &mesh.triangles {
        let src_pts = [
            mesh.points[triangle[0]].from,
            mesh.points[triangle[1]].from,
            mesh.points[triangle[2]].from,
        ];
        let target = [
            mesh.points[triangle[0]].to,
            mesh.points[triangle[1]].to,
            mesh.points[triangle[2]].to,
        ];
        let area = triangle_area(target[0], target[1], target[2]);
        if area.abs() < 0.5 {
            continue;
        }
        let min_x = target
            .iter()
            .map(|point| point[0])
            .fold(f32::INFINITY, f32::min);
        let max_x = target
            .iter()
            .map(|point| point[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let min_y = target
            .iter()
            .map(|point| point[1])
            .fold(f32::INFINITY, f32::min);
        let max_y = target
            .iter()
            .map(|point| point[1])
            .fold(f32::NEG_INFINITY, f32::max);
        let x0 = (min_x.floor() as i32).max(region.rect.x0).max(0);
        let x1 = (max_x.ceil() as i32)
            .min(region.rect.x0 + region.rect.width as i32)
            .min(width);
        let y0 = (min_y.floor() as i32).max(region.rect.y0).max(0);
        let y1 = (max_y.ceil() as i32)
            .min(region.rect.y0 + region.rect.height as i32)
            .min(height);
        if x1 <= x0 || y1 <= y0 {
            continue;
        }
        for y in y0..y1 {
            if y % 32 == 0 {
                cancel.check()?;
            }
            for x in x0..x1 {
                let alpha = region.alpha_at(x, y);
                if alpha <= 0.001 {
                    continue;
                }
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let w0 = ((target[1][0] - px) * (target[2][1] - py)
                    - (target[2][0] - px) * (target[1][1] - py))
                    / area;
                let w1 = ((target[2][0] - px) * (target[0][1] - py)
                    - (target[0][0] - px) * (target[2][1] - py))
                    / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < -0.001 || w1 < -0.001 || w2 < -0.001 {
                    continue;
                }
                let sample_x = w0 * src_pts[0][0] + w1 * src_pts[1][0] + w2 * src_pts[2][0];
                let sample_y = w0 * src_pts[0][1] + w1 * src_pts[1][1] + w2 * src_pts[2][1];
                if !sample_x.is_finite() || !sample_y.is_finite() {
                    continue;
                }
                let warped = sample_bilinear(source, sample_x - 0.5, sample_y - 0.5);
                let original = source.get_pixel(x as u32, y as u32).0;
                let output = crate::face_reshape::blending::mix_pixel(&original, &warped, alpha);
                image.put_pixel(x as u32, y as u32, Rgba(output));
                stats.pixels += 1;
            }
        }
    }
    stats.max_displacement = mesh.max_displacement();
    Ok(stats)
}

fn triangle_area(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])
}

pub fn sample_bilinear(image: &Image16, x: f32, y: f32) -> [u16; 4] {
    let max_x = image.width().saturating_sub(1) as f32;
    let max_y = image.height().saturating_sub(1) as f32;
    let x = x.clamp(0.0, max_x);
    let y = y.clamp(0.0, max_y);
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(max_x as u32);
    let y1 = (y0 + 1).min(max_y as u32);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let p00 = image.get_pixel(x0, y0).0;
    let p10 = image.get_pixel(x1, y0).0;
    let p01 = image.get_pixel(x0, y1).0;
    let p11 = image.get_pixel(x1, y1).0;
    let mut output = [0u16; 4];
    for channel in 0..4 {
        let top = p00[channel] as f32 * (1.0 - tx) + p10[channel] as f32 * tx;
        let bottom = p01[channel] as f32 * (1.0 - tx) + p11[channel] as f32 * tx;
        output[channel] = (top * (1.0 - ty) + bottom * ty).round().clamp(0.0, 65535.0) as u16;
    }
    output
}

/// 手动笔触模式是否会产生位移（保护/恢复不算几何位移）。
pub fn mode_displaces(mode: WarpMode) -> bool {
    matches!(
        mode,
        WarpMode::Push | WarpMode::Pull | WarpMode::ZoomIn | WarpMode::ZoomOut
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::face_reshape::boundary::{build_region, Rect};
    use crate::face_reshape::mesh::{ControlMesh, ControlPoint, ControlTag, MESH_VERSION};

    fn gradient(width: u32, height: u32) -> Image16 {
        Image16::from_fn(width, height, |x, y| {
            Rgba([(x * 1200) as u16, (y * 1200) as u16, 10_000, 65535])
        })
    }

    fn point(from: [f32; 2], to: [f32; 2]) -> ControlPoint {
        ControlPoint {
            local: [0.0, 0.0],
            local_to: [0.0, 0.0],
            from,
            to,
            tag: ControlTag::Fixed,
            weight: 0.0,
        }
    }

    fn shifted_mesh(dx: f32) -> ControlMesh {
        // 中心点右移，四周固定：形成平滑的局部拉伸。
        let points = vec![
            point([10.0, 10.0], [10.0, 10.0]),
            point([50.0, 10.0], [50.0, 10.0]),
            point([50.0, 50.0], [50.0, 50.0]),
            point([10.0, 50.0], [10.0, 50.0]),
            point([30.0, 30.0], [30.0 + dx, 30.0]),
        ];
        let mut mesh = ControlMesh {
            points,
            triangles: Vec::new(),
            eye_distance: 20.0,
            bbox: [10.0, 10.0, 50.0, 50.0],
            version: MESH_VERSION,
            signature: String::new(),
        };
        mesh.rebuild_triangles();
        mesh
    }

    fn full_region(width: u32, height: u32) -> AlphaRegion {
        // 复用边界模块的构造方式：直接给一个全 1 的区域用于测试采样。
        let rect = Rect {
            x0: 0,
            y0: 0,
            width,
            height,
        };
        build_full(rect)
    }

    fn build_full(rect: Rect) -> AlphaRegion {
        // 通过一个覆盖整图的人脸几何生成太绕，这里用测试专用构造。
        AlphaRegion::from_alpha(rect, vec![1.0; rect.width as usize * rect.height as usize])
    }

    #[test]
    fn warp_moves_interior_and_keeps_outside() {
        let mut image = gradient(64, 64);
        let before = image.clone();
        let mesh = shifted_mesh(6.0);
        let region = full_region(64, 64);
        warp_mesh(&mut image, &before, &mesh, &region, &CancelToken::default()).unwrap();
        assert_ne!(image.get_pixel(30, 30), before.get_pixel(30, 30));
        assert_eq!(image.get_pixel(2, 2), before.get_pixel(2, 2));
        assert_eq!(image.get_pixel(60, 60), before.get_pixel(60, 60));
    }

    /// S1.2.2 回归：位移方向与行序一致时，旧实现会读到「已写入的变形像素」
    /// （自污染）。修复后目标像素必须等于 pristine 快照的逆映射采样值。
    #[test]
    fn warp_samples_pristine_snapshot_without_feedback() {
        let mut image = gradient(64, 64);
        let before = image.clone();
        // 四角整体下移 8px（纯 +Y 平移，逆映射 = 目标 - (0,8)）。
        let pt = |fx: f32, fy: f32| ControlPoint {
            local: [0.0, 0.0],
            local_to: [0.0, 0.0],
            from: [fx, fy],
            to: [fx, fy + 8.0],
            tag: ControlTag::Fixed,
            weight: 0.0,
        };
        let points = vec![
            pt(10.0, 10.0),
            pt(54.0, 10.0),
            pt(54.0, 54.0),
            pt(10.0, 54.0),
        ];
        let mut mesh = ControlMesh {
            points,
            triangles: Vec::new(),
            eye_distance: 44.0,
            bbox: [10.0, 10.0, 54.0, 62.0],
            version: MESH_VERSION,
            signature: String::new(),
        };
        mesh.rebuild_triangles();
        let region = full_region(64, 64);
        warp_mesh(&mut image, &before, &mesh, &region, &CancelToken::default()).unwrap();
        // 目标 (32,32) ← pristine (32,24)；旧自污染实现会得到 (32,16)。
        assert_eq!(
            image.get_pixel(32, 32).0,
            before.get_pixel(32, 24).0,
            "必须采样 pristine 快照，不得读到已写入的变形像素"
        );
        assert_ne!(
            image.get_pixel(32, 32).0,
            before.get_pixel(32, 16).0,
            "若等于 (32,16) 说明仍在自污染"
        );
    }

    #[test]
    fn zero_displacement_is_identity() {
        let mut image = gradient(32, 32);
        let before = image.clone();
        let mesh = shifted_mesh(0.0);
        let region = full_region(32, 32);
        warp_mesh(&mut image, &before, &mesh, &region, &CancelToken::default()).unwrap();
        assert_eq!(image.into_raw(), before.into_raw());
    }

    #[test]
    fn face_region_builds_soft_alpha() {
        use crate::ai_engine::face_frame::{FaceFrame, NormPoint};
        let face = FaceFrame {
            id: 1,
            uuid: String::new(),
            score: 0.9,
            x: 0.3,
            y: 0.25,
            width: 0.4,
            height: 0.4,
            landmarks: vec![
                NormPoint { x: 0.44, y: 0.40 },
                NormPoint { x: 0.56, y: 0.40 },
                NormPoint { x: 0.50, y: 0.50 },
                NormPoint { x: 0.44, y: 0.58 },
                NormPoint { x: 0.56, y: 0.58 },
            ],
            dense: None,
            level3: None,
        };
        let geometry =
            crate::ai_engine::face_frame::FaceGeometry::from_face(&face, 200, 200).unwrap();
        let region = build_region(&geometry, 200, 200).unwrap();
        assert!(region.alpha_at(100, 80) > 0.5);
        assert_eq!(region.alpha_at(0, 0), 0.0);
    }
}
