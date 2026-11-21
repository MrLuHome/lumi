//! Bowyer–Watson Delaunay 三角剖分。
//!
//! 只服务面部控制网格：输入是几十到一百多个控制点，输出三角形索引。
//! 实现使用 f64 计算外接圆，退化三角形直接丢弃；重复点先去重，
//! 返回值仍使用原始点索引，方便调用方直接做网格质量检查。

#[derive(Clone, Copy)]
struct Triangle {
    a: usize,
    b: usize,
    c: usize,
    cx: f64,
    cy: f64,
    r2: f64,
}

fn circumcircle(points: &[[f64; 2]], a: usize, b: usize, c: usize) -> Option<Triangle> {
    let (ax, ay) = (points[a][0], points[a][1]);
    let (bx, by) = (points[b][0], points[b][1]);
    let (cx, cy) = (points[c][0], points[c][1]);
    let d = 2.0 * (ax * (by - cy) + bx * (cy - ay) + cx * (ay - by));
    if d.abs() < 1e-9 {
        return None;
    }
    let a2 = ax * ax + ay * ay;
    let b2 = bx * bx + by * by;
    let c2 = cx * cx + cy * cy;
    let ux = (a2 * (by - cy) + b2 * (cy - ay) + c2 * (ay - by)) / d;
    let uy = (a2 * (cx - bx) + b2 * (ax - cx) + c2 * (bx - ax)) / d;
    let r2 = (ax - ux).powi(2) + (ay - uy).powi(2);
    Some(Triangle {
        a,
        b,
        c,
        cx: ux,
        cy: uy,
        r2,
    })
}

fn contains(triangle: &Triangle, x: f64, y: f64) -> bool {
    (x - triangle.cx).powi(2) + (y - triangle.cy).powi(2) <= triangle.r2 + 1e-7
}

/// 对二维点集做 Delaunay 三角剖分。
///
/// - 输入少于 3 个不重复点或所有点共线时返回空；
/// - 输出三角形使用原始索引，按确定性顺序生成；
/// - 不保证三角形绕向，调用方自行统一。
pub fn delaunay(points: &[[f32; 2]]) -> Vec<[usize; 3]> {
    if points.len() < 3 {
        return Vec::new();
    }

    // 去重：相近点视为同一点，避免零面积三角形。
    let mut unique: Vec<[f64; 2]> = Vec::with_capacity(points.len());
    let mut index_of = Vec::with_capacity(points.len());
    for point in points {
        let x = point[0] as f64;
        let y = point[1] as f64;
        if !x.is_finite() || !y.is_finite() {
            index_of.push(0usize);
            continue;
        }
        let existing = unique
            .iter()
            .position(|entry| (entry[0] - x).abs() < 1e-4 && (entry[1] - y).abs() < 1e-4);
        match existing {
            Some(index) => index_of.push(index),
            None => {
                index_of.push(unique.len());
                unique.push([x, y]);
            }
        }
    }
    if unique.len() < 3 {
        return Vec::new();
    }

    let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
    let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for point in &unique {
        min_x = min_x.min(point[0]);
        min_y = min_y.min(point[1]);
        max_x = max_x.max(point[0]);
        max_y = max_y.max(point[1]);
    }
    let span = (max_x - min_x).max(max_y - min_y).max(1.0);
    let mid_x = (min_x + max_x) * 0.5;
    let mid_y = (min_y + max_y) * 0.5;
    let margin = span * 8.0;

    let mut working = unique.clone();
    let super_a = working.len();
    working.push([mid_x - margin * 2.0, mid_y - margin]);
    let super_b = working.len();
    working.push([mid_x, mid_y + margin * 2.0]);
    let super_c = working.len();
    working.push([mid_x + margin * 2.0, mid_y - margin]);

    let mut triangles: Vec<Triangle> = Vec::new();
    if let Some(triangle) = circumcircle(&working, super_a, super_b, super_c) {
        triangles.push(triangle);
    }

    for index in 0..unique.len() {
        let x = working[index][0];
        let y = working[index][1];
        let mut edges: Vec<(usize, usize)> = Vec::new();
        let mut kept: Vec<Triangle> = Vec::with_capacity(triangles.len());
        for triangle in triangles.drain(..) {
            if contains(&triangle, x, y) {
                edges.push((triangle.a, triangle.b));
                edges.push((triangle.b, triangle.c));
                edges.push((triangle.c, triangle.a));
            } else {
                kept.push(triangle);
            }
        }
        // 只保留没有被相邻坏三角形共享的边，形成空腔边界。
        let mut boundary: Vec<(usize, usize)> = Vec::new();
        for (index, edge) in edges.iter().enumerate() {
            let reversed = (edge.1, edge.0);
            let shared = edges
                .iter()
                .enumerate()
                .any(|(other, candidate)| other != index && *candidate == reversed);
            if !shared && !boundary.contains(edge) {
                boundary.push(*edge);
            }
        }
        triangles = kept;
        for (a, b) in boundary {
            if a == b {
                continue;
            }
            if let Some(triangle) = circumcircle(&working, a, b, index) {
                triangles.push(triangle);
            }
        }
    }

    let mut output = Vec::with_capacity(triangles.len());
    for triangle in triangles {
        if triangle.a >= unique.len() || triangle.b >= unique.len() || triangle.c >= unique.len() {
            continue;
        }
        let (a, b, c) = (triangle.a, triangle.b, triangle.c);
        if a == b || b == c || a == c {
            continue;
        }
        if triangle_area(points[a], points[b], points[c]).abs() < 1e-3 {
            continue;
        }
        output.push([a, b, c]);
    }
    output
}

/// 二维叉积，图像坐标 y 向下；符号用于检查三角翻转。
pub fn triangle_area(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn squares_are_triangulated() {
        let points = vec![
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 10.0],
            [0.0, 10.0],
            [5.0, 5.0],
        ];
        let triangles = delaunay(&points);
        assert!(triangles.len() >= 4);
        for triangle in &triangles {
            assert!(
                triangle_area(
                    points[triangle[0]],
                    points[triangle[1]],
                    points[triangle[2]]
                )
                .abs()
                    > 0.01
            );
        }
    }

    #[test]
    fn duplicate_and_collinear_points_are_safe() {
        let points = vec![
            [0.0, 0.0],
            [10.0, 0.0],
            [20.0, 0.0],
            [10.0, 0.0],
            [5.0, 0.0],
        ];
        assert!(delaunay(&points).is_empty());
        let mixed = vec![[0.0, 0.0], [10.0, 0.0], [0.0, 10.0], [0.0, 0.0]];
        let triangles = delaunay(&mixed);
        assert_eq!(triangles.len(), 1);
    }

    #[test]
    fn every_inner_point_is_connected() {
        let points = vec![
            [0.0, 0.0],
            [20.0, 0.0],
            [20.0, 20.0],
            [0.0, 20.0],
            [10.0, 10.0],
        ];
        let triangles = delaunay(&points);
        let mut touched = [false; 5];
        for triangle in &triangles {
            for index in triangle {
                touched[*index] = true;
            }
        }
        assert!(touched.iter().all(|value| *value));
    }
}
