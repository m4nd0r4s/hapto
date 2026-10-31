//! Running checked formulas.
//!
//! Evaluation cannot fail on a type: the checker has settled every one. It
//! fails in exactly one way — running out of fuel (E604), so no formula can
//! hang a reader's page — and everything else that goes wrong at a particular
//! state becomes no value (SPEC-2.0 §4.8): the figure skips a mark, prints
//! "—", and carries on.

use std::sync::Arc;

use crate::builtins::{self, Host};
use crate::check::Checked;
use crate::error::{codes, Error};
use crate::ir::{Arith, BinKind, FnId, Loop, Node, Reducer};
use crate::linalg;
use crate::module::Module;
use crate::types::Coercion;
use crate::value::Value;

/// The default budget per state change: 50 million operations (§19.3).
pub const DEFAULT_FUEL: u64 = 50_000_000;

/// What a formula reads from outside itself.
pub struct Env<'a> {
    /// The figure's values, in declaration order.
    pub values: &'a [Value],
    /// Compute node results, in declaration order.
    pub compute: &'a [Value],
    /// Theme colours by token name.
    pub palette: &'a dyn Fn(&str) -> Option<[f64; 4]>,
}

/// No theme: every palette lookup is no value.
pub fn no_palette(_: &str) -> Option<[f64; 4]> {
    None
}

/// Let values that survive between evaluations.
#[derive(Debug, Clone, Default)]
pub struct Cache {
    lets: Vec<Option<Value>>,
    /// How many times a let was actually computed (not served from cache).
    pub computed: u64,
}

impl Cache {
    pub fn new(module: &Module) -> Self {
        Cache {
            lets: vec![None; module.lets().len()],
            computed: 0,
        }
    }

    /// Forget one let, so the next read recomputes it.
    pub fn forget(&mut self, let_index: usize) {
        if let Some(slot) = self.lets.get_mut(let_index) {
            *slot = None;
        }
    }

    pub fn forget_all(&mut self) {
        self.lets.iter_mut().for_each(|slot| *slot = None);
    }
}

/// One evaluation pass: a module, an environment, a cache and a budget.
pub struct Evaluation<'a> {
    module: &'a Module,
    env: Env<'a>,
    cache: &'a mut Cache,
    fuel: u64,
    budget: u64,
}

impl<'a> Evaluation<'a> {
    pub fn new(module: &'a Module, env: Env<'a>, cache: &'a mut Cache, budget: u64) -> Self {
        if cache.lets.len() != module.lets().len() {
            *cache = Cache::new(module);
        }
        Evaluation {
            module,
            env,
            cache,
            fuel: budget,
            budget,
        }
    }

    /// Operations spent so far in this pass.
    pub fn spent(&self) -> u64 {
        self.budget - self.fuel
    }

    /// Evaluate a checked formula with `bound` in its first local slots.
    pub fn eval(&mut self, checked: &Checked, bound: &[Value]) -> Result<Value, Error> {
        let mut frame = vec![Value::Nothing; checked.frame.max(bound.len())];
        frame[..bound.len()].clone_from_slice(bound);
        self.node(&checked.node, &mut frame)
    }

    /// The value of a let, from the cache when it is still valid.
    pub fn let_value(&mut self, index: usize) -> Result<Value, Error> {
        if let Some(Some(v)) = self.cache.lets.get(index) {
            return Ok(v.clone());
        }
        let checked = &self.module.lets()[index].checked;
        let mut frame = vec![Value::Nothing; checked.frame];
        let value = self.node(&checked.node, &mut frame)?;
        self.cache.lets[index] = Some(value.clone());
        self.cache.computed += 1;
        Ok(value)
    }

    fn call_instance(&mut self, id: FnId, args: Vec<Value>) -> Result<Value, Error> {
        let checked = &self.module.instance(id).checked;
        let mut frame = vec![Value::Nothing; checked.frame.max(args.len())];
        for (slot, arg) in frame.iter_mut().zip(args) {
            *slot = arg;
        }
        self.node(&checked.node, &mut frame)
    }

