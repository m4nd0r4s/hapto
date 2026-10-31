//! What a formula evaluates to.
//!
//! Every failure that depends on the state — `1/x` at zero, `sqrt(-1)`, an
//! index past the end — evaluates to *no value* rather than stopping
//! (SPEC-2.0 §4.8). Numbers carry it as NaN; everything else as
//! [`Value::Nothing`]. Whoever consumes a value decides what no value means:
//! a mark is skipped, a readout prints "—", a condition is false.

use std::fmt;
use std::sync::Arc;

/// A matrix of 1×1 to 4×4, row-major.
#[derive(Debug, Clone, PartialEq)]
pub struct Matrix {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<f64>,
}

impl Matrix {
    pub fn get(&self, row: usize, col: usize) -> f64 {
        self.data[row * self.cols + col]
    }

    pub fn row(&self, row: usize) -> &[f64] {
        &self.data[row * self.cols..(row + 1) * self.cols]
    }
}

/// A record value: a compute node's result, or what `eig` returns.
pub trait Record: fmt::Debug + Send + Sync {
    fn field(&self, name: &str) -> Value;

    /// Call a method, as in `orbit.at(s)`. Records without methods keep the
    /// default.
    fn call(&self, _method: &str, _args: &[Value]) -> Value {
        Value::Nothing
    }
}

/// A record that is just named fields.
#[derive(Debug, Clone)]
pub struct Fields(pub Vec<(&'static str, Value)>);

impl Record for Fields {
    fn field(&self, name: &str) -> Value {
        self.0
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.clone())
            .unwrap_or(Value::Nothing)
    }
}

#[derive(Debug, Clone)]
pub enum Value {
    Num(f64),
    Bool(bool),
    Str(Arc<str>),
    Vec([f64; 2]),
    /// sRGB with straight alpha, components in 0..=1.
    Colour([f64; 4]),
    List(Arc<Vec<Value>>),
    Mat(Arc<Matrix>),
    Record(Arc<dyn Record>),
    /// No value, for anything that is not a number (§4.8).
    Nothing,
}

impl Value {
    pub fn list(items: Vec<Value>) -> Value {
        Value::List(Arc::new(items))
    }

    pub fn str(s: &str) -> Value {
        Value::Str(Arc::from(s))
    }

    pub fn nums(items: impl IntoIterator<Item = f64>) -> Value {
        Value::list(items.into_iter().map(Value::Num).collect())
    }

    /// The number, or NaN for anything else (no value included).
    pub fn num(&self) -> f64 {
        match self {
            Value::Num(n) => *n,
            _ => f64::NAN,
        }
    }

    pub fn vec(&self) -> Option<[f64; 2]> {
        match self {
            Value::Vec(v) if v[0].is_finite() && v[1].is_finite() => Some(*v),
            _ => None,
        }
    }

    pub fn bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn items(&self) -> Option<&[Value]> {
        match self {
            Value::List(items) => Some(items),
            _ => None,
        }
    }

    /// Whether this is no value: NaN, a non-finite vector, or nothing.
    pub fn is_nothing(&self) -> bool {
        match self {
            Value::Num(n) => !n.is_finite(),
            Value::Vec(v) => !(v[0].is_finite() && v[1].is_finite()),
            Value::Nothing => true,
            _ => false,
        }
    }
}

/// Structural equality, with no value equal to no value, for tests.
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (a, b) if a.is_nothing() && b.is_nothing() => true,
            (Value::Num(a), Value::Num(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Vec(a), Value::Vec(b)) => a == b,
            (Value::Colour(a), Value::Colour(b)) => a == b,
            (Value::List(a), Value::List(b)) => a == b,
            (Value::Mat(a), Value::Mat(b)) => a == b,
            (Value::Record(a), Value::Record(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            v if v.is_nothing() => f.write_str("—"),
            Value::Num(n) => write!(f, "{n}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Str(s) => f.write_str(s),
            Value::Vec([x, y]) => write!(f, "[{x}, {y}]"),
            Value::Colour(c) => write!(f, "rgba({}, {}, {}, {})", c[0], c[1], c[2], c[3]),
            Value::List(items) => {
                f.write_str("[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{item}")?;
                }
                f.write_str("]")
            }
            Value::Mat(m) => {
                f.write_str("mat(")?;
                for r in 0..m.rows {
                    if r > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{:?}", m.row(r))?;
                }
                f.write_str(")")
            }
            Value::Record(r) => write!(f, "{r:?}"),
            Value::Nothing => f.write_str("—"),
        }
    }
}
