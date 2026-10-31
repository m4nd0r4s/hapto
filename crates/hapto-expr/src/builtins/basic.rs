//! Arithmetic and trigonometry (SPEC-2.0 §4.6.2–§4.6.3). The constants
//! `pi`, `tau` and `e` are names, resolved by the checker (§4.6.1).
//!
//! Every transcendental function goes through `libm`, one implementation on
//! every target, so a native test and the browser compute the same digits.

use std::f64::consts::{PI, TAU};

use super::{num, nums, Builtin, Host, Overload, Pat};
use crate::colour;
use crate::error::Error;
use crate::ir::FnId;
use crate::value::Value;

const N_N: &[Overload] = &[Overload::new(&[Pat::Num], Pat::Num)];
const NN_N: &[Overload] = &[Overload::new(&[Pat::Num, Pat::Num], Pat::Num)];
const NNN_N: &[Overload] = &[Overload::new(&[Pat::Num, Pat::Num, Pat::Num], Pat::Num)];

/// `min(a, b, …)` over numbers, or `min(list)`.
const EXTREMUM: &[Overload] = &[
    Overload::variadic(&[Pat::Num, Pat::Num], Pat::Num, Pat::Num),
    Overload::new(&[Pat::ListNum], Pat::Num),
];

const ROUND: &[Overload] = &[
    Overload::new(&[Pat::Num], Pat::Num),
    Overload::new(&[Pat::Num, Pat::Num], Pat::Num),
];

const LOG: &[Overload] = &[Overload::new(&[Pat::Num, Pat::Num], Pat::Num)];

const LERP: &[Overload] = &[
    Overload::new(&[Pat::Num, Pat::Num, Pat::Num], Pat::Num),
    Overload::new(&[Pat::Vec, Pat::Vec, Pat::Num], Pat::Vec),
    Overload::new(&[Pat::Colour, Pat::Colour, Pat::Num], Pat::Colour),
];

const REMAP: &[Overload] = &[Overload::new(
    &[Pat::Num, Pat::Num, Pat::Num, Pat::Num, Pat::Num],
    Pat::Num,
)];

const NUM_OF_BOOL: &[Overload] = &[Overload::new(&[Pat::Bool], Pat::Num)];

fn one(a: &[Value], f: fn(f64) -> f64) -> Result<Value, Error> {
    num(f(a[0].num()))
}

