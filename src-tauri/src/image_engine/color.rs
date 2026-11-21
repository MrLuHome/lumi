use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use crate::error::{AppError, AppResult};
use crate::image_engine::params::{CurvePoint, CurveSet};

pub fn luma(r: f32, g: f32, b: f32) -> f32 {
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

pub fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    if edge1 <= edge0 {
        return if value < edge0 { 0.0 } else { 1.0 };
    }
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// sRGB 归一化 RGB → HSL（h 0..360，s/l 0..1）。
pub fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let lightness = (max + min) * 0.5;
    let delta = max - min;
    if delta <= f32::EPSILON {
        return (0.0, 0.0, lightness);
    }
    let saturation = if lightness > 0.5 {
        delta / (2.0 - max - min)
    } else {
        delta / (max + min)
    };
    let hue = if max == r {
        60.0 * (((g - b) / delta) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    ((hue + 360.0) % 360.0, saturation, lightness)
}

pub fn hsl_to_rgb(hue: f32, saturation: f32, lightness: f32) -> (f32, f32, f32) {
    if saturation <= f32::EPSILON {
        return (lightness, lightness, lightness);
    }
    let c = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let h = hue.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = lightness - c * 0.5;
    (r1 + m, g1 + m, b1 + m)
}

/// 4096 级曲线查找表，单调三次插值，避免高阶多项式在拖点后出现振铃。
pub struct CurveLut {
    values: Vec<f32>,
}

impl CurveLut {
    const SIZE: usize = 4096;

    pub fn build(points: &[CurvePoint]) -> Self {
        let mut sorted: Vec<CurvePoint> = points
            .iter()
            .copied()
            .filter(|point| point.x.is_finite() && point.y.is_finite())
            .map(|point| CurvePoint::new(point.x, point.y))
            .collect();
        sorted.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
        sorted.dedup_by(|a, b| (a.x - b.x).abs() < 1e-5);

        if sorted.len() < 2 {
            return Self::identity();
        }

        let slopes = monotone_slopes(&sorted);
        let mut values = Vec::with_capacity(Self::SIZE);
        for index in 0..Self::SIZE {
            let x = index as f32 / (Self::SIZE - 1) as f32;
            values.push(evaluate(&sorted, &slopes, x));
        }
        Self { values }
    }

    pub fn identity() -> Self {
        let values = (0..Self::SIZE)
            .map(|index| index as f32 / (Self::SIZE - 1) as f32)
            .collect();
        Self { values }
    }

    pub fn apply(&self, value: f32) -> f32 {
        let position = value.clamp(0.0, 1.0) * (Self::SIZE - 1) as f32;
        let low = position.floor() as usize;
        let high = (low + 1).min(Self::SIZE - 1);
        let fraction = position - low as f32;
        self.values[low] * (1.0 - fraction) + self.values[high] * fraction
    }

    pub fn values(&self) -> &[f32] {
        &self.values
    }
}

pub struct CurveSetLuts {
    pub master: CurveLut,
    pub red: CurveLut,
    pub green: CurveLut,
    pub blue: CurveLut,
}

impl CurveSetLuts {
    pub fn build(curves: &CurveSet) -> Self {
        Self {
            master: CurveLut::build(&curves.rgb),
            red: CurveLut::build(&curves.red),
            green: CurveLut::build(&curves.green),
            blue: CurveLut::build(&curves.blue),
        }
    }
}

fn monotone_slopes(points: &[CurvePoint]) -> Vec<f32> {
    let n = points.len();
    let mut slopes = vec![0f32; n];
    for index in 0..n - 1 {
        let dx = (points[index + 1].x - points[index].x).max(1e-6);
        slopes[index] = (points[index + 1].y - points[index].y) / dx;
    }
    let mut tangents = vec![0f32; n];
    tangents[0] = slopes[0];
    tangents[n - 1] = slopes[n - 2];
    for index in 1..n - 1 {
        if slopes[index - 1] * slopes[index] <= 0.0 {
            tangents[index] = 0.0;
        } else {
            tangents[index] = (slopes[index - 1] + slopes[index]) * 0.5;
        }
    }
    tangents
}

fn evaluate(points: &[CurvePoint], tangents: &[f32], x: f32) -> f32 {
    if x <= points[0].x {
        return points[0].y;
    }
    let last = points.len() - 1;
    if x >= points[last].x {
        return points[last].y;
    }
    let mut index = 0;
    while index + 1 < points.len() && points[index + 1].x < x {
        index += 1;
    }
    let p0 = points[index];
    let p1 = points[index + 1];
    let dx = (p1.x - p0.x).max(1e-6);
    let t = (x - p0.x) / dx;
    let t2 = t * t;
    let t3 = t2 * t;
    let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
    let h10 = t3 - 2.0 * t2 + t;
    let h01 = -2.0 * t3 + 3.0 * t2;
    let h11 = t3 - t2;
    let value =
        h00 * p0.y + h10 * dx * tangents[index] + h01 * p1.y + h11 * dx * tangents[index + 1];
    value.clamp(0.0, 1.0)
}

/// 解析 .cube 3D LUT。仅支持 3D（1D LUT 会在加载时明确报错）。
#[derive(Debug)]
pub struct Lut3D {
    pub size: usize,
    pub data: Vec<[f32; 3]>,
}

impl Lut3D {
    pub fn load(path: &Path) -> AppResult<Arc<Lut3D>> {
        static CACHE: OnceLock<Mutex<HashMap<String, Arc<Lut3D>>>> = OnceLock::new();
        let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        let key = path.to_string_lossy().into_owned();
        if let Some(hit) = cache.lock().ok().and_then(|guard| guard.get(&key).cloned()) {
            return Ok(hit);
        }

        let text = std::fs::read_to_string(path)?;
        let mut size = 0usize;
        let mut data = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let upper = line.to_ascii_uppercase();
            if upper.starts_with("LUT_3D_SIZE") {
                size = line
                    .split_whitespace()
                    .nth(1)
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or(0);
                continue;
            }
            if upper.starts_with("LUT_1D_SIZE") {
                return Err(AppError::Unsupported(
                    "暂不支持 1D LUT，请使用 .cube 3D LUT".into(),
                ));
            }
            if upper.starts_with("TITLE") || upper.starts_with("DOMAIN_") {
                continue;
            }
            let mut parts = line.split_whitespace();
            let parsed: Vec<f32> = parts
                .by_ref()
                .take(3)
                .filter_map(|value| value.parse::<f32>().ok())
                .collect();
            if parsed.len() == 3 {
                data.push([parsed[0], parsed[1], parsed[2]]);
            }
        }

        if size == 0 {
            return Err(AppError::Unsupported("LUT 文件缺少 LUT_3D_SIZE".into()));
        }
        if data.len() != size * size * size {
            return Err(AppError::Unsupported(format!(
                "LUT 数据量不匹配：期望 {}，实际 {}",
                size * size * size,
                data.len()
            )));
        }

        let lut = Arc::new(Lut3D { size, data });
        if let Ok(mut guard) = cache.lock() {
            guard.insert(key, lut.clone());
        }
        Ok(lut)
    }

    /// 三线性插值采样。.cube 的行顺序为 r 变化最快。
    pub fn sample(&self, r: f32, g: f32, b: f32) -> [f32; 3] {
        let n = self.size;
        let scale = (n - 1) as f32;
        let rf = r.clamp(0.0, 1.0) * scale;
        let gf = g.clamp(0.0, 1.0) * scale;
        let bf = b.clamp(0.0, 1.0) * scale;
        let r0 = rf.floor() as usize;
        let g0 = gf.floor() as usize;
        let b0 = bf.floor() as usize;
        let r1 = (r0 + 1).min(n - 1);
        let g1 = (g0 + 1).min(n - 1);
        let b1 = (b0 + 1).min(n - 1);
        let fr = rf - r0 as f32;
        let fg = gf - g0 as f32;
        let fb = bf - b0 as f32;

        let at = |r: usize, g: usize, b: usize| -> [f32; 3] { self.data[r + g * n + b * n * n] };
        let lerp = |a: [f32; 3], b: [f32; 3], t: f32| -> [f32; 3] {
            [
                a[0] + (b[0] - a[0]) * t,
                a[1] + (b[1] - a[1]) * t,
                a[2] + (b[2] - a[2]) * t,
            ]
        };

        let c00 = lerp(at(r0, g0, b0), at(r1, g0, b0), fr);
        let c01 = lerp(at(r0, g0, b1), at(r1, g0, b1), fr);
        let c10 = lerp(at(r0, g1, b0), at(r1, g1, b0), fr);
        let c11 = lerp(at(r0, g1, b1), at(r1, g1, b1), fr);
        let c0 = lerp(c00, c01, fb);
        let c1 = lerp(c10, c11, fb);
        lerp(c0, c1, fg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_curve_is_linear() {
        let lut = CurveLut::identity();
        assert!((lut.apply(0.25) - 0.25).abs() < 1e-3);
        assert!((lut.apply(0.75) - 0.75).abs() < 1e-3);
    }

    #[test]
    fn curve_points_shape_the_tone() {
        let lut = CurveLut::build(&[
            CurvePoint::new(0.0, 0.0),
            CurvePoint::new(0.5, 0.7),
            CurvePoint::new(1.0, 1.0),
        ]);
        assert!(lut.apply(0.5) > 0.65);
        assert!(lut.apply(0.0) < 0.02);
        assert!(lut.apply(1.0) > 0.98);
    }

    #[test]
    fn hsl_conversion_roundtrips() {
        let (h, s, l) = rgb_to_hsl(0.8, 0.3, 0.2);
        let (r, g, b) = hsl_to_rgb(h, s, l);
        assert!((r - 0.8).abs() < 1e-3);
        assert!((g - 0.3).abs() < 1e-3);
        assert!((b - 0.2).abs() < 1e-3);
    }

    #[test]
    fn lut_identity_samples_preserve_color() {
        let size = 2;
        let mut data = Vec::new();
        // .cube 行顺序：r 变化最快，其次 g，最后 b。
        for b in 0..size {
            for g in 0..size {
                for r in 0..size {
                    data.push([r as f32, g as f32, b as f32]);
                }
            }
        }
        let lut = Lut3D { size, data };
        let sample = lut.sample(0.25, 0.5, 0.75);
        assert!((sample[0] - 0.25).abs() < 1e-3);
        assert!((sample[1] - 0.5).abs() < 1e-3);
        assert!((sample[2] - 0.75).abs() < 1e-3);
    }
}
