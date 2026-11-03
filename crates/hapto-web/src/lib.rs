//! A figure in the browser: state in, pixels out.
//!
//! The page owns the canvas and the controls. This holds the figure, takes a
//! value when something sets one, and hands back the pixels. There is no
//! clock here and nothing to play: `render` is a pure function of the state.
//!
//! [`Figure`] is the 2.0 surface the page runtime's worker calls (SPEC-2.0
//! §15.10); its logic is [`session::Session`]. [`BookFigure`] is 1.0's, kept
//! until the `legacy` preset carries its specs (hapto-hl7.2).

pub mod session;

use hapto_core::v1::book::{columns, plot, Range};
use hapto_core::v1::{Domain, Figure as CoreFigure, Size, Value};
use serde_json::Value as Json;
use session::Session;
use wasm_bindgen::prelude::*;

/// A figure a book page asks for by describing it.
///
/// The book's compiler knows the curve, the range and the knob — it wrote
/// them into the lesson — so it hands them over as JSON and gets a figure
/// back. Nothing about kinemo, no exported scene folder and no iframe: the
/// page owns a canvas and this draws into it.
///
/// ```js
/// const figure = BookFigure.fromSpec(JSON.stringify({
///   kind: "plot", variable: "x", at: 1,
///   curve: "x**2 - x*(2)", tangent: true,
///   x_range: [-2, 4], y_range: [-2.35, 9.35],
/// }), 960, 540);
/// figure.set("x", 3);          // a slider, or a word in the prose
/// const rgba = figure.draw();
/// ```
#[wasm_bindgen]
pub struct BookFigure {
    figure: CoreFigure,
    variable: String,
    size: Size,
}

#[wasm_bindgen]
impl BookFigure {
    /// Build the figure a lesson's `playground` step describes.
    ///
    /// A spec that does not make a figure is an error with a reason, never a
    /// blank canvas: the book should fail loudly rather than quietly in the
    /// reader's browser.
    #[wasm_bindgen(js_name = fromSpec)]
    pub fn from_spec(spec: &str, width: u32, height: u32) -> Result<BookFigure, JsError> {
        let spec: serde_json::Value =
            serde_json::from_str(spec).map_err(|e| JsError::new(&format!("bad spec: {e}")))?;

        let text = |key: &str| -> Result<String, JsError> {
            spec.get(key)
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .ok_or_else(|| JsError::new(&format!("the spec needs a \"{key}\"")))
        };
        let range = |key: &str| -> Result<Range, JsError> {
            let pair = spec
                .get(key)
                .and_then(|v| v.as_array())
                .ok_or_else(|| JsError::new(&format!("the spec needs \"{key}\": [low, high]")))?;
            let read = |i: usize| pair.get(i).and_then(serde_json::Value::as_f64);
            match (read(0), read(1)) {
                (Some(low), Some(high)) => Ok(Range::new(low, high)),
                _ => Err(JsError::new(&format!("\"{key}\" has to be two numbers"))),
            }
        };

        let variable = text("variable")?;
        let at = spec
            .get("at")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);
        let x_range = range("x_range")?;
        // How far the reader may move the value. A lesson that does not say
        // is drawn over its own axes, which is what a plot wants anyway.
        let knob = range("knob").unwrap_or(x_range);

