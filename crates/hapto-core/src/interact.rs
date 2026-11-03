//! Touching a figure: which mark is under the pointer, and what a drag sets
//! (SPEC-2.0 §12.4).
//!
//! Hit geometry is hapto's own evaluated geometry, never pixels: each
//! draggable or focusable mark has an anchor — a point, the tip of an arrow
//! or vector, the centre of a circle, sector or box — placed in CSS pixels by
//! its view. A drag evaluates the mark's `drag` formulas with the pointer in
//! the view's data coordinates and admits what they give as one set.

use hapto_expr::eval::{Env, Evaluation, DEFAULT_FUEL};
use hapto_expr::{Error, Value};

use crate::program::{CompiledMark, Slot};
use crate::render::{eval, instances, place, Figure};
use crate::scene::Canvas;
use crate::state::{Admitted, Input};
use crate::theme::Theme;

/// What is pointing: the hit radius depends on it (§12.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pointer {
    Mouse,
    Touch,
}

impl Pointer {
    /// `mouse`, `pen` and anything else aim like a mouse; `touch` is coarse.
    pub fn named(kind: &str) -> Pointer {
        if kind == "touch" {
            Pointer::Touch
        } else {
            Pointer::Mouse
        }
    }
}

/// A mark instance the pointer can take hold of or focus.
#[derive(Debug, Clone, PartialEq)]
pub struct Handle {
    /// The mark's `name`, else its JSON Pointer; `[i]` for a repeat instance.
    pub id: String,
    pub css: [f64; 2],
    /// The drawn radius in CSS pixels, for the hit radius.
    pub r: f64,
    pub z: f64,
    pub drag: bool,
    pub focusable: bool,
    /// Where the anchor is in its view's data coordinates.
    data: [f64; 2],
    mark: usize,
    view: usize,
    bound: Vec<Value>,
    /// A vector's origin, which its drag formulas read as `at`.
    at: Option<[f64; 2]>,
}

/// What `hit_test` found.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub id: String,
    pub drag: bool,
    pub focusable: bool,
}

/// A drag in progress: the handle as it was grabbed, and where.
#[derive(Debug, Clone)]
pub struct Grab {
    handle: Handle,
    start: [f64; 2],
}

/// An anchor in data coordinates, its drawn radius in CSS pixels, and a
/// vector's origin.
type Anchor = ([f64; 2], f64, Option<[f64; 2]>);

/// The anchor of a mark instance; `None` for marks without one.
fn anchor(ev: &mut Evaluation, m: &CompiledMark, bound: &[Value]) -> Result<Option<Anchor>, Error> {
    let main = match m.main.formula() {
        Some(c) => eval(ev, c, bound)?,
        None => return Ok(None),
    };
    let option = |ev: &mut Evaluation, key: &str| -> Result<Option<Value>, Error> {
        match m.option(key).and_then(Slot::formula) {
            Some(c) => Ok(Some(eval(ev, c, bound)?)),
            None => Ok(None),
        }
    };
    let finite = |p: [f64; 2]| p[0].is_finite() && p[1].is_finite();
    let found = match m.source.kind {
        "point" => {
            let r = option(ev, "r")?
                .map(|v| v.num())
                .filter(|r| r.is_finite())
                .unwrap_or(5.0);
            main.vec().map(|p| (p, r, None))
        }
        "circle" | "sector" | "box" => main.vec().map(|p| (p, 0.0, None)),
        "arrow" => main
            .items()
            .and_then(|items| items.last())
            .and_then(Value::vec)
            .map(|p| (p, 0.0, None)),
        "vector" => {
            let at = option(ev, "at")?
                .and_then(|v| v.vec())
                .unwrap_or([0.0, 0.0]);
            main.vec()
                .map(|v| ([at[0] + v[0], at[1] + v[1]], 0.0, Some(at)))
        }
        _ => None,
    };
    Ok(found.filter(|(p, _, _)| finite(*p)))
}

