//! Lists (SPEC-2.0 §4.6.5): making them, reducing them, reshaping them.
//!
//! A list may hold at most 100 000 items (§19.3); every item made or visited
//! is charged to the evaluation budget, so a slider that sets `n` cannot
//! make a figure hang.

use std::sync::{Arc, OnceLock};

use super::{num, Builtin, Host, Overload, Pat};
use crate::error::{codes, Error};
use crate::types::{RecordType, Type};
use crate::value::{Fields, Value};

/// The longest list a formula may build (§19.3).
pub const MAX_LEN: usize = 100_000;

const SUM: &[Overload] = &[
    Overload::new(&[Pat::ListNum], Pat::Num),
    Overload::new(&[Pat::ListVec], Pat::Vec),
];
const LN_N: &[Overload] = &[Overload::new(&[Pat::ListNum], Pat::Num)];
const LN_LN: &[Overload] = &[Overload::new(&[Pat::ListNum], Pat::ListNum)];
const LT_N: &[Overload] = &[Overload::new(&[Pat::ListT], Pat::Num)];
const LT_T: &[Overload] = &[Overload::new(&[Pat::ListT], Pat::T)];
const LT_LT: &[Overload] = &[Overload::new(&[Pat::ListT], Pat::ListT)];
const RANGE: &[Overload] = &[
    Overload::new(&[Pat::Num, Pat::Num], Pat::ListNum),
    Overload::new(&[Pat::Num, Pat::Num, Pat::Num], Pat::ListNum),
];
const LINSPACE: &[Overload] = &[Overload::new(&[Pat::Num, Pat::Num, Pat::Num], Pat::ListNum)];
const ZIP: &[Overload] = &[Overload::new(&[Pat::ListNum, Pat::ListNum], Pat::ListVec)];
const LV_LN: &[Overload] = &[Overload::new(&[Pat::ListVec], Pat::ListNum)];
const INTERP: &[Overload] = &[Overload::new(
    &[Pat::Num, Pat::ListNum, Pat::ListNum],
    Pat::Num,
)];
const HISTOGRAM: &[Overload] = &[Overload::record(
    &[Pat::ListNum, Pat::Num, Pat::Num, Pat::Num],
    histogram_type,
)];

/// `histogram(xs, lo, hi, bins)`: `{edges, counts, density}`.
pub fn histogram_type() -> Arc<RecordType> {
    static T: OnceLock<Arc<RecordType>> = OnceLock::new();
    T.get_or_init(|| {
        let nums = Type::list(Type::Num);
        Arc::new(RecordType {
            name: "histogram".into(),
            fields: vec![
                ("edges".into(), nums.clone()),
                ("counts".into(), nums.clone()),
                ("density".into(), nums),
            ],
            methods: vec![],
        })
    })
    .clone()
}

fn items(v: &Value) -> &[Value] {
    v.items().unwrap_or(&[])
}

fn floats(v: &Value) -> Vec<f64> {
    items(v).iter().map(Value::num).collect()
}

/// Refuse a list longer than the limit, and charge for the rest.
fn budget(host: &mut dyn Host, len: f64) -> Result<Option<usize>, Error> {
    if !len.is_finite() || len < 0.0 {
        return Ok(None);
    }
    if len > MAX_LEN as f64 {
        return Err(Error::new(
            codes::E604,
            format!(
                "a list of {} items is over the limit of {MAX_LEN}",
                len as u64
            ),
        )
        .hint("lower the count, or widen the step"));
    }
    host.charge(len as u64)?;
    Ok(Some(len as usize))
}

fn range(host: &mut dyn Host, a: f64, b: f64, step: f64) -> Result<Value, Error> {
    if step == 0.0 || !step.is_finite() || !a.is_finite() || !b.is_finite() {
        return Ok(Value::Nothing);
    }
    // `(b - a) / step` can land a hair above a whole number (`0.3 / 0.1`);
    // the end is exclusive, so a near-whole count is not rounded up.
    let steps = (b - a) / step;
    let count = (steps - 1e-9 * steps.abs().max(1.0)).ceil().max(0.0);
    let Some(count) = budget(host, count)? else {
        return Ok(Value::Nothing);
    };
    // Computed from the start each time, so floating steps do not drift.
    Ok(Value::nums((0..count).map(|i| a + i as f64 * step)))
}

