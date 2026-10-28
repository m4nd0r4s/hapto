//! State in, drawing out.
//!
//! kinemo's frame is a function of time. hapto's frame is a function of
//! *state*: a handful of named values that a slider, a drag, a hover or a
//! sentence in the prose beside the figure can set. Nothing advances on its
//! own, and there is no cursor to wait for.
//!
//! ```text
//! state:  x = 3           set by anything
//! figure: f(state) -> drawing
//! ```
//!
//! That is the whole difference, and everything else follows from it. See
//! `docs/why.md` for why this could not be a layer on top of the timeline.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A value the reader can change, and the figure reads.
///
/// The variants carry named fields rather than tuples because the JSON that
/// crosses to the browser is internally tagged (`{"kind": "number", "n": 3}`),
/// and serde cannot tag a tuple variant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Value {
    /// A number on a slider, or typed, or dragged.
    Number { n: f64 },
    /// One of a few named options.
    Choice { option: String },
    /// A position, from dragging a point.
    Point { x: f64, y: f64 },
    /// On or off.
    Flag { on: bool },
}

impl Value {
    pub fn number(n: f64) -> Self {
        Value::Number { n }
    }

    pub fn choice(option: impl Into<String>) -> Self {
        Value::Choice { option: option.into() }
    }

    pub fn point(x: f64, y: f64) -> Self {
        Value::Point { x, y }
    }

    pub fn flag(on: bool) -> Self {
        Value::Flag { on }
    }

    pub fn as_number(&self) -> Option<f64> {
        match self {
            Value::Number { n } => Some(*n),
            Value::Flag { on } => Some(if *on { 1.0 } else { 0.0 }),
            _ => None,
        }
    }
}

/// What a value may be, so a control can be drawn for it and a bad set refused.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Domain {
    /// A number between two bounds. `step` is how finely it moves; `None`
    /// means continuous, which is a hundredth of the range in practice.
    ///
    /// kinemo derived a slider's step from a question's answer tolerance,
    /// which gave a two-position slider whenever the tolerance was wide. Here
    /// the step belongs to the value, because it is a property of the value.
    Number { low: f64, high: f64, step: Option<f64> },
    Choice { options: Vec<String> },
    Point { low: (f64, f64), high: (f64, f64) },
    Flag,
}

impl Domain {
    /// Clamp a value into this domain, or say why it does not belong.
    pub fn admit(&self, value: &Value) -> Result<Value, String> {
        match (self, value) {
            (Domain::Number { low, high, step }, Value::Number { n }) => {
                let mut n = n.clamp(*low, *high);
                if let Some(step) = step {
                    if *step > 0.0 {
                        n = low + ((n - low) / step).round() * step;
                        n = n.clamp(*low, *high);
                    }
                }
                Ok(Value::number(n))
            }
            (Domain::Choice { options }, Value::Choice { option }) => {
                if options.iter().any(|o| o == option) {
                    Ok(Value::choice(option.clone()))
                } else {
                    Err(format!(
                        "\"{option}\" is not one of its options ({})",
                        options.join(", ")
                    ))
                }
            }
            (Domain::Point { low, high }, Value::Point { x, y }) => {
                Ok(Value::point(x.clamp(low.0, high.0), y.clamp(low.1, high.1)))
            }
            (Domain::Flag, Value::Flag { on }) => Ok(Value::flag(*on)),
            _ => Err("that value is not the kind this one holds".into()),
        }
    }

    /// The step a control should move in. A continuous number gets a
    /// hundredth of its range, which is fine enough to feel smooth.
    pub fn step(&self) -> Option<f64> {
        match self {
            Domain::Number { low, high, step } => {
                Some(step.unwrap_or_else(|| (high - low) / 100.0))
            }
            _ => None,
        }
    }
}

/// One named value, with what it may be and what it is now.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Knob {
    pub name: String,
    pub domain: Domain,
    pub value: Value,
    /// Shown beside the control. Empty means use the name.
    #[serde(default)]
    pub label: String,
}

/// Everything a figure reads. Changing it is the only thing that happens.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct State {
    knobs: BTreeMap<String, Knob>,
}

impl State {
    pub fn new() -> Self {
        Self::default()
    }

    /// Declare a value the reader can change.
    pub fn declare(&mut self, knob: Knob) {
        self.knobs.insert(knob.name.clone(), knob);
    }

    /// Set a value. Anything may call this: a slider, a drag, or a word in
    /// the prose beside the figure. It is the whole interface.
    pub fn set(&mut self, name: &str, value: Value) -> Result<(), String> {
        let knob = self
            .knobs
            .get_mut(name)
            .ok_or_else(|| format!("no value named \"{name}\" in this figure"))?;
        knob.value = knob.domain.admit(&value)?;
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.knobs.get(name).map(|k| &k.value)
    }

    pub fn number(&self, name: &str) -> Option<f64> {
        self.get(name).and_then(Value::as_number)
    }

    pub fn knobs(&self) -> impl Iterator<Item = &Knob> {
        self.knobs.values()
    }

    pub fn is_empty(&self) -> bool {
        self.knobs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn number(name: &str, low: f64, high: f64, at: f64) -> Knob {
        Knob {
            name: name.into(),
            domain: Domain::Number { low, high, step: None },
            value: Value::number(at),
            label: String::new(),
        }
    }

    #[test]
    fn a_value_can_be_set_by_name() {
        let mut state = State::new();
        state.declare(number("x", 0.0, 10.0, 3.0));
        assert_eq!(state.number("x"), Some(3.0));
        state.set("x", Value::number(7.0)).unwrap();
        assert_eq!(state.number("x"), Some(7.0));
    }

    #[test]
    fn a_value_is_clamped_to_its_domain() {
        let mut state = State::new();
        state.declare(number("x", 0.0, 5.0, 1.0));
        state.set("x", Value::number(99.0)).unwrap();
        assert_eq!(state.number("x"), Some(5.0));
    }

    #[test]
    fn a_continuous_slider_is_not_coarse() {
        // kinemo's player derived the step from a question's tolerance, and a
        // wide tolerance gave a slider with two positions. A hundredth of the
        // range is the floor here.
        let domain = Domain::Number { low: 2.0, high: 8.0, step: None };
        let step = domain.step().unwrap();
        assert!((8.0 - 2.0) / step >= 100.0, "step {step} is too coarse");
    }

    #[test]
    fn setting_an_unknown_name_says_so() {
        let mut state = State::new();
        let err = state.set("nope", Value::number(1.0)).unwrap_err();
        assert!(err.contains("nope"), "{err}");
    }

    #[test]
    fn a_choice_outside_its_options_is_refused() {
        let mut state = State::new();
        state.declare(Knob {
            name: "mode".into(),
            domain: Domain::Choice { options: vec!["free".into(), "damped".into()] },
            value: Value::choice("free"),
            label: String::new(),
        });
        assert!(state.set("mode", Value::choice("spinning")).is_err());
        assert!(state.set("mode", Value::choice("damped")).is_ok());
    }
}
