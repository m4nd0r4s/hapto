//! Type checking: from the tree as written to the typed IR the evaluator runs.
//!
//! The checker resolves every name (bound variables first, then the figure's
//! lets, compute nodes and values, §4.4), picks the operator and the builtin
//! overload each node means, and inserts the coercions the context asks for.
//! Anything that does not fit is an error naming the type found, the type
//! wanted, and — when the fix is mechanical — the fix (§4.7).

use crate::ast::{BinOp, Comprehension, Expr, ExprKind, Ident, UnOp};
use crate::builtins::{self, concrete, Builtin, Pat, Ret};
use crate::error::{codes, Error};
use crate::ir::{Arith, BinKind, FnId, Loop, Node, Reducer};
use crate::span::Span;
use crate::suggest::nearest;
use crate::types::{fits, join, Coercion, Type};
use crate::value::Value;

/// What a name in a figure is.
#[derive(Debug, Clone, PartialEq)]
pub enum Named {
    Value(usize, Type),
    Compute(usize, Type),
    Let(usize, Type),
    /// A user function: it can only be called.
    Function,
}

/// The names a formula can see beyond its own bound variables.
pub trait Scope {
    fn name(&mut self, name: &str) -> Option<Named>;

    /// Compile the user function `name` for these argument types, returning
    /// its instance and result type; `None` when there is no such function.
    fn function(
        &mut self,
        name: &str,
        args: &[Type],
        span: Span,
    ) -> Option<Result<(FnId, Type), Error>>;

    /// Every name in scope, for "did you mean".
    fn known_names(&self) -> Vec<String>;
}

/// What one name in a formula resolved to, for `hapto explain`.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolution {
    pub span: Span,
    pub name: String,
    pub what: String,
}

/// A checked formula.
#[derive(Debug, Clone)]
pub struct Checked {
    pub node: Node,
    /// The settled type: a pair has become a vector.
    pub ty: Type,
    /// How many local slots evaluation needs, bound variables included.
    pub frame: usize,
    pub warnings: Vec<Error>,
    pub resolutions: Vec<Resolution>,
}

/// Names the language reserves beyond keywords and library functions: the
/// constants, and the context names an interaction binds (§4.1).
pub const CONSTANTS: &[(&str, f64)] = &[
    ("pi", std::f64::consts::PI),
    ("tau", std::f64::consts::TAU),
    ("e", std::f64::consts::E),
];
pub const CONTEXT_NAMES: &[&str] = &["pointer", "start", "anchor0", "delta", "focus", "theme"];

/// Whether `name` is reserved and may not name a value, let, compute node,
/// parameter or comprehension variable.
pub fn is_reserved(name: &str) -> bool {
    crate::lexer::KEYWORDS.contains(&name)
        || CONSTANTS.iter().any(|(c, _)| *c == name)
        || CONTEXT_NAMES.contains(&name)
        || REDUCERS.contains(&name)
        || builtins::lookup(name).is_some()
}

/// The reductions that take a generator: `sum(x for x in xs)`.
const REDUCERS: &[&str] = &["sum", "prod", "min", "max", "mean", "count"];

/// Check `expr` against `scope`, with `bound` names (a mark's `x`, a drag's
/// `pointer`) in the first local slots, and the result fitting `expected`.
pub fn check(
    expr: &Expr,
    scope: &mut dyn Scope,
    bound: &[(String, Type)],
    expected: Option<&Type>,
) -> Result<Checked, Error> {
    let mut checker = Checker {
        scope,
        locals: bound.to_vec(),
        frame: bound.len(),
        warnings: Vec::new(),
        resolutions: Vec::new(),
    };
    let (node, ty) = match expected {
        Some(wanted) => (
            checker.expect(expr, wanted, "this formula")?,
            wanted.settled(),
        ),
        None => {
            let (node, ty) = checker.expr(expr)?;
            (node, ty.settled())
        }
    };
    Ok(Checked {
        node,
        ty,
        frame: checker.frame,
        warnings: checker.warnings,
        resolutions: checker.resolutions,
    })
}

/// Parse and check `text`, with every error pointing into it.
pub fn check_text(
    text: &str,
    scope: &mut dyn Scope,
    bound: &[(String, Type)],
    expected: Option<&Type>,
) -> Result<Checked, Error> {
    let expr = crate::parser::parse(text)?;
    let mut checked = check(&expr, scope, bound, expected).map_err(|e| e.in_formula(text))?;
    for warning in &mut checked.warnings {
        warning.formula = Some(text.to_string());
    }
    Ok(checked)
}

struct Checker<'s> {
    scope: &'s mut dyn Scope,
    locals: Vec<(String, Type)>,
    frame: usize,
    warnings: Vec<Error>,
    resolutions: Vec<Resolution>,
}

fn mismatch(span: Span, what: &str, found: &Type, wanted: &Type) -> Error {
    Error::new(
        codes::E201,
        format!("{what} is a {found}, but a {wanted} is needed here"),
    )
    .at(span)
}

fn coerce(node: Node, c: Option<Coercion>) -> Node {
    match c {
        Some(c) => Node::Coerce(Box::new(node), c),
        None => node,
    }
}

