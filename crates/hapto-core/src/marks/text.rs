//! Text, maths and labels on marks (SPEC-2.0 §10).

use hapto_expr::{format, Error, Value};

use super::Ctx;
use crate::geom::Rect;
use crate::program::{Slot, TextPiece};
use crate::scene::{Builder, Paint};

pub fn draws(kind: &str) -> bool {
    matches!(kind, "text" | "math")
}

/// The smallest text drawn, in CSS px (§10.5).
const FLOOR: f64 = 11.0;

/// A value as text: numbers through the spec, or four significant digits.
pub fn show(v: &Value, spec: Option<&str>) -> String {
    match v {
        Value::Num(n) => match spec {
            Some(s) => format::format(*n, s).unwrap_or_else(|| format::default(*n)),
            None => format::default(*n),
        },
        Value::Vec([x, y]) if x.is_finite() && y.is_finite() => {
            format!("({}, {})", format::default(*x), format::default(*y))
        }
        v if v.is_nothing() => format::NO_VALUE.into(),
        other => other.to_string(),
    }
}

impl Ctx<'_, '_> {
    /// A compiled template, evaluated.
    pub fn template(&mut self, key: &str) -> Result<Option<String>, Error> {
        let Some((_, pieces)) = self.m.texts.iter().find(|(k, _)| k == key) else {
            return Ok(None);
        };
        let mut out = String::new();
        for piece in pieces {
            match piece {
                TextPiece::Lit(l) => out.push_str(l),
                TextPiece::Expr(c, spec) => {
                    let v = self.eval(c)?;
                    out.push_str(&show(&v, spec.as_deref()));
                }
            }
        }
        Ok(Some(out))
    }
}

/// Where the anchor point sits on the text's box, as a shift from the
/// anchor to the box's centre, in units of the box's half-size.
fn shift(anchor: &str) -> (f64, f64) {
    let h = if anchor.ends_with("left") {
        1.0
    } else if anchor.ends_with("right") {
        -1.0
    } else {
        0.0
    };
    let v = if anchor.starts_with("top") {
        1.0
    } else if anchor.starts_with("bottom") || anchor.starts_with("baseline") {
        -1.0
    } else {
        0.0
    };
    (h, v)
}

/// Draw text with a halo of the background behind it, for legibility over
/// data (§9.2).
#[allow(clippy::too_many_arguments)]
fn put(
    b: &mut Builder,
    text: &str,
    centre: [f64; 2],
    size: f64,
    rotate: f64,
    wrap: Option<f64>,
    ink: Option<[f64; 4]>,
    bg: [f64; 4],
    halo: bool,
    z: f64,
) {
    if halo {
        let halo_paint = Paint {
            stroke: Some(bg),
            width: 3.0,
            fill: Some(bg),
            fill_opacity: 1.0,
            opacity: 1.0,
            dash: None,
        };
        b.text(text, centre, size, rotate, wrap, &halo_paint, z);
    }
    let paint = Paint {
        fill: ink,
        fill_opacity: 1.0,
        opacity: 1.0,
        ..Default::default()
    };
    b.text(text, centre, size, rotate, wrap, &paint, z + 0.001);
}

