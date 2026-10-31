//! The formula language of hapto.
//!
//! A figure says what it draws in formulas — `x**2 - 2*x`, `[x, f(x)]`,
//! `if zeta < 1 then "underdamped" else "overdamped"` — and every number a
//! reader sees, drawn or printed, comes from one. This crate reads them,
//! checks them against the figure's values before anything is drawn, and
//! evaluates them natively, the same way on every target.
//!
//! The language is specified in `docs/SPEC-2.0.md` §4, with the grammar in its
//! Appendix B. It has no kinemo dependency: hapto-core turns evaluated values
//! into a scene, this crate only says what they are.

// An `Error` carries a message, a hint, options and a span, which makes it
// large next to an `Ok`. Errors happen when a document is loaded, never per
// frame, so the size costs nothing that matters; boxing would only make every
// field one dereference further from the code that builds and tests it.
#![allow(clippy::result_large_err)]

pub mod ast;
pub mod builtins;
pub mod check;
mod colour;
pub mod deps;
pub mod error;
pub mod eval;
pub mod format;
pub mod ir;
pub mod lexer;
mod linalg;
pub mod module;
pub mod parser;
pub mod span;
mod suggest;
pub mod types;
pub mod value;

pub use ast::Expr;
pub use error::{Code, Error, Severity};
pub use eval::{Cache, Env, Evaluation};
pub use module::{Declarations, Module};
pub use parser::parse;
pub use span::Span;
pub use types::Type;
pub use value::Value;
