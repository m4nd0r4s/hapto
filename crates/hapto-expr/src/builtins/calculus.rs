//! Calculus over user functions (SPEC-2.0 §4.6.6).
//!
//! Each builtin takes the name of a function declared in `let` as `"f(x)"`
//! and samples it through the host, so every evaluation is charged to the
//! budget: an integral of something expensive cannot hang a page. The
//! methods are the standard ones, with their limits stated where a reader of
//! the result might not expect them.

use super::{num, Builtin, Host, Overload, Pat};
use crate::error::Error;
use crate::ir::FnId;
use crate::value::Value;

const AT: &[Overload] = &[Overload::new(&[Pat::Num], Pat::Num)];
const OVER: &[Overload] = &[Overload::new(&[Pat::Num, Pat::Num], Pat::Num)];
const ROOTS: &[Overload] = &[Overload::new(&[Pat::Num, Pat::Num], Pat::ListNum)];

/// Subintervals scanned for sign changes and extrema.
const SCAN: usize = 512;

type F<'a> = dyn FnMut(f64) -> Result<f64, Error> + 'a;

fn sampler<'a>(
    host: &'a mut dyn Host,
    f: Option<FnId>,
) -> impl FnMut(f64) -> Result<f64, Error> + 'a {
    move |x| match f {
        Some(f) => host.call_fn(f, x),
        None => Ok(f64::NAN),
    }
}

fn derivative(f: &mut F, x: f64) -> Result<f64, Error> {
    let h = 1e-5 * x.abs().max(1.0);
    Ok((f(x + h)? - f(x - h)?) / (2.0 * h))
}

fn derivative2(f: &mut F, x: f64) -> Result<f64, Error> {
    // A second difference loses twice the digits: a wider step balances
    // truncation against rounding.
    let h = 1e-4 * x.abs().max(1.0);
    Ok((f(x + h)? - 2.0 * f(x)? + f(x - h)?) / (h * h))
}

/// Adaptive Simpson, relative tolerance 1e-9, depth at most 30.
fn integral(f: &mut F, a: f64, b: f64) -> Result<f64, Error> {
    if !a.is_finite() || !b.is_finite() {
        return Ok(f64::NAN);
    }
    if a == b {
        return Ok(0.0);
    }
    if a > b {
        return Ok(-integral(f, b, a)?);
    }
    let (fa, fb) = (f(a)?, f(b)?);
    let m = (a + b) / 2.0;
    let fm = f(m)?;
    let whole = (b - a) / 6.0 * (fa + 4.0 * fm + fb);
    let tolerance = 1e-9 * whole.abs().max(1e-9);
    simpson(f, a, b, fa, fm, fb, whole, tolerance, 30)
}

#[allow(clippy::too_many_arguments)]
fn simpson(
    f: &mut F,
    a: f64,
    b: f64,
    fa: f64,
    fm: f64,
    fb: f64,
    whole: f64,
    tol: f64,
    depth: u32,
) -> Result<f64, Error> {
    let m = (a + b) / 2.0;
    let (lm, rm) = ((a + m) / 2.0, (m + b) / 2.0);
    let (flm, frm) = (f(lm)?, f(rm)?);
    let left = (m - a) / 6.0 * (fa + 4.0 * flm + fm);
    let right = (b - m) / 6.0 * (fm + 4.0 * frm + fb);
    let delta = left + right - whole;
    if !delta.is_finite() {
        return Ok(f64::NAN);
    }
    if depth == 0 || delta.abs() <= 15.0 * tol {
        return Ok(left + right + delta / 15.0);
    }
    Ok(simpson(f, a, m, fa, flm, fm, left, tol / 2.0, depth - 1)?
        + simpson(f, m, b, fm, frm, fb, right, tol / 2.0, depth - 1)?)
}

