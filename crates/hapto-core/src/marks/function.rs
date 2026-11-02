//! Function plots (SPEC-2.0 §8.6): adaptive, broken where there is no
//! value or a jump through infinity, clipped exactly at the plot.

use hapto_expr::{Error, Value};

use super::Ctx;
use crate::program::{Compiled, Slot};

/// Samples to start from when `samples: auto`.
const START: usize = 128;
/// The most points one plot keeps.
const MAX_POINTS: usize = 4096;
/// Subdivide while the midpoint is further than this from the chord, px.
const TOLERANCE: f64 = 0.5;
const MAX_DEPTH: u32 = 10;

pub fn draws(kind: &str) -> bool {
    kind == "function"
}

/// One sample: the parameter and where it lands in CSS px (none: no value).
type Sample = (f64, Option<[f64; 2]>);

struct Plot<'c, 'a, 'e> {
    c: &'c mut Ctx<'a, 'e>,
    x: Option<&'c Compiled>,
    y: &'c Compiled,
    count: usize,
}

impl Plot<'_, '_, '_> {
    fn at(&mut self, t: f64) -> Result<Sample, Error> {
        self.count += 1;
        let mut bound = self.c.bound.to_vec();
        bound.push(Value::Num(t));
        let y = self
            .c
            .ev
            .eval(&self.y.checked, &bound)
            .map_err(|e| e.at_path(&self.y.path))?
            .num();
        let x = match self.x {
            Some(xc) => self
                .c
                .ev
                .eval(&xc.checked, &bound)
                .map_err(|e| e.at_path(&xc.path))?
                .num(),
            None => t,
        };
        Ok((
            t,
            (x.is_finite() && y.is_finite()).then(|| self.c.css([x, y])),
        ))
    }

    /// Fill in between two samples until the curve is smooth to half a pixel.
    fn refine(
        &mut self,
        a: Sample,
        b: Sample,
        depth: u32,
        out: &mut Vec<Sample>,
    ) -> Result<(), Error> {
        if depth >= MAX_DEPTH || self.count >= MAX_POINTS {
            out.push(b);
            return Ok(());
        }
        let m = self.at((a.0 + b.0) / 2.0)?;
        let smooth = match (a.1, m.1, b.1) {
            (Some(p), Some(q), Some(r)) => {
                let chord = [(p[0] + r[0]) / 2.0, (p[1] + r[1]) / 2.0];
                (q[0] - chord[0]).hypot(q[1] - chord[1]) <= TOLERANCE
            }
            // Both ends and the middle missing: nothing to refine.
            (None, None, None) => true,
            _ => false,
        };
        if smooth {
            out.push(b);
            return Ok(());
        }
        self.refine(a, m, depth + 1, out)?;
        self.refine(m, b, depth + 1, out)
    }
}

