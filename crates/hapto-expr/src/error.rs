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
