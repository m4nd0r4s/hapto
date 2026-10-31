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

pub mod error;
pub mod span;

pub use error::{Code, Error, Severity};
pub use span::Span;
