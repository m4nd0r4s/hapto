//! Figures for things a textbook actually teaches.
//!
//! `Build` draws shapes; these draw *ideas*. A mass on a spring is the
//! example every Hamiltonian chapter reaches for, and a reader learns more
//! from pulling the block than from reading that the energy is ½kx².
//!
//! Each one names its values, so a page or a sentence can set them by name.
//! The numbers that go beside a figure (an energy, a determinant) are the
//! page's to write: it knows them, because it set the values they come from.

use crate::build::{colour, weight, Build, Num, Style};
use crate::Figure;

/// A block on a spring against a wall.
///
/// Values: `x`, how far the block sits from rest, and `k`, the stiffness.
/// The spring stretches with the block because both read the same `x`.
///
/// What a reader finds by pulling it: the restoring force grows with the
/// pull, and the energy grows with its square — twice as far is four times
/// the energy, which is what ½kx² says.
pub fn mass_spring(x_at: f64, k_at: f64) -> Figure {
    let mut build = Build::new();
    let x = build.value("x", x_at);
    let k = build.value("k", k_at);

    let wall = -5.0;
    let floor = -1.6;
    let block_half = 0.55;

    build.line(
        (Num::Fixed(wall), Num::Fixed(floor)),
        (Num::Fixed(6.0), Num::Fixed(floor)),
        Style::stroke(colour::GREY, weight::LINE),
    );
    build.line(
        (Num::Fixed(wall), Num::Fixed(floor)),
        (Num::Fixed(wall), Num::Fixed(1.8)),
        Style::stroke(colour::GREY, weight::HEAVY),
    );

    // Rest is x = 0, so the block's centre is the value itself.
    let centre = x.clone();
    let left_face = centre.clone().minus(block_half);

    build.spring(
        (Num::Fixed(wall), Num::Fixed(0.0)),
        (left_face, Num::Fixed(0.0)),
        9,
        0.34,
        Style::stroke(colour::GREY, weight::LINE),
    );

    build.rect(
        centre.clone(),
        Num::Fixed(0.0),
        Num::Fixed(block_half * 2.0),
        Num::Fixed(1.5),
        Style::fill(colour::BLUE).and_opacity(0.85),
    );

    // The rest position, so "how far from rest" is seen rather than told.
    build.line(
        (Num::Fixed(0.0), Num::Fixed(floor)),
        (Num::Fixed(0.0), Num::Fixed(1.4)),
        Style::stroke(colour::GREY, weight::HAIR),
    );

    // The pull, as an arrow from rest to the block.
    build.arrow(
        (Num::Fixed(0.0), Num::Fixed(-1.05)),
        (centre.clone(), Num::Fixed(-1.05)),
        Style::stroke(colour::RED, weight::HEAVY).and_fill(colour::RED),
    );

    // The restoring force, -kx: an arrow from the block, pointing back. It
    // reads the same x and k as everything else, so it cannot disagree.
    let force = k.times(x).negated();
    build.arrow(
        (centre.clone(), Num::Fixed(1.1)),
        (centre.plus(force.times(0.6)), Num::Fixed(1.1)),
        Style::stroke(colour::GREEN, weight::HEAVY).and_fill(colour::GREEN),
    );

    Figure::built(build.finish())
}

/// Two column vectors and the parallelogram they span.
///
/// Values: `a` and `b`, the second column's entries; the first is fixed at
/// (3, 1). The area of the parallelogram is the determinant, and watching it
/// flatten is what "these columns are not independent" looks like.
pub fn columns(a_at: f64, b_at: f64) -> Figure {
    let mut build = Build::new();
    let a = build.value("a", a_at);
    let b = build.value("b", b_at);

    let first = (Num::Fixed(3.0), Num::Fixed(1.0));
    let origin = (Num::Fixed(0.0), Num::Fixed(0.0));

    build.line(
        (Num::Fixed(-1.0), Num::Fixed(0.0)),
        (Num::Fixed(7.0), Num::Fixed(0.0)),
        Style::stroke(colour::GREY, weight::HAIR),
    );
    build.line(
        (Num::Fixed(0.0), Num::Fixed(-1.0)),
        (Num::Fixed(0.0), Num::Fixed(5.0)),
        Style::stroke(colour::GREY, weight::HAIR),
    );

    // The far corner is the sum of the two columns.
    let sum = (
        a.clone().plus(first.0.clone()),
        b.clone().plus(first.1.clone()),
    );
    build.quad(
        [origin.clone(), first.clone(), sum, (a.clone(), b.clone())],
        Style::stroke(colour::YELLOW, weight::HAIR),
    );

    build.arrow(
        origin.clone(),
        first,
        Style::stroke(colour::BLUE, weight::HEAVY).and_fill(colour::BLUE),
    );
    build.arrow(
        origin,
        (a, b),
        Style::stroke(colour::RED, weight::HEAVY).and_fill(colour::RED),
    );

    Figure::built(build.finish())
}

