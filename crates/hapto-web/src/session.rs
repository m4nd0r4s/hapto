//! The 2.0 surface (SPEC-2.0 §15.10) as plain Rust: JSON in, JSON out.
//!
//! The worker talks to [`crate::Figure`], which is this with `wasm-bindgen`
//! around it. Keeping the logic here lets the native test suite hold every
//! call to its contract; the Node tests then only check the binding.

use hapto_core::document::{Kind, Scale, Step};
use hapto_core::interact::{Grab, Pointer};
use hapto_core::render::Figure as Core;
use hapto_core::scene::Canvas;
use hapto_core::state::{control_step, Admitted, Input};
use hapto_core::theme::Theme;
use hapto_expr::{Error, Value};
use serde_json::{json, Map, Value as Json};

/// A loaded figure, the size it draws at, and a drag in progress.
pub struct Session {
    figure: Core,
    canvas: Canvas,
    grab: Option<Grab>,
}

/// Errors as the page receives them: the first, with all of them listed.
pub fn errors_json(errors: &[Error]) -> Json {
    let all: Vec<Json> = errors
        .iter()
        .map(|e| serde_json::to_value(e).unwrap_or(Json::Null))
        .collect();
    let mut first = all.first().cloned().unwrap_or_else(|| json!({}));
    if let Json::Object(o) = &mut first {
        o.insert("errors".into(), Json::Array(all));
    }
    first
}

fn error(code: hapto_expr::error::Code, message: impl Into<String>) -> Json {
    errors_json(&[Error::new(code, message.into())])
}

/// A formula value as JSON. No value is `null`, as is a number that is not
/// finite: JSON has no NaN, and the page shows both as "—".
pub fn value_json(v: &Value) -> Json {
    let num = |n: f64| {
        if n.is_finite() {
            json!(n)
        } else {
            Json::Null
        }
    };
    match v {
        Value::Num(n) => num(*n),
        Value::Bool(b) => json!(b),
        Value::Str(s) => json!(s.as_ref()),
        Value::Vec(p) => json!([num(p[0]), num(p[1])]),
        Value::Colour(c) => json!(c.map(num)),
        Value::List(items) => Json::Array(items.iter().map(value_json).collect()),
        Value::Mat(m) => Json::Array(
            (0..m.rows)
                .map(|r| Json::Array(m.row(r).iter().map(|n| num(*n)).collect()))
                .collect(),
        ),
        Value::Record(_) | Value::Nothing => Json::Null,
    }
}

fn admitted_json(set: Vec<(String, Admitted)>) -> Json {
    let mut out = Map::new();
    for (name, a) in set {
        out.insert(
            name,
            json!({ "value": value_json(&a.applied), "clamped": a.clamped }),
        );
    }
    Json::Object(out)
}

/// A value's domain, control and current value, for the page's controls.
fn decl_json(d: &hapto_core::document::ValueDecl, v: &Value) -> Json {
    let mut o = Map::new();
    o.insert("name".into(), json!(d.name));
    o.insert("type".into(), json!(d.kind.type_name()));
    o.insert("label".into(), json!(d.label));
    o.insert("value".into(), value_json(v));
    o.insert("shared".into(), json!(d.shared));
    match &d.kind {
        Kind::Number {
            low,
            high,
            default,
            step,
            scale,
            unit,
            format,
        } => {
            o.insert("range".into(), json!([low, high]));
            o.insert("default".into(), json!(default));
            o.insert("step".into(), json!(control_step(&d.kind)));
            o.insert("integer".into(), json!(*step == Step::Integer));
            let scale = match scale {
                Scale::Linear => "linear",
                Scale::Log => "log",
            };
            o.insert("scale".into(), json!(scale));
            o.insert("unit".into(), json!(unit));
            o.insert("format".into(), json!(format));
        }
        Kind::Choice {
            options,
            labels,
            default,
        } => {
            o.insert("options".into(), json!(options));
            let labels: Map<String, Json> =
                labels.iter().map(|(k, v)| (k.clone(), json!(v))).collect();
            o.insert("labels".into(), Json::Object(labels));
            o.insert("default".into(), json!(default));
        }
        Kind::Point { low, high, default } => {
            o.insert("range".into(), json!([low, high]));
            o.insert("default".into(), json!(default));
        }
        Kind::Flag { default } => {
            o.insert("default".into(), json!(default));
        }
    }
    let control = d.control.as_ref().map_or(
        Json::Null,
        |c| json!({ "type": c.kind, "marks": c.marks, "mark_labels": c.mark_labels }),
    );
    o.insert("control".into(), control);
    let play = d.play.as_ref().map_or(
        Json::Null,
        |p| json!({ "duration": p.duration, "loop": p.looping }),
    );
    o.insert("play".into(), play);
    Json::Object(o)
}

