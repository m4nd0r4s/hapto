//! Colours and text (SPEC-2.0 §4.6.10–§4.6.11).
//!
//! Colours are sRGB with straight alpha and mix in OKLab. Theme colours come
//! from the host by token name (`palette("hot")`), so a figure says what a
//! colour *means* and the page decides what it looks like in light and dark.

use super::{Builtin, Overload, Pat};
use crate::colour::{mix, oklab, srgb};
use crate::error::Error;
use crate::format;
use crate::value::Value;

const RGB: &[Overload] = &[Overload::new(&[Pat::Num, Pat::Num, Pat::Num], Pat::Colour)];
const RGBA: &[Overload] = &[Overload::new(
    &[Pat::Num, Pat::Num, Pat::Num, Pat::Num],
    Pat::Colour,
)];
const PALETTE: &[Overload] = &[Overload::new(&[Pat::Str], Pat::Colour)];
const MIX: &[Overload] = &[Overload::new(
    &[Pat::Colour, Pat::Colour, Pat::Num],
    Pat::Colour,
)];
const WITH_ALPHA: &[Overload] = &[Overload::new(&[Pat::Colour, Pat::Num], Pat::Colour)];
const SCALE: &[Overload] = &[Overload::new(
    &[Pat::Str, Pat::Num, Pat::Num, Pat::Num],
    Pat::Colour,
)];
const FORMAT: &[Overload] = &[Overload::new(&[Pat::Num, Pat::Str], Pat::Str)];

/// The scale names `colour_scale` and the `scale:` keys accept (§9.3).
pub const SCALES: &[&str] = &["sequential", "diverging", "cyclic", "categorical"];

/// Viridis, sampled at five stops: perceptually even, colour-blind safe.
const VIRIDIS: [[f64; 3]; 5] = [
    [0.267, 0.005, 0.329],
    [0.231, 0.322, 0.545],
    [0.129, 0.565, 0.553],
    [0.369, 0.788, 0.384],
    [0.992, 0.906, 0.145],
];

/// Okabe–Ito, the categorical fallback when the page has no `c1`…`c8`.
const OKABE_ITO: [[f64; 3]; 8] = [
    [0.0, 0.447, 0.698],
    [0.835, 0.369, 0.0],
    [0.0, 0.620, 0.451],
    [0.800, 0.475, 0.655],
    [0.902, 0.624, 0.0],
    [0.337, 0.706, 0.914],
    [0.941, 0.894, 0.259],
    [0.0, 0.0, 0.0],
];

fn colour(v: &Value) -> Option<[f64; 4]> {
    match v {
        Value::Colour(c) => Some(*c),
        _ => None,
    }
}

fn unit(x: f64) -> Option<f64> {
    x.is_finite().then(|| x.clamp(0.0, 1.0))
}

fn out(c: Option<[f64; 4]>) -> Result<Value, Error> {
    Ok(c.map(Value::Colour).unwrap_or(Value::Nothing))
}

fn hsl(h: f64, s: f64, l: f64) -> Option<[f64; 4]> {
    let (s, l) = (unit(s)?, unit(l)?);
    if !h.is_finite() {
        return None;
    }
    let h = (h % 360.0 + 360.0) % 360.0 / 60.0;
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    Some([r + m, g + m, b + m, 1.0])
}

/// Piecewise OKLab interpolation across `stops` at `t` in [0, 1].
fn ramp(stops: &[[f64; 3]], t: f64) -> [f64; 4] {
    let t = t.clamp(0.0, 1.0) * (stops.len() - 1) as f64;
    let i = (t.floor() as usize).min(stops.len() - 2);
    let (a, b) = (stops[i], stops[i + 1]);
    mix(
        [a[0], a[1], a[2], 1.0],
        [b[0], b[1], b[2], 1.0],
        t - i as f64,
    )
}

/// Whether the page background is dark: sequential scales run light to dark
/// on a light page and dark to light on a dark one (§9.3).
fn dark_page(palette: &dyn Fn(&str) -> Option<[f64; 4]>) -> bool {
    palette("bg").is_some_and(|bg| oklab([bg[0], bg[1], bg[2]])[0] < 0.5)
}

fn scale(
    name: &str,
    v: f64,
    lo: f64,
    hi: f64,
    palette: &dyn Fn(&str) -> Option<[f64; 4]>,
) -> Option<[f64; 4]> {
    if !(v.is_finite() && lo.is_finite() && hi.is_finite()) || lo == hi {
        return None;
    }
    let t = (v - lo) / (hi - lo);
    match name {
        "sequential" => {
            // Viridis runs dark to light; reversed, it stands out on a light page.
            let t = if dark_page(palette) { t } else { 1.0 - t };
            Some(ramp(&VIRIDIS, t))
        }
        "diverging" => {
            let cool = palette("cool").unwrap_or([0.0, 0.447, 0.698, 1.0]);
            let hot = palette("hot").unwrap_or([0.835, 0.369, 0.0, 1.0]);
            let neutral = if dark_page(palette) {
                [0.3, 0.3, 0.3, 1.0]
            } else {
                [0.94, 0.94, 0.94, 1.0]
            };
            let t = t.clamp(0.0, 1.0);
            Some(if t < 0.5 {
                mix(cool, neutral, t * 2.0)
            } else {
                mix(neutral, hot, (t - 0.5) * 2.0)
            })
        }
        "cyclic" => {
            // Constant lightness and chroma, hue all the way round.
            let angle = std::f64::consts::TAU * (t - t.floor());
            let [r, g, b] = srgb([0.7, 0.12 * libm::cos(angle), 0.12 * libm::sin(angle)]);
            Some([r, g, b, 1.0])
        }
        "categorical" => {
            let k = (v.round() as i64).rem_euclid(8) as usize;
            palette(&format!("c{}", k + 1)).or({
                let c = OKABE_ITO[k];
                Some([c[0], c[1], c[2], 1.0])
            })
        }
        _ => None,
    }
}

