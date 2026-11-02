//! `hapto check`: everything that can be known about a figure without a
//! reader (SPEC-2.0 §17, §21.1).
//!
//! Load it (parse, structure, types, TeX), render it at its default state in
//! both themes and at the ends of every value's range, and report: a figure
//! that paints too little at rest is an error (E601); a value whose ends draw
//! the same picture is a defect worth a warning (W301), because a control
//! that changes nothing teaches nothing.

use std::time::Instant;

use hapto_expr::Error;
use kinemo_render::raster::Image;
use serde::Serialize;
use serde_json::Value as Tree;

use crate::codes;
use crate::document::Kind;
use crate::render::Figure;
use crate::scene::{ink, Canvas};
use crate::state::Input;
use crate::theme::Theme;

/// The least ink a figure paints at rest, as a fraction of its pixels (§21.1).
pub const MIN_INK: f64 = 0.005;

/// One render `check` made.
#[derive(Debug, Clone, Serialize)]
pub struct Rendered {
    /// `default`, or the value set, as `x=-1`.
    pub state: String,
    pub theme: String,
    /// Painted fraction of the pixels.
    pub ink: f64,
    pub ms: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub id: Option<String>,
    pub errors: Vec<Error>,
    pub warnings: Vec<Error>,
    pub renders: Vec<Rendered>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Values to set, as `x=3,mode=damped,p=[1, 2],show=true`. Commas inside
/// brackets belong to the value.
pub fn parse_set(text: &str) -> Result<Vec<(String, Input)>, String> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0i32, 0usize);
    for (i, ch) in text.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&text[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
        .into_iter()
        .filter(|p| !p.trim().is_empty())
        .map(|p| {
            let (name, value) = p
                .split_once('=')
                .ok_or_else(|| format!("\"{p}\" is not name=value"))?;
            let (name, value) = (name.trim().to_string(), value.trim());
            let input = if let Ok(n) = value.parse::<f64>() {
                Input::Number(n)
            } else if value == "true" || value == "false" {
                Input::Flag(value == "true")
            } else if let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
                let xy: Vec<f64> = inner
                    .split(',')
                    .map(|v| v.trim().parse::<f64>())
                    .collect::<Result<_, _>>()
                    .map_err(|_| format!("\"{value}\" is not a point [x, y]"))?;
                match xy.as_slice() {
                    [x, y] => Input::Point([*x, *y]),
                    _ => return Err(format!("\"{value}\" is not a point [x, y]")),
                }
            } else {
                Input::Choice(value.to_string())
            };
            Ok((name, input))
        })
        .collect()
}

/// Set several values, in order.
pub fn apply(figure: &mut Figure, set: &[(String, Input)]) -> Result<(), Error> {
    for (name, input) in set {
        figure.set(name, input.clone())?;
    }
    Ok(())
}

/// The ends of every value's domain: `(value, [low, high, …])`.
fn corners(figure: &Figure) -> Vec<(String, Vec<Input>)> {
    figure
        .program
        .doc
        .values
        .iter()
        .map(|v| {
            let ends = match &v.kind {
                Kind::Number { low, high, .. } => vec![Input::Number(*low), Input::Number(*high)],
                Kind::Choice { options, .. } => {
                    options.iter().cloned().map(Input::Choice).collect()
                }
                Kind::Point { low, high, .. } => vec![Input::Point(*low), Input::Point(*high)],
                Kind::Flag { .. } => vec![Input::Flag(false), Input::Flag(true)],
            };
            (v.name.clone(), ends)
        })
        .collect()
}

pub fn describe(name: &str, input: &Input) -> String {
    match input {
        Input::Number(n) => format!("{name}={n}"),
        Input::Choice(c) => format!("{name}={c}"),
        Input::Point([x, y]) => format!("{name}=[{x}, {y}]"),
        Input::Flag(b) => format!("{name}={b}"),
    }
}

