//! Value declarations (SPEC-2.0 §5.2) and what they promise.

use serde_json::Value as Tree;

use hapto_expr::check::is_reserved;
use hapto_expr::{format, Error};

use crate::codes;
use crate::tree::{child_path, index_path, At, Reader};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Step {
    /// Continuous: a thousandth of the range for keys and sliders (§5.3).
    Continuous,
    /// A fixed step; on a log scale, a factor above 1.
    By(f64),
    /// Whole numbers.
    Integer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scale {
    Linear,
    Log,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Number {
        low: f64,
        high: f64,
        default: f64,
        step: Step,
        scale: Scale,
        unit: Option<String>,
        format: Option<String>,
    },
    Choice {
        options: Vec<String>,
        labels: Vec<(String, String)>,
        default: String,
    },
    Point {
        low: [f64; 2],
        high: [f64; 2],
        default: [f64; 2],
    },
    Flag {
        default: bool,
    },
}

impl Kind {
    pub fn type_name(&self) -> &'static str {
        match self {
            Kind::Number { .. } => "number",
            Kind::Choice { .. } => "choice",
            Kind::Point { .. } => "point",
            Kind::Flag { .. } => "flag",
        }
    }
}

/// How the page draws a control for a value (§12.1).
#[derive(Debug, Clone, PartialEq)]
pub struct Control {
    pub kind: String,
    pub marks: Vec<f64>,
    pub mark_labels: Vec<String>,
}

/// A play button on a number (§12.6).
#[derive(Debug, Clone, PartialEq)]
pub struct Play {
    pub duration: f64,
    pub looping: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValueDecl {
    pub name: String,
    pub kind: Kind,
    pub label: Option<String>,
    pub control: Option<Control>,
    pub play: Option<Play>,
    pub shared: bool,
    pub path: String,
}

pub const KEYS: &[&str] = &[
    "name", "type", "range", "default", "step", "scale", "label", "unit", "format", "control",
    "play", "shared", "options", "labels",
];

/// The controls each type of value may have (§12.1).
pub fn controls_for(type_name: &str) -> &'static [&'static str] {
    match type_name {
        "number" => &["slider", "stepper", "input", "reroll", "none"],
        "choice" => &["buttons", "dropdown", "none"],
        "point" => &["handle", "inputs", "none"],
        _ => &["toggle", "checkbox", "none"],
    }
}

fn pair(r: &mut Reader, at: At, what: &str) -> Option<[f64; 2]> {
    let items = r.list(at, what)?;
    if items.len() != 2 {
        r.error(
            Error::new(codes::E403, format!("{what} is two numbers, [low, high]")).at_path(at.path),
        );
        return None;
    }
    let mut out = [0.0; 2];
    for (i, item) in items.iter().enumerate() {
        let path = index_path(at.path, i);
        out[i] = r.number(
            At {
                tree: item,
                path: &path,
            },
            what,
        )?;
    }
    Some(out)
}

pub fn read_values(r: &mut Reader, at: At) -> Vec<ValueDecl> {
    let Some(items) = r.list(at, "the values") else {
        return Vec::new();
    };
    let mut out: Vec<ValueDecl> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let path = index_path(at.path, i);
        if let Some(v) = read_value(
            r,
            At {
                tree: item,
                path: &path,
            },
        ) {
            if out.iter().any(|o| o.name == v.name) {
                r.error(
                    Error::new(
                        hapto_expr::error::codes::E306,
                        format!("two values are named \"{}\"", v.name),
                    )
                    .at_path(&path),
                );
                continue;
            }
            out.push(v);
        }
    }
    out
}

