//! Colour arithmetic: mixing in OKLab, as kinemo does (`Expr::Mix`).
//!
//! Mixing in sRGB makes the middle of a blue-to-yellow ramp grey and dark;
//! OKLab is perceptually even, so `mix(cool, hot, 0.5)` looks half-way.

fn to_linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        libm::pow((c + 0.055) / 1.055, 2.4)
    }
}

fn to_srgb(c: f64) -> f64 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * libm::pow(c, 1.0 / 2.4) - 0.055
    }
}

/// sRGB (0..1) to OKLab.
pub fn oklab(rgb: [f64; 3]) -> [f64; 3] {
    let [r, g, b] = rgb.map(to_linear);
    let l = libm::cbrt(0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b);
    let m = libm::cbrt(0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b);
    let s = libm::cbrt(0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b);
    [
        0.210_454_255_3 * l + 0.793_617_785_0 * m - 0.004_072_046_8 * s,
        1.977_998_495_1 * l - 2.428_592_205_0 * m + 0.450_593_709_9 * s,
        0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766_0 * s,
    ]
}

/// OKLab to sRGB (0..1), clamped into gamut.
pub fn srgb(lab: [f64; 3]) -> [f64; 3] {
    let [l, a, b] = lab;
    let l_ = l + 0.396_337_777_4 * a + 0.215_803_757_3 * b;
    let m_ = l - 0.105_561_345_8 * a - 0.063_854_172_8 * b;
    let s_ = l - 0.089_484_177_5 * a - 1.291_485_548_0 * b;
    let (l3, m3, s3) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    let r = 4.076_741_662_1 * l3 - 3.307_711_591_3 * m3 + 0.230_969_929_2 * s3;
    let g = -1.268_438_004_6 * l3 + 2.609_757_401_1 * m3 - 0.341_319_396_5 * s3;
    let bl = -0.004_196_086_3 * l3 - 0.703_418_614_7 * m3 + 1.707_614_701_0 * s3;
    [r, g, bl].map(|c| to_srgb(c.clamp(0.0, 1.0)).clamp(0.0, 1.0))
}

/// `a` and `b` mixed by `t` in OKLab; alpha mixes linearly.
pub fn mix(a: [f64; 4], b: [f64; 4], t: f64) -> [f64; 4] {
    let la = oklab([a[0], a[1], a[2]]);
    let lb = oklab([b[0], b[1], b[2]]);
    let lab = [0, 1, 2].map(|i| la[i] + (lb[i] - la[i]) * t);
    let [r, g, bl] = srgb(lab);
    [r, g, bl, a[3] + (b[3] - a[3]) * t]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oklab_round_trips() {
        for rgb in [
            [1.0, 0.0, 0.0],
            [0.2, 0.5, 0.8],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 0.0],
        ] {
            let back = srgb(oklab(rgb));
            for i in 0..3 {
                assert!((back[i] - rgb[i]).abs() < 1e-6, "{rgb:?} -> {back:?}");
            }
        }
    }

    #[test]
    fn mixing_ends_at_its_ends() {
        let a = [0.0, 0.447, 0.698, 1.0];
        let b = [0.835, 0.369, 0.0, 0.5];
        let start = mix(a, b, 0.0);
        let end = mix(a, b, 1.0);
        for i in 0..4 {
            assert!((start[i] - a[i]).abs() < 1e-6);
            assert!((end[i] - b[i]).abs() < 1e-6);
        }
    }
}
