//! A figure: a kinemo scene whose signals the state drives, drawn at rest.
//!
//! kinemo's IR turns out to suit this better than expected. Every property of
//! every object is a *signal id*, and a signal is an initial value plus a
//! timeline of keyframes. A scene built without animation has 694 of its 806
//! signals sitting on an empty timeline — they simply hold their initial
//! value.
//!
//! So hapto never advances time. It writes the state into those initial
//! values and renders at `t = 0`. The axes, the curve and the point are
//! kinemo's; what changes is who decides the numbers.

use std::sync::Arc;

use kinemo_eval::{Evaluator, TimelineIndex};
use kinemo_ir::Scene;
use kinemo_layout::Layout;
use kinemo_render::raster::{rasterize, Image};
use kinemo_render::{display_list_with, FrameSize};

use crate::{State, Value};

/// Where the figure is drawn, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Default for Size {
    fn default() -> Self {
        Size {
            width: 960,
            height: 540,
        }
    }
}

/// A drawing whose numbers come from state rather than from a clock.
pub struct Figure {
    scene: Scene,
    /// Which signal each named value writes to.
    bindings: Vec<(String, u32)>,
}

impl Figure {
    /// Take a kinemo scene as the drawing, with nothing bound to it yet.
    pub fn new(scene: Scene) -> Self {
        Figure {
            scene,
            bindings: Vec::new(),
        }
    }

    /// Read a scene from the JSON kinemo emits (`scene.builder.to_json()`).
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str::<Scene>(json)
            .map(Figure::new)
            .map_err(|err| format!("that is not a kinemo scene: {err}"))
    }

    /// Say that a named value drives the scene's free signals.
    ///
    /// A *free* signal is one no object owns: in kinemo's Python that is a
    /// bare `k.signal(2.0)`, and it is what a figure should be built around,
    /// because everything derived from it recomputes. Writing an object's own
    /// property instead does nothing when a constraint positions that object:
    /// `place(at=curve.point_at(3.0))` recomputes the dot's `x` from the
    /// constant 3.0 and discards whatever was written there.
    ///
    /// Scenes here have one knob, so there is one free signal and no
    /// ambiguity. More than one needs a way to tell them apart, which kinemo
    /// does not give a bare signal; that is a reason for hapto to build its
    /// own scenes rather than borrow kinemo's, later.
    pub fn bind_free(&mut self, name: &str) -> Result<(), String> {
        let free: Vec<u32> = self
            .scene
            .signals
            .iter()
            .filter(|s| s.owner.is_none())
            .map(|s| s.id)
            .collect();

        match free.len() {
            0 => Err("this figure has no free signal to drive. Build the scene \
                      around a `k.signal(...)` and use it where the value belongs."
                .into()),
            1 => {
                self.bindings.push((name.to_string(), free[0]));
                Ok(())
            }
            n => Err(format!(
                "this figure has {n} free signals and no way to tell them apart; \
                 bind by object and property instead"
            )),
        }
    }

    /// Say that a named value drives one object's property.
    ///
    /// Useful for a property nothing else computes — a colour, a radius. For
    /// a position that a constraint derives, see `bind_free`.
    pub fn bind(&mut self, name: &str, object: &str, property: &str) -> Result<(), String> {
        let found = self
            .scene
            .objects
            .iter()
            .find(|o| o.name.as_deref() == Some(object))
            .ok_or_else(|| format!("this figure has no object named \"{object}\""))?;

        let signal = found
            .props
            .get(property)
            .copied()
            .ok_or_else(|| format!("\"{object}\" has no property \"{property}\" to drive"))?;

        self.bindings.push((name.to_string(), signal));
        Ok(())
    }

    /// Draw the figure as the state has it now.
    ///
    /// There is no time to pass: every bound signal is written into its
    /// initial value and the frame is taken at rest.
    pub fn draw(&self, state: &State, size: Size) -> Result<Image, String> {
        let mut scene = self.scene.clone();

        for (name, signal) in &self.bindings {
            let value = state.get(name).ok_or_else(|| {
                format!("this figure drives \"{name}\", which the state does not hold")
            })?;
            let slot = scene
                .signals
                .iter_mut()
                .find(|s| s.id == *signal)
                .ok_or_else(|| format!("signal {signal} is not in this scene"))?;
            slot.initial = to_ir(value);
            // A keyframe would override the value just written.
            slot.timeline.clear();
        }

        let frame = FrameSize {
            width: size.width,
            height: size.height,
        };
        let timeline = Arc::new(TimelineIndex::new(&scene));
        let evaluator = Evaluator::with_index(&scene, timeline);
        let list = display_list_with(&Layout::new(&evaluator), 0.0, frame, false);
        Ok(rasterize(&list, true))
    }

    /// The names this figure drives, for checking a state against it.
    pub fn driven(&self) -> impl Iterator<Item = &str> {
        self.bindings.iter().map(|(name, _)| name.as_str())
    }
}

fn to_ir(value: &Value) -> kinemo_ir::Value {
    use kinemo_ir::Value as Ir;
    match value {
        Value::Number { n } => Ir::Float(*n),
        Value::Flag { on } => Ir::Float(if *on { 1.0 } else { 0.0 }),
        Value::Point { x, y } => Ir::Vec2([*x, *y]),
        Value::Choice { option } => Ir::Str(option.clone()),
    }
}
