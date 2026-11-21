//! X-Trans 基础插值。
//!
//! 说明（能力标注）：这是**基础**实现——按 6×6 色阵在邻域内对同色样本取平均，
//! 不做方向判别、不做 AHD / Markesteijn 迭代。它能从真实 RAF 得到可用的彩色图像，
//! 但细线与高对比边缘会出现拉链伪影，质量等级明确低于 Bayer 的 Malvar-He-Cutler。
//! UI 中必须显示“X-Trans 基础插值（质量有限）”，不得标注为专业级。

use rayon::prelude::*;

use super::demosaic::RgbLinear;
use super::linear::LinearImage;

const SIDE: usize = 6;

type PhaseOffsets = [Vec<(isize, isize)>; 3];

pub fn interpolate(image: &LinearImage, matrix: &[[u8; SIDE]; SIDE], scale: f32) -> RgbLinear {
    let width = image.width as usize;
    let height = image.height as usize;
    // 预计算每个相位、每个目标通道在半径 2/3 内的采样偏移。
    let mut offsets: Vec<Vec<PhaseOffsets>> = Vec::with_capacity(SIDE);
    for py in 0..SIDE {
        let mut row: Vec<PhaseOffsets> = Vec::with_capacity(SIDE);
        for px in 0..SIDE {
            let mut cell: PhaseOffsets = Default::default();
            for (channel, slot) in cell.iter_mut().enumerate() {
                let mut found = collect_offsets(matrix, px, py, channel as u8, 2);
                if found.is_empty() {
                    found = collect_offsets(matrix, px, py, channel as u8, 3);
                }
                *slot = found;
            }
            row.push(cell);
        }
        offsets.push(row);
    }

    let at = |x: isize, y: isize| -> f32 {
        let x = x.clamp(0, width as isize - 1) as usize;
        let y = y.clamp(0, height as isize - 1) as usize;
        image.data[y * width + x] as f32 * scale
    };

    let mut data = vec![0u16; width * height * 3];
    data.par_chunks_mut(3).enumerate().for_each(|(index, pixel)| {
        let x = index % width;
        let y = index / width;
        let phase = matrix[y % SIDE][x % SIDE] as usize;
        let own = matrix[y % SIDE][x % SIDE];
        let center = at(x as isize, y as isize);
        for channel in 0..3usize {
            let value = if own as usize == channel {
                center
            } else if channel == 1 {
                // 绿色样本更密，用 3×3 内所有绿色样本平均。
                average_at(&at, x as isize, y as isize, &offsets[y % SIDE][x % SIDE][1])
            } else {
                average_at(&at, x as isize, y as isize, &offsets[y % SIDE][x % SIDE][channel])
            };
            pixel[channel] = value.clamp(0.0, 1.0).mul_add(65535.0, 0.5) as u16;
        }
        let _ = phase;
    });

    RgbLinear {
        width: image.width,
        height: image.height,
        data,
        white_level: image.white_level,
        algorithm: "X-Trans 基础插值（非 AHD，质量有限）".to_string(),
    }
}

fn collect_offsets(
    matrix: &[[u8; SIDE]; SIDE],
    px: usize,
    py: usize,
    channel: u8,
    radius: isize,
) -> Vec<(isize, isize)> {
    let mut out = Vec::new();
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx == 0 && dy == 0 {
                continue;
            }
            // 曼哈顿距离限制，避免 3 半径下取到过远的样本。
            if dx.abs() + dy.abs() > radius + 1 {
                continue;
            }
            let x = (px as isize + dx).rem_euclid(SIDE as isize) as usize;
            let y = (py as isize + dy).rem_euclid(SIDE as isize) as usize;
            if matrix[y][x] == channel {
                out.push((dx, dy));
            }
        }
    }
    out
}

fn average_at(
    sample: &impl Fn(isize, isize) -> f32,
    x: isize,
    y: isize,
    offsets: &[(isize, isize)],
) -> f32 {
    if offsets.is_empty() {
        return 0.0;
    }
    let mut sum = 0.0;
    for (dx, dy) in offsets {
        sum += sample(x + dx, y + dy);
    }
    sum / offsets.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn x_trans_image() -> LinearImage {
        // 标准 X-Trans 6×6 色阵（0=R, 1=G, 2=B）。
        let matrix: [[u8; 6]; 6] = [
            [0, 1, 1, 2, 1, 1],
            [1, 1, 0, 1, 1, 2],
            [1, 0, 1, 1, 2, 1],
            [2, 1, 1, 0, 1, 1],
            [1, 1, 2, 1, 1, 0],
            [1, 2, 1, 1, 0, 1],
        ];
        let (width, height) = (12u32, 12u32);
        let mut data = Vec::with_capacity((width * height) as usize);
        for y in 0..height as usize {
            for x in 0..width as usize {
                data.push(match matrix[y % 6][x % 6] {
                    0 => 1000,
                    1 => 2000,
                    _ => 3000,
                });
            }
        }
        LinearImage {
            width,
            height,
            data,
            cfa: "X-Trans".into(),
            black_level: [0; 4],
            white_balance: [1.0; 4],
            white_level: 65535,
            xtrans: Some(matrix),
            camera_matrix: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        }
    }

    #[test]
    fn xtrans_flat_field_is_reconstructed() {
        let image = x_trans_image();
        let matrix = image.xtrans.unwrap();
        let out = interpolate(&image, &matrix, 1.0 / 65535.0);
        assert!(out.algorithm.contains("基础"));
        for y in 3..9 {
            for x in 3..9 {
                let [r, g, b] = out.pixel(x, y);
                assert!((r as i32 - 1000).abs() <= 2, "R {r} at {x},{y}");
                assert!((g as i32 - 2000).abs() <= 2, "G {g} at {x},{y}");
                assert!((b as i32 - 3000).abs() <= 2, "B {b} at {x},{y}");
            }
        }
    }
}