fn read_value(r: &mut Reader, at: At) -> Option<ValueDecl> {
    let map = r.map(at, KEYS, &["name", "type", "default"], "a value")?;
    let get = |key: &str| map.get(key);
    let path_of = |key: &str| child_path(at.path, key);

    let name_path = path_of("name");
    let name = r.ident(
        At {
            tree: get("name")?,
            path: &name_path,
        },
        "the value's name",
    )?;
    if is_reserved(&name) {
        r.error(
            Error::new(
                hapto_expr::error::codes::E302,
                format!("\"{name}\" is reserved and cannot name a value"),
            )
            .hint("pick another name")
            .at_path(&name_path),
        );
        return None;
    }
    let type_path = path_of("type");
    let type_name = r.choice(
        At {
            tree: get("type")?,
            path: &type_path,
        },
        &["number", "choice", "point", "flag"],
        "value type",
    )?;
    let default_path = path_of("default");
    let default = At {
        tree: get("default")?,
        path: &default_path,
    };

    // Keys that only some types take.
    let only = |r: &mut Reader, key: &str, types: &[&str]| {
        if map.contains_key(key) && !types.contains(&type_name.as_str()) {
            r.error(
                Error::new(codes::E401, format!("a {type_name} value has no \"{key}\""))
                    .hint(format!("`{key}` belongs to {} values", types.join(" and ")))
                    .at_path(child_path(at.path, key)),
            );
        }
    };
    only(r, "range", &["number", "point"]);
    only(r, "step", &["number"]);
    only(r, "scale", &["number"]);
    only(r, "unit", &["number"]);
    only(r, "format", &["number"]);
    only(r, "play", &["number"]);
    only(r, "options", &["choice"]);
    only(r, "labels", &["choice"]);

    let kind = match type_name.as_str() {
        "number" => read_number(r, at, map, default)?,
        "choice" => read_choice(r, at, map, default)?,
        "point" => read_point(r, at, map, default)?,
        _ => Kind::Flag {
            default: r.boolean(default, "the default")?,
        },
    };

    let label = match get("label") {
        Some(t) => r.string(
            At {
                tree: t,
                path: &path_of("label"),
            },
            "the label",
        ),
        None => None,
    };
    let control = match get("control") {
        Some(t) => read_control(
            r,
            At {
                tree: t,
                path: &path_of("control"),
            },
            &type_name,
        ),
        None => None,
    };
    let play = match get("play") {
        Some(t) => read_play(
            r,
            At {
                tree: t,
                path: &path_of("play"),
            },
        ),
        None => None,
    };
    let shared = match get("shared") {
        Some(t) => r
            .boolean(
                At {
                    tree: t,
                    path: &path_of("shared"),
                },
                "shared",
            )
            .unwrap_or(false),
        None => false,
    };
    Some(ValueDecl {
        name,
        kind,
        label,
        control,
        play,
        shared,
        path: at.path.to_string(),
    })
}

fn read_number(
    r: &mut Reader,
    at: At,
    map: &serde_json::Map<String, Tree>,
    default: At,
) -> Option<Kind> {
    let range_path = child_path(at.path, "range");
    let Some(range_tree) = map.get("range") else {
        r.error(Error::new(codes::E402, "a number value needs a \"range\"").at_path(at.path));
        return None;
    };
    let [low, high] = pair(
        r,
        At {
            tree: range_tree,
            path: &range_path,
        },
        "the range",
    )?;
    let scale = match map.get("scale") {
        Some(t) => match r
            .choice(
                At {
                    tree: t,
                    path: &child_path(at.path, "scale"),
                },
                &["linear", "log"],
                "scale",
            )?
            .as_str()
        {
            "log" => Scale::Log,
            _ => Scale::Linear,
        },
        None => Scale::Linear,
    };
    if low >= high {
        r.error(
            Error::new(
                codes::E502,
                format!("the range [{low}, {high}] is empty or inverted"),
            )
            .hint("write the range as [low, high] with low < high")
            .at_path(&range_path),
        );
        return None;
    }
    if scale == Scale::Log && low <= 0.0 {
        r.error(
            Error::new(codes::E503, "a logarithmic range must be above zero")
                .hint("start the range at a small positive number, such as 0.01")
                .at_path(&range_path),
        );
        return None;
    }
    let d = r.number(default, "the default")?;
    if !(low..=high).contains(&d) {
        r.error(
            Error::new(
                codes::E501,
                format!("the default {d} is outside the range [{low}, {high}]"),
            )
            .at_path(default.path),
        );
    }
    let step = match map.get("step") {
        None => Step::Continuous,
        Some(Tree::String(s)) if s == "integer" => Step::Integer,
        Some(t) => {
            let step_path = child_path(at.path, "step");
            let s = r.number(
                At {
                    tree: t,
                    path: &step_path,
                },
                "the step (a number, or \"integer\")",
            )?;
            let ok = if scale == Scale::Log {
                s > 1.0
            } else {
                s > 0.0
            };
            if !ok {
                let want = if scale == Scale::Log {
                    "a factor above 1 on a log scale"
                } else {
                    "above zero"
                };
                r.error(
                    Error::new(codes::E403, format!("the step must be {want}")).at_path(step_path),
                );
                return None;
            }
            Step::By(s)
        }
    };
    let unit = match map.get("unit") {
        Some(t) => r.string(
            At {
                tree: t,
                path: &child_path(at.path, "unit"),
            },
            "the unit",
        ),
        None => None,
    };
    let fmt = match map.get("format") {
        Some(t) => {
            let path = child_path(at.path, "format");
            let spec = r.string(
                At {
                    tree: t,
                    path: &path,
                },
                "the format",
            )?;
            if !format::is_valid(&spec) {
                r.error(
                    Error::new(codes::E403, format!("\"{spec}\" is not a format spec"))
                        .hint("use a Python format spec such as \".2f\", \"+.3g\" or \".1%\"")
                        .at_path(path),
                );
                None
            } else {
                Some(spec)
            }
        }
        None => None,
    };
    Some(Kind::Number {
        low,
        high,
        default: d.clamp(low, high),
        step,
        scale,
        unit,
        format: fmt,
    })
}

