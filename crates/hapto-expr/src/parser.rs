//! Reading tokens into a tree, by recursive descent over SPEC-2.0 Appendix B.
//!
//! One function per grammar rule, loosest binding first. The places where a
//! formula is plausibly wrong in a common way — a chained comparison, an
//! implicit multiplication against a bracket, a comma used as a decimal
//! point, a generator that is not alone in its call — each get their own
//! error with the fix, rather than a bare "unexpected token".

use crate::ast::{BinOp, Comprehension, Expr, ExprKind, Ident, UnOp};
use crate::error::{codes, Error};
use crate::lexer::{lex, Tok, Token};
use crate::span::Span;

/// Read `text` as one formula.
pub fn parse(text: &str) -> Result<Expr, Error> {
    let tokens = lex(text)?;
    let mut parser = Parser {
        tokens: &tokens,
        at: 0,
        text,
        open: Vec::new(),
    };
    parser.formula().map_err(|err| err.in_formula(text))
}

struct Parser<'a> {
    tokens: &'a [Token],
    at: usize,
    text: &'a str,
    /// Brackets opened and not yet closed, for "this was never closed".
    open: Vec<(Tok, Span)>,
}

/// What `atom` produced, so `postfix` can tell `2(…)` and `(a)(b)` — implicit
/// multiplications — from a call.
#[derive(PartialEq)]
enum AtomShape {
    Number,
    Parenthesised,
    Other,
}

