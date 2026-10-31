//! A 2.0 figure end to end: read, compile, set values, rebuild, draw.

use hapto_expr::eval::{Cache, Env, Evaluation, DEFAULT_FUEL};
use hapto_expr::{Error, Value};
use kinemo_render::raster::Image;

use crate::codes;
use crate::document::marks::kind;
use crate::document::Dash;
use crate::geom::{clip_polygon, clip_polyline};
use crate::layout::{draw_axes, frames, Frame};
use crate::program::{Compiled, CompiledMark, CompiledStyle, Program};
use crate::scene::{rasterise, Builder, Canvas, Paint};
use crate::state::{Admitted, Input};
use crate::theme::Theme;

/// The most mark instances a figure draws after `repeat` (§19.3).
pub const MAX_INSTANCES: usize = 5000;

/// A figure: its program, its let cache, and nothing else that moves.
pub struct Figure {
    pub program: Program,
    cache: Cache,
}

impl Figure {
    pub fn from_text(text: &str) -> Result<Figure, Vec<Error>> {
        let doc = crate::document::read(text)?;
        let program = Program::new(doc)?;
        let cache = Cache::new(&program.module);
        Ok(Figure { program, cache })
    }

    /// Set a value through its domain, forgetting what it invalidates.
    pub fn set(&mut self, name: &str, input: Input) -> Result<Admitted, Error> {
        let admitted = self.program.state.set(name, input)?;
        if let Some(i) = self.program.state.index(name) {
            self.cache.invalidate(&self.program.module, &[i], &[]);
        }
        Ok(admitted)
    }

    pub fn reset(&mut self) {
        self.program.state.reset();
        self.cache.forget_all();
    }

    /// Rebuild the scene from the state and draw it.
    pub fn render(&mut self, canvas: Canvas, theme: &Theme) -> Result<Image, Error> {
        let scene = self.scene(canvas, theme)?;
        Ok(rasterise(&scene, canvas, false))
    }

    pub fn scene(&mut self, canvas: Canvas, theme: &Theme) -> Result<kinemo_ir::Scene, Error> {
        let palette = |name: &str| theme.colour(name);
        let values = self.program.state.values().to_vec();
        let env = Env {
            values: &values,
            compute: &[],
            palette: &palette,
        };
        let mut ev = Evaluation::new(&self.program.module, env, &mut self.cache, DEFAULT_FUEL);
        let mut b = Builder::new(canvas, theme.bg());
        let mut ranges = Vec::new();
        for (x, y) in &self.program.axes {
            ranges.push((range(&mut ev, &x.range)?, range(&mut ev, &y.range)?));
        }
        let doc = &self.program.doc;
        let placed = frames(
            &doc.views,
            &doc.layout,
            &ranges,
            canvas.css_width,
            canvas.css_height,
        );
        for (view, frame) in doc.views.iter().zip(&placed) {
            draw_axes(&mut b, view, frame, theme);
        }
        let mut drawn = 0usize;
        for (z, m) in self.program.marks.iter().enumerate() {
            let Some((view, frame)) = doc
                .views
                .iter()
                .zip(&placed)
                .find(|(v, _)| m.source.common.view.as_ref().is_none_or(|id| id == &v.id))
            else {
                continue;
            };
            let target = Target {
                frame,
                clip: view.clip,
            };
            draw(
                &mut b,
                &mut ev,
                m,
                &target,
                theme,
                z as f64,
                &[],
                &mut drawn,
            )?;
        }
        Ok(b.finish())
    }
}

fn eval(ev: &mut Evaluation, c: &Compiled, bound: &[Value]) -> Result<Value, Error> {
    ev.eval(&c.checked, bound).map_err(|e| e.at_path(&c.path))
}

/// An axis range at this state; one with no answer keeps a sane fallback.
fn range(ev: &mut Evaluation, r: &[Compiled; 2]) -> Result<(f64, f64), Error> {
    let (lo, hi) = (eval(ev, &r[0], &[])?.num(), eval(ev, &r[1], &[])?.num());
    Ok(if lo.is_finite() && hi.is_finite() && lo < hi {
        (lo, hi)
    } else {
        (0.0, 1.0)
    })
}

/// Where a mark draws: its view's frame, and whether it is clipped to it.
pub struct Target<'a> {
    pub frame: &'a Frame,
    pub clip: bool,
}

