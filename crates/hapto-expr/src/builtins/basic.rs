//! Arithmetic, constants and trigonometry (SPEC-2.0 §4.6.1–§4.6.3).

use super::{num, nums, Builtin, Host, Overload, Pat};
use crate::error::Error;
use crate::ir::FnId;
use crate::value::Value;

const NUM_NUM: &[Overload] = &[Overload::new(&[Pat::Num], Pat::Num)];

/// `min(a, b, …)` over numbers, or `min(list)`.
const EXTREMUM: &[Overload] = &[
    Overload::variadic(&[Pat::Num, Pat::Num], Pat::Num, Pat::Num),
    Overload::new(&[Pat::ListNum], Pat::Num),
];

fn unary(args: &[Value], f: fn(f64) -> f64) -> Result<Value, Error> {
    num(f(args[0].num()))
}

fn extremum(
    host: &mut dyn Host,
    args: &[Value],
    pick: fn(f64, f64) -> f64,
) -> Result<Value, Error> {
    let values: Vec<f64> = match args {
        [Value::List(items)] => items.iter().map(Value::num).collect(),
        _ => nums(args),
    };
    host.charge(values.len() as u64)?;
    if values.is_empty() || values.iter().any(|v| v.is_nan()) {
        return num(f64::NAN);
    }
    num(values.into_iter().reduce(pick).unwrap_or(f64::NAN))
}

pub(super) const BUILTINS: &[Builtin] = &[
    Builtin {
        name: "sqrt",
        overloads: NUM_NUM,
        takes_fn: false,
        imp: |_, _, a| unary(a, libm::sqrt),
    },
    Builtin {
        name: "abs",
        overloads: NUM_NUM,
        takes_fn: false,
        imp: |_, _, a| unary(a, f64::abs),
    },
    Builtin {
        name: "min",
        overloads: EXTREMUM,
        takes_fn: false,
        imp: |h, _: Option<FnId>, a| extremum(h, a, f64::min),
    },
    Builtin {
        name: "max",
        overloads: EXTREMUM,
        takes_fn: false,
        imp: |h, _: Option<FnId>, a| extremum(h, a, f64::max),
    },
];
