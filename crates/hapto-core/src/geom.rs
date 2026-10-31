//! Plane geometry hapto does itself: clipping to the plot area.
//!
//! kinemo has no clip region, and a curve leaving the plot must stop at its
//! edge (§7.2 `clip`, §8.6). Polylines are clipped segment by segment
//! (Liang–Barsky), polygons with Sutherland–Hodgman, all in CSS pixels.

/// An axis-aligned rectangle: left, top, right, bottom.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

impl Rect {
    pub fn contains(&self, p: [f64; 2]) -> bool {
        p[0] >= self.left && p[0] <= self.right && p[1] >= self.top && p[1] <= self.bottom
    }

    pub fn grow(&self, by: f64) -> Rect {
        Rect {
            left: self.left - by,
            top: self.top - by,
            right: self.right + by,
            bottom: self.bottom + by,
        }
    }
}

/// The part of segment `a`–`b` inside `r`, if any.
pub fn clip_segment(a: [f64; 2], b: [f64; 2], r: &Rect) -> Option<([f64; 2], [f64; 2])> {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [
        (-dx, a[0] - r.left),
        (dx, r.right - a[0]),
        (-dy, a[1] - r.top),
        (dy, r.bottom - a[1]),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    (t0 <= t1).then(|| {
        (
            [a[0] + t0 * dx, a[1] + t0 * dy],
            [a[0] + t1 * dx, a[1] + t1 * dy],
        )
    })
}

/// A polyline cut to `r`: the runs that stay inside, each cut exactly at the
/// edge. Non-finite points break the line (§4.8).
pub fn clip_polyline(points: &[[f64; 2]], r: &Rect) -> Vec<Vec<[f64; 2]>> {
    let mut runs: Vec<Vec<[f64; 2]>> = Vec::new();
    let mut current: Vec<[f64; 2]> = Vec::new();
    let finite = |p: &[f64; 2]| p[0].is_finite() && p[1].is_finite();
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        if !finite(&a) || !finite(&b) {
            if current.len() > 1 {
                runs.push(std::mem::take(&mut current));
            }
            current.clear();
            continue;
        }
        match clip_segment(a, b, r) {
            Some((p, q)) => {
                if current.last() != Some(&p) {
                    if current.len() > 1 {
                        runs.push(std::mem::take(&mut current));
                    }
                    current = vec![p];
                }
                current.push(q);
                if q != b {
                    runs.push(std::mem::take(&mut current));
                }
            }
            None => {
                if current.len() > 1 {
                    runs.push(std::mem::take(&mut current));
                }
                current.clear();
            }
        }
    }
    if current.len() > 1 {
        runs.push(current);
    }
    runs
}

/// Whether a point is on the kept side of one rectangle edge.
type Inside = fn(&[f64; 2], &Rect) -> bool;
/// Where a segment crosses that edge.
type Cross = fn([f64; 2], [f64; 2], &Rect) -> [f64; 2];

/// A polygon cut to `r` (Sutherland–Hodgman).
pub fn clip_polygon(points: &[[f64; 2]], r: &Rect) -> Vec<[f64; 2]> {
    let edges: [(Inside, Cross); 4] = [
        (|p, r| p[0] >= r.left, |a, b, r| at_x(a, b, r.left)),
        (|p, r| p[0] <= r.right, |a, b, r| at_x(a, b, r.right)),
        (|p, r| p[1] >= r.top, |a, b, r| at_y(a, b, r.top)),
        (|p, r| p[1] <= r.bottom, |a, b, r| at_y(a, b, r.bottom)),
    ];
    let mut out: Vec<[f64; 2]> = points
        .iter()
        .copied()
        .filter(|p| p[0].is_finite() && p[1].is_finite())
        .collect();
    for (inside, cross) in edges {
        if out.is_empty() {
            break;
        }
        let input = std::mem::take(&mut out);
        let mut prev = *input.last().unwrap_or(&[0.0, 0.0]);
        for p in input {
            match (inside(&p, r), inside(&prev, r)) {
                (true, true) => out.push(p),
                (true, false) => {
                    out.push(cross(prev, p, r));
                    out.push(p);
                }
                (false, true) => out.push(cross(prev, p, r)),
                (false, false) => {}
            }
            prev = p;
        }
    }
    out
}

fn at_x(a: [f64; 2], b: [f64; 2], x: f64) -> [f64; 2] {
    let t = (x - a[0]) / (b[0] - a[0]);
    [x, a[1] + t * (b[1] - a[1])]
}

fn at_y(a: [f64; 2], b: [f64; 2], y: f64) -> [f64; 2] {
    let t = (y - a[1]) / (b[1] - a[1]);
    [a[0] + t * (b[0] - a[0]), y]
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: Rect = Rect {
        left: 0.0,
        top: 0.0,
        right: 10.0,
        bottom: 10.0,
    };

    #[test]
    fn segments_are_cut_at_the_edge() {
        assert_eq!(
            clip_segment([-5.0, 5.0], [5.0, 5.0], &R),
            Some(([0.0, 5.0], [5.0, 5.0]))
        );
        assert_eq!(clip_segment([-5.0, -5.0], [-1.0, -1.0], &R), None);
        assert_eq!(
            clip_segment([2.0, 2.0], [3.0, 3.0], &R),
            Some(([2.0, 2.0], [3.0, 3.0]))
        );
    }

    #[test]
    fn a_polyline_leaving_and_returning_is_two_runs() {
        let runs = clip_polyline(
            &[[1.0, 5.0], [5.0, 5.0], [15.0, 5.0], [5.0, 8.0], [2.0, 8.0]],
            &R,
        );
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0], vec![[1.0, 5.0], [5.0, 5.0], [10.0, 5.0]]);
        assert_eq!(*runs[1].last().unwrap(), [2.0, 8.0]);
        let broken = clip_polyline(
            &[
                [1.0, 1.0],
                [2.0, 2.0],
                [f64::NAN, 3.0],
                [4.0, 4.0],
                [5.0, 5.0],
            ],
            &R,
        );
        assert_eq!(broken.len(), 2);
    }

    #[test]
    fn polygons_are_cut_to_the_rectangle() {
        let cut = clip_polygon(&[[-5.0, -5.0], [5.0, -5.0], [5.0, 5.0], [-5.0, 5.0]], &R);
        let area: f64 = cut
            .iter()
            .zip(cut.iter().cycle().skip(1))
            .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
            .sum::<f64>()
            .abs()
            / 2.0;
        assert!((area - 25.0).abs() < 1e-9, "area {area}");
        assert!(clip_polygon(&[[20.0, 20.0], [30.0, 20.0], [30.0, 30.0]], &R).is_empty());
    }
}
