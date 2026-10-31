//! Curves and areas (SPEC-2.0 §8.3). Every curve is sampled by hapto in data
//! units and mapped, so it stays right on a view whose axes differ in scale.

use std::f64::consts::{PI, TAU};

use hapto_expr::{Error, Value};

use super::Ctx;
use crate::program::Slot;
use crate::scene::Paint;

pub fn draws(kind: &str) -> bool {
    matches!(
        kind,
        "circle"
            | "ellipse"
            | "arc"
            | "sector"
            | "angle"
            | "path"
            | "bezier"
            | "brace"
            | "spring"
            | "area"
    )
}

const ROUND: usize = 96;

fn pair(v: &Value) -> Option<[[f64; 2]; 2]> {
    let items = v.items()?;
    Some([items.first()?.vec()?, items.get(1)?.vec()?])
}

pub fn draw(c: &mut Ctx) -> Result<(), Error> {
    match c.m.source.kind {
        "circle" | "ellipse" => {
            let Some(o) = c.main()?.vec() else {
                return Ok(());
            };
            let (rx, ry, tilt) = if c.m.source.kind == "circle" {
                let r = c.opt_num("r", f64::NAN)?;
                (r, r, 0.0)
            } else {
                (
                    c.opt_num("rx", f64::NAN)?,
                    c.opt_num("ry", f64::NAN)?,
                    c.opt_num("tilt", 0.0)?,
                )
            };
            if rx.is_nan() || ry.is_nan() || rx <= 0.0 || ry <= 0.0 {
                return Ok(());
            }
            let (s, co) = (tilt.sin(), tilt.cos());
            let pts: Vec<[f64; 2]> = (0..ROUND)
                .map(|i| {
                    let t = TAU * i as f64 / ROUND as f64;
                    let (x, y) = (rx * t.cos(), ry * t.sin());
                    c.css([o[0] + co * x - s * y, o[1] + s * x + co * y])
                })
                .collect();
            closed_shape(c, &pts);
        }
        "arc" | "sector" => {
            let Some(o) = c.main()?.vec() else {
                return Ok(());
            };
            let r = c.opt_num("r", f64::NAN)?;
            let (from, to) = (c.opt_num("from", 0.0)?, c.opt_num("to", TAU)?);
            if r.is_nan() || r <= 0.0 {
                return Ok(());
            }
            let ring = |radius: f64, c: &Ctx, reverse: bool| -> Vec<[f64; 2]> {
                let n = ((to - from).abs() / TAU * ROUND as f64).ceil().max(8.0) as usize;
                (0..=n)
                    .map(|i| {
                        let t = if reverse {
                            to + (from - to) * i as f64 / n as f64
                        } else {
                            from + (to - from) * i as f64 / n as f64
                        };
                        c.css([o[0] + radius * t.cos(), o[1] + radius * t.sin()])
                    })
                    .collect()
            };
            if c.m.source.kind == "arc" {
                let pts = ring(r, c, false);
                c.stroke(&pts, false);
            } else {
                let inner = c.opt_num("inner", 0.0)?.clamp(0.0, r);
                let mut pts = ring(r, c, false);
                if inner > 0.0 {
                    pts.extend(ring(inner, c, true));
                } else {
                    pts.push(c.css(o));
                }
                c.fill(&pts);
            }
        }
        "angle" => angle(c)?,
        "path" => {
            let Slot::Text(d) = &c.m.main else {
                return Ok(());
            };
            let closed = c.opt_bool("closed")?;
            for sub in svg_path(d) {
                let css: Vec<[f64; 2]> = sub.iter().map(|p| c.css(*p)).collect();
                if closed && c.paint.fill.is_some() {
                    c.fill(&css);
                } else {
                    c.stroke(&css, closed);
                }
            }
        }
        "bezier" => {
            let Some(cp) = c
                .main()?
                .items()
                .map(|i| i.iter().filter_map(Value::vec).collect::<Vec<_>>())
            else {
                return Ok(());
            };
            if !(3..=4).contains(&cp.len()) {
                return Ok(());
            }
            let pts: Vec<[f64; 2]> = (0..=64)
                .map(|i| c.css(bezier(&cp, i as f64 / 64.0)))
                .collect();
            c.stroke(&pts, false);
        }
        "brace" => {
            let Some([a, b]) = pair(&c.main()?) else {
                return Ok(());
            };
            let depth = c.opt_num("depth", 10.0)?;
            let side = if c.word("side", "left") == "right" {
                -1.0
            } else {
                1.0
            };
            let pts = brace(c.css(a), c.css(b), depth * side);
            c.stroke(&pts, false);
        }
        "spring" => {
            let Some([a, b]) = pair(&c.main()?) else {
                return Ok(());
            };
            let coils = c.opt_num("coils", 8.0)?.round().clamp(1.0, 200.0) as usize;
            let width = c.opt_num("width", 12.0)?;
            let pts = spring(c.css(a), c.css(b), coils, width);
            c.stroke(&pts, false);
        }
        "area" => area(c)?,
        _ => {}
    }
    Ok(())
}