    fn node(&mut self, node: &Node, frame: &mut Vec<Value>) -> Result<Value, Error> {
        self.charge(1)?;
        Ok(match node {
            Node::Const(v) => v.clone(),
            Node::Local(i) => frame.get(*i).cloned().unwrap_or(Value::Nothing),
            Node::Value(i) => self.env.values.get(*i).cloned().unwrap_or(Value::Nothing),
            Node::Compute(i) => self.env.compute.get(*i).cloned().unwrap_or(Value::Nothing),
            Node::Let(i) => self.let_value(*i)?,
            Node::Neg(arith, a) => {
                let a = self.node(a, frame)?;
                match (arith, &a) {
                    (Arith::Num, _) => finite(-a.num()),
                    (Arith::Vec, Value::Vec([x, y])) => Value::Vec([-x, -y]),
                    (Arith::Mat, Value::Mat(m)) => Value::Mat(Arc::new(linalg::scale(m, -1.0))),
                    _ => Value::Nothing,
                }
            }
            Node::Not(a) => match self.node(a, frame)?.bool() {
                Some(b) => Value::Bool(!b),
                None => Value::Nothing,
            },
            Node::And(a, b) => match self.node(a, frame)?.bool() {
                Some(false) => Value::Bool(false),
                Some(true) => match self.node(b, frame)?.bool() {
                    Some(b) => Value::Bool(b),
                    None => Value::Nothing,
                },
                None => Value::Nothing,
            },
            Node::Or(a, b) => match self.node(a, frame)?.bool() {
                Some(true) => Value::Bool(true),
                Some(false) => match self.node(b, frame)?.bool() {
                    Some(b) => Value::Bool(b),
                    None => Value::Nothing,
                },
                None => Value::Nothing,
            },
            Node::Binary(kind, a, b) => {
                let a = self.node(a, frame)?;
                let b = self.node(b, frame)?;
                self.charge_mat(&a, &b)?;
                binary(*kind, &a, &b)
            }
            Node::If(c, a, b) => match self.node(c, frame)?.bool() {
                Some(true) => self.node(a, frame)?,
                Some(false) => self.node(b, frame)?,
                None => Value::Nothing,
            },
            Node::MakeVec(a, b) => {
                Value::Vec([self.node(a, frame)?.num(), self.node(b, frame)?.num()])
            }
            Node::List(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(self.node(item, frame)?);
                }
                Value::list(out)
            }
            Node::Builtin(id, args) => {
                let args = self.args(args, frame)?;
                (builtins::get(*id).imp)(self, None, &args)?
            }
            Node::BuiltinWithFn(id, f, args) => {
                let args = self.args(args, frame)?;
                (builtins::get(*id).imp)(self, Some(*f), &args)?
            }
            Node::CallUser(f, args) => {
                let args = self.args(args, frame)?;
                self.call_instance(*f, args)?
            }
            Node::ListComp(lp) => {
                let mut out = Vec::new();
                self.each(lp, frame, &mut |v| {
                    out.push(v);
                    Ok(())
                })?;
                Value::list(out)
            }
            Node::Reduce(reducer, lp) => self.reduce(*reducer, lp, frame)?,
            Node::VecX(a) => match self.node(a, frame)? {
                Value::Vec([x, _]) => Value::Num(x),
                _ => Value::Num(f64::NAN),
            },
            Node::VecY(a) => match self.node(a, frame)? {
                Value::Vec([_, y]) => Value::Num(y),
                _ => Value::Num(f64::NAN),
            },
            Node::Field(a, name) => match self.node(a, frame)? {
                Value::Record(r) => r.field(name),
                _ => Value::Nothing,
            },
            Node::Method(a, name, args) => {
                let base = self.node(a, frame)?;
                let args = self.args(args, frame)?;
                match base {
                    Value::Record(r) => r.call(name, &args),
                    _ => Value::Nothing,
                }
            }
            Node::IndexList(a, i) => {
                let list = self.node(a, frame)?;
                let i = self.node(i, frame)?.num();
                match (list.items(), index(i)) {
                    (Some(items), Some(i)) => items.get(i).cloned().unwrap_or(Value::Nothing),
                    _ => Value::Nothing,
                }
            }
            Node::IndexMat(a, i) => {
                let m = self.node(a, frame)?;
                let i = self.node(i, frame)?.num();
                match (&m, index(i)) {
                    (Value::Mat(m), Some(i)) if i < m.rows => Value::nums(m.row(i).iter().copied()),
                    _ => Value::Nothing,
                }
            }
            Node::Coerce(a, c) => coerce(self.node(a, frame)?, c),
        })
    }

    fn args(&mut self, args: &[Node], frame: &mut Vec<Value>) -> Result<Vec<Value>, Error> {
        let mut out = Vec::with_capacity(args.len());
        for arg in args {
            out.push(self.node(arg, frame)?);
        }
        Ok(out)
    }

    /// Run a comprehension's body for every item that passes its filter.
    fn each(
        &mut self,
        lp: &Loop,
        frame: &mut Vec<Value>,
        visit: &mut dyn FnMut(Value) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let iter = self.node(&lp.iter, frame)?;
        let Some(items) = iter.items() else {
            return Ok(());
        };
        if frame.len() <= lp.slot {
            frame.resize(lp.slot + 1, Value::Nothing);
        }
        for item in items.iter() {
            frame[lp.slot] = item.clone();
            if let Some(cond) = &lp.cond {
                if self.node(cond, frame)?.bool() != Some(true) {
                    continue;
                }
            }
            let v = self.node(&lp.body, frame)?;
            visit(v)?;
        }
        Ok(())
    }

    fn reduce(
        &mut self,
        reducer: Reducer,
        lp: &Loop,
        frame: &mut Vec<Value>,
    ) -> Result<Value, Error> {
        let mut items = Vec::new();
        self.each(lp, frame, &mut |v| {
            items.push(v);
            Ok(())
        })?;
        Ok(reduce(reducer, &items))
    }

    /// Matrix products cost their size cubed; charge for it.
    fn charge_mat(&mut self, a: &Value, b: &Value) -> Result<(), Error> {
        if let (Value::Mat(a), Value::Mat(b)) = (a, b) {
            self.charge((a.rows * a.cols * b.cols) as u64)?;
        }
        Ok(())
    }
}