pub fn draw(c: &mut Ctx) -> Result<(), Error> {
    let Some(at) = c.opt("at")?.and_then(|v| v.vec()) else {
        return Ok(());
    };
    let size = c.opt_num("size", 14.0)?.max(FLOOR);
    let rotate = c.opt_num("rotate", 0.0)?;
    let wrap = c
        .opt("max_width")?
        .map(|v| v.num())
        .filter(|w| w.is_finite() && *w > 0.0);
    let offset = c.opt("offset")?.and_then(|v| v.vec()).unwrap_or([0.0, 0.0]);
    let anchor = c.word("anchor", "center");
    let p = c.css(at);
    let ink = c.paint.fill.or(c.palette("fg"));
    let bg = c.palette("bg").unwrap_or([1.0; 4]);
    if c.m.source.kind == "math" {
        let Some(tex) = c.template("main")? else {
            return Ok(());
        };
        c.b.math(
            &tex,
            [p[0] + offset[0], p[1] - offset[1]],
            size,
            true,
            ink.unwrap_or([0.0, 0.0, 0.0, 1.0]),
            c.z,
        );
        return Ok(());
    }
    let Some(mut text) = c.template("main")? else {
        return Ok(());
    };
    if c.word("weight", "regular") == "bold" {
        text = format!("**{text}**");
    }
    let [w, h] = Builder::measure(&text, size, wrap);
    let (sx, sy) = shift(&anchor);
    let centre = [
        p[0] + offset[0] + sx * w / 2.0,
        p[1] - offset[1] + sy * h / 2.0,
    ];
    if c.word("background", "none") == "box" {
        let pad = 4.0;
        let boxed = Paint {
            fill: Some(bg),
            fill_opacity: 1.0,
            stroke: c.palette("faint"),
            width: 1.0,
            opacity: 1.0,
            dash: None,
        };
        let r = [
            centre[0] - w / 2.0 - pad,
            centre[1] - h / 2.0 - pad,
            centre[0] + w / 2.0 + pad,
            centre[1] + h / 2.0 + pad,
        ];
        c.b.polyline(
            &[[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]],
            true,
            &boxed,
            c.z - 0.01,
        );
    }
    let halo = c.m.style.halo.is_none()
        || matches!(
            c.m.style.halo.clone().map(|h| c.eval(&h)),
            Some(Ok(Value::Bool(true)))
        );
    put(c.b, &text, centre, size, rotate, wrap, ink, bg, halo, c.z);
    c.b.labels.push(Rect {
        left: centre[0] - w / 2.0,
        top: centre[1] - h / 2.0,
        right: centre[0] + w / 2.0,
        bottom: centre[1] + h / 2.0,
    });
    Ok(())
}

fn overlap(a: &Rect, b: &Rect) -> f64 {
    let w = (a.right.min(b.right) - a.left.max(b.left)).max(0.0);
    let h = (a.bottom.min(b.bottom) - a.top.max(b.top)).max(0.0);
    w * h
}

