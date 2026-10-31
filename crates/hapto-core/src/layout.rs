//! Where each view sits and how its data maps to pixels (SPEC-2.0 §7).

use crate::document::{Layout as GridLayout, Scale, Size, View};
use crate::geom::Rect;
use crate::scene::{Builder, Paint};
use crate::theme::Theme;

/// The figure's height for a width, by the size rules of §7.6.
pub fn figure_height(size: &Size, css_width: f64) -> f64 {
    let aspect = match size.phone_aspect {
        Some(a) if css_width < 480.0 => a,
        _ => size.aspect,
    };
    (css_width / aspect).clamp(size.min_height, size.max_height)
}

/// Data coordinates onto a plot area in CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mapping {
    pub x: (f64, f64),
    pub y: (f64, f64),
    pub x_log: bool,
    pub y_log: bool,
    pub plot: Rect,
}

fn axis_t(v: f64, (lo, hi): (f64, f64), log: bool) -> f64 {
    if log {
        (v.ln() - lo.ln()) / (hi.ln() - lo.ln())
    } else {
        (v - lo) / (hi - lo)
    }
}

impl Mapping {
    pub fn to_css(&self, p: [f64; 2]) -> [f64; 2] {
        let tx = axis_t(p[0], self.x, self.x_log);
        let ty = axis_t(p[1], self.y, self.y_log);
        let r = &self.plot;
        [
            r.left + tx * (r.right - r.left),
            r.bottom - ty * (r.bottom - r.top),
        ]
    }

    /// The inverse: a CSS-pixel position in data coordinates (for drags).
    pub fn to_data(&self, p: [f64; 2]) -> [f64; 2] {
        let r = &self.plot;
        let tx = (p[0] - r.left) / (r.right - r.left);
        let ty = (r.bottom - p[1]) / (r.bottom - r.top);
        let back = |t: f64, (lo, hi): (f64, f64), log: bool| {
            if log {
                (lo.ln() + t * (hi.ln() - lo.ln())).exp()
            } else {
                lo + t * (hi - lo)
            }
        };
        [back(tx, self.x, self.x_log), back(ty, self.y, self.y_log)]
    }

    /// CSS pixels per data unit along x and y (linear axes).
    pub fn scale(&self) -> (f64, f64) {
        (
            (self.plot.right - self.plot.left) / (self.x.1 - self.x.0),
            (self.plot.bottom - self.plot.top) / (self.y.1 - self.y.0),
        )
    }
}

/// One view placed on the canvas.
#[derive(Debug, Clone)]
pub struct Frame {
    pub id: String,
    pub cell: Rect,
    pub mapping: Mapping,
}

/// The cell of a view in the layout grid.
fn cell(grid: &GridLayout, area: [usize; 4], width: f64, height: f64) -> Rect {
    let cols = grid.columns.max(area[0] + area[2]);
    let rows = grid.rows.max(area[1] + area[3]);
    let weights = |given: &[f64], n: usize| -> Vec<f64> {
        (0..n)
            .map(|i| given.get(i).copied().unwrap_or(1.0).max(0.0))
            .collect()
    };
    let (wx, wy) = (weights(&grid.widths, cols), weights(&grid.heights, rows));
    let gap = if cols > 1 || rows > 1 { grid.gap } else { 0.0 };
    let span = |w: &[f64], total: f64, from: usize, n: usize| {
        let avail = total - gap * (w.len().saturating_sub(1)) as f64;
        let sum: f64 = w.iter().sum::<f64>().max(1e-9);
        let start: f64 = w[..from].iter().sum::<f64>() / sum * avail + gap * from as f64;
        let len: f64 = w[from..from + n].iter().sum::<f64>() / sum * avail + gap * (n - 1) as f64;
        (start, start + len)
    };
    let (l, r) = span(&wx, width, area[0], area[2]);
    let (t, b) = span(&wy, height, area[1], area[3]);
    Rect {
        left: l,
        top: t,
        right: r,
        bottom: b,
    }
}

/// Room around the plot for tick labels and axis titles (§7.2).
fn padding(view: &View) -> [f64; 4] {
    if let Some(p) = view.padding {
        return [p; 4];
    }
    match view.axes.as_str() {
        "none" => [8.0; 4],
        _ => {
            let label_x = view.x.label.is_some() as u8 as f64 * 18.0;
            let label_y = view.y.label.is_some() as u8 as f64 * 18.0;
            let title = view.title.is_some() as u8 as f64 * 22.0;
            // left, top, right, bottom
            [40.0 + label_y, 14.0 + title, 18.0, 30.0 + label_x]
        }
    }
}