impl Host for Evaluation<'_> {
    fn charge(&mut self, ops: u64) -> Result<(), Error> {
        if ops > self.fuel {
            self.fuel = 0;
            return Err(Error::new(
                codes::E604,
                format!("this figure took more than {} operations to evaluate", self.budget),
            )
            .hint("lower a count, a number of steps or a resolution; or move the work into a compute node"));
        }
        self.fuel -= ops;
        Ok(())
    }

    fn palette(&self, name: &str) -> Option<[f64; 4]> {
        (self.env.palette)(name)
    }

    fn call_fn(&mut self, f: FnId, x: f64) -> Result<f64, Error> {
        Ok(self.call_instance(f, vec![Value::Num(x)])?.num())
    }
}

/// A list index from a number: whole, non-negative, or none.
fn index(i: f64) -> Option<usize> {
    let r = i.round();
    ((i - r).abs() < 1e-9 && r >= 0.0 && r.is_finite()).then_some(r as usize)
}

fn coerce(v: Value, c: &Coercion) -> Value {
    match (c, v) {
        (Coercion::PairToList, Value::Vec([x, y])) => Value::nums([x, y]),
        (Coercion::Elements(inner), Value::List(items)) => {
            Value::list(items.iter().map(|i| coerce(i.clone(), inner)).collect())
        }
        (_, other) => other,
    }
}

fn mat(v: &Value) -> Option<&crate::value::Matrix> {
    match v {
        Value::Mat(m) => Some(m),
        _ => None,
    }
}

fn mat_value(m: Option<crate::value::Matrix>) -> Value {
    m.map(|m| Value::Mat(Arc::new(m))).unwrap_or(Value::Nothing)
}

fn vec2(v: &Value) -> Option<[f64; 2]> {
    match v {
        Value::Vec(v) => Some(*v),
        _ => None,
    }
}

/// A number result; infinities are no value too (`1/0` is not a quantity).
fn finite(n: f64) -> Value {
    Value::Num(if n.is_finite() { n } else { f64::NAN })
}

