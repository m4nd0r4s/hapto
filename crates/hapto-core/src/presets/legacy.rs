//! The `legacy` preset: 1.0 `BookFigure` specs, read unchanged (§14.3).
//!
//! ```json
//! { "kind": "plot", "variable": "x", "at": 1, "curve": "x**2 - x*(2)",
//!   "tangent": true, "x_range": [-2, 4], "y_range": [-2.35, 9.35], "knob": [-1, 5] }
//! ```
//!
//! A `plot` is a curve with a point riding it, dropped to the axis, and
//! optionally its tangent; `columns` are two vectors from the origin and the
//! parallelogram they span. Both draw what 1.0 drew, gain the axes' ticks
//! and labels, and get an alt text from their formulas. A document without
//! `hapto:` whose `kind` is one of these reads as `preset: legacy`, with
//! W801 saying so. The preset goes in 3.0.

use hapto_expr::Error;
use serde_json::{json, Map, Value as Tree};

use crate::codes;

/// The keys of a 1.0 spec.
const KEYS: &[&str] = &[
    "kind", "variable", "at", "knob", "x_range", "y_range", "curve", "tangent", "first", "second",
];

/// Keys a `hapto: 2, preset: legacy` document may also have (§14.1).
const BESIDE: &[&str] = &[
    "hapto", "preset", "id", "title", "caption", "alt", "size", "values", "let", "marks",
    "readouts", "controls", "baked",
];

pub fn applies(tree: &Tree) -> bool {
    let Tree::Object(map) = tree else {
        return false;
    };
    match map.get("hapto") {
        None => matches!(
            map.get("kind").and_then(Tree::as_str),
            Some("plot" | "columns")
        ),
        Some(_) => map.get("preset").and_then(Tree::as_str) == Some("legacy"),
    }
}

/// Reads a 1.0 spec's fields, collecting every error with its path.
struct Spec<'a> {
    map: &'a Map<String, Tree>,
    errors: Vec<Error>,
}

impl Spec<'_> {
    fn missing(&mut self, key: &str, what: &str) {
        self.errors.push(
            Error::new(
                codes::E402,
                format!("a legacy figure needs \"{key}\": {what}"),
            )
            .at_path(format!("/{key}")),
        );
    }

    fn wrong(&mut self, key: &str, what: &str) {
        self.errors.push(
            Error::new(codes::E403, format!("\"{key}\" has to be {what}"))
                .at_path(format!("/{key}")),
        );
    }

    fn text(&mut self, key: &str, what: &str) -> Option<String> {
        match self.map.get(key) {
            Some(Tree::String(s)) => Some(s.clone()),
            Some(_) => {
                self.wrong(key, what);
                None
            }
            None => {
                self.missing(key, what);
                None
            }
        }
    }

    fn number(&mut self, key: &str) -> Option<f64> {
        match self.map.get(key) {
            None => None,
            Some(t) => {
                let n = t.as_f64();
                if n.is_none() {
                    self.wrong(key, "a number");
                }
                n
            }
        }
    }

    fn range(&mut self, key: &str, required: bool) -> Option<[f64; 2]> {
        let what = "two numbers, [low, high]";
        let Some(t) = self.map.get(key) else {
            if required {
                self.missing(key, what);
            }
            return None;
        };
        match t
            .as_array()
            .map(|a| a.iter().map(Tree::as_f64).collect::<Vec<_>>())
        {
            Some(v) if v.len() == 2 && v.iter().all(Option::is_some) => {
                Some([v[0].unwrap_or(0.0), v[1].unwrap_or(0.0)])
            }
            _ => {
                self.wrong(key, what);
                None
            }
        }
    }

    /// A column, `[across, up]`, as formulas; numbers are taken as written.
    fn column(&mut self, key: &str) -> Option<[String; 2]> {
        let what = "two formulas, [across, up]";
        let Some(t) = self.map.get(key) else {
            self.missing(key, what);
            return None;
        };
        let item = |t: &Tree| match t {
            Tree::String(s) => Some(s.clone()),
            Tree::Number(n) => Some(n.to_string()),
            _ => None,
        };
        match t.as_array().map(|a| a.iter().map(item).collect::<Vec<_>>()) {
            Some(v) if v.len() == 2 && v.iter().all(Option::is_some) => {
                let mut v = v.into_iter().flatten();
                Some([v.next()?, v.next()?])
            }
            _ => {
                self.wrong(key, what);
                None
            }
        }
    }
}

