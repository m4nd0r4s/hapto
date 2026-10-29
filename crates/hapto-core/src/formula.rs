//! Reading a written formula into a `Num`.
//!
//! A lesson says `x**2 - x*(2)` and means a curve the reader can move. The
//! book has those strings already — they were written for a Python emitter
//! that pasted them into generated code — so hapto reads the same dialect
//! rather than asking anyone to rewrite them.
//!
//! What it accepts is deliberately small: numbers, the named values of the
//! figure, `+ - * / **` with the usual precedence, unary minus, brackets,
//! and the handful of functions the book's own validator allows (`abs`,
//! `min`, `max`, `sqrt`, `sin`, `cos`). Anything else is an error that says
//! what it found, because a formula that silently parses wrong is how `2^3`
//! once got graded as 1.

use std::collections::BTreeMap;

use crate::build::Num;

/// Read `text` into a number, with `values` naming what may appear in it.
pub fn parse(text: &str, values: &BTreeMap<String, Num>) -> Result<Num, String> {
    let tokens = lex(text)?;
    let mut parser = Parser {
        tokens: &tokens,
        at: 0,
        values,
        text,
    };
    let out = parser.expression()?;
    match parser.peek() {
        None => Ok(out),
        Some(extra) => Err(parser.complain(&format!("{extra} was not expected here"))),
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Name(String),
    Plus,
    Minus,
    Star,
    Slash,
    Power,
    Open,
    Close,
    Comma,
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Token::Number(n) => write!(f, "{n}"),
            Token::Name(n) => write!(f, "\"{n}\""),
            Token::Plus => write!(f, "+"),
            Token::Minus => write!(f, "-"),
            Token::Star => write!(f, "*"),
            Token::Slash => write!(f, "/"),
            Token::Power => write!(f, "**"),
            Token::Open => write!(f, "("),
            Token::Close => write!(f, ")"),
            Token::Comma => write!(f, ","),
        }
    }
}

fn lex(text: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        let digit_next = chars.get(i + 1).is_some_and(char::is_ascii_digit);
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || (c == '.' && digit_next) {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            let found: String = chars[start..i].iter().collect();
            let n = found
                .parse::<f64>()
                .map_err(|_| format!("in the formula \"{text}\": \"{found}\" is not a number"))?;
            tokens.push(Token::Number(n));
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            tokens.push(Token::Name(chars[start..i].iter().collect()));
        } else {
            i += 1;
            match c {
                '+' => tokens.push(Token::Plus),
                '-' => tokens.push(Token::Minus),
                '/' => tokens.push(Token::Slash),
                '(' => tokens.push(Token::Open),
                ')' => tokens.push(Token::Close),
                ',' => tokens.push(Token::Comma),
                '*' => {
                    if chars.get(i) == Some(&'*') {
                        i += 1;
                        tokens.push(Token::Power);
                    } else {
                        tokens.push(Token::Star);
                    }
                }
                // `^` is exclusive-or in Python, not a power, and a formula
                // written with it has already been graded wrongly once. It is
                // refused rather than guessed at.
                '^' => {
                    return Err(format!(
                        "in the formula \"{text}\": ^ is exclusive-or, not a power. \
                         Write a power as **."
                    ))
                }
                other => {
                    return Err(format!(
                        "in the formula \"{text}\": {other:?} is not something a \
                         formula here may use"
                    ))
                }
            }
        }
    }
    Ok(tokens)
}