/// Brent's method on `[a, b]` where `f(a)` and `f(b)` differ in sign.
fn brent(f: &mut F, mut a: f64, mut b: f64, mut fa: f64, mut fb: f64) -> Result<f64, Error> {
    if fa.abs() < fb.abs() {
        std::mem::swap(&mut a, &mut b);
        std::mem::swap(&mut fa, &mut fb);
    }
    let (mut c, mut fc) = (a, fa);
    let mut d = b - a;
    let mut bisected = true;
    for _ in 0..200 {
        if fb == 0.0 || (b - a).abs() < 1e-12 * b.abs().max(1.0) {
            return Ok(b);
        }
        let mut s = if fa != fc && fb != fc {
            // Inverse quadratic interpolation.
            a * fb * fc / ((fa - fb) * (fa - fc))
                + b * fa * fc / ((fb - fa) * (fb - fc))
                + c * fa * fb / ((fc - fa) * (fc - fb))
        } else {
            b - fb * (b - a) / (fb - fa)
        };
        let lo = ((3.0 * a + b) / 4.0).min(b);
        let hi = ((3.0 * a + b) / 4.0).max(b);
        let use_bisection = !(lo..=hi).contains(&s)
            || (bisected && (s - b).abs() >= (b - c).abs() / 2.0)
            || (!bisected && (s - b).abs() >= (c - d).abs() / 2.0);
        if use_bisection {
            s = (a + b) / 2.0;
        }
        bisected = use_bisection;
        let fs = f(s)?;
        d = c;
        c = b;
        fc = fb;
        if fa * fs < 0.0 {
            b = s;
            fb = fs;
        } else {
            a = s;
            fa = fs;
        }
        if fa.abs() < fb.abs() {
            std::mem::swap(&mut a, &mut b);
            std::mem::swap(&mut fa, &mut fb);
        }
    }
    Ok(b)
}

/// Every sign change of `f` in `[a, b]`, refined. Roots where `f` touches
/// zero without crossing it are found only if a sample lands on them.
fn roots(f: &mut F, a: f64, b: f64) -> Result<Value, Error> {
    if !(a.is_finite() && b.is_finite()) || a >= b {
        return Ok(Value::Nothing);
    }
    let xs: Vec<f64> = (0..=SCAN)
        .map(|i| a + (b - a) * i as f64 / SCAN as f64)
        .collect();
    let mut ys = Vec::with_capacity(xs.len());
    for &x in &xs {
        ys.push(f(x)?);
    }
    let mut found: Vec<f64> = Vec::new();
    for i in 0..SCAN {
        let (y0, y1) = (ys[i], ys[i + 1]);
        if y0 == 0.0 {
            found.push(xs[i]);
        } else if y0.is_finite() && y1.is_finite() && y0 * y1 < 0.0 {
            found.push(brent(f, xs[i], xs[i + 1], y0, y1)?);
        }
    }
    if ys[SCAN] == 0.0 {
        found.push(xs[SCAN]);
    }
    found.sort_by(f64::total_cmp);
    found.dedup_by(|p, q| (*p - *q).abs() < 1e-9 * q.abs().max(1.0));
    Ok(Value::nums(found))
}

/// Golden-section search for the extremum near the best sample.
fn extremum(f: &mut F, a: f64, b: f64, maximise: bool) -> Result<f64, Error> {
    if !(a.is_finite() && b.is_finite()) || a >= b {
        return Ok(f64::NAN);
    }
    let sign = if maximise { -1.0 } else { 1.0 };
    let step = (b - a) / SCAN as f64;
    let (mut best, mut best_y) = (f64::NAN, f64::INFINITY);
    for i in 0..=SCAN {
        let x = a + step * i as f64;
        let y = sign * f(x)?;
        if y < best_y {
            best = x;
            best_y = y;
        }
    }
    if best.is_nan() {
        return Ok(f64::NAN);
    }
    let (mut lo, mut hi) = ((best - step).max(a), (best + step).min(b));
    let ratio = (5f64.sqrt() - 1.0) / 2.0;
    let mut x1 = hi - ratio * (hi - lo);
    let mut x2 = lo + ratio * (hi - lo);
    let (mut y1, mut y2) = (sign * f(x1)?, sign * f(x2)?);
    while hi - lo > 1e-12 * best.abs().max(1.0) {
        if y1 < y2 {
            hi = x2;
            x2 = x1;
            y2 = y1;
            x1 = hi - ratio * (hi - lo);
            y1 = sign * f(x1)?;
        } else {
            lo = x1;
            x1 = x2;
            y1 = y2;
            x2 = lo + ratio * (hi - lo);
            y2 = sign * f(x2)?;
        }
    }
    let x = (lo + hi) / 2.0;
    // Golden section never leaves the bracket, but the ends themselves may
    // be the extremum: keep whichever is better.
    Ok(if sign * f(x)? <= best_y { x } else { best })
}

