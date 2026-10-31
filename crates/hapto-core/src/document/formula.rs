//! Formulas as written in a document, with where they were written.
//!
//! Where a formula is expected, a literal number, flag or list is accepted
//! too and means the constant (SPEC-2.0 §6.1): `r: 4` and `r: "4"` are the
//! same, and `at: [0, "h"]` is the formula `[0, h]`.

use serde_json::Value as Tree;

use crate::tree::{index_path, At, Reader};

/// A formula's text and its JSON Pointer in the document.
#[derive(Debug, Clone, PartialEq)]
pub struct Formula {
    pub text: String,
    pub path: String,
}

impl Formula {
    pub fn new(text: impl Into<String>, path: impl Into<String>) -> Self {
        Formula {
            text: text.into(),
            path: path.into(),
        }
    }
}

/// The text of a number that reads back as the same number.
pub fn number_text(n: f64) -> String {
    format!("{n}")
}

/// Read a formula, accepting literals where a formula is expected.
pub fn formula(r: &mut Reader, at: At, what: &str) -> Option<Formula> {
    let text = formula_text(r, at, what)?;
    Some(Formula::new(text, at.path))
}

fn formula_text(r: &mut Reader, at: At, what: &str) -> Option<String> {
    match at.tree {
        Tree::String(s) => Some(s.clone()),
        Tree::Number(n) => n.as_f64().map(number_text),
        Tree::Bool(b) => Some(b.to_string()),
        Tree::Array(items) => {
            let mut parts = Vec::new();
            for (i, item) in items.iter().enumerate() {
                let path = index_path(at.path, i);
                parts.push(formula_text(
                    r,
                    At {
                        tree: item,
                        path: &path,
                    },
                    what,
                )?);
            }
            Some(format!("[{}]", parts.join(", ")))
        }
        _ => {
            r.wrong_type(at, &format!("a formula ({what})"));
            None
        }
    }
}

/// The colour tokens of the theme (§9.1).
pub const COLOUR_TOKENS: &[&str] = &[
    "fg",
    "muted",
    "faint",
    "bg",
    "accent",
    "hot",
    "cool",
    "positive",
    "negative",
    "highlight",
    "c1",
    "c2",
    "c3",
    "c4",
    "c5",
    "c6",
    "c7",
    "c8",
];

/// A colour: a theme token (`hot`), a hex literal, or a formula.
pub fn colour(r: &mut Reader, at: At, what: &str) -> Option<Formula> {
    if let Tree::String(s) = at.tree {
        if COLOUR_TOKENS.contains(&s.as_str()) {
            return Some(Formula::new(format!("palette(\"{s}\")"), at.path));
        }
    }
    formula(r, at, what)
}

/// The named stroke weights, in CSS pixels (§9.2).
pub const WEIGHTS: &[(&str, f64)] = &[("hair", 1.0), ("line", 2.0), ("heavy", 3.5)];

/// A width in CSS pixels: a named weight, a number or a formula.
pub fn width(r: &mut Reader, at: At, what: &str) -> Option<Formula> {
    if let Tree::String(s) = at.tree {
        if let Some((_, w)) = WEIGHTS.iter().find(|(n, _)| n == s) {
            return Some(Formula::new(number_text(*w), at.path));
        }
    }
    formula(r, at, what)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(json: &str, f: fn(&mut Reader, At, &str) -> Option<Formula>) -> Option<Formula> {
        let tree: Tree = serde_json::from_str(json).unwrap();
        f(
            &mut Reader::default(),
            At {
                tree: &tree,
                path: "/x",
            },
            "test",
        )
    }

    #[test]
    fn literals_are_formulas() {
        assert_eq!(read("4", formula).unwrap().text, "4");
        assert_eq!(read("0.5", formula).unwrap().text, "0.5");
        assert_eq!(read("true", formula).unwrap().text, "true");
        assert_eq!(read("[0, \"h\"]", formula).unwrap().text, "[0, h]");
        assert_eq!(
            read("[[1, 2], [3, 4]]", formula).unwrap().text,
            "[[1, 2], [3, 4]]"
        );
        assert_eq!(read("\"x**2\"", formula).unwrap().path, "/x");
        assert!(read("{}", formula).is_none());
    }

    #[test]
    fn tokens_and_weights() {
        assert_eq!(read("\"hot\"", colour).unwrap().text, "palette(\"hot\")");
        assert_eq!(read("\"#d55e00\"", colour).unwrap().text, "#d55e00");
        assert_eq!(
            read("\"mix(a, b, t)\"", colour).unwrap().text,
            "mix(a, b, t)"
        );
        assert_eq!(read("\"heavy\"", width).unwrap().text, "3.5");
        assert_eq!(read("\"2 + x\"", width).unwrap().text, "2 + x");
    }
}
