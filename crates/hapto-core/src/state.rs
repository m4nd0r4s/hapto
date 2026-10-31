//! The state a figure reads: its values, and the reserved `focus` and
//! `theme` (SPEC-2.0 §5).
//!
//! Setting a value goes through its domain (§5.3): a number is clamped into
//! its range, snapped to its step and clamped again; a point is clamped per
//! coordinate; a choice outside its options and a value of the wrong type
//! are refused with the reason. `set` reports whether it clamped, so the
//! pointer is clamped quietly and a program learns that it was.

use hapto_expr::error::codes as expr_codes;
use hapto_expr::{Error, Type, Value};

use crate::codes;
use crate::document::{Kind, Scale, Step, ValueDecl};

/// What a caller asks a value to become.
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    Number(f64),
    Choice(String),
    Point([f64; 2]),
    Flag(bool),
}

/// What a set did.
#[derive(Debug, Clone, PartialEq)]
pub struct Admitted {
    pub applied: Value,
    /// The input lay outside the range and was brought inside it.
    pub clamped: bool,
}

/// A figure's values, in declaration order, plus its reserved state.
#[derive(Debug, Clone)]
pub struct State {
    decls: Vec<ValueDecl>,
    values: Vec<Value>,
    pub focus: String,
    pub theme: String,
}

/// The type a value has in formulas (§5.1).
pub fn type_of(kind: &Kind) -> Type {
    match kind {
        Kind::Number { .. } => Type::Num,
        Kind::Choice { .. } => Type::Str,
        Kind::Point { .. } => Type::Vec,
        Kind::Flag { .. } => Type::Bool,
    }
}

fn default_of(kind: &Kind) -> Value {
    match kind {
        Kind::Number { default, .. } => Value::Num(*default),
        Kind::Choice { default, .. } => Value::str(default),
        Kind::Point { default, .. } => Value::Vec(*default),
        Kind::Flag { default } => Value::Bool(*default),
    }
}

/// Snap `n` to the step grid anchored at `low` (§5.3, §5.4).
fn snap(n: f64, low: f64, step: Step, scale: Scale) -> f64 {
    match (step, scale) {
        (Step::Continuous, _) => n,
        (Step::By(s), Scale::Linear) => low + ((n - low) / s).round() * s,
        (Step::By(s), Scale::Log) => low * s.powf(((n / low).ln() / s.ln()).round()),
        (Step::Integer, _) => n.round(),
    }
}

impl State {
    pub fn new(decls: &[ValueDecl]) -> Self {
        State {
            decls: decls.to_vec(),
            values: decls.iter().map(|d| default_of(&d.kind)).collect(),
            focus: String::new(),
            theme: "light".into(),
        }
    }

    /// The values in declaration order: what formulas read as `Value(i)`.
    pub fn values(&self) -> &[Value] {
        &self.values
    }

    pub fn decls(&self) -> &[ValueDecl] {
        &self.decls
    }

    /// Names and types, for the formula module (§4.4).
    pub fn types(&self) -> Vec<(String, Type)> {
        self.decls
            .iter()
            .map(|d| (d.name.clone(), type_of(&d.kind)))
            .collect()
    }

    pub fn index(&self, name: &str) -> Option<usize> {
        self.decls.iter().position(|d| d.name == name)
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.index(name).map(|i| &self.values[i])
    }

    /// Back to every default (§12.7).
    pub fn reset(&mut self) {
        self.values = self.decls.iter().map(|d| default_of(&d.kind)).collect();
        self.focus.clear();
    }

    /// Whether any value differs from its default: the reset button shows.
    pub fn is_default(&self) -> bool {
        self.decls
            .iter()
            .zip(&self.values)
            .all(|(d, v)| default_of(&d.kind) == *v)
    }

    /// Set a value through its domain.
    pub fn set(&mut self, name: &str, input: Input) -> Result<Admitted, Error> {
        let Some(i) = self.index(name) else {
            let names: Vec<&str> = self.decls.iter().map(|d| d.name.as_str()).collect();
            let mut err = Error::new(
                expr_codes::E301,
                format!("this figure has no value named \"{name}\""),
            );
            err = match crate::tree::nearest(name, &names) {
                Some(n) => err.hint(format!("did you mean `{n}`?")),
                None => err.options(names.iter().copied()),
            };
            return Err(err);
        };
        let admitted = admit(&self.decls[i], input)?;
        self.values[i] = admitted.applied.clone();
        Ok(admitted)
    }
}

/// The step a control moves by: the declared step, or a thousandth of the
/// range when continuous, so no slider is ever coarse (§5.3).
pub fn control_step(kind: &Kind) -> Option<f64> {
    match kind {
        Kind::Number {
            low, high, step, ..
        } => Some(match step {
            Step::Continuous => (high - low) / 1000.0,
            Step::By(s) => *s,
            Step::Integer => 1.0,
        }),
        _ => None,
    }
}