/// Fill a closed shape when the paint fills, stroke it otherwise.
fn closed_shape(c: &mut Ctx, pts: &[[f64; 2]]) {
    if c.paint.fill.is_some() {
        c.fill(pts);
    } else {
        c.stroke(pts, true);
    }
}

fn bezier(cp: &[[f64; 2]], t: f64) -> [f64; 2] {
    let mut pts = cp.to_vec();
    while pts.len() > 1 {
        pts = pts
            .windows(2)
            .map(|w| {
                [
                    w[0][0] + (w[1][0] - w[0][0]) * t,
                    w[0][1] + (w[1][1] - w[0][1]) * t,
                ]
            })
            .collect();
    }
    pts[0]
}

/// An angle marker at the vertex, `r` CSS px, the short way round unless
/// `reflex`; a square corner when the angle is within half a degree of 90°.
fn angle(c: &mut Ctx) -> Result<(), Error> {
    let Some(v) = c.main()?.vec() else {
        return Ok(());
    };
    let (Some(a), Some(b)) = (
        c.opt("from")?.and_then(|p| p.vec()),
        c.opt("to")?.and_then(|p| p.vec()),
    ) else {
        return Ok(());
    };
    let r = c.opt_num("r", 22.0)?;
    let reflex = c.opt_bool("reflex")?;
    let (pv, pa, pb) = (c.css(v), c.css(a), c.css(b));
    // Angles in screen space, where y grows downwards.
    let ta = (pa[1] - pv[1]).atan2(pa[0] - pv[0]);
    let tb = (pb[1] - pv[1]).atan2(pb[0] - pv[0]);
    let mut sweep = (tb - ta).rem_euclid(TAU);
    if (sweep > PI) != reflex {
        sweep -= TAU;
    }
    let right = match c.word("right", "auto").as_str() {
        "always" => true,
        "never" => false,
        _ => !reflex && (sweep.abs() - PI / 2.0).abs() < 0.5f64.to_radians(),
    };
    if right {
        let s = r * 0.7;
        let (ua, ub) = ([ta.cos(), ta.sin()], [tb.cos(), tb.sin()]);
        let pts = [
            [pv[0] + ua[0] * s, pv[1] + ua[1] * s],
            [pv[0] + (ua[0] + ub[0]) * s, pv[1] + (ua[1] + ub[1]) * s],
            [pv[0] + ub[0] * s, pv[1] + ub[1] * s],
        ];
        c.stroke(&pts, false);
        return Ok(());
    }
    let n = (sweep.abs() / TAU * ROUND as f64).ceil().max(6.0) as usize;
    let pts: Vec<[f64; 2]> = (0..=n)
        .map(|i| {
            let t = ta + sweep * i as f64 / n as f64;
            [pv[0] + r * t.cos(), pv[1] + r * t.sin()]
        })
        .collect();
    c.stroke(&pts, false);
    Ok(())
}

fn brace(a: [f64; 2], b: [f64; 2], depth: f64) -> Vec<[f64; 2]> {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = dx.hypot(dy).max(1e-9);
    let (ux, uy, nx, ny) = (dx / len, dy / len, -dy / len, dx / len);
    // Half the depth rises smoothly from each end; the other half is the
    // tip in the middle: the profile of a curly brace.
    let profile = |s: f64| {
        let u = (s.min(1.0 - s) / 0.12).clamp(0.0, 1.0);
        let rise = u * u * (3.0 - 2.0 * u);
        let tip = (1.0 - (s - 0.5).abs() / 0.06).max(0.0);
        depth * (0.5 * rise + 0.5 * tip)
    };
    (0..=80)
        .map(|i| {
            let s = i as f64 / 80.0;
            let off = profile(s);
            [
                a[0] + ux * s * len + nx * off,
                a[1] + uy * s * len + ny * off,
            ]
        })
        .collect()
}