impl Checker<'_> {
    /// Check `e` and make it fit `wanted`.
    fn expect(&mut self, e: &Expr, wanted: &Type, what: &str) -> Result<Node, Error> {
        let (node, found) = self.expr(e)?;
        match fits(&found, wanted) {
            Some(c) => Ok(coerce(node, c)),
            None => {
                let mut err = mismatch(e.span, what, &found.settled(), wanted);
                if *wanted == Type::Bool && found == Type::Num {
                    err = err.hint("compare it, as in `x > 0`, or convert a flag with `num(...)` the other way");
                }
                if *wanted == Type::Num && found == Type::Bool {
                    err = err.hint("use `num(b)` to turn a flag into 1 or 0");
                }
                Err(err)
            }
        }
    }

    fn local(&self, name: &str) -> Option<(usize, Type)> {
        self.locals
            .iter()
            .enumerate()
            .rev()
            .find(|(_, (n, _))| n == name)
            .map(|(i, (_, t))| (i, t.clone()))
    }

    fn resolved(&mut self, span: Span, name: &str, what: String) {
        self.resolutions.push(Resolution {
            span,
            name: name.to_string(),
            what,
        });
    }

    fn bind(&mut self, var: &Ident, ty: Type) -> Result<usize, Error> {
        if is_reserved(&var.name) {
            return Err(Error::new(
                codes::E302,
                format!("\"{}\" is reserved and cannot name a variable", var.name),
            )
            .at(var.span)
            .hint("pick another name"));
        }
        let slot = self.locals.len();
        self.locals.push((var.name.clone(), ty));
        self.frame = self.frame.max(self.locals.len());
        Ok(slot)
    }

    fn expr(&mut self, e: &Expr) -> Result<(Node, Type), Error> {
        match &e.kind {
            ExprKind::Number(n) => Ok((Node::Const(Value::Num(*n)), Type::Num)),
            ExprKind::Str(s) => Ok((Node::Const(Value::str(s)), Type::Str)),
            ExprKind::Bool(b) => Ok((Node::Const(Value::Bool(*b)), Type::Bool)),
            ExprKind::Colour(c) => Ok((Node::Const(Value::Colour(*c)), Type::Colour)),
            ExprKind::Name(name) => self.name(name, e.span),
            ExprKind::List(items) => self.list(items),
            ExprKind::ListComp(c) => {
                let (lp, body) = self.comprehension(c)?;
                Ok((Node::ListComp(lp), Type::list(body)))
            }
            ExprKind::Generator(_) => Err(Error::new(
                codes::E101,
                "a generator only works as the argument of sum, prod, min, max, mean or count",
            )
            .at(e.span)),
            ExprKind::Unary { op, operand } => self.unary(*op, operand, e.span),
            ExprKind::Binary { op, left, right } => self.binary(*op, left, right, e.span),
            ExprKind::If {
                cond,
                then,
                otherwise,
            } => {
                let (c, ct) = self.expr(cond)?;
                if ct != Type::Bool {
                    return Err(Error::new(
                        codes::E206,
                        format!("the condition of an `if` is a {}, not a bool", ct.settled()),
                    )
                    .at(cond.span)
                    .hint("compare something, as in `if x > 0 then …`"));
                }
                let (a, at) = self.expr(then)?;
                let (b, bt) = self.expr(otherwise)?;
                let Some((ty, ca, cb)) = join(&at, &bt) else {
                    return Err(Error::new(
                        codes::E205,
                        format!(
                            "the branches of this `if` disagree: `then` is a {}, `else` is a {}",
                            at.settled(),
                            bt.settled()
                        ),
                    )
                    .at(e.span)
                    .hint("both branches must give the same type"));
                };
                Ok((
                    Node::If(
                        Box::new(c),
                        Box::new(coerce(a, ca)),
                        Box::new(coerce(b, cb)),
                    ),
                    ty,
                ))
            }
            ExprKind::Call { callee, args } => self.call(callee, args, e.span),
            ExprKind::Index { base, index } => self.index(base, index, e.span),
            ExprKind::Field { base, field } => self.field(base, field),
        }
    }

    fn name(&mut self, name: &str, span: Span) -> Result<(Node, Type), Error> {
        if let Some((slot, ty)) = self.local(name) {
            self.resolved(span, name, format!("bound variable: {}", ty.settled()));
            return Ok((Node::Local(slot), ty));
        }
        match self.scope.name(name) {
            Some(Named::Value(i, ty)) => {
                self.resolved(span, name, format!("value: {ty}"));
                return Ok((Node::Value(i), ty));
            }
            Some(Named::Compute(i, ty)) => {
                self.resolved(span, name, format!("compute node: {ty}"));
                return Ok((Node::Compute(i), ty));
            }
            Some(Named::Let(i, ty)) => {
                self.resolved(span, name, format!("let: {ty}"));
                return Ok((Node::Let(i), ty));
            }
            Some(Named::Function) => {
                return Err(Error::new(
                    codes::E201,
                    format!("\"{name}\" is a function, not a value"),
                )
                .at(span)
                .hint(format!("call it, as in `{name}(x)`")));
            }
            None => {}
        }
        if let Some((_, v)) = CONSTANTS.iter().find(|(c, _)| *c == name) {
            self.resolved(span, name, "constant".into());
            return Ok((Node::Const(Value::Num(*v)), Type::Num));
        }
        if builtins::lookup(name).is_some() || REDUCERS.contains(&name) {
            return Err(Error::new(
                codes::E201,
                format!("\"{name}\" is a function, not a value"),
            )
            .at(span)
            .hint(format!("call it, as in `{name}(…)`")));
        }
        let mut known: Vec<String> = self.locals.iter().map(|(n, _)| n.clone()).collect();
        known.extend(self.scope.known_names());
        known.extend(CONSTANTS.iter().map(|(c, _)| c.to_string()));
        let mut err = Error::new(
            codes::E301,
            format!("nothing in this figure is named \"{name}\""),
        )
        .at(span);
        if let Some(near) = nearest(name, known.iter().map(String::as_str)) {
            err = err.hint(format!("did you mean `{near}`?"));
        } else {
            err = err.hint("declare it as a value or a let, or check the spelling");
        }
        Err(err)
    }

    fn list(&mut self, items: &[Expr]) -> Result<(Node, Type), Error> {
        if items.is_empty() {
            return Ok((Node::List(Vec::new()), Type::list(Type::Any)));
        }
        let mut checked = Vec::new();
        for item in items {
            checked.push((self.expr(item)?, item.span));
        }
        if checked.len() == 2 && checked.iter().all(|((_, t), _)| *t == Type::Num) {
            let mut it = checked.into_iter();
            let ((a, _), _) = it.next().unwrap_or_else(|| unreachable!());
            let ((b, _), _) = it.next().unwrap_or_else(|| unreachable!());
            return Ok((Node::MakeVec(Box::new(a), Box::new(b)), Type::Pair));
        }
        // Every item must agree with the first, widening as items allow.
        let mut ty = checked[0].0 .1.clone();
        for ((_, t), span) in &checked[1..] {
            ty = match join(&ty, t) {
                Some((joined, _, _)) => joined,
                None => {
                    return Err(Error::new(
                        codes::E201,
                        format!(
                            "a list holds one type, but this item is a {} among {}s",
                            t.settled(),
                            ty.settled()
                        ),
                    )
                    .at(*span));
                }
            };
        }
        let nodes = checked
            .into_iter()
            .map(|((node, t), _)| coerce(node, fits(&t, &ty).flatten()))
            .collect();
        Ok((Node::List(nodes), Type::list(ty)))
    }

    /// Check a comprehension's iterable, variable, filter and body.
    fn comprehension(&mut self, c: &Comprehension) -> Result<(Loop, Type), Error> {
        let (iter, iter_ty) = self.expr(&c.iter)?;
        let (iter, elem) = match &iter_ty {
            Type::List(inner) => (iter, (**inner).clone()),
            Type::Pair => (coerce(iter, Some(Coercion::PairToList)), Type::Num),
            other => {
                return Err(Error::new(
                    codes::E201,
                    format!(
                        "a comprehension runs over a list, and this is a {}",
                        other.settled()
                    ),
                )
                .at(c.iter.span)
                .hint("use `range(a, b)` or `linspace(a, b, n)` to make one"));
            }
        };
        let depth = self.locals.len();
        let slot = self.bind(&c.var, elem.clone())?;
        self.resolved(
            c.var.span,
            &c.var.name,
            format!("bound variable: {}", elem.settled()),
        );
        let result = (|| {
            let cond = match &c.cond {
                Some(cond) => Some(Box::new(self.expect(cond, &Type::Bool, "the filter")?)),
                None => None,
            };
            let (body, body_ty) = self.expr(&c.body)?;
            Ok((
                Loop {
                    slot,
                    iter: Box::new(iter),
                    body: Box::new(body),
                    cond,
                },
                body_ty,
            ))
        })();
        self.locals.truncate(depth);
        result
    }

    fn unary(&mut self, op: UnOp, operand: &Expr, span: Span) -> Result<(Node, Type), Error> {
        let (node, ty) = self.expr(operand)?;
        match (op, &ty) {
            (UnOp::Not, Type::Bool) => Ok((Node::Not(Box::new(node)), Type::Bool)),
            (UnOp::Neg, Type::Num) => Ok((Node::Neg(Arith::Num, Box::new(node)), Type::Num)),
            (UnOp::Neg, Type::Vec | Type::Pair) => {
                Ok((Node::Neg(Arith::Vec, Box::new(node)), Type::Vec))
            }
            (UnOp::Neg, Type::Mat) => Ok((Node::Neg(Arith::Mat, Box::new(node)), Type::Mat)),
            (UnOp::Not, other) => Err(Error::new(
                codes::E204,
                format!("`not` needs a bool, and this is a {}", other.settled()),
            )
            .at(span)),
            (UnOp::Neg, other) => Err(Error::new(
                codes::E204,
                format!("a {} cannot be negated", other.settled()),
            )
            .at(span)),
        }
    }

    fn binary(
        &mut self,
        op: BinOp,
        left: &Expr,
        right: &Expr,
        span: Span,
    ) -> Result<(Node, Type), Error> {
        let (l, lt) = self.expr(left)?;
        let (r, rt) = self.expr(right)?;
        if matches!(op, BinOp::And | BinOp::Or) {
            for (t, e) in [(&lt, left), (&rt, right)] {
                if *t != Type::Bool {
                    return Err(Error::new(
                        codes::E204,
                        format!("`{}` joins bools, and this is a {}", op.text(), t.settled()),
                    )
                    .at(e.span)
                    .hint("compare it first, as in `x > 0`"));
                }
            }
            let (l, r) = (Box::new(l), Box::new(r));
            return Ok((
                if op == BinOp::And {
                    Node::And(l, r)
                } else {
                    Node::Or(l, r)
                },
                Type::Bool,
            ));
        }

        use Type::{Mat, Num, Str};
        let vec = |t: &Type| t.is_vec_like();
        let kind_ty: Option<(BinKind, Type)> = match op {
            BinOp::Add | BinOp::Sub => {
                let arith = match (&lt, &rt) {
                    (Num, Num) => Some((Arith::Num, Num)),
                    (a, b) if vec(a) && vec(b) => Some((Arith::Vec, Type::Vec)),
                    (Mat, Mat) => Some((Arith::Mat, Mat)),
                    _ => None,
                };
                arith.map(|(a, t)| {
                    (
                        if op == BinOp::Add {
                            BinKind::Add(a)
                        } else {
                            BinKind::Sub(a)
                        },
                        t,
                    )
                })
            }
            BinOp::Mul => match (&lt, &rt) {
                (Num, Num) => Some((BinKind::MulNum, Num)),
                (Num, b) if vec(b) => Some((BinKind::ScaleVec, Type::Vec)),
                (a, Num) if vec(a) => Some((BinKind::VecScale, Type::Vec)),
                (Mat, b) if vec(b) => Some((BinKind::MatVec, Type::Vec)),
                (Mat, Mat) => Some((BinKind::MatMat, Mat)),
                (Num, Mat) => Some((BinKind::ScaleMat { mat_left: false }, Mat)),
                (Mat, Num) => Some((BinKind::ScaleMat { mat_left: true }, Mat)),
                _ => None,
            },
            BinOp::Div => match (&lt, &rt) {
                (Num, Num) => Some((BinKind::DivNum, Num)),
                (a, Num) if vec(a) => Some((BinKind::DivVec, Type::Vec)),
                (Mat, Num) => Some((BinKind::DivMat, Mat)),
                _ => None,
            },
            BinOp::Mod => matches!((&lt, &rt), (Num, Num)).then_some((BinKind::Mod, Num)),
            BinOp::Pow => match (&lt, &rt) {
                (Num, Num) => Some((BinKind::PowNum, Num)),
                (Mat, Num) => Some((BinKind::PowMat, Mat)),
                _ => None,
            },
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                matches!((&lt, &rt), (Num, Num)).then(|| {
                    let k = match op {
                        BinOp::Lt => BinKind::Lt,
                        BinOp::Le => BinKind::Le,
                        BinOp::Gt => BinKind::Gt,
                        _ => BinKind::Ge,
                    };
                    (k, Type::Bool)
                })
            }
            BinOp::Eq | BinOp::Ne => match (&lt, &rt) {
                (Num, Num) | (Str, Str) | (Type::Bool, Type::Bool) => {
                    if lt == Num {
                        self.warnings.push(
                            Error::warning(codes::W101, "comparing numbers with `==` is exact")
                                .at(span)
                                .hint("compare with a tolerance, as in `abs(a - b) < 1e-9`"),
                        );
                    }
                    Some((
                        if op == BinOp::Eq {
                            BinKind::Eq
                        } else {
                            BinKind::Ne
                        },
                        Type::Bool,
                    ))
                }
                _ => None,
            },
            BinOp::And | BinOp::Or => unreachable!("handled above"),
        };
        match kind_ty {
            Some((kind, ty)) => Ok((Node::Binary(kind, Box::new(l), Box::new(r)), ty)),
            None => {
                let mut err = Error::new(
                    codes::E204,
                    format!(
                        "`{} {} {}` is not defined",
                        lt.settled(),
                        op.text(),
                        rt.settled()
                    ),
                )
                .at(span);
                if op == BinOp::Mul && vec(&lt) && vec(&rt) {
                    err = err.hint(
                        "for the dot product write `dot(a, b)`; for the cross product `cross(a, b)`; \
                         for complex multiplication `cmul(a, b)`",
                    );
                } else if op.is_comparison() && (vec(&lt) || vec(&rt)) {
                    err = err
                        .hint("compare lengths with `norm(v)`, or components with `.x` and `.y`");
                } else if lt == Str || rt == Str {
                    err = err.hint("strings only compare with `==` and `!=`");
                }
                Err(err)
            }
        }
    }

    fn index(&mut self, base: &Expr, index: &Expr, span: Span) -> Result<(Node, Type), Error> {
        let (b, bt) = self.expr(base)?;
        let i = self.expect(index, &Type::Num, "an index")?;
        match bt {
            Type::List(inner) => Ok((Node::IndexList(Box::new(b), Box::new(i)), *inner)),
            Type::Pair => Ok((
                Node::IndexList(Box::new(coerce(b, Some(Coercion::PairToList))), Box::new(i)),
                Type::Num,
            )),
            Type::Mat => Ok((
                Node::IndexMat(Box::new(b), Box::new(i)),
                Type::list(Type::Num),
            )),
            Type::Vec => Err(Error::new(codes::E201, "a vector is not indexed")
                .at(span)
                .hint("read its components with `.x` and `.y`")),
            other => Err(Error::new(
                codes::E201,
                format!("a {} cannot be indexed", other.settled()),
            )
            .at(span)),
        }
    }

    fn field(&mut self, base: &Expr, field: &Ident) -> Result<(Node, Type), Error> {
        let (b, bt) = self.expr(base)?;
        match (&bt, field.name.as_str()) {
            (t, "x") if t.is_vec_like() => Ok((Node::VecX(Box::new(b)), Type::Num)),
            (t, "y") if t.is_vec_like() => Ok((Node::VecY(Box::new(b)), Type::Num)),
            (Type::Record(rec), name) => match rec.field(name) {
                Some(t) => Ok((Node::Field(Box::new(b), name.to_string()), t.clone())),
                None => {
                    let mut err = Error::new(
                        codes::E207,
                        format!("a {} record has no field \"{name}\"", rec.name),
                    )
                    .at(field.span)
                    .options(rec.members());
                    if rec.method(name).is_some() {
                        err =
                            err.hint(format!("`{name}` is a method: call it, as in `.{name}(…)`"));
                    } else if let Some(near) =
                        nearest(name, rec.fields.iter().map(|(n, _)| n.as_str()))
                    {
                        err = err.hint(format!("did you mean `.{near}`?"));
                    }
                    Err(err)
                }
            },
            (t, name) if t.is_vec_like() => Err(Error::new(
                codes::E207,
                format!("a vector has no field \"{name}\""),
            )
            .at(field.span)
            .options(["x", "y"])),
            (other, name) => Err(Error::new(
                codes::E207,
                format!("a {} has no field \"{name}\"", other.settled()),
            )
            .at(field.span)),
        }
    }

    fn call(&mut self, callee: &Expr, args: &[Expr], span: Span) -> Result<(Node, Type), Error> {
        // A method on a record: `orbit.at(s)`.
        if let ExprKind::Field { base, field } = &callee.kind {
            return self.method(base, field, args, span);
        }
        let ExprKind::Name(name) = &callee.kind else {
            return Err(
                Error::new(codes::E201, "only a function name can be called").at(callee.span),
            );
        };
        if let [Expr {
            kind: ExprKind::Generator(c),
            span: gen_span,
        }] = args
        {
            return self.reduce(name, c, *gen_span, callee.span);
        }
        if self.local(name).is_some()
            || matches!(
                self.scope.name(name),
                Some(Named::Value(..) | Named::Let(..) | Named::Compute(..))
            )
        {
            return Err(Error::new(
                codes::E201,
                format!("\"{name}\" is a value, not a function"),
            )
            .at(callee.span)
            .hint("to multiply, write `*` explicitly"));
        }
        if let Some((id, builtin)) = builtins::lookup(name) {
            return self.builtin(id, builtin, args, span, callee.span);
        }
        // A user function, compiled for the argument types it is called with.
        let mut checked = Vec::new();
        for arg in args {
            checked.push(self.expr(arg)?);
        }
        let types: Vec<Type> = checked.iter().map(|(_, t)| t.settled()).collect();
        match self.scope.function(name, &types, span) {
            Some(Ok((fn_id, ret))) => {
                self.resolved(callee.span, name, format!("user function -> {ret}"));
                let nodes = checked
                    .into_iter()
                    .zip(&types)
                    .map(|((node, t), want)| coerce(node, fits(&t, want).flatten()))
                    .collect();
                Ok((Node::CallUser(fn_id, nodes), ret))
            }
            Some(Err(err)) => Err(err),
            None => {
                let mut known: Vec<String> = builtins::names().map(str::to_string).collect();
                known.extend(REDUCERS.iter().map(|s| s.to_string()));
                known.extend(self.scope.known_names());
                let mut err = Error::new(codes::E202, format!("there is no function \"{name}\""))
                    .at(callee.span);
                err = match nearest(name, known.iter().map(String::as_str)) {
                    Some(near) => err.hint(format!("did you mean `{near}`?")),
                    None => err.hint(format!("define it in `let` as \"{name}(x)\": …")),
                };
                Err(err)
            }
        }
    }

    fn builtin(
        &mut self,
        id: usize,
        builtin: &'static Builtin,
        args: &[Expr],
        span: Span,
        name_span: Span,
    ) -> Result<(Node, Type), Error> {
        let mut rest = args;
        let mut user_fn = None;
        if builtin.takes_fn {
            let Some(first) = args.first() else {
                return Err(Error::new(
                    codes::E203,
                    format!("`{}` needs a function as its first argument", builtin.name),
                )
                .at(span)
                .hint(format!(
                    "as in `{}(f, …)` with \"f(x)\" defined in `let`",
                    builtin.name
                )));
            };
            let ExprKind::Name(fname) = &first.kind else {
                return Err(Error::new(
                    codes::E201,
                    format!(
                        "the first argument of `{}` is the name of a function",
                        builtin.name
                    ),
                )
                .at(first.span)
                .hint("define it in `let` as \"f(x)\": … and pass `f`"));
            };
            match self.scope.function(fname, &[Type::Num], first.span) {
                Some(Ok((fn_id, Type::Num))) => user_fn = Some(fn_id),
                Some(Ok((_, other))) => {
                    return Err(Error::new(
                        codes::E201,
                        format!(
                            "`{fname}` gives a {other}, and `{}` needs a function giving a num",
                            builtin.name
                        ),
                    )
                    .at(first.span));
                }
                Some(Err(err)) => return Err(err),
                None => {
                    return Err(Error::new(
                        codes::E301,
                        format!(
                            "there is no function \"{fname}\" to pass to `{}`",
                            builtin.name
                        ),
                    )
                    .at(first.span)
                    .hint(format!("define it in `let` as \"{fname}(x)\": …")));
                }
            }
            rest = &args[1..];
        }

        let mut checked = Vec::new();
        for arg in rest {
            checked.push((self.expr(arg)?, arg.span));
        }
        let found: Vec<Type> = checked.iter().map(|((_, t), _)| t.clone()).collect();

        let mut arity_ok = false;
        for overload in builtin.overloads {
            let fixed = overload.params.len();
            let count_ok = match overload.rest {
                Some(_) => found.len() >= fixed,
                None => found.len() == fixed,
            };
            if !count_ok {
                continue;
            }
            arity_ok = true;
            let pats: Vec<Pat> = (0..found.len())
                .map(|i| {
                    overload
                        .params
                        .get(i)
                        .copied()
                        .or(overload.rest)
                        .unwrap_or(Pat::T)
                })
                .collect();
            if let Some((coercions, t)) = match_pats(&pats, &found) {
                let ret = match overload.ret {
                    Ret::Pat(p) => match concrete(p, &t) {
                        Some(ty) => ty,
                        None => continue,
                    },
                    Ret::Record(r) => Type::Record(r()),
                };
                let nodes: Vec<Node> = checked
                    .into_iter()
                    .zip(coercions)
                    .map(|(((node, _), _), c)| coerce(node, c))
                    .collect();
                self.resolved(
                    name_span,
                    builtin.name,
                    format!("library function -> {ret}"),
                );
                let node = match user_fn {
                    Some(f) => Node::BuiltinWithFn(id, f, nodes),
                    None => Node::Builtin(id, nodes),
                };
                return Ok((node, ret));
            }
        }

        let accepted: Vec<String> = builtin
            .overloads
            .iter()
            .map(|o| format!("{}{}", builtin.name, o.describe()))
            .collect();
        let found_text: Vec<String> = found.iter().map(|t| t.settled().to_string()).collect();
        if !arity_ok {
            let n = found.len();
            return Err(Error::new(
                codes::E203,
                format!(
                    "`{}` does not take {n} argument{}",
                    builtin.name,
                    if n == 1 { "" } else { "s" }
                ),
            )
            .at(span)
            .options(accepted));
        }
        Err(Error::new(
            codes::E201,
            format!(
                "`{}` does not take ({})",
                builtin.name,
                found_text.join(", ")
            ),
        )
        .at(span)
        .options(accepted))
    }

    fn method(
        &mut self,
        base: &Expr,
        field: &Ident,
        args: &[Expr],
        span: Span,
    ) -> Result<(Node, Type), Error> {
        let (b, bt) = self.expr(base)?;
        let Type::Record(rec) = &bt else {
            return Err(Error::new(
                codes::E207,
                format!("a {} has no method \"{}\"", bt.settled(), field.name),
            )
            .at(field.span));
        };
        let Some(method) = rec.method(&field.name).cloned() else {
            return Err(Error::new(
                codes::E207,
                format!("a {} record has no method \"{}\"", rec.name, field.name),
            )
            .at(field.span)
            .options(rec.members()));
        };
        if args.len() != method.params.len() {
            return Err(Error::new(
                codes::E203,
                format!(
                    "`.{}` takes {} argument(s), not {}",
                    field.name,
                    method.params.len(),
                    args.len()
                ),
            )
            .at(span));
        }
        let mut nodes = Vec::new();
        for (arg, want) in args.iter().zip(&method.params) {
            nodes.push(self.expect(arg, want, "this argument")?);
        }
        Ok((
            Node::Method(Box::new(b), field.name.clone(), nodes),
            method.ret,
        ))
    }

    fn reduce(
        &mut self,
        name: &str,
        c: &Comprehension,
        span: Span,
        name_span: Span,
    ) -> Result<(Node, Type), Error> {
        if !REDUCERS.contains(&name) {
            return Err(
                Error::new(codes::E201, format!("`{name}` does not take a generator"))
                    .at(span)
                    .hint("only sum, prod, min, max, mean and count do; make a list with [...]")
                    .options(REDUCERS.iter().copied()),
            );
        }
        let (lp, body) = self.comprehension(c)?;
        let bad = |want: &str| {
            Error::new(
                codes::E201,
                format!(
                    "`{name}` needs {want}, and each item here is a {}",
                    body.settled()
                ),
            )
            .at(c.body.span)
        };
        let (reducer, ty) = match name {
            "count" => (Reducer::Count, Type::Num),
            "sum" | "mean" => {
                let arith = match &body {
                    Type::Num => Arith::Num,
                    t if t.is_vec_like() => Arith::Vec,
                    _ => return Err(bad("numbers or vectors")),
                };
                let ty = if arith == Arith::Num {
                    Type::Num
                } else {
                    Type::Vec
                };
                (
                    if name == "sum" {
                        Reducer::Sum(arith)
                    } else {
                        Reducer::Mean(arith)
                    },
                    ty,
                )
            }
            _ => {
                if body != Type::Num {
                    return Err(bad("numbers"));
                }
                let r = match name {
                    "prod" => Reducer::Prod,
                    "min" => Reducer::Min,
                    _ => Reducer::Max,
                };
                (r, Type::Num)
            }
        };
        self.resolved(name_span, name, format!("reduction -> {ty}"));
        Ok((Node::Reduce(reducer, lp), ty))
    }
}