struct Parser<'a> {
    tokens: &'a [Token],
    at: usize,
    values: &'a BTreeMap<String, Num>,
    text: &'a str,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&'a Token> {
        self.tokens.get(self.at)
    }

    fn take(&mut self) -> Option<&'a Token> {
        let out = self.tokens.get(self.at);
        if out.is_some() {
            self.at += 1;
        }
        out
    }

    fn eat(&mut self, token: &Token) -> bool {
        if self.peek() == Some(token) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn complain(&self, what: &str) -> String {
        format!("in the formula \"{}\": {what}", self.text)
    }

    /// `a + b - c`, lowest precedence.
    fn expression(&mut self) -> Result<Num, String> {
        let mut left = self.term()?;
        loop {
            if self.eat(&Token::Plus) {
                left = left.plus(self.term()?);
            } else if self.eat(&Token::Minus) {
                left = left.minus(self.term()?);
            } else {
                return Ok(left);
            }
        }
    }

    /// `a * b / c`.
    fn term(&mut self) -> Result<Num, String> {
        let mut left = self.unary()?;
        loop {
            if self.eat(&Token::Star) {
                left = left.times(self.unary()?);
            } else if self.eat(&Token::Slash) {
                left = left.over(self.unary()?);
            } else {
                return Ok(left);
            }
        }
    }

    /// `-a`, and `+a` for symmetry.
    fn unary(&mut self) -> Result<Num, String> {
        if self.eat(&Token::Minus) {
            return Ok(self.unary()?.negated());
        }
        if self.eat(&Token::Plus) {
            return self.unary();
        }
        self.power()
    }

    /// `a ** b`, which binds tighter than `*` and groups to the right.
    fn power(&mut self) -> Result<Num, String> {
        let base = self.atom()?;
        if self.eat(&Token::Power) {
            // Right-associative, and the exponent may itself be negated:
            // `x ** -1` is a perfectly ordinary curve.
            return Ok(base.pow(self.unary()?));
        }
        Ok(base)
    }

    /// A number, a named value, a bracketed expression, or a call.
    fn atom(&mut self) -> Result<Num, String> {
        match self.take() {
            Some(Token::Number(n)) => Ok(Num::Fixed(*n)),
            Some(Token::Open) => {
                let inside = self.expression()?;
                if !self.eat(&Token::Close) {
                    return Err(self.complain("a bracket was opened and never closed"));
                }
                Ok(inside)
            }
            Some(Token::Name(name)) => {
                if self.eat(&Token::Open) {
                    return self.call(name);
                }
                self.values.get(name).cloned().ok_or_else(|| {
                    let known: Vec<&str> = self.values.keys().map(String::as_str).collect();
                    self.complain(&format!(
                        "\"{name}\" is not a value this figure has. It has: {}",
                        if known.is_empty() {
                            "none".to_string()
                        } else {
                            known.join(", ")
                        }
                    ))
                })
            }
            Some(other) => Err(self.complain(&format!("{other} is not the start of a value"))),
            None => Err(self.complain("it ends where a value was expected")),
        }
    }

    /// `abs(x)`, `min(a, b)` and the rest.
    fn call(&mut self, name: &str) -> Result<Num, String> {
        let mut args = Vec::new();
        if !self.eat(&Token::Close) {
            loop {
                args.push(self.expression()?);
                if self.eat(&Token::Comma) {
                    continue;
                }
                if self.eat(&Token::Close) {
                    break;
                }
                return Err(self.complain(&format!("{name}( was opened and never closed")));
            }
        }

        let count = args.len();
        let wrong = |wanted: usize| {
            self.complain(&format!(
                "{name} takes {wanted} argument{}, and was given {count}",
                if wanted == 1 { "" } else { "s" }
            ))
        };

        match (name, args.len()) {
            ("abs", 1) => Ok(args.remove(0).abs()),
            ("sqrt", 1) => Ok(args.remove(0).sqrt()),
            ("sin", 1) => Ok(args.remove(0).sin()),
            ("cos", 1) => Ok(args.remove(0).cos()),
            ("min", 2) => {
                let b = args.remove(1);
                Ok(args.remove(0).min(b))
            }
            ("max", 2) => {
                let b = args.remove(1);
                Ok(args.remove(0).max(b))
            }
            ("abs" | "sqrt" | "sin" | "cos", _) => Err(wrong(1)),
            ("min" | "max", _) => Err(wrong(2)),
            (other, _) => Err(self.complain(&format!(
                "\"{other}\" is not a function a formula here may call. \
                 It may call: abs, sqrt, sin, cos, min, max"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::Build;

    /// What a formula works out to, with `x` at a given value.
    ///
    /// `Num` is an expression and nothing can read one directly, so the
    /// scene's own evaluator is the oracle — the same machinery that draws.
    fn value_of(formula: &str, x_at: f64) -> f64 {
        let mut build = Build::new();
        let x = build.value("x", x_at);
        let mut values = BTreeMap::new();
        values.insert("x".to_string(), x);
        let out = parse(formula, &values).unwrap();
        build.probe(out)
    }

    #[test]
    fn arithmetic_comes_out_right() {
        assert_eq!(value_of("1 + 2 * 3", 0.0), 7.0);
        assert_eq!(value_of("(1 + 2) * 3", 0.0), 9.0);
        assert_eq!(value_of("10 / 4", 0.0), 2.5);
        assert_eq!(value_of("-3 + 1", 0.0), -2.0);
    }

    #[test]
    fn a_power_binds_tighter_than_a_product() {
        // 2 * 3**2 is 18, not 36.
        assert_eq!(value_of("2 * 3**2", 0.0), 18.0);
        // and groups to the right: 2**3**2 is 2**9.
        assert_eq!(value_of("2**3**2", 0.0), 512.0);
    }

    #[test]
    fn a_named_value_reaches_the_result() {
        // The curve a real lesson carries.
        assert_eq!(value_of("x**2 - x*(2)", 3.0), 3.0);
        assert_eq!(value_of("x**2 - x*(2)", 0.0), 0.0);
        assert_eq!(value_of("x**2 - x*(2)", 2.0), 0.0);
    }

    #[test]
    fn the_functions_the_book_allows() {
        assert_eq!(value_of("abs(0 - 4)", 0.0), 4.0);
        assert_eq!(value_of("sqrt(9)", 0.0), 3.0);
        assert_eq!(value_of("min(2, 5)", 0.0), 2.0);
        assert_eq!(value_of("max(2, 5)", 0.0), 5.0);
        // The determinant readout of a real lesson.
        assert_eq!(value_of("abs(x - 4)", 1.0), 3.0);
    }

    #[test]
    fn a_caret_is_refused_rather_than_guessed() {
        let values = BTreeMap::new();
        let err = parse("2^3", &values).unwrap_err();
        assert!(err.contains("**"), "{err}");
    }

    #[test]
    fn an_unknown_name_says_what_the_figure_has() {
        let mut build = Build::new();
        let x = build.value("x", 1.0);
        let mut values = BTreeMap::new();
        values.insert("x".to_string(), x);

        let err = parse("y + 1", &values).unwrap_err();
        assert!(err.contains("\"y\""), "{err}");
        assert!(err.contains("x"), "{err}");
    }

    #[test]
    fn an_unknown_function_lists_the_ones_allowed() {
        let values = BTreeMap::new();
        let err = parse("tan(1)", &values).unwrap_err();
        assert!(err.contains("tan"), "{err}");
        assert!(err.contains("sqrt"), "{err}");
    }

    #[test]
    fn a_wrong_argument_count_says_so() {
        let values = BTreeMap::new();
        let err = parse("min(1)", &values).unwrap_err();
        assert!(err.contains("2 arguments"), "{err}");
    }

    #[test]
    fn an_unclosed_bracket_says_so() {
        let values = BTreeMap::new();
        let err = parse("2 * (1 + 3", &values).unwrap_err();
        assert!(err.contains("closed"), "{err}");
    }

    #[test]
    fn trailing_rubbish_is_not_ignored() {
        let values = BTreeMap::new();
        let err = parse("1 + 2)", &values).unwrap_err();
        assert!(err.contains("not expected"), "{err}");
    }
}