fn linspace(host: &mut dyn Host, a: f64, b: f64, n: f64) -> Result<Value, Error> {
    if n.fract() != 0.0 || n < 1.0 || !a.is_finite() || !b.is_finite() {
        return Ok(Value::Nothing);
    }
    let Some(n) = budget(host, n)? else {
        return Ok(Value::Nothing);
    };
    if n == 1 {
        return Ok(Value::nums([a]));
    }
    let step = (b - a) / (n - 1) as f64;
    // The last point is exactly `b`, not `a + (n-1)*step`.
    Ok(Value::nums((0..n).map(|i| {
        if i == n - 1 {
            b
        } else {
            a + i as f64 * step
        }
    })))
}

fn sum(host: &mut dyn Host, list: &Value, mean: bool) -> Result<Value, Error> {
    let items = items(list);
    host.charge(items.len() as u64)?;
    let n = items.len() as f64;
    if let Some(Value::Vec(_)) = items.first() {
        let mut total = [0.0, 0.0];
        for item in items {
            let Some(p) = item.vec() else {
                return Ok(Value::Vec([f64::NAN, f64::NAN]));
            };
            total = [total[0] + p[0], total[1] + p[1]];
        }
        if mean {
            if items.is_empty() {
                return Ok(Value::Nothing);
            }
            total = [total[0] / n, total[1] / n];
        }
        return Ok(Value::Vec(total));
    }
    let total: f64 = items.iter().map(Value::num).sum();
    if mean {
        return num(if items.is_empty() {
            f64::NAN
        } else {
            total / n
        });
    }
    num(total)
}

/// Piecewise-linear over a table, clamped to its end values outside it.
fn interp(x: f64, xs: &[f64], ys: &[f64]) -> f64 {
    if xs.is_empty()
        || xs.len() != ys.len()
        || x.is_nan()
        || xs
            .windows(2)
            .any(|w| w[0].partial_cmp(&w[1]).is_none_or(|o| o.is_gt()))
    {
        return f64::NAN;
    }
    if x <= xs[0] {
        return ys[0];
    }
    let last = xs.len() - 1;
    if x >= xs[last] {
        return ys[last];
    }
    let i = xs.partition_point(|&v| v <= x) - 1;
    let (x0, x1, y0, y1) = (xs[i], xs[i + 1], ys[i], ys[i + 1]);
    if x1 == x0 {
        return y1;
    }
    y0 + (x - x0) / (x1 - x0) * (y1 - y0)
}

fn histogram(host: &mut dyn Host, xs: &[f64], lo: f64, hi: f64, bins: f64) -> Result<Value, Error> {
    if bins.fract() != 0.0 || bins < 1.0 || lo.partial_cmp(&hi) != Some(std::cmp::Ordering::Less) {
        return Ok(Value::Nothing);
    }
    let Some(bins) = budget(host, bins)? else {
        return Ok(Value::Nothing);
    };
    host.charge(xs.len() as u64)?;
    let width = (hi - lo) / bins as f64;
    let mut counts = vec![0.0; bins];
    let mut inside = 0.0;
    for &x in xs {
        if !(lo..=hi).contains(&x) {
            continue;
        }
        // The top edge belongs to the last bin.
        let i = (((x - lo) / width) as usize).min(bins - 1);
        counts[i] += 1.0;
        inside += 1.0;
    }
    let edges = (0..=bins).map(|i| if i == bins { hi } else { lo + i as f64 * width });
    let density: Vec<f64> = counts
        .iter()
        .map(|c| {
            if inside > 0.0 {
                c / (inside * width)
            } else {
                0.0
            }
        })
        .collect();
    Ok(Value::Record(Arc::new(Fields(vec![
        ("edges", Value::nums(edges)),
        ("counts", Value::nums(counts)),
        ("density", Value::nums(density)),
    ]))))
}

