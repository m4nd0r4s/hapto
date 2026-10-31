//! Views: coordinate systems with axes, ticks and labels (SPEC-2.0 §7.2).

use serde_json::Value as Tree;

use hapto_expr::Error;

use super::formula::{formula, Formula};
use super::values::Scale;
use crate::codes;
use crate::tree::{child_path, index_path, At, Reader};

#[derive(Debug, Clone, PartialEq)]
pub enum Ticks {
    Auto,
    None,
    Pi,
    Step(f64),
    At(Vec<f64>),
    Labelled(Vec<(f64, String)>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Axis {
    pub range: [Formula; 2],
    pub label: Option<String>,
    pub ticks: Ticks,
    pub scale: Scale,
}

#[derive(Debug, Clone, PartialEq)]
pub struct View {
    pub id: String,
    pub x: Axis,
    pub y: Axis,
    pub equal_aspect: bool,
    pub axes: String,
    pub grid: String,
    /// `[column, row, column_span, row_span]` in the layout grid.
    pub area: [usize; 4],
    pub clip: bool,
    pub background: String,
    pub title: Option<String>,
    pub legend: String,
    pub polar: bool,
    pub padding: Option<f64>,
    pub path: String,
}

pub const KEYS: &[&str] = &[
    "id",
    "x",
    "y",
    "aspect",
    "axes",
    "grid",
    "area",
    "clip",
    "background",
    "title",
    "legend",
    "coords",
    "padding",
];
pub const AXIS_KEYS: &[&str] = &["range", "label", "ticks", "scale"];
pub const AXES: &[&str] = &["cross", "box", "left-bottom", "x", "y", "none"];
pub const LEGENDS: &[&str] = &[
    "top-left",
    "top-right",
    "bottom-left",
    "bottom-right",
    "outside-right",
    "outside-bottom",
    "none",
];

fn axis(r: &mut Reader, at: At, name: &str) -> Option<Axis> {
    let map = r.map(at, AXIS_KEYS, &["range"], &format!("the {name} axis"))?;
    let range_path = child_path(at.path, "range");
    let range = match map.get("range")? {
        Tree::Array(items) if items.len() == 2 => {
            let lo = formula(
                r,
                At {
                    tree: &items[0],
                    path: &index_path(&range_path, 0),
                },
                "the low end",
            )?;
            let hi = formula(
                r,
                At {
                    tree: &items[1],
                    path: &index_path(&range_path, 1),
                },
                "the high end",
            )?;
            [lo, hi]
        }
        _ => {
            r.error(
                Error::new(codes::E403, format!("the {name} range is [low, high]"))
                    .at_path(range_path),
            );
            return None;
        }
    };
    let [Formula { text: lo, .. }, Formula { text: hi, .. }] = &range;
    {
        if let (Ok(lo), Ok(hi)) = (lo.parse::<f64>(), hi.parse::<f64>()) {
            if lo >= hi {
                r.error(
                    Error::new(
                        codes::E502,
                        format!("the {name} range [{lo}, {hi}] is empty or inverted"),
                    )
                    .hint("write it as [low, high] with low < high")
                    .at_path(child_path(at.path, "range")),
                );
            }
        }
    }
    let label = map.get("label").and_then(|t| {
        r.string(
            At {
                tree: t,
                path: &child_path(at.path, "label"),
            },
            "the axis label",
        )
    });
    let scale = match map.get("scale") {
        Some(t) => match r
            .choice(
                At {
                    tree: t,
                    path: &child_path(at.path, "scale"),
                },
                &["linear", "log"],
                "axis scale",
            )?
            .as_str()
        {
            "log" => Scale::Log,
            _ => Scale::Linear,
        },
        None => Scale::Linear,
    };
    let ticks = match map.get("ticks") {
        Some(t) => ticks(
            r,
            At {
                tree: t,
                path: &child_path(at.path, "ticks"),
            },
        )?,
        None => Ticks::Auto,
    };
    Some(Axis {
        range,
        label,
        ticks,
        scale,
    })
}

fn ticks(r: &mut Reader, at: At) -> Option<Ticks> {
    Some(match at.tree {
        Tree::String(s) => match s.as_str() {
            "auto" => Ticks::Auto,
            "none" => Ticks::None,
            "pi" => Ticks::Pi,
            _ => {
                r.choice(at, &["auto", "none", "pi"], "ticks setting")?;
                return None;
            }
        },
        Tree::Number(n) => {
            let step = n.as_f64()?;
            if step <= 0.0 {
                r.error(Error::new(codes::E403, "a tick step is above zero").at_path(at.path));
                return None;
            }
            Ticks::Step(step)
        }
        Tree::Array(items) => {
            let mut at_list = Vec::new();
            for (i, t) in items.iter().enumerate() {
                at_list.push(r.number(
                    At {
                        tree: t,
                        path: &index_path(at.path, i),
                    },
                    "a tick position",
                )?);
            }
            Ticks::At(at_list)
        }
        Tree::Object(_) => {
            let map = r.map(at, &["at", "labels"], &["at", "labels"], "ticks")?;
            let positions = r
                .list(
                    At {
                        tree: &map["at"],
                        path: &child_path(at.path, "at"),
                    },
                    "tick positions",
                )?
                .clone();
            let labels = r
                .list(
                    At {
                        tree: &map["labels"],
                        path: &child_path(at.path, "labels"),
                    },
                    "tick labels",
                )?
                .clone();
            if positions.len() != labels.len() {
                r.error(
                    Error::new(codes::E403, "there must be one label per tick").at_path(at.path),
                );
                return None;
            }
            let mut out = Vec::new();
            for (i, (p, l)) in positions.iter().zip(&labels).enumerate() {
                let n = r.number(
                    At {
                        tree: p,
                        path: &index_path(&child_path(at.path, "at"), i),
                    },
                    "a tick position",
                )?;
                let s = r.string(
                    At {
                        tree: l,
                        path: &index_path(&child_path(at.path, "labels"), i),
                    },
                    "a tick label",
                )?;
                out.push((n, s));
            }
            Ticks::Labelled(out)
        }
        _ => {
            r.wrong_type(at, "ticks: auto, none, pi, a step, a list, or {at, labels}");
            return None;
        }
    })
}

pub fn read_view(r: &mut Reader, at: At, default_id: &str) -> Option<View> {
    let map = r.map(at, KEYS, &["x", "y"], "a view")?;
    let s = |r: &mut Reader, key: &str, options: &[&str], default: &str| -> String {
        match map.get(key) {
            Some(t) => r
                .choice(
                    At {
                        tree: t,
                        path: &child_path(at.path, key),
                    },
                    options,
                    key,
                )
                .unwrap_or_else(|| default.into()),
            None => default.into(),
        }
    };
    let id = match map.get("id") {
        Some(t) => r.ident(
            At {
                tree: t,
                path: &child_path(at.path, "id"),
            },
            "the view's id",
        )?,
        None => default_id.to_string(),
    };
    let x = axis(
        r,
        At {
            tree: map.get("x")?,
            path: &child_path(at.path, "x"),
        },
        "x",
    );
    let y = axis(
        r,
        At {
            tree: map.get("y")?,
            path: &child_path(at.path, "y"),
        },
        "y",
    );
    let equal_aspect = s(r, "aspect", &["free", "equal"], "free") == "equal";
    let axes = s(r, "axes", AXES, "cross");
    let grid = match map.get("grid") {
        Some(Tree::Bool(false)) | None => "none".to_string(),
        Some(t) => r
            .choice(
                At {
                    tree: t,
                    path: &child_path(at.path, "grid"),
                },
                &["major", "minor"],
                "grid",
            )
            .unwrap_or_else(|| "none".into()),
    };
    let area = match map.get("area") {
        Some(t) => {
            let path = child_path(at.path, "area");
            let items = r.list(
                At {
                    tree: t,
                    path: &path,
                },
                "the area",
            )?;
            let mut nums = Vec::new();
            for (i, item) in items.iter().enumerate() {
                let n = r.number(
                    At {
                        tree: item,
                        path: &index_path(&path, i),
                    },
                    "a grid cell",
                )?;
                if n < 0.0 || n.fract() != 0.0 {
                    r.error(
                        Error::new(codes::E403, "grid cells are whole numbers from 0")
                            .at_path(&path),
                    );
                    return None;
                }
                nums.push(n as usize);
            }
            match nums.as_slice() {
                [c, row] => [*c, *row, 1, 1],
                [c, row, cs, rs] if *cs > 0 && *rs > 0 => [*c, *row, *cs, *rs],
                _ => {
                    r.error(
                        Error::new(
                            codes::E403,
                            "an area is [column, row] or [column, row, column_span, row_span]",
                        )
                        .at_path(path),
                    );
                    return None;
                }
            }
        }
        None => [0, 0, 1, 1],
    };
    let clip = match map.get("clip") {
        Some(t) => r.boolean(
            At {
                tree: t,
                path: &child_path(at.path, "clip"),
            },
            "clip",
        )?,
        None => true,
    };
    let background = s(r, "background", &["none", "panel"], "none");
    let title = map.get("title").and_then(|t| {
        r.string(
            At {
                tree: t,
                path: &child_path(at.path, "title"),
            },
            "the title",
        )
    });
    let legend = s(r, "legend", LEGENDS, "top-right");
    let polar = s(r, "coords", &["cartesian", "polar"], "cartesian") == "polar";
    let padding = match map.get("padding") {
        Some(t) => Some(r.number(
            At {
                tree: t,
                path: &child_path(at.path, "padding"),
            },
            "the padding in px",
        )?),
        None => None,
    };
    Some(View {
        id,
        x: x?,
        y: y?,
        equal_aspect,
        axes,
        grid,
        area,
        clip,
        background,
        title,
        legend,
        polar,
        padding,
        path: at.path.to_string(),
    })
}

/// A fixed range of an axis, when both ends are literal numbers.
pub fn literal_range(axis: &Axis) -> Option<(f64, f64)> {
    Some((
        axis.range[0].text.parse().ok()?,
        axis.range[1].text.parse().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::parse;

    fn read(yaml: &str) -> (Option<View>, Vec<&'static str>) {
        let tree = parse(yaml).unwrap();
        let mut r = Reader::default();
        let v = read_view(
            &mut r,
            At {
                tree: &tree,
                path: "/view",
            },
            "main",
        );
        (v, r.errors.iter().map(|e| e.code.0).collect())
    }

    #[test]
    fn a_full_view() {
        let (v, errors) = read(
            "{ x: { range: [-2, 6], label: '$x$', ticks: pi }, y: { range: [-7, 'top'], scale: linear, ticks: { at: [0, 1], labels: [a, b] } },
               aspect: equal, axes: box, grid: major, area: [1, 0], clip: false, legend: none, padding: 12 }",
        );
        assert!(errors.is_empty(), "{errors:?}");
        let v = v.unwrap();
        assert_eq!(v.id, "main");
        assert!(v.equal_aspect);
        assert_eq!(v.x.ticks, Ticks::Pi);
        assert_eq!(v.y.range[1].text, "top");
        assert_eq!(v.area, [1, 0, 1, 1]);
        assert!(matches!(v.y.ticks, Ticks::Labelled(ref l) if l.len() == 2));
    }

    #[test]
    fn mistakes_are_named() {
        assert_eq!(
            read("{ x: { range: [1, 0] }, y: { range: [0, 1] } }").1,
            ["E502"]
        );
        assert_eq!(read("{ x: { range: [0, 1] } }").1, ["E402"]);
        assert_eq!(
            read("{ x: { range: [0, 1] }, y: { range: [0, 1] }, axes: crossed }").1,
            ["E403"]
        );
        assert_eq!(
            read("{ x: { range: [0, 1], tick: 1 }, y: { range: [0, 1] } }").1,
            ["E401"]
        );
        assert_eq!(
            read("{ x: { range: [0, 1] }, y: { range: [0, 1] }, area: [1] }").1,
            ["E403"]
        );
    }
}
