//! 控制网格结构与网格缓存。
//!
//! 控制点用脸部局部坐标（u 沿双眼方向，v 垂直向下，单位为眼距）保存，
//! 最终统一转换到原图像素坐标。`from` 是原图位置，`to` 是变形后位置，
//! 背景外圈与五官锚点保持 `from == to`，从结构上保证脸外像素不变。

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

use serde::Serialize;

use crate::face_reshape::triangulation::{delaunay, triangle_area};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlTag {
    /// 背景外圈或五官锚点，永远不移动。
    Fixed,
    /// 脸型轮廓环，`ring` 为 0..=3（内到外）。
    Face {
        ring: usize,
    },
    Eye {
        side: u8,
        ring: usize,
    },
    Nose {
        ring: usize,
    },
    Mouth {
        ring: usize,
    },
}

#[derive(Debug, Clone)]
pub struct ControlPoint {
    pub local: [f32; 2],
    pub local_to: [f32; 2],
    pub from: [f32; 2],
    pub to: [f32; 2],
    pub tag: ControlTag,
    /// 该控制点在所属环上的位移权重（0..1）。
    pub weight: f32,
}

impl ControlPoint {
    pub fn displacement(&self) -> f32 {
        ((self.to[0] - self.from[0]).powi(2) + (self.to[1] - self.from[1]).powi(2)).sqrt()
    }
}

#[derive(Debug, Clone)]
pub struct ControlMesh {
    pub points: Vec<ControlPoint>,
    pub triangles: Vec<[usize; 3]>,
    pub eye_distance: f32,
    pub bbox: [f32; 4],
    /// 控制网格版本，写入操作报告，便于非破坏历史追溯。
    pub version: u32,
    pub signature: String,
}

pub const MESH_VERSION: u32 = 1;

impl ControlMesh {
    pub fn max_displacement(&self) -> f32 {
        self.points
            .iter()
            .map(|point| point.displacement())
            .fold(0.0, f32::max)
    }

    pub fn max_displacement_ratio(&self) -> f32 {
        if self.eye_distance <= 1.0 {
            0.0
        } else {
            self.max_displacement() / self.eye_distance
        }
    }

    pub fn is_empty(&self) -> bool {
        self.triangles.is_empty()
    }

    /// 把像素坐标 bbox 向外扩展，供 ROI 与质量检查使用。
    pub fn expanded_bbox(&self, margin: f32) -> [f32; 4] {
        [
            self.bbox[0] - margin,
            self.bbox[1] - margin,
            self.bbox[2] + margin,
            self.bbox[3] + margin,
        ]
    }

    pub fn rebuild_triangles(&mut self) {
        let from: Vec<[f32; 2]> = self.points.iter().map(|point| point.from).collect();
        self.triangles = delaunay(&from);
    }

    pub fn signature_of(from: &[[f32; 2]], to: &[[f32; 2]]) -> String {
        let mut signature = String::with_capacity(from.len() * 24);
        for (index, point) in from.iter().enumerate() {
            signature.push_str(&format!(
                "{:.1},{:.1}>{:.1},{:.1};",
                point[0], point[1], to[index][0], to[index][1]
            ));
        }
        signature
    }

    /// 平均有向面积比，用于判断纹理是否被过度拉伸或压缩。
    pub fn area_ratio(&self) -> (f32, f32) {
        let mut min_ratio = f32::INFINITY;
        let mut max_ratio = 0.0f32;
        for triangle in &self.triangles {
            let source = triangle_area(
                self.points[triangle[0]].from,
                self.points[triangle[1]].from,
                self.points[triangle[2]].from,
            )
            .abs();
            if source < 1e-3 {
                continue;
            }
            let target = triangle_area(
                self.points[triangle[0]].to,
                self.points[triangle[1]].to,
                self.points[triangle[2]].to,
            )
            .abs();
            let ratio = target / source;
            if ratio.is_finite() {
                min_ratio = min_ratio.min(ratio);
                max_ratio = max_ratio.max(ratio);
            }
        }
        if !min_ratio.is_finite() {
            min_ratio = 1.0;
        }
        (min_ratio, max_ratio.max(1.0))
    }

    /// 有向面积的符号翻转数量；大于 0 表示网格折叠，必须降强度。
    pub fn flipped_triangles(&self) -> usize {
        let mut flipped = 0;
        for triangle in &self.triangles {
            let source = triangle_area(
                self.points[triangle[0]].from,
                self.points[triangle[1]].from,
                self.points[triangle[2]].from,
            );
            let target = triangle_area(
                self.points[triangle[0]].to,
                self.points[triangle[1]].to,
                self.points[triangle[2]].to,
            );
            if source.abs() > 1e-3 && target.abs() > 1e-3 && source.signum() != target.signum() {
                flipped += 1;
            }
        }
        flipped
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshPreview {
    pub points: Vec<[f32; 2]>,
    pub triangles: Vec<[u32; 3]>,
    pub dense_landmarks: bool,
    pub source: String,
    pub version: u32,
}

/// 小型 LRU：网格只依赖人脸局部坐标与参数，命中后省掉重复三角剖分。
struct MeshCache {
    entries: VecDeque<(String, std::sync::Arc<ControlMesh>)>,
    capacity: usize,
}

fn cache() -> &'static Mutex<MeshCache> {
    static CACHE: OnceLock<Mutex<MeshCache>> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(MeshCache {
            entries: VecDeque::new(),
            capacity: 8,
        })
    })
}

pub fn cached(key: String, build: impl FnOnce() -> ControlMesh) -> std::sync::Arc<ControlMesh> {
    if let Ok(mut guard) = cache().lock() {
        if let Some(index) = guard.entries.iter().position(|(stored, _)| *stored == key) {
            let entry = guard.entries.remove(index).expect("索引来自当前缓存");
            let mesh = entry.1.clone();
            guard.entries.push_back(entry);
            return mesh;
        }
    }
    let mesh = std::sync::Arc::new(build());
    if let Ok(mut guard) = cache().lock() {
        guard.entries.push_back((key, mesh.clone()));
        while guard.entries.len() > guard.capacity {
            guard.entries.pop_front();
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn displacement_and_ratio_are_measured() {
        let mesh = ControlMesh {
            points: vec![
                point([0.0, 0.0], [0.0, 0.0]),
                point([10.0, 0.0], [10.0, 2.0]),
                point([0.0, 10.0], [0.0, 10.0]),
            ],
            triangles: vec![[0, 1, 2]],
            eye_distance: 20.0,
            bbox: [0.0, 0.0, 10.0, 10.0],
            version: MESH_VERSION,
            signature: String::new(),
        };
        assert!(mesh.max_displacement() > 1.9);
        assert!(mesh.max_displacement_ratio() > 0.09);
        assert_eq!(mesh.flipped_triangles(), 0);
        let (min, max) = mesh.area_ratio();
        assert!(min <= 1.0 && max >= 1.0);
    }

    #[test]
    fn flipped_triangle_is_detected() {
        let mesh = ControlMesh {
            points: vec![
                point([0.0, 0.0], [0.0, 0.0]),
                point([10.0, 0.0], [0.0, 10.0]),
                point([0.0, 10.0], [10.0, 0.0]),
            ],
            triangles: vec![[0, 1, 2]],
            eye_distance: 20.0,
            bbox: [0.0, 0.0, 10.0, 10.0],
            version: MESH_VERSION,
            signature: String::new(),
        };
        assert!(mesh.flipped_triangles() > 0);
    }
}
