//! Vectors and plane geometry (SPEC-2.0 §4.6.4).
//!
//! Degenerate inputs have no answer rather than a surprising one: the unit
//! vector of zero is zero (and `check` warns), parallel lines do not
//! intersect, three points on a line have no circle through them.

use std::sync::{Arc, OnceLock};

use super::{num, Builtin, Overload, Pat};
use crate::error::Error;
use crate::types::{RecordType, Type};
use crate::value::{Fields, Value};

const VV_N: &[Overload] = &[Overload::new(&[Pat::Vec, Pat::Vec], Pat::Num)];
const V_N: &[Overload] = &[Overload::new(&[Pat::Vec], Pat::Num)];
const V_V: &[Overload] = &[Overload::new(&[Pat::Vec], Pat::Vec)];
const VV_V: &[Overload] = &[Overload::new(&[Pat::Vec, Pat::Vec], Pat::Vec)];
const NN_V: &[Overload] = &[Overload::new(&[Pat::Num, Pat::Num], Pat::Vec)];
const ROTATE: &[Overload] = &[
    Overload::new(&[Pat::Vec, Pat::Num], Pat::Vec),
    Overload::new(&[Pat::Vec, Pat::Num, Pat::Vec], Pat::Vec),
];
const INTERSECT: &[Overload] = &[Overload::new(
    &[Pat::Vec, Pat::Vec, Pat::Vec, Pat::Vec],
    Pat::Vec,
)];
const CIRCLE: &[Overload] = &[Overload::record(
    &[Pat::Vec, Pat::Vec, Pat::Vec],
    circle_type,
)];

/// `circle_through(a, b, c)`: `{center, radius}`.
pub fn circle_type() -> Arc<RecordType> {
    static T: OnceLock<Arc<RecordType>> = OnceLock::new();
    T.get_or_init(|| {
        Arc::new(RecordType {
            name: "circle".into(),
            fields: vec![("center".into(), Type::Vec), ("radius".into(), Type::Num)],
            methods: vec![],
        })
    })
    .clone()
}

type V = [f64; 2];

fn v(value: &Value) -> V {
    match value {
        Value::Vec(v) => *v,
        _ => [f64::NAN, f64::NAN],
    }
}

fn out(p: V) -> Result<Value, Error> {
    Ok(if p[0].is_finite() && p[1].is_finite() {
        Value::Vec(p)
    } else {
        Value::Vec([f64::NAN, f64::NAN])
    })
}

fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn cross(a: V, b: V) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn norm(a: V) -> f64 {
    libm::hypot(a[0], a[1])
}

fn rotate(p: V, angle: f64, about: V) -> V {
    let (s, c) = (libm::sin(angle), libm::cos(angle));
    let d = [p[0] - about[0], p[1] - about[1]];
    [
        about[0] + c * d[0] - s * d[1],
        about[1] + s * d[0] + c * d[1],
    ]
}

/// Where the line through `p1` along `d1` meets the one through `p2` along `d2`.
fn intersect(p1: V, d1: V, p2: V, d2: V) -> V {
    let denom = cross(d1, d2);
    let scale = norm(d1) * norm(d2);
    if denom.abs() <= 1e-12 * scale.max(1e-300) {
        return [f64::NAN, f64::NAN];
    }
    let t = cross([p2[0] - p1[0], p2[1] - p1[1]], d2) / denom;
    [p1[0] + t * d1[0], p1[1] + t * d1[1]]
}

fn circle_through(a: V, b: V, c: V) -> Value {
    let d = 2.0 * (a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1]));
    let scale = norm([b[0] - a[0], b[1] - a[1]]) * norm([c[0] - a[0], c[1] - a[1]]);
    if d.abs() <= 1e-12 * scale.max(1e-300) {
        return Value::Nothing;
    }
    let sq = |p: V| dot(p, p);
    let ux = (sq(a) * (b[1] - c[1]) + sq(b) * (c[1] - a[1]) + sq(c) * (a[1] - b[1])) / d;
    let uy = (sq(a) * (c[0] - b[0]) + sq(b) * (a[0] - c[0]) + sq(c) * (b[0] - a[0])) / d;
    let center = [ux, uy];
    Value::Record(Arc::new(Fields(vec![
        ("center", Value::Vec(center)),
        ("radius", Value::Num(norm([a[0] - ux, a[1] - uy]))),
    ])))
}

