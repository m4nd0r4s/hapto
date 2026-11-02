//! Ticks, tick labels, axis labels and view titles (SPEC-2.0 §7.2, §7.4).

use crate::document::{Ticks, View};
use crate::layout::{log_ticks, nice_ticks, Frame};
use crate::scene::{Builder, Paint};
use crate::theme::Theme;

/// Tick labels, CSS px.
pub const SIZE: f64 = 12.0;
/// Axis labels and view titles, CSS px.
const TITLE: f64 = 13.0;
/// Length of a tick mark, CSS px.
const TICK: f64 = 4.0;
/// The least space between two tick labels, CSS px.
const GAP: f64 = 6.0;

/// One tick: where it is, and its label (none for a minor tick).
#[derive(Debug, Clone, PartialEq)]
pub struct Tick {
    pub at: f64,
    pub label: Option<String>,
}

/// The ticks of one axis over `(lo, hi)`, which is `length` CSS px long.
pub fn ticks(spec: &Ticks, lo: f64, hi: f64, log: bool, length: f64, phone: bool) -> Vec<Tick> {
    if !(lo.is_finite() && hi.is_finite() && lo < hi) {
        return vec![];
    }
    // About 80 px apart (60 on phones), but at least three ticks on a short axis.
    let spacing = f64::min(if phone { 60.0 } else { 80.0 }, length / 3.0);
    let inside = |v: &f64| *v >= lo - (hi - lo) * 1e-9 && *v <= hi + (hi - lo) * 1e-9;
    match spec {
        Ticks::None => vec![],
        Ticks::Auto if log => {
            let mut out: Vec<Tick> = log_ticks(lo, hi)
                .into_iter()
                .map(|v| Tick {
                    at: v,
                    label: Some(decade(v)),
                })
                .collect();
            // Minor ticks at 2 and 5 of each decade in view.
            let (a, b) = (lo.log10().floor() as i32, hi.log10().ceil() as i32);
            for k in a..=b {
                for m in [2.0, 5.0] {
                    let v = m * 10f64.powi(k);
                    if inside(&v) {
                        out.push(Tick { at: v, label: None });
                    }
                }
            }
            out
        }
        Ticks::Auto => numbered(nice_ticks(lo, hi, length, spacing)),
        Ticks::Step(step) if *step > 0.0 => {
            let (a, b) = ((lo / step).ceil() as i64, (hi / step).floor() as i64);
            if b - a > 1000 {
                return numbered(nice_ticks(lo, hi, length, spacing));
            }
            numbered((a..=b).map(|i| i as f64 * step).collect())
        }
        Ticks::Step(_) => vec![],
        Ticks::At(at) => numbered(at.iter().copied().filter(inside).collect()),
        Ticks::Labelled(pairs) => pairs
            .iter()
            .filter(|(v, _)| inside(v))
            .map(|(v, l)| Tick {
                at: *v,
                label: Some(l.clone()),
            })
            .collect(),
        Ticks::Pi => {
            let quarter = std::f64::consts::FRAC_PI_4;
            // π/4 when there is room for it, else π/2.
            let per_unit = length / (hi - lo);
            let q = if quarter * per_unit >= spacing { 1 } else { 2 };
            let (a, b) = (
                (lo / (quarter * q as f64)).ceil() as i64,
                (hi / (quarter * q as f64)).floor() as i64,
            );
            (a..=b)
                .map(|i| Tick {
                    at: i as f64 * quarter * q as f64,
                    label: Some(pi_label(i * q)),
                })
                .collect()
        }
    }
}

/// Numbers labelled with the fewest digits that tell them apart.
fn numbered(values: Vec<f64>) -> Vec<Tick> {
    // The fewest decimals at which every value is exact.
    let digits = (0..=12)
        .find(|d| {
            let k = 10f64.powi(*d);
            values.iter().all(|v| {
                let x = v * k;
                (x - x.round()).abs() < 1e-6 * x.abs().max(1.0)
            })
        })
        .unwrap_or(12) as usize;
    values
        .into_iter()
        .map(|v| Tick {
            at: v,
            label: Some(number(v, digits)),
        })
        .collect()
}

