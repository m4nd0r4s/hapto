//! The standard library: every function a formula can call (SPEC-2.0 §4.6).
//!
//! Each builtin declares its overloads as patterns over types, so the checker
//! can pick one, coerce the arguments it needs to (a pair into a list), and
//! say exactly what it accepts when nothing fits. Implementations receive the
//! evaluated arguments and a [`Host`] that meters their work, supplies
//! palette colours and calls user functions for the calculus builtins.

mod basic;
mod list;
#[cfg(test)]
pub(crate) mod testing;
mod vector;

use std::sync::{Arc, OnceLock};

use crate::error::Error;
use crate::ir::FnId;
use crate::types::{RecordType, Type};
use crate::value::Value;

/// What a builtin can ask of the evaluator while it runs.
pub trait Host {
    /// Spend `ops` units of the evaluation budget; fails with E604 when it is
    /// exhausted, so no formula can hang a page.
    fn charge(&mut self, ops: u64) -> Result<(), Error>;

    /// A theme colour by token name (`"hot"`, `"cool"`, …).
    fn palette(&self, name: &str) -> Option<[f64; 4]>;

    /// Call a user function of one number, for `derivative`, `integral`, …
    fn call_fn(&mut self, f: FnId, x: f64) -> Result<f64, Error>;
}

/// How a builtin computes. `f` is the user function for builtins that take
/// one (`integral(f, a, b)`), and `None` otherwise.
pub type Impl = fn(host: &mut dyn Host, f: Option<FnId>, args: &[Value]) -> Result<Value, Error>;

/// One parameter or result position in a signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pat {
    Num,
    Bool,
    Str,
    Vec,
    Colour,
    Mat,
    ListNum,
    ListVec,
    ListListNum,
    /// A list of anything; binds `T` to its element type.
    ListT,
    /// Any type; binds `T`.
    T,
}

/// What a builtin returns.
#[derive(Debug, Clone, Copy)]
pub enum Ret {
    Pat(Pat),
    Record(fn() -> Arc<RecordType>),
}

#[derive(Debug, Clone, Copy)]
pub struct Overload {
    pub params: &'static [Pat],
    /// Further arguments, any number of them, each matching this pattern.
    pub rest: Option<Pat>,
    pub ret: Ret,
}

impl Overload {
    pub const fn new(params: &'static [Pat], ret: Pat) -> Self {
        Overload {
            params,
            rest: None,
            ret: Ret::Pat(ret),
        }
    }

    pub const fn variadic(params: &'static [Pat], rest: Pat, ret: Pat) -> Self {
        Overload {
            params,
            rest: Some(rest),
            ret: Ret::Pat(ret),
        }
    }

    pub const fn record(params: &'static [Pat], ret: fn() -> Arc<RecordType>) -> Self {
        Overload {
            params,
            rest: None,
            ret: Ret::Record(ret),
        }
    }

    /// How the signature reads in an error message: `(num, num) -> num`.
    pub fn describe(&self) -> String {
        let mut parts: Vec<String> = self
            .params
            .iter()
            .map(|p| pat_name(*p).to_string())
            .collect();
        if let Some(rest) = self.rest {
            parts.push(format!("{}…", pat_name(rest)));
        }
        let ret = match self.ret {
            Ret::Pat(p) => pat_name(p).to_string(),
            Ret::Record(r) => format!("{} record", r().name),
        };
        format!("({}) -> {ret}", parts.join(", "))
    }
}

fn pat_name(p: Pat) -> &'static str {
    match p {
        Pat::Num => "num",
        Pat::Bool => "bool",
        Pat::Str => "str",
        Pat::Vec => "vec",
        Pat::Colour => "colour",
        Pat::Mat => "mat",
        Pat::ListNum => "list<num>",
        Pat::ListVec => "list<vec>",
        Pat::ListListNum => "list<list<num>>",
        Pat::ListT => "list<T>",
        Pat::T => "T",
    }
}

/// One library function.
#[derive(Debug, Clone, Copy)]
pub struct Builtin {
    pub name: &'static str,
    pub overloads: &'static [Overload],
    /// Takes a user function name as its first argument (`integral(f, a, b)`).
    pub takes_fn: bool,
    pub imp: Impl,
}

/// The whole library, in a fixed order: a builtin's id is its index.
pub fn library() -> &'static [Builtin] {
    static LIBRARY: OnceLock<Vec<Builtin>> = OnceLock::new();
    LIBRARY.get_or_init(|| {
        let mut all = Vec::new();
        all.extend_from_slice(basic::BUILTINS);
        all.extend_from_slice(vector::BUILTINS);
        all.extend_from_slice(list::BUILTINS);
        all
    })
}

pub fn lookup(name: &str) -> Option<(usize, &'static Builtin)> {
    library().iter().enumerate().find(|(_, b)| b.name == name)
}

pub fn get(id: usize) -> &'static Builtin {
    &library()[id]
}

/// Every library name, which a figure may not reuse (§4.1).
pub fn names() -> impl Iterator<Item = &'static str> {
    library().iter().map(|b| b.name)
}

/// The type a pattern stands for, given what `T` is bound to.
pub fn concrete(p: Pat, t: &Option<Type>) -> Option<Type> {
    Some(match p {
        Pat::Num => Type::Num,
        Pat::Bool => Type::Bool,
        Pat::Str => Type::Str,
        Pat::Vec => Type::Vec,
        Pat::Colour => Type::Colour,
        Pat::Mat => Type::Mat,
        Pat::ListNum => Type::list(Type::Num),
        Pat::ListVec => Type::list(Type::Vec),
        Pat::ListListNum => Type::list(Type::list(Type::Num)),
        Pat::ListT => Type::list(t.clone()?),
        Pat::T => t.clone()?,
    })
}

// --- small helpers the implementations share ---

/// The numeric arguments, in order (NaN where an argument is no value).
pub(crate) fn nums(args: &[Value]) -> Vec<f64> {
    args.iter().map(Value::num).collect()
}

/// A number result, with non-finite results as no value.
pub(crate) fn num(n: f64) -> Result<Value, Error> {
    Ok(Value::Num(if n.is_finite() { n } else { f64::NAN }))
}
