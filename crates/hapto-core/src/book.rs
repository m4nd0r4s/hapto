//! Figures a lesson asks for by writing one down.
//!
//! `demos.rs` holds figures with the mathematics baked in — a mass on a
//! spring is always a mass on a spring. A book is the other way round: the
//! chapter carries a curve as a string and expects a figure of it, and the
//! next chapter carries a different one.
//!
//! The two shapes a book needs, both already in the lessons:
//!
//! - a **plot**: a curve with a point riding it, and the tangent when the
//!   chapter is about a slope;
//! - **columns**: two vectors and the parallelogram they span, where the
//!   area going to zero is the determinant going to zero.
//!
//! Both take their mathematics as written formulas, so the figure and the
//! number printed beside it come from the same source and cannot disagree.

use std::collections::BTreeMap;

use crate::build::{colour, weight, Build, Num, Style};
use crate::formula::parse;
use crate::Figure;

/// Where a figure's axes start and stop, in the figure's own units.
#[derive(Debug, Clone, Copy)]
pub struct Range {
    pub low: f64,
    pub high: f64,
}

impl Range {
    pub fn new(low: f64, high: f64) -> Self {
        Range { low, high }
    }

    fn span(&self) -> f64 {
        (self.high - self.low).abs().max(1e-9)
    }
}

/// How the figure's own units map onto the frame kinemo draws in.
///
/// kinemo's frame is 16 by 9 with the origin at the centre. A lesson's curve
/// might run x from -2 to 6 and y from -7 to 3, so everything is scaled and
/// shifted onto the frame once, here, rather than at every call site.
#[derive(Debug, Clone, Copy)]
struct Frame {
    x: Range,
    y: Range,
}

/// The drawable area, inside kinemo's 16x9 frame, leaving room at the edges.
const HALF_W: f64 = 6.6;
const HALF_H: f64 = 3.5;

impl Frame {
    /// A fixed number in figure units, placed on the frame.
    fn at_x(&self, value: f64) -> Num {
        Num::Fixed(self.map_x(value))
    }

    fn at_y(&self, value: f64) -> Num {
        Num::Fixed(self.map_y(value))
    }

    fn map_x(&self, value: f64) -> f64 {
        (value - self.x.low) / self.x.span() * (2.0 * HALF_W) - HALF_W
    }

    fn map_y(&self, value: f64) -> f64 {
        (value - self.y.low) / self.y.span() * (2.0 * HALF_H) - HALF_H
    }

    /// An expression in figure units, placed on the frame. The shift and the
    /// scale travel with the expression, so a curve that follows a value is
    /// mapped exactly as a fixed point would be.
    fn on_x(&self, value: Num) -> Num {
        value
            .minus(self.x.low)
            .times(2.0 * HALF_W / self.x.span())
            .minus(HALF_W)
    }

    fn on_y(&self, value: Num) -> Num {
        value
            .minus(self.y.low)
            .times(2.0 * HALF_H / self.y.span())
            .minus(HALF_H)
    }

    /// Where zero sits, pinned into view when the range does not contain it.
    fn zero_x(&self) -> f64 {
        0.0_f64.clamp(self.x.low, self.x.high)
    }

    fn zero_y(&self) -> f64 {
        0.0_f64.clamp(self.y.low, self.y.high)
    }

    /// Whether a figure-unit value is inside the drawn area.
    fn holds_y(&self, value: f64) -> bool {
        value >= self.y.low && value <= self.y.high
    }
}

/// Axes through zero where zero is in view, at the edge where it is not.
fn axes(build: &mut Build, frame: &Frame) {
    let hair = Style::stroke(colour::GREY, weight::HAIR);
    build.line(
        (Num::Fixed(-HALF_W), frame.at_y(frame.zero_y())),
        (Num::Fixed(HALF_W), frame.at_y(frame.zero_y())),
        hair.clone(),
    );
    build.line(
        (frame.at_x(frame.zero_x()), Num::Fixed(-HALF_H)),
        (frame.at_x(frame.zero_x()), Num::Fixed(HALF_H)),
        hair,
    );
}

/// What a formula works out to with one value at one number.
fn value_at(curve: &str, variable: &str, x: f64) -> Result<f64, String> {
    let mut fixed = BTreeMap::new();
    fixed.insert(variable.to_string(), Num::Fixed(x));
    let n = parse(curve, &fixed)?;
    Ok(Build::new().probe(n))
}