pub fn expand(tree: &Tree) -> Result<(Tree, Vec<Error>), Vec<Error>> {
    let Tree::Object(map) = tree else {
        unreachable!("applies() saw a map");
    };
    let versioned = map.contains_key("hapto");
    let mut s = Spec {
        map,
        errors: Vec::new(),
    };
    for key in map.keys() {
        let known = KEYS.contains(&key.as_str()) || (versioned && BESIDE.contains(&key.as_str()));
        if !known {
            let mut known: Vec<&str> = KEYS.to_vec();
            if versioned {
                known.extend(BESIDE);
            }
            let mut err = Error::new(codes::E401, format!("a legacy figure has no \"{key}\""))
                .at_path(format!("/{}", crate::tree::escape(key)));
            if let Some(n) = crate::tree::nearest(key, &known) {
                err = err.hint(format!("did you mean `{n}`?"));
            }
            s.errors.push(err);
        }
    }

    let kind = s.text("kind", "plot or columns");
    let variable = s.text("variable", "the name of the value the reader moves");
    let at = s.number("at").unwrap_or(0.0);
    let x_range = s.range("x_range", true);
    let y_range = s.range("y_range", true);
    // How far the reader may move the value; a spec that does not say
    // moves it over its own axis, as 1.0 did.
    let knob = s.range("knob", false).or(x_range);

    let body = match kind.as_deref() {
        Some("plot") => {
            let curve = s.text("curve", "a formula over the variable");
            let tangent = match map.get("tangent") {
                None => false,
                Some(Tree::Bool(b)) => *b,
                Some(_) => {
                    s.wrong("tangent", "true or false");
                    false
                }
            };
            curve.map(|c| Body::Plot { curve: c, tangent })
        }
        Some("columns") => match (s.column("first"), s.column("second")) {
            (Some(first), Some(second)) => Some(Body::Columns { first, second }),
            _ => None,
        },
        Some(other) => {
            s.errors.push(
                Error::new(
                    codes::E403,
                    format!("\"{other}\" is not a kind of legacy figure"),
                )
                .options(["plot", "columns"])
                .at_path("/kind"),
            );
            None
        }
        None => None,
    };
    if !s.errors.is_empty() {
        return Err(s.errors);
    }
    let (Some(body), Some(v), Some(x_range), Some(y_range), Some(knob)) =
        (body, variable, x_range, y_range, knob)
    else {
        return Err(s.errors);
    };

    let (low, high) = (knob[0].min(knob[1]), knob[0].max(knob[1]));
    let mut out = json!({
        "hapto": 2,
        "values": [{
            "name": v,
            "type": "number",
            "range": [low, high],
            // 1.0 clamped a start outside the knob rather than refusing it.
            "default": at.clamp(low, high),
            "label": v,
        }],
        "view": {
            "x": { "range": x_range, "label": v },
            "y": { "range": y_range },
        },
    });
    let o = out.as_object_mut().expect("a map");
    match body {
        Body::Plot { curve, tangent } => {
            // The curve as a function of its own, so the tangent can name it.
            let f = if v == "f" { "g" } else { "f" };
            let on = format!("[{v}, {f}({v})]");
            let mut marks = vec![json!({ "function": format!("{f}({v})"), "var": v })];
            if tangent {
                // 1.0's tangent reached a sixth of the axis either side.
                let reach = (x_range[1] - x_range[0]).abs() / 3.0;
                marks.push(json!({
                    "tangent": v,
                    "of": f,
                    "length": format!("{reach} * sqrt(1 + derivative({f}, {v})**2)"),
                    "style": { "stroke": "positive" },
                }));
            }
            marks.push(json!({ "drop": on, "to": "x" }));
            marks.push(json!({
                "point": on,
                "drag": { v.clone(): "pointer.x" },
                "style": { "fill": "negative" },
            }));
            let mut lets = Map::new();
            lets.insert(format!("{f}({v})"), json!(curve));
            let shown = curve.replace("**", "^");
            let mut alt = format!("The curve y = {shown} with a point at {v} = {{{v}:.2f}}");
            if tangent {
                lets.insert(
                    "legacy_slope".into(),
                    json!(format!("derivative({f}, {v})")),
                );
                alt.push_str(", where its tangent has slope {legacy_slope:.2f}");
            }
            alt.push('.');
            o.insert("let".into(), Tree::Object(lets));
            o.insert("marks".into(), Tree::Array(marks));
            o.insert("alt".into(), json!(alt));
        }
        Body::Columns { first, second } => {
            // The axes are the plane's, not the value's.
            o["view"]["x"]
                .as_object_mut()
                .expect("a map")
                .remove("label");
            let (a, b) = ("first_column", "second_column");
            o.insert(
                "let".into(),
                json!({
                    a: format!("[{}, {}]", first[0], first[1]),
                    b: format!("[{}, {}]", second[0], second[1]),
                    "legacy_area": format!("abs({a}.x * {b}.y - {a}.y * {b}.x)"),
                }),
            );
            o.insert(
                "marks".into(),
                json!([
                    { "polygon": format!("[[0, 0], {a}, {a} + {b}, {b}]"), "style": { "stroke": "highlight" } },
                    { "vector": a, "style": { "stroke": "cool" } },
                    { "vector": b, "style": { "stroke": "negative" } },
                ]),
            );
            let alt = format!(
                "The columns ({}, {}) and ({}, {}) and the parallelogram they span, at {v} = {{{v}:.2f}}; its area is {{legacy_area:.2f}}.",
                first[0], first[1], second[0], second[1]
            );
            o.insert("alt".into(), json!(alt));
        }
    }
    if versioned {
        super::append(o, map);
    }
    let warning = Error::warning(
        codes::W801,
        "this is a 1.0 BookFigure spec, read through the `legacy` preset",
    )
    .hint("the preset goes in 3.0; write the figure in the 2.0 language before then")
    .at_path("/");
    Ok((out, vec![warning]))
}

