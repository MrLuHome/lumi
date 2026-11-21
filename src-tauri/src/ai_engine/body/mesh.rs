//! 粗网格位移场与反向采样。
//!
//! 位移定义在规则网格顶点上，双线性插值到像素；这样全分辨率导出时
//! 内存占用与图像尺寸无关，也不会因为逐像素位移场占用数百 MB。
//! `dx/dy` 表示「内容移动量」，反向采样时源坐标为 `(x - dx, y - dy)`。

use rayon::prelude::*;

use crate::image_engine::buffer::Image16;

use super::constraints::FaceGuard;

#[derive(Debug, Clone)]
pub struct GridField {
    pub width: u32,
    pub height: u32,
    pub cols: u32,
    pub rows: u32,
    pub dx: Vec<f32>,
    pub dy: Vec<f32>,
}

impl GridField {
    pub fn new(width: u32, height: u32) -> Self {
        let short = width.min(height).max(1) as f32;
        // 约 8..24 像素一个控制点：足够平滑，又能表达腰部/手臂附近的局部变化。
        let cell = (short / 160.0).clamp(6.0, 24.0);
        let cols = ((width as f32 / cell).ceil() as u32 + 1).max(2);
        let rows = ((height as f32 / cell).ceil() as u32 + 1).max(2);
        Self {
            width,
            height,
            cols,
            rows,
            dx: vec![0.0; (cols * rows) as usize],
            dy: vec![0.0; (cols * rows) as usize],
        }
    }

    pub fn index(&self, col: u32, row: u32) -> usize {
        (row.min(self.rows - 1) * self.cols + col.min(self.cols - 1)) as usize
    }

    pub fn add(&mut self, col: u32, row: u32, dx: f32, dy: f32) {
        let index = self.index(col, row);
        self.dx[index] += dx;
        self.dy[index] += dy;
    }

    /// 网格顶点的图像坐标。
    pub fn vertex(&self, col: u32, row: u32) -> (f32, f32) {
        let x = col.min(self.cols - 1) as f32 / (self.cols - 1) as f32 * self.width as f32;
        let y = row.min(self.rows - 1) as f32 / (self.rows - 1) as f32 * self.height as f32;
        (x, y)
    }

    /// 双线性插值；返回内容位移（像素）。
    pub fn displacement_at(&self, x: f32, y: f32) -> (f32, f32) {
        if self.cols < 2 || self.rows < 2 {
            return (0.0, 0.0);
        }
        let gx = (x / self.width.max(1) as f32 * (self.cols - 1) as f32)
            .clamp(0.0, (self.cols - 1) as f32);
        let gy = (y / self.height.max(1) as f32 * (self.rows - 1) as f32)
            .clamp(0.0, (self.rows - 1) as f32);
        let x0 = gx.floor() as u32;
        let y0 = gy.floor() as u32;
        let x1 = (x0 + 1).min(self.cols - 1);
        let y1 = (y0 + 1).min(self.rows - 1);
        let tx = gx - x0 as f32;
        let ty = gy - y0 as f32;
        let sample = |values: &[f32]| {
            let a = values[self.index(x0, y0)];
            let b = values[self.index(x1, y0)];
            let c = values[self.index(x0, y1)];
            let d = values[self.index(x1, y1)];
            let top = a * (1.0 - tx) + b * tx;
            let bottom = c * (1.0 - tx) + d * tx;
            top * (1.0 - ty) + bottom * ty
        };
        (sample(&self.dx), sample(&self.dy))
    }

    pub fn max_magnitude(&self) -> f32 {
        self.dx
            .iter()
            .zip(self.dy.iter())
            .map(|(dx, dy)| (dx * dx + dy * dy).sqrt())
            .fold(0.0f32, f32::max)
    }

    pub fn scale(&mut self, factor: f32) {
        for value in self.dx.iter_mut().chain(self.dy.iter_mut()) {
            *value *= factor;
        }
    }

    /// 把超过上限的位移按比例压回。`max_x/max_y` 为像素。
    pub fn clamp_magnitude(&mut self, max_x: f32, max_y: f32) {
        let max_x = max_x.max(0.5);
        let max_y = max_y.max(0.5);
        for index in 0..self.dx.len() {
            self.dx[index] = self.dx[index].clamp(-max_x, max_x);
            self.dy[index] = self.dy[index].clamp(-max_y, max_y);
        }
    }

    pub fn is_zero(&self) -> bool {
        self.dx.iter().all(|value| value.abs() < 1e-4)
            && self.dy.iter().all(|value| value.abs() < 1e-4)
    }
}

/// 反向采样原图。人脸区域由 `FaceGuard` 平滑压回 0 位移，保证第四轮塑形坐标不变。
pub fn warp_image(image: &Image16, field: &GridField, guard: &FaceGuard) -> Image16 {
    let width = image.width();
    let height = image.height();
    if width == 0 || height == 0 || field.width != width || field.height != height {
        return image.clone();
    }
    if field.is_zero() {
        return image.clone();
    }
    let mut output: Image16 = Image16::new(width, height);
    let row_len = width as usize * 4;
    output
        .as_mut()
        .par_chunks_mut(row_len)
        .enumerate()
        .for_each(|(y, row)| {
            for x in 0..width as usize {
                let gx = x as f32 + 0.5;
                let gy = y as f32 + 0.5;
                let (dx, dy) = field.displacement_at(gx, gy);
                let weight = guard.weight(gx / width as f32, gy / height as f32);
                let sx = gx - dx * weight;
                let sy = gy - dy * weight;
                let pixel = sample_bilinear(image, sx, sy);
                let offset = x * 4;
                row[offset..offset + 4].copy_from_slice(&pixel);
            }
        });
    output
}