/// A curve with a point riding it, and optionally the tangent through it.
///
/// `curve` is written over `variable`, which is the value the reader moves:
/// `plot("x**2 - x*(2)", "x", ...)` is a real lesson's parabola. The point
/// sits at `(v, curve(v))` because both read the same value, so it cannot
/// drift off the curve.
///
/// The tangent is a secant over a vanishing step rather than a derivative:
/// that is the derivative in the limit, it needs no symbolic algebra here,
/// and it is drawn from the same formula as the curve, so the line a reader
/// sees cannot disagree with the curve it touches.
pub fn plot(
    curve: &str,
    variable: &str,
    at: f64,
    knob: Range,
    x_range: Range,
    y_range: Range,
    tangent: bool,
) -> Result<Figure, String> {
    let frame = Frame {
        x: x_range,
        y: y_range,
    };
    let mut build = Build::new();
    let value = build.value(variable, at);

    let mut values = BTreeMap::new();
    values.insert(variable.to_string(), value.clone());

    // Refuse early and clearly: a curve that does not parse would otherwise
    // become a figure that silently draws nothing.
    let at_value = parse(curve, &values)?;

    axes(&mut build, &frame);

    // The curve itself does not move, so it is sampled once as fixed points.
    let samples = 160;
    let mut points: Vec<(Num, Num)> = Vec::with_capacity(samples + 1);
    let mut inside = 0;
    for i in 0..=samples {
        let x = x_range.low + x_range.span() * i as f64 / samples as f64;
        let y = value_at(curve, variable, x)?;
        if !y.is_finite() {
            continue;
        }
        if frame.holds_y(y) {
            inside += 1;
        }
        // Clamped rather than dropped: a curve that leaves the top of the
        // frame should run up to the edge, not stop short of it.
        points.push((
            frame.at_x(x),
            frame.at_y(y.clamp(y_range.low, y_range.high)),
        ));
    }
    if inside == 0 {
        return Err(format!(
            "the curve \"{curve}\" is nowhere inside the range this figure draws \
             (y from {} to {}), so it would be an empty picture",
            y_range.low, y_range.high
        ));
    }
    build.polyline(points, Style::stroke(colour::YELLOW, weight::LINE))?;

    let point_x = frame.on_x(value.clone());
    let point_y = frame.on_y(at_value.clone());

    if tangent {
        let step = x_range.span() / 400.0;
        let mut ahead = BTreeMap::new();
        ahead.insert(variable.to_string(), value.clone().plus(step));
        let mut behind = BTreeMap::new();
        behind.insert(variable.to_string(), value.clone().minus(step));

        let slope = parse(curve, &ahead)?
            .minus(parse(curve, &behind)?)
            .over(2.0 * step);

        // Long enough to read as a line, in figure units either side.
        let reach = x_range.span() / 6.0;
        build.line(
            (
                frame.on_x(value.clone().minus(reach)),
                frame.on_y(at_value.clone().minus(slope.clone().times(reach))),
            ),
            (
                frame.on_x(value.clone().plus(reach)),
                frame.on_y(at_value.clone().plus(slope.times(reach))),
            ),
            Style::stroke(colour::GREEN, weight::HEAVY),
        );
    }

    // A dropped line to the axis, so the reader sees which x this is.
    build.line(
        (point_x.clone(), point_y.clone()),
        (point_x.clone(), frame.at_y(frame.zero_y())),
        Style::stroke(colour::GREY, weight::HAIR),
    );
    build.dot(point_x, point_y, 0.14, Style::fill(colour::RED));

    let mut figure = Figure::built(build.finish());
    figure.bound(variable, knob.low, knob.high)?;
    Ok(figure)
}