fn spring(a: [f64; 2], b: [f64; 2], coils: usize, width: f64) -> Vec<[f64; 2]> {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = dx.hypot(dy).max(1e-9);
    let (ux, uy, nx, ny) = (dx / len, dy / len, -dy / len, dx / len);
    let lead = (len * 0.1).min(12.0);
    let mut pts = vec![a, [a[0] + ux * lead, a[1] + uy * lead]];
    let body = len - 2.0 * lead;
    let zig = coils * 2;
    for k in 1..zig {
        let t = lead + body * k as f64 / zig as f64;
        let side = if k % 2 == 1 {
            width / 2.0
        } else {
            -width / 2.0
        };
        pts.push([a[0] + ux * t + nx * side, a[1] + uy * t + ny * side]);
    }
    pts.push([b[0] - ux * lead, b[1] - uy * lead]);
    pts.push(b);
    pts
}

/// One area sample: x, the top curve, the base.
type Sample = (f64, f64, f64);

/// The area under a curve, or between two, over [from, to]. Signed areas
/// paint each part by its sign and hatch the negative ones (§8.3, §20.1).
fn area(c: &mut Ctx) -> Result<(), Error> {
    let (Some(under), between) = (
        c.m.main
            .get("under")
            .or(c.m.main.get("between"))
            .and_then(Slot::formula),
        c.m.main.get("between").is_some(),
    ) else {
        return Ok(());
    };
    let from = c
        .main_key("from")?
        .map(|v| v.num())
        .filter(|n| n.is_finite())
        .unwrap_or(c.map.x.0);
    let to = c
        .main_key("to")?
        .map(|v| v.num())
        .filter(|n| n.is_finite())
        .unwrap_or(c.map.x.1);
    let signed = c.opt_bool("signed")?;
    const N: usize = 240;
    let mut samples = Vec::with_capacity(N + 1);
    for i in 0..=N {
        let x = from + (to - from) * i as f64 / N as f64;
        let mut bound = c.bound.to_vec();
        bound.push(Value::Num(x));
        let v =
            c.ev.eval(&under.checked, &bound)
                .map_err(|e| e.at_path(&under.path))?;
        let (top, base) = if between {
            match v.items() {
                Some([f, g, ..]) => (f.num(), g.num()),
                _ => (f64::NAN, f64::NAN),
            }
        } else {
            (v.num(), 0.0)
        };
        samples.push((x, top, base));
    }
    // Runs of finite samples with one sign of (top - base).
    let mut runs: Vec<(f64, Vec<Sample>)> = Vec::new();
    for s in samples {
        if !(s.1.is_finite() && s.2.is_finite()) {
            runs.push((0.0, vec![]));
            continue;
        }
        let sign = if signed { (s.1 - s.2).signum() } else { 1.0 };
        match runs.last_mut() {
            Some((sg, run)) if *sg == sign || run.is_empty() => {
                *sg = sign;
                run.push(s);
            }
            _ => runs.push((sign, vec![s])),
        }
    }
    for (sign, run) in runs.into_iter().filter(|(_, r)| r.len() > 1) {
        let mut pts: Vec<[f64; 2]> = run.iter().map(|(x, t, _)| c.css([*x, *t])).collect();
        pts.extend(run.iter().rev().map(|(x, _, b)| c.css([*x, *b])));
        let base = c.paint.clone();
        if signed {
            let token = if sign >= 0.0 { "positive" } else { "negative" };
            let colour = c.palette(token);
            c.paint = Paint {
                fill: colour.or(base.fill),
                fill_opacity: base.fill_opacity.max(0.25),
                ..base.clone()
            };
        }
        c.fill(&pts);
        if signed && sign < 0.0 {
            // Hatching: vertical strokes every 6 px across the negative part.
            let hatch = Paint {
                stroke: c.paint.fill,
                width: 1.0,
                fill: None,
                fill_opacity: 0.0,
                ..c.paint.clone()
            };
            let saved = std::mem::replace(&mut c.paint, hatch);
            let (x0, x1) = (
                c.css([run[0].0, 0.0])[0],
                c.css([run[run.len() - 1].0, 0.0])[0],
            );
            let mut x = x0.min(x1) + 3.0;
            while x < x0.max(x1) {
                let data_x = c.map.to_data([x, 0.0])[0];
                let k = run
                    .iter()
                    .position(|(rx, _, _)| *rx >= data_x)
                    .unwrap_or(run.len() - 1);
                let (_, t, b) = run[k];
                let line = [c.css([data_x, t]), c.css([data_x, b])];
                c.stroke(&line, false);
                x += 6.0;
            }
            c.paint = saved;
        }
        c.paint = base;
    }
    Ok(())
}