        let built = match spec.get("kind").and_then(|v| v.as_str()) {
            Some("plot") => plot(
                &text("curve")?,
                &variable,
                at,
                knob,
                x_range,
                range("y_range")?,
                spec.get("tangent").and_then(serde_json::Value::as_bool) == Some(true),
            ),
            Some("columns") => {
                let column = |key: &str| -> Result<(String, String), JsError> {
                    let pair = spec.get(key).and_then(|v| v.as_array()).ok_or_else(|| {
                        JsError::new(&format!("the spec needs \"{key}\": [across, up]"))
                    })?;
                    let read = |i: usize| pair.get(i).and_then(|v| v.as_str()).map(str::to_string);
                    match (read(0), read(1)) {
                        (Some(a), Some(b)) => Ok((a, b)),
                        _ => Err(JsError::new(&format!("\"{key}\" has to be two formulas"))),
                    }
                };
                let first = column("first")?;
                let second = column("second")?;
                columns(
                    (&first.0, &first.1),
                    (&second.0, &second.1),
                    &variable,
                    at,
                    knob,
                    x_range,
                    range("y_range")?,
                )
            }
            Some(other) => {
                return Err(JsError::new(&format!(
                    "\"{other}\" is not a kind of figure. There are: plot, columns"
                )))
            }
            None => return Err(JsError::new("the spec needs a \"kind\": plot or columns")),
        };

        Ok(BookFigure {
            figure: built.map_err(|e| JsError::new(&e))?,
            variable,
            size: Size { width, height },
        })
    }

    /// The value this figure is drawn over, which the page puts on a slider.
    pub fn variable(&self) -> String {
        self.variable.clone()
    }

    /// Set a value: a slider, a drag, or a word in the prose beside it.
    pub fn set(&mut self, name: &str, value: f64) -> Result<(), JsError> {
        self.figure
            .set(name, Value::number(value))
            .map_err(|e| JsError::new(&e))
    }

    /// What a value is now, after its bounds had their say.
    pub fn get(&self, name: &str) -> Option<f64> {
        self.figure.state().number(name)
    }

    /// How finely a control for this value should move.
    pub fn step(&self, name: &str) -> Option<f64> {
        self.figure
            .state()
            .knobs()
            .find(|k| k.name == name)
            .and_then(|k| k.domain.step())
    }

    /// The low and high ends of a value, for a slider's own bounds.
    pub fn bounds(&self, name: &str) -> Option<Vec<f64>> {
        self.figure
            .state()
            .knobs()
            .find(|k| k.name == name)
            .and_then(|k| match k.domain {
                Domain::Number { low, high, .. } => Some(vec![low, high]),
                _ => None,
            })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.size = Size { width, height };
    }

    pub fn width(&self) -> u32 {
        self.size.width
    }

    pub fn height(&self) -> u32 {
        self.size.height
    }

    /// The figure as the state has it now, as RGBA bytes, row by row.
    pub fn draw(&self) -> Result<Vec<u8>, JsError> {
        self.figure
            .draw_now(self.size)
            .map(|image| image.rgba)
            .map_err(|e| JsError::new(&e))
    }
}

/// A JSON value as a JS object.
fn to_js(v: &Json) -> JsValue {
    js_sys::JSON::parse(&v.to_string()).unwrap_or(JsValue::NULL)
}

/// An error as a JS `Error` with the first error's `code`, `path` and
/// `hint`, and every error under `errors` (§15.12).
fn throw(e: Json) -> JsValue {
    let message = e["message"].as_str().unwrap_or("hapto failed").to_string();
    let code = e["code"].as_str().unwrap_or("");
    let error = js_sys::Error::new(&format!("{code}: {message}"));
    if let Json::Object(fields) = &e {
        for (k, v) in fields {
            if k != "message" {
                let _ = js_sys::Reflect::set(&error, &JsValue::from_str(k), &to_js(v));
            }
        }
    }
    error.into()
}

/// A 2.0 figure: a document, its state, and the pixels it draws (§15.10).
///
/// ```js
/// const figure = Figure.load(yamlText, "yaml");
/// figure.resize(cssWidth, figure.heightFor(cssWidth), Math.min(devicePixelRatio, 2));
/// figure.setState({ x: 3 });            // a slider, a drag, a word in the prose
/// const { readouts, alt } = figure.evaluate([]);
/// const rgba = figure.render();          // pixelWidth × pixelHeight × 4 bytes
/// ```
#[wasm_bindgen]
pub struct Figure {
    session: Session,
}

