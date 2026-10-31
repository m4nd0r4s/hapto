//! Matrices and complex numbers (SPEC-2.0 §4.6.7–§4.6.8).
//!
//! Matrices are 1×1 to 4×4 and row-major. Eigenproblems are 2×2 only: that
//! is what a figure of a linear map in the plane needs, and its complex case
//! (a rotation) is reported rather than hidden. A complex number is a `vec`.

use std::sync::{Arc, OnceLock};

use super::{num, Builtin, Overload, Pat};
use crate::error::Error;
use crate::linalg;
use crate::types::{RecordType, Type};
use crate::value::{Fields, Matrix, Value};

const MAT: &[Overload] = &[Overload::variadic(&[Pat::ListNum], Pat::ListNum, Pat::Mat)];
const M_M: &[Overload] = &[Overload::new(&[Pat::Mat], Pat::Mat)];
const M_N: &[Overload] = &[Overload::new(&[Pat::Mat], Pat::Num)];
const N_M: &[Overload] = &[Overload::new(&[Pat::Num], Pat::Mat)];
const MN_LN: &[Overload] = &[Overload::new(&[Pat::Mat, Pat::Num], Pat::ListNum)];
const EIG: &[Overload] = &[Overload::record(&[Pat::Mat], eig_type)];
const VV_V: &[Overload] = &[Overload::new(&[Pat::Vec, Pat::Vec], Pat::Vec)];
const V_V: &[Overload] = &[Overload::new(&[Pat::Vec], Pat::Vec)];
const V_N: &[Overload] = &[Overload::new(&[Pat::Vec], Pat::Num)];
const VN_V: &[Overload] = &[Overload::new(&[Pat::Vec, Pat::Num], Pat::Vec)];
const NN_V: &[Overload] = &[Overload::new(&[Pat::Num, Pat::Num], Pat::Vec)];

/// `eig(M)`: `{real, values, vectors, re, im}`.
pub fn eig_type() -> Arc<RecordType> {
    static T: OnceLock<Arc<RecordType>> = OnceLock::new();
    T.get_or_init(|| {
        Arc::new(RecordType {
            name: "eig".into(),
            fields: vec![
                ("real".into(), Type::Bool),
                ("values".into(), Type::list(Type::Num)),
                ("vectors".into(), Type::list(Type::Vec)),
                ("re".into(), Type::Num),
                ("im".into(), Type::Num),
            ],
            methods: vec![],
        })
    })
    .clone()
}

fn matrix(v: &Value) -> Option<&Matrix> {
    match v {
        Value::Mat(m) => Some(m),
        _ => None,
    }
}

fn out(m: Option<Matrix>) -> Result<Value, Error> {
    Ok(m.map(|m| Value::Mat(Arc::new(m))).unwrap_or(Value::Nothing))
}

fn c(v: &Value) -> [f64; 2] {
    match v {
        Value::Vec(z) => *z,
        _ => [f64::NAN, f64::NAN],
    }
}

fn complex(z: [f64; 2]) -> Result<Value, Error> {
    Ok(if z[0].is_finite() && z[1].is_finite() {
        Value::Vec(z)
    } else {
        Value::Vec([f64::NAN, f64::NAN])
    })
}

fn cmul(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] * b[0] - a[1] * b[1], a[0] * b[1] + a[1] * b[0]]
}

fn index(i: f64, len: usize) -> Option<usize> {
    let r = i.round();
    ((i - r).abs() < 1e-9 && r >= 0.0 && (r as usize) < len).then_some(r as usize)
}

/// Build a matrix from rows of equal length.
fn from_rows(rows: &[Value]) -> Option<Matrix> {
    let rows: Vec<Vec<f64>> = rows
        .iter()
        .map(|r| {
            r.items()
                .map(|items| items.iter().map(Value::num).collect())
        })
        .collect::<Option<_>>()?;
    let cols = rows.first()?.len();
    if rows.iter().any(|r| r.len() != cols) {
        return None;
    }
    linalg::new(rows.len(), cols, rows.concat())
}