/// What a JSON value asks a figure value to become.
fn input(name: &str, v: &Json) -> Result<Input, Json> {
    Ok(match v {
        Json::Number(n) => Input::Number(n.as_f64().unwrap_or(f64::NAN)),
        Json::String(s) => Input::Choice(s.clone()),
        Json::Bool(b) => Input::Flag(*b),
        Json::Array(p) if p.len() == 2 && p.iter().all(Json::is_number) => Input::Point([
            p[0].as_f64().unwrap_or(f64::NAN),
            p[1].as_f64().unwrap_or(f64::NAN),
        ]),
        other => {
            return Err(error(
                hapto_core::codes::E403,
                format!("\"{name}\" cannot be set to {other}"),
            ))
        }
    })
}

impl Session {
    /// Read and compile a document. YAML and JSON are told apart by the
    /// reader, so `format` is taken for the page's sake and not needed.
    pub fn load(text: &str, _format: &str) -> Result<Session, Json> {
        let figure = Core::from_text(text).map_err(|e| errors_json(&e))?;
        let width = 720.0;
        let height = hapto_core::layout::figure_height(&figure.program.doc.size, width);
        Ok(Session {
            figure,
            canvas: Canvas::new(width, height, 1.0),
            grab: None,
        })
    }

    fn theme(&self) -> Theme {
        if self.figure.program.state.theme == "dark" {
            Theme::dark()
        } else {
            Theme::light()
        }
    }

    /// The height a box of this CSS width reserves (§15.3: no layout shift).
    pub fn height_for(&self, css_width: f64) -> f64 {
        hapto_core::layout::figure_height(&self.figure.program.doc.size, css_width)
    }

    /// Every value with its domain, control and current value, in
    /// declaration order, and the reserved `focus` and `theme`.
    pub fn values(&self) -> Json {
        let state = &self.figure.program.state;
        let values: Vec<Json> = state
            .decls()
            .iter()
            .zip(state.values())
            .map(|(d, v)| decl_json(d, v))
            .collect();
        json!({
            "values": values,
            "focus": state.focus,
            "theme": state.theme,
            "at_default": state.is_default(),
        })
    }

    /// Set values from `{ name: value, … }`: all admitted, or none set.
    /// `focus` and `theme` are the reserved state (§5.5).
    pub fn set_state(&mut self, text: &str) -> Result<Json, Json> {
        let tree: Json = serde_json::from_str(text).map_err(|e| {
            error(
                hapto_core::codes::E400,
                format!("setState takes a JSON object: {e}"),
            )
        })?;
        let Json::Object(entries) = tree else {
            return Err(error(
                hapto_core::codes::E400,
                "setState takes a JSON object of value names",
            ));
        };
        let mut inputs = Vec::new();
        let (mut focus, mut theme) = (None, None);
        for (name, v) in &entries {
            match name.as_str() {
                "focus" => match v {
                    Json::String(s) => focus = Some(s.clone()),
                    _ => {
                        return Err(error(
                            hapto_core::codes::E403,
                            "focus is a mark's name, or \"\"",
                        ))
                    }
                },
                "theme" => match v.as_str() {
                    Some(t @ ("light" | "dark")) => theme = Some(t.to_string()),
                    _ => {
                        return Err(errors_json(&[Error::new(
                            hapto_core::codes::E403,
                            "theme is \"light\" or \"dark\"",
                        )
                        .options(["light", "dark"])]))
                    }
                },
                _ => inputs.push((name.clone(), input(name, v)?)),
            }
        }
        let set = self.figure.set_all(inputs).map_err(|e| errors_json(&[e]))?;
        let mut out = admitted_json(set);
        let o = out.as_object_mut().expect("an object");
        if let Some(f) = focus {
            o.insert("focus".into(), json!(f));
            self.figure.program.state.focus = f;
        }
        if let Some(t) = theme {
            o.insert("theme".into(), json!(t));
            self.figure.program.state.theme = t;
        }
        Ok(out)
    }

    /// Back to every default, and no focus (§12.7).
    pub fn reset(&mut self) {
        self.figure.reset();
        self.grab = None;
    }