pub fn draw(c: &mut Ctx) -> Result<(), Error> {
    let m = c.m;
    let (x, y) = match &m.main {
        Slot::Formula(f) => (None, f),
        Slot::Map(_) => match (
            m.main.get("x").and_then(Slot::formula),
            m.main.get("y").and_then(Slot::formula),
        ) {
            (Some(x), Some(y)) => (Some(x), y),
            _ => return Ok(()),
        },
        _ => return Ok(()),
    };
    let domain = match c.opt("domain")?.and_then(|d| {
        d.items()
            .map(|i| (i.first().map(Value::num), i.get(1).map(Value::num)))
    }) {
        Some((Some(a), Some(b))) if a.is_finite() && b.is_finite() && a < b => (a, b),
        _ if x.is_some() => (0.0, std::f64::consts::TAU),
        _ => c.map.x,
    };
    let start = match c.opt("samples")? {
        Some(v) if v.num().is_finite() && v.num() >= 2.0 => (v.num() as usize).min(MAX_POINTS),
        _ => START,
    };
    let height = c.map.plot.bottom - c.map.plot.top;
    let plot_rect = c.map.plot;
    let mut plot = Plot { c, x, y, count: 0 };
    let mut samples = vec![plot.at(domain.0)?];
    for i in 1..start {
        let next = plot.at(domain.0 + (domain.1 - domain.0) * i as f64 / (start - 1) as f64)?;
        let prev = *samples.last().unwrap_or(&next);
        plot.refine(prev, next, 0, &mut samples)?;
    }

    // Break at no value, and across a jump through infinity: two samples
    // more than the plot's height apart whose midpoint is outside the plot.
    let mut runs: Vec<Vec<[f64; 2]>> = vec![vec![]];
    for w in samples.windows(2) {
        let (a, b) = (w[0], w[1]);
        match (a.1, b.1) {
            (Some(p), Some(q)) => {
                let jump = (q[1] - p[1]).abs() > height && {
                    let mid = plot.at((a.0 + b.0) / 2.0)?;
                    mid.1.is_none_or(|m| !plot_rect.grow(height).contains(m))
                };
                let run = runs.last_mut().expect("there is always a run");
                if jump {
                    runs.push(vec![]);
                    continue;
                }
                if run.is_empty() {
                    run.push(p);
                }
                run.push(q);
            }
            _ => runs.push(vec![]),
        }
    }
    for run in runs.iter().filter(|r| r.len() > 1) {
        plot.c.stroke(run, false);
    }
    // The label goes at the last point the reader can see (§10.3).
    let inside = plot_rect.grow(0.5);
    plot.c.anchor = runs
        .iter()
        .flatten()
        .rev()
        .find(|p| inside.contains(**p))
        .copied();
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::render::Figure;
    use crate::scene::Canvas;
    use crate::theme::Theme;

    fn scene(marks: &str, view: &str) -> kinemo_ir::Scene {
        let doc = format!("hapto: 2\nalt: a\nview: {view}\nmarks: [ {marks} ]");
        let mut f = Figure::from_text(&doc).unwrap_or_else(|e| panic!("{e:?}"));
        f.scene(Canvas::new(400.0, 200.0, 1.0), &Theme::light())
            .unwrap()
    }

    fn runs(s: &kinemo_ir::Scene) -> usize {
        s.objects.iter().filter(|o| o.kind == "polyline").count()
    }

    #[test]
    fn a_smooth_curve_is_one_run() {
        let s = scene(
            "{ function: 'x**2' }",
            "{ x: { range: [-2, 2] }, y: { range: [-1, 5] }, axes: none }",
        );
        assert_eq!(runs(&s), 1);
    }

    #[test]
    fn tan_has_no_vertical_joins() {
        // Three branches of tan in view, never joined across an asymptote.
        let s = scene(
            "{ function: 'tan(x)' }",
            "{ x: { range: [-4, 4] }, y: { range: [-3, 3] }, axes: none }",
        );
        assert_eq!(runs(&s), 3, "one run per branch");
    }

    #[test]
    fn one_over_x_breaks_at_zero() {
        let s = scene(
            "{ function: '1 / x' }",
            "{ x: { range: [-2, 2] }, y: { range: [-5, 5] }, axes: none }",
        );
        assert_eq!(runs(&s), 2);
    }

    #[test]
    fn sampling_is_bounded() {
        // A wild function cannot make more than the point limit.
        let s = scene(
            "{ function: 'sin(1 / x)' }",
            "{ x: { range: [-1, 1] }, y: { range: [-1, 1] }, axes: none }",
        );
        let points: usize = s
            .signals
            .iter()
            .filter_map(|sig| match &sig.initial {
                kinemo_ir::Value::List(l) => Some(l.len()),
                _ => None,
            })
            .sum();
        assert!(points <= super::MAX_POINTS + 64, "{points} points");
    }

    #[test]
    fn parametric_and_domain() {
        let s = scene(
            "{ function: { x: 'cos(t)', y: 'sin(t)' }, var: t }",
            "{ x: { range: [-2, 2] }, y: { range: [-2, 2] }, aspect: equal, axes: none }",
        );
        assert_eq!(runs(&s), 1);
        let s = scene(
            "{ function: 'x', domain: [0, 1] }",
            "{ x: { range: [-2, 2] }, y: { range: [-2, 2] }, axes: none }",
        );
        assert_eq!(runs(&s), 1);
    }
}