/// Resolve a style against the kind's defaults (§9.4) and the theme.
fn paint(
    ev: &mut Evaluation,
    s: &CompiledStyle,
    dash: &Option<Dash>,
    theme: &Theme,
    defaults: Paint,
    bound: &[Value],
) -> Result<Paint, Error> {
    let mut p = defaults;
    let colour = |v: Value| match v {
        Value::Colour(c) => Some(c),
        _ => None,
    };
    if let Some(c) = &s.stroke {
        p.stroke = colour(eval(ev, c, bound)?).or(p.stroke);
    }
    if let Some(c) = &s.fill {
        p.fill = colour(eval(ev, c, bound)?).or(p.fill);
        if p.fill_opacity == 0.0 {
            p.fill_opacity = 1.0;
        }
    }
    for (slot, target) in [
        (&s.width, &mut p.width),
        (&s.fill_opacity, &mut p.fill_opacity),
        (&s.opacity, &mut p.opacity),
    ] {
        if let Some(c) = slot {
            let n = eval(ev, c, bound)?.num();
            if n.is_finite() {
                *target = n.max(0.0);
            }
        }
    }
    p.dash = match dash {
        Some(Dash::Dashed) => Some(vec![6.0, 4.0]),
        Some(Dash::Dotted) => Some(vec![1.5, 3.0]),
        Some(Dash::Pattern(d)) => Some(d.clone()),
        Some(Dash::Solid) | None => None,
    };
    let _ = theme;
    Ok(p)
}

fn token(theme: &Theme, t: &str) -> Option<[f64; 4]> {
    theme.colour(t)
}

/// The product of a mark's repeat lists, as bound values per instance.
fn instances(
    ev: &mut Evaluation,
    m: &CompiledMark,
    outer: &[Value],
    drawn: &mut usize,
) -> Result<Vec<Vec<Value>>, Error> {
    let mut rows = vec![outer.to_vec()];
    for (_, c) in &m.repeat {
        let mut next = Vec::new();
        for row in &rows {
            let list = eval(ev, c, row)?;
            for item in list.items().unwrap_or(&[]) {
                let mut r = row.clone();
                r.push(item.clone());
                next.push(r);
            }
        }
        rows = next;
    }
    *drawn += rows.len();
    if *drawn > MAX_INSTANCES {
        return Err(Error::new(
            hapto_expr::error::codes::E604,
            format!("this figure draws more than {MAX_INSTANCES} marks"),
        )
        .hint("lower a repeat's count; a field mark draws many arrows as one")
        .at_path(&m.source.path));
    }
    Ok(rows)
}