impl Figure {
    /// Every anchor a pointer can take hold of or focus, at this state.
    ///
    /// Marks inside groups are not handles yet: their pointer would have to
    /// go back through the group's transform.
    pub fn handles(&mut self, canvas: Canvas, theme: &Theme) -> Result<Vec<Handle>, Error> {
        let palette = |name: &str| theme.colour(name);
        let values = self.program.state.values().to_vec();
        let env = Env {
            values: &values,
            compute: &[],
            palette: &palette,
        };
        let mut ev = Evaluation::new(&self.program.module, env, &mut self.cache, DEFAULT_FUEL);
        let placed = place(&mut ev, &self.program, canvas)?;
        let doc = &self.program.doc;
        let mut out = Vec::new();
        let mut drawn = 0usize;
        for (z, m) in self.program.marks.iter().enumerate() {
            let common = &m.source.common;
            let drag = !m.drag.is_empty();
            let focusable = common.name.is_some() || common.focus_group.is_some();
            if !(drag || focusable) {
                continue;
            }
            let Some(view) = doc
                .views
                .iter()
                .position(|v| common.view.as_ref().is_none_or(|id| id == &v.id))
            else {
                continue;
            };
            let Some(frame) = placed.get(view) else {
                continue;
            };
            let base = common.name.clone().unwrap_or_else(|| m.source.path.clone());
            let repeated = !m.repeat.is_empty();
            for (i, bound) in instances(&mut ev, m, &[], &mut drawn)?
                .into_iter()
                .enumerate()
            {
                if let Some(w) = &m.when {
                    if eval(&mut ev, w, &bound)?.bool() != Some(true) {
                        continue;
                    }
                }
                let Some((data, r, at)) = anchor(&mut ev, m, &bound)? else {
                    continue;
                };
                out.push(Handle {
                    id: if repeated {
                        format!("{base}[{i}]")
                    } else {
                        base.clone()
                    },
                    css: frame.mapping.to_css(data),
                    r,
                    z: common.z.unwrap_or(z as f64),
                    drag,
                    focusable,
                    data,
                    mark: z,
                    view,
                    bound,
                    at,
                });
            }
        }
        Ok(out)
    }

    /// The topmost handle within reach of a pointer at `css`, if any:
    /// `max(r + 6, 12)` CSS px for a mouse, 22 for touch (§12.4).
    pub fn hit_test(
        &mut self,
        canvas: Canvas,
        theme: &Theme,
        css: [f64; 2],
        pointer: Pointer,
    ) -> Result<Option<Hit>, Error> {
        let handles = self.handles(canvas, theme)?;
        Ok(nearest_on_top(&handles, css, pointer).map(|h| Hit {
            id: h.id.clone(),
            drag: h.drag,
            focusable: h.focusable,
        }))
    }

    /// Take hold of the draggable handle `id`, with the pointer at `css`.
    pub fn grab(
        &mut self,
        canvas: Canvas,
        theme: &Theme,
        id: &str,
        css: [f64; 2],
    ) -> Result<Grab, Error> {
        let handles = self.handles(canvas, theme)?;
        let draggable: Vec<&str> = handles
            .iter()
            .filter(|h| h.drag)
            .map(|h| h.id.as_str())
            .collect();
        let Some(handle) = handles.iter().find(|h| h.drag && h.id == id) else {
            let err = Error::new(
                crate::codes::E505,
                format!("this figure has no draggable mark \"{id}\""),
            );
            return Err(match crate::tree::nearest(id, &draggable) {
                Some(n) => err.hint(format!("did you mean `{n}`?")),
                None => err.options(draggable.iter().copied()),
            });
        };
        let placed = self.frames(canvas, theme)?;
        let start = placed[handle.view].mapping.to_data(css);
        Ok(Grab {
            handle: handle.clone(),
            start,
        })
    }

    /// Move a grabbed handle's pointer to `css`: evaluate its drag formulas
    /// and admit what they give, all as one set. A formula with no answer
    /// here (the pointer off a curve's domain) leaves its value alone.
    pub fn drag_to(
        &mut self,
        canvas: Canvas,
        theme: &Theme,
        grab: &Grab,
        css: [f64; 2],
    ) -> Result<Vec<(String, Admitted)>, Error> {
        let placed = self.frames(canvas, theme)?;
        let h = &grab.handle;
        let pointer = placed[h.view].mapping.to_data(css);
        let delta = [pointer[0] - grab.start[0], pointer[1] - grab.start[1]];
        let mut bound = h.bound.clone();
        bound.extend([
            Value::Vec(pointer),
            Value::Vec(grab.start),
            Value::Vec(h.data),
            Value::Vec(delta),
        ]);
        if let Some(at) = h.at {
            bound.push(Value::Vec(at));
        }
        let mut inputs = Vec::new();
        {
            let palette = |name: &str| theme.colour(name);
            let values = self.program.state.values().to_vec();
            let env = Env {
                values: &values,
                compute: &[],
                palette: &palette,
            };
            let mut ev = Evaluation::new(&self.program.module, env, &mut self.cache, DEFAULT_FUEL);
            let m = &self.program.marks[h.mark];
            for (i, c) in &m.drag {
                let input = match eval(&mut ev, c, &bound)? {
                    Value::Num(n) if n.is_finite() => Input::Number(n),
                    Value::Vec(p) if p[0].is_finite() && p[1].is_finite() => Input::Point(p),
                    Value::Bool(b) => Input::Flag(b),
                    Value::Str(s) => Input::Choice(s.to_string()),
                    _ => continue,
                };
                inputs.push((self.program.state.decls()[*i].name.clone(), input));
            }
        }
        self.set_all(inputs)
    }