/// Load, render and judge a figure document at `canvas`.
pub fn check(text: &str, canvas: Canvas) -> Report {
    let mut report = Report {
        id: None,
        errors: Vec::new(),
        warnings: Vec::new(),
        renders: Vec::new(),
    };
    let mut figure = match Figure::from_text(text) {
        Ok(f) => f,
        Err(errors) => {
            report.errors = errors;
            return report;
        }
    };
    report.id = figure.program.doc.id.clone();
    report
        .warnings
        .extend(figure.program.warnings.iter().cloned());

    let draw =
        |report: &mut Report, figure: &mut Figure, state: String, theme: &str| -> Option<Image> {
            let t = Theme::by_name(theme);
            let started = Instant::now();
            let result = figure.render(canvas, &t);
            let ms = started.elapsed().as_secs_f64() * 1000.0;
            match result {
                Ok(image) => {
                    report.renders.push(Rendered {
                        state,
                        theme: theme.to_string(),
                        ink: ink(&image, t.bg()),
                        ms,
                    });
                    for w in &figure.warnings {
                        if !report.warnings.iter().any(|o| o.code == w.code) {
                            report.warnings.push(w.clone());
                        }
                    }
                    Some(image)
                }
                Err(e) => {
                    if !report.errors.contains(&e) {
                        report.errors.push(e);
                    }
                    None
                }
            }
        };

    for theme in ["light", "dark"] {
        if draw(&mut report, &mut figure, "default".into(), theme).is_some() {
            let painted = report.renders.last().map_or(0.0, |r| r.ink);
            if painted < MIN_INK {
                report.errors.push(
                    Error::new(
                        codes::E601,
                        format!(
                            "this figure paints {:.2}% of its pixels at rest in the {theme} theme",
                            painted * 100.0
                        ),
                    )
                    .hint(format!(
                        "a figure should paint at least {:.1}%; check ranges, `when` and colours",
                        MIN_INK * 100.0
                    )),
                );
            }
        }
    }

    for (name, ends) in corners(&figure) {
        let mut images = Vec::new();
        for input in &ends {
            figure.reset();
            if let Err(e) = figure.set(&name, input.clone()) {
                report.errors.push(e);
                continue;
            }
            if let Some(img) = draw(&mut report, &mut figure, describe(&name, input), "light") {
                images.push(img.rgba);
            }
        }
        if images.len() > 1 && images.windows(2).all(|w| w[0] == w[1]) {
            let path = figure
                .program
                .doc
                .values
                .iter()
                .find(|v| v.name == name)
                .map(|v| v.path.clone())
                .unwrap_or_default();
            report.warnings.push(
                Error::warning(
                    codes::W301,
                    format!("moving \"{name}\" across its whole range changes nothing drawn"),
                )
                .hint("a control that changes nothing teaches nothing: use the value, or remove it")
                .at_path(&path),
            );
        }
    }
    figure.reset();
    report
}

#[derive(Debug, Clone, Serialize)]
pub struct Explained {
    pub path: String,
    pub formula: String,
    #[serde(rename = "type")]
    pub ty: String,
    /// Each name and what it resolved to.
    pub names: Vec<(String, String)>,
}