enum Body {
    Plot {
        curve: String,
        tangent: bool,
    },
    Columns {
        first: [String; 2],
        second: [String; 2],
    },
}

#[cfg(test)]
mod tests {
    use crate::render::Figure;
    use crate::scene::{ink, Canvas};
    use crate::state::Input;
    use crate::theme::Theme;

    /// The three specs hmu's book emits (calc-partial-derivatives,
    /// la-matrix-theory, la-vector-spaces).
    pub const HMU: [&str; 3] = [
        r#"{"kind": "plot", "variable": "x", "at": 1.0, "curve": "x**2 - x*(2)", "tangent": true, "x_range": [-2.0, 4.0], "y_range": [-2.35, 9.35], "knob": [-2.0, 4.0]}"#,
        r#"{"kind": "columns", "variable": "x", "at": 2.0, "first": ["2", "1"], "second": ["x", "2"], "x_range": [-1.0, 9.0], "y_range": [-1.0, 3.0], "knob": [-2.0, 6.0]}"#,
        r#"{"kind": "columns", "variable": "x", "at": 1.5, "first": ["1", "2"], "second": ["x", "4"], "x_range": [-1.0, 7.0], "y_range": [-1.0, 7.0], "knob": [-2.0, 5.0]}"#,
    ];

    fn codes(text: &str) -> Vec<String> {
        match Figure::from_text(text) {
            Ok(_) => vec![],
            Err(e) => e.into_iter().map(|e| e.code.0.to_string()).collect(),
        }
    }

