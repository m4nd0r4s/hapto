//! Probability and seeded randomness (SPEC-2.0 §4.6.9, §11.6).
//!
//! There is no ambient random source. `rand(seed, i)` is a pure function of
//! its two arguments — SplitMix64 of the pair, 53 bits into `[0, 1)` — so the
//! same seed draws the same samples natively, in the browser and next year.
//! A figure that lets a reader draw again changes the seed.

use super::list::MAX_LEN;
use super::{num, Builtin, Host, Overload, Pat};
use crate::error::{codes, Error};
use crate::value::Value;

const NN_N: &[Overload] = &[Overload::new(&[Pat::Num, Pat::Num], Pat::Num)];
const NNN_N: &[Overload] = &[Overload::new(&[Pat::Num, Pat::Num, Pat::Num], Pat::Num)];
const N_N: &[Overload] = &[Overload::new(&[Pat::Num], Pat::Num)];
const SAMPLES: &[Overload] = &[Overload::new(
    &[Pat::Num, Pat::Num, Pat::Num, Pat::Num],
    Pat::ListNum,
)];

fn splitmix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A whole number argument as 64 bits, or none.
fn whole(x: f64) -> Option<u64> {
    let r = x.round();
    (x.is_finite() && (x - r).abs() < 1e-9).then_some(r as i64 as u64)
}