    /// Draw at `css_w`×`css_h` CSS pixels from now on, at `dpr` device
    /// pixels per CSS pixel. The page caps `dpr` (§15.8); this does not.
    pub fn resize(&mut self, css_w: f64, css_h: f64, dpr: f64) {
        let at_least = |n: f64, low: f64| if n.is_finite() && n >= low { n } else { low };
        self.canvas = Canvas::new(
            at_least(css_w, 1.0),
            at_least(css_h, 1.0),
            at_least(dpr, 0.25),
        );
    }

    /// The backing store's size in device pixels.
    pub fn pixels(&self) -> (u32, u32) {
        self.canvas.pixels()
    }

    /// Readouts, the lets prose reads, and the alt text, at this state.
    pub fn evaluate(&mut self, lets: &[String]) -> Result<Json, Json> {
        let theme = self.theme();
        let fail = |e: Error| errors_json(&[e]);
        let readouts: Vec<Json> = self
            .figure
            .readouts(&theme)
            .map_err(fail)?
            .into_iter()
            .map(|s| json!({ "id": s.id, "label": s.label, "text": s.text, "place": s.place }))
            .collect();
        let mut values = Map::new();
        for name in lets {
            let v = self.figure.let_value(name, &theme).map_err(fail)?;
            values.insert(name.clone(), value_json(&v));
        }
        let alt = self.figure.alt(&theme).map_err(fail)?;
        Ok(json!({ "readouts": readouts, "lets": values, "alt": alt }))
    }

    /// The figure at this state, as RGBA bytes row by row.
    pub fn render(&mut self) -> Result<Vec<u8>, Json> {
        let theme = self.theme();
        self.figure
            .render(self.canvas, &theme)
            .map(|image| image.rgba)
            .map_err(|e| errors_json(&[e]))
    }

    /// What loading found (W801 and the like) and what the last render
    /// found (W602), for development mode.
    pub fn warnings(&self) -> Json {
        Json::Array(
            self.figure
                .program
                .warnings
                .iter()
                .chain(&self.figure.warnings)
                .map(|e| serde_json::to_value(e).unwrap_or(Json::Null))
                .collect(),
        )
    }

    /// The heat-map cell grid drawn under the canvas (§15.8). Heat maps draw
    /// from M5; until then no figure has an underlay.
    pub fn underlay(&self) -> Json {
        Json::Null
    }

    pub fn render_svg(&mut self) -> Result<String, Json> {
        let theme = self.theme();
        self.figure
            .svg(self.canvas, &theme)
            .map_err(|e| errors_json(&[e]))
    }

    /// The mark under a pointer: `{ mark, drag, focusable }`, or `null`.
    pub fn hit_test(&mut self, css_x: f64, css_y: f64, pointer: &str) -> Result<Json, Json> {
        let theme = self.theme();
        let hit = self
            .figure
            .hit_test(self.canvas, &theme, [css_x, css_y], Pointer::named(pointer))
            .map_err(|e| errors_json(&[e]))?;
        Ok(hit.map_or(
            Json::Null,
            |h| json!({ "mark": h.id, "drag": h.drag, "focusable": h.focusable }),
        ))
    }

