//! Small dense matrices: the arithmetic `mat` values need.
//!
//! Everything returns `None` where the mathematics has no answer — mismatched
//! sizes, a singular inverse — and the caller turns that into no value.

use crate::value::Matrix;

/// Pivots smaller than this are treated as zero: the matrix is singular.
const SINGULAR: f64 = 1e-12;

pub fn new(rows: usize, cols: usize, data: Vec<f64>) -> Option<Matrix> {
    (rows > 0 && cols > 0 && data.len() == rows * cols && rows <= 4 && cols <= 4)
        .then_some(Matrix { rows, cols, data })
}

pub fn identity(n: usize) -> Option<Matrix> {
    let mut data = vec![0.0; n * n];
    for i in 0..n {
        data[i * n + i] = 1.0;
    }
    new(n, n, data)
}

pub fn zip(a: &Matrix, b: &Matrix, f: fn(f64, f64) -> f64) -> Option<Matrix> {
    (a.rows == b.rows && a.cols == b.cols).then(|| Matrix {
        rows: a.rows,
        cols: a.cols,
        data: a.data.iter().zip(&b.data).map(|(x, y)| f(*x, *y)).collect(),
    })
}

pub fn scale(a: &Matrix, k: f64) -> Matrix {
    Matrix {
        rows: a.rows,
        cols: a.cols,
        data: a.data.iter().map(|x| x * k).collect(),
    }
}

pub fn mul(a: &Matrix, b: &Matrix) -> Option<Matrix> {
    if a.cols != b.rows {
        return None;
    }
    let mut data = vec![0.0; a.rows * b.cols];
    for i in 0..a.rows {
        for j in 0..b.cols {
            data[i * b.cols + j] = (0..a.cols).map(|k| a.get(i, k) * b.get(k, j)).sum();
        }
    }
    Some(Matrix {
        rows: a.rows,
        cols: b.cols,
        data,
    })
}

/// A 2×2 matrix applied to a vector; other sizes have no vector to act on.
pub fn apply(a: &Matrix, v: [f64; 2]) -> Option<[f64; 2]> {
    (a.rows == 2 && a.cols == 2).then(|| {
        [
            a.get(0, 0) * v[0] + a.get(0, 1) * v[1],
            a.get(1, 0) * v[0] + a.get(1, 1) * v[1],
        ]
    })
}

#[allow(dead_code)] // the matrix builtins (§4.6.7) are its caller
pub fn transpose(a: &Matrix) -> Matrix {
    let mut data = vec![0.0; a.rows * a.cols];
    for i in 0..a.rows {
        for j in 0..a.cols {
            data[j * a.rows + i] = a.get(i, j);
        }
    }
    Matrix {
        rows: a.cols,
        cols: a.rows,
        data,
    }
}

/// Gauss–Jordan with partial pivoting.
pub fn inverse(a: &Matrix) -> Option<Matrix> {
    if a.rows != a.cols {
        return None;
    }
    let n = a.rows;
    let mut m = a.data.clone();
    let mut inv = identity(n)?.data;
    for col in 0..n {
        let pivot =
            (col..n).max_by(|&i, &j| m[i * n + col].abs().total_cmp(&m[j * n + col].abs()))?;
        if m[pivot * n + col].abs() < SINGULAR {
            return None;
        }
        for k in 0..n {
            m.swap(col * n + k, pivot * n + k);
            inv.swap(col * n + k, pivot * n + k);
        }
        let p = m[col * n + col];
        for k in 0..n {
            m[col * n + k] /= p;
            inv[col * n + k] /= p;
        }
        for row in 0..n {
            if row != col {
                let factor = m[row * n + col];
                for k in 0..n {
                    m[row * n + k] -= factor * m[col * n + k];
                    inv[row * n + k] -= factor * inv[col * n + k];
                }
            }
        }
    }
    new(n, n, inv)
}

/// Determinant by elimination with partial pivoting.
#[allow(dead_code)] // the matrix builtins (§4.6.7) are its caller
pub fn det(a: &Matrix) -> Option<f64> {
    if a.rows != a.cols {
        return None;
    }
    let n = a.rows;
    let mut m = a.data.clone();
    let mut d = 1.0;
    for col in 0..n {
        let pivot =
            (col..n).max_by(|&i, &j| m[i * n + col].abs().total_cmp(&m[j * n + col].abs()))?;
        if m[pivot * n + col] == 0.0 {
            return Some(0.0);
        }
        if pivot != col {
            for k in 0..n {
                m.swap(col * n + k, pivot * n + k);
            }
            d = -d;
        }
        let p = m[col * n + col];
        d *= p;
        for row in col + 1..n {
            let factor = m[row * n + col] / p;
            for k in col..n {
                m[row * n + k] -= factor * m[col * n + k];
            }
        }
    }
    Some(d)
}

/// `a ** k` for an integer `k`; a negative power inverts first.
pub fn pow(a: &Matrix, k: f64) -> Option<Matrix> {
    if a.rows != a.cols || k.fract() != 0.0 || k.abs() > 64.0 {
        return None;
    }
    let base = if k < 0.0 { inverse(a)? } else { a.clone() };
    let mut result = identity(a.rows)?;
    for _ in 0..(k.abs() as usize) {
        result = mul(&result, &base)?;
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(rows: usize, data: &[f64]) -> Matrix {
        new(rows, data.len() / rows, data.to_vec()).unwrap()
    }

    #[test]
    fn products_and_powers() {
        let a = m(2, &[1.0, 2.0, 3.0, 4.0]);
        assert_eq!(mul(&a, &a).unwrap().data, vec![7.0, 10.0, 15.0, 22.0]);
        assert_eq!(pow(&a, 2.0).unwrap(), mul(&a, &a).unwrap());
        assert_eq!(pow(&a, 0.0).unwrap(), identity(2).unwrap());
        assert_eq!(apply(&a, [1.0, 1.0]), Some([3.0, 7.0]));
        assert!(pow(&a, 0.5).is_none());
    }

    #[test]
    fn inverse_and_determinant() {
        let a = m(2, &[4.0, 7.0, 2.0, 6.0]);
        assert!((det(&a).unwrap() - 10.0).abs() < 1e-12);
        let inv = inverse(&a).unwrap();
        let id = mul(&a, &inv).unwrap();
        for (x, y) in id.data.iter().zip(identity(2).unwrap().data) {
            assert!((x - y).abs() < 1e-12);
        }
        assert!(inverse(&m(2, &[1.0, 2.0, 2.0, 4.0])).is_none());
        assert_eq!(det(&m(2, &[1.0, 2.0, 2.0, 4.0])), Some(0.0));
        let three = m(3, &[2.0, 0.0, 1.0, 1.0, 3.0, 2.0, 1.0, 1.0, 2.0]);
        assert!((det(&three).unwrap() - 6.0).abs() < 1e-12);
    }

    #[test]
    fn mismatched_sizes_have_no_answer() {
        let a = m(2, &[1.0, 2.0, 3.0, 4.0]);
        let b = m(3, &[1.0; 9]);
        assert!(zip(&a, &b, |x, y| x + y).is_none());
        assert!(mul(&a, &b).is_none());
        assert!(new(5, 5, vec![0.0; 25]).is_none());
    }
}