/// Eigenvalues and eigenvectors of a 2×2 matrix.
fn eig(m: &Matrix) -> Value {
    if m.rows != 2 || m.cols != 2 {
        return Value::Nothing;
    }
    let (a, b, c, d) = (m.get(0, 0), m.get(0, 1), m.get(1, 0), m.get(1, 1));
    let half_trace = (a + d) / 2.0;
    let disc = half_trace * half_trace - (a * d - b * c);
    let scale = (a.abs() + b.abs() + c.abs() + d.abs()).max(1e-300);
    let record = |real: bool, values: Vec<f64>, vectors: Vec<[f64; 2]>, re: f64, im: f64| {
        Value::Record(Arc::new(Fields(vec![
            ("real", Value::Bool(real)),
            ("values", Value::nums(values)),
            (
                "vectors",
                Value::list(vectors.into_iter().map(Value::Vec).collect()),
            ),
            ("re", Value::Num(re)),
            ("im", Value::Num(im)),
        ])))
    };
    if disc < -1e-12 * scale * scale {
        return record(false, vec![], vec![], half_trace, (-disc).sqrt());
    }
    let root = disc.max(0.0).sqrt();
    let values = [half_trace + root, half_trace - root];
    let vector = |l: f64| -> [f64; 2] {
        // (A - lI)v = 0: take whichever row is not (nearly) zero.
        let v = if b.abs() > 1e-12 * scale {
            [b, l - a]
        } else if c.abs() > 1e-12 * scale {
            [l - d, c]
        } else if (a - l).abs() <= (d - l).abs() {
            [1.0, 0.0]
        } else {
            [0.0, 1.0]
        };
        let n = libm::hypot(v[0], v[1]);
        [v[0] / n, v[1] / n]
    };
    let mut vectors = vec![vector(values[0]), vector(values[1])];
    // A repeated eigenvalue of a diagonal matrix has the whole plane: give the axes.
    if root == 0.0 && b.abs() <= 1e-12 * scale && c.abs() <= 1e-12 * scale {
        vectors = vec![[1.0, 0.0], [0.0, 1.0]];
    }
    record(true, values.to_vec(), vectors, half_trace, 0.0)
}