/// Two column vectors and the parallelogram they span.
///
/// Each column is a pair of written formulas, so a lesson's
/// `first: ["2", "1"], second: ["x", "2"]` becomes a figure where moving `x`
/// shears the second column and flattens the area. The area *is* the
/// determinant: watching it vanish is the lesson.
///
/// `knob` is how far the reader may move the value, which is not the same as
/// the axes: the figure is drawn over the area the columns reach, while the
/// lesson decides what the value itself may be.
pub fn columns(
    first: (&str, &str),
    second: (&str, &str),
    variable: &str,
    at: f64,
    knob: Range,
    x_range: Range,
    y_range: Range,
) -> Result<Figure, String> {
    let frame = Frame {
        x: x_range,
        y: y_range,
    };
    let mut build = Build::new();
    let value = build.value(variable, at);

    let mut values = BTreeMap::new();
    values.insert(variable.to_string(), value);

    let a = (parse(first.0, &values)?, parse(first.1, &values)?);
    let b = (parse(second.0, &values)?, parse(second.1, &values)?);

    axes(&mut build, &frame);

    let origin = (frame.at_x(0.0), frame.at_y(0.0));
    let on = |p: &(Num, Num)| (frame.on_x(p.0.clone()), frame.on_y(p.1.clone()));
    // The far corner is the sum of the two columns.
    let sum = (a.0.clone().plus(b.0.clone()), a.1.clone().plus(b.1.clone()));

    build.quad(
        [origin.clone(), on(&a), on(&sum), on(&b)],
        Style::stroke(colour::YELLOW, weight::HAIR),
    );
    build.arrow(
        origin.clone(),
        on(&a),
        Style::stroke(colour::BLUE, weight::HEAVY).and_fill(colour::BLUE),
    );
    build.arrow(
        origin,
        on(&b),
        Style::stroke(colour::RED, weight::HEAVY).and_fill(colour::RED),
    );

    let mut figure = Figure::built(build.finish());
    figure.bound(variable, knob.low, knob.high)?;
    Ok(figure)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Size, Value};

    const SIZE: Size = Size {
        width: 320,
        height: 180,
    };

    fn painted(image: &kinemo_render::raster::Image) -> usize {
        let (pixels, _) = image.rgba.as_chunks::<4>();
        let background = pixels[0];
        pixels.iter().filter(|p| **p != background).count()
    }

    /// The curve and ranges of lessons/calc-partial-derivatives.
    fn parabola(tangent: bool) -> Figure {
        plot(
            "x**2 - x*(2)",
            "x",
            1.0,
            Range::new(-2.0, 4.0),
            Range::new(-2.0, 4.0),
            Range::new(-2.35, 9.35),
            tangent,
        )
        .unwrap()
    }

    #[test]
    fn a_plot_draws_and_follows_its_value() {
        let mut figure = parabola(true);
        let before = figure.draw_now(SIZE).unwrap();
        assert!(painted(&before) > 100, "the plot is blank");

        figure.set("x", Value::number(3.0)).unwrap();
        let after = figure.draw_now(SIZE).unwrap();
        assert_ne!(before.rgba, after.rgba, "the plot ignored x");
    }

    #[test]
    fn the_tangent_is_drawn_only_when_it_is_asked_for() {
        let with = parabola(true).draw_now(SIZE).unwrap();
        let without = parabola(false).draw_now(SIZE).unwrap();
        assert!(
            painted(&with) > painted(&without),
            "asking for a tangent drew no more than not asking"
        );
    }

    #[test]
    fn the_tangent_follows_the_slope_of_the_curve() {
        // At the vertex of x**2 - 2x (x = 1) the tangent is flat; at x = 3 it
        // climbs. A figure where the line never tilts would pass the test
        // above and still be wrong.
        let flat = parabola(true).draw_now(SIZE).unwrap();
        let mut tilted = parabola(true);
        tilted.set("x", Value::number(3.0)).unwrap();
        assert_ne!(flat.rgba, tilted.draw_now(SIZE).unwrap().rgba);
    }

    #[test]
    fn columns_flatten_when_the_determinant_does() {
        // lessons/la-matrix-theory: first (2,1), second (x,2), singular at x=4.
        let mut figure = columns(
            ("2", "1"),
            ("x", "2"),
            "x",
            1.0,
            Range::new(-2.0, 6.0),
            Range::new(-1.0, 9.0),
            Range::new(-1.0, 3.0),
        )
        .unwrap();
        let open = figure.draw_now(SIZE).unwrap();

        figure.set("x", Value::number(4.0)).unwrap();
        let flat = figure.draw_now(SIZE).unwrap();

        assert_ne!(open.rgba, flat.rgba, "the columns ignored x");
        // At x = 4 the columns are parallel: the area collapses, so less ink.
        assert!(
            painted(&flat) < painted(&open),
            "the parallelogram did not flatten at the singular value"
        );
    }

    #[test]
    fn a_curve_that_does_not_parse_is_refused_with_a_reason() {
        let err = plot(
            "x^2",
            "x",
            1.0,
            Range::new(-2.0, 2.0),
            Range::new(-2.0, 2.0),
            Range::new(-2.0, 2.0),
            false,
        )
        .unwrap_err();
        assert!(err.contains("**"), "{err}");
    }

    #[test]
    fn a_curve_outside_the_frame_is_refused_rather_than_drawn_empty() {
        let err = plot(
            "x + 100",
            "x",
            0.0,
            Range::new(-1.0, 1.0),
            Range::new(-1.0, 1.0),
            Range::new(-1.0, 1.0),
            false,
        )
        .unwrap_err();
        assert!(err.contains("empty picture"), "{err}");
    }

    /// The axes and the knob are different things: a matrix figure is drawn
    /// over the area its columns reach, while the lesson says how far the
    /// value may go. Binding the value to the axes let a reader set it
    /// outside what the lesson intended.
    #[test]
    fn the_value_is_bounded_by_the_knob_not_by_the_axes() {
        let figure = parabola(false);
        let knob = figure.state().knobs().find(|k| k.name == "x").unwrap();
        match knob.domain {
            crate::Domain::Number { low, high, .. } => assert_eq!((low, high), (-2.0, 4.0)),
            ref other => panic!("x should be a number, and is {other:?}"),
        }

        // Columns: the knob is -2..6 while the axes run -1..9.
        let figure = columns(
            ("2", "1"),
            ("x", "2"),
            "x",
            1.0,
            Range::new(-2.0, 6.0),
            Range::new(-1.0, 9.0),
            Range::new(-1.0, 3.0),
        )
        .unwrap();
        let knob = figure.state().knobs().find(|k| k.name == "x").unwrap();
        match knob.domain {
            crate::Domain::Number { low, high, .. } => assert_eq!((low, high), (-2.0, 6.0)),
            ref other => panic!("x should be a number, and is {other:?}"),
        }
    }
}
