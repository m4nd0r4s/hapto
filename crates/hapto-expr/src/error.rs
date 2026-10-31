//! Errors that say what to do.
//!
//! Every mistake a formula can make has a stable code (SPEC-2.0 §18.3), a
//! message naming the thing that is wrong, the span it is in, and — when the
//! fix is mechanical — the fix. The codes never change meaning, so a book
//! build, an agent and a test can all match on them.

use std::fmt;

use serde::Serialize;

use crate::span::Span;

/// A stable error or warning code, such as `E102`.
///
/// Errors go out (to a page, an agent, CI) but never come back in, so they
/// serialise and do not deserialise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct Code(pub &'static str);

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// Whether a finding stops the figure or only advises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

/// One finding about a formula.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Error {
    pub code: Code,
    pub severity: Severity,
    pub message: String,
    /// The formula the span indexes into, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub formula: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    /// The way out, when there is a mechanical one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// The valid choices, when there are few.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    /// A JSON Pointer into the figure document, filled in by whoever owns the
    /// document (hapto-core); a bare formula has none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl Error {
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Error {
            code,
            severity: Severity::Error,
            message: message.into(),
            formula: None,
            span: None,
            hint: None,
            options: Vec::new(),
            path: None,
        }
    }

    pub fn warning(code: Code, message: impl Into<String>) -> Self {
        Error {
            severity: Severity::Warning,
            ..Error::new(code, message)
        }
    }

    pub fn at(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn options<I, S>(mut self, options: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.options = options.into_iter().map(Into::into).collect();
        self
    }

    pub fn in_formula(mut self, formula: &str) -> Self {
        self.formula = Some(formula.to_string());
        self
    }

    pub fn at_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

/// Every code the formula language and the figure format can report.
///
/// The catalogue is SPEC-2.0 §18.3. A code never changes meaning once
/// released; a new mistake gets a new code.
pub mod codes {
    use super::Code;

    // E1xx — formula syntax
    pub const E101: Code = Code("E101"); // unexpected token
    pub const E102: Code = Code("E102"); // `^` used for power
    pub const E103: Code = Code("E103"); // implicit multiplication
    pub const E104: Code = Code("E104"); // unclosed bracket or quote
    pub const E105: Code = Code("E105"); // chained comparison
    pub const E106: Code = Code("E106"); // non-ASCII operator
    pub const E107: Code = Code("E107"); // interpolation inside TeX

    // E2xx — types
    pub const E201: Code = Code("E201"); // type mismatch
    pub const E202: Code = Code("E202"); // unknown function
    pub const E203: Code = Code("E203"); // wrong argument count
    pub const E204: Code = Code("E204"); // operator not defined for these types
    pub const E205: Code = Code("E205"); // `if` branches differ
    pub const E206: Code = Code("E206"); // condition not bool
    pub const E207: Code = Code("E207"); // no such field

    // E3xx — names and scope
    pub const E301: Code = Code("E301"); // unknown name
    pub const E302: Code = Code("E302"); // reserved word used as a name
    pub const E303: Code = Code("E303"); // illegal shadowing
    pub const E304: Code = Code("E304"); // cycle in lets
    pub const E305: Code = Code("E305"); // recursion
    pub const E306: Code = Code("E306"); // duplicate name

    // E6xx — evaluation
    pub const E604: Code = Code("E604"); // evaluation over budget

    // Warnings
    pub const W101: Code = Code("W101"); // float equality
}
