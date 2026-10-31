//! Turning a formula's text into tokens.
//!
//! The lexer accepts exactly the tokens of SPEC-2.0 Appendix B, and it also
//! recognises the things people and agents commonly type instead — `2^3`,
//! `2x`, `x²`, `θ`, a lone `=` — so that each gets an error that names the
//! spelling to use rather than a generic "unexpected character". `2^3` read as
//! exclusive-or once graded a lesson wrongly; that is why `^` is never quietly
//! accepted.

use crate::error::{codes, Error};
use crate::span::Span;

/// What a token is.
#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Number(f64),
    Str(String),
    /// sRGB with straight alpha, components in 0..=1.
    Colour([f64; 4]),
    Ident(String),
    True,
    False,
    If,
    Then,
    Else,
    And,
    Or,
    Not,
    For,
    In,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Dot,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Pow,
    Lt,
    Le,
    Gt,
    Ge,
    EqEq,
    Ne,
}

impl Tok {
    /// How the token reads in an error message.
    pub fn describe(&self) -> String {
        match self {
            Tok::Number(n) => format!("the number {n}"),
            Tok::Str(s) => format!("the string \"{s}\""),
            Tok::Colour(_) => "a colour".into(),
            Tok::Ident(name) => format!("\"{name}\""),
            other => format!("`{}`", other.text()),
        }
    }

    /// The source spelling of a fixed token.
    pub fn text(&self) -> &'static str {
        match self {
            Tok::True => "true",
            Tok::False => "false",
            Tok::If => "if",
            Tok::Then => "then",
            Tok::Else => "else",
            Tok::And => "and",
            Tok::Or => "or",
            Tok::Not => "not",
            Tok::For => "for",
            Tok::In => "in",
            Tok::LParen => "(",
            Tok::RParen => ")",
            Tok::LBracket => "[",
            Tok::RBracket => "]",
            Tok::Comma => ",",
            Tok::Dot => ".",
            Tok::Plus => "+",
            Tok::Minus => "-",
            Tok::Star => "*",
            Tok::Slash => "/",
            Tok::Percent => "%",
            Tok::Pow => "**",
            Tok::Lt => "<",
            Tok::Le => "<=",
            Tok::Gt => ">",
            Tok::Ge => ">=",
            Tok::EqEq => "==",
            Tok::Ne => "!=",
            Tok::Number(_) | Tok::Str(_) | Tok::Colour(_) | Tok::Ident(_) => "",
        }
    }
}

/// A token and where it is.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

/// Every word the grammar reserves for itself. Constants and library
/// functions are reserved too, but they lex as identifiers (§4.1).
pub const KEYWORDS: &[&str] = &[
    "if", "then", "else", "and", "or", "not", "for", "in", "true", "false",
];

/// Read `text` into tokens, or say what is wrong with the first bad one.
pub fn lex(text: &str) -> Result<Vec<Token>, Error> {
    Lexer {
        text,
        bytes: text.as_bytes(),
        at: 0,
        tokens: Vec::new(),
    }
    .run()
    .map_err(|err| err.in_formula(text))
}

struct Lexer<'a> {
    text: &'a str,
    bytes: &'a [u8],
    at: usize,
    tokens: Vec<Token>,
}