pub(super) const BUILTINS: &[Builtin] = &[
    Builtin {
        name: "sum",
        overloads: SUM,
        takes_fn: false,
        imp: |h, _, a| sum(h, &a[0], false),
    },
    Builtin {
        name: "mean",
        overloads: SUM,
        takes_fn: false,
        imp: |h, _, a| sum(h, &a[0], true),
    },
    Builtin {
        name: "prod",
        overloads: LN_N,
        takes_fn: false,
        imp: |h, _, a| {
            let xs = floats(&a[0]);
            h.charge(xs.len() as u64)?;
            num(xs.iter().product())
        },
    },
    Builtin {
        name: "count",
        overloads: LT_N,
        takes_fn: false,
        imp: |_, _, a| num(items(&a[0]).len() as f64),
    },
    Builtin {
        name: "range",
        overloads: RANGE,
        takes_fn: false,
        imp: |h, _, a| {
            range(
                h,
                a[0].num(),
                a[1].num(),
                a.get(2).map(Value::num).unwrap_or(1.0),
            )
        },
    },
    Builtin {
        name: "linspace",
        overloads: LINSPACE,
        takes_fn: false,
        imp: |h, _, a| linspace(h, a[0].num(), a[1].num(), a[2].num()),
    },
    Builtin {
        name: "first",
        overloads: LT_T,
        takes_fn: false,
        imp: |_, _, a| Ok(items(&a[0]).first().cloned().unwrap_or(Value::Nothing)),
    },
    Builtin {
        name: "last",
        overloads: LT_T,
        takes_fn: false,
        imp: |_, _, a| Ok(items(&a[0]).last().cloned().unwrap_or(Value::Nothing)),
    },
    Builtin {
        name: "cumsum",
        overloads: LN_LN,
        takes_fn: false,
        imp: |h, _, a| {
            let xs = floats(&a[0]);
            h.charge(xs.len() as u64)?;
            let mut total = 0.0;
            Ok(Value::nums(xs.into_iter().map(|x| {
                total += x;
                total
            })))
        },
    },
    Builtin {
        name: "diffs",
        overloads: LN_LN,
        takes_fn: false,
        imp: |h, _, a| {
            let xs = floats(&a[0]);
            h.charge(xs.len() as u64)?;
            Ok(Value::nums(xs.windows(2).map(|w| w[1] - w[0])))
        },
    },
    Builtin {
        name: "zip",
        overloads: ZIP,
        takes_fn: false,
        imp: |h, _, a| {
            let (xs, ys) = (floats(&a[0]), floats(&a[1]));
            h.charge(xs.len() as u64)?;
            if xs.len() != ys.len() {
                return Ok(Value::Nothing);
            }
            Ok(Value::list(
                xs.into_iter()
                    .zip(ys)
                    .map(|(x, y)| Value::Vec([x, y]))
                    .collect(),
            ))
        },
    },
    Builtin {
        name: "xs_of",
        overloads: LV_LN,
        takes_fn: false,
        imp: |_, _, a| {
            Ok(Value::nums(
                items(&a[0])
                    .iter()
                    .map(|p| p.vec().map_or(f64::NAN, |v| v[0])),
            ))
        },
    },
    Builtin {
        name: "ys_of",
        overloads: LV_LN,
        takes_fn: false,
        imp: |_, _, a| {
            Ok(Value::nums(
                items(&a[0])
                    .iter()
                    .map(|p| p.vec().map_or(f64::NAN, |v| v[1])),
            ))
        },
    },
    Builtin {
        name: "sort",
        overloads: LN_LN,
        takes_fn: false,
        imp: |h, _, a| {
            let mut xs = floats(&a[0]);
            h.charge((xs.len() as f64 * (xs.len().max(2) as f64).log2()) as u64)?;
            xs.sort_by(f64::total_cmp);
            Ok(Value::nums(xs))
        },
    },
    Builtin {
        name: "reverse",
        overloads: LT_LT,
        takes_fn: false,
        imp: |_, _, a| Ok(Value::list(items(&a[0]).iter().rev().cloned().collect())),
    },
    Builtin {
        name: "interp",
        overloads: INTERP,
        takes_fn: false,
        imp: |h, _, a| {
            let (xs, ys) = (floats(&a[1]), floats(&a[2]));
            h.charge(xs.len() as u64)?;
            num(interp(a[0].num(), &xs, &ys))
        },
    },
    Builtin {
        name: "histogram",
        overloads: HISTOGRAM,
        takes_fn: false,
        imp: |h, _, a| histogram(h, &floats(&a[0]), a[1].num(), a[2].num(), a[3].num()),
    },
];

#[cfg(test)]
mod tests {
    use super::super::testing::{close, close_vec, eval_at, nothing};
    use crate::value::Value;

    fn list(formula: &str) -> Vec<f64> {
        eval_at(formula, 0.0)
            .items()
            .unwrap_or_else(|| panic!("{formula} is not a list"))
            .iter()
            .map(Value::num)
            .collect()
    }

    #[test]
    fn ranges_are_end_exclusive_and_do_not_drift() {
        assert_eq!(list("range(0, 4)"), vec![0.0, 1.0, 2.0, 3.0]);
        assert_eq!(list("range(1, 2, 0.25)"), vec![1.0, 1.25, 1.5, 1.75]);
        assert_eq!(list("range(3, 0, -1)"), vec![3.0, 2.0, 1.0]);
        assert_eq!(list("range(0, 0)"), Vec::<f64>::new());
        assert_eq!(list("range(0, 1, 0.1)").len(), 10);
        assert_eq!(list("range(0, 0.3, 0.1)").len(), 3);
        assert_eq!(list("range(0, 0.7, 0.1)").len(), 7);
        nothing("range(0, 1, 0)");
        let tenth = list("range(0, 1, 0.1)");
        assert_eq!(tenth[7], 7.0 * 0.1);
    }

