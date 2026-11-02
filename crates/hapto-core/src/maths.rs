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
use crate::document::{Arg, Document, Kind, Mark, Scale, Ticks};

/// One TeX string a document can show: the TeX, display or inline, and the
/// JSON Pointer of where it was written.
pub type Found = (String, bool, String);

/// The `$…$` pieces of a text.
fn inline(text: &str, path: &str, out: &mut Vec<Found>) {
    for (i, part) in text.split('$').enumerate() {
        if i % 2 == 1 && !part.trim().is_empty() {
            out.push((part.to_string(), false, path.to_string()));
        }
    }
}

fn walk_arg(a: &Arg, path: &str, out: &mut Vec<Found>) {
    match a {
        Arg::Text(t) => inline(t, path, out),
        Arg::Map(entries) => entries
            .iter()
            .for_each(|(k, v)| walk_arg(v, &crate::tree::child_path(path, k), out)),
        Arg::List(items) => items
            .iter()
            .enumerate()
            .for_each(|(i, v)| walk_arg(v, &crate::tree::index_path(path, i), out)),
        Arg::Marks(marks) => marks.iter().for_each(|m| walk_mark(m, out)),
        _ => {}
    }
}

fn walk_mark(m: &Mark, out: &mut Vec<Found>) {
    let main = crate::tree::child_path(&m.path, m.kind);
    match (&m.main, m.kind) {
        (Arg::Text(t), "math") => out.push((t.clone(), true, main)),
        (Arg::Text(_), "path") => {}
        (a, _) => walk_arg(a, &main, out),
    }
    for (k, a) in &m.options {
        walk_arg(a, &crate::tree::child_path(&m.path, k), out);
    }
    if let Some(l) = &m.common.label {
        inline(&l.text, &crate::tree::child_path(&m.path, "label"), out);
    }
    if let Some(l) = &m.common.legend {
        inline(l, &crate::tree::child_path(&m.path, "legend"), out);
    }
}

/// Every TeX string the document can show, generated tick labels included.
pub fn formulas(doc: &Document) -> Vec<Found> {
    let mut out = Vec::new();
    for v in &doc.values {
        if let Some(l) = &v.label {
            inline(l, &crate::tree::child_path(&v.path, "label"), &mut out);
        }
        if let Kind::Choice { labels, .. } = &v.kind {
            for (_, l) in labels {
                inline(l, &crate::tree::child_path(&v.path, "labels"), &mut out);
            }
        }
    }
    for view in &doc.views {
        if let Some(t) = &view.title {
            inline(t, &crate::tree::child_path(&view.path, "title"), &mut out);
        }
        for (name, axis) in [("x", &view.x), ("y", &view.y)] {
            let path = crate::tree::child_path(&view.path, name);
            if let Some(l) = &axis.label {
                inline(l, &crate::tree::child_path(&path, "label"), &mut out);
            }
            match &axis.ticks {
                Ticks::Labelled(pairs) => pairs.iter().for_each(|(_, l)| {
                    inline(l, &crate::tree::child_path(&path, "ticks"), &mut out)
                }),
                Ticks::Pi => crate::ticks::every_pi_label()
                    .iter()
                    .for_each(|l| inline(l, &path, &mut out)),
                Ticks::Auto if axis.scale == Scale::Log => (-12..=12)
                    .map(|k| crate::ticks::decade(10f64.powi(k)))
                    .for_each(|l| inline(&l, &path, &mut out)),
                _ => {}
            }
        }
    }
    for m in &doc.marks {
        walk_mark(m, &mut out);
    }
    for r in &doc.readouts {
        if let Some(l) = &r.label {
            inline(l, &crate::tree::child_path(&r.path, "label"), &mut out);
        }
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|(t, d, _)| seen.insert((t.clone(), *d)));
    out
}

/// Refuse a document whose TeX this build cannot draw: E608 when it was not
/// baked (the browser), E606 when it is not TeX hapto can typeset.
pub fn check_document(doc: &Document) -> Vec<Error> {
    formulas(doc)
        .into_iter()
        .filter_map(|(tex, display, path)| check(&tex, display).err().map(|e| e.at_path(&path)))
        .collect()
}

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

/// A document with every formula it can show typeset and baked into it, as
/// JSON (§10.4, §17 `hapto bake`).
#[cfg(feature = "typeset")]
pub fn bake_document(text: &str) -> Result<String, Vec<Error>> {
    let doc = crate::document::read(text)?;
    let found: Vec<(String, bool)> = formulas(&doc).into_iter().map(|(t, d, _)| (t, d)).collect();
    let baked = bake(&found).map_err(|e| vec![e])?;
    let mut tree = crate::tree::parse(text).map_err(|e| vec![e])?;
    if let serde_json::Value::Object(map) = &mut tree {
        map.insert("baked".into(), serde_json::Value::String(baked));
    }
    Ok(serde_json::to_string_pretty(&tree).expect("a tree is JSON"))
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