fn read_choice(
    r: &mut Reader,
    at: At,
    map: &serde_json::Map<String, Tree>,
    default: At,
) -> Option<Kind> {
    let options_path = child_path(at.path, "options");
    let Some(options_tree) = map.get("options") else {
        r.error(Error::new(codes::E402, "a choice value needs \"options\"").at_path(at.path));
        return None;
    };
    let items = r.list(
        At {
            tree: options_tree,
            path: &options_path,
        },
        "the options",
    )?;
    let mut options: Vec<String> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let path = index_path(&options_path, i);
        let o = r.ident(
            At {
                tree: item,
                path: &path,
            },
            "an option",
        )?;
        if options.contains(&o) {
            r.error(
                Error::new(
                    hapto_expr::error::codes::E306,
                    format!("the option \"{o}\" appears twice"),
                )
                .at_path(path),
            );
            continue;
        }
        options.push(o);
    }
    if !(2..=12).contains(&options.len()) {
        r.error(
            Error::new(codes::E403, "a choice has between 2 and 12 options").at_path(&options_path),
        );
        return None;
    }
    let d = r.string(default, "the default")?;
    if !options.contains(&d) {
        r.error(
            Error::new(codes::E504, format!("\"{d}\" is not one of the options"))
                .options(options.clone())
                .at_path(default.path),
        );
        return None;
    }
    let mut labels = Vec::new();
    if let Some(t) = map.get("labels") {
        let path = child_path(at.path, "labels");
        let known: Vec<&str> = options.iter().map(String::as_str).collect();
        if let Some(m) = r.map(
            At {
                tree: t,
                path: &path,
            },
            &known,
            &[],
            "the labels",
        ) {
            for (k, v) in m {
                if let Some(text) = r.string(
                    At {
                        tree: v,
                        path: &child_path(&path, k),
                    },
                    "a label",
                ) {
                    labels.push((k.clone(), text));
                }
            }
        }
    }
    Some(Kind::Choice {
        options,
        labels,
        default: d,
    })
}