fn binary(kind: BinKind, a: &Value, b: &Value) -> Value {
    let (x, y) = (a.num(), b.num());
    let vec_op = |f: fn(f64, f64) -> f64| match (vec2(a), vec2(b)) {
        (Some(p), Some(q)) => Value::Vec([f(p[0], q[0]), f(p[1], q[1])]),
        _ => Value::Nothing,
    };
    let scaled = |v: &Value, k: f64| match vec2(v) {
        Some(p) => Value::Vec([p[0] * k, p[1] * k]),
        None => Value::Nothing,
    };
    let compare = |f: fn(f64, f64) -> bool| {
        if x.is_nan() || y.is_nan() {
            Value::Nothing
        } else {
            Value::Bool(f(x, y))
        }
    };
    match kind {
        BinKind::Add(Arith::Num) => finite(x + y),
        BinKind::Sub(Arith::Num) => finite(x - y),
        BinKind::Add(Arith::Vec) => vec_op(|p, q| p + q),
        BinKind::Sub(Arith::Vec) => vec_op(|p, q| p - q),
        BinKind::Add(Arith::Mat) => match (mat(a), mat(b)) {
            (Some(p), Some(q)) => mat_value(linalg::zip(p, q, |s, t| s + t)),
            _ => Value::Nothing,
        },
        BinKind::Sub(Arith::Mat) => match (mat(a), mat(b)) {
            (Some(p), Some(q)) => mat_value(linalg::zip(p, q, |s, t| s - t)),
            _ => Value::Nothing,
        },
        BinKind::MulNum => finite(x * y),
        BinKind::ScaleVec => scaled(b, x),
        BinKind::VecScale => scaled(a, y),
        BinKind::ScaleMat { mat_left } => {
            let (m, k) = if mat_left { (a, y) } else { (b, x) };
            match mat(m) {
                Some(m) if k.is_finite() => Value::Mat(Arc::new(linalg::scale(m, k))),
                _ => Value::Nothing,
            }
        }
        BinKind::MatVec => match (mat(a), vec2(b)) {
            (Some(m), Some(v)) => linalg::apply(m, v)
                .map(Value::Vec)
                .unwrap_or(Value::Nothing),
            _ => Value::Nothing,
        },
        BinKind::MatMat => match (mat(a), mat(b)) {
            (Some(p), Some(q)) => mat_value(linalg::mul(p, q)),
            _ => Value::Nothing,
        },
        BinKind::DivNum => finite(x / y),
        BinKind::DivVec => scaled(a, 1.0 / y),
        BinKind::DivMat => match mat(a) {
            Some(m) if y != 0.0 && y.is_finite() => Value::Mat(Arc::new(linalg::scale(m, 1.0 / y))),
            _ => Value::Nothing,
        },
        // Mathematical modulo: the result has the sign of the divisor.
        BinKind::Mod => Value::Num(if y == 0.0 {
            f64::NAN
        } else {
            x - y * libm::floor(x / y)
        }),
        BinKind::PowNum => finite(libm::pow(x, y)),
        BinKind::PowMat => match mat(a) {
            Some(m) => mat_value(linalg::pow(m, y)),
            None => Value::Nothing,
        },
        BinKind::Lt => compare(|p, q| p < q),
        BinKind::Le => compare(|p, q| p <= q),
        BinKind::Gt => compare(|p, q| p > q),
        BinKind::Ge => compare(|p, q| p >= q),
        BinKind::Eq | BinKind::Ne => {
            let equal = match (a, b) {
                (Value::Num(p), Value::Num(q)) if !p.is_nan() && !q.is_nan() => Some(p == q),
                (Value::Str(p), Value::Str(q)) => Some(p == q),
                (Value::Bool(p), Value::Bool(q)) => Some(p == q),
                _ => None,
            };
            match equal {
                Some(e) => Value::Bool(if kind == BinKind::Eq { e } else { !e }),
                None => Value::Nothing,
            }
        }
    }
}

