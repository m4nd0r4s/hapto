//! State in, drawing out: the figure language of hapto.
//!
//! A figure is a document — values, lets, views and marks (SPEC-2.0 §6) —
//! whose drawing is a pure function of its state. This crate reads and
//! validates documents, evaluates them through `hapto-expr`, and rebuilds a
//! kinemo scene from literal props on every change (§3.3).
//!
//! The 1.0 model lives in [`v1`] until the 2.0 language replaces it.

// Errors (`hapto_expr::Error`) carry a message, hint, options and path, which
// makes them large beside an `Ok`. They happen when a document is loaded, not
// per frame, so the size costs nothing that matters.
#![allow(clippy::result_large_err)]

pub mod codes;
pub mod document;
pub mod geom;
pub mod layout;
pub mod marks;
pub mod maths;
pub mod program;
pub mod render;
pub mod scene;
pub mod schema;
pub mod state;
pub mod style;
pub mod text;
pub mod theme;
pub mod ticks;
pub mod tree;
pub mod v1;
