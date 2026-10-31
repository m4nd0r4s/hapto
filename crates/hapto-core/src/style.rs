//! Style: how a mark is painted (SPEC-2.0 §9).
//!
//! Every style key is a formula, so a stroke can thicken with dissipation
//! and a fill can shift from cool to hot as a value moves. What a mark does
//! not say comes from its kind's defaults (§9.4), and a focused mark draws
//! with its highlight while the rest of its view fades (§12.5).

use hapto_expr::eval::Evaluation;
use hapto_expr::{Error, Value};

use crate::document::Dash;
use crate::program::{Compiled, CompiledMark, CompiledStyle};
use crate::scene::Paint;
use crate::theme::Theme;

/// How much other marks fade while one is focused.
pub const DIM: f64 = 0.35;
/// How much heavier a focused stroke is.
pub const FOCUS_WIDTH: f64 = 1.6;

fn colour(theme: &Theme, token: &str) -> Option<[f64; 4]> {
    theme.colour(token)
}

/// The paint a kind starts from (§9.4). `series` counts the curve-like marks
/// before this one in its view: the second and later take `c1`, `c2`, …
pub fn defaults(kind: &str, theme: &Theme, series: usize) -> Paint {
    let stroke = |token: &str, width: f64| Paint {
        stroke: colour(theme, token),
        width,
        fill: None,
        fill_opacity: 0.0,
        opacity: 1.0,
        dash: None,
    };
    let filled = |token: &str, opacity: f64| Paint {
        stroke: None,
        width: 2.0,
        fill: colour(theme, token),
        fill_opacity: opacity,
        opacity: 1.0,
        dash: None,
    };
    match kind {
        // Data curves rotate through the categorical tokens; shapes do not.
        "bezier" | "spring" | "arc" => stroke("accent", 2.0),
        "function" | "polyline" | "trace" => {
            let token = if series == 0 {
                "accent".to_string()
            } else {
                format!("c{}", (series - 1) % 8 + 1)
            };
            stroke(&token, 2.0)
        }
        "point" => Paint {
            fill: colour(theme, "accent"),
            fill_opacity: 1.0,
            stroke: Some(theme.bg()),
            width: 1.5,
            opacity: 1.0,
            dash: None,
        },
        "area" | "polygon" | "sector" => filled("accent", 0.18),
        "rect" | "circle" | "ellipse" => stroke("fg", 2.0),
        "box" => Paint {
            fill: Some(theme.bg()),
            fill_opacity: 1.0,
            stroke: colour(theme, "fg"),
            width: 2.0,
            opacity: 1.0,
            dash: None,
        },
        "text" | "math" => filled("fg", 1.0),
        "drop" | "hline" | "vline" | "angle" | "brace" => Paint {
            dash: if kind == "drop" {
                Some(vec![4.0, 3.0])
            } else {
                None
            },
            ..stroke("muted", 1.0)
        },
        _ => stroke("fg", 2.0),
    }
}

/// Whether a kind counts as a series for the colour rotation.
pub fn is_series(kind: &str) -> bool {
    matches!(kind, "function" | "polyline" | "trace")
}

fn eval(ev: &mut Evaluation, c: &Compiled, bound: &[Value]) -> Result<Value, Error> {
    ev.eval(&c.checked, bound).map_err(|e| e.at_path(&c.path))
}

/// Apply a style's formulas over `p`. A key that evaluates to no value keeps
/// the default (§4.8).
pub fn apply(
    ev: &mut Evaluation,
    s: &CompiledStyle,
    mut p: Paint,
    bound: &[Value],
) -> Result<Paint, Error> {
    let as_colour = |v: Value| match v {
        Value::Colour(c) => Some(c),
        _ => None,
    };
    if let Some(c) = &s.stroke {
        p.stroke = as_colour(eval(ev, c, bound)?).or(p.stroke);
    }
    if let Some(c) = &s.fill {
        if let Some(fill) = as_colour(eval(ev, c, bound)?) {
            p.fill = Some(fill);
            // Asking for a fill is asking to see it (kinemo defaults it to 0).
            if p.fill_opacity == 0.0 {
                p.fill_opacity = 1.0;
            }
        }
    }
    for (slot, target) in [
        (&s.width, &mut p.width),
        (&s.fill_opacity, &mut p.fill_opacity),
        (&s.opacity, &mut p.opacity),
    ] {
        if let Some(c) = slot {
            let n = eval(ev, c, bound)?.num();
            if n.is_finite() {
                *target = n.max(0.0);
            }
        }
    }
    Ok(p)
}

fn dash(d: &Option<Dash>) -> Option<Option<Vec<f64>>> {
    match d {
        Some(Dash::Dashed) => Some(Some(vec![6.0, 4.0])),
        Some(Dash::Dotted) => Some(Some(vec![1.5, 3.0])),
        Some(Dash::Pattern(p)) => Some(Some(p.clone())),
        Some(Dash::Solid) => Some(None),
        None => None,
    }
}

/// Whether a mark is the focused one: by its name, its repeat-indexed name
/// (`bar[3]`), or its focus group.
pub fn is_focused(m: &CompiledMark, focus: &str, index: Option<usize>) -> bool {
    if focus.is_empty() {
        return false;
    }
    let c = &m.source.common;
    c.focus_group.as_deref() == Some(focus)
        || c.name.as_deref() == Some(focus)
        || matches!((&c.name, index), (Some(n), Some(i)) if format!("{n}[{i}]") == focus)
}

/// The full paint of one mark instance: defaults, style, dash, then focus.
#[allow(clippy::too_many_arguments)]
pub fn resolve(
    ev: &mut Evaluation,
    m: &CompiledMark,
    theme: &Theme,
    series: usize,
    focus: &str,
    index: Option<usize>,
    view_has_focus: bool,
    bound: &[Value],
) -> Result<Paint, Error> {
    let mut p = apply(ev, &m.style, defaults(m.source.kind, theme, series), bound)?;
    if let Some(d) = dash(&m.source.common.style.dash) {
        p.dash = d;
    }
    if is_focused(m, focus, index) {
        p = match &m.highlight {
            Some(h) => apply(ev, h, p, bound)?,
            None => {
                let hl = theme.colour("highlight");
                Paint {
                    stroke: p.stroke.and(hl),
                    fill: if p.stroke.is_none() {
                        p.fill.and(hl)
                    } else {
                        p.fill
                    },
                    width: p.width * FOCUS_WIDTH,
                    ..p
                }
            }
        };
    } else if view_has_focus {
        p.opacity *= DIM;
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn series_rotate_through_the_categorical_tokens() {
        let t = Theme::light();
        assert_eq!(defaults("function", &t, 0).stroke, t.colour("accent"));
        assert_eq!(defaults("function", &t, 1).stroke, t.colour("c1"));
        assert_eq!(defaults("function", &t, 9).stroke, t.colour("c1"));
    }

    #[test]
    fn fills_that_fill_by_default_are_visible() {
        let t = Theme::dark();
        for kind in ["point", "area", "polygon", "box", "text"] {
            let p = defaults(kind, &t, 0);
            assert!(p.fill.is_some() && p.fill_opacity > 0.0, "{kind}");
        }
        assert_eq!(defaults("drop", &t, 0).dash, Some(vec![4.0, 3.0]));
    }
}
