//! The shape of a formula, as written.
//!
//! The tree keeps the source spans so every later stage can point at the
//! exact text it is talking about. Its `Display` prints canonical text with
//! the fewest parentheses that keep the meaning, which is what `explain`
//! shows and what the round-trip property test checks: printing a tree and
//! reading it back gives the same tree.

use std::fmt;

use crate::span::Span;

/// A name as written, and where.
#[derive(Debug, Clone, PartialEq)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinOp {
    Or,
    And,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
}

impl BinOp {
    pub fn text(self) -> &'static str {
        match self {
            BinOp::Or => "or",
            BinOp::And => "and",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Mod => "%",
            BinOp::Pow => "**",
        }
    }

    pub fn is_comparison(self) -> bool {
        matches!(
            self,
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge | BinOp::Eq | BinOp::Ne
        )
    }

    /// Binding strength, loosest first (SPEC-2.0 §4.3).
    fn level(self) -> u8 {
        match self {
            BinOp::Or => OR,
            BinOp::And => AND,
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge | BinOp::Eq | BinOp::Ne => COMPARE,
            BinOp::Add | BinOp::Sub => ADD,
            BinOp::Mul | BinOp::Div | BinOp::Mod => MUL,
            BinOp::Pow => POW,
        }
    }
}