/// Match argument types against patterns, binding `T`; the coercions each
/// argument needs and what `T` became.
fn match_pats(pats: &[Pat], found: &[Type]) -> Option<(Vec<Option<Coercion>>, Option<Type>)> {
    let mut t: Option<Type> = None;
    // Bind T from the arguments that determine it.
    for (p, f) in pats.iter().zip(found) {
        let candidate = match p {
            Pat::ListT => f.element(),
            Pat::T => Some(f.settled()),
            _ => None,
        };
        if let Some(c) = candidate {
            t = match &t {
                None => Some(c),
                Some(prev) => Some(join(prev, &c)?.0),
            };
        }
    }
    let mut coercions = Vec::new();
    for (p, f) in pats.iter().zip(found) {
        let want = concrete(*p, &t)?;
        coercions.push(fits(f, &want)?);
    }
    Some((coercions, t))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Method, RecordType};
    use std::sync::Arc;

    /// A figure with a few values, a let, a compute record and one function.
    struct Figure;

    fn orbit() -> Arc<RecordType> {
        Arc::new(RecordType {
            name: "integrate".into(),
            fields: vec![("q".into(), Type::list(Type::Num)), ("n".into(), Type::Num)],
            methods: vec![Method {
                name: "at".into(),
                params: vec![Type::Num],
                ret: Type::Record(Arc::new(RecordType {
                    name: "state".into(),
                    fields: vec![("q".into(), Type::Num)],
                    methods: vec![],
                })),
            }],
        })
    }

    impl Scope for Figure {
        fn name(&mut self, name: &str) -> Option<Named> {
            Some(match name {
                "x" => Named::Value(0, Type::Num),
                "p" => Named::Value(1, Type::Vec),
                "mode" => Named::Value(2, Type::Str),
                "on" => Named::Value(3, Type::Bool),
                "M" => Named::Value(4, Type::Mat),
                "xs" => Named::Let(0, Type::list(Type::Num)),
                "slope" => Named::Let(1, Type::Num),
                "orbit" => Named::Compute(0, Type::Record(orbit())),
                "f" => Named::Function,
                _ => return None,
            })
        }

        fn function(
            &mut self,
            name: &str,
            args: &[Type],
            _: Span,
        ) -> Option<Result<(FnId, Type), Error>> {
            (name == "f").then(|| match args {
                [Type::Num] => Ok((0, Type::Num)),
                [Type::Vec] => Ok((1, Type::Vec)),
                other => Err(Error::new(codes::E201, format!("f cannot take {other:?}"))),
            })
        }

        fn known_names(&self) -> Vec<String> {
            ["x", "p", "mode", "on", "M", "xs", "slope", "orbit", "f"]
                .map(String::from)
                .to_vec()
        }
    }

    fn ty(text: &str) -> Type {
        check_text(text, &mut Figure, &[], None)
            .map(|c| c.ty)
            .unwrap_or_else(|e| panic!("{text}: {e} ({:?})", e.hint))
    }

    fn err(text: &str) -> Error {
        check_text(text, &mut Figure, &[], None).unwrap_err()
    }

    #[test]
    fn arithmetic_rows_of_the_table() {
        assert_eq!(ty("x + 1"), Type::Num);
        assert_eq!(ty("p + [1, 2]"), Type::Vec);
        assert_eq!(ty("M + M"), Type::Mat);
        assert_eq!(ty("2 * p"), Type::Vec);
        assert_eq!(ty("p * 2"), Type::Vec);
        assert_eq!(ty("M * p"), Type::Vec);
        assert_eq!(ty("M * M"), Type::Mat);
        assert_eq!(ty("2 * M"), Type::Mat);
        assert_eq!(ty("M * 2"), Type::Mat);
        assert_eq!(ty("p / 2"), Type::Vec);
        assert_eq!(ty("M / 2"), Type::Mat);
        assert_eq!(ty("x % 3"), Type::Num);
        assert_eq!(ty("x ** 2"), Type::Num);
        assert_eq!(ty("M ** 3"), Type::Mat);
        assert_eq!(ty("-p"), Type::Vec);
        assert_eq!(ty("-M"), Type::Mat);
    }

    #[test]
    fn a_vector_times_a_vector_names_the_three_products() {
        let e = err("p * p");
        assert_eq!(e.code, codes::E204);
        let hint = e.hint.unwrap();
        assert!(hint.contains("dot(a, b)") && hint.contains("cross") && hint.contains("cmul"));
        assert_eq!(err("2 / p").code, codes::E204);
        assert_eq!(err("p ** 2").code, codes::E204);
        assert_eq!(err("\"a\" + \"b\"").code, codes::E204);
    }

    #[test]
    fn comparisons_and_logic() {
        assert_eq!(ty("x < 1 and on"), Type::Bool);
        assert_eq!(ty("mode == \"damped\""), Type::Bool);
        assert_eq!(ty("not on or x >= 0"), Type::Bool);
        assert_eq!(err("p < p").code, codes::E204);
        assert_eq!(err("x and on").code, codes::E204);
        assert_eq!(err("not x").code, codes::E204);
    }

    #[test]
    fn exact_float_equality_is_a_warning() {
        let checked = check_text("x == 1", &mut Figure, &[], None).unwrap();
        assert_eq!(checked.warnings[0].code, codes::W101);
        let checked = check_text("mode == 'a'", &mut Figure, &[], None).unwrap();
        assert!(checked.warnings.is_empty());
    }

    #[test]
    fn if_needs_a_bool_and_agreeing_branches() {
        assert_eq!(ty("if on then 1 else 2"), Type::Num);
        assert_eq!(ty("if on then p else [0, 0]"), Type::Vec);
        assert_eq!(err("if x then 1 else 2").code, codes::E206);
        assert_eq!(err("if on then 1 else 'a'").code, codes::E205);
    }

    #[test]
    fn a_pair_is_a_vector_unless_a_list_is_wanted() {
        assert_eq!(ty("[1, 2]"), Type::Vec);
        assert_eq!(ty("[1, 2, 3]"), Type::list(Type::Num));
        assert_eq!(ty("[[1, 2], [3, 4]]"), Type::list(Type::Vec));
        assert_eq!(ty("[]"), Type::list(Type::Any));
        let checked = check_text("[1, 2]", &mut Figure, &[], Some(&Type::list(Type::Num))).unwrap();
        assert!(matches!(
            checked.node,
            Node::Coerce(_, Coercion::PairToList)
        ));
        assert_eq!(checked.ty, Type::list(Type::Num));
        assert_eq!(ty("min([3, 4])"), Type::Num);
        assert_eq!(err("[1, 'a']").code, codes::E201);
    }

    #[test]
    fn names_resolve_in_order_with_suggestions() {
        assert_eq!(ty("slope"), Type::Num);
        assert_eq!(ty("pi * 2"), Type::Num);
        let e = err("slop + 1");
        assert_eq!(e.code, codes::E301);
        assert_eq!(e.hint.as_deref(), Some("did you mean `slope`?"));
        assert_eq!(err("f + 1").code, codes::E201);
        assert_eq!(err("sqrt + 1").code, codes::E201);
    }

    #[test]
    fn bound_variables_shadow_values() {
        let bound = [("x".to_string(), Type::Vec)];
        let checked = check_text("x.y", &mut Figure, &bound, None).unwrap();
        assert_eq!(checked.ty, Type::Num);
        assert_eq!(checked.node, Node::VecY(Box::new(Node::Local(0))));
    }

    #[test]
    fn library_calls_pick_an_overload() {
        assert_eq!(ty("sqrt(x)"), Type::Num);
        assert_eq!(ty("max(1, 2, 3)"), Type::Num);
        assert_eq!(ty("max(xs)"), Type::Num);
        let e = err("sqrt(p)");
        assert_eq!(e.code, codes::E201);
        assert!(e.options[0].starts_with("sqrt(num)"));
        assert_eq!(err("sqrt(1, 2)").code, codes::E203);
        // One argument fits `max(list)` by count, so the fault is its type.
        let e = err("max(1)");
        assert_eq!(e.code, codes::E201);
        assert_eq!(e.options.len(), 2);
        let e = err("sqtr(2)");
        assert_eq!(e.code, codes::E202);
        assert_eq!(e.hint.as_deref(), Some("did you mean `sqrt`?"));
        assert_eq!(err("x(2)").code, codes::E201);
    }

    #[test]
    fn user_functions_are_instantiated_per_argument_type() {
        assert_eq!(ty("f(x)"), Type::Num);
        assert_eq!(ty("f(p)"), Type::Vec);
        assert_eq!(ty("f([1, 2])"), Type::Vec);
        assert_eq!(err("f('a')").code, codes::E201);
    }

    #[test]
    fn comprehensions_and_reductions() {
        assert_eq!(ty("[k * x for k in xs]"), Type::list(Type::Num));
        assert_eq!(
            ty("[[k, k] for k in xs if k > 0]"),
            Type::list(Type::Pair).settled()
        );
        assert_eq!(ty("sum(k for k in xs)"), Type::Num);
        assert_eq!(ty("sum([k, 1] for k in xs)"), Type::Vec);
        assert_eq!(ty("count(k for k in xs if k > 2)"), Type::Num);
        assert_eq!(ty("max(k * k for k in [1, 2, 3])"), Type::Num);
        assert_eq!(ty("sum(k for k in [1, 2])"), Type::Num);
        assert_eq!(err("[k for k in x]").code, codes::E201);
        assert_eq!(err("sqrt(k for k in xs)").code, codes::E201);
        assert_eq!(err("[k for k in xs if k]").code, codes::E201);
        assert_eq!(err("[k for pi in xs]").code, codes::E302);
    }

    #[test]
    fn fields_methods_and_indexing() {
        assert_eq!(ty("p.x + p.y"), Type::Num);
        assert_eq!(ty("orbit.n"), Type::Num);
        assert_eq!(ty("orbit.q[0]"), Type::Num);
        assert_eq!(ty("orbit.at(2).q"), Type::Num);
        assert_eq!(ty("M[0][1]"), Type::Num);
        assert_eq!(ty("xs[0]"), Type::Num);
        let e = err("orbit.qq");
        assert_eq!(e.code, codes::E207);
        assert_eq!(e.hint.as_deref(), Some("did you mean `.q`?"));
        assert_eq!(err("orbit.at").code, codes::E207);
        assert_eq!(err("p.z").code, codes::E207);
        assert_eq!(err("p[0]").code, codes::E201);
        assert_eq!(err("orbit.at(1, 2)").code, codes::E203);
    }

    #[test]
    fn an_expected_type_is_enforced_with_a_hint() {
        let e = check_text("x", &mut Figure, &[], Some(&Type::Bool)).unwrap_err();
        assert_eq!(e.code, codes::E201);
        assert!(e.hint.is_some());
        let e = check_text("on", &mut Figure, &[], Some(&Type::Num)).unwrap_err();
        assert_eq!(
            e.hint.as_deref(),
            Some("use `num(b)` to turn a flag into 1 or 0")
        );
    }

    #[test]
    fn resolutions_say_what_each_name_is() {
        let checked = check_text("slope * x + sqrt(pi)", &mut Figure, &[], None).unwrap();
        let whats: Vec<&str> = checked
            .resolutions
            .iter()
            .map(|r| r.what.as_str())
            .collect();
        assert!(whats.contains(&"let: num"));
        assert!(whats.contains(&"value: num"));
        assert!(whats.contains(&"constant"));
    }
}
