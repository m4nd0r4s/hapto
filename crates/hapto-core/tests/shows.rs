//! That a figure actually puts something on screen, and that moving a value
//! moves it.
//!
//! The tests in `draws.rs` compare one frame to another, which two blank
//! frames pass: every demo rendered empty while the suite stayed green,
//! because an object with no presence toggle is never present and kinemo
//! drew nothing. These tests look at the pixels instead.

use hapto_core::{demos, Figure, Size, Value};

const SIZE: Size = Size {
    width: 480,
    height: 270,
};

/// How many pixels differ from the background, which is the first one.
fn painted(image: &kinemo_render::raster::Image) -> usize {
    let (pixels, _) = image.rgba.as_chunks::<4>();
    let background = pixels[0];
    pixels.iter().filter(|pixel| **pixel != background).count()
}

fn drawn(figure: &Figure) -> usize {
    painted(&figure.draw_now(SIZE).unwrap())
}

#[test]
fn every_demo_draws_something() {
    // A tenth of a percent of the frame: far below any real figure, far
    // above the nothing a scene without presence produces.
    let floor = (SIZE.width * SIZE.height) as usize / 1000;

    for (name, figure) in [
        ("mass_spring", demos::mass_spring(2.0, 1.0)),
        ("columns", demos::columns(1.0, 3.0)),
        ("slope", demos::slope(3.0)),
        ("angle", demos::angle(0.9)),
    ] {
        let lit = drawn(&figure);
        assert!(lit > floor, "{name} drew {lit} pixels, which is blank");
    }
}

#[test]
fn pulling_the_mass_changes_the_picture() {
    let at_rest = demos::mass_spring(0.0, 1.0).draw_now(SIZE).unwrap();

    let mut pulled = demos::mass_spring(0.0, 1.0);
    pulled.set("x", Value::number(2.4)).unwrap();
    let stretched = pulled.draw_now(SIZE).unwrap();

    assert_ne!(
        at_rest.rgba, stretched.rgba,
        "the block and the spring ignored x"
    );
}

#[test]
fn a_value_set_through_state_reaches_the_drawing() {
    // Every demo's named values must actually drive it; a figure whose
    // signals are bound wrongly renders the same thing whatever is set.
    for (name, mut figure, value, to) in [
        ("columns", demos::columns(1.0, 3.0), "a", 3.0),
        ("slope", demos::slope(1.0), "x", 3.5),
        ("angle", demos::angle(0.3), "angle", 1.2),
    ] {
        let before = figure.draw_now(SIZE).unwrap();
        figure.set(value, Value::number(to)).unwrap();
        let after = figure.draw_now(SIZE).unwrap();
        assert_ne!(before.rgba, after.rgba, "{name} ignored \"{value}\"");
    }
}