/// Widen the axis with more pixels per unit until both match (§7.5).
fn equalise(x: (f64, f64), y: (f64, f64), plot: &Rect) -> ((f64, f64), (f64, f64)) {
    let (w, h) = (plot.right - plot.left, plot.bottom - plot.top);
    let (ux, uy) = (w / (x.1 - x.0), h / (y.1 - y.0));
    if ux > uy {
        let half = w / uy / 2.0;
        let c = (x.0 + x.1) / 2.0;
        ((c - half, c + half), y)
    } else {
        let half = h / ux / 2.0;
        let c = (y.0 + y.1) / 2.0;
        (x, (c - half, c + half))
    }
}

/// An axis range in data units, low to high.
pub type Span = (f64, f64);

/// Place every view: cell, padding, ranges (already evaluated), aspect.
pub fn frames(
    views: &[View],
    grid: &GridLayout,
    ranges: &[(Span, Span)],
    width: f64,
    height: f64,
) -> Vec<Frame> {
    views
        .iter()
        .zip(ranges)
        .map(|(v, &(x, y))| {
            let c = cell(grid, v.area, width, height);
            let [pl, pt, pr, pb] = padding(v);
            let plot = Rect {
                left: c.left + pl,
                top: c.top + pt,
                right: (c.right - pr).max(c.left + pl + 1.0),
                bottom: (c.bottom - pb).max(c.top + pt + 1.0),
            };
            let (x_log, y_log) = (v.x.scale == Scale::Log, v.y.scale == Scale::Log);
            let (x, y) = if (v.equal_aspect || v.polar) && !x_log && !y_log {
                equalise(x, y, &plot)
            } else {
                (x, y)
            };
            Frame {
                id: v.id.clone(),
                cell: c,
                mapping: Mapping {
                    x,
                    y,
                    x_log,
                    y_log,
                    plot,
                },
            }
        })
        .collect()
}

/// "Nice" tick positions: steps of 1, 2 or 5 × 10^k about `spacing` px apart.
pub fn nice_ticks(lo: f64, hi: f64, length_px: f64, spacing: f64) -> Vec<f64> {
    // NaN compares false either way: written so that NaN returns nothing.
    if lo.is_nan() || hi.is_nan() || lo >= hi || length_px <= 0.0 {
        return vec![];
    }
    let target = (hi - lo) / (length_px / spacing).max(1.0);
    let mag = 10f64.powf(target.log10().floor());
    let step = [1.0, 2.0, 5.0, 10.0]
        .iter()
        .map(|m| m * mag)
        .find(|s| *s >= target)
        .unwrap_or(10.0 * mag);
    let first = (lo / step).ceil() as i64;
    let last = (hi / step).floor() as i64;
    (first..=last)
        .map(|i| i as f64 * step)
        .map(|v| if v.abs() < step * 1e-9 { 0.0 } else { v })
        .collect()
}

/// Decades for a log axis.
pub fn log_ticks(lo: f64, hi: f64) -> Vec<f64> {
    if lo.is_nan() || hi.is_nan() || lo <= 0.0 || lo >= hi {
        return vec![];
    }
    let (a, b) = (lo.log10().ceil() as i32, hi.log10().floor() as i32);
    (a..=b).map(|k| 10f64.powi(k)).collect()
}

fn hair(theme: &Theme, token: &str) -> Paint {
    Paint {
        stroke: theme.colour(token),
        width: 1.0,
        fill: None,
        fill_opacity: 0.0,
        opacity: 1.0,
        dash: None,
    }
}