/// A curve with a point on it and the tangent through it.
///
/// Values: `x`. The tangent's slope is the derivative of the same curve the
/// point sits on, so the line a reader sees is the number they are told.
pub fn slope(x_at: f64) -> Figure {
    let mut build = Build::new();
    let x = build.value("x", x_at);

    // y = x²/4, flat enough to read across the frame.
    let scale = 0.25;
    let y = x.clone().squared().times(scale);
    let slope = x.clone().times(2.0 * scale);

    build.line(
        (Num::Fixed(-6.0), Num::Fixed(0.0)),
        (Num::Fixed(6.0), Num::Fixed(0.0)),
        Style::stroke(colour::GREY, weight::HAIR),
    );
    build.line(
        (Num::Fixed(0.0), Num::Fixed(-0.6)),
        (Num::Fixed(0.0), Num::Fixed(4.0)),
        Style::stroke(colour::GREY, weight::HAIR),
    );

    // The curve itself does not move, so its points are fixed.
    let curve: Vec<(Num, Num)> = (-24..=24)
        .map(|i| {
            let t = i as f64 / 4.0;
            (Num::Fixed(t), Num::Fixed(t * t * scale))
        })
        .collect();
    let _ = build.polyline(curve, Style::stroke(colour::YELLOW, weight::LINE));

    // The tangent, as a segment either side of the point.
    let reach = 1.6;
    build.line(
        (
            x.clone().minus(reach),
            y.clone().minus(slope.clone().times(reach)),
        ),
        (x.clone().plus(reach), y.clone().plus(slope.times(reach))),
        Style::stroke(colour::GREEN, weight::HEAVY),
    );

    build.dot(x, y, 0.13, Style::fill(colour::RED));

    Figure::built(build.finish())
}

/// A ray at an angle, with its components dropped to the axes.
///
/// Values: `angle`, in radians.
pub fn angle(at: f64) -> Figure {
    let mut build = Build::new();
    let angle = build.value("angle", at);
    let reach = 3.0;

    build.line(
        (Num::Fixed(-4.0), Num::Fixed(0.0)),
        (Num::Fixed(4.0), Num::Fixed(0.0)),
        Style::stroke(colour::GREY, weight::HAIR),
    );
    build.line(
        (Num::Fixed(0.0), Num::Fixed(-3.0)),
        (Num::Fixed(0.0), Num::Fixed(3.0)),
        Style::stroke(colour::GREY, weight::HAIR),
    );

    let tip_x = angle.clone().cos().times(reach);
    let tip_y = angle.sin().times(reach);

    // The components, down to the axis and across to it.
    build.line(
        (tip_x.clone(), tip_y.clone()),
        (tip_x.clone(), Num::Fixed(0.0)),
        Style::stroke(colour::GREY, weight::HAIR),
    );
    build.line(
        (tip_x.clone(), tip_y.clone()),
        (Num::Fixed(0.0), tip_y.clone()),
        Style::stroke(colour::GREY, weight::HAIR),
    );
    build.arrow(
        (Num::Fixed(0.0), Num::Fixed(0.0)),
        (tip_x, tip_y),
        Style::stroke(colour::BLUE, weight::HEAVY).and_fill(colour::BLUE),
    );

    Figure::built(build.finish())
}