#[allow(clippy::too_many_arguments)]
fn draw(
    b: &mut Builder,
    ev: &mut Evaluation,
    m: &CompiledMark,
    target: &Target,
    theme: &Theme,
    z: f64,
    outer: &[Value],
    drawn: &mut usize,
) -> Result<(), Error> {
    for bound in instances(ev, m, outer, drawn)? {
        if let Some(w) = &m.when {
            if eval(ev, w, &bound)?.bool() != Some(true) {
                continue;
            }
        }
        let z = m.source.common.z.unwrap_or(z);
        let map = &target.frame.mapping;
        let plot = map.plot;
        let main = |ev: &mut Evaluation| -> Result<Value, Error> {
            match m.main.formula() {
                Some(c) => eval(ev, c, &bound),
                None => Ok(Value::Nothing),
            }
        };
        let points = |v: &Value| -> Option<Vec<[f64; 2]>> {
            v.items()?
                .iter()
                .map(|p| p.vec().map(|q| map.to_css(q)))
                .collect()
        };
        let dash = &m.source.common.style.dash;
        let fg = token(theme, "fg");
        let accent = token(theme, "accent");
        match m.source.kind {
            "point" => {
                let Some(at) = main(ev)?.vec() else { continue };
                let r = match m.option("r").and_then(|s| s.formula()) {
                    Some(c) => eval(ev, c, &bound)?.num(),
                    None => 5.0,
                };
                let defaults = Paint {
                    fill: accent,
                    fill_opacity: 1.0,
                    stroke: Some(theme.bg()),
                    width: 1.5,
                    opacity: 1.0,
                    dash: None,
                };
                let p = paint(ev, &m.style, dash, theme, defaults, &bound)?;
                let at = map.to_css(at);
                if target.clip && !plot.grow(r).contains(at) {
                    continue;
                }
                b.dot(at, r, &p, z);
            }
            "segment" | "polyline" | "polygon" => {
                let Some(pts) = points(&main(ev)?) else {
                    continue;
                };
                if m.source.kind == "segment" && pts.len() != 2 {
                    continue;
                }
                let closed = m.source.kind == "polygon"
                    || match m.option("closed").and_then(|s| s.formula()) {
                        Some(c) => eval(ev, c, &bound)?.bool() == Some(true),
                        None => false,
                    };
                let defaults = if m.source.kind == "polygon" {
                    Paint {
                        fill: accent,
                        fill_opacity: 0.18,
                        stroke: None,
                        width: 2.0,
                        opacity: 1.0,
                        dash: None,
                    }
                } else {
                    Paint {
                        stroke: if m.source.kind == "segment" {
                            fg
                        } else {
                            accent
                        },
                        width: 2.0,
                        fill: None,
                        fill_opacity: 0.0,
                        opacity: 1.0,
                        dash: None,
                    }
                };
                let p = paint(ev, &m.style, dash, theme, defaults, &bound)?;
                if !target.clip {
                    b.polyline(&pts, closed, &p, z);
                } else if closed && p.fill.is_some() {
                    b.polyline(&clip_polygon(&pts, &plot), true, &p, z);
                } else {
                    let mut ring = pts.clone();
                    if closed {
                        ring.push(pts[0]);
                    }
                    for run in clip_polyline(&ring, &plot) {
                        b.polyline(&run, false, &p, z);
                    }
                }
            }
            other => {
                let lands = kind(other)
                    .map(|k| k.draws_in)
                    .unwrap_or("a later milestone");
                return Err(Error::new(
                    codes::E610,
                    format!("this build does not draw `{other}` marks yet"),
                )
                .hint(format!("they land in milestone {lands} of SPEC-2.0 §23"))
                .at_path(&m.source.path));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::ink;

    const ZIGZAG: &str = r#"
hapto: 2
alt: "A zigzag through x = {x}."
values:
  - { name: x, type: number, range: [0, 4], default: 2 }
view: { x: { range: [0, 4] }, y: { range: [0, 4] } }
marks:
  - { polyline: "[[0, 0], [1, 4], [2, 0], [3, 4], [4, 0]]", style: { width: heavy } }
  - { polygon: "[[0, 0], [x, 3], [4, 0]]" }
  - { point: "[x, 3]", r: 8 }
  - { segment: "[[0, 2], [4, 2]]" }
"#;

    #[test]
    fn a_default_render_has_ink_in_light_and_dark() {
        let mut f = Figure::from_text(ZIGZAG).unwrap_or_else(|e| panic!("{e:?}"));
        for theme in [Theme::light(), Theme::dark()] {
            let img = f.render(Canvas::new(720.0, 405.0, 2.0), &theme).unwrap();
            let painted = ink(&img, theme.bg());
            assert!(painted >= 0.005, "{}: ink {painted}", theme.name);
        }
    }

    #[test]
    fn setting_a_value_changes_the_drawing() {
        let mut f = Figure::from_text(ZIGZAG).unwrap();
        let canvas = Canvas::new(360.0, 203.0, 1.0);
        let before = f.render(canvas, &Theme::light()).unwrap();
        let admitted = f.set("x", Input::Number(9.0)).unwrap();
        assert!(admitted.clamped);
        let after = f.render(canvas, &Theme::light()).unwrap();
        assert_ne!(before.rgba, after.rgba);
        f.reset();
        assert_eq!(f.render(canvas, &Theme::light()).unwrap().rgba, before.rgba);
    }

    #[test]
    fn repeat_when_and_kinds_not_yet_drawn() {
        let doc = "hapto: 2\nalt: a\nvalues: [ { name: n, type: number, range: [0, 10], default: 3, step: integer } ]\nview: { x: { range: [0, 10] }, y: { range: [0, 1] } }\nmarks:\n  - { point: '[i, 0.5]', repeat: { i: 'range(0, n)' }, when: 'i != 1' }\n";
        let mut f = Figure::from_text(doc).unwrap();
        let s = f
            .scene(Canvas::new(200.0, 100.0, 1.0), &Theme::light())
            .unwrap();
        let dots = s.objects.iter().filter(|o| o.kind == "circle").count();
        assert_eq!(dots, 2, "i = 0 and 2; 1 is filtered by when");
        let doc = "hapto: 2\nalt: a\nview: { x: { range: [0, 1] }, y: { range: [0, 1] } }\nmarks: [ { heatmap: 'x * y' } ]";
        let e = Figure::from_text(doc)
            .unwrap()
            .render(Canvas::new(100.0, 50.0, 1.0), &Theme::light())
            .unwrap_err();
        assert_eq!(e.code, codes::E610);
        assert!(e.hint.unwrap().contains("M5"));
    }

    #[test]
    fn too_many_instances_are_refused() {
        let doc = "hapto: 2\nalt: a\nview: { x: { range: [0, 1] }, y: { range: [0, 1] } }\nmarks: [ { point: '[0, 0]', repeat: { i: 'range(0, 80)', j: 'range(0, 80)' } } ]";
        let e = Figure::from_text(doc)
            .unwrap()
            .render(Canvas::new(100.0, 50.0, 1.0), &Theme::light())
            .unwrap_err();
        assert_eq!(e.code.0, "E604");
    }
}