impl Lexer<'_> {
    fn run(mut self) -> Result<Vec<Token>, Error> {
        while let Some(c) = self.peek() {
            let start = self.at;
            match c {
                b' ' | b'\t' | b'\n' | b'\r' => self.at += 1,
                b'0'..=b'9' => self.number()?,
                b'.' if self.peek_at(1).is_some_and(|d| d.is_ascii_digit()) => self.number()?,
                b'"' | b'\'' => self.string(c)?,
                b'#' => self.colour()?,
                b'a'..=b'z' | b'A'..=b'Z' => self.word(),
                b'_' => {
                    return Err(Error::new(codes::E101, "a name cannot start with `_`")
                        .at(Span::new(start, start + 1))
                        .hint("start the name with a letter, as in `x_f`"));
                }
                b'*' if self.peek_at(1) == Some(b'*') => self.push(Tok::Pow, 2),
                b'<' if self.peek_at(1) == Some(b'=') => self.push(Tok::Le, 2),
                b'>' if self.peek_at(1) == Some(b'=') => self.push(Tok::Ge, 2),
                b'=' if self.peek_at(1) == Some(b'=') => self.push(Tok::EqEq, 2),
                b'!' if self.peek_at(1) == Some(b'=') => self.push(Tok::Ne, 2),
                b'(' => self.push(Tok::LParen, 1),
                b')' => self.push(Tok::RParen, 1),
                b'[' => self.push(Tok::LBracket, 1),
                b']' => self.push(Tok::RBracket, 1),
                b',' => self.push(Tok::Comma, 1),
                b'.' => self.push(Tok::Dot, 1),
                b'+' => self.push(Tok::Plus, 1),
                b'-' => self.push(Tok::Minus, 1),
                b'*' => self.push(Tok::Star, 1),
                b'/' => self.push(Tok::Slash, 1),
                b'%' => self.push(Tok::Percent, 1),
                b'<' => self.push(Tok::Lt, 1),
                b'>' => self.push(Tok::Gt, 1),
                b'^' => {
                    return Err(Error::new(codes::E102, "`^` is exclusive-or, not a power")
                        .at(Span::new(start, start + 1))
                        .hint("write a power as `**`, as in `x**2`"));
                }
                _ => return Err(self.stray(start)),
            }
        }
        Ok(self.tokens)
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.at + offset).copied()
    }

    fn push(&mut self, tok: Tok, len: usize) {
        self.tokens.push(Token {
            tok,
            span: Span::new(self.at, self.at + len),
        });
        self.at += len;
    }

    fn number(&mut self) -> Result<(), Error> {
        let start = self.at;
        self.digits();
        if self.peek() == Some(b'.') && self.peek_at(1).is_some_and(|d| d.is_ascii_digit()) {
            self.at += 1;
            self.digits();
        } else if self.peek() == Some(b'.')
            && !self.peek_at(1).is_some_and(|c| c.is_ascii_alphabetic())
        {
            // `3.` is a number with nothing after the point; `v.x` is a field.
            self.at += 1;
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            let sign = usize::from(matches!(self.peek_at(1), Some(b'+' | b'-')));
            if self.peek_at(1 + sign).is_some_and(|d| d.is_ascii_digit()) {
                self.at += 1 + sign;
                self.digits();
            }
        }
        let span = Span::new(start, self.at);
        let value: f64 = self.text[start..self.at].parse().map_err(|_| {
            Error::new(
                codes::E101,
                format!("\"{}\" is not a number", &self.text[start..self.at]),
            )
            .at(span)
        })?;

        // `2x` and `2pi` are multiplications a reader sees and the grammar
        // does not: refuse them by name.
        if self
            .peek()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
        {
            let word_end = self.word_end(self.at);
            let word = &self.text[self.at..word_end];
            let number = &self.text[start..self.at];
            return Err(Error::new(
                codes::E103,
                format!("\"{number}{word}\" multiplies without saying so"),
            )
            .at(Span::new(start, word_end))
            .hint(format!("write `{number}*{word}`")));
        }

        self.tokens.push(Token {
            tok: Tok::Number(value),
            span,
        });
        Ok(())
    }

    fn digits(&mut self) {
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.at += 1;
        }
    }

    fn word_end(&self, from: usize) -> usize {
        let mut end = from;
        while self
            .bytes
            .get(end)
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
        {
            end += 1;
        }
        end
    }

    fn word(&mut self) {
        let start = self.at;
        self.at = self.word_end(start);
        let word = &self.text[start..self.at];
        let tok = match word {
            "true" => Tok::True,
            "false" => Tok::False,
            "if" => Tok::If,
            "then" => Tok::Then,
            "else" => Tok::Else,
            "and" => Tok::And,
            "or" => Tok::Or,
            "not" => Tok::Not,
            "for" => Tok::For,
            "in" => Tok::In,
            _ => Tok::Ident(word.to_string()),
        };
        self.tokens.push(Token {
            tok,
            span: Span::new(start, self.at),
        });
    }

    fn string(&mut self, quote: u8) -> Result<(), Error> {
        let start = self.at;
        self.at += 1;
        let mut out = String::new();
        loop {
            match self.peek() {
                None => {
                    return Err(Error::new(codes::E104, "this string is never closed")
                        .at(Span::new(start, self.at))
                        .hint(format!("end it with {}", quote as char)));
                }
                Some(c) if c == quote => {
                    self.at += 1;
                    break;
                }
                Some(b'\\') => {
                    let escaped = self.peek_at(1);
                    match escaped {
                        Some(e @ (b'"' | b'\'' | b'\\')) => {
                            out.push(e as char);
                            self.at += 2;
                        }
                        _ => {
                            return Err(Error::new(codes::E101, "unknown escape in a string")
                                .at(Span::new(self.at, self.at + 2))
                                .hint("only \\\", \\' and \\\\ are escapes"));
                        }
                    }
                }
                Some(_) => {
                    // Copy one whole UTF-8 character: strings may hold any text.
                    let ch = self.text[self.at..].chars().next().unwrap_or('\u{FFFD}');
                    out.push(ch);
                    self.at += ch.len_utf8();
                }
            }
        }
        self.tokens.push(Token {
            tok: Tok::Str(out),
            span: Span::new(start, self.at),
        });
        Ok(())
    }

    fn colour(&mut self) -> Result<(), Error> {
        let start = self.at;
        self.at += 1;
        while self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
            self.at += 1;
        }
        let span = Span::new(start, self.at);
        let hex = &self.text[start + 1..self.at];
        let channel = |i: usize, width: usize| -> f64 {
            let digits = &hex[i * width..(i + 1) * width];
            let value = u8::from_str_radix(digits, 16).unwrap_or(0) as f64;
            if width == 1 {
                (value * 17.0) / 255.0
            } else {
                value / 255.0
            }
        };
        let rgba = match hex.len() {
            3 => [channel(0, 1), channel(1, 1), channel(2, 1), 1.0],
            6 => [channel(0, 2), channel(1, 2), channel(2, 2), 1.0],
            8 => [channel(0, 2), channel(1, 2), channel(2, 2), channel(3, 2)],
            _ => {
                return Err(
                    Error::new(codes::E101, format!("\"#{hex}\" is not a colour"))
                        .at(span)
                        .hint("write a colour as #rgb, #rrggbb or #rrggbbaa"),
                );
            }
        };
        self.tokens.push(Token {
            tok: Tok::Colour(rgba),
            span,
        });
        Ok(())
    }

    /// A character the grammar has no use for, with the spelling it wants.
    fn stray(&self, start: usize) -> Error {
        let ch = self.text[start..].chars().next().unwrap_or('\u{FFFD}');
        let span = Span::new(start, start + ch.len_utf8());
        if let Some(instead) = ascii_for(ch) {
            return Error::new(codes::E106, format!("`{ch}` is not an operator here"))
                .at(span)
                .hint(format!("write `{instead}`"));
        }
        if let Some(name) = greek_name(ch) {
            return Error::new(codes::E106, format!("`{ch}` is not a name here"))
                .at(span)
                .hint(format!("spell it out: `{name}`"));
        }
        let (message, hint) = match ch {
            '=' => ("a single `=` assigns nothing here", "compare with `==`"),
            '!' => ("`!` on its own means nothing here", "write `not` or `!=`"),
            '&' => ("`&` means nothing here", "write `and`"),
            '|' => (
                "`|` means nothing here",
                "write `or`, or `abs(...)` for a magnitude",
            ),
            '{' | '}' => (
                "braces mean nothing in a formula",
                "group with ( ) and make lists with [ ]",
            ),
            ';' => ("a formula is one expression", "remove the `;`"),
            _ => (
                "this character is not part of a formula",
                "formulas use ASCII letters, digits and operators",
            ),
        };
        Error::new(
            if ch.is_ascii() {
                codes::E101
            } else {
                codes::E106
            },
            message,
        )
        .at(span)
        .hint(hint)
    }
}

