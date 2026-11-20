//! sRGB ↔ CIELAB（D65）转换。肤色调整在感知均匀空间完成，
//! 避免直接缩放 RGB 造成的偏色与灰白。

pub fn srgb_to_lab(r: f32, g: f32, b: f32) -> [f32; 3] {
    let linear = |value: f32| {
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    let r = linear(r.clamp(0.0, 1.0));
    let g = linear(g.clamp(0.0, 1.0));
    let b = linear(b.clamp(0.0, 1.0));

    let x = (0.412_456 * r + 0.357_576 * g + 0.180_423 * b) / 0.950_47;
    let y = 0.212_673 * r + 0.715_160 * g + 0.072_169 * b;
    let z = (0.019_334 * r + 0.119_192 * g + 0.950_304 * b) / 1.088_83;

    let f = |value: f32| {
        if value > 0.008_856 {
            value.cbrt()
        } else {
            7.787 * value + 16.0 / 116.0
        }
    };
    let fx = f(x);
    let fy = f(y);
    let fz = f(z);
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

pub fn lab_to_srgb(l: f32, a: f32, b: f32) -> [f32; 3] {
    let fy = (l + 16.0) / 116.0;
    let fx = fy + a / 500.0;
    let fz = fy - b / 200.0;
    let inverse = |value: f32| {
        if value > 0.206_897 {
            value.powi(3)
        } else {
            (value - 16.0 / 116.0) / 7.787
        }
    };
    let x = inverse(fx) * 0.950_47;
    let y = inverse(fy);
    let z = inverse(fz) * 1.088_83;

    let r = 3.240_454 * x - 1.537_138 * y - 0.498_531 * z;
    let g = -0.969_266 * x + 1.876_010 * y + 0.041_556 * z;
    let b = 0.055_643 * x - 0.204_026 * y + 1.057_225 * z;

    let gamma = |value: f32| {
        if value <= 0.003_130_8 {
            12.92 * value
        } else {
            1.055 * value.powf(1.0 / 2.4) - 0.055
        }
    };
    [
        gamma(r).clamp(0.0, 1.0),
        gamma(g).clamp(0.0, 1.0),
        gamma(b).clamp(0.0, 1.0),
    ]
}

/// Lab 色度幅度，用于限制调整后的饱和度不越界。
pub fn chroma(a: f32, b: f32) -> f32 {
    (a * a + b * b).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lab_round_trip_is_close() {
        for color in [
            [0.2, 0.3, 0.4],
            [0.8, 0.6, 0.5],
            [0.95, 0.9, 0.85],
            [0.05, 0.05, 0.05],
        ] {
            let lab = srgb_to_lab(color[0], color[1], color[2]);
            let back = lab_to_srgb(lab[0], lab[1], lab[2]);
            for channel in 0..3 {
                assert!(
                    (back[channel] - color[channel]).abs() < 0.01,
                    "通道 {channel} 往返误差过大: {color:?} -> {back:?}"
                );
            }
        }
    }
}