/// Grid and axis lines of one view, beneath the marks.
pub fn draw_axes(b: &mut Builder, view: &View, f: &Frame, theme: &Theme) {
    let m = &f.mapping;
    let p = m.plot;
    let ticks = |lo: f64, hi: f64, log: bool, len: f64| {
        if log {
            log_ticks(lo, hi)
        } else {
            nice_ticks(lo, hi, len, 80.0)
        }
    };
    if view.grid != "none" {
        let paint = hair(theme, "faint");
        for x in ticks(m.x.0, m.x.1, m.x_log, p.right - p.left) {
            let px = m.to_css([x, m.y.0])[0];
            b.polyline(&[[px, p.top], [px, p.bottom]], false, &paint, -900.0);
        }
        for y in ticks(m.y.0, m.y.1, m.y_log, p.bottom - p.top) {
            let py = m.to_css([m.x.0, y])[1];
            b.polyline(&[[p.left, py], [p.right, py]], false, &paint, -900.0);
        }
    }
    let paint = hair(theme, "muted");
    // Zero, pinned to the nearest edge when out of view (1.0 behaviour).
    let zero_x = m.to_css([
        if m.x_log {
            m.x.0
        } else {
            0.0f64.clamp(m.x.0, m.x.1)
        },
        m.y.0,
    ])[0];
    let zero_y = m.to_css([
        m.x.0,
        if m.y_log {
            m.y.0
        } else {
            0.0f64.clamp(m.y.0, m.y.1)
        },
    ])[1];
    let (horizontal, vertical) = match view.axes.as_str() {
        "cross" => (Some(zero_y), Some(zero_x)),
        "left-bottom" => (Some(p.bottom), Some(p.left)),
        "x" => (Some(zero_y), None),
        "y" => (None, Some(zero_x)),
        _ => (None, None),
    };
    if view.axes == "box" {
        b.polyline(
            &[
                [p.left, p.top],
                [p.right, p.top],
                [p.right, p.bottom],
                [p.left, p.bottom],
            ],
            true,
            &paint,
            -800.0,
        );
    }
    if let Some(y) = horizontal {
        b.polyline(&[[p.left, y], [p.right, y]], false, &paint, -800.0);
    }
    if let Some(x) = vertical {
        b.polyline(&[[x, p.top], [x, p.bottom]], false, &paint, -800.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::read;

    fn view(yaml_view: &str) -> View {
        let doc = read(&format!(
            "hapto: 2\nalt: a\nview: {yaml_view}\nmarks: [ {{ point: '[0, 0]' }} ]"
        ))
        .unwrap();
        doc.views[0].clone()
    }

    #[test]
    fn size_rules() {
        let s = Size::default();
        assert!((figure_height(&s, 720.0) - 405.0).abs() < 1e-9);
        assert_eq!(figure_height(&s, 200.0), 240.0);
        let phone = Size {
            phone_aspect: Some(4.0 / 3.0),
            ..Size::default()
        };
        assert!((figure_height(&phone, 360.0) - 270.0).abs() < 1e-9);
    }

    /// A circle stays a circle: with equal aspect, a unit is the same number
    /// of pixels on both axes at every size.
    #[test]
    fn equal_aspect_at_three_sizes() {
        let v = view("{ x: { range: [-1, 1] }, y: { range: [-1, 1] }, aspect: equal }");
        for (w, h) in [(720.0, 405.0), (360.0, 360.0), (300.0, 600.0)] {
            let f = &frames(
                std::slice::from_ref(&v),
                &GridLayout::default(),
                &[((-1.0, 1.0), (-1.0, 1.0))],
                w,
                h,
            )[0];
            let (sx, sy) = f.mapping.scale();
            assert!((sx - sy).abs() < 1e-9, "{w}x{h}: {sx} vs {sy}");
            // The requested range is a minimum: both ends still visible.
            assert!(f.mapping.x.0 <= -1.0 && f.mapping.y.0 <= -1.0);
        }
    }

    #[test]
    fn mapping_round_trips_linear_and_log() {
        let v = view("{ x: { range: [1, 1000], scale: log }, y: { range: [-2, 2] } }");
        let f = &frames(
            &[v],
            &GridLayout::default(),
            &[((1.0, 1000.0), (-2.0, 2.0))],
            720.0,
            405.0,
        )[0];
        let m = f.mapping;
        let mid = m.to_css([10f64.powf(1.5), 0.0]);
        assert!((mid[0] - (m.plot.left + m.plot.right) / 2.0).abs() < 1e-9);
        let back = m.to_data(m.to_css([42.0, 1.25]));
        assert!((back[0] - 42.0).abs() < 1e-9 && (back[1] - 1.25).abs() < 1e-12);
    }

    #[test]
    fn two_views_share_the_grid() {
        let doc = read("hapto: 2\nalt: a\nlayout: { columns: 2, gap: 20 }\nviews:\n  - { id: a, x: { range: [0, 1] }, y: { range: [0, 1] }, area: [0, 0] }\n  - { id: b, x: { range: [0, 1] }, y: { range: [0, 1] }, area: [1, 0] }\nmarks: [ { point: '[0, 0]', view: a } ]").unwrap();
        let r = [((0.0, 1.0), (0.0, 1.0)); 2];
        let fs = frames(&doc.views, &doc.layout, &r, 820.0, 400.0);
        assert_eq!(fs[0].cell.right, 400.0);
        assert_eq!(fs[1].cell.left, 420.0);
    }

    #[test]
    fn nice_ticks_are_round() {
        assert_eq!(
            nice_ticks(0.0, 10.0, 800.0, 80.0),
            (0..=10).map(f64::from).collect::<Vec<_>>()
        );
        assert_eq!(
            nice_ticks(-2.5, 2.5, 400.0, 80.0),
            vec![-2.0, -1.0, 0.0, 1.0, 2.0]
        );
        assert_eq!(nice_ticks(0.0, 1.0, 200.0, 80.0), vec![0.0, 0.5, 1.0]);
        assert_eq!(log_ticks(1.0, 1000.0), vec![1.0, 10.0, 100.0, 1000.0]);
    }
}