/// A tiny SVG path reader: M L H V C Q Z, absolute and relative.
fn svg_path(d: &str) -> Vec<Vec<[f64; 2]>> {
    let mut tokens: Vec<String> = Vec::new();
    let mut num = String::new();
    for ch in d.chars() {
        if ch.is_ascii_alphabetic() && ch != 'e' && ch != 'E' {
            if !num.is_empty() {
                tokens.push(std::mem::take(&mut num));
            }
            tokens.push(ch.to_string());
        } else if ch == ','
            || ch.is_whitespace()
            || (ch == '-' && !num.is_empty() && !num.ends_with(['e', 'E']))
        {
            if !num.is_empty() {
                tokens.push(std::mem::take(&mut num));
            }
            if ch == '-' {
                num.push(ch);
            }
        } else {
            num.push(ch);
        }
    }
    if !num.is_empty() {
        tokens.push(num);
    }
    let mut subs: Vec<Vec<[f64; 2]>> = Vec::new();
    let (mut cur, mut start) = ([0.0, 0.0], [0.0, 0.0]);
    let mut i = 0;
    let mut cmd = 'M';
    let n = |i: &mut usize| -> Option<f64> {
        let v = tokens.get(*i)?.parse().ok();
        if v.is_some() {
            *i += 1;
        }
        v
    };
    while i < tokens.len() {
        if let Some(c) = tokens[i].chars().next().filter(|c| c.is_ascii_alphabetic()) {
            cmd = c;
            i += 1;
        }
        let rel = cmd.is_ascii_lowercase();
        let off = |p: [f64; 2], cur: [f64; 2]| {
            if rel {
                [p[0] + cur[0], p[1] + cur[1]]
            } else {
                p
            }
        };
        match cmd.to_ascii_uppercase() {
            'M' => {
                let (Some(x), Some(y)) = (n(&mut i), n(&mut i)) else {
                    break;
                };
                cur = off([x, y], cur);
                start = cur;
                subs.push(vec![cur]);
                cmd = if rel { 'l' } else { 'L' };
            }
            'L' => {
                let (Some(x), Some(y)) = (n(&mut i), n(&mut i)) else {
                    break;
                };
                cur = off([x, y], cur);
                if let Some(s) = subs.last_mut() {
                    s.push(cur);
                }
            }
            'H' => {
                let Some(x) = n(&mut i) else { break };
                cur = [if rel { cur[0] + x } else { x }, cur[1]];
                if let Some(s) = subs.last_mut() {
                    s.push(cur);
                }
            }
            'V' => {
                let Some(y) = n(&mut i) else { break };
                cur = [cur[0], if rel { cur[1] + y } else { y }];
                if let Some(s) = subs.last_mut() {
                    s.push(cur);
                }
            }
            'C' | 'Q' => {
                let count = if cmd.eq_ignore_ascii_case(&'C') { 3 } else { 2 };
                let mut cp = vec![cur];
                for _ in 0..count {
                    let (Some(x), Some(y)) = (n(&mut i), n(&mut i)) else {
                        return subs;
                    };
                    cp.push(off([x, y], cur));
                }
                for k in 1..=24 {
                    let p = bezier(&cp, k as f64 / 24.0);
                    if let Some(s) = subs.last_mut() {
                        s.push(p);
                    }
                }
                cur = *cp.last().unwrap_or(&cur);
            }
            'Z' => {
                cur = start;
                if let Some(s) = subs.last_mut() {
                    s.push(start);
                }
            }
            _ => i += 1,
        }
    }
    subs.into_iter().filter(|s| s.len() > 1).collect()
}