impl Parser<'_> {
    fn formula(&mut self) -> Result<Expr, Error> {
        if self.tokens.is_empty() {
            return Err(Error::new(codes::E101, "the formula is empty")
                .at(Span::new(0, self.text.len()))
                .hint("write a number, a name, or an expression such as `2*x`"));
        }
        let expr = self.expr()?;
        if let Some(token) = self.peek() {
            return Err(self.unexpected(token));
        }
        Ok(expr)
    }

    // --- helpers ---

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn peek_tok(&self) -> Option<&Tok> {
        self.peek().map(|t| &t.tok)
    }

    fn peek_is(&self, tok: &Tok) -> bool {
        self.peek_tok() == Some(tok)
    }

    fn bump(&mut self) -> Token {
        let token = self.tokens[self.at].clone();
        self.at += 1;
        token
    }

    fn eat(&mut self, tok: &Tok) -> Option<Token> {
        if self.peek_is(tok) {
            Some(self.bump())
        } else {
            None
        }
    }

    fn end_span(&self) -> Span {
        Span::new(self.text.len(), self.text.len())
    }

    /// Expect `tok`, or explain what was found instead.
    fn expect(&mut self, tok: Tok, context: &str) -> Result<Token, Error> {
        if let Some(token) = self.eat(&tok) {
            return Ok(token);
        }
        match self.peek() {
            Some(found) => Err(Error::new(
                codes::E101,
                format!(
                    "expected `{}` {context}, found {}",
                    tok.text(),
                    found.tok.describe()
                ),
            )
            .at(found.span)),
            None => Err(self.ended(Some(&tok), context)),
        }
    }

    /// The formula stopped while something was still open or expected.
    fn ended(&self, wanted: Option<&Tok>, context: &str) -> Error {
        if let Some((bracket, span)) = self.open.last() {
            let close = match bracket {
                Tok::LParen => ")",
                _ => "]",
            };
            return Error::new(
                codes::E104,
                format!("this `{}` is never closed", bracket.text()),
            )
            .at(*span)
            .hint(format!("add `{close}` where it should end"));
        }
        let message = match wanted {
            Some(tok) => format!(
                "the formula ends where `{}` was expected {context}",
                tok.text()
            ),
            None => format!("the formula ends where {context} was expected"),
        };
        Error::new(codes::E101, message).at(self.end_span())
    }

    fn unexpected(&self, token: &Token) -> Error {
        let mut err = Error::new(
            codes::E101,
            format!("{} was not expected here", token.tok.describe()),
        )
        .at(token.span);
        match &token.tok {
            Tok::Comma => {
                // `3,5` at the top level is a decimal comma.
                let prev = self.at.checked_sub(1).and_then(|i| self.tokens.get(i));
                let next = self.tokens.get(self.at + 1);
                if let (Some(prev), Some(next)) = (prev, next) {
                    if let (Tok::Number(_), Tok::Number(_)) = (&prev.tok, &next.tok) {
                        if prev.span.end == token.span.start && token.span.end == next.span.start {
                            let a = &self.text[prev.span.start..prev.span.end];
                            let b = &self.text[next.span.start..next.span.end];
                            err = err.hint(format!("a decimal point is `.`: write `{a}.{b}`"));
                        }
                    }
                }
            }
            Tok::RParen | Tok::RBracket => {
                err = err.hint("there is no matching opening bracket for it");
            }
            Tok::Then | Tok::Else => {
                err = err.hint("an `if` reads `if condition then value else value`");
            }
            Tok::For => {
                err = err.hint("a comprehension lives in brackets: `[x*2 for x in xs]`");
            }
            Tok::Number(_) | Tok::Ident(_) | Tok::Str(_) | Tok::LParen => {
                err = err.hint("two values side by side need an operator between them");
            }
            _ => {}
        }
        err
    }

    // --- grammar, loosest first ---

    fn expr(&mut self) -> Result<Expr, Error> {
        if let Some(start) = self.eat(&Tok::If) {
            let cond = self.expr()?;
            self.expect(Tok::Then, "after the condition of `if`")?;
            let then = self.expr()?;
            self.expect(Tok::Else, "after `then` (every `if` needs both branches)")?;
            let otherwise = self.expr()?;
            let span = start.span.to(otherwise.span);
            return Ok(Expr::new(
                ExprKind::If {
                    cond: Box::new(cond),
                    then: Box::new(then),
                    otherwise: Box::new(otherwise),
                },
                span,
            ));
        }
        self.or()
    }

    fn binary_chain(
        &mut self,
        ops: &[(Tok, BinOp)],
        next: fn(&mut Self) -> Result<Expr, Error>,
    ) -> Result<Expr, Error> {
        let mut left = next(self)?;
        'outer: loop {
            for (tok, op) in ops {
                if self.eat(tok).is_some() {
                    let right = next(self)?;
                    let span = left.span.to(right.span);
                    left = Expr::new(
                        ExprKind::Binary {
                            op: *op,
                            left: Box::new(left),
                            right: Box::new(right),
                        },
                        span,
                    );
                    continue 'outer;
                }
            }
            return Ok(left);
        }
    }

    fn or(&mut self) -> Result<Expr, Error> {
        self.binary_chain(&[(Tok::Or, BinOp::Or)], Self::and)
    }

    fn and(&mut self) -> Result<Expr, Error> {
        self.binary_chain(&[(Tok::And, BinOp::And)], Self::not)
    }

    fn not(&mut self) -> Result<Expr, Error> {
        if let Some(start) = self.eat(&Tok::Not) {
            let operand = self.not()?;
            let span = start.span.to(operand.span);
            return Ok(Expr::new(
                ExprKind::Unary {
                    op: UnOp::Not,
                    operand: Box::new(operand),
                },
                span,
            ));
        }
        self.comparison()
    }

    fn comparison_op(&self) -> Option<BinOp> {
        Some(match self.peek_tok()? {
            Tok::Lt => BinOp::Lt,
            Tok::Le => BinOp::Le,
            Tok::Gt => BinOp::Gt,
            Tok::Ge => BinOp::Ge,
            Tok::EqEq => BinOp::Eq,
            Tok::Ne => BinOp::Ne,
            _ => return None,
        })
    }

    fn comparison(&mut self) -> Result<Expr, Error> {
        let left = self.additive()?;
        let Some(op) = self.comparison_op() else {
            return Ok(left);
        };
        self.bump();
        let right = self.additive()?;
        if self.comparison_op().is_some() {
            let second = self.peek().map(|t| t.span).unwrap_or_default();
            let mid = &self.text[right.span.start..right.span.end];
            return Err(Error::new(
                codes::E105,
                "comparisons do not chain: `a < b < c` is not `a < b and b < c`",
            )
            .at(left.span.to(second))
            .hint(format!(
                "join them with `and`, as in `… {} {mid} and {mid} …`",
                op.text()
            )));
        }
        let span = left.span.to(right.span);
        Ok(Expr::new(
            ExprKind::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            },
            span,
        ))
    }

    fn additive(&mut self) -> Result<Expr, Error> {
        self.binary_chain(
            &[(Tok::Plus, BinOp::Add), (Tok::Minus, BinOp::Sub)],
            Self::term,
        )
    }

    fn term(&mut self) -> Result<Expr, Error> {
        self.binary_chain(
            &[
                (Tok::Star, BinOp::Mul),
                (Tok::Slash, BinOp::Div),
                (Tok::Percent, BinOp::Mod),
            ],
            Self::unary,
        )
    }

    fn unary(&mut self) -> Result<Expr, Error> {
        if let Some(start) = self.eat(&Tok::Minus) {
            let operand = self.unary()?;
            let span = start.span.to(operand.span);
            return Ok(Expr::new(
                ExprKind::Unary {
                    op: UnOp::Neg,
                    operand: Box::new(operand),
                },
                span,
            ));
        }
        if let Some(plus) = self.peek().filter(|t| t.tok == Tok::Plus) {
            return Err(Error::new(codes::E101, "a leading `+` means nothing")
                .at(plus.span)
                .hint("remove it"));
        }
        self.power()
    }

    fn power(&mut self) -> Result<Expr, Error> {
        let base = self.postfix()?;
        if self.eat(&Tok::Pow).is_none() {
            return Ok(base);
        }
        // `postfix ** unary`: right-associative, and `2 ** -1` is allowed.
        let exponent = self.unary()?;
        let span = base.span.to(exponent.span);
        Ok(Expr::new(
            ExprKind::Binary {
                op: BinOp::Pow,
                left: Box::new(base),
                right: Box::new(exponent),
            },
            span,
        ))
    }

    fn postfix(&mut self) -> Result<Expr, Error> {
        let (mut expr, shape) = self.atom()?;
        let mut first = true;
        loop {
            match self.peek_tok() {
                Some(Tok::LParen) => {
                    if first && shape != AtomShape::Other {
                        let open = self.peek().map(|t| t.span).unwrap_or_default();
                        let left = &self.text[expr.span.start..expr.span.end];
                        return Err(Error::new(
                            codes::E103,
                            format!("\"{left}(…)\" multiplies without saying so"),
                        )
                        .at(expr.span.to(open))
                        .hint(format!("write `{left}*(…)`")));
                    }
                    expr = self.call(expr)?;
                }
                Some(Tok::LBracket) => {
                    let open = self.bump();
                    self.open.push((Tok::LBracket, open.span));
                    let index = self.expr()?;
                    let close = self.expect(Tok::RBracket, "to close the index")?;
                    self.open.pop();
                    let span = expr.span.to(close.span);
                    expr = Expr::new(
                        ExprKind::Index {
                            base: Box::new(expr),
                            index: Box::new(index),
                        },
                        span,
                    );
                }
                Some(Tok::Dot) => {
                    self.bump();
                    let field = self.ident("after `.`")?;
                    let span = expr.span.to(field.span);
                    expr = Expr::new(
                        ExprKind::Field {
                            base: Box::new(expr),
                            field,
                        },
                        span,
                    );
                }
                _ => return Ok(expr),
            }
            first = false;
        }
    }

    fn ident(&mut self, context: &str) -> Result<Ident, Error> {
        match self.peek() {
            Some(Token {
                tok: Tok::Ident(name),
                span,
            }) => {
                let ident = Ident {
                    name: name.clone(),
                    span: *span,
                };
                self.bump();
                Ok(ident)
            }
            Some(other) => Err(Error::new(
                codes::E101,
                format!("expected a name {context}, found {}", other.tok.describe()),
            )
            .at(other.span)),
            None => Err(self.ended(None, &format!("a name {context}"))),
        }
    }

    fn call(&mut self, callee: Expr) -> Result<Expr, Error> {
        let open = self.bump();
        self.open.push((Tok::LParen, open.span));
        let mut args = Vec::new();
        if !self.peek_is(&Tok::RParen) {
            let first = self.expr()?;
            if self.peek_is(&Tok::For) {
                args.push(self.generator(first)?);
                if self.peek_is(&Tok::Comma) {
                    let comma = self.peek().map(|t| t.span).unwrap_or_default();
                    return Err(Error::new(
                        codes::E101,
                        "a generator must be the only argument of its call",
                    )
                    .at(comma)
                    .hint("make it a list first: `[x for x in xs]`"));
                }
            } else {
                args.push(first);
                while self.eat(&Tok::Comma).is_some() {
                    let arg = self.expr()?;
                    if self.peek_is(&Tok::For) {
                        return Err(Error::new(
                            codes::E101,
                            "a generator must be the only argument of its call",
                        )
                        .at(arg.span)
                        .hint("make it a list first: `[x for x in xs]`"));
                    }
                    args.push(arg);
                }
            }
        }
        let close = self.expect(Tok::RParen, "to close the call")?;
        self.open.pop();
        let span = callee.span.to(close.span);
        Ok(Expr::new(
            ExprKind::Call {
                callee: Box::new(callee),
                args,
            },
            span,
        ))
    }

    /// The `for … in … [if …]` tail, after its body has been read.
    fn comprehension(&mut self, body: Expr) -> Result<Comprehension, Error> {
        self.expect(Tok::For, "in the comprehension")?;
        let var = self.ident("after `for`")?;
        self.expect(Tok::In, "after the comprehension's variable")?;
        let iter = self.or()?;
        let cond = match self.eat(&Tok::If) {
            Some(_) => Some(Box::new(self.or()?)),
            None => None,
        };
        Ok(Comprehension {
            body: Box::new(body),
            var,
            iter: Box::new(iter),
            cond,
        })
    }

    fn generator(&mut self, body: Expr) -> Result<Expr, Error> {
        let start = body.span;
        let comp = self.comprehension(body)?;
        let end = comp.cond.as_ref().map(|c| c.span).unwrap_or(comp.iter.span);
        Ok(Expr::new(ExprKind::Generator(comp), start.to(end)))
    }

    fn atom(&mut self) -> Result<(Expr, AtomShape), Error> {
        let Some(token) = self.peek().cloned() else {
            return Err(self.ended(None, "a value"));
        };
        let leaf = |kind| Ok((Expr::new(kind, token.span), AtomShape::Other));
        match &token.tok {
            Tok::Number(n) => {
                self.bump();
                Ok((
                    Expr::new(ExprKind::Number(*n), token.span),
                    AtomShape::Number,
                ))
            }
            Tok::Str(s) => {
                self.bump();
                leaf(ExprKind::Str(s.clone()))
            }
            Tok::Colour(c) => {
                self.bump();
                leaf(ExprKind::Colour(*c))
            }
            Tok::True => {
                self.bump();
                leaf(ExprKind::Bool(true))
            }
            Tok::False => {
                self.bump();
                leaf(ExprKind::Bool(false))
            }
            Tok::Ident(name) => {
                self.bump();
                leaf(ExprKind::Name(name.clone()))
            }
            Tok::LParen => {
                self.bump();
                self.open.push((Tok::LParen, token.span));
                let inner = self.expr()?;
                let close = self.expect(Tok::RParen, "to close the bracket")?;
                self.open.pop();
                let mut inner = inner;
                inner.span = token.span.to(close.span);
                Ok((inner, AtomShape::Parenthesised))
            }
            Tok::LBracket => {
                self.bump();
                self.open.push((Tok::LBracket, token.span));
                let mut items = Vec::new();
                let mut comp = None;
                if !self.peek_is(&Tok::RBracket) {
                    let first = self.expr()?;
                    if self.peek_is(&Tok::For) {
                        comp = Some(self.comprehension(first)?);
                    } else {
                        items.push(first);
                        while self.eat(&Tok::Comma).is_some() {
                            items.push(self.expr()?);
                        }
                    }
                }
                let close = self.expect(Tok::RBracket, "to close the list")?;
                self.open.pop();
                let span = token.span.to(close.span);
                let kind = match comp {
                    Some(c) => ExprKind::ListComp(c),
                    None => ExprKind::List(items),
                };
                Ok((Expr::new(kind, span), AtomShape::Other))
            }
            _ => Err(self.unexpected(&token)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(text: &str) -> String {
        format!("{}", parse(text).unwrap())
    }

    fn refused(text: &str) -> Error {
        parse(text).unwrap_err()
    }

    #[test]
    fn precedence_follows_the_table() {
        assert_eq!(shape("1 + 2 * 3"), "1 + 2 * 3");
        assert_eq!(shape("(1 + 2) * 3"), "(1 + 2) * 3");
        assert_eq!(shape("a or b and c"), "a or b and c");
        assert_eq!(shape("(a or b) and c"), "(a or b) and c");
        assert_eq!(shape("not a < b"), "not a < b");
        assert_eq!(shape("a - (b - c)"), "a - (b - c)");
        assert_eq!(shape("a - b - c"), "a - b - c");
    }

    #[test]
    fn a_negative_square_is_minus_the_square() {
        // `-x**2` is `-(x**2)`, as in Python and as every lesson expects.
        let tree = parse("-x**2").unwrap();
        let ExprKind::Unary {
            op: UnOp::Neg,
            operand,
        } = tree.kind
        else {
            panic!("{tree:?}")
        };
        assert!(matches!(
            operand.kind,
            ExprKind::Binary { op: BinOp::Pow, .. }
        ));
        assert_eq!(shape("(-x)**2"), "(-x) ** 2");
    }

    #[test]
    fn power_is_right_associative_and_takes_a_signed_exponent() {
        assert_eq!(shape("2**3**2"), "2 ** 3 ** 2");
        assert_eq!(shape("(2**3)**2"), "(2 ** 3) ** 2");
        assert_eq!(shape("2**-1"), "2 ** -1");
    }

    #[test]
    fn if_then_else_and_postfix() {
        assert_eq!(
            shape("if zeta < 1 then 'under' else 'over'"),
            "if zeta < 1 then \"under\" else \"over\""
        );
        assert_eq!(shape("traj.at(s).q"), "traj.at(s).q");
        assert_eq!(shape("M[0][1]"), "M[0][1]");
        assert_eq!(
            shape("a + (if c then 1 else 2)"),
            "a + (if c then 1 else 2)"
        );
    }

    #[test]
    fn lists_comprehensions_and_generators() {
        assert_eq!(shape("[1, 2, 3]"), "[1, 2, 3]");
        assert_eq!(shape("[]"), "[]");
        assert_eq!(
            shape("[k*x for k in range(1, n)]"),
            "[k * x for k in range(1, n)]"
        );
        assert_eq!(
            shape("sum(sin(k*x)/k for k in range(1, n, 2) if k > 0)"),
            "sum(sin(k * x) / k for k in range(1, n, 2) if k > 0)"
        );
    }

    #[test]
    fn spans_cover_their_text() {
        let text = "f(x) + 2";
        let tree = parse(text).unwrap();
        let ExprKind::Binary { left, .. } = &tree.kind else {
            panic!()
        };
        assert_eq!(&text[left.span.start..left.span.end], "f(x)");
        assert_eq!(tree.span, Span::new(0, text.len()));
    }

    #[test]
    fn chained_comparisons_are_refused_with_the_and_form() {
        let err = refused("0 < x < 1");
        assert_eq!(err.code, codes::E105);
        assert!(err.hint.unwrap().contains("and"));
    }

    #[test]
    fn implicit_multiplication_against_a_bracket() {
        let err = refused("2(x + 1)");
        assert_eq!(err.code, codes::E103);
        assert_eq!(err.hint.as_deref(), Some("write `2*(…)`"));
        assert_eq!(refused("(a + b)(c + d)").code, codes::E103);
    }

    #[test]
    fn unclosed_brackets_point_at_the_opening() {
        let err = refused("sin(x + (1");
        assert_eq!(err.code, codes::E104);
        assert_eq!(err.span, Some(Span::new(8, 9)));
        let err = refused("[1, 2");
        assert_eq!(err.code, codes::E104);
        assert_eq!(err.span, Some(Span::new(0, 1)));
    }

    #[test]
    fn a_decimal_comma_is_named() {
        let err = refused("3,5");
        assert_eq!(err.code, codes::E101);
        assert_eq!(
            err.hint.as_deref(),
            Some("a decimal point is `.`: write `3.5`")
        );
        // In a call, a comma is just a comma.
        assert!(parse("max(3,5)").is_ok());
    }

    #[test]
    fn a_generator_is_alone_in_its_call() {
        assert_eq!(refused("max(x for x in xs, 3)").code, codes::E101);
        assert_eq!(refused("max(3, x for x in xs)").code, codes::E101);
    }

    #[test]
    fn missing_pieces_are_named() {
        let err = refused("if a then b");
        assert_eq!(err.code, codes::E101);
        assert!(err.message.contains("else"));
        assert_eq!(refused("").code, codes::E101);
        assert_eq!(refused("1 +").code, codes::E101);
        assert_eq!(
            refused("x y").hint.as_deref(),
            Some("two values side by side need an operator between them")
        );
        assert_eq!(refused("+x").code, codes::E101);
        assert_eq!(
            refused("a )").hint.as_deref(),
            Some("there is no matching opening bracket for it")
        );
    }
}

#[cfg(test)]
mod round_trip {
    //! Printing a tree and reading it back gives the same tree (M1 exit).

    use super::*;
    use proptest::prelude::*;

    fn name() -> impl Strategy<Value = String> {
        "[a-z][a-z0-9_]{0,4}".prop_filter("not a keyword", |s| {
            !crate::lexer::KEYWORDS.contains(&s.as_str())
        })
    }

    fn leaf() -> impl Strategy<Value = Expr> {
        let at = Span::default();
        prop_oneof![
            (0u32..100_000, 0u32..4).prop_map(move |(n, d)| {
                Expr::new(ExprKind::Number(n as f64 / 10f64.powi(d as i32)), at)
            }),
            any::<f64>()
                .prop_filter("finite, non-negative", |n| n.is_finite()
                    && n.is_sign_positive())
                .prop_map(move |n| Expr::new(ExprKind::Number(n), at)),
            "[a-z \"'\\\\]{0,6}".prop_map(move |s| Expr::new(ExprKind::Str(s), at)),
            any::<bool>().prop_map(move |b| Expr::new(ExprKind::Bool(b), at)),
            (any::<u8>(), any::<u8>(), any::<u8>(), any::<u8>()).prop_map(move |(r, g, b, a)| {
                let c = |v: u8| v as f64 / 255.0;
                Expr::new(ExprKind::Colour([c(r), c(g), c(b), c(a)]), at)
            }),
            name().prop_map(move |n| Expr::new(ExprKind::Name(n), at)),
        ]
    }

    fn ident() -> impl Strategy<Value = Ident> {
        name().prop_map(|name| Ident {
            name,
            span: Span::default(),
        })
    }

    fn tree() -> impl Strategy<Value = Expr> {
        let at = Span::default();
        leaf().prop_recursive(5, 48, 4, move |inner| {
            let bin = prop_oneof![
                Just(BinOp::Or),
                Just(BinOp::And),
                Just(BinOp::Lt),
                Just(BinOp::Le),
                Just(BinOp::Gt),
                Just(BinOp::Ge),
                Just(BinOp::Eq),
                Just(BinOp::Ne),
                Just(BinOp::Add),
                Just(BinOp::Sub),
                Just(BinOp::Mul),
                Just(BinOp::Div),
                Just(BinOp::Mod),
                Just(BinOp::Pow),
            ];
            let comp = (
                inner.clone(),
                ident(),
                inner.clone(),
                prop::option::of(inner.clone()),
            )
                .prop_map(|(body, var, iter, cond)| Comprehension {
                    body: Box::new(body),
                    var,
                    iter: Box::new(iter),
                    cond: cond.map(Box::new),
                })
                .boxed();
            prop_oneof![
                (bin, inner.clone(), inner.clone()).prop_map(move |(op, l, r)| Expr::new(
                    ExprKind::Binary {
                        op,
                        left: Box::new(l),
                        right: Box::new(r)
                    },
                    at
                )),
                (prop_oneof![Just(UnOp::Neg), Just(UnOp::Not)], inner.clone()).prop_map(
                    move |(op, e)| Expr::new(
                        ExprKind::Unary {
                            op,
                            operand: Box::new(e)
                        },
                        at
                    )
                ),
                (inner.clone(), inner.clone(), inner.clone()).prop_map(move |(c, t, o)| Expr::new(
                    ExprKind::If {
                        cond: Box::new(c),
                        then: Box::new(t),
                        otherwise: Box::new(o)
                    },
                    at
                )),
                (name(), prop::collection::vec(inner.clone(), 0..3)).prop_map(move |(f, args)| {
                    Expr::new(
                        ExprKind::Call {
                            callee: Box::new(Expr::new(ExprKind::Name(f), at)),
                            args,
                        },
                        at,
                    )
                }),
                (name(), comp.clone()).prop_map(move |(f, c)| Expr::new(
                    ExprKind::Call {
                        callee: Box::new(Expr::new(ExprKind::Name(f), at)),
                        args: vec![Expr::new(ExprKind::Generator(c), at)],
                    },
                    at
                )),
                comp.prop_map(move |c| Expr::new(ExprKind::ListComp(c), at)),
                prop::collection::vec(inner.clone(), 0..4)
                    .prop_map(move |items| Expr::new(ExprKind::List(items), at)),
                (inner.clone(), inner.clone()).prop_map(move |(b, i)| Expr::new(
                    ExprKind::Index {
                        base: Box::new(b),
                        index: Box::new(i)
                    },
                    at
                )),
                (inner, ident()).prop_map(move |(b, field)| Expr::new(
                    ExprKind::Field {
                        base: Box::new(b),
                        field
                    },
                    at
                )),
            ]
        })
    }

    /// Trees the parser itself can produce: chained comparisons and indexing
    /// or calling a bare number are refused, so they are not generated.
    fn parseable(e: &Expr) -> bool {
        let comp_ok = |c: &Comprehension| {
            parseable(&c.body) && parseable(&c.iter) && c.cond.as_deref().is_none_or(parseable)
        };
        match &e.kind {
            ExprKind::Binary { op, left, right } => {
                let chained = op.is_comparison()
                    && [left, right].iter().any(|side| {
                        matches!(&side.kind, ExprKind::Binary { op, .. } if op.is_comparison())
                    });
                !chained && parseable(left) && parseable(right)
            }
            ExprKind::Unary { operand, .. } => parseable(operand),
            ExprKind::If {
                cond,
                then,
                otherwise,
            } => parseable(cond) && parseable(then) && parseable(otherwise),
            ExprKind::Call { callee, args } => parseable(callee) && args.iter().all(parseable),
            ExprKind::Index { base, index } => parseable(base) && parseable(index),
            ExprKind::Field { base, .. } => parseable(base),
            ExprKind::List(items) => items.iter().all(parseable),
            ExprKind::ListComp(c) | ExprKind::Generator(c) => comp_ok(c),
            _ => true,
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(2000))]

        #[test]
        fn printing_and_reparsing_gives_the_same_tree(tree in tree().prop_filter("parseable", parseable)) {
            let text = tree.to_string();
            let back = parse(&text).map_err(|e| TestCaseError::fail(format!("{text:?}: {e} {:?}", e.hint)))?;
            prop_assert_eq!(back.without_spans(), tree.without_spans(), "printed as {:?}", text);
        }
    }
}
