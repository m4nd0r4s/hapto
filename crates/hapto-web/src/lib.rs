//! A figure in the browser: state in, pixels out.
//!
//! The page owns the canvas and the controls. This holds the figure, takes a
//! value when something sets one, and hands back the pixels. There is no
//! clock here and nothing to play: `draw` is a pure function of the state.

use hapto_core::{Domain, Figure as CoreFigure, Knob, Size, State, Value};
use wasm_bindgen::prelude::*;

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