/// The ASCII spelling of an operator people type from a maths keyboard.
fn ascii_for(ch: char) -> Option<&'static str> {
    Some(match ch {
        '²' => "**2",
        '³' => "**3",
        '×' | '·' | '⋅' => "*",
        '÷' => "/",
        '−' | '–' => "-",
        '≤' => "<=",
        '≥' => ">=",
        '≠' => "!=",
        '√' => "sqrt(...)",
        '∞' => "a large number; infinity is not a value",
        _ => return None,
    })
}

/// Greek letters are names in the prose; in a formula they are spelled out.
fn greek_name(ch: char) -> Option<&'static str> {
    Some(match ch {
        'α' => "alpha",
        'β' => "beta",
        'γ' => "gamma",
        'δ' => "delta",
        'ε' => "epsilon",
        'ζ' => "zeta",
        'η' => "eta",
        'θ' => "theta",
        'κ' => "kappa",
        'λ' => "lambda",
        'μ' => "mu",
        'ν' => "nu",
        'ξ' => "xi",
        'π' => "pi",
        'ρ' => "rho",
        'σ' => "sigma",
        'τ' => "tau",
        'φ' | 'ϕ' => "phi",
        'χ' => "chi",
        'ψ' => "psi",
        'ω' => "omega",
        'Δ' => "Delta",
        'Ω' => "Omega",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(text: &str) -> Vec<Tok> {
        lex(text).unwrap().into_iter().map(|t| t.tok).collect()
    }

    fn refused(text: &str) -> Error {
        lex(text).unwrap_err()
    }

    #[test]
    fn numbers_in_every_written_form() {
        assert_eq!(toks("3"), vec![Tok::Number(3.0)]);
        assert_eq!(toks("0.5"), vec![Tok::Number(0.5)]);
        assert_eq!(toks(".5"), vec![Tok::Number(0.5)]);
        assert_eq!(toks("1e-3"), vec![Tok::Number(0.001)]);
        assert_eq!(toks("6.02E23"), vec![Tok::Number(6.02e23)]);
        assert_eq!(toks("3."), vec![Tok::Number(3.0)]);
    }

    #[test]
    fn a_field_is_not_a_decimal_point() {
        assert_eq!(
            toks("p.x"),
            vec![Tok::Ident("p".into()), Tok::Dot, Tok::Ident("x".into())]
        );
    }

    #[test]
    fn both_quote_styles_and_escapes() {
        assert_eq!(toks("\"damped\""), vec![Tok::Str("damped".into())]);
        assert_eq!(toks("'damped'"), vec![Tok::Str("damped".into())]);
        assert_eq!(toks(r#""a\"b""#), vec![Tok::Str("a\"b".into())]);
        assert_eq!(toks(r"'it\'s'"), vec![Tok::Str("it's".into())]);
        assert_eq!(toks("\"θ ok\""), vec![Tok::Str("θ ok".into())]);
    }

    #[test]
    fn colours_in_three_widths() {
        assert_eq!(toks("#fff"), vec![Tok::Colour([1.0, 1.0, 1.0, 1.0])]);
        assert_eq!(toks("#000000"), vec![Tok::Colour([0.0, 0.0, 0.0, 1.0])]);
        let Tok::Colour(c) = &toks("#ff000080")[0] else {
            panic!()
        };
        assert!((c[3] - 128.0 / 255.0).abs() < 1e-12);
        assert_eq!(refused("#ff00").code, codes::E101);
    }

    #[test]
    fn keywords_and_names() {
        assert_eq!(
            toks("if a then b else c"),
            vec![
                Tok::If,
                Tok::Ident("a".into()),
                Tok::Then,
                Tok::Ident("b".into()),
                Tok::Else,
                Tok::Ident("c".into())
            ]
        );
        assert_eq!(toks("x_f e_R q_b theta2"), {
            ["x_f", "e_R", "q_b", "theta2"]
                .iter()
                .map(|s| Tok::Ident(s.to_string()))
                .collect::<Vec<_>>()
        });
        assert_eq!(
            toks("true false not"),
            vec![Tok::True, Tok::False, Tok::Not]
        );
    }

    #[test]
    fn every_operator() {
        assert_eq!(
            toks("+ - * / % ** < <= > >= == != ( ) [ ] , ."),
            vec![
                Tok::Plus,
                Tok::Minus,
                Tok::Star,
                Tok::Slash,
                Tok::Percent,
                Tok::Pow,
                Tok::Lt,
                Tok::Le,
                Tok::Gt,
                Tok::Ge,
                Tok::EqEq,
                Tok::Ne,
                Tok::LParen,
                Tok::RParen,
                Tok::LBracket,
                Tok::RBracket,
                Tok::Comma,
                Tok::Dot
            ]
        );
    }

    #[test]
    fn spans_point_at_the_text() {
        let tokens = lex("x  **  20").unwrap();
        assert_eq!(tokens[1].span, Span::new(3, 5));
        assert_eq!(tokens[2].span, Span::new(7, 9));
    }

    #[test]
    fn caret_is_refused_as_power() {
        let err = refused("2^3");
        assert_eq!(err.code, codes::E102);
        assert_eq!(err.span, Some(Span::new(1, 2)));
        assert!(err.hint.unwrap().contains("**"));
        assert_eq!(err.formula.as_deref(), Some("2^3"));
    }

    #[test]
    fn implicit_multiplication_is_refused_with_the_fix() {
        let err = refused("3 + 2x");
        assert_eq!(err.code, codes::E103);
        assert_eq!(err.span, Some(Span::new(4, 6)));
        assert_eq!(err.hint.as_deref(), Some("write `2*x`"));
        assert_eq!(refused("2pi").hint.as_deref(), Some("write `2*pi`"));
    }

    #[test]
    fn non_ascii_operators_say_what_to_type() {
        let err = refused("x²");
        assert_eq!(err.code, codes::E106);
        assert_eq!(err.hint.as_deref(), Some("write `**2`"));
        assert_eq!(refused("a × b").hint.as_deref(), Some("write `*`"));
        assert_eq!(refused("a − b").hint.as_deref(), Some("write `-`"));
        assert_eq!(refused("a ≤ b").hint.as_deref(), Some("write `<=`"));
    }

    #[test]
    fn greek_letters_are_spelled_out() {
        let err = refused("sin(θ)");
        assert_eq!(err.code, codes::E106);
        assert_eq!(err.hint.as_deref(), Some("spell it out: `theta`"));
        assert_eq!(err.span, Some(Span::new(4, 6)));
    }

    #[test]
    fn common_slips_get_their_spelling() {
        assert_eq!(refused("a = b").hint.as_deref(), Some("compare with `==`"));
        assert_eq!(refused("a && b").hint.as_deref(), Some("write `and`"));
        assert_eq!(refused("!a").hint.as_deref(), Some("write `not` or `!=`"));
        assert_eq!(refused("_x").code, codes::E101);
    }

    #[test]
    fn an_unclosed_string_says_how_to_close_it() {
        let err = refused("\"damped");
        assert_eq!(err.code, codes::E104);
        assert_eq!(err.hint.as_deref(), Some("end it with \""));
    }
}
