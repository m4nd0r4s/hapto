//! State in, drawing out: the figure language of hapto.
//!
//! A figure is a document — values, lets, views and marks (SPEC-2.0 §6) —
//! whose drawing is a pure function of its state. This crate reads and
//! validates documents, evaluates them through `hapto-expr`, and rebuilds a
//! kinemo scene from literal props on every change (§3.3).
//!
//! The 1.0 model lives in [`v1`] until the 2.0 language replaces it.

pub mod v1;