/// For `hapto explain`: every let and every formula, with its type and what
/// each name in it resolved to.
pub fn explain(text: &str) -> Result<Vec<Explained>, Vec<Error>> {
    let figure = Figure::from_text(text)?;
    let tree = crate::tree::parse(text).map_err(|e| vec![e])?;
    let source = |path: &str| match tree.pointer(path) {
        Some(Tree::String(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
    };
    let mut out = Vec::new();
    for (name, ty, resolutions) in figure.program.module.explain() {
        let path = figure
            .program
            .doc
            .lets
            .iter()
            .find(|(k, _)| k.split('(').next().map(str::trim) == Some(name.as_str()))
            .map(|(_, f)| f.path.clone())
            .unwrap_or_default();
        out.push(Explained {
            formula: source(&path),
            path,
            ty: ty.to_string(),
            names: resolutions.into_iter().map(|r| (r.name, r.what)).collect(),
        });
    }
    for c in figure.program.formulas() {
        out.push(Explained {
            path: c.path.clone(),
            formula: source(&c.path),
            ty: c.checked.ty.to_string(),
            names: c
                .checked
                .resolutions
                .iter()
                .map(|r| (r.name.clone(), r.what.clone()))
                .collect(),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "hapto: 2\nid: good\nalt: a\nvalues: [ { name: x, type: number, range: [-1, 1], default: 0 } ]\nview: { x: { range: [-2, 2] }, y: { range: [-2, 2] } }\nmarks: [ { point: '[x, 0]', r: 12 }, { function: 'x**2', var: x } ]";

    #[test]
    fn a_good_figure_passes_with_renders_at_its_corners() {
        let r = check(GOOD, Canvas::new(360.0, 200.0, 1.0));
        assert!(r.ok(), "{:?}", r.errors);
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        let states: Vec<&str> = r.renders.iter().map(|r| r.state.as_str()).collect();
        assert_eq!(states, ["default", "default", "x=-1", "x=1"]);
        assert_eq!(r.id.as_deref(), Some("good"));
    }

    #[test]
    fn a_blank_figure_is_e601() {
        let doc = "hapto: 2\nalt: a\nview: { x: { range: [0, 1] }, y: { range: [0, 1] }, axes: none }\nmarks: [ { point: '[5, 5]' } ]";
        let r = check(doc, Canvas::new(360.0, 200.0, 1.0));
        assert!(
            r.errors.iter().any(|e| e.code == codes::E601),
            "{:?}",
            r.errors
        );
    }

    #[test]
    fn a_value_that_changes_nothing_is_w301() {
        let doc = "hapto: 2\nalt: a\nvalues: [ { name: k, type: number, range: [0, 1], default: 0 } ]\nview: { x: { range: [-2, 2] }, y: { range: [-2, 2] } }\nmarks: [ { function: 'x**2', var: x } ]";
        let r = check(doc, Canvas::new(360.0, 200.0, 1.0));
        let w = r
            .warnings
            .iter()
            .find(|w| w.code == codes::W301)
            .expect("W301");
        assert_eq!(w.path.as_deref(), Some("/values/0"));
    }

    #[test]
    fn load_errors_come_back_unrendered() {
        let r = check(
            "hapto: 2\nalt: a\nmarks: []",
            Canvas::new(100.0, 100.0, 1.0),
        );
        assert!(!r.ok() && r.renders.is_empty());
    }

    #[test]
    fn set_lists_parse() {
        let s = parse_set("x=3, mode=damped,p=[1, -2],show=false").unwrap();
        assert_eq!(s.len(), 4);
        assert_eq!(s[0], ("x".into(), Input::Number(3.0)));
        assert_eq!(s[1], ("mode".into(), Input::Choice("damped".into())));
        assert_eq!(s[2], ("p".into(), Input::Point([1.0, -2.0])));
        assert_eq!(s[3], ("show".into(), Input::Flag(false)));
        assert!(parse_set("nonsense").is_err());
    }

    #[test]
    fn explain_names_every_formula() {
        let doc = "hapto: 2\nalt: a\nvalues: [ { name: x, type: number, range: [-1, 1], default: 0 } ]\nlet: { 'f(t)': 't**2', top: 'f(x) + 1' }\nview: { x: { range: [-2, 2] }, y: { range: [-2, 2] } }\nmarks: [ { point: '[x, top]' } ]";
        let e = explain(doc).unwrap();
        let point = e.iter().find(|e| e.path == "/marks/0/point").unwrap();
        assert_eq!(point.formula, "[x, top]");
        assert_eq!(point.ty, "vec");
        assert!(point.names.iter().any(|(n, _)| n == "top"));
        let top = e.iter().find(|e| e.path == "/let/top").unwrap();
        assert_eq!(top.formula, "f(x) + 1");
    }
}