    #[test]
    fn linspace_includes_both_ends() {
        assert_eq!(list("linspace(0, 1, 5)"), vec![0.0, 0.25, 0.5, 0.75, 1.0]);
        assert_eq!(list("linspace(2, 2, 1)"), vec![2.0]);
        assert_eq!(*list("linspace(0, 0.3, 4)").last().unwrap(), 0.3);
        nothing("linspace(0, 1, 2.5)");
        nothing("linspace(0, 1, 0)");
    }

    #[test]
    fn too_long_a_list_is_refused_by_the_budget() {
        let decls = crate::module::Declarations {
            values: vec![("x".into(), crate::types::Type::Num)],
            ..Default::default()
        };
        let mut module = crate::module::Module::new(&decls).unwrap();
        let checked = module
            .compile("count(range(0, 1000000))", &[], None)
            .unwrap();
        let mut cache = crate::eval::Cache::new(&module);
        let env = crate::eval::Env {
            values: &[Value::Num(0.0)],
            compute: &[],
            palette: &crate::eval::no_palette,
        };
        let err = crate::eval::Evaluation::new(&module, env, &mut cache, crate::eval::DEFAULT_FUEL)
            .eval(&checked, &[])
            .unwrap_err();
        assert_eq!(err.code.0, "E604");
        assert!(err.message.contains("100000"));
    }

    #[test]
    fn reductions_over_lists() {
        close("sum([1, 2, 3])", 6.0);
        close("sum([1, 2])", 3.0);
        close_vec("sum([[1, 2], [3, 4], [5, 6]])", [9.0, 12.0]);
        close("mean([2, 4, 6])", 4.0);
        close_vec("mean([[0, 0], [2, 4]])", [1.0, 2.0]);
        nothing("mean([])");
        close("prod([2, 3, 4])", 24.0);
        close("count([5, 6, 7])", 3.0);
        close("count([])", 0.0);
        close("min([3, 1, 2]) + max([3, 1, 2])", 4.0);
    }

    #[test]
    fn picking_and_reshaping() {
        close("first([7, 8, 9])", 7.0);
        close("last([7, 8, 9])", 9.0);
        nothing("first([])");
        assert_eq!(list("cumsum([1, 2, 3])"), vec![1.0, 3.0, 6.0]);
        assert_eq!(list("diffs([1, 4, 9])"), vec![3.0, 5.0]);
        assert_eq!(list("sort([3, 1, 2])"), vec![1.0, 2.0, 3.0]);
        assert_eq!(list("reverse([1, 2, 3])"), vec![3.0, 2.0, 1.0]);
        close_vec("zip([1, 2], [3, 4])[1]", [2.0, 4.0]);
        nothing("zip([1, 2, 3], [3, 4])");
        assert_eq!(list("xs_of(zip([1, 2], [3, 4]))"), vec![1.0, 2.0]);
        assert_eq!(list("ys_of(zip([1, 2], [3, 4]))"), vec![3.0, 4.0]);
    }

    #[test]
    fn interpolation_over_a_table() {
        close("interp(1.5, [0, 1, 2], [0, 10, 30])", 20.0);
        close("interp(-5, [0, 1, 2], [0, 10, 30])", 0.0);
        close("interp(5, [0, 1, 2], [0, 10, 30])", 30.0);
        nothing("interp(1, [0, 2, 1], [0, 1, 2])");
        nothing("interp(1, [0, 1], [0, 1, 2])");
    }

    #[test]
    fn a_histogram_counts_and_normalises() {
        assert_eq!(
            list("histogram([0, 0.5, 1, 1.5, 2, 9], 0, 2, 4).counts"),
            vec![1.0, 1.0, 1.0, 2.0]
        );
        assert_eq!(
            list("histogram([0, 2], 0, 2, 2).edges"),
            vec![0.0, 1.0, 2.0]
        );
        // The density integrates to one over the values inside the range.
        close(
            "sum(histogram([0.1, 0.2, 1.5], 0, 2, 4).density) * 0.5",
            1.0,
        );
        nothing("histogram([1], 2, 0, 4)");
    }
}