fn two(a: &[Value], f: fn(f64, f64) -> f64) -> Result<Value, Error> {
    num(f(a[0].num(), a[1].num()))
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

fn sign(x: f64) -> f64 {
    if x.is_nan() {
        f64::NAN
    } else if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

/// Half away from zero, at `digits` decimal places.
fn round_to(x: f64, digits: f64) -> f64 {
    if digits.fract() != 0.0 || !(-15.0..=15.0).contains(&digits) {
        return f64::NAN;
    }
    let k = libm::pow(10.0, digits);
    libm::round(x * k) / k
}

/// An angle into (-pi, pi].
pub(crate) fn wrap(a: f64) -> f64 {
    let r = a - TAU * libm::floor(a / TAU);
    if r > PI {
        r - TAU
    } else {
        r
    }
}

fn lerp(args: &[Value]) -> Result<Value, Error> {
    let t = args[2].num();
    Ok(match (&args[0], &args[1]) {
        (Value::Num(a), Value::Num(b)) => return num(a + (b - a) * t),
        (Value::Vec(a), Value::Vec(b)) => {
            Value::Vec([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t])
        }
        (Value::Colour(a), Value::Colour(b)) if t.is_finite() => {
            Value::Colour(colour::mix(*a, *b, t))
        }
        _ => Value::Nothing,
    })
}

macro_rules! unary {
    ($name:literal, $f:expr) => {
        Builtin {
            name: $name,
            overloads: N_N,
            takes_fn: false,
            imp: |_, _, a| one(a, $f),
        }
    };
}

pub(super) const BUILTINS: &[Builtin] = &[
    unary!("abs", f64::abs),
    unary!("sign", sign),
    unary!("floor", libm::floor),
    unary!("ceil", libm::ceil),
    unary!("trunc", libm::trunc),
    Builtin {
        name: "round",
        overloads: ROUND,
        takes_fn: false,
        imp: |_, _, a| match a {
            [x] => num(libm::round(x.num())),
            [x, d] => num(round_to(x.num(), d.num())),
            _ => num(f64::NAN),
        },
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
    Builtin {
        name: "clamp",
        overloads: NNN_N,
        takes_fn: false,
        imp: |_, _, a| {
            let (x, lo, hi) = (a[0].num(), a[1].num(), a[2].num());
            if lo > hi {
                return num(f64::NAN);
            }
            num(x.max(lo).min(hi))
        },
    },
    Builtin {
        name: "lerp",
        overloads: LERP,
        takes_fn: false,
        imp: |_, _, a| lerp(a),
    },
    Builtin {
        name: "remap",
        overloads: REMAP,
        takes_fn: false,
        imp: |_, _, a| {
            let [x, a0, b0, c, d] = [0, 1, 2, 3, 4].map(|i| a[i].num());
            num(c + (x - a0) / (b0 - a0) * (d - c))
        },
    },
    unary!("sqrt", libm::sqrt),
    unary!("cbrt", libm::cbrt),
    Builtin {
        name: "hypot",
        overloads: NN_N,
        takes_fn: false,
        imp: |_, _, a| two(a, libm::hypot),
    },
    unary!("exp", libm::exp),
    unary!("ln", libm::log),
    unary!("log10", libm::log10),
    unary!("log2", libm::log2),
    Builtin {
        name: "log",
        overloads: LOG,
        takes_fn: false,
        imp: |_, _, a| two(a, |x, base| libm::log(x) / libm::log(base)),
    },
    Builtin {
        name: "num",
        overloads: NUM_OF_BOOL,
        takes_fn: false,
        imp: |_, _, a| {
            Ok(match a[0].bool() {
                Some(true) => Value::Num(1.0),
                Some(false) => Value::Num(0.0),
                None => Value::Num(f64::NAN),
            })
        },
    },
    // `int(x)` asserts a whole number. `hapto check` reports a formula whose
    // argument is not within 1e-9 of one; at runtime it rounds.
    unary!("int", libm::round),
    unary!("sin", libm::sin),
    unary!("cos", libm::cos),
    unary!("tan", libm::tan),
    unary!("asin", libm::asin),
    unary!("acos", libm::acos),
    unary!("atan", libm::atan),
    Builtin {
        name: "atan2",
        overloads: NN_N,
        takes_fn: false,
        imp: |_, _, a| two(a, libm::atan2),
    },
    unary!("sinh", libm::sinh),
    unary!("cosh", libm::cosh),
    unary!("tanh", libm::tanh),
    unary!("deg", |d| d * PI / 180.0),
    unary!("to_deg", |r| r * 180.0 / PI),
    unary!("wrap", wrap),
];

#[cfg(test)]
mod tests {
    use crate::eval::{no_palette, Cache, Env, Evaluation, DEFAULT_FUEL};
    use crate::module::{Declarations, Module};
    use crate::types::Type;
    use crate::value::Value;

    /// Evaluate a formula over one value `x`.
    pub(crate) fn eval_at(formula: &str, x: f64) -> Value {
        let decls = Declarations {
            values: vec![("x".into(), Type::Num)],
            ..Default::default()
        };
        let mut module = Module::new(&decls).unwrap();
        let checked = module
            .compile(formula, &[], None)
            .unwrap_or_else(|e| panic!("{formula}: {e} {:?}", e.hint));
        let values = [Value::Num(x)];
        let mut cache = Cache::new(&module);
        let env = Env {
            values: &values,
            compute: &[],
            palette: &no_palette,
        };
        Evaluation::new(&module, env, &mut cache, DEFAULT_FUEL)
            .eval(&checked, &[])
            .unwrap()
    }

    fn n(formula: &str) -> f64 {
        eval_at(formula, 0.0).num()
    }

    fn close(formula: &str, want: f64) {
        let got = n(formula);
        assert!((got - want).abs() < 1e-12, "{formula} = {got}, want {want}");
    }

    fn nothing(formula: &str) {
        assert!(
            eval_at(formula, 0.0).is_nothing(),
            "{formula} should be no value"
        );
    }

    #[test]
    fn constants() {
        close("pi", std::f64::consts::PI);
        close("tau", std::f64::consts::TAU);
        close("e", std::f64::consts::E);
    }

    #[test]
    fn rounding_family() {
        close("abs(-2.5)", 2.5);
        close("sign(-3)", -1.0);
        close("sign(0)", 0.0);
        close("floor(-1.5)", -2.0);
        close("ceil(-1.5)", -1.0);
        close("trunc(-1.7)", -1.0);
        close("round(2.5)", 3.0);
        close("round(-2.5)", -3.0);
        close("round(2.71828, 2)", 2.72);
        close("round(1234, -2)", 1200.0);
        nothing("round(1, 0.5)");
        close("int(2.9999999999)", 3.0);
    }

    #[test]
    fn extrema_clamp_lerp_remap() {
        close("min(3, 1, 2)", 1.0);
        close("max([3, 1, 9, 2])", 9.0);
        nothing("max(1, 1 / 0)");
        close("clamp(5, 0, 2)", 2.0);
        close("clamp(-5, 0, 2)", 0.0);
        nothing("clamp(1, 2, 0)");
        close("lerp(10, 20, 0.25)", 12.5);
        assert_eq!(
            eval_at("lerp([0, 0], [2, 4], 0.5)", 0.0),
            Value::Vec([1.0, 2.0])
        );
        let Value::Colour(c) = eval_at("lerp(#000000, #ffffff, 1)", 0.0) else {
            panic!()
        };
        assert!((c[0] - 1.0).abs() < 1e-6);
        close("remap(5, 0, 10, 100, 200)", 150.0);
        nothing("remap(5, 1, 1, 0, 1)");
    }

    #[test]
    fn powers_and_logs() {
        close("sqrt(16)", 4.0);
        nothing("sqrt(-1)");
        close("cbrt(-27)", -3.0);
        close("hypot(3, 4)", 5.0);
        close("exp(0)", 1.0);
        close("ln(e)", 1.0);
        close("log10(1000)", 3.0);
        close("log2(8)", 3.0);
        close("log(81, 3)", 4.0);
        nothing("ln(0)");
        nothing("ln(-1)");
        // Overflow is no value, like any other non-finite result.
        nothing("exp(800)");
    }

    #[test]
    fn flags_become_numbers_only_when_asked() {
        close("num(1 < 2)", 1.0);
        close("num(not true)", 0.0);
    }

    #[test]
    fn trigonometry_in_radians() {
        close("sin(pi / 2)", 1.0);
        close("cos(0)", 1.0);
        close("tan(pi / 4)", 1.0);
        close("asin(1)", std::f64::consts::FRAC_PI_2);
        close("acos(1)", 0.0);
        close("atan(1)", std::f64::consts::FRAC_PI_4);
        close("atan2(1, -1)", 3.0 * std::f64::consts::FRAC_PI_4);
        close("sinh(0) + cosh(0) + tanh(0)", 1.0);
        close("deg(90)", std::f64::consts::FRAC_PI_2);
        close("to_deg(pi)", 180.0);
        nothing("asin(2)");
    }

    #[test]
    fn wrap_lands_in_the_half_open_turn() {
        close("wrap(pi)", std::f64::consts::PI);
        close("wrap(-pi)", std::f64::consts::PI);
        close("wrap(3 * pi / 2)", -std::f64::consts::FRAC_PI_2);
        close("wrap(0)", 0.0);
        close("wrap(tau + 0.5)", 0.5);
    }
}
