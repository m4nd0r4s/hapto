//! The checked form of a formula: what the evaluator runs.
//!
//! Every decision the type checker made is explicit here — which `+` (number,
//! vector, matrix), which slot a name lives in, which pair became a list — so
//! the evaluator never has to look at a type, and a formula that checked can
//! only fail at runtime by producing no value (§4.8) or running out of fuel.

use crate::types::Coercion;
use crate::value::Value;

/// Index of a builtin in the library registry.
pub type BuiltinId = usize;

/// An instance of a user function, compiled for one list of argument types.
pub type FnId = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arith {
    Num,
    Vec,
    Mat,
}

/// A binary operation with its operand types already settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinKind {
    Add(Arith),
    Sub(Arith),
    MulNum,
    /// `num * vec`
    ScaleVec,
    /// `vec * num`
    VecScale,
    /// `num * mat` or `mat * num`; the matrix is on the side `mat_left` says.
    ScaleMat {
        mat_left: bool,
    },
    MatVec,
    MatMat,
    DivNum,
    DivVec,
    DivMat,
    Mod,
    PowNum,
    PowMat,
    Lt,
    Le,
    Gt,
    Ge,
    /// `==` and `!=` on numbers, strings or booleans.
    Eq,
    Ne,
}

/// What a reduction over a generator computes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reducer {
    Sum(Arith),
    Prod,
    Min,
    Max,
    Mean(Arith),
    Count,
}

/// `body for slot in iter if cond`.
#[derive(Debug, Clone, PartialEq)]
pub struct Loop {
    pub slot: usize,
    pub iter: Box<Node>,
    pub body: Box<Node>,
    pub cond: Option<Box<Node>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Const(Value),
    /// A bound variable in the current frame: a comprehension variable, a
    /// function parameter, or a name the caller binds (a mark's `x`).
    Local(usize),
    /// A value of the figure, by its index in the declaration order.
    Value(usize),
    /// A compute node's record, by index.
    Compute(usize),
    /// A let, by index.
    Let(usize),
    Neg(Arith, Box<Node>),
    Not(Box<Node>),
    And(Box<Node>, Box<Node>),
    Or(Box<Node>, Box<Node>),
    Binary(BinKind, Box<Node>, Box<Node>),
    If(Box<Node>, Box<Node>, Box<Node>),
    /// `[a, b]` read as a vector.
    MakeVec(Box<Node>, Box<Node>),
    List(Vec<Node>),
    Builtin(BuiltinId, Vec<Node>),
    /// A library function taking a user function first: `integral(f, a, b)`.
    BuiltinWithFn(BuiltinId, FnId, Vec<Node>),
    CallUser(FnId, Vec<Node>),
    ListComp(Loop),
    Reduce(Reducer, Loop),
    VecX(Box<Node>),
    VecY(Box<Node>),
    Field(Box<Node>, String),
    Method(Box<Node>, String, Vec<Node>),
    IndexList(Box<Node>, Box<Node>),
    IndexMat(Box<Node>, Box<Node>),
    Coerce(Box<Node>, Coercion),
}
