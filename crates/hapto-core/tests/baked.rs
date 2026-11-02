//! A baked document draws its maths in a build without typst — the browser's
//! build (SPEC-2.0 §10.4). `fixtures/maths.baked.json` is what
//! `hapto bake fixtures/maths.yaml -o fixtures/maths.baked.json` wrote.
//!
//! Baked layouts are registered for the whole process, so the unbaked case
//! lives in its own test binary (`unbaked.rs`).

use hapto_core::render::Figure;
use hapto_core::scene::{ink, Canvas};
use hapto_core::theme::Theme;

#[test]
fn a_baked_document_draws_its_maths() {
    let mut f = Figure::from_text(include_str!("fixtures/maths.baked.json"))
        .unwrap_or_else(|e| panic!("{e:?}"));
    let theme = Theme::light();
    let img = f.render(Canvas::new(400.0, 200.0, 1.0), &theme).unwrap();
    // A display fraction at 40 px and a line of text paint well over 1%.
    assert!(ink(&img, theme.bg()) > 0.01, "{}", ink(&img, theme.bg()));
}