/// The point a label attaches to, in CSS px, by the mark's kind (§10.3).
fn label_anchor(c: &mut Ctx) -> Result<Option<[f64; 2]>, Error> {
    let main = c.main()?;
    let points: Option<Vec<[f64; 2]>> = main
        .items()
        .map(|i| i.iter().filter_map(Value::vec).collect());
    let at_mid =
        c.m.source
            .common
            .label
            .as_ref()
            .and_then(|l| l.at.as_deref())
            == Some("mid");
    Ok(match c.m.source.kind {
        "vector" => {
            let at = c.opt("at")?.and_then(|v| v.vec()).unwrap_or([0.0, 0.0]);
            main.vec().map(|v| c.css([at[0] + v[0], at[1] + v[1]]))
        }
        "segment" | "arrow" | "ray" | "brace" | "spring" | "bezier" | "polyline" => points
            .and_then(|p| {
                let (a, b) = (*p.first()?, *p.last()?);
                Some(c.css(if at_mid {
                    [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
                } else {
                    b
                }))
            }),
        "polygon" => points.filter(|p| !p.is_empty()).map(|p| {
            let n = p.len() as f64;
            c.css([
                p.iter().map(|q| q[0]).sum::<f64>() / n,
                p.iter().map(|q| q[1]).sum::<f64>() / n,
            ])
        }),
        "function" => {
            // The last visible point: just inside the right edge of the view.
            let x = c.map.x.1 - (c.map.x.1 - c.map.x.0) * 0.02;
            match c.m.main.formula() {
                Some(f) => {
                    let mut bound = c.bound.to_vec();
                    bound.push(Value::Num(x));
                    let y =
                        c.ev.eval(&f.checked, &bound)
                            .map_err(|e| e.at_path(&f.path))?
                            .num();
                    y.is_finite().then(|| c.css([x, y]))
                }
                None => None,
            }
        }
        _ => main.vec().map(|v| c.css(v)),
    })
}

/// Place a mark's label, trying the four diagonals when it would cover an
/// earlier label by more than 30% of its area (§10.3).
pub fn label(c: &mut Ctx) -> Result<(), Error> {
    let Some(text) = c.template("label")? else {
        return Ok(());
    };
    let Some(p) = label_anchor(c)? else {
        return Ok(());
    };
    let spec = c.m.source.common.label.clone();
    let size = match spec.as_ref().and_then(|l| l.size.clone()) {
        Some(f) => c.formula_num(&f.text, 14.0)?,
        None => 14.0,
    }
    .max(FLOOR);
    let [w, h] = Builder::measure(&text, size, None);
    let gap = 6.0;
    let candidates = if c.m.source.kind == "function" {
        vec![(1.0, 0.0)]
    } else {
        vec![(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)]
    };
    let rect_for = |(dx, dy): (f64, f64)| {
        let cx = p[0] + dx * (gap + w / 2.0);
        let cy = p[1] - dy * (gap + h / 2.0);
        Rect {
            left: cx - w / 2.0,
            top: cy - h / 2.0,
            right: cx + w / 2.0,
            bottom: cy + h / 2.0,
        }
    };
    let chosen = candidates
        .iter()
        .map(|d| rect_for(*d))
        .find(|r| c.b.labels.iter().all(|o| overlap(r, o) <= 0.3 * w * h))
        .unwrap_or_else(|| rect_for(candidates[0]));
    let centre = [
        (chosen.left + chosen.right) / 2.0,
        (chosen.top + chosen.bottom) / 2.0,
    ];
    let ink = c.palette("fg");
    let bg = c.palette("bg").unwrap_or([1.0; 4]);
    put(
        c.b,
        &text,
        centre,
        size,
        0.0,
        None,
        ink,
        bg,
        true,
        c.z + 0.5,
    );
    c.b.labels.push(chosen);
    Ok(())
}

impl Ctx<'_, '_> {
    /// A literal number written where a formula may be (label sizes).
    fn formula_num(&self, text: &str, default: f64) -> Result<f64, Error> {
        Ok(text.parse().unwrap_or(default))
    }

    /// The main argument as written, for text kinds.
    pub fn main_text(&self) -> Option<&str> {
        match &self.m.main {
            Slot::Text(t) => Some(t),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::render::Figure;
    use crate::scene::{ink, Canvas};
    use crate::theme::Theme;

    fn render(marks: &str) -> (Figure, f64) {
        let doc = format!("hapto: 2\nalt: a\nvalues: [ {{ name: s, type: number, range: [0, 2], default: 1.25 }} ]\nview: {{ x: {{ range: [0, 10] }}, y: {{ range: [0, 10] }}, axes: none }}\nmarks: [ {marks} ]");
        let mut f = Figure::from_text(&doc).unwrap_or_else(|e| panic!("{e:?}"));
        let img = f
            .render(Canvas::new(400.0, 200.0, 1.0), &Theme::light())
            .unwrap();
        (f, ink(&img, Theme::light().bg()))
    }

    #[test]
    fn text_draws_with_interpolation() {
        let (_, painted) = render(
            "{ text: 'slope = {s:.2f}', at: '[5, 5]', anchor: bottom-left, background: box }",
        );
        assert!(painted > 0.005, "{painted}");
        assert_eq!(
            super::show(&hapto_expr::Value::Num(1.25), Some(".2f")),
            "1.25"
        );
        assert_eq!(super::show(&hapto_expr::Value::Num(f64::NAN), None), "—");
    }

    #[test]
    fn a_label_follows_its_mark_and_avoids_another() {
        let doc = "hapto: 2\nalt: a\nview: { x: { range: [0, 10] }, y: { range: [0, 10] }, axes: none }\nmarks:\n  - { point: '[5, 5]', label: 'first' }\n  - { point: '[5, 5]', label: 'second' }";
        let mut f = Figure::from_text(doc).unwrap();
        let s = f
            .scene(Canvas::new(400.0, 200.0, 1.0), &Theme::light())
            .unwrap();
        let texts: Vec<f64> = s
            .objects
            .iter()
            .filter(|o| o.kind == "text")
            .filter_map(|o| {
                o.props
                    .get("x")
                    .map(|id| s.signals[*id as usize].initial.as_f64())
            })
            .collect();
        // Two labels, two text objects each (halo and ink), not on top of each other.
        assert_eq!(texts.len(), 4);
        assert!(
            (texts[0] - texts[2]).abs() > 0.1,
            "the second label moved: {texts:?}"
        );
    }

    #[test]
    fn interpolation_in_tex_is_refused() {
        let doc = "hapto: 2\nalt: a\nvalues: [ { name: s, type: number, range: [0, 2], default: 1 } ]\nview: { x: { range: [0, 1] }, y: { range: [0, 1] } }\nmarks: [ { math: 'x = {s:.2f}', at: '[0.5, 0.5]' } ]";
        let e = Figure::from_text(doc).err().unwrap();
        assert_eq!(e[0].code.0, "E107");
    }
}