/// `[body for var in iter if cond]`, or the same as the sole argument of a
/// reduction (`sum(body for var in iter)`).
#[derive(Debug, Clone, PartialEq)]
pub struct Comprehension {
    pub body: Box<Expr>,
    pub var: Ident,
    pub iter: Box<Expr>,
    pub cond: Option<Box<Expr>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Number(f64),
    Str(String),
    Bool(bool),
    Colour([f64; 4]),
    Name(String),
    List(Vec<Expr>),
    ListComp(Comprehension),
    /// Only ever the single argument of a call.
    Generator(Comprehension),
    Unary {
        op: UnOp,
        operand: Box<Expr>,
    },
    Binary {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    If {
        cond: Box<Expr>,
        then: Box<Expr>,
        otherwise: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    Field {
        base: Box<Expr>,
        field: Ident,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

impl Expr {
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Expr { kind, span }
    }

    /// How tightly this expression binds, for deciding parentheses.
    fn level(&self) -> u8 {
        match &self.kind {
            ExprKind::If { .. } => IF,
            ExprKind::Binary { op, .. } => op.level(),
            ExprKind::Unary { op: UnOp::Not, .. } => NOT,
            ExprKind::Unary { op: UnOp::Neg, .. } => NEG,
            ExprKind::Call { .. } | ExprKind::Index { .. } | ExprKind::Field { .. } => POSTFIX,
            _ => ATOM,
        }
    }

    /// The same tree with every span zeroed, for comparing shapes.
    pub fn without_spans(&self) -> Expr {
        let strip = |e: &Expr| Box::new(e.without_spans());
        let comp = |c: &Comprehension| Comprehension {
            body: strip(&c.body),
            var: Ident {
                name: c.var.name.clone(),
                span: Span::default(),
            },
            iter: strip(&c.iter),
            cond: c.cond.as_deref().map(strip),
        };
        let kind = match &self.kind {
            ExprKind::List(items) => {
                ExprKind::List(items.iter().map(Expr::without_spans).collect())
            }
            ExprKind::ListComp(c) => ExprKind::ListComp(comp(c)),
            ExprKind::Generator(c) => ExprKind::Generator(comp(c)),
            ExprKind::Unary { op, operand } => ExprKind::Unary {
                op: *op,
                operand: strip(operand),
            },
            ExprKind::Binary { op, left, right } => ExprKind::Binary {
                op: *op,
                left: strip(left),
                right: strip(right),
            },
            ExprKind::If {
                cond,
                then,
                otherwise,
            } => ExprKind::If {
                cond: strip(cond),
                then: strip(then),
                otherwise: strip(otherwise),
            },
            ExprKind::Call { callee, args } => ExprKind::Call {
                callee: strip(callee),
                args: args.iter().map(Expr::without_spans).collect(),
            },
            ExprKind::Index { base, index } => ExprKind::Index {
                base: strip(base),
                index: strip(index),
            },
            ExprKind::Field { base, field } => ExprKind::Field {
                base: strip(base),
                field: Ident {
                    name: field.name.clone(),
                    span: Span::default(),
                },
            },
            leaf => leaf.clone(),
        };
        Expr::new(kind, Span::default())
    }
}

// Binding levels, loosest first. Matches the table in SPEC-2.0 §4.3.
const IF: u8 = 1;
const OR: u8 = 2;
const AND: u8 = 3;
const NOT: u8 = 4;
const COMPARE: u8 = 5;
const ADD: u8 = 6;
const MUL: u8 = 7;
const NEG: u8 = 8;
const POW: u8 = 9;
const POSTFIX: u8 = 10;
const ATOM: u8 = 11;

/// Print `expr`, in parentheses unless it binds at least as tightly as `min`.
fn at_least(f: &mut fmt::Formatter<'_>, expr: &Expr, min: u8) -> fmt::Result {
    if expr.level() >= min {
        write!(f, "{expr}")
    } else {
        write!(f, "({expr})")
    }
}

fn comprehension(f: &mut fmt::Formatter<'_>, c: &Comprehension) -> fmt::Result {
    // An `if` inside a comprehension would read as its filter: bracket it.
    at_least(f, &c.body, OR)?;
    write!(f, " for {} in ", c.var.name)?;
    at_least(f, &c.iter, OR)?;
    if let Some(cond) = &c.cond {
        write!(f, " if ")?;
        at_least(f, cond, OR)?;
    }
    Ok(())
}

fn number(f: &mut fmt::Formatter<'_>, n: f64) -> fmt::Result {
    // Rust prints the shortest text that reads back as the same double.
    let text = format!("{n}");
    f.write_str(&text)
}

fn string(f: &mut fmt::Formatter<'_>, s: &str) -> fmt::Result {
    f.write_str("\"")?;
    for ch in s.chars() {
        match ch {
            '"' => f.write_str("\\\"")?,
            '\\' => f.write_str("\\\\")?,
            other => write!(f, "{other}")?,
        }
    }
    f.write_str("\"")
}

fn colour(f: &mut fmt::Formatter<'_>, c: &[f64; 4]) -> fmt::Result {
    let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    write!(f, "#{:02x}{:02x}{:02x}", byte(c[0]), byte(c[1]), byte(c[2]))?;
    if byte(c[3]) != 255 {
        write!(f, "{:02x}", byte(c[3]))?;
    }
    Ok(())
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ExprKind::Number(n) => number(f, *n),
            ExprKind::Str(s) => string(f, s),
            ExprKind::Bool(b) => write!(f, "{b}"),
            ExprKind::Colour(c) => colour(f, c),
            ExprKind::Name(name) => f.write_str(name),
            ExprKind::List(items) => {
                f.write_str("[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{item}")?;
                }
                f.write_str("]")
            }
            ExprKind::ListComp(c) => {
                f.write_str("[")?;
                comprehension(f, c)?;
                f.write_str("]")
            }
            ExprKind::Generator(c) => comprehension(f, c),
            ExprKind::Unary {
                op: UnOp::Neg,
                operand,
            } => {
                f.write_str("-")?;
                at_least(f, operand, NEG)
            }
            ExprKind::Unary {
                op: UnOp::Not,
                operand,
            } => {
                f.write_str("not ")?;
                at_least(f, operand, NOT)
            }
            ExprKind::Binary { op, left, right } => {
                let level = op.level();
                let (left_min, right_min) = match op {
                    // `postfix ** unary`: right-associative, and the exponent
                    // may carry its own sign (`2 ** -1`).
                    BinOp::Pow => (POSTFIX, NEG),
                    // Comparisons do not chain: both sides one level tighter.
                    op if op.is_comparison() => (level + 1, level + 1),
                    // Everything else is left-associative.
                    _ => (level, level + 1),
                };
                at_least(f, left, left_min)?;
                write!(f, " {} ", op.text())?;
                at_least(f, right, right_min)
            }
            ExprKind::If {
                cond,
                then,
                otherwise,
            } => write!(f, "if {cond} then {then} else {otherwise}"),
            ExprKind::Call { callee, args } => {
                at_least(f, callee, POSTFIX)?;
                f.write_str("(")?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{arg}")?;
                }
                f.write_str(")")
            }
            ExprKind::Index { base, index } => {
                at_least(f, base, POSTFIX)?;
                write!(f, "[{index}]")
            }
            ExprKind::Field { base, field } => {
                at_least(f, base, POSTFIX)?;
                write!(f, ".{}", field.name)
            }
        }
    }
}
