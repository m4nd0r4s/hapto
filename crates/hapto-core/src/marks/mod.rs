//! Drawing marks: one function per family, sharing a context (SPEC-2.0 §8).

mod curves;
mod function;
mod lines;
pub mod text;

use hapto_expr::eval::Evaluation;
use hapto_expr::{Error, Value};

use crate::codes;
use crate::document::marks::kind;
use crate::geom::{clip_polygon, clip_polyline};
use crate::layout::Mapping;
use crate::program::{Compiled, CompiledMark, Slot};
use crate::scene::{Builder, Paint};

/// Everything drawing one mark instance needs.
pub struct Ctx<'a, 'e> {
    pub b: &'a mut Builder,
    pub ev: &'a mut Evaluation<'e>,
    pub m: &'a CompiledMark,
    pub map: &'a Mapping,
    pub clip: bool,
    pub z: f64,
    pub bound: &'a [Value],
    pub paint: Paint,
    /// Where the label attaches, when drawing the mark found it (a function's
    /// last visible point); otherwise each kind's own rule (§10.3).
    pub anchor: Option<[f64; 2]>,
}

impl Ctx<'_, '_> {
    pub fn eval(&mut self, c: &Compiled) -> Result<Value, Error> {
        self.ev
            .eval(&c.checked, self.bound)
            .map_err(|e| e.at_path(&c.path))
    }

    pub fn main(&mut self) -> Result<Value, Error> {
        match self.m.main.formula() {
            Some(c) => self.eval(c),
            None => Ok(Value::Nothing),
        }
    }

    /// The main argument's map entry `key`, evaluated.
    pub fn main_key(&mut self, key: &str) -> Result<Option<Value>, Error> {
        match self.m.main.get(key).and_then(Slot::formula) {
            Some(c) => Ok(Some(self.eval(c)?)),
            None => Ok(None),
        }
    }

    pub fn opt(&mut self, key: &str) -> Result<Option<Value>, Error> {
        match self.m.option(key).and_then(Slot::formula) {
            Some(c) => Ok(Some(self.eval(c)?)),
            None => Ok(None),
        }
    }

    pub fn opt_num(&mut self, key: &str, default: f64) -> Result<f64, Error> {
        Ok(self
            .opt(key)?
            .map(|v| v.num())
            .filter(|n| n.is_finite())
            .unwrap_or(default))
    }

    pub fn opt_bool(&mut self, key: &str) -> Result<bool, Error> {
        Ok(self.opt(key)?.and_then(|v| v.bool()).unwrap_or(false))
    }

    pub fn word(&self, key: &str, default: &'static str) -> String {
        match self.m.option(key) {
            Some(Slot::Word(w)) => w.clone(),
            _ => default.to_string(),
        }
    }

    /// A theme colour by token, for marks that colour by meaning.
    pub fn palette(&self, token: &str) -> Option<[f64; 4]> {
        use hapto_expr::builtins::Host;
        self.ev.palette(token)
    }

    pub fn css(&self, p: [f64; 2]) -> [f64; 2] {
        self.map.to_css(p)
    }

    /// Stroke a CSS-pixel polyline, clipped to the plot when asked.
    pub fn stroke(&mut self, pts: &[[f64; 2]], closed: bool) {
        let paint = Paint {
            fill: None,
            ..self.paint.clone()
        };
        if !self.clip {
            self.b.polyline(pts, closed, &paint, self.z);
            return;
        }
        let mut ring = pts.to_vec();
        if closed && !pts.is_empty() {
            ring.push(pts[0]);
        }
        for run in clip_polyline(&ring, &self.map.plot) {
            self.b.polyline(&run, false, &paint, self.z);
        }
    }

    /// Fill (and stroke, if the paint has one) a CSS-pixel polygon.
    pub fn fill(&mut self, pts: &[[f64; 2]]) {
        let pts = if self.clip {
            clip_polygon(pts, &self.map.plot)
        } else {
            pts.to_vec()
        };
        self.b.polyline(&pts, true, &self.paint, self.z);
    }

    /// A filled arrowhead `size` px long, its tip at `tip`, pointing away
    /// from `from`.
    pub fn arrowhead(&mut self, tip: [f64; 2], from: [f64; 2], size: f64) {
        let (dx, dy) = (tip[0] - from[0], tip[1] - from[1]);
        let len = dx.hypot(dy);
        if len < 1e-9 || (self.clip && !self.map.plot.grow(1.0).contains(tip)) {
            return;
        }
        let (ux, uy) = (dx / len, dy / len);
        let back = [tip[0] - ux * size, tip[1] - uy * size];
        let half = size * 0.42;
        let head = [
            tip,
            [back[0] - uy * half, back[1] + ux * half],
            [back[0] + uy * half, back[1] - ux * half],
        ];
        let colour = self.paint.stroke.or(self.paint.fill);
        let paint = Paint {
            fill: colour,
            fill_opacity: 1.0,
            stroke: None,
            dash: None,
            ..self.paint.clone()
        };
        self.b.polyline(&head, true, &paint, self.z);
    }

    /// Two points far beyond the view along the line through `p` with
    /// direction `d`, in data units: an infinite line, for clipping.
    pub fn far(&self, p: [f64; 2], d: [f64; 2]) -> Option<([f64; 2], [f64; 2])> {
        let n = d[0].hypot(d[1]);
        if n.is_nan() || n <= 0.0 || !p[0].is_finite() || !p[1].is_finite() {
            return None;
        }
        let span = (self.map.x.1 - self.map.x.0).abs() + (self.map.y.1 - self.map.y.0).abs();
        let k = 4.0 * span / n;
        Some((
            [p[0] - d[0] * k, p[1] - d[1] * k],
            [p[0] + d[0] * k, p[1] + d[1] * k],
        ))
    }
}

/// Draw one instance of a mark, then its label.
pub fn draw(ctx: &mut Ctx) -> Result<(), Error> {
    draw_mark(ctx)?;
    text::label(ctx)
}

fn draw_mark(ctx: &mut Ctx) -> Result<(), Error> {
    if text::draws(ctx.m.source.kind) {
        return text::draw(ctx);
    }
    if lines::draws(ctx.m.source.kind) {
        return lines::draw(ctx);
    }
    if function::draws(ctx.m.source.kind) {
        return function::draw(ctx);
    }
    if curves::draws(ctx.m.source.kind) {
        return curves::draw(ctx);
    }
    let name = ctx.m.source.kind;
    let lands = kind(name)
        .map(|k| k.draws_in)
        .unwrap_or("a later milestone");
    Err(Error::new(
        codes::E610,
        format!("this build does not draw `{name}` marks yet"),
    )
    .hint(format!("they land in milestone {lands} of SPEC-2.0 §23"))
    .at_path(&ctx.m.source.path))
}
