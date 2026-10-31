//! Helpers the library's tests share: evaluate a formula over `x` and
//! compare with a tolerance.

use crate::eval::{Cache, Env, Evaluation, DEFAULT_FUEL};
use crate::module::{Declarations, Module};
use crate::types::Type;
use crate::value::Value;

/// A palette with the tokens the colour tests use.
pub fn test_palette(name: &str) -> Option<[f64; 4]> {
    match name {
        "hot" => Some([0.835, 0.369, 0.0, 1.0]),
        "cool" => Some([0.0, 0.447, 0.698, 1.0]),
        "bg" => Some([0.992, 0.992, 0.988, 1.0]),
        "c1" => Some([0.0, 0.447, 0.698, 1.0]),
        "c2" => Some([0.835, 0.369, 0.0, 1.0]),
        _ => None,
    }
}

/// Evaluate `formula` with a value `x` set to `x`, and optional lets.
pub fn eval_with(formula: &str, x: f64, lets: &[(&str, &str)]) -> Value {
    let decls = Declarations {
        values: vec![("x".into(), Type::Num)],
        compute: vec![],
        lets: lets
            .iter()
            .map(|(k, f)| (k.to_string(), f.to_string()))
            .collect(),
    };
    let mut module = Module::new(&decls).unwrap_or_else(|e| panic!("{e:?}"));
    let checked = module
        .compile(formula, &[], None)
        .unwrap_or_else(|e| panic!("{formula}: {e} {:?}", e.hint));
    let values = [Value::Num(x)];
    let mut cache = Cache::new(&module);
    let env = Env {
        values: &values,
        compute: &[],
        palette: &test_palette,
    };
    Evaluation::new(&module, env, &mut cache, DEFAULT_FUEL)
        .eval(&checked, &[])
        .unwrap_or_else(|e| panic!("{formula}: {e}"))
}

pub fn eval_at(formula: &str, x: f64) -> Value {
    eval_with(formula, x, &[])
}

pub fn close(formula: &str, want: f64) {
    let got = eval_at(formula, 0.0).num();
    assert!((got - want).abs() < 1e-9, "{formula} = {got}, want {want}");
}

pub fn close_vec(formula: &str, want: [f64; 2]) {
    let got = eval_at(formula, 0.0);
    let Value::Vec(v) = got else {
        panic!("{formula} = {got}, want a vector")
    };
    assert!(
        (v[0] - want[0]).abs() < 1e-9 && (v[1] - want[1]).abs() < 1e-9,
        "{formula} = {v:?}, want {want:?}"
    );
}

pub fn nothing(formula: &str) {
    let got = eval_at(formula, 0.0);
    assert!(got.is_nothing(), "{formula} = {got}, want no value");
}

/// The checker's error code for a formula over `x`.
pub fn refused(formula: &str) -> &'static str {
    let decls = Declarations {
        values: vec![("x".into(), Type::Num)],
        ..Default::default()
    };
    let mut module = Module::new(&decls).unwrap();
    module.compile(formula, &[], None).unwrap_err().code.0
}
