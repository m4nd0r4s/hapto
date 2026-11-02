//! A 2.0 figure end to end: read, compile, set values, rebuild, draw.

use hapto_expr::eval::{Cache, Env, Evaluation, DEFAULT_FUEL};
use hapto_expr::{Error, Value};
use kinemo_render::raster::Image;

use crate::layout::{draw_axes, frames, Frame};
use crate::program::{Compiled, CompiledMark, Program};
use crate::scene::{rasterise, Builder, Canvas};
use crate::state::{Admitted, Input};
use crate::theme::Theme;
use crate::{marks, style};

/// The most mark instances a figure draws after `repeat` (§19.3).
pub const MAX_INSTANCES: usize = 5000;

/// A figure: its program, its let cache, and nothing else that moves.
pub struct Figure {
    pub program: Program,
    cache: Cache,
    /// Findings of the last render, such as too many labels (W602).
    pub warnings: Vec<Error>,
}

impl Figure {
    pub fn from_text(text: &str) -> Result<Figure, Vec<Error>> {
        let doc = crate::document::read(text)?;
        if let Some(json) = &doc.baked {
            crate::maths::register(json).map_err(|e| vec![e])?;
        }
        let program = Program::new(doc)?;
        let cache = Cache::new(&program.module);
        Ok(Figure {
            program,
            cache,
            warnings: Vec::new(),
        })
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
            let in_view = |other: &CompiledMark| {
                other
                    .source
                    .common
                    .view
                    .as_ref()
                    .is_none_or(|id| id == &view.id)
            };
            let series = self.program.marks[..z]
                .iter()
                .filter(|o| in_view(o) && style::is_series(o.source.kind))
                .count();
            let focus = self.program.state.focus.as_str();
            let base = focus.split('[').next().unwrap_or("");
            let view_has_focus = !focus.is_empty()
                && self.program.marks.iter().filter(|o| in_view(o)).any(|o| {
                    let c = &o.source.common;
                    c.name.as_deref() == Some(base) || c.focus_group.as_deref() == Some(focus)
                });
            let target = Target {
                frame,
                clip: view.clip,
                series,
                focus,
                view_has_focus,
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
        self.warnings.clear();
        if b.labels.len() > 60 {
            self.warnings.push(
                Error::warning(
                    crate::codes::W602,
                    format!("this figure shows {} labels at once", b.labels.len()),
                )
                .hint("each label costs time on every frame; show fewer, or label on focus"),
            );
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
    /// Curve-like marks before this one in its view (§9.4 colour rotation).
    pub series: usize,
    pub focus: &'a str,
    /// Some mark of this view is focused, so the others fade (§12.5).
    pub view_has_focus: bool,
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
    let repeated = !m.repeat.is_empty();
    for (idx, bound) in instances(ev, m, outer, drawn)?.into_iter().enumerate() {
        if let Some(w) = &m.when {
            if eval(ev, w, &bound)?.bool() != Some(true) {
                continue;
            }
        }
        let index = repeated.then_some(idx);
        let paint = style::resolve(
            ev,
            m,
            theme,
            target.series,
            target.focus,
            index,
            target.view_has_focus,
            &bound,
        )?;
        let mut ctx = marks::Ctx {
            b: &mut *b,
            ev: &mut *ev,
            m,
            map: &target.frame.mapping,
            clip: target.clip,
            z: m.source.common.z.unwrap_or(z),
            bound: &bound,
            paint,
            anchor: None,
        };
        marks::draw(&mut ctx)?;
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
        assert_eq!(e.code, crate::codes::E610);
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

    /// The thickness lesson: a stroke width that is a formula follows its value.
    #[test]
    fn a_width_formula_follows_its_value() {
        let doc = "hapto: 2\nalt: a\nvalues: [ { name: k, type: number, range: [0, 1], default: 0 } ]\nview: { x: { range: [0, 1] }, y: { range: [0, 1] }, axes: none }\nmarks: [ { segment: '[[0.1, 0.5], [0.9, 0.5]]', style: { width: '1 + 12 * k' } } ]";
        let mut f = Figure::from_text(doc).unwrap();
        let c = Canvas::new(300.0, 150.0, 1.0);
        let thin = ink(&f.render(c, &Theme::light()).unwrap(), Theme::light().bg());
        f.set("k", Input::Number(1.0)).unwrap();
        let thick = ink(&f.render(c, &Theme::light()).unwrap(), Theme::light().bg());
        assert!(thick > thin * 5.0, "thin {thin}, thick {thick}");
    }

    #[test]
    fn focus_highlights_one_mark_and_fades_the_rest() {
        let doc = "hapto: 2\nalt: a\nview: { x: { range: [0, 1] }, y: { range: [0, 1] }, axes: none }\nmarks:\n  - { segment: '[[0.1, 0.3], [0.9, 0.3]]', name: a }\n  - { segment: '[[0.1, 0.7], [0.9, 0.7]]', name: b }";
        let mut f = Figure::from_text(doc).unwrap();
        let c = Canvas::new(200.0, 100.0, 1.0);
        let theme = Theme::light();
        let px = |img: &kinemo_render::raster::Image, x: u32, y: u32| {
            let i = ((y * img.width + x) * 4) as usize;
            [img.rgba[i], img.rgba[i + 1], img.rgba[i + 2]]
        };
        let plain = f.render(c, &theme).unwrap();
        f.program.state.focus = "a".into();
        let focused = f.render(c, &theme).unwrap();
        // Find the rows the two segments sit on: the darkest pixel per half.
        let row_of = |img: &kinemo_render::raster::Image, top: u32, bottom: u32| {
            (top..bottom)
                .min_by_key(|y| px(img, 100, *y).iter().map(|v| *v as u32).sum::<u32>())
                .unwrap()
        };
        let (ya, yb) = (row_of(&plain, 50, 100), row_of(&plain, 0, 50));
        let hl = theme
            .colour("highlight")
            .unwrap()
            .map(|v| (v * 255.0).round() as i32);
        let got = px(&focused, 100, ya);
        assert!(
            (0..3).all(|i| (got[i] as i32 - hl[i]).abs() < 40),
            "focused mark is the highlight colour: {got:?}"
        );
        let faded = px(&focused, 100, yb).iter().map(|v| *v as u32).sum::<u32>();
        let solid = px(&plain, 100, yb).iter().map(|v| *v as u32).sum::<u32>();
        assert!(
            faded > solid,
            "the unfocused mark fades toward the background"
        );
    }
}