pub(super) const BUILTINS: &[Builtin] = &[
    Builtin {
        name: "rgb",
        overloads: RGB,
        takes_fn: false,
        imp: |_, _, a| {
            out((|| {
                Some([unit(a[0].num())?, unit(a[1].num())?, unit(a[2].num())?, 1.0])
            })())
        },
    },
    Builtin {
        name: "rgba",
        overloads: RGBA,
        takes_fn: false,
        imp: |_, _, a| {
            out((|| {
                Some([
                    unit(a[0].num())?,
                    unit(a[1].num())?,
                    unit(a[2].num())?,
                    unit(a[3].num())?,
                ])
            })())
        },
    },
    Builtin {
        name: "hsl",
        overloads: RGB,
        takes_fn: false,
        imp: |_, _, a| out(hsl(a[0].num(), a[1].num(), a[2].num())),
    },
    Builtin {
        name: "palette",
        overloads: PALETTE,
        takes_fn: false,
        imp: |h, _, a| {
            out(match &a[0] {
                Value::Str(name) => h.palette(name),
                _ => None,
            })
        },
    },
    Builtin {
        name: "mix",
        overloads: MIX,
        takes_fn: false,
        imp: |_, _, a| {
            let t = a[2].num();
            out(match (colour(&a[0]), colour(&a[1])) {
                (Some(p), Some(q)) if t.is_finite() => Some(mix(p, q, t)),
                _ => None,
            })
        },
    },
    Builtin {
        name: "with_alpha",
        overloads: WITH_ALPHA,
        takes_fn: false,
        imp: |_, _, a| {
            out((|| {
                let c = colour(&a[0])?;
                Some([c[0], c[1], c[2], unit(a[1].num())?])
            })())
        },
    },
    Builtin {
        name: "colour_scale",
        overloads: SCALE,
        takes_fn: false,
        imp: |h, _, a| {
            let Value::Str(name) = &a[0] else {
                return out(None);
            };
            let palette = |n: &str| h.palette(n);
            out(scale(name, a[1].num(), a[2].num(), a[3].num(), &palette))
        },
    },
    Builtin {
        name: "format",
        overloads: FORMAT,
        takes_fn: false,
        imp: |_, _, a| {
            Ok(match &a[1] {
                Value::Str(spec) => match format::format(a[0].num(), spec) {
                    Some(text) => Value::str(&text),
                    None => Value::Nothing,
                },
                _ => Value::Nothing,
            })
        },
    },
];

#[cfg(test)]
mod tests {
    use super::super::testing::{eval_at, nothing};
    use crate::value::Value;

    fn rgba(formula: &str) -> [f64; 4] {
        match eval_at(formula, 0.0) {
            Value::Colour(c) => c,
            other => panic!("{formula} = {other}, want a colour"),
        }
    }

    fn near(a: [f64; 4], b: [f64; 4]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3)
    }

    #[test]
    fn building_colours() {
        assert_eq!(rgba("rgb(1, 0, 0)"), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(rgba("rgba(0, 0, 1, 0.5)"), [0.0, 0.0, 1.0, 0.5]);
        assert!(near(rgba("hsl(120, 1, 0.5)"), [0.0, 1.0, 0.0, 1.0]));
        assert!(near(rgba("hsl(-120, 1, 0.5)"), [0.0, 0.0, 1.0, 1.0]));
        assert_eq!(rgba("with_alpha(#ff0000, 0.25)")[3], 0.25);
        assert_eq!(rgba("rgb(2, -1, 0)"), [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn theme_colours_come_from_the_page() {
        assert!(near(rgba("palette('hot')"), [0.835, 0.369, 0.0, 1.0]));
        nothing("palette('nope')");
        let mid = rgba("mix(palette('cool'), palette('hot'), 0.5)");
        assert!(mid[0] > 0.0 && mid[0] < 0.835);
    }

    #[test]
    fn scales() {
        // On a light page (the test palette's bg), sequential runs light to dark.
        let low = rgba("colour_scale('sequential', 0, 0, 1)");
        let high = rgba("colour_scale('sequential', 1, 0, 1)");
        assert!(low[1] > high[1], "{low:?} {high:?}");
        let centre = rgba("colour_scale('diverging', 0, -1, 1)");
        assert!(near(centre, [0.94, 0.94, 0.94, 1.0]));
        assert!(near(
            rgba("colour_scale('diverging', 1, -1, 1)"),
            rgba("palette('hot')")
        ));
        assert!(near(
            rgba("colour_scale('cyclic', 0, 0, 1)"),
            rgba("colour_scale('cyclic', 1, 0, 1)")
        ));
        assert!(near(
            rgba("colour_scale('categorical', 1, 0, 1)"),
            rgba("palette('c2')")
        ));
        // Beyond c2 the test page has none: Okabe-Ito fills in.
        assert!(near(
            rgba("colour_scale('categorical', 2, 0, 1)"),
            [0.0, 0.620, 0.451, 1.0]
        ));
        nothing("colour_scale('rainbow', 0, 0, 1)");
        nothing("colour_scale('sequential', 0, 1, 1)");
    }

    #[test]
    fn format_builds_strings() {
        assert_eq!(eval_at("format(pi, '.2f')", 0.0), Value::str("3.14"));
        assert_eq!(eval_at("format(0.25, '.0%')", 0.0), Value::str("25%"));
        nothing("format(1, 'zz')");
        assert_eq!(eval_at("format(1 / 0, '.2f')", 0.0), Value::str("—"));
    }
}
