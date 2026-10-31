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

impl Error {
    /// The error as a person reads it: the code and message, the formula
    /// with a caret under the span, then the hint and the options.
    ///
    /// ```text
    /// E102: `^` is exclusive-or, not a power
    ///   2^3
    ///    ^
    ///   hint: write a power as `**`, as in `x**2`
    /// ```
    pub fn render(&self) -> String {
        let mut out = format!("{}: {}", self.code, self.message);
        if let Some(path) = &self.path {
            out.push_str(&format!("\n  at {path}"));
        }
        if let Some(formula) = &self.formula {
            out.push_str(&format!("\n  {formula}"));
            if let Some(span) = self.span {
                let lead = formula
                    .get(..span.start.min(formula.len()))
                    .map_or(0, |s| s.chars().count());
                let width = formula
                    .get(span.start.min(formula.len())..span.end.min(formula.len()))
                    .map_or(1, |s| s.chars().count())
                    .max(1);
                out.push_str(&format!("\n  {}{}", " ".repeat(lead), "^".repeat(width)));
            }
        }
        if let Some(hint) = &self.hint {
            out.push_str(&format!("\n  hint: {hint}"));
        }
        if !self.options.is_empty() {
            out.push_str(&format!("\n  options: {}", self.options.join(", ")));
        }
        out
    }
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

/// One entry of the catalogue `hapto errors` prints.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Entry {
    pub code: Code,
    pub title: &'static str,
    /// A formula that triggers it.
    pub example: &'static str,
}

/// The formula-language part of the catalogue (SPEC-2.0 §18.3). hapto-core
/// adds the document, value, rendering and book codes.
pub const CATALOGUE: &[Entry] = &[
    Entry {
        code: codes::E101,
        title: "unexpected token",
        example: "x y",
    },
    Entry {
        code: codes::E102,
        title: "`^` used for power",
        example: "2^3",
    },
    Entry {
        code: codes::E103,
        title: "implicit multiplication",
        example: "2x",
    },
    Entry {
        code: codes::E104,
        title: "unclosed bracket or quote",
        example: "sin(x",
    },
    Entry {
        code: codes::E105,
        title: "chained comparison",
        example: "0 < x < 1",
    },
    Entry {
        code: codes::E106,
        title: "non-ASCII operator",
        example: "x²",
    },
    Entry {
        code: codes::E107,
        title: "interpolation inside TeX",
        example: "$x = {x}$",
    },
    Entry {
        code: codes::E201,
        title: "type mismatch",
        example: "sqrt([1, 2])",
    },
    Entry {
        code: codes::E202,
        title: "unknown function",
        example: "sqtr(2)",
    },
    Entry {
        code: codes::E203,
        title: "wrong argument count",
        example: "sqrt(1, 2)",
    },
    Entry {
        code: codes::E204,
        title: "operator not defined for these types",
        example: "[1, 2] * [3, 4]",
    },
    Entry {
        code: codes::E205,
        title: "`if` branches differ",
        example: "if x > 0 then 1 else \"no\"",
    },
    Entry {
        code: codes::E206,
        title: "condition not bool",
        example: "if x then 1 else 2",
    },
    Entry {
        code: codes::E207,
        title: "no such field",
        example: "[1, 2].z",
    },
    Entry {
        code: codes::E301,
        title: "unknown name",
        example: "slop * 2",
    },
    Entry {
        code: codes::E302,
        title: "reserved word used as a name",
        example: "let pi: 3",
    },
    Entry {
        code: codes::E303,
        title: "illegal shadowing",
        example: "let x: 3 (x is a value)",
    },
    Entry {
        code: codes::E304,
        title: "cycle in lets",
        example: "let a: b, b: a",
    },
    Entry {
        code: codes::E305,
        title: "recursion",
        example: "let f(n): f(n - 1)",
    },
    Entry {
        code: codes::E306,
        title: "duplicate name",
        example: "let f(t, t): t",
    },
    Entry {
        code: codes::E604,
        title: "evaluation over budget",
        example: "count(range(0, 1e9))",
    },
    Entry {
        code: codes::W101,
        title: "float equality",
        example: "x == 0.1",
    },
];

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module::{Declarations, Module};
    use crate::types::Type;

    #[test]
    fn rendering_puts_a_caret_under_the_span() {
        let err = crate::parse("1 + 2^3").unwrap_err();
        let text = err.render();
        assert_eq!(
            text,
            "E102: `^` is exclusive-or, not a power\n  1 + 2^3\n       ^\n  hint: write a power as `**`, as in `x**2`"
        );
    }

    #[test]
    fn the_caret_counts_characters_not_bytes() {
        let err = crate::parse("\"é\" + x²").unwrap_err();
        let text = err.render();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[2].trim_end(), "         ^");
    }

    #[test]
    fn options_and_paths_are_rendered() {
        let err = Error::new(codes::E201, "bad")
            .options(["a", "b"])
            .at_path("/let/k");
        assert_eq!(err.render(), "E201: bad\n  at /let/k\n  options: a, b");
    }

    /// Every formula code in the catalogue is triggered by its own example,
    /// with a span to point at and a hint that says what to do.
    #[test]
    fn every_catalogue_example_triggers_its_code() {
        let decls = Declarations {
            values: vec![("x".into(), Type::Num)],
            ..Default::default()
        };
        for entry in CATALOGUE {
            let found: Error = match entry.code.0 {
                // Declaration codes come from building a module.
                "E302" => module_error(&[("pi", "3")]),
                "E303" => module_error(&[("x", "3")]),
                "E304" => module_error(&[("a", "b"), ("b", "a")]),
                "E305" => module_error(&[("f(n)", "f(n - 1)")]),
                "E306" => module_error(&[("f(t, t)", "t")]),
                // Interpolation in TeX and the budget belong to later stages.
                "E107" | "E604" => continue,
                _ => {
                    let mut module = Module::new(&decls).unwrap();
                    match module.compile(entry.example, &[], None) {
                        Err(e) => e,
                        Ok(checked) => match checked.warnings.into_iter().next() {
                            Some(w) => w,
                            None => panic!(
                                "{}: `{}` did not trigger anything",
                                entry.code, entry.example
                            ),
                        },
                    }
                }
            };
            assert_eq!(
                found.code,
                entry.code,
                "`{}`: {}",
                entry.example,
                found.render()
            );
            assert!(
                found.hint.is_some() || !found.options.is_empty(),
                "{} has no way out: {}",
                entry.code,
                found.render()
            );
            if !entry.code.0.starts_with("E3") || entry.code.0 == "E301" {
                assert!(
                    found.span.is_some(),
                    "{} has no span: {}",
                    entry.code,
                    found.render()
                );
            }
        }
    }

    fn module_error(lets: &[(&str, &str)]) -> Error {
        let decls = Declarations {
            values: vec![("x".into(), Type::Num)],
            compute: vec![],
            lets: lets
                .iter()
                .map(|(k, f)| (k.to_string(), f.to_string()))
                .collect(),
        };
        Module::new(&decls).unwrap_err().remove(0)
    }
}