#[wasm_bindgen]
impl Figure {
    /// Read and compile a document. A document that does not make a figure
    /// throws every error it has, never draws a blank.
    pub fn load(text: &str, format: &str) -> Result<Figure, JsValue> {
        Ok(Figure {
            session: Session::load(text, format).map_err(throw)?,
        })
    }

    /// Every value with its domain, control and current value.
    pub fn values(&self) -> JsValue {
        to_js(&self.session.values())
    }

    /// Set values, `{ name: value }`: all admitted or none. Returns what
    /// each became and whether it was clamped.
    #[wasm_bindgen(js_name = setState)]
    pub fn set_state(&mut self, state: JsValue) -> Result<JsValue, JsValue> {
        let text = js_sys::JSON::stringify(&state)
            .map(String::from)
            .unwrap_or_default();
        self.session
            .set_state(&text)
            .map(|v| to_js(&v))
            .map_err(throw)
    }

    /// Back to the defaults, and no focus.
    pub fn reset(&mut self) {
        self.session.reset();
    }

    /// The document's id, title, caption and size rules.
    pub fn meta(&self) -> JsValue {
        to_js(&self.session.meta())
    }

    /// The height to reserve for this CSS width.
    #[wasm_bindgen(js_name = heightFor)]
    pub fn height_for(&self, css_width: f64) -> f64 {
        self.session.height_for(css_width)
    }

    pub fn resize(&mut self, css_width: f64, css_height: f64, dpr: f64) {
        self.session.resize(css_width, css_height, dpr);
    }

    #[wasm_bindgen(getter, js_name = pixelWidth)]
    pub fn pixel_width(&self) -> u32 {
        self.session.pixels().0
    }

    #[wasm_bindgen(getter, js_name = pixelHeight)]
    pub fn pixel_height(&self) -> u32 {
        self.session.pixels().1
    }

    /// Readouts, the named lets, and the alt text, at this state.
    pub fn evaluate(&mut self, lets: Option<Vec<String>>) -> Result<JsValue, JsValue> {
        self.session
            .evaluate(&lets.unwrap_or_default())
            .map(|v| to_js(&v))
            .map_err(throw)
    }

    /// The figure as RGBA bytes, `pixelWidth` × `pixelHeight`, row by row.
    /// The worker puts them on its OffscreenCanvas or posts them.
    pub fn render(&mut self) -> Result<Vec<u8>, JsValue> {
        self.session.render().map_err(throw)
    }

    /// Warnings from loading and from the last render, for development mode.
    pub fn warnings(&self) -> JsValue {
        to_js(&self.session.warnings())
    }

    /// The heat-map cell grid under the canvas, or `null`.
    pub fn underlay(&self) -> JsValue {
        to_js(&self.session.underlay())
    }

    #[wasm_bindgen(js_name = renderSVG)]
    pub fn render_svg(&mut self) -> Result<String, JsValue> {
        self.session.render_svg().map_err(throw)
    }

    /// `{ mark, drag, focusable }` under a pointer, or `null`.
    /// `pointer` is the event's `pointerType`.
    #[wasm_bindgen(js_name = hitTest)]
    pub fn hit_test(&mut self, css_x: f64, css_y: f64, pointer: &str) -> Result<JsValue, JsValue> {
        self.session
            .hit_test(css_x, css_y, pointer)
            .map(|v| to_js(&v))
            .map_err(throw)
    }

    /// `phase` is `start`, `move`, `end` or `cancel`; `move` returns the
    /// values set, as `setState` does.
    pub fn drag(
        &mut self,
        mark: &str,
        phase: &str,
        css_x: f64,
        css_y: f64,
    ) -> Result<JsValue, JsValue> {
        self.session
            .drag(mark, phase, css_x, css_y)
            .map(|v| to_js(&v))
            .map_err(throw)
    }
}
