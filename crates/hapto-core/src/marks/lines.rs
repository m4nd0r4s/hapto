//! Points and lines (SPEC-2.0 §8.2), plus polylines and polygons.

use hapto_expr::{Error, Value};

use super::Ctx;
use crate::scene::Paint;

pub fn draws(kind: &str) -> bool {
    matches!(
        kind,
        "point"
            | "segment"
            | "arrow"
            | "vector"
            | "line"
            | "ray"
            | "hline"
            | "vline"
            | "tangent"
            | "secant"
            | "drop"
            | "polyline"
            | "polygon"
    )
}

fn points(v: &Value) -> Option<Vec<[f64; 2]>> {
    v.items()?.iter().map(Value::vec).collect()
}

pub fn draw(c: &mut Ctx) -> Result<(), Error> {
    match c.m.source.kind {
        "point" => point(c),
        "segment" => {
            let Some([a, b]) = points(&c.main()?).and_then(|p| <[[f64; 2]; 2]>::try_from(p).ok())
            else {
                return Ok(());
            };
            let arrows = c.word("arrows", "none");
            arrow(
                c,
                a,
                b,
                matches!(arrows.as_str(), "end" | "both"),
                matches!(arrows.as_str(), "start" | "both"),
                10.0,
            )
        }
        "arrow" => {
            let Some([a, b]) = points(&c.main()?).and_then(|p| <[[f64; 2]; 2]>::try_from(p).ok())
            else {
                return Ok(());
            };
            let head = c.opt_num("head", 10.0)?;
            let double = c.opt_bool("double")?;
            arrow(c, a, b, true, double, head)
        }
        "vector" => {
            let Some(v) = c.main()?.vec() else {
                return Ok(());
            };
            let at = c.opt("at")?.and_then(|p| p.vec()).unwrap_or([0.0, 0.0]);
            let head = c.opt_num("head", 10.0)?;
            arrow(c, at, [at[0] + v[0], at[1] + v[1]], true, false, head)
        }
        "line" => {
            let (p, d) = match c.main()? {
                v if v.items().is_some() => {
                    let Some([a, b]) = points(&v).and_then(|p| <[[f64; 2]; 2]>::try_from(p).ok())
                    else {
                        return Ok(());
                    };
                    (a, [b[0] - a[0], b[1] - a[1]])
                }
                _ => {
                    let Some(p) = c.main_key("through")?.and_then(|v| v.vec()) else {
                        return Ok(());
                    };
                    let d = match (c.main_key("direction")?, c.main_key("slope")?) {
                        (Some(d), _) => d.vec(),
                        (None, Some(s)) => Some([1.0, s.num()]),
                        _ => None,
                    };
                    let Some(d) = d else { return Ok(()) };
                    (p, d)
                }
            };
            infinite(c, p, d, true)
        }
        "ray" => {
            let Some([a, b]) = points(&c.main()?).and_then(|p| <[[f64; 2]; 2]>::try_from(p).ok())
            else {
                return Ok(());
            };
            infinite(c, a, [b[0] - a[0], b[1] - a[1]], false)
        }
        "hline" => {
            let y = c.main()?.num();
            let (x0, x1) = c.map.x;
            straight(c, [x0, y], [x1, y])
        }
        "vline" => {
            let x = c.main()?.num();
            let (y0, y1) = c.map.y;
            straight(c, [x, y0], [x, y1])
        }
        "tangent" => {
            let x = c.main()?.num();
            let (y, slope) = (c.opt_num("_y", f64::NAN)?, c.opt_num("_slope", f64::NAN)?);
            if !(x.is_finite() && y.is_finite() && slope.is_finite()) {
                return Ok(());
            }
            match c
                .opt("length")?
                .map(|v| v.num())
                .filter(|l| l.is_finite() && *l > 0.0)
            {
                Some(len) => {
                    let n = 1f64.hypot(slope);
                    let (dx, dy) = (len / 2.0 / n, len / 2.0 * slope / n);
                    straight(c, [x - dx, y - dy], [x + dx, y + dy])
                }
                None => infinite(c, [x, y], [1.0, slope], true),
            }
        }
        "secant" => {
            let xs = c.main()?;
            let ys = c.opt("_ys")?.unwrap_or(Value::Nothing);
            let (Some(xs), Some(ys)) = (xs.items(), ys.items()) else {
                return Ok(());
            };
            let (a, b) = ([xs[0].num(), ys[0].num()], [xs[1].num(), ys[1].num()]);
            infinite(c, a, [b[0] - a[0], b[1] - a[1]], true)
        }
        "drop" => {
            let Some(p) = c.main()?.vec() else {
                return Ok(());
            };
            let to = c.word("to", "both");
            let zero_x = 0f64.clamp(c.map.x.0.min(c.map.x.1), c.map.x.0.max(c.map.x.1));
            let zero_y = 0f64.clamp(c.map.y.0.min(c.map.y.1), c.map.y.0.max(c.map.y.1));
            if to == "x" || to == "both" {
                straight(c, p, [p[0], zero_y])?;
            }
            if to == "y" || to == "both" {
                straight(c, p, [zero_x, p[1]])?;
            }
            Ok(())
        }
        "polyline" | "polygon" => {
            let Some(pts) = points(&c.main()?) else {
                return Ok(());
            };
            let css: Vec<[f64; 2]> = pts.iter().map(|p| c.css(*p)).collect();
            if c.m.source.kind == "polygon" {
                c.fill(&css);
            } else {
                let closed = c.opt_bool("closed")?;
                c.stroke(&css, closed);
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn point(c: &mut Ctx) -> Result<(), Error> {
    let Some(at) = c.main()?.vec() else {
        return Ok(());
    };
    let r = c.opt_num("r", 5.0)?;
    let p = c.css(at);
    if c.clip && !c.map.plot.grow(r).contains(p) {
        return Ok(());
    }
    let shape = c.word("shape", "circle");
    let paint = c.paint.clone();
    match shape.as_str() {
        "square" => c.b.polyline(
            &[
                [p[0] - r, p[1] - r],
                [p[0] + r, p[1] - r],
                [p[0] + r, p[1] + r],
                [p[0] - r, p[1] + r],
            ],
            true,
            &paint,
            c.z,
        ),
        "diamond" => c.b.polyline(
            &[
                [p[0], p[1] - r],
                [p[0] + r, p[1]],
                [p[0], p[1] + r],
                [p[0] - r, p[1]],
            ],
            true,
            &paint,
            c.z,
        ),
        "cross" => {
            let ink = Paint {
                stroke: paint.fill.or(paint.stroke),
                width: 2.0,
                fill: None,
                ..paint.clone()
            };
            c.b.polyline(
                &[[p[0] - r, p[1] - r], [p[0] + r, p[1] + r]],
                false,
                &ink,
                c.z,
            );
            c.b.polyline(
                &[[p[0] - r, p[1] + r], [p[0] + r, p[1] - r]],
                false,
                &ink,
                c.z,
            );
        }
        "ring" => {
            let ring = Paint {
                stroke: paint.fill.or(paint.stroke),
                width: 2.0,
                fill: None,
                fill_opacity: 0.0,
                ..paint.clone()
            };
            c.b.dot(p, r, &ring, c.z);
        }
        _ => c.b.dot(p, r, &paint, c.z),
    }
    Ok(())
}

fn straight(c: &mut Ctx, a: [f64; 2], b: [f64; 2]) -> Result<(), Error> {
    let (a, b) = (c.css(a), c.css(b));
    c.stroke(&[a, b], false);
    Ok(())
}

fn infinite(c: &mut Ctx, p: [f64; 2], d: [f64; 2], both_ways: bool) -> Result<(), Error> {
    let Some((back, ahead)) = c.far(p, d) else {
        return Ok(());
    };
    straight(c, if both_ways { back } else { p }, ahead)
}

fn arrow(
    c: &mut Ctx,
    a: [f64; 2],
    b: [f64; 2],
    end: bool,
    start: bool,
    head: f64,
) -> Result<(), Error> {
    let (pa, pb) = (c.css(a), c.css(b));
    // Shorten the shaft under each head so its square end does not show.
    let shorten = |from: [f64; 2], to: [f64; 2]| {
        let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
        let len = dx.hypot(dy).max(1e-9);
        let k = (head * 0.6).min(len / 2.0) / len;
        [to[0] - dx * k, to[1] - dy * k]
    };
    let shaft_b = if end { shorten(pa, pb) } else { pb };
    let shaft_a = if start { shorten(pb, pa) } else { pa };
    c.stroke(&[shaft_a, shaft_b], false);
    if end {
        c.arrowhead(pb, pa, head);
    }
    if start {
        c.arrowhead(pa, pb, head);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::render::Figure;
    use crate::scene::Canvas;
    use crate::theme::Theme;

    /// Every kind here draws something, alone, at its defaults.
    #[test]
    fn every_point_and_line_kind_draws() {
        let marks = [
            "{ point: '[1, 1]', shape: square }",
            "{ segment: '[[0, 0], [1, 1]]', arrows: both }",
            "{ arrow: '[[0, 0], [1, 1]]' }",
            "{ vector: '[1, 1]' }",
            "{ line: '[[0, 0], [1, 1]]' }",
            "{ line: { through: '[0, 1]', direction: '[1, 0]' } }",
            "{ ray: '[[0, 0], [1, 1]]' }",
            "{ hline: 1 }",
            "{ vline: 1 }",
            "{ tangent: 1, of: f }",
            "{ tangent: 1, of: f, length: 1 }",
            "{ secant: '[0, 1]', of: f }",
            "{ drop: '[1, 1]' }",
            "{ polyline: '[[0, 0], [1, 2], [2, 0]]' }",
            "{ polygon: '[[0, 0], [1, 2], [2, 0]]' }",
        ];
        for mark in marks {
            let doc = format!("hapto: 2\nalt: a\nlet: {{ 'f(t)': 't**2' }}\nview: {{ x: {{ range: [-2, 2] }}, y: {{ range: [-2, 2] }}, axes: none }}\nmarks: [ {mark} ]");
            let mut f = Figure::from_text(&doc).unwrap_or_else(|e| panic!("{mark}: {e:?}"));
            let scene = f
                .scene(Canvas::new(200.0, 100.0, 1.0), &Theme::light())
                .unwrap();
            assert!(!scene.objects.is_empty(), "{mark} drew nothing");
        }
    }
}