pub(super) const BUILTINS: &[Builtin] = &[
    Builtin {
        name: "derivative",
        overloads: AT,
        takes_fn: true,
        imp: |h, f, a| num(derivative(&mut sampler(h, f), a[0].num())?),
    },
    Builtin {
        name: "derivative2",
        overloads: AT,
        takes_fn: true,
        imp: |h, f, a| num(derivative2(&mut sampler(h, f), a[0].num())?),
    },
    Builtin {
        name: "integral",
        overloads: OVER,
        takes_fn: true,
        imp: |h, f, a| num(integral(&mut sampler(h, f), a[0].num(), a[1].num())?),
    },
    Builtin {
        name: "roots",
        overloads: ROOTS,
        takes_fn: true,
        imp: |h, f, a| roots(&mut sampler(h, f), a[0].num(), a[1].num()),
    },
    Builtin {
        name: "argmin",
        overloads: OVER,
        takes_fn: true,
        imp: |h, f, a| num(extremum(&mut sampler(h, f), a[0].num(), a[1].num(), false)?),
    },
    Builtin {
        name: "argmax",
        overloads: OVER,
        takes_fn: true,
        imp: |h, f, a| num(extremum(&mut sampler(h, f), a[0].num(), a[1].num(), true)?),
    },
    Builtin {
        name: "newton_step",
        overloads: AT,
        takes_fn: true,
        imp: |h, f, a| {
            let x = a[0].num();
            let mut s = sampler(h, f);
            let fx = s(x)?;
            let d = derivative(&mut s, x)?;
            num(x - fx / d)
        },
    },
];

#[cfg(test)]
mod tests {
    use super::super::testing::{eval_with, refused};
    use crate::value::Value;

    fn with(formula: &str, f: &str) -> Value {
        eval_with(formula, 0.0, &[("f(t)", f)])
    }

    fn close(formula: &str, f: &str, want: f64, tol: f64) {
        let got = with(formula, f).num();
        assert!(
            (got - want).abs() <= tol,
            "{formula} with f = {f}: {got}, want {want}"
        );
    }

    #[test]
    fn derivatives_against_closed_forms() {
        close("derivative(f, 3)", "t**2", 6.0, 1e-8);
        close("derivative(f, 0)", "sin(t)", 1.0, 1e-9);
        close("derivative(f, 1000)", "t**2", 2000.0, 1e-4);
        close("derivative2(f, 2)", "t**3", 12.0, 1e-5);
        close("derivative2(f, 0)", "cos(t)", -1.0, 1e-6);
    }

    #[test]
    fn integrals_against_closed_forms() {
        close("integral(f, 0, pi)", "sin(t)", 2.0, 1e-9);
        close("integral(f, 0, 1)", "t**2", 1.0 / 3.0, 1e-12);
        close("integral(f, 1, 0)", "t**2", -1.0 / 3.0, 1e-12);
        close(
            "integral(f, 0, 2)",
            "exp(-t*t)",
            0.882_081_390_762_421_4,
            1e-9,
        );
        close("integral(f, 2, 2)", "t", 0.0, 0.0);
        assert!(with("integral(f, -1, 1)", "1 / t").is_nothing());
    }

    #[test]
    fn roots_are_every_sign_change_refined() {
        let r = with("roots(f, -2, 2)", "t**2 - 2");
        let r: Vec<f64> = r.items().unwrap().iter().map(Value::num).collect();
        assert_eq!(r.len(), 2);
        assert!((r[0] + 2f64.sqrt()).abs() < 1e-12);
        assert!((r[1] - 2f64.sqrt()).abs() < 1e-12);
        // 0, pi, 2pi and 3pi: a root on the sample at the start counts.
        let r = with("roots(f, 0, 10)", "sin(t)");
        assert_eq!(r.items().unwrap().len(), 4, "{r}");
        let r = with("roots(f, -1, 1)", "t**2 + 1");
        assert_eq!(r.items().unwrap().len(), 0);
        // A tangential root without a sign change is only found if sampled.
        let r = with("roots(f, -1, 1)", "t**2");
        assert_eq!(r.items().unwrap().len(), 1);
    }

    #[test]
    fn extrema_by_golden_section() {
        close("argmin(f, -5, 5)", "(t - 1.3)**2", 1.3, 1e-6);
        close(
            "argmax(f, 0, pi)",
            "sin(t)",
            std::f64::consts::FRAC_PI_2,
            1e-6,
        );
        close("argmin(f, 0, 1)", "t", 0.0, 1e-9);
    }

    #[test]
    fn newton_takes_one_step() {
        close("newton_step(f, 1)", "t**2 - 2", 1.5, 1e-8);
    }

    #[test]
    fn the_first_argument_is_a_user_function() {
        assert_eq!(refused("derivative(sin, 1)"), "E301");
        assert_eq!(refused("integral(x, 0, 1)"), "E301");
        assert_eq!(refused("derivative(1, 2)"), "E201");
        assert_eq!(refused("derivative()"), "E203");
    }
}
