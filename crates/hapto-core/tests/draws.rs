//! The whole claim, end to end: state in, drawing out, and the point moves.
//!
//! `parabola.json` is a kinemo scene — axes, the curve y = x², and a dot on
//! it — exported with `scene.builder.to_json()`. hapto never advances its
//! time; it writes the state into the signals and takes the frame at rest.

use hapto_core::{Domain, Figure, Knob, Size, State, Value};

fn parabola() -> Figure {
    let json = include_str!("parabola.json");
    let mut figure = Figure::from_json(json).expect("the fixture is a kinemo scene");
    // The dot's horizontal position is what the reader moves.
    figure
        .bind_free("x")
        .expect("the scene is built around one free signal");
    figure
}

fn state_at(x: f64) -> State {
    let mut state = State::new();
    state.declare(Knob {
        name: "x".into(),
        domain: Domain::Number {
            low: -1.0,
            high: 5.0,
            step: None,
        },
        value: Value::number(x),
        label: String::new(),
    });
    state
}

/// Pixels that are not the background, which is what "something was drawn"
/// means without asserting on an exact image.
fn ink(image: &kinemo_render::raster::Image) -> usize {
    let background = image.rgba.first().copied().unwrap_or(0);
    image.rgba.iter().filter(|p| **p != background).count()
}

#[test]
fn a_figure_draws_from_state() {
    let figure = parabola();
    let image = figure
        .draw(&state_at(2.0), Size::default())
        .expect("the figure draws");

    assert_eq!(image.width, 960);
    assert_eq!(image.height, 540);
    // Axes, a curve and a dot: far more than a blank frame.
    assert!(
        ink(&image) > 5_000,
        "only {} pixels were drawn",
        ink(&image)
    );
}

#[test]
fn the_point_follows_the_value() {
    // The claim this library exists for: change the state, and the drawing
    // changes with it. No time passes between these two frames.
    let figure = parabola();
    let left = figure.draw(&state_at(0.5), Size::default()).unwrap();
    let right = figure.draw(&state_at(4.5), Size::default()).unwrap();

    assert_ne!(
        left.rgba, right.rgba,
        "the dot did not move when x went from 0.5 to 4.5"
    );
}

#[test]
fn the_same_state_draws_the_same_frame() {
    // A figure is a function of its state and nothing else: no clock, no
    // frame counter, no hidden cursor.
    let figure = parabola();
    let once = figure.draw(&state_at(3.0), Size::default()).unwrap();
    let twice = figure.draw(&state_at(3.0), Size::default()).unwrap();
    assert_eq!(once.rgba, twice.rgba);
}

#[test]
fn binding_to_something_that_is_not_there_says_so() {
    let json = include_str!("parabola.json");
    let mut figure = Figure::from_json(json).unwrap();

    let err = figure.bind("x", "nonesuch", "x").unwrap_err();
    assert!(err.contains("nonesuch"), "{err}");

    let err = figure.bind("x", "d", "nonesuch").unwrap_err();
    assert!(err.contains("nonesuch"), "{err}");
}

#[test]
fn drawing_without_the_value_says_which_one() {
    let figure = parabola();
    let err = figure.draw(&State::new(), Size::default()).unwrap_err();
    assert!(err.contains("\"x\""), "{err}");
}