fn reduce(reducer: Reducer, items: &[Value]) -> Value {
    let nums = || items.iter().map(Value::num);
    match reducer {
        Reducer::Count => Value::Num(items.len() as f64),
        Reducer::Sum(Arith::Num) => Value::Num(nums().sum()),
        Reducer::Mean(Arith::Num) => {
            if items.is_empty() {
                Value::Num(f64::NAN)
            } else {
                Value::Num(nums().sum::<f64>() / items.len() as f64)
            }
        }
        Reducer::Sum(_) | Reducer::Mean(_) => {
            let mut total = [0.0, 0.0];
            for item in items {
                match vec2(item) {
                    Some(v) => {
                        total[0] += v[0];
                        total[1] += v[1];
                    }
                    None => return Value::Nothing,
                }
            }
            if let Reducer::Mean(_) = reducer {
                if items.is_empty() {
                    return Value::Nothing;
                }
                let n = items.len() as f64;
                total = [total[0] / n, total[1] / n];
            }
            Value::Vec(total)
        }
        Reducer::Prod => Value::Num(nums().product()),
        Reducer::Min | Reducer::Max => {
            if items.is_empty() || nums().any(f64::is_nan) {
                return Value::Num(f64::NAN);
            }
            let pick: fn(f64, f64) -> f64 = if reducer == Reducer::Min {
                f64::min
            } else {
                f64::max
            };
            Value::Num(nums().reduce(pick).unwrap_or(f64::NAN))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module::Declarations;
    use crate::types::Type;

    struct Fixture {
        module: Module,
        values: Vec<Value>,
    }

    fn fixture(lets: &[(&str, &str)]) -> Fixture {
        let decls = Declarations {
            values: vec![
                ("x".into(), Type::Num),
                ("p".into(), Type::Vec),
                ("mode".into(), Type::Str),
            ],
            compute: vec![],
            lets: lets
                .iter()
                .map(|(k, f)| (k.to_string(), f.to_string()))
                .collect(),
        };
        Fixture {
            module: Module::new(&decls).unwrap(),
            values: vec![
                Value::Num(3.0),
                Value::Vec([1.0, 2.0]),
                Value::str("damped"),
            ],
        }
    }

    fn run(f: &mut Fixture, formula: &str) -> Value {
        run_with(f, formula, DEFAULT_FUEL).unwrap()
    }

    fn run_with(f: &mut Fixture, formula: &str, fuel: u64) -> Result<Value, Error> {
        let checked = f
            .module
            .compile(formula, &[], None)
            .unwrap_or_else(|e| panic!("{formula}: {e}"));
        let mut cache = Cache::new(&f.module);
        let env = Env {
            values: &f.values,
            compute: &[],
            palette: &no_palette,
        };
        Evaluation::new(&f.module, env, &mut cache, fuel).eval(&checked, &[])
    }

    #[test]
    fn arithmetic_on_every_type() {
        let mut f = fixture(&[]);
        assert_eq!(run(&mut f, "x * 2 + 1"), Value::Num(7.0));
        assert_eq!(run(&mut f, "-x ** 2"), Value::Num(-9.0));
        assert_eq!(run(&mut f, "2 ** -1"), Value::Num(0.5));
        assert_eq!(run(&mut f, "p + [1, 1]"), Value::Vec([2.0, 3.0]));
        assert_eq!(run(&mut f, "2 * p - p / 2"), Value::Vec([1.5, 3.0]));
        assert_eq!(run(&mut f, "-p"), Value::Vec([-1.0, -2.0]));
        assert_eq!(run(&mut f, "-1 % 3"), Value::Num(2.0));
        assert_eq!(run(&mut f, "7 % -3"), Value::Num(-2.0));
    }

    #[test]
    fn logic_short_circuits() {
        let mut f = fixture(&[]);
        assert_eq!(run(&mut f, "x > 1 and mode == 'damped'"), Value::Bool(true));
        assert_eq!(run(&mut f, "x < 1 and 1 / 0 > 0"), Value::Bool(false));
        assert_eq!(run(&mut f, "x > 1 or 1 / 0 > 0"), Value::Bool(true));
        assert_eq!(
            run(&mut f, "if x > 2 then 'big' else 'small'"),
            Value::str("big")
        );
        assert_eq!(run(&mut f, "not (x == 3)"), Value::Bool(false));
    }

    #[test]
    fn runtime_failures_are_no_value_not_errors() {
        let mut f = fixture(&[]);
        assert!(run(&mut f, "1 / (x - 3)").is_nothing());
        assert!(
            run(&mut f, "(-1) ** 0.5").is_nothing() || run(&mut f, "(-1) ** 0.5").num().is_nan()
        );
        assert!(run(&mut f, "[1, 2, 3][5]").is_nothing());
        assert!(run(&mut f, "[1, 2, 3][1.5]").is_nothing());
        assert!(run(&mut f, "[1, 2, 3][-1]").is_nothing());
        assert!(run(&mut f, "1 / 0 < 2").is_nothing());
        assert!(run(&mut f, "if 1 / 0 > 0 then 1 else 2").is_nothing());
        assert!(run(&mut f, "x % 0").is_nothing());
        assert!(run(&mut f, "[1 / 0, 2]").is_nothing());
    }

    #[test]
    fn lets_functions_and_comprehensions() {
        let mut f = fixture(&[
            ("k", "x + 1"),
            ("sq(t)", "t * t"),
            ("ys", "[sq(i) for i in [1, 2, 3]]"),
        ]);
        assert_eq!(run(&mut f, "k * 2"), Value::Num(8.0));
        assert_eq!(run(&mut f, "sq(k)"), Value::Num(16.0));
        assert_eq!(run(&mut f, "ys"), Value::nums([1.0, 4.0, 9.0]));
        assert_eq!(
            run(&mut f, "[i for i in ys if i > 2]"),
            Value::nums([4.0, 9.0])
        );
        assert_eq!(run(&mut f, "sum(i for i in ys)"), Value::Num(14.0));
        assert_eq!(run(&mut f, "mean(i for i in ys)"), Value::Num(14.0 / 3.0));
        assert_eq!(
            run(&mut f, "count(i for i in ys if i > 1)"),
            Value::Num(2.0)
        );
        assert_eq!(run(&mut f, "prod(i for i in ys)"), Value::Num(36.0));
        assert_eq!(run(&mut f, "max(i for i in ys)"), Value::Num(9.0));
        assert_eq!(
            run(&mut f, "sum([i, 1] for i in ys)"),
            Value::Vec([14.0, 3.0])
        );
        assert!(run(&mut f, "min(i for i in ys if i > 100)").is_nothing());
        assert_eq!(
            run(&mut f, "[[i, i] for i in [1, 2]][1].y"),
            Value::Num(2.0)
        );
    }

    #[test]
    fn pairs_become_lists_where_lists_are_wanted() {
        let mut f = fixture(&[]);
        assert_eq!(
            run(&mut f, "[i * 10 for i in [1, 2]]"),
            Value::nums([10.0, 20.0])
        );
        assert_eq!(run(&mut f, "max([4, 7])"), Value::Num(7.0));
        assert_eq!(run(&mut f, "[1, 2][1]"), Value::Num(2.0));
    }

    #[test]
    fn running_out_of_fuel_is_the_one_error() {
        let mut f = fixture(&[]);
        let err = run_with(
            &mut f,
            "sum(i * i for i in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10])",
            20,
        )
        .unwrap_err();
        assert_eq!(err.code, codes::E604);
        assert!(err.hint.is_some());
    }

    #[test]
    fn lets_are_computed_once_per_cache() {
        let f = fixture(&[("k", "x + 1"), ("j", "k * k + k")]);
        let checked = {
            let mut m = f.module.clone();
            m.compile("j + k", &[], None).unwrap()
        };
        let mut cache = Cache::new(&f.module);
        let env = Env {
            values: &f.values,
            compute: &[],
            palette: &no_palette,
        };
        let mut ev = Evaluation::new(&f.module, env, &mut cache, DEFAULT_FUEL);
        assert_eq!(ev.eval(&checked, &[]).unwrap(), Value::Num(24.0));
        assert_eq!(ev.eval(&checked, &[]).unwrap(), Value::Num(24.0));
        assert_eq!(cache.computed, 2);
    }

    #[test]
    fn bound_values_fill_the_first_slots() {
        let mut f = fixture(&[]);
        let checked = f
            .module
            .compile(
                "x * 10 + y",
                &[("x".into(), Type::Num), ("y".into(), Type::Num)],
                None,
            )
            .unwrap();
        let mut cache = Cache::new(&f.module);
        let env = Env {
            values: &f.values,
            compute: &[],
            palette: &no_palette,
        };
        let v = Evaluation::new(&f.module, env, &mut cache, DEFAULT_FUEL)
            .eval(&checked, &[Value::Num(2.0), Value::Num(5.0)])
            .unwrap();
        assert_eq!(v, Value::Num(25.0));
    }
}
