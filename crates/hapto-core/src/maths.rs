//! TeX in figures: typeset ahead of time, baked, scaled when drawn
//! (SPEC-2.0 §10.4, docs/m0.md).
//!
//! typst compiles to wasm but costs 10 MB in the browser, so it never ships
//! there. A build with the `typeset` feature lays formulas out natively and
//! writes them into the document as baked layouts; the browser registers
//! those and draws them. Layouts are keyed by size, and a figure's text size
//! changes with its width, so everything is laid out at size 1 and scaled
//! at draw time — otherwise every resize would miss the baked layouts.

use hapto_expr::Error;
use kinemo_math::{layout_math, MathError, MathOptions};

use crate::codes;

/// The one size every formula is laid out at.
pub const SIZE: f64 = 1.0;

pub fn options(display: bool) -> MathOptions {
    MathOptions {
        size: SIZE,
        display,
    }
}

/// Whether this build can draw `tex`: typeset now, or baked earlier.
pub fn check(tex: &str, display: bool) -> Result<(), Error> {
    match layout_math(tex, &options(display)) {
        Ok(_) => Ok(()),
        Err(MathError::NotBaked(_)) => Err(Error::new(
            codes::E608,
            format!("the formula \"{tex}\" was not typeset ahead of time"),
        )
        .hint("run `hapto bake` on this figure, or build the book with `hapto book build`")),
        Err(MathError::Unsupported { command }) => Err(Error::new(
            codes::E606,
            format!("`{command}` is not a TeX command hapto can typeset"),
        )
        .hint("see the supported commands in SPEC-2.0 §10.4")),
        Err(other) => Err(
            Error::new(codes::E606, format!("this TeX cannot be typeset: {other}"))
                .hint("check the braces and arguments"),
        ),
    }
}

/// Lay every formula out natively and return the baked layouts as JSON, to
/// travel with the document.
#[cfg(feature = "typeset")]
pub fn bake(formulas: &[(String, bool)]) -> Result<String, Error> {
    kinemo_math::baked::start_recording();
    for (tex, display) in formulas {
        check(tex, *display)?;
    }
    Ok(kinemo_math::baked::recorded())
}

/// Make baked layouts available to this process (the browser does this when
/// it loads a document).
pub fn register(json: &str) -> Result<(), Error> {
    kinemo_math::baked::register(json).map_err(|e| {
        Error::new(
            codes::E403,
            format!("the baked layouts cannot be read: {e}"),
        )
        .at_path("/baked")
    })
}

#[cfg(all(test, not(feature = "typeset")))]
mod without_typst {
    use super::*;

    #[test]
    fn an_unbaked_formula_is_refused_not_drawn_blank() {
        let e = check(r"\frac{a}{b} + \unbaked", true).unwrap_err();
        assert_eq!(e.code, codes::E608);
        assert!(e.hint.unwrap().contains("hapto bake"));
    }
}

#[cfg(all(test, feature = "typeset"))]
mod with_typst {
    use super::*;

    #[test]
    fn baking_records_the_layouts() {
        let json = bake(&[
            (r"\frac{\partial H}{\partial q}".into(), true),
            ("x^2".into(), false),
        ])
        .unwrap();
        assert!(json.contains("partial"), "{json}");
        register(&json).unwrap();
    }

    #[test]
    fn unsupported_commands_are_named() {
        let e = check(r"\notacommand{x}", true).unwrap_err();
        assert_eq!(e.code, codes::E606);
    }
}