    /// Admit every input first, then apply them: one set, or none (§12.4).
    pub fn set_all(
        &mut self,
        inputs: Vec<(String, Input)>,
    ) -> Result<Vec<(String, Admitted)>, Error> {
        for (name, input) in &inputs {
            let Some(i) = self.program.state.index(name) else {
                // Let `set` say what is wrong, with its suggestion.
                return Err(self.set(name, input.clone()).unwrap_err());
            };
            crate::state::admit(&self.program.state.decls()[i], input.clone())?;
        }
        inputs
            .into_iter()
            .map(|(name, input)| Ok((name.clone(), self.set(&name, input)?)))
            .collect()
    }

    fn frames(
        &mut self,
        canvas: Canvas,
        theme: &Theme,
    ) -> Result<Vec<crate::layout::Frame>, Error> {
        let palette = |name: &str| theme.colour(name);
        let values = self.program.state.values().to_vec();
        let env = Env {
            values: &values,
            compute: &[],
            palette: &palette,
        };
        let mut ev = Evaluation::new(&self.program.module, env, &mut self.cache, DEFAULT_FUEL);
        place(&mut ev, &self.program, canvas)
    }
}

fn nearest_on_top(handles: &[Handle], css: [f64; 2], pointer: Pointer) -> Option<&Handle> {
    handles
        .iter()
        .filter(|h| {
            let reach = match pointer {
                Pointer::Mouse => (h.r + 6.0).max(12.0),
                Pointer::Touch => 22.0,
            };
            (h.css[0] - css[0]).hypot(h.css[1] - css[1]) <= reach
        })
        // Topmost z wins; among equals, the one drawn last.
        .max_by(|a, b| a.z.total_cmp(&b.z).then(a.mark.cmp(&b.mark)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "hapto: 2\nalt: a\nvalues:\n  - { name: x, type: number, range: [-2, 2], default: 1 }\n  - { name: p, type: point, range: [[-2, -2], [2, 2]], default: [0, 0] }\nlet:\n  'f(t)': 't**2'\nview: { x: { range: [-2, 2] }, y: { range: [-2, 2] }, axes: none }\nmarks:\n  - { function: 'f(x)', var: x }\n  - { point: '[x, f(x)]', drag: { x: 'pointer.x' }, name: P }\n  - { point: 'p' }\n  - { point: '[-1, -1]', name: fixed }\n";

    fn fig() -> (Figure, Canvas, Theme) {
        let f = Figure::from_text(DOC).unwrap_or_else(|e| panic!("{e:?}"));
        (f, Canvas::new(400.0, 400.0, 1.0), Theme::light())
    }

    fn at(f: &mut Figure, c: Canvas, t: &Theme, id: &str) -> [f64; 2] {
        f.handles(c, t)
            .unwrap()
            .into_iter()
            .find(|h| h.id == id)
            .unwrap()
            .css
    }

    /// Where a data point is on the canvas, through the first view.
    fn css(f: &mut Figure, c: Canvas, t: &Theme, p: [f64; 2]) -> [f64; 2] {
        f.frames(c, t).unwrap()[0].mapping.to_css(p)
    }

    #[test]
    fn handles_are_the_draggable_and_named_anchors() {
        let (mut f, c, t) = fig();
        let hs = f.handles(c, &t).unwrap();
        let ids: Vec<(&str, bool, bool)> = hs
            .iter()
            .map(|h| (h.id.as_str(), h.drag, h.focusable))
            .collect();
        assert_eq!(
            ids,
            vec![
                ("P", true, true),
                ("/marks/2", true, false),
                ("fixed", false, true)
            ]
        );
    }

    #[test]
    fn hits_reach_further_for_touch() {
        let (mut f, c, t) = fig();
        let p = at(&mut f, c, &t, "P");
        let near = [p[0] + 14.0, p[1]];
        assert_eq!(f.hit_test(c, &t, near, Pointer::Mouse).unwrap(), None);
        let hit = f.hit_test(c, &t, near, Pointer::Touch).unwrap().unwrap();
        assert_eq!((hit.id.as_str(), hit.drag), ("P", true));
        assert!(f
            .hit_test(c, &t, [p[0] + 30.0, p[1]], Pointer::Touch)
            .unwrap()
            .is_none());
    }

    #[test]
    fn a_point_on_a_curve_stays_on_the_curve() {
        let (mut f, c, t) = fig();
        let p = at(&mut f, c, &t, "P");
        let g = f.grab(c, &t, "P", p).unwrap();
        // Pull up and to the left: only x follows, so the point slides along.
        let to = css(&mut f, c, &t, [0.5, 1.7]);
        let set = f.drag_to(c, &t, &g, to).unwrap();
        assert_eq!(set.len(), 1);
        assert_eq!(set[0].0, "x");
        let x = set[0].1.applied.num();
        assert!((x - 0.5).abs() < 1e-9, "x = {x}");
        let q = at(&mut f, c, &t, "P");
        let data = f.frames(c, &t).unwrap()[0].mapping.to_data(q);
        assert!((data[1] - data[0] * data[0]).abs() < 1e-9);
    }

    #[test]
    fn a_point_value_drags_relatively_and_is_clamped() {
        let (mut f, c, t) = fig();
        // Grab off-centre: no jump on grab, the offset is kept.
        let from = css(&mut f, c, &t, [0.25, 0.0]);
        let g = f.grab(c, &t, "/marks/2", from).unwrap();
        let to = css(&mut f, c, &t, [0.75, 0.5]);
        let set = f.drag_to(c, &t, &g, to).unwrap();
        let Value::Vec(q) = set[0].1.applied else {
            panic!()
        };
        assert!(
            (q[0] - 0.5).abs() < 1e-9 && (q[1] - 0.5).abs() < 1e-9,
            "{q:?}"
        );
        let set = f.drag_to(c, &t, &g, [from[0] + 5000.0, from[1]]).unwrap();
        assert_eq!(set[0].1.applied.vec().map(|v| v[0]), Some(2.0));
        assert!(set[0].1.clamped);
    }

    #[test]
    fn drag_false_turns_the_automatic_drag_off() {
        let doc = DOC.replace("{ point: 'p' }", "{ point: 'p', drag: false }");
        let mut f = Figure::from_text(&doc).unwrap();
        let hs = f
            .handles(Canvas::new(400.0, 400.0, 1.0), &Theme::light())
            .unwrap();
        assert!(hs.iter().all(|h| h.id != "/marks/2"));
    }

    #[test]
    fn only_draggable_marks_can_be_grabbed() {
        let (mut f, c, t) = fig();
        let e = f.grab(c, &t, "fixed", [0.0, 0.0]).unwrap_err();
        assert_eq!(e.code.0, "E505");
    }

    #[test]
    fn a_vector_tip_drags_its_vector() {
        let doc = "hapto: 2\nalt: a\nvalues:\n  - { name: v, type: point, range: [[-3, -3], [3, 3]], default: [1, 0] }\nview: { x: { range: [-2, 2] }, y: { range: [-2, 2] }, axes: none }\nmarks:\n  - { vector: 'v', at: '[1, 1]' }\n";
        let mut f = Figure::from_text(doc).unwrap();
        let (c, t) = (Canvas::new(400.0, 400.0, 1.0), Theme::light());
        let h = f.handles(c, &t).unwrap().remove(0);
        let g = f.grab(c, &t, &h.id, h.css).unwrap();
        let to = css(&mut f, c, &t, [2.0, 2.0]);
        let set = f.drag_to(c, &t, &g, to).unwrap();
        let Value::Vec(v) = set[0].1.applied else {
            panic!()
        };
        // The tip went to [2, 2] from the origin [1, 1].
        assert!(
            (v[0] - 1.0).abs() < 1e-9 && (v[1] - 1.0).abs() < 1e-9,
            "{v:?}"
        );
    }

    #[test]
    fn a_set_of_several_values_is_all_or_nothing() {
        let (mut f, ..) = fig();
        let e = f
            .set_all(vec![
                ("x".into(), Input::Number(0.5)),
                ("p".into(), Input::Number(1.0)),
            ])
            .unwrap_err();
        assert_eq!(e.code.0, "E403");
        assert_eq!(f.program.state.get("x"), Some(&Value::Num(1.0)));
    }
}