/// 用同一个位移场采样 Alpha，保证像素与蒙版始终同步。
pub fn warp_alpha(
    alpha: &[f32],
    width: u32,
    height: u32,
    field: &GridField,
    guard: &FaceGuard,
) -> Vec<f32> {
    if width == 0 || height == 0 || alpha.len() != width as usize * height as usize {
        return alpha.to_vec();
    }
    if field.is_zero() {
        return alpha.to_vec();
    }
    let mut output = vec![0.0f32; alpha.len()];
    output
        .par_chunks_mut(width as usize)
        .enumerate()
        .for_each(|(y, row)| {
            for x in 0..width as usize {
                let gx = x as f32 + 0.5;
                let gy = y as f32 + 0.5;
                let (dx, dy) = field.displacement_at(gx, gy);
                let weight = guard.weight(gx / width as f32, gy / height as f32);
                row[x] = sample_plane(alpha, width, height, gx - dx * weight, gy - dy * weight);
            }
        });
    output
}

fn sample_bilinear(image: &Image16, x: f32, y: f32) -> [u16; 4] {
    let width = image.width() as i32;
    let height = image.height() as i32;
    let x = (x - 0.5).clamp(0.0, (width - 1) as f32);
    let y = (y - 0.5).clamp(0.0, (height - 1) as f32);
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let p00 = image.get_pixel(x0 as u32, y0 as u32).0;
    let p10 = image.get_pixel(x1 as u32, y0 as u32).0;
    let p01 = image.get_pixel(x0 as u32, y1 as u32).0;
    let p11 = image.get_pixel(x1 as u32, y1 as u32).0;
    let mut out = [0u16; 4];
    for channel in 0..4 {
        let top = p00[channel] as f32 * (1.0 - tx) + p10[channel] as f32 * tx;
        let bottom = p01[channel] as f32 * (1.0 - tx) + p11[channel] as f32 * tx;
        out[channel] = (top * (1.0 - ty) + bottom * ty).round().clamp(0.0, 65535.0) as u16;
    }
    out
}

fn sample_plane(values: &[f32], width: u32, height: u32, x: f32, y: f32) -> f32 {
    let w = width as i32;
    let h = height as i32;
    let x = (x - 0.5).clamp(0.0, (w - 1) as f32);
    let y = (y - 0.5).clamp(0.0, (h - 1) as f32);
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let index = |px: i32, py: i32| py as usize * width as usize + px as usize;
    let top = values[index(x0, y0)] * (1.0 - tx) + values[index(x1, y0)] * tx;
    let bottom = values[index(x0, y1)] * (1.0 - tx) + values[index(x1, y1)] * tx;
    top * (1.0 - ty) + bottom * ty
}

#[cfg(test)]
mod tests {
    use super::super::constraints::FaceGuard;
    use super::*;
    use image::Rgba;

    #[test]
    fn zero_field_is_identity_for_image_and_alpha() {
        let image: Image16 = Image16::from_pixel(16, 16, Rgba([100, 200, 300, 65535]));
        let field = GridField::new(16, 16);
        let guard = FaceGuard::empty();
        assert_eq!(
            warp_image(&image, &field, &guard).into_raw(),
            image.into_raw()
        );
        let alpha = vec![0.5f32; 256];
        assert_eq!(warp_alpha(&alpha, 16, 16, &field, &guard), alpha);
    }

    #[test]
    fn displacement_moves_content() {
        let image: Image16 = Image16::from_fn(16, 16, |x, _| {
            if x == 8 {
                Rgba([65535, 0, 0, 65535])
            } else {
                Rgba([0, 0, 0, 65535])
            }
        });
        let mut field = GridField::new(16, 16);
        for row in 0..field.rows {
            for col in 0..field.cols {
                field.add(col, row, 3.0, 0.0);
            }
        }
        let warped = warp_image(&image, &field, &FaceGuard::empty());
        // 内容右移 3px：原始第 8 列红色出现在第 11 列。
        assert!(warped.get_pixel(11, 8).0[0] > 30000);
    }

    #[test]
    fn guard_blocks_displacement_inside_face() {
        let image: Image16 = Image16::from_pixel(20, 20, Rgba([0, 0, 0, 65535]));
        let mut field = GridField::new(20, 20);
        for row in 0..field.rows {
            for col in 0..field.cols {
                field.add(col, row, 4.0, 0.0);
            }
        }
        let guard = FaceGuard::from_normalized(&[(0.2, 0.2, 0.6, 0.6)]);
        let warped = warp_image(&image, &field, &guard);
        // 输出尺寸不变；示例像素仍为黑色（不做逐像素断言，只验证不越界）。
        assert_eq!(warped.dimensions(), (20, 20));
        assert_eq!(guard.weight(0.5, 0.5), 0.0);
        assert_eq!(guard.weight(0.01, 0.01), 1.0);
    }

    #[test]
    fn field_clamps_and_scales() {
        let mut field = GridField::new(40, 40);
        for row in 0..field.rows {
            for col in 0..field.cols {
                field.add(col, row, 100.0, -100.0);
            }
        }
        field.clamp_magnitude(4.0, 6.0);
        let (dx, dy) = field.displacement_at(10.0, 10.0);
        assert!(dx <= 4.0 + 0.01 && dy >= -6.01);
        field.scale(0.5);
        let magnitude = field.max_magnitude();
        assert!(magnitude <= (4.0f32 * 4.0 + 6.0 * 6.0).sqrt() + 0.01);
    }
}