fn read_point(
    r: &mut Reader,
    at: At,
    map: &serde_json::Map<String, Tree>,
    default: At,
) -> Option<Kind> {
    let range_path = child_path(at.path, "range");
    let Some(range_tree) = map.get("range") else {
        r.error(
            Error::new(
                codes::E402,
                "a point value needs a \"range\": [[x0, y0], [x1, y1]]",
            )
            .at_path(at.path),
        );
        return None;
    };
    let corners = r.list(
        At {
            tree: range_tree,
            path: &range_path,
        },
        "the range",
    )?;
    if corners.len() != 2 {
        r.error(
            Error::new(
                codes::E403,
                "a point's range is two corners, [[x0, y0], [x1, y1]]",
            )
            .at_path(&range_path),
        );
        return None;
    }
    let low = pair(
        r,
        At {
            tree: &corners[0],
            path: &index_path(&range_path, 0),
        },
        "the low corner",
    )?;
    let high = pair(
        r,
        At {
            tree: &corners[1],
            path: &index_path(&range_path, 1),
        },
        "the high corner",
    )?;
    if low[0] >= high[0] || low[1] >= high[1] {
        r.error(
            Error::new(codes::E502, "the point's range is empty or inverted")
                .hint("write it as [[x_low, y_low], [x_high, y_high]]")
                .at_path(&range_path),
        );
        return None;
    }
    let d = pair(r, default, "the default point")?;
    if !(low[0]..=high[0]).contains(&d[0]) || !(low[1]..=high[1]).contains(&d[1]) {
        r.error(
            Error::new(
                codes::E501,
                format!("the default {d:?} is outside the range"),
            )
            .at_path(default.path),
        );
    }
    Some(Kind::Point {
        low,
        high,
        default: d,
    })
}

fn read_control(r: &mut Reader, at: At, type_name: &str) -> Option<Control> {
    let allowed = controls_for(type_name);
    let what = format!("control for a {type_name}");
    if let Tree::String(_) = at.tree {
        let kind = r.choice(at, allowed, &what)?;
        return Some(Control {
            kind,
            marks: vec![],
            mark_labels: vec![],
        });
    }
    let map = r.map(
        at,
        &["type", "marks", "mark_labels"],
        &["type"],
        "a control",
    )?;
    let kind = r.choice(
        At {
            tree: map.get("type")?,
            path: &child_path(at.path, "type"),
        },
        allowed,
        &what,
    )?;
    let mut marks = Vec::new();
    if let Some(t) = map.get("marks") {
        let path = child_path(at.path, "marks");
        if let Some(items) = r.list(
            At {
                tree: t,
                path: &path,
            },
            "the marks",
        ) {
            for (i, m) in items.iter().enumerate() {
                if let Some(n) = r.number(
                    At {
                        tree: m,
                        path: &index_path(&path, i),
                    },
                    "a mark",
                ) {
                    marks.push(n);
                }
            }
        }
    }
    let mut mark_labels = Vec::new();
    if let Some(t) = map.get("mark_labels") {
        let path = child_path(at.path, "mark_labels");
        if let Some(items) = r.list(
            At {
                tree: t,
                path: &path,
            },
            "the mark labels",
        ) {
            for (i, m) in items.iter().enumerate() {
                if let Some(s) = r.string(
                    At {
                        tree: m,
                        path: &index_path(&path, i),
                    },
                    "a mark label",
                ) {
                    mark_labels.push(s);
                }
            }
        }
        if mark_labels.len() != marks.len() {
            r.error(Error::new(codes::E403, "there must be one mark label per mark").at_path(path));
        }
    }
    Some(Control {
        kind,
        marks,
        mark_labels,
    })
}