    #[test]
    fn every_spec_hmu_emits_draws_and_follows_its_value() {
        let (canvas, theme) = (Canvas::new(480.0, 270.0, 1.0), Theme::light());
        for spec in HMU {
            let mut f = Figure::from_text(spec).unwrap_or_else(|e| panic!("{spec}: {e:?}"));
            assert_eq!(f.program.warnings[0].code.0, "W801");
            let before = f.render(canvas, &theme).unwrap();
            assert!(ink(&before, theme.bg()) > 0.01, "{spec} is blank");
            f.set("x", Input::Number(4.0)).unwrap();
            let after = f.render(canvas, &theme).unwrap();
            assert_ne!(before.rgba, after.rgba, "{spec} ignored x");
        }
    }

    #[test]
    fn the_plot_keeps_1_0_behaviour() {
        let mut f = Figure::from_text(HMU[0]).unwrap();
        let theme = Theme::light();
        // The value moves over the knob; the point drags along the curve.
        let x = &f.program.doc.values[0];
        assert!(matches!(
            x.kind,
            crate::document::Kind::Number {
                low: -2.0,
                high: 4.0,
                default: 1.0,
                ..
            }
        ));
        let kinds: Vec<&str> = f.program.marks.iter().map(|m| m.source.kind).collect();
        assert_eq!(kinds, ["function", "tangent", "drop", "point"]);
        assert!(!f.program.marks[3].drag.is_empty());
        assert_eq!(
            f.alt(&theme).unwrap(),
            "The curve y = x^2 - x*(2) with a point at x = 1.00, where its tangent has slope 0.00."
        );
    }

    #[test]
    fn columns_flatten_when_the_determinant_does() {
        let mut f = Figure::from_text(HMU[1]).unwrap();
        let (canvas, theme) = (Canvas::new(480.0, 270.0, 1.0), Theme::light());
        let open = ink(&f.render(canvas, &theme).unwrap(), theme.bg());
        f.set("x", Input::Number(4.0)).unwrap();
        let flat = ink(&f.render(canvas, &theme).unwrap(), theme.bg());
        assert!(flat < open, "the parallelogram did not flatten");
        assert!(f.alt(&theme).unwrap().ends_with("its area is 0.00."));
    }

    #[test]
    fn a_start_outside_the_knob_is_clamped_as_in_1_0() {
        let spec = HMU[0].replace("\"at\": 1.0", "\"at\": 9.0");
        let f = Figure::from_text(&spec).unwrap();
        assert_eq!(f.program.state.get("x"), Some(&hapto_expr::Value::Num(4.0)));
    }

    #[test]
    fn mistakes_are_named_with_their_paths() {
        assert_eq!(
            codes(&HMU[0].replace("\"tangent\"", "\"tangnet\"")),
            ["E401"]
        );
        assert_eq!(
            codes(&HMU[0].replace("\"tangent\": true", "\"tangent\": 1")),
            ["E403"]
        );
        assert_eq!(
            codes(&HMU[1].replace("[\"2\", \"1\"]", "[\"2\"]")),
            ["E403"]
        );
        assert_eq!(codes(&HMU[0].replace("x**2 - x*(2)", "x^^2")), ["E102"]);
    }

    #[test]
    fn a_versioned_document_names_the_preset_and_adds_to_it() {
        let doc = "hapto: 2\npreset: legacy\nkind: plot\nvariable: x\ncurve: 'x**2'\nx_range: [-2, 2]\ny_range: [-1, 4]\nalt: 'A parabola.'\nlet: { top: 'x**2 + 1' }\nmarks: [ { point: '[x, top]' } ]\nreadouts: [ { id: top, value: top } ]";
        let mut f = Figure::from_text(doc).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(f.program.marks.len(), 4);
        assert_eq!(f.program.marks[3].source.kind, "point");
        assert_eq!(f.alt(&Theme::light()).unwrap(), "A parabola.");
        assert_eq!(f.readouts(&Theme::light()).unwrap()[0].text, "1");
        // Another preset is still refused until it lands.
        assert_eq!(
            codes("hapto: 2\nalt: a\npreset: plot\ncurve: 'x'"),
            ["E401", "E610"]
        );
    }
}