/// A tick number: fixed digits, a true minus sign, never "-0".
pub fn number(v: f64, digits: usize) -> String {
    let s = format!("{v:.digits$}");
    let zero = s
        .trim_start_matches('-')
        .chars()
        .all(|c| c == '0' || c == '.');
    let s = if zero {
        s.trim_start_matches('-').to_string()
    } else {
        s
    };
    s.replace('-', "\u{2212}")
}

/// A decade on a log axis: 1, 10, 100, 1000, else 10^k in TeX.
pub fn decade(v: f64) -> String {
    let k = v.log10().round() as i32;
    if (0..=3).contains(&k) {
        number(v, 0)
    } else if (-2..0).contains(&k) {
        number(v, (-k) as usize)
    } else {
        format!("$10^{{{k}}}$")
    }
}

/// `n` quarters of π, reduced, in TeX: 0, π/4, π/2, 3π/4, π, …
pub fn pi_label(n: i64) -> String {
    if n == 0 {
        return "0".into();
    }
    let g = gcd(n.unsigned_abs(), 4) as i64;
    let (num, den) = (n / g, 4 / g);
    let sign = if num < 0 { "-" } else { "" };
    let k = num.abs();
    let top = if k == 1 {
        r"\pi".to_string()
    } else {
        format!(r"{k}\pi")
    };
    if den == 1 {
        format!("${sign}{top}$")
    } else {
        format!(r"${sign}\frac{{{top}}}{{{den}}}$")
    }
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// Every TeX string a `pi` axis can show, so a bake can include them all.
pub fn every_pi_label() -> Vec<String> {
    (-16..=16)
        .map(pi_label)
        .filter(|l| l.starts_with('$'))
        .collect()
}

/// Drop every other label, repeatedly, until no two overlap (§7.4). `extent`
/// is a label's size along the axis.
fn thin(mut shown: Vec<(f64, String, f64)>) -> Vec<(f64, String, f64)> {
    let overlaps = |s: &[(f64, String, f64)]| {
        s.windows(2)
            .any(|w| (w[1].0 - w[0].0).abs() < (w[0].2 + w[1].2) / 2.0 + GAP)
    };
    shown.sort_by(|a, b| a.0.total_cmp(&b.0));
    while shown.len() > 1 && overlaps(&shown) {
        shown = shown.into_iter().step_by(2).collect();
    }
    shown
}

fn ink(theme: &Theme, token: &str) -> Paint {
    Paint {
        fill: theme.colour(token),
        fill_opacity: 1.0,
        opacity: 1.0,
        ..Default::default()
    }
}

fn hair(theme: &Theme, token: &str) -> Paint {
    Paint {
        stroke: theme.colour(token),
        width: 1.0,
        opacity: 1.0,
        ..Default::default()
    }
}

/// The ticks of both axes at this frame, as drawn.
pub fn of_frame(view: &View, f: &Frame, phone: bool) -> (Vec<Tick>, Vec<Tick>) {
    let m = &f.mapping;
    let p = m.plot;
    (
        ticks(
            &view.x.ticks,
            m.x.0,
            m.x.1,
            m.x_log,
            p.right - p.left,
            phone,
        ),
        ticks(
            &view.y.ticks,
            m.y.0,
            m.y.1,
            m.y_log,
            p.bottom - p.top,
            phone,
        ),
    )
}

/// Tick marks and labels where the axes are, then axis labels and the title.
/// `horizontal` and `vertical` are the CSS positions of the axis lines.
pub fn draw(
    b: &mut Builder,
    view: &View,
    f: &Frame,
    theme: &Theme,
    horizontal: Option<f64>,
    vertical: Option<f64>,
) {
    let m = &f.mapping;
    let p = m.plot;
    let phone = b.canvas().css_width < 480.0;
    let (xs, ys) = of_frame(view, f, phone);
    let line = hair(theme, "muted");
    let text = ink(theme, "fg");
    let cross = view.axes == "cross";
    // On crossing axes the origin is labelled by neither: both would sit on it.
    let skip_zero = |t: &Tick, other: Option<f64>| cross && other.is_some() && t.at == 0.0;

    if let Some(y) = horizontal {
        let mut shown = Vec::new();
        for t in &xs {
            let px = m.to_css([t.at, m.y.0])[0];
            let len = if t.label.is_some() { TICK } else { TICK / 2.0 };
            b.polyline(&[[px, y], [px, y + len]], false, &line, -800.0);
            if let Some(l) = &t.label {
                if !skip_zero(t, vertical) {
                    shown.push((px, l.clone(), Builder::measure(l, SIZE, None)[0]));
                }
            }
        }
        for (px, l, _) in thin(shown) {
            let h = Builder::measure(&l, SIZE, None)[1];
            let at = [px, y + TICK + 2.0 + h / 2.0];
            b.text(&l, at, SIZE, 0.0, None, &text, -790.0);
            push_rect(b, at, &l);
        }
    }
    if let Some(x) = vertical {
        let mut shown = Vec::new();
        for t in &ys {
            let py = m.to_css([m.x.0, t.at])[1];
            let len = if t.label.is_some() { TICK } else { TICK / 2.0 };
            b.polyline(&[[x - len, py], [x, py]], false, &line, -800.0);
            if let Some(l) = &t.label {
                if !skip_zero(t, horizontal) {
                    shown.push((py, l.clone(), Builder::measure(l, SIZE, None)[1]));
                }
            }
        }
        for (py, l, _) in thin(shown) {
            let w = Builder::measure(&l, SIZE, None)[0];
            let at = [x - TICK - 3.0 - w / 2.0, py];
            b.text(&l, at, SIZE, 0.0, None, &text, -790.0);
            push_rect(b, at, &l);
        }
    }

    if let Some(l) = &view.x.label {
        let h = Builder::measure(l, TITLE, None)[1];
        let at = [(p.left + p.right) / 2.0, f.cell.bottom - 2.0 - h / 2.0];
        b.text(l, at, TITLE, 0.0, None, &text, -790.0);
    }
    if let Some(l) = &view.y.label {
        let h = Builder::measure(l, TITLE, None)[1];
        let at = [f.cell.left + 2.0 + h / 2.0, (p.top + p.bottom) / 2.0];
        b.text(
            l,
            at,
            TITLE,
            std::f64::consts::FRAC_PI_2,
            None,
            &text,
            -790.0,
        );
    }
    if let Some(t) = &view.title {
        let label = format!("**{t}**");
        let h = Builder::measure(&label, TITLE, None)[1];
        let at = [(p.left + p.right) / 2.0, f.cell.top + 2.0 + h / 2.0];
        b.text(&label, at, TITLE, 0.0, None, &text, -790.0);
    }
}

/// Tick labels count as labels, so a mark's label steps around them.
fn push_rect(b: &mut Builder, at: [f64; 2], l: &str) {
    let [w, h] = Builder::measure(l, SIZE, None);
    b.labels.push(crate::geom::Rect {
        left: at[0] - w / 2.0,
        top: at[1] - h / 2.0,
        right: at[0] + w / 2.0,
        bottom: at[1] + h / 2.0,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(t: &[Tick]) -> Vec<String> {
        t.iter().filter_map(|t| t.label.clone()).collect()
    }

    #[test]
    fn auto_labels_use_the_fewest_digits() {
        let t = ticks(&Ticks::Auto, 0.0, 1.0, false, 200.0, false);
        assert_eq!(labels(&t), ["0.0", "0.5", "1.0"]);
        let t = ticks(
            &Ticks::At(vec![-1.0, 0.0, 1.0]),
            -2.0,
            2.0,
            false,
            400.0,
            false,
        );
        assert_eq!(labels(&t), ["−1", "0", "1"]);
        let t = ticks(&Ticks::Auto, -2.5, 2.5, false, 400.0, false);
        assert_eq!(labels(&t), ["−2", "−1", "0", "1", "2"]);
        let t = ticks(&Ticks::Auto, 0.0, 0.05, false, 500.0, false);
        assert_eq!(labels(&t)[1], "0.01");
    }

    #[test]
    fn steps_lists_and_labelled_ticks() {
        let t = ticks(&Ticks::Step(0.25), 0.0, 1.0, false, 400.0, false);
        assert_eq!(labels(&t), ["0.00", "0.25", "0.50", "0.75", "1.00"]);
        let t = ticks(
            &Ticks::At(vec![-5.0, 1.0, 3.0]),
            0.0,
            2.0,
            false,
            400.0,
            false,
        );
        assert_eq!(labels(&t), ["1"]);
        let t = ticks(
            &Ticks::Labelled(vec![(0.0, "low".into()), (1.0, "$H$".into())]),
            0.0,
            1.0,
            false,
            400.0,
            false,
        );
        assert_eq!(labels(&t), ["low", "$H$"]);
        assert!(ticks(&Ticks::None, 0.0, 1.0, false, 400.0, false).is_empty());
    }

    #[test]
    fn pi_ticks_are_reduced_fractions_in_tex() {
        assert_eq!(pi_label(2), r"$\frac{\pi}{2}$");
        assert_eq!(pi_label(4), r"$\pi$");
        assert_eq!(pi_label(6), r"$\frac{3\pi}{2}$");
        assert_eq!(pi_label(8), r"$2\pi$");
        assert_eq!(pi_label(-1), r"$-\frac{\pi}{4}$");
        let wide = ticks(&Ticks::Pi, 0.0, 6.3, false, 800.0, false);
        assert_eq!(wide.len(), 9, "quarters when there is room");
        let narrow = ticks(&Ticks::Pi, 0.0, 6.3, false, 300.0, false);
        assert_eq!(narrow.len(), 5, "halves when there is not");
        assert!(every_pi_label().contains(&r"$\frac{3\pi}{4}$".to_string()));
    }

    #[test]
    fn log_axes_have_decades_and_minors() {
        let t = ticks(&Ticks::Auto, 1.0, 1000.0, true, 400.0, false);
        assert_eq!(labels(&t), ["1", "10", "100", "1000"]);
        assert!(t.iter().any(|t| t.at == 20.0 && t.label.is_none()));
        assert_eq!(decade(1e6), "$10^{6}$");
        assert_eq!(decade(0.01), "0.01");
    }

    #[test]
    fn labels_that_would_overlap_are_thinned() {
        let crowded: Vec<_> = (0..20)
            .map(|i| (i as f64 * 10.0, i.to_string(), 30.0))
            .collect();
        let kept = thin(crowded);
        assert!(kept.windows(2).all(|w| w[1].0 - w[0].0 >= 30.0 + GAP));
        assert_eq!(kept[0].0, 0.0);
    }

    #[test]
    fn no_negative_zero() {
        assert_eq!(number(-0.0, 1), "0.0");
        assert_eq!(number(-0.00001, 2), "0.00");
        assert_eq!(number(-1.5, 1), "−1.5");
    }

    /// §7.4: tick labels never overlap, at any width.
    #[test]
    fn no_overlap_across_a_sweep_of_sizes() {
        use crate::document::read;
        use crate::layout::{draw_axes, frames};
        let doc = read("hapto: 2\nalt: a\nview: { x: { range: [-123.4, 9876.5] }, y: { range: [-0.0031, 0.0047] }, axes: box }\nmarks: [ { point: '[0, 0]' } ]").unwrap();
        let ranges = [((-123.4, 9876.5), (-0.0031, 0.0047))];
        for width in (240..=1400).step_by(37) {
            let w = width as f64;
            let h = (w * 9.0 / 16.0).max(200.0);
            let f = &frames(&doc.views, &doc.layout, &ranges, w, h)[0];
            let mut b = Builder::new(crate::scene::Canvas::new(w, h, 1.0), [1.0; 4]);
            draw_axes(&mut b, &doc.views[0], f, &Theme::light());
            let r = &b.labels;
            assert!(r.len() >= 3, "{width}: only {} labels", r.len());
            for (i, a) in r.iter().enumerate() {
                for c in &r[i + 1..] {
                    let x = a.right.min(c.right) - a.left.max(c.left);
                    let y = a.bottom.min(c.bottom) - a.top.max(c.top);
                    assert!(x <= 0.0 || y <= 0.0, "{width}: {a:?} overlaps {c:?}");
                }
            }
        }
    }
}