pub(super) const BUILTINS: &[Builtin] = &[
    Builtin {
        name: "mat",
        overloads: MAT,
        takes_fn: false,
        imp: |_, _, a| out(from_rows(a)),
    },
    Builtin {
        name: "transpose",
        overloads: M_M,
        takes_fn: false,
        imp: |_, _, a| out(matrix(&a[0]).map(linalg::transpose)),
    },
    Builtin {
        name: "det",
        overloads: M_N,
        takes_fn: false,
        imp: |_, _, a| num(matrix(&a[0]).and_then(linalg::det).unwrap_or(f64::NAN)),
    },
    Builtin {
        name: "inverse",
        overloads: M_M,
        takes_fn: false,
        imp: |_, _, a| out(matrix(&a[0]).and_then(linalg::inverse)),
    },
    Builtin {
        name: "trace",
        overloads: M_N,
        takes_fn: false,
        imp: |_, _, a| {
            num(match matrix(&a[0]) {
                Some(m) if m.rows == m.cols => (0..m.rows).map(|i| m.get(i, i)).sum(),
                _ => f64::NAN,
            })
        },
    },
    Builtin {
        name: "identity",
        overloads: N_M,
        takes_fn: false,
        imp: |_, _, a| {
            let n = a[0].num();
            out(if n.fract() == 0.0 && (1.0..=4.0).contains(&n) {
                linalg::identity(n as usize)
            } else {
                None
            })
        },
    },
    Builtin {
        name: "rows",
        overloads: M_N,
        takes_fn: false,
        imp: |_, _, a| num(matrix(&a[0]).map_or(f64::NAN, |m| m.rows as f64)),
    },
    Builtin {
        name: "cols",
        overloads: M_N,
        takes_fn: false,
        imp: |_, _, a| num(matrix(&a[0]).map_or(f64::NAN, |m| m.cols as f64)),
    },
    Builtin {
        name: "row",
        overloads: MN_LN,
        takes_fn: false,
        imp: |_, _, a| {
            Ok(match matrix(&a[0]) {
                Some(m) => match index(a[1].num(), m.rows) {
                    Some(i) => Value::nums(m.row(i).iter().copied()),
                    None => Value::Nothing,
                },
                None => Value::Nothing,
            })
        },
    },
    Builtin {
        name: "col",
        overloads: MN_LN,
        takes_fn: false,
        imp: |_, _, a| {
            Ok(match matrix(&a[0]) {
                Some(m) => match index(a[1].num(), m.cols) {
                    Some(j) => Value::nums((0..m.rows).map(|i| m.get(i, j))),
                    None => Value::Nothing,
                },
                None => Value::Nothing,
            })
        },
    },
    Builtin {
        name: "eig",
        overloads: EIG,
        takes_fn: false,
        imp: |_, _, a| Ok(matrix(&a[0]).map(eig).unwrap_or(Value::Nothing)),
    },
    Builtin {
        name: "cmul",
        overloads: VV_V,
        takes_fn: false,
        imp: |_, _, a| complex(cmul(c(&a[0]), c(&a[1]))),
    },
    Builtin {
        name: "cdiv",
        overloads: VV_V,
        takes_fn: false,
        imp: |_, _, a| {
            let (p, q) = (c(&a[0]), c(&a[1]));
            let d = q[0] * q[0] + q[1] * q[1];
            complex([
                (p[0] * q[0] + p[1] * q[1]) / d,
                (p[1] * q[0] - p[0] * q[1]) / d,
            ])
        },
    },
    Builtin {
        name: "conj",
        overloads: V_V,
        takes_fn: false,
        imp: |_, _, a| {
            let z = c(&a[0]);
            complex([z[0], -z[1]])
        },
    },
    Builtin {
        name: "cabs",
        overloads: V_N,
        takes_fn: false,
        imp: |_, _, a| {
            let z = c(&a[0]);
            num(libm::hypot(z[0], z[1]))
        },
    },
    Builtin {
        name: "carg",
        overloads: V_N,
        takes_fn: false,
        imp: |_, _, a| {
            let z = c(&a[0]);
            num(libm::atan2(z[1], z[0]))
        },
    },
    Builtin {
        name: "cexp",
        overloads: V_V,
        takes_fn: false,
        imp: |_, _, a| {
            let z = c(&a[0]);
            let r = libm::exp(z[0]);
            complex([r * libm::cos(z[1]), r * libm::sin(z[1])])
        },
    },
    Builtin {
        name: "cpow",
        overloads: VN_V,
        takes_fn: false,
        imp: |h, _, a| {
            let (z, n) = (c(&a[0]), a[1].num());
            if n.fract() != 0.0 || n.abs() > 1e6 {
                return complex([f64::NAN, f64::NAN]);
            }
            h.charge(n.abs().log2().max(1.0) as u64)?;
            // Square-and-multiply on the integer power; a negative power inverts.
            let (mut base, mut e, mut acc) = (z, n.abs() as u64, [1.0, 0.0]);
            while e > 0 {
                if e & 1 == 1 {
                    acc = cmul(acc, base);
                }
                base = cmul(base, base);
                e >>= 1;
            }
            if n < 0.0 {
                let d = acc[0] * acc[0] + acc[1] * acc[1];
                acc = [acc[0] / d, -acc[1] / d];
            }
            complex(acc)
        },
    },
    Builtin {
        name: "cpolar",
        overloads: NN_V,
        takes_fn: false,
        imp: |_, _, a| {
            let (r, t) = (a[0].num(), a[1].num());
            complex([r * libm::cos(t), r * libm::sin(t)])
        },
    },
];

#[cfg(test)]
mod tests {
    use super::super::testing::{close, close_vec, eval_at, nothing, refused};
    use crate::value::Value;

    fn list(formula: &str) -> Vec<f64> {
        eval_at(formula, 0.0)
            .items()
            .unwrap()
            .iter()
            .map(Value::num)
            .collect()
    }