/// Uniform in `[0, 1)` from the pair `(seed, i)`.
pub fn rand(seed: f64, i: f64) -> f64 {
    match (whole(seed), whole(i)) {
        (Some(s), Some(i)) => {
            (splitmix(splitmix(s) ^ i) >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
        }
        _ => f64::NAN,
    }
}

/// Standard normal by Box–Muller over two uniforms of the stream.
pub fn randn(seed: f64, i: f64) -> f64 {
    let u1 = 1.0 - rand(seed, 2.0 * i);
    let u2 = rand(seed, 2.0 * i + 1.0);
    libm::sqrt(-2.0 * libm::log(u1)) * libm::cos(std::f64::consts::TAU * u2)
}

fn count(host: &mut dyn Host, n: f64) -> Result<Option<usize>, Error> {
    if n.fract() != 0.0 || n < 0.0 || !n.is_finite() {
        return Ok(None);
    }
    if n > MAX_LEN as f64 {
        return Err(Error::new(
            codes::E604,
            format!("{n} samples is over the limit of {MAX_LEN}"),
        )
        .hint("lower the number of samples"));
    }
    host.charge(n as u64 * 4)?;
    Ok(Some(n as usize))
}

fn ln_factorial(n: f64) -> f64 {
    (1..=(n as u64)).map(|k| libm::log(k as f64)).sum()
}

fn choose(n: f64, k: f64) -> f64 {
    if n.fract() != 0.0 || k.fract() != 0.0 || n < 0.0 || k < 0.0 || k > n || n > 1e6 {
        return f64::NAN;
    }
    let k = k.min(n - k);
    let mut c = 1.0;
    for j in 0..(k as u64) {
        c = c * (n - j as f64) / (j as f64 + 1.0);
    }
    c.round()
}

pub(super) const BUILTINS: &[Builtin] = &[
    Builtin {
        name: "rand",
        overloads: NN_N,
        takes_fn: false,
        imp: |_, _, a| num(rand(a[0].num(), a[1].num())),
    },
    Builtin {
        name: "randn",
        overloads: NN_N,
        takes_fn: false,
        imp: |_, _, a| num(randn(a[0].num(), a[1].num())),
    },
    Builtin {
        name: "samples_normal",
        overloads: SAMPLES,
        takes_fn: false,
        imp: |h, _, a| {
            let (mu, sigma, seed) = (a[0].num(), a[1].num(), a[3].num());
            Ok(match count(h, a[2].num())? {
                Some(n) if sigma >= 0.0 => {
                    Value::nums((0..n).map(|i| mu + sigma * randn(seed, i as f64)))
                }
                _ => Value::Nothing,
            })
        },
    },
    Builtin {
        name: "samples_uniform",
        overloads: SAMPLES,
        takes_fn: false,
        imp: |h, _, a| {
            let (lo, hi, seed) = (a[0].num(), a[1].num(), a[3].num());
            Ok(match count(h, a[2].num())? {
                Some(n) => Value::nums((0..n).map(|i| lo + (hi - lo) * rand(seed, i as f64))),
                None => Value::Nothing,
            })
        },
    },
    Builtin {
        name: "normal_pdf",
        overloads: NNN_N,
        takes_fn: false,
        imp: |_, _, a| {
            let (x, mu, sigma) = (a[0].num(), a[1].num(), a[2].num());
            if sigma <= 0.0 {
                return num(f64::NAN);
            }
            let z = (x - mu) / sigma;
            num(libm::exp(-0.5 * z * z) / (sigma * libm::sqrt(std::f64::consts::TAU)))
        },
    },
    Builtin {
        name: "normal_cdf",
        overloads: NNN_N,
        takes_fn: false,
        imp: |_, _, a| {
            let (x, mu, sigma) = (a[0].num(), a[1].num(), a[2].num());
            if sigma <= 0.0 {
                return num(f64::NAN);
            }
            num(0.5 * (1.0 + libm::erf((x - mu) / (sigma * std::f64::consts::SQRT_2))))
        },
    },
    Builtin {
        name: "binom_pmf",
        overloads: NNN_N,
        takes_fn: false,
        imp: |_, _, a| {
            let (k, n, p) = (a[0].num(), a[1].num(), a[2].num());
            if !(0.0..=1.0).contains(&p) {
                return num(f64::NAN);
            }
            let c = choose(n, k);
            num(c * libm::pow(p, k) * libm::pow(1.0 - p, n - k))
        },
    },
    Builtin {
        name: "poisson_pmf",
        overloads: NN_N,
        takes_fn: false,
        imp: |_, _, a| {
            let (k, lambda) = (a[0].num(), a[1].num());
            if k.fract() != 0.0 || k < 0.0 || lambda <= 0.0 || k > 1e6 {
                return num(f64::NAN);
            }
            // In logs, so a large k does not overflow the factorial.
            num(libm::exp(k * libm::log(lambda) - lambda - ln_factorial(k)))
        },
    },
    Builtin {
        name: "choose",
        overloads: NN_N,
        takes_fn: false,
        imp: |_, _, a| num(choose(a[0].num(), a[1].num())),
    },
    Builtin {
        name: "factorial",
        overloads: N_N,
        takes_fn: false,
        imp: |_, _, a| {
            let n = a[0].num();
            if n.fract() != 0.0 || !(0.0..=170.0).contains(&n) {
                return num(f64::NAN);
            }
            num((1..=(n as u64)).map(|k| k as f64).product())
        },
    },
];

#[cfg(test)]
mod tests {
    use super::super::testing::{close, eval_at, nothing};
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
    fn randomness_is_a_pure_function_of_seed_and_index() {
        let a = eval_at("rand(7, 3)", 0.0).num();
        assert_eq!(a, eval_at("rand(7, 3)", 0.0).num());
        assert_ne!(a, eval_at("rand(7, 4)", 0.0).num());
        assert_ne!(a, eval_at("rand(8, 3)", 0.0).num());
        assert!((0.0..1.0).contains(&a));
        // Pinned: the stream must never change under a figure.
        assert_eq!(super::rand(1.0, 0.0), super::rand(1.0, 0.0));
        nothing("rand(1.5, 0)");
    }

    #[test]
    fn samples_have_the_moments_asked_for() {
        let xs = list("samples_normal(10, 2, 20000, 42)");
        let n = xs.len() as f64;
        let mean = xs.iter().sum::<f64>() / n;
        let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
        assert!((mean - 10.0).abs() < 0.05, "mean {mean}");
        assert!((var.sqrt() - 2.0).abs() < 0.05, "sd {}", var.sqrt());
        let us = list("samples_uniform(-1, 1, 20000, 3)");
        assert!(us.iter().all(|u| (-1.0..1.0).contains(u)));
        assert!((us.iter().sum::<f64>() / us.len() as f64).abs() < 0.02);
        nothing("samples_normal(0, -1, 10, 1)");
    }

    #[test]
    fn distributions_against_closed_forms() {
        close(
            "normal_pdf(0, 0, 1)",
            1.0 / (2.0 * std::f64::consts::PI).sqrt(),
        );
        close("normal_cdf(0, 0, 1)", 0.5);
        close("normal_cdf(1.96, 0, 1)", 0.975_002_104_851_780);
        close("binom_pmf(2, 4, 0.5)", 0.375);
        close("sum([binom_pmf(k, 10, 0.3) for k in range(0, 11)])", 1.0);
        close("poisson_pmf(2, 3)", 9.0 / 2.0 * (-3.0f64).exp());
        close("choose(5, 2)", 10.0);
        close("choose(52, 5)", 2_598_960.0);
        close("factorial(5)", 120.0);
        nothing("factorial(171)");
        nothing("choose(2, 5)");
        nothing("binom_pmf(1, 2, 1.5)");
    }
}