pub(super) const BUILTINS: &[Builtin] = &[
    Builtin {
        name: "vec",
        overloads: NN_V,
        takes_fn: false,
        imp: |_, _, a| out([a[0].num(), a[1].num()]),
    },
    Builtin {
        name: "norm",
        overloads: V_N,
        takes_fn: false,
        imp: |_, _, a| num(norm(v(&a[0]))),
    },
    Builtin {
        name: "dist",
        overloads: VV_N,
        takes_fn: false,
        imp: |_, _, a| {
            let (p, q) = (v(&a[0]), v(&a[1]));
            num(norm([p[0] - q[0], p[1] - q[1]]))
        },
    },
    Builtin {
        name: "dot",
        overloads: VV_N,
        takes_fn: false,
        imp: |_, _, a| num(dot(v(&a[0]), v(&a[1]))),
    },
    Builtin {
        name: "cross",
        overloads: VV_N,
        takes_fn: false,
        imp: |_, _, a| num(cross(v(&a[0]), v(&a[1]))),
    },
    Builtin {
        name: "unit",
        overloads: V_V,
        takes_fn: false,
        imp: |_, _, a| {
            let p = v(&a[0]);
            let n = norm(p);
            // The unit of zero stays zero (§4.6.4); `check` warns about it.
            if n == 0.0 {
                return out([0.0, 0.0]);
            }
            out([p[0] / n, p[1] / n])
        },
    },
    Builtin {
        name: "perp",
        overloads: V_V,
        takes_fn: false,
        imp: |_, _, a| {
            let p = v(&a[0]);
            out([-p[1], p[0]])
        },
    },
    Builtin {
        name: "rotate",
        overloads: ROTATE,
        takes_fn: false,
        imp: |_, _, a| {
            let about = a.get(2).map(v).unwrap_or([0.0, 0.0]);
            out(rotate(v(&a[0]), a[1].num(), about))
        },
    },
    Builtin {
        name: "angle",
        overloads: V_N,
        takes_fn: false,
        imp: |_, _, a| {
            let p = v(&a[0]);
            num(libm::atan2(p[1], p[0]))
        },
    },
    Builtin {
        name: "angle_between",
        overloads: VV_N,
        takes_fn: false,
        imp: |_, _, a| {
            let (p, q) = (v(&a[0]), v(&a[1]));
            if norm(p) == 0.0 || norm(q) == 0.0 {
                return num(f64::NAN);
            }
            // Signed, in (-pi, pi]: from p to q, counter-clockwise positive.
            num(super::basic::wrap(libm::atan2(cross(p, q), dot(p, q))))
        },
    },
    Builtin {
        name: "polar",
        overloads: NN_V,
        takes_fn: false,
        imp: |_, _, a| {
            let (r, t) = (a[0].num(), a[1].num());
            out([r * libm::cos(t), r * libm::sin(t)])
        },
    },
    Builtin {
        name: "mid",
        overloads: VV_V,
        takes_fn: false,
        imp: |_, _, a| {
            let (p, q) = (v(&a[0]), v(&a[1]));
            out([(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0])
        },
    },
    Builtin {
        name: "proj",
        overloads: VV_V,
        takes_fn: false,
        imp: |_, _, a| {
            let (p, onto) = (v(&a[0]), v(&a[1]));
            let k = dot(p, onto) / dot(onto, onto);
            out([k * onto[0], k * onto[1]])
        },
    },
    Builtin {
        name: "reflect",
        overloads: VV_V,
        takes_fn: false,
        imp: |_, _, a| {
            let (p, n) = (v(&a[0]), v(&a[1]));
            let k = 2.0 * dot(p, n) / dot(n, n);
            out([p[0] - k * n[0], p[1] - k * n[1]])
        },
    },
    Builtin {
        name: "intersect",
        overloads: INTERSECT,
        takes_fn: false,
        imp: |_, _, a| out(intersect(v(&a[0]), v(&a[1]), v(&a[2]), v(&a[3]))),
    },
    Builtin {
        name: "circle_through",
        overloads: CIRCLE,
        takes_fn: false,
        imp: |_, _, a| Ok(circle_through(v(&a[0]), v(&a[1]), v(&a[2]))),
    },
];

#[cfg(test)]
mod tests {
    use super::super::testing::{close, close_vec, nothing, refused};
    use std::f64::consts::{FRAC_PI_2, PI};

    #[test]
    fn construction_and_components() {
        close_vec("vec(3, 4)", [3.0, 4.0]);
        close("vec(3, 4).y", 4.0);
        close("norm([3, 4])", 5.0);
        close("dist([1, 1], [4, 5])", 5.0);
        nothing("vec(1 / 0, 2)");
    }

    #[test]
    fn products() {
        close("dot([1, 2], [3, 4])", 11.0);
        close("cross([1, 0], [0, 1])", 1.0);
        close("cross([0, 1], [1, 0])", -1.0);
    }

    #[test]
    fn directions() {
        close_vec("unit([3, 4])", [0.6, 0.8]);
        close_vec("unit([0, 0])", [0.0, 0.0]);
        close_vec("perp([1, 0])", [0.0, 1.0]);
        close_vec("rotate([1, 0], pi / 2)", [0.0, 1.0]);
        close_vec("rotate([2, 1], pi, [1, 1])", [0.0, 1.0]);
        close("angle([0, 2])", FRAC_PI_2);
        close("angle([-1, 0])", PI);
        close("angle_between([1, 0], [0, 1])", FRAC_PI_2);
        close("angle_between([0, 1], [1, 0])", -FRAC_PI_2);
        close("angle_between([1, 0], [-1, 0])", PI);
        nothing("angle_between([0, 0], [1, 0])");
        close_vec("polar(2, pi / 2)", [0.0, 2.0]);
    }

    #[test]
    fn constructions() {
        close_vec("mid([0, 0], [2, 4])", [1.0, 2.0]);
        close_vec("proj([2, 3], [1, 0])", [2.0, 0.0]);
        nothing("proj([2, 3], [0, 0])");
        close_vec("reflect([1, -1], [0, 1])", [1.0, 1.0]);
        close_vec("intersect([0, 0], [1, 1], [0, 2], [1, -1])", [1.0, 1.0]);
        nothing("intersect([0, 0], [1, 1], [0, 2], [2, 2])");
    }

    #[test]
    fn the_circle_through_three_points() {
        close_vec("circle_through([1, 0], [0, 1], [-1, 0]).center", [0.0, 0.0]);
        close("circle_through([1, 0], [0, 1], [-1, 0]).radius", 1.0);
        close("circle_through([2, 1], [1, 2], [0, 1]).radius", 1.0);
        nothing("circle_through([0, 0], [1, 1], [2, 2])");
        assert_eq!(
            refused("circle_through([0, 0], [1, 1], [2, 2]).centre"),
            "E207"
        );
    }
}