/// Admit an input into a declaration's domain (§5.3).
pub fn admit(decl: &ValueDecl, input: Input) -> Result<Admitted, Error> {
    let wrong = |wanted: &str| {
        Error::new(
            codes::E403,
            format!(
                "\"{}\" is a {} value; set it with {wanted}",
                decl.name,
                decl.kind.type_name()
            ),
        )
    };
    Ok(match (&decl.kind, input) {
        (
            Kind::Number {
                low,
                high,
                step,
                scale,
                ..
            },
            Input::Number(n),
        ) => {
            if !n.is_finite() {
                return Err(wrong("a finite number"));
            }
            let clamped_in = n.clamp(*low, *high);
            let snapped = snap(clamped_in, *low, *step, *scale).clamp(*low, *high);
            Admitted {
                applied: Value::Num(snapped),
                clamped: clamped_in != n,
            }
        }
        (Kind::Point { low, high, .. }, Input::Point(p)) => {
            if !(p[0].is_finite() && p[1].is_finite()) {
                return Err(wrong("two finite numbers"));
            }
            let q = [p[0].clamp(low[0], high[0]), p[1].clamp(low[1], high[1])];
            Admitted {
                applied: Value::Vec(q),
                clamped: q != p,
            }
        }
        (Kind::Choice { options, .. }, Input::Choice(c)) => {
            if !options.contains(&c) {
                return Err(Error::new(
                    codes::E504,
                    format!("\"{c}\" is not one of: {}", options.join(", ")),
                )
                .options(options.clone()));
            }
            Admitted {
                applied: Value::str(&c),
                clamped: false,
            }
        }
        (Kind::Flag { .. }, Input::Flag(b)) => Admitted {
            applied: Value::Bool(b),
            clamped: false,
        },
        (Kind::Number { .. }, _) => return Err(wrong("a number")),
        (Kind::Point { .. }, _) => return Err(wrong("a point [x, y]")),
        (Kind::Choice { .. }, _) => return Err(wrong("one of its options")),
        (Kind::Flag { .. }, _) => return Err(wrong("true or false")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::values::read_values;
    use crate::tree::{parse, At, Reader};

    fn state(yaml: &str) -> State {
        let tree = parse(yaml).unwrap();
        let mut r = Reader::default();
        let decls = read_values(
            &mut r,
            At {
                tree: &tree,
                path: "/values",
            },
        );
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        State::new(&decls)
    }

    const VALUES: &str = "
- { name: x, type: number, range: [0, 5], default: 1 }
- { name: s, type: number, range: [0, 1], default: 0, step: 0.25 }
- { name: n, type: number, range: [1, 1000], default: 10, scale: log, step: 10 }
- { name: k, type: number, range: [0, 10], default: 3, step: integer }
- { name: mode, type: choice, options: [free, damped, driven], default: free }
- { name: p, type: point, range: [[-1, -1], [1, 1]], default: [0, 0] }
- { name: on, type: flag, default: true }";

    #[test]
    fn defaults_in_declaration_order() {
        let s = state(VALUES);
        assert_eq!(s.values()[0], Value::Num(1.0));
        assert_eq!(s.values()[4], Value::str("free"));
        assert_eq!(s.values()[5], Value::Vec([0.0, 0.0]));
        assert_eq!(s.types()[5], ("p".to_string(), Type::Vec));
        assert!(s.is_default());
    }

    #[test]
    fn numbers_clamp_snap_and_say_so() {
        let mut s = state(VALUES);
        assert_eq!(
            s.set("x", Input::Number(99.0)).unwrap(),
            Admitted {
                applied: Value::Num(5.0),
                clamped: true
            }
        );
        assert_eq!(
            s.set("x", Input::Number(2.5)).unwrap(),
            Admitted {
                applied: Value::Num(2.5),
                clamped: false
            }
        );
        assert_eq!(
            s.set("s", Input::Number(0.6)).unwrap().applied,
            Value::Num(0.5)
        );
        assert_eq!(
            s.set("k", Input::Number(3.4)).unwrap().applied,
            Value::Num(3.0)
        );
        // A log step multiplies: 10 → 100 → 1000.
        assert_eq!(
            s.set("n", Input::Number(140.0)).unwrap().applied,
            Value::Num(100.0)
        );
        assert!(!s.is_default());
        s.reset();
        assert!(s.is_default());
    }

    #[test]
    fn points_clamp_per_coordinate() {
        let mut s = state(VALUES);
        let a = s.set("p", Input::Point([2.0, 0.5])).unwrap();
        assert_eq!(
            a,
            Admitted {
                applied: Value::Vec([1.0, 0.5]),
                clamped: true
            }
        );
    }

    #[test]
    fn choices_and_types_are_refused_with_reasons() {
        let mut s = state(VALUES);
        let e = s.set("mode", Input::Choice("spinning".into())).unwrap_err();
        assert_eq!(e.code, codes::E504);
        assert_eq!(e.options, ["free", "damped", "driven"]);
        assert_eq!(s.set("x", Input::Flag(true)).unwrap_err().code, codes::E403);
        assert_eq!(
            s.set("x", Input::Number(f64::NAN)).unwrap_err().code,
            codes::E403
        );
        let e = s.set("modee", Input::Choice("free".into())).unwrap_err();
        assert_eq!(e.hint.as_deref(), Some("did you mean `mode`?"));
        assert_eq!(
            s.set("on", Input::Flag(false)).unwrap().applied,
            Value::Bool(false)
        );
    }

    /// kinemo derived a slider's step from a question's tolerance, and a wide
    /// tolerance gave a two-position slider. Ported from 1.0 and tightened:
    /// a continuous number moves in thousandths of its range.
    #[test]
    fn a_continuous_slider_is_never_coarse() {
        let s = state("- { name: x, type: number, range: [2, 8], default: 3 }");
        let step = control_step(&s.decls()[0].kind).unwrap();
        assert!((8.0 - 2.0) / step >= 1000.0, "step {step} is too coarse");
    }
}