    /// A drag's `start`, `move` and `end`. `start` takes hold of `mark`;
    /// `move` returns the values it set, as `setState` does; `end` lets go.
    pub fn drag(&mut self, mark: &str, phase: &str, css_x: f64, css_y: f64) -> Result<Json, Json> {
        let theme = self.theme();
        let fail = |e: Error| errors_json(&[e]);
        match phase {
            "start" => {
                let g = self
                    .figure
                    .grab(self.canvas, &theme, mark, [css_x, css_y])
                    .map_err(fail)?;
                self.grab = Some(g);
                Ok(json!({}))
            }
            "move" => {
                let Some(g) = &self.grab else {
                    return Err(error(
                        hapto_core::codes::E400,
                        "a drag moves only after it starts",
                    ));
                };
                let set = self
                    .figure
                    .drag_to(self.canvas, &theme, g, [css_x, css_y])
                    .map_err(fail)?;
                Ok(admitted_json(set))
            }
            "end" | "cancel" => {
                self.grab = None;
                Ok(json!({}))
            }
            other => Err(errors_json(&[Error::new(
                hapto_core::codes::E400,
                format!("\"{other}\" is not a drag phase"),
            )
            .options(["start", "move", "end", "cancel"])])),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"hapto: 2
alt: "A parabola with a point at x = {x:.1f}."
values:
  - { name: x, type: number, range: [-2, 2], default: 1, step: 0.5, unit: m }
  - { name: mode, type: choice, options: [a, b], default: a }
let:
  'f(t)': 't**2'
  slope: '2*x'
view: { x: { range: [-2, 2] }, y: { range: [-1, 4] } }
marks:
  - { function: 'f(x)', var: x }
  - { point: '[x, f(x)]', drag: { x: 'pointer.x' }, name: P }
readouts:
  - { id: s, label: slope, value: slope, format: .2f }
"#;

    fn session() -> Session {
        let mut s = Session::load(DOC, "yaml").unwrap_or_else(|e| panic!("{e}"));
        s.resize(400.0, 300.0, 2.0);
        s
    }

    #[test]
    fn a_bad_document_is_every_error_with_its_code() {
        let e = Session::load(
            "hapto: 2\nalt: a\nview: { x: { range: [0, 1] }, y: { range: [0, 1] } }\nmarks: [ { point: '[q, 0]' } ]",
            "yaml",
        )
            .err()
            .unwrap();
        assert_eq!(e["code"], "E301");
        assert!(e["errors"].as_array().is_some_and(|a| !a.is_empty()));
    }

    #[test]
    fn values_describe_their_domains() {
        let v = session().values();
        assert_eq!(v["values"][0]["name"], "x");
        assert_eq!(v["values"][0]["range"], json!([-2.0, 2.0]));
        assert_eq!(v["values"][0]["step"], json!(0.5));
        assert_eq!(v["values"][0]["unit"], "m");
        assert_eq!(v["values"][1]["options"], json!(["a", "b"]));
        assert_eq!(v["at_default"], true);
    }

    #[test]
    fn set_state_admits_and_reports_clamping() {
        let mut s = session();
        let out = s.set_state(r#"{ "x": 7, "mode": "b" }"#).unwrap();
        assert_eq!(out["x"], json!({ "value": 2.0, "clamped": true }));
        assert_eq!(out["mode"]["value"], "b");
        assert_eq!(s.values()["at_default"], false);
        // One bad entry and nothing is set.
        let e = s.set_state(r#"{ "x": 0, "mode": "c" }"#).unwrap_err();
        assert!(e["code"].as_str().is_some());
        assert_eq!(s.values()["values"][0]["value"], json!(2.0));
        let e = s.set_state(r#"{ "nope": 1 }"#).unwrap_err();
        assert_eq!(e["code"], "E301");
        s.reset();
        assert_eq!(s.values()["at_default"], true);
    }

    #[test]
    fn evaluate_gives_readouts_lets_and_alt() {
        let mut s = session();
        s.set_state(r#"{ "x": 1.5 }"#).unwrap();
        let out = s.evaluate(&["slope".into()]).unwrap();
        assert_eq!(out["readouts"][0]["text"], "3.00");
        assert_eq!(out["lets"]["slope"], json!(3.0));
        assert_eq!(out["alt"], "A parabola with a point at x = 1.5.");
        assert_eq!(s.evaluate(&["slop".into()]).unwrap_err()["code"], "E301");
    }

    #[test]
    fn render_draws_at_device_pixels() {
        let mut s = session();
        assert_eq!(s.pixels(), (800, 600));
        let rgba = s.render().unwrap();
        assert_eq!(rgba.len(), 800 * 600 * 4);
        s.set_state(r#"{ "theme": "dark" }"#).unwrap();
        let dark = s.render().unwrap();
        assert_ne!(rgba[..4], dark[..4], "the background follows the theme");
        assert!(s.render_svg().unwrap().starts_with("<svg"));
        assert_eq!(s.underlay(), Json::Null);
    }

    #[test]
    fn a_drag_moves_the_point_along_its_curve() {
        let mut s = session();
        let hs = s.figure.handles(s.canvas, &s.theme()).unwrap();
        let p = hs[0].css;
        let hit = s.hit_test(p[0] + 3.0, p[1], "mouse").unwrap();
        assert_eq!(hit, json!({ "mark": "P", "drag": true, "focusable": true }));
        assert_eq!(s.hit_test(0.0, 0.0, "mouse").unwrap(), Json::Null);
        assert!(s.drag("P", "move", p[0], p[1]).is_err());
        s.drag("P", "start", p[0], p[1]).unwrap();
        let out = s.drag("P", "move", p[0] - 1000.0, p[1]).unwrap();
        assert_eq!(out["x"], json!({ "value": -2.0, "clamped": true }));
        s.drag("P", "end", 0.0, 0.0).unwrap();
        assert!(s.drag("P", "move", p[0], p[1]).is_err());
        assert_eq!(s.drag("P", "fling", 0.0, 0.0).unwrap_err()["code"], "E400");
    }
}
