//! A figure in the browser: state in, pixels out.
//!
//! The page owns the canvas and the controls. This holds the figure, takes a
//! value when something sets one, and hands back the pixels. There is no
//! clock here and nothing to play: `draw` is a pure function of the state.

use hapto_core::book::{columns, plot, Range};
use hapto_core::{Domain, Figure as CoreFigure, Knob, Size, State, Value};
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

/// A figure the page can touch.
#[wasm_bindgen]
pub struct Figure {
    figure: CoreFigure,
    state: State,
    size: Size,
}

#[wasm_bindgen]
impl Figure {
    /// A figure for the scene `ir` (kinemo's JSON), drawing `width`×`height`.
    ///
    /// The scene must be built around one free signal — a bare
    /// `k.signal(...)` that no object owns — which becomes the value named
    /// `name`. See `hapto_core::Figure::bind_free`.
    #[wasm_bindgen(constructor)]
    pub fn new(ir: &str, name: &str, width: u32, height: u32) -> Result<Figure, JsError> {
        let mut figure = CoreFigure::from_json(ir).map_err(|e| JsError::new(&e))?;
        figure.bind_free(name).map_err(|e| JsError::new(&e))?;
        Ok(Figure {
            figure,
            state: State::new(),
            size: Size { width, height },
        })
    }

    /// Declare a number the reader can move, with its bounds and where it starts.
    ///
    /// The step is the domain's, a hundredth of the range, so a control built
    /// from `step()` is never coarse.
    pub fn declare_number(&mut self, name: &str, low: f64, high: f64, at: f64) {
        self.state.declare(Knob {
            name: name.to_string(),
            domain: Domain::Number {
                low,
                high,
                step: None,
            },
            value: Value::number(at),
            label: String::new(),
        });
    }

    /// Set a value. This is the whole interface: a slider, a drag, or a word
    /// in the prose beside the figure all call exactly this.
    pub fn set(&mut self, name: &str, value: f64) -> Result<(), JsError> {
        self.state
            .set(name, Value::number(value))
            .map_err(|e| JsError::new(&e))
    }

    /// What a value is now, after the domain had its say.
    pub fn get(&self, name: &str) -> Option<f64> {
        self.state.number(name)
    }

    /// How finely a control for this value should move.
    pub fn step(&self, name: &str) -> Option<f64> {
        self.state
            .knobs()
            .find(|k| k.name == name)
            .and_then(|k| k.domain.step())
    }

    /// Draw frames at another size from now on (the canvas was resized).
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
            .draw(&self.state, self.size)
            .map(|image| image.rgba)
            .map_err(|e| JsError::new(&e))
    }
}