#[cfg(test)]
mod tests {
    use crate::render::Figure;
    use crate::scene::{ink, Canvas};
    use crate::theme::Theme;

    fn scene(mark: &str) -> kinemo_ir::Scene {
        let doc = format!("hapto: 2\nalt: a\nlet: {{ 'f(t)': 'sin(t)' }}\nview: {{ x: {{ range: [-4, 4] }}, y: {{ range: [-2, 2] }}, aspect: equal, axes: none }}\nmarks: [ {mark} ]");
        let mut f = Figure::from_text(&doc).unwrap_or_else(|e| panic!("{mark}: {e:?}"));
        f.scene(Canvas::new(400.0, 200.0, 1.0), &Theme::light())
            .unwrap()
    }

    #[test]
    fn every_curve_kind_draws() {
        for mark in [
            "{ circle: '[0, 0]', r: 1 }",
            "{ ellipse: '[0, 0]', rx: 2, ry: 1, tilt: 0.3 }",
            "{ arc: '[0, 0]', r: 1, from: 0, to: pi }",
            "{ sector: '[0, 0]', r: 1, inner: 0.5, from: 0, to: 2 }",
            "{ angle: '[0, 0]', from: '[1, 0]', to: '[0, 1]' }",
            "{ angle: '[0, 0]', from: '[1, 0]', to: '[1, 1]', reflex: true }",
            "{ path: 'M 0 0 L 1 1 Q 2 0 3 1 Z', closed: true }",
            "{ bezier: '[[0, 0], [1, 2], [2, 0]]' }",
            "{ brace: '[[-1, 0], [1, 0]]', side: right }",
            "{ spring: '[[-2, 0], [2, 0]]', coils: 6 }",
            "{ area: { under: 'f(x)', from: -3, to: 3 }, signed: true }",
            "{ area: { between: '[f(x), 0.5]', from: -1, to: 1 } }",
        ] {
            assert!(!scene(mark).objects.is_empty(), "{mark} drew nothing");
        }
    }

    #[test]
    fn an_ellipse_tilts() {
        let flat = scene("{ ellipse: '[0, 0]', rx: 2, ry: 0.5 }");
        let tilted = scene("{ ellipse: '[0, 0]', rx: 2, ry: 0.5, tilt: 0.7 }");
        assert_ne!(
            format!("{:?}", flat.signals),
            format!("{:?}", tilted.signals)
        );
    }

    #[test]
    fn a_right_angle_gets_a_square_corner() {
        let right = scene("{ angle: '[0, 0]', from: '[1, 0]', to: '[0, 1]' }");
        let arc = scene("{ angle: '[0, 0]', from: '[1, 0]', to: '[1, 1]' }");
        let points = |s: &kinemo_ir::Scene| {
            s.signals
                .iter()
                .filter_map(|sig| match &sig.initial {
                    kinemo_ir::Value::List(l) => Some(l.len()),
                    _ => None,
                })
                .max()
                .unwrap_or(0)
        };
        assert_eq!(points(&right), 3, "a corner is three points");
        assert!(points(&arc) > 3, "an arc is sampled");
    }

    #[test]
    fn a_signed_area_marks_its_negative_part_twice() {
        // sin over [-3, 3]: a negative half and a positive half. The negative
        // one is hatched, so it has more objects than the positive one.
        let s = scene("{ area: { under: 'f(x)', from: -3, to: 3 }, signed: true }");
        let polygons = s.objects.iter().filter(|o| o.kind == "polygon").count();
        let hatches = s.objects.iter().filter(|o| o.kind == "polyline").count();
        assert_eq!(polygons, 2);
        assert!(hatches > 5, "{hatches} hatch strokes");
        let mut f = Figure::from_text("hapto: 2\nalt: a\nview: { x: { range: [-1, 1] }, y: { range: [-1, 1] } }\nmarks: [ { circle: '[0, 0]', r: 0.5, style: { fill: hot } } ]").unwrap();
        let img = f
            .render(Canvas::new(200.0, 200.0, 1.0), &Theme::light())
            .unwrap();
        assert!(ink(&img, Theme::light().bg()) > 0.1);
    }
}