    #[test]
    fn building_and_reading_matrices() {
        close("mat([1, 2], [3, 4])[1][0]", 3.0);
        close("rows(mat([1, 2, 3], [4, 5, 6]))", 2.0);
        close("cols(mat([1, 2, 3], [4, 5, 6]))", 3.0);
        assert_eq!(list("row(mat([1, 2], [3, 4]), 1)"), vec![3.0, 4.0]);
        assert_eq!(list("col(mat([1, 2], [3, 4]), 1)"), vec![2.0, 4.0]);
        nothing("row(mat([1, 2], [3, 4]), 2)");
        nothing("mat([1, 2], [3])");
        nothing("mat([1, 2, 3, 4, 5])");
        close("trace(identity(3))", 3.0);
        nothing("identity(5)");
    }

    #[test]
    fn matrix_algebra() {
        close("det(mat([4, 7], [2, 6]))", 10.0);
        close("det(mat([1, 2], [2, 4]))", 0.0);
        close("inverse(mat([4, 7], [2, 6]))[0][0]", 0.6);
        nothing("inverse(mat([1, 2], [2, 4]))");
        close("transpose(mat([1, 2], [3, 4]))[0][1]", 3.0);
        close_vec("mat([0, -1], [1, 0]) * [1, 0]", [0.0, 1.0]);
        close("(mat([1, 1], [0, 1]) ** 3)[0][1]", 3.0);
        close("(mat([2, 0], [0, 4]) ** -1)[1][1]", 0.25);
        close("(2 * mat([1, 0], [0, 1]) - identity(2))[0][0]", 1.0);
        nothing("(mat([1, 2]) + mat([1], [2]))[0][0]");
    }

    #[test]
    fn eigenproblems_of_2x2() {
        close("eig(mat([2, 0], [0, 3])).values[0]", 3.0);
        close("eig(mat([2, 0], [0, 3])).values[1]", 2.0);
        // A shear: one repeated eigenvalue, one direction.
        close("eig(mat([1, 1], [0, 1])).values[0]", 1.0);
        close_vec("eig(mat([1, 1], [0, 1])).vectors[0]", [1.0, 0.0]);
        // Each eigenvector satisfies A v = lambda v.
        let check = "mat([2, 1], [1, 3]) * eig(mat([2, 1], [1, 3])).vectors[0] - eig(mat([2, 1], [1, 3])).values[0] * eig(mat([2, 1], [1, 3])).vectors[0]";
        let Value::Vec(r) = eval_at(check, 0.0) else {
            panic!()
        };
        assert!(r[0].abs() < 1e-12 && r[1].abs() < 1e-12, "{r:?}");
        // A rotation has complex eigenvalues: reported, not hidden.
        assert_eq!(
            eval_at("eig(mat([0, -1], [1, 0])).real", 0.0),
            Value::Bool(false)
        );
        close("eig(mat([0, -1], [1, 0])).im", 1.0);
        close("count(eig(mat([0, -1], [1, 0])).values)", 0.0);
        nothing("eig(mat([1, 2, 3], [4, 5, 6], [7, 8, 9]))");
    }

    #[test]
    fn complex_numbers_are_vectors() {
        close_vec("cmul([0, 1], [0, 1])", [-1.0, 0.0]);
        close_vec("cdiv([1, 0], [0, 1])", [0.0, -1.0]);
        close_vec("conj([1, 2])", [1.0, -2.0]);
        close("cabs([3, 4])", 5.0);
        close("carg([0, 1])", std::f64::consts::FRAC_PI_2);
        close_vec("cexp([0, pi])", [-1.0, 0.0]);
        close_vec("cpow([0, 1], 4)", [1.0, 0.0]);
        close_vec("cpow([0, 2], -1)", [0.0, -0.5]);
        close_vec("cpolar(2, pi / 2)", [0.0, 2.0]);
        nothing("cpow([1, 1], 0.5)");
        nothing("cdiv([1, 0], [0, 0])");
        assert_eq!(refused("[1, 2] * [3, 4]"), "E204");
    }
}
