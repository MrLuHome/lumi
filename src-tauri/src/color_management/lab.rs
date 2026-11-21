//! 色彩计算：sRGB ↔ CIE Lab、CIEDE2000 色差。
//!
//! 只做数值计算，不依赖 Little CMS；用于色卡评估、软打样与色域警告。

/// sRGB 编码值（0..1）→ CIE XYZ（D65）。
pub fn srgb_to_xyz(rgb: [f32; 3]) -> [f32; 3] {
    let linear: Vec<f32> = rgb
        .iter()
        .map(|value| {
            let v = value.clamp(0.0, 1.0);
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        })
        .collect();
    [
        0.4124564 * linear[0] + 0.3575761 * linear[1] + 0.1804375 * linear[2],
        0.2126729 * linear[0] + 0.7151522 * linear[1] + 0.0721750 * linear[2],
        0.0193339 * linear[0] + 0.119192 * linear[1] + 0.9503041 * linear[2],
    ]
}

/// sRGB（0..1）→ CIE Lab（D65 白点）。
pub fn srgb_to_lab(rgb: [f32; 3]) -> [f32; 3] {
    let xyz = srgb_to_xyz(rgb);
    // D65
    let white = [0.95047, 1.0, 1.08883];
    let epsilon = 216.0 / 24389.0;
    let kappa = 24389.0 / 27.0;
    let f = |value: f32| -> f32 {
        if value > epsilon {
            value.cbrt()
        } else {
            (kappa * value + 16.0) / 116.0
        }
    };
    let fx = f(xyz[0] / white[0]);
    let fy = f(xyz[1] / white[1]);
    let fz = f(xyz[2] / white[2]);
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// 16-bit sRGB → Lab。
pub fn rgb16_to_lab(pixel: [u16; 3]) -> [f32; 3] {
    srgb_to_lab([
        pixel[0] as f32 / 65535.0,
        pixel[1] as f32 / 65535.0,
        pixel[2] as f32 / 65535.0,
    ])
}

/// CIEDE2000 色差。
pub fn delta_e00(lab1: [f32; 3], lab2: [f32; 3]) -> f32 {
    let [l1, a1, b1] = lab1;
    let [l2, a2, b2] = lab2;
    let c1 = (a1 * a1 + b1 * b1).sqrt();
    let c2 = (a2 * a2 + b2 * b2).sqrt();
    let c_bar = (c1 + c2) / 2.0;
    let c_bar7 = c_bar.powi(7);
    let g = 0.5 * (1.0 - (c_bar7 / (c_bar7 + 25f32.powi(7))).sqrt());
    let a1p = (1.0 + g) * a1;
    let a2p = (1.0 + g) * a2;
    let c1p = (a1p * a1p + b1 * b1).sqrt();
    let c2p = (a2p * a2p + b2 * b2).sqrt();
    let h1p = atan2_deg(b1, a1p);
    let h2p = atan2_deg(b2, a2p);

    let delta_l = l2 - l1;
    let delta_c = c2p - c1p;
    let delta_h = if c1p * c2p <= 1e-9 {
        0.0
    } else if (h2p - h1p).abs() <= 180.0 {
        h2p - h1p
    } else if h2p - h1p > 180.0 {
        h2p - h1p - 360.0
    } else {
        h2p - h1p + 360.0
    };
    let delta_h_big = 2.0 * (c1p * c2p).sqrt() * (delta_h.to_radians() / 2.0).sin();

    let l_bar = (l1 + l2) / 2.0;
    let c_bar_p = (c1p + c2p) / 2.0;
    let h_bar = if c1p * c2p <= 1e-9 {
        h1p + h2p
    } else if (h1p - h2p).abs() <= 180.0 {
        (h1p + h2p) / 2.0
    } else if h1p + h2p < 360.0 {
        (h1p + h2p + 360.0) / 2.0
    } else {
        (h1p + h2p - 360.0) / 2.0
    };

    let t = 1.0 - 0.17 * (h_bar - 30.0).to_radians().cos()
        + 0.24 * (2.0 * h_bar).to_radians().cos()
        + 0.32 * (3.0 * h_bar + 6.0).to_radians().cos()
        - 0.20 * (4.0 * h_bar - 63.0).to_radians().cos();
    let delta_theta = 30.0 * (-((h_bar - 275.0) / 25.0).powi(2)).exp();
    let r_c = 2.0 * (c_bar_p.powi(7) / (c_bar_p.powi(7) + 25f32.powi(7))).sqrt();
    let s_l = 1.0 + 0.015 * (l_bar - 50.0).powi(2) / (20.0 + (l_bar - 50.0).powi(2)).sqrt();
    let s_c = 1.0 + 0.045 * c_bar_p;
    let s_h = 1.0 + 0.015 * c_bar_p * t;
    let r_t = -r_c * (2.0 * delta_theta).to_radians().sin();

    let term_l = delta_l / s_l;
    let term_c = delta_c / s_c;
    let term_h = delta_h_big / s_h;
    (term_l * term_l + term_c * term_c + term_h * term_h + r_t * term_c * term_h).sqrt()
}

fn atan2_deg(y: f32, x: f32) -> f32 {
    let angle = y.atan2(x).to_degrees();
    if angle < 0.0 {
        angle + 360.0
    } else {
        angle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_primaries_map_to_expected_lab() {
        let white = srgb_to_lab([1.0, 1.0, 1.0]);
        assert!((white[0] - 100.0).abs() < 0.2);
        assert!(white[1].abs() < 0.1 && white[2].abs() < 0.1);

        let red = srgb_to_lab([1.0, 0.0, 0.0]);
        assert!((red[0] - 53.24).abs() < 0.6, "L={}", red[0]);
        assert!((red[1] - 80.09).abs() < 0.8, "a={}", red[1]);
    }

    #[test]
    fn delta_e_is_zero_for_identical_and_large_for_opposites() {
        let a = srgb_to_lab([0.2, 0.5, 0.8]);
        assert!(delta_e00(a, a) < 1e-4);
        let b = srgb_to_lab([0.8, 0.3, 0.1]);
        assert!(delta_e00(a, b) > 20.0);
    }

    #[test]
    fn known_delta_e_pair() {
        // Sharma 标准测试对：ΔE00 应约为 2.0425。
        let a = [50.0, 2.6772, -79.7751];
        let b = [50.0, 0.0, -82.7485];
        let delta = delta_e00(a, b);
        assert!((delta - 2.0425).abs() < 0.01, "ΔE={delta}");
    }
}
