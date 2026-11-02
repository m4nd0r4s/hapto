//! Rectangles and boxes (SPEC-2.0 §8.4).

use hapto_expr::{Error, Value};

use super::text::{put, FLOOR};
use super::Ctx;
use crate::geom::Rect;
use crate::scene::Builder;

pub fn draws(kind: &str) -> bool {
    matches!(kind, "rect" | "box")
}

/// Box text, CSS px.
const TEXT: f64 = 14.0;

pub fn draw(c: &mut Ctx) -> Result<(), Error> {
    match c.m.source.kind {
        "rect" => rect(c),
        _ => boxed(c),
    }
}

fn rect(c: &mut Ctx) -> Result<(), Error> {
    // A lower-left corner with `w` and `h`, or two corners.
    let (a, b) = match c.main()? {
        Value::Vec(p) => {
            let w = c.opt_num("w", f64::NAN)?;
            let h = c.opt_num("h", f64::NAN)?;
            (p, [p[0] + w, p[1] + h])
        }
        _ => match (c.main_key("from")?, c.main_key("to")?) {
            (Some(f), Some(t)) => match (f.vec(), t.vec()) {
                (Some(f), Some(t)) => (f, t),
                _ => return Ok(()),
            },
            _ => return Ok(()),
        },
    };
    if ![a[0], a[1], b[0], b[1]].iter().all(|v| v.is_finite()) {
        return Ok(());
    }
    let corners = [a, [b[0], a[1]], b, [a[0], b[1]]].map(|p| c.css(p));
    let radius = c.opt_num("radius", 0.0)?;
    shape(c, &rounded(&corners, radius));
    Ok(())
}

fn boxed(c: &mut Ctx) -> Result<(), Error> {
    let Some(centre) = c.main()?.vec() else {
        return Ok(());
    };
    if !(centre[0].is_finite() && centre[1].is_finite()) {
        return Ok(());
    }
    let text = c.template("text")?.filter(|t| !t.is_empty());
    let padding = c.opt_num("padding", 8.0)?.max(0.0);
    let radius = c.opt_num("radius", 6.0)?;
    let size = TEXT.max(FLOOR);
    let w = c.opt("w")?.map(|v| v.num()).filter(|v| v.is_finite());
    let h = c.opt("h")?.map(|v| v.num()).filter(|v| v.is_finite());
    let corners = match (w, h, &text) {
        (Some(w), Some(h), _) => {
            let (x, y) = (w / 2.0, h / 2.0);
            [[-x, -y], [x, -y], [x, y], [-x, y]]
                .map(|d| c.css([centre[0] + d[0], centre[1] + d[1]]))
        }
        // Without both sides, a box sizes to its text (§8.4).
        (_, _, Some(t)) => {
            let [tw, th] = Builder::measure(t, size, None);
            let o = c.css(centre);
            let (x, y) = (tw / 2.0 + padding, th / 2.0 + padding);
            [[-x, y], [x, y], [x, -y], [-x, -y]].map(|d| [o[0] + d[0], o[1] + d[1]])
        }
        _ => return Ok(()),
    };
    shape(c, &rounded(&corners, radius));
    if let Some(t) = text {
        let at = c.css(centre);
        let ink = c.palette("fg");
        let bg = c.paint.fill.unwrap_or(c.palette("bg").unwrap_or([1.0; 4]));
        put(c.b, &t, at, size, 0.0, None, ink, bg, false, c.z + 0.01);
        let [tw, th] = Builder::measure(&t, size, None);
        c.b.labels.push(Rect {
            left: at[0] - tw / 2.0,
            top: at[1] - th / 2.0,
            right: at[0] + tw / 2.0,
            bottom: at[1] + th / 2.0,
        });
    }
    Ok(())
}

/// Fill and stroke a closed outline in CSS px, as the paint asks.
fn shape(c: &mut Ctx, pts: &[[f64; 2]]) {
    if c.paint.fill.is_some() && c.paint.fill_opacity > 0.0 {
        c.fill(pts);
    }
    if c.paint.stroke.is_some() {
        c.stroke(pts, true);
    }
}

/// A quadrilateral with its corners rounded by `radius` CSS px (never more
/// than half the shorter side).
fn rounded(corners: &[[f64; 2]; 4], radius: f64) -> Vec<[f64; 2]> {
    let side = |i: usize| {
        let (p, q) = (corners[i], corners[(i + 1) % 4]);
        (q[0] - p[0]).hypot(q[1] - p[1])
    };
    let shortest = (0..4).map(side).fold(f64::INFINITY, f64::min);
    let r = radius.min(shortest / 2.0);
    if r.is_nan() || r <= 0.5 {
        return corners.to_vec();
    }
    let mut out = Vec::with_capacity(4 * 9);
    for i in 0..4 {
        let (prev, p, next) = (corners[(i + 3) % 4], corners[i], corners[(i + 1) % 4]);
        let toward = |q: [f64; 2]| {
            let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
            let n = dx.hypot(dy).max(1e-9);
            [p[0] + dx / n * r, p[1] + dy / n * r]
        };
        let (a, b) = (toward(prev), toward(next));
        // A quadratic Bézier from `a` to `b` with its control at the corner.
        for k in 0..=8 {
            let t = k as f64 / 8.0;
            let u = 1.0 - t;
            out.push([
                u * u * a[0] + 2.0 * u * t * p[0] + t * t * b[0],
                u * u * a[1] + 2.0 * u * t * p[1] + t * t * b[1],
            ]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::render::Figure;
    use crate::scene::{ink, Canvas};
    use crate::theme::Theme;

    fn render(marks: &str) -> f64 {
        let doc = format!("hapto: 2\nalt: a\nvalues: [ {{ name: a, type: number, range: [0, 3], default: 0.5 }} ]\nview: {{ x: {{ range: [-4, 4] }}, y: {{ range: [-2, 2] }}, aspect: equal, axes: none }}\nmarks: [ {marks} ]");
        let mut f = Figure::from_text(&doc).unwrap_or_else(|e| panic!("{marks}: {e:?}"));
        let img = f
            .render(Canvas::new(400.0, 200.0, 1.0), &Theme::light())
            .unwrap();
        ink(&img, Theme::light().bg())
    }

    #[test]
    fn rects_and_boxes_draw() {
        for mark in [
            "{ rect: '[-1, -0.5]', w: 2, h: 1 }",
            "{ rect: { from: '[-1, -1]', to: '[1, 1]' }, radius: 8, style: { fill: accent } }",
            "{ box: '[0, 0]', text: 'plant {a:.1f}' }",
            "{ box: '[0, 0]', w: 3, h: 1, text: 'H' }",
        ] {
            assert!(render(mark) > 0.004, "{mark} drew too little");
        }
    }

    #[test]
    fn a_box_without_sides_or_text_draws_nothing() {
        assert_eq!(render("{ box: '[0, 0]' }"), 0.0);
    }

    #[test]
    fn rounding_keeps_the_corners_inside() {
        let sq = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let r = super::rounded(&sq, 3.0);
        assert!(r.len() > 4);
        assert!(r
            .iter()
            .all(|p| (0.0..=10.0).contains(&p[0]) && (0.0..=10.0).contains(&p[1])));
        assert_eq!(super::rounded(&sq, 0.0).len(), 4);
    }
}
