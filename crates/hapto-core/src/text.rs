//! Text with interpolation: `"slope = {s:.2f}"` (SPEC-2.0 §10.2).
//!
//! `{expr}` or `{expr:spec}` outside `$…$` is a formula printed with a
//! Python format spec; `{{` and `}}` are literal braces. Inside TeX, `{` is
//! grouping and nothing is evaluated: a formula-looking `{=…}` or `{x:.2f}`
//! there is refused (E107), because TeX is baked ahead of time and cannot
//! hold a number that moves.

use hapto_expr::error::codes;
use hapto_expr::Error;

/// One piece of a text template.
#[derive(Debug, Clone, PartialEq)]
pub enum Piece {
    Lit(String),
    Expr {
        formula: String,
        spec: Option<String>,
    },
}

/// Split a template into literal text and interpolated formulas.
pub fn parse(template: &str, path: &str) -> Result<Vec<Piece>, Error> {
    let chars: Vec<char> = template.chars().collect();
    let mut out = Vec::new();
    let mut lit = String::new();
    let mut in_tex = false;
    let mut i = 0;
    let refuse = |what: &str| {
        Error::new(
            codes::E107,
            format!(
                "{what} inside TeX: TeX is typeset ahead of time and cannot hold a moving number"
            ),
        )
        .hint("put the number in text beside the maths, as in `$\\frac{a}{b}$ = {x:.2f}`")
        .at_path(path)
        .in_formula(template)
    };
    while i < chars.len() {
        let c = chars[i];
        if c == '$' {
            in_tex = !in_tex;
            lit.push(c);
            i += 1;
            continue;
        }
        if in_tex {
            if c == '{' {
                let rest: String = chars[i + 1..].iter().take_while(|c| **c != '}').collect();
                let spec_like = rest.split_once(':').is_some_and(|(e, s)| {
                    hapto_expr::parse(e).is_ok() && hapto_expr::format::is_valid(s) && !s.is_empty()
                });
                if rest.starts_with('=') || spec_like {
                    return Err(refuse("an interpolation"));
                }
            }
            lit.push(c);
            i += 1;
            continue;
        }
        match c {
            '{' if chars.get(i + 1) == Some(&'{') => {
                lit.push('{');
                i += 2;
            }
            '}' if chars.get(i + 1) == Some(&'}') => {
                lit.push('}');
                i += 2;
            }
            '{' => {
                let end = chars[i + 1..]
                    .iter()
                    .position(|c| *c == '}')
                    .map(|p| i + 1 + p);
                let Some(end) = end else {
                    return Err(Error::new(codes::E104, "this `{` is never closed")
                        .hint("close it with `}`, or write `{{` for a brace")
                        .at_path(path)
                        .in_formula(template));
                };
                let inner: String = chars[i + 1..end].iter().collect();
                if !lit.is_empty() {
                    out.push(Piece::Lit(std::mem::take(&mut lit)));
                }
                // The format spec is after the last `:` that is not part of the formula.
                let (formula, spec) = match inner.rsplit_once(':') {
                    Some((f, s)) if hapto_expr::format::is_valid(s) => {
                        (f.to_string(), Some(s.to_string()))
                    }
                    _ => (inner.clone(), None),
                };
                out.push(Piece::Expr { formula, spec });
                i = end + 1;
            }
            _ => {
                lit.push(c);
                i += 1;
            }
        }
    }
    if !lit.is_empty() {
        out.push(Piece::Lit(lit));
    }
    Ok(out)
}

/// Every `$…$` span of a text, for baking and checking.
pub fn tex_spans(text: &str) -> Vec<String> {
    text.split('$')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolation_and_escapes() {
        let p = parse("slope = {s:.2f} at {x}, {{literal}}", "/t").unwrap();
        assert_eq!(p[0], Piece::Lit("slope = ".into()));
        assert_eq!(
            p[1],
            Piece::Expr {
                formula: "s".into(),
                spec: Some(".2f".into())
            }
        );
        assert_eq!(
            p[3],
            Piece::Expr {
                formula: "x".into(),
                spec: None
            }
        );
        assert_eq!(p[4], Piece::Lit(", {literal}".into()));
    }

    #[test]
    fn tex_is_left_alone_and_never_interpolated() {
        let p = parse("$\\frac{a}{b}$ = {k}", "/t").unwrap();
        assert_eq!(p[0], Piece::Lit("$\\frac{a}{b}$ = ".into()));
        assert_eq!(parse("$x = {=k}$", "/t").unwrap_err().code, codes::E107);
        assert_eq!(parse("$x = {k:.2f}$", "/t").unwrap_err().code, codes::E107);
        assert_eq!(parse("{k", "/t").unwrap_err().code, codes::E104);
        assert_eq!(tex_spans("a $x^2$ and $y$"), vec!["x^2", "y"]);
    }
}