fn read_play(r: &mut Reader, at: At) -> Option<Play> {
    let map = r.map(at, &["duration", "loop", "autoplay"], &["duration"], "play")?;
    if map.contains_key("autoplay") {
        r.error(
            Error::new(codes::E401, "a figure never plays on its own")
                .hint("remove `autoplay`: the reader presses play (§12.6)")
                .at_path(child_path(at.path, "autoplay")),
        );
    }
    let duration_path = child_path(at.path, "duration");
    let duration = r.number(
        At {
            tree: map.get("duration")?,
            path: &duration_path,
        },
        "the duration in seconds",
    )?;
    if duration <= 0.0 {
        r.error(Error::new(codes::E403, "a duration is above zero seconds").at_path(duration_path));
    }
    let looping = match map.get("loop") {
        Some(t) => r.choice(
            At {
                tree: t,
                path: &child_path(at.path, "loop"),
            },
            &["once", "wrap", "bounce"],
            "loop",
        )?,
        None => "once".into(),
    };
    Some(Play { duration, looping })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::parse;

    fn read(yaml: &str) -> (Vec<ValueDecl>, Vec<Error>) {
        let tree = parse(yaml).unwrap();
        let mut r = Reader::default();
        let values = read_values(
            &mut r,
            At {
                tree: &tree,
                path: "/values",
            },
        );
        (values, r.errors)
    }

    fn codes_of(yaml: &str) -> Vec<&'static str> {
        read(yaml).1.iter().map(|e| e.code.0).collect()
    }

    #[test]
    fn the_four_types() {
        let (v, errors) = read(
            "- { name: x, type: number, range: [-1, 5], default: 1, step: 0.01, unit: s, format: '.2f', play: { duration: 4, loop: wrap } }
- { name: mode, type: choice, options: [free, damped], default: damped, labels: { free: Free } }
- { name: p, type: point, range: [[-4, -4], [4, 4]], default: [2.2, 0.9] }
- { name: on, type: flag, default: true, control: checkbox }
- { name: n, type: number, range: [1, 100], default: 10, step: integer, scale: log }",
        );
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(v.len(), 5);
        assert!(matches!(v[0].kind, Kind::Number { step: Step::By(s), .. } if s == 0.01));
        assert_eq!(v[0].play.as_ref().unwrap().looping, "wrap");
        assert!(matches!(&v[1].kind, Kind::Choice { default, .. } if default == "damped"));
        assert!(matches!(
            v[2].kind,
            Kind::Point {
                default: [2.2, 0.9],
                ..
            }
        ));
        assert_eq!(v[3].control.as_ref().unwrap().kind, "checkbox");
        assert!(matches!(
            v[4].kind,
            Kind::Number {
                step: Step::Integer,
                scale: Scale::Log,
                ..
            }
        ));
    }

    #[test]
    fn domains_are_checked() {
        assert_eq!(
            codes_of("- { name: x, type: number, range: [5, 1], default: 2 }"),
            ["E502"]
        );
        assert_eq!(
            codes_of("- { name: x, type: number, range: [0, 1], default: 2 }"),
            ["E501"]
        );
        assert_eq!(
            codes_of("- { name: x, type: number, range: [0, 1], default: 0.5, scale: log }"),
            ["E503"]
        );
        assert_eq!(
            codes_of("- { name: m, type: choice, options: [a, b], default: c }"),
            ["E504"]
        );
        assert_eq!(
            codes_of("- { name: m, type: choice, options: [a], default: a }"),
            ["E403"]
        );
        assert_eq!(
            codes_of("- { name: p, type: point, range: [[0, 0], [1, 1]], default: [2, 0] }"),
            ["E501"]
        );
        assert_eq!(
            codes_of(
                "- { name: x, type: number, range: [1, 10], default: 2, scale: log, step: 0.5 }"
            ),
            ["E403"]
        );
    }

    #[test]
    fn names_types_and_keys_are_checked() {
        assert_eq!(
            codes_of("- { name: pi, type: number, range: [0, 1], default: 0 }"),
            ["E302"]
        );
        assert_eq!(
            codes_of("- { name: x, type: number, range: [0, 1], default: 0 }\n- { name: x, type: flag, default: true }"),
            ["E306"]
        );
        assert_eq!(
            codes_of("- { name: x, type: numbr, range: [0, 1], default: 0 }"),
            ["E403"]
        );
        assert_eq!(
            codes_of("- { name: x, type: number, range: [0, 1], defualt: 0 }"),
            ["E401", "E402"]
        );
        assert_eq!(
            codes_of("- { name: f, type: flag, default: true, step: 1 }"),
            ["E401"]
        );
        assert_eq!(
            codes_of("- { name: x, type: number, range: [0, 1], default: 0, control: buttons }"),
            ["E403"]
        );
        assert_eq!(
            codes_of("- { name: x, type: number, range: [0, 1], default: 0, format: zz }"),
            ["E403"]
        );
        assert_eq!(
            codes_of("- { name: x, type: number, range: [0, 1], default: 0, play: { duration: 2, autoplay: true } }"),
            ["E401"]
        );
    }

    #[test]
    fn errors_point_into_the_document() {
        let (_, errors) = read("- { name: x, type: number, range: [0, 1], default: 0 }\n- { name: y, type: number, range: [3, 1], default: 2 }");
        assert_eq!(errors[0].path.as_deref(), Some("/values/1/range"));
    }
}
