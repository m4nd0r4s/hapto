//! The figure document (SPEC-2.0 §6): read, validated, ready to compile.
//!
//! [`read`] turns JSON or YAML text into a [`Document`], or into every error
//! the text has, each at its JSON Pointer. Formulas are not checked here —
//! that needs the whole figure's names and is the program's job — but every
//! key, type, range, option and reference that can be checked from the
//! document alone is.

pub mod formula;
pub mod marks;
pub mod values;
pub mod views;

use serde_json::Value as Tree;

use hapto_expr::Error;

pub use formula::Formula;
pub use marks::{Arg, Common, Dash, Drag, Label, Mark, Style};
pub use values::{Kind, Scale, Step, ValueDecl};
pub use views::{Axis, Ticks, View};

use crate::codes;
use crate::tree::{child_path, index_path, At, Reader};

/// How big the figure is drawn (§7.6).
#[derive(Debug, Clone, PartialEq)]
pub struct Size {
    pub aspect: f64,
    pub min_height: f64,
    pub max_height: f64,
    pub phone_aspect: Option<f64>,
}

impl Default for Size {
    fn default() -> Self {
        Size {
            aspect: 16.0 / 9.0,
            min_height: 240.0,
            max_height: 560.0,
            phone_aspect: None,
        }
    }
}

/// The grid several views share (§7.7).
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub columns: usize,
    pub rows: usize,
    pub phone_stack: bool,
    pub gap: f64,
    pub widths: Vec<f64>,
    pub heights: Vec<f64>,
}

impl Default for Layout {
    fn default() -> Self {
        Layout {
            columns: 1,
            rows: 1,
            phone_stack: true,
            gap: 16.0,
            widths: vec![],
            heights: vec![],
        }
    }
}

/// A page value the figure reads (§5.6).
#[derive(Debug, Clone, PartialEq)]
pub struct Import {
    pub name: String,
    pub alias: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Readout {
    pub id: String,
    pub label: Option<String>,
    pub value: Formula,
    pub format: Option<String>,
    pub unit: Option<String>,
    pub compare: Option<Formula>,
    pub place: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Controls {
    pub columns: Option<usize>,
    pub position: String,
    pub groups: Vec<(String, Vec<String>)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub id: Option<String>,
    pub title: Option<String>,
    pub caption: Option<String>,
    pub alt: String,
    pub size: Size,
    pub uses: Vec<Import>,
    pub values: Vec<ValueDecl>,
    /// `let:` entries as written: key, formula, JSON Pointer.
    pub lets: Vec<(String, Formula)>,
    pub layout: Layout,
    pub views: Vec<View>,
    pub marks: Vec<Mark>,
    pub readouts: Vec<Readout>,
    pub controls: Controls,
}

/// Every top-level key of the format (§6.2).
pub const KEYS: &[&str] = &[
    "hapto", "id", "title", "caption", "alt", "size", "uses", "values", "let", "compute", "layout",
    "view", "views", "marks", "readouts", "controls", "preset",
];

/// Read a document; on failure, every error it has.
pub fn read(text: &str) -> Result<Document, Vec<Error>> {
    let tree = crate::tree::parse(text).map_err(|e| vec![e])?;
    let mut r = Reader::default();
    let doc = read_tree(&mut r, &tree);
    match doc {
        Some(doc) if r.errors.is_empty() => Ok(doc),
        _ => Err(r.errors),
    }
}

fn read_tree(r: &mut Reader, tree: &Tree) -> Option<Document> {
    let root = At { tree, path: "" };
    let Tree::Object(map) = tree else {
        r.wrong_type(root, "a map (a figure document)");
        return None;
    };
    // The version decides how to read the rest (§6.4).
    match map.get("hapto") {
        None => {
            let legacy = matches!(
                map.get("kind").and_then(Tree::as_str),
                Some("plot" | "columns")
            );
            let mut err = Error::new(codes::E407, "this document does not say which format it is")
                .at_path("/");
            err = if legacy {
                err.hint("this looks like a 1.0 BookFigure spec, which the `legacy` preset reads (it lands with the page runtime)")
            } else {
                err.hint("add `hapto: 2` at the top")
            };
            r.error(err);
            return None;
        }
        Some(Tree::Number(n)) if n.as_f64() == Some(2.0) => {}
        Some(other) => {
            r.error(
                Error::new(
                    codes::E403,
                    format!("this build reads `hapto: 2`, not {other}"),
                )
                .at_path("/hapto"),
            );
            return None;
        }
    }
    r.map(root, KEYS, &["alt"], "a figure");
    let at = |key: &'static str| map.get(key).map(|t| (t, child_path("", key)));

    for (key, milestone) in [("compute", "M4"), ("preset", "M6")] {
        if map.contains_key(key) {
            r.error(
                Error::new(
                    codes::E610,
                    format!(
                        "`{key}` is part of the format, but this build does not support it yet"
                    ),
                )
                .hint(format!("it lands in milestone {milestone} of SPEC-2.0 §23"))
                .at_path(child_path("", key)),
            );
        }
    }

    let text = |r: &mut Reader, key: &'static str, what: &str| -> Option<String> {
        let (t, p) = at(key)?;
        r.string(At { tree: t, path: &p }, what)
    };
    let id = at("id").and_then(|(t, p)| r.ident(At { tree: t, path: &p }, "the figure's id"));
    let title = text(r, "title", "the title");
    let caption = text(r, "caption", "the caption");
    let alt = text(r, "alt", "the alt text").unwrap_or_default();

    let size = match at("size") {
        Some((t, p)) => read_size(r, At { tree: t, path: &p }).unwrap_or_default(),
        None => Size::default(),
    };
    let uses = match at("uses") {
        Some((t, p)) => read_uses(r, At { tree: t, path: &p }),
        None => vec![],
    };
    let values = match at("values") {
        Some((t, p)) => values::read_values(r, At { tree: t, path: &p }),
        None => vec![],
    };
    let lets = match at("let") {
        Some((t, p)) => read_lets(r, At { tree: t, path: &p }),
        None => vec![],
    };
    let layout = match at("layout") {
        Some((t, p)) => read_layout(r, At { tree: t, path: &p }).unwrap_or_default(),
        None => Layout::default(),
    };

    // One view by shorthand, or several (§6.2).
    let views = match (at("view"), at("views")) {
        (Some(_), Some(_)) => {
            r.error(
                Error::new(codes::E406, "a figure has `view` or `views`, not both")
                    .hint("use `view` for one view, `views` for several")
                    .at_path("/views"),
            );
            vec![]
        }
        (Some((t, p)), None) => {
            if t.get("id").is_some() {
                r.error(
                    Error::new(codes::E401, "the single `view` has no id")
                        .hint("drop `id`, or use `views`")
                        .at_path(child_path(&p, "id")),
                );
            }
            views::read_view(r, At { tree: t, path: &p }, "main")
                .into_iter()
                .collect()
        }
        (None, Some((t, p))) => {
            let mut out: Vec<View> = Vec::new();
            for (i, item) in r
                .list(At { tree: t, path: &p }, "the views")
                .cloned()
                .unwrap_or_default()
                .iter()
                .enumerate()
            {
                let path = index_path(&p, i);
                if item.get("id").is_none() {
                    r.error(
                        Error::new(codes::E402, "each of several views needs an \"id\"")
                            .at_path(&path),
                    );
                    continue;
                }
                if let Some(v) = views::read_view(
                    r,
                    At {
                        tree: item,
                        path: &path,
                    },
                    "main",
                ) {
                    if out.iter().any(|o| o.id == v.id) {
                        r.error(
                            Error::new(
                                hapto_expr::error::codes::E306,
                                format!("two views are named \"{}\"", v.id),
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
        (None, None) => vec![],
    };

    let marks = match at("marks") {
        Some((t, p)) => marks::read_marks(r, At { tree: t, path: &p }),
        None => vec![],
    };
    if marks.is_empty() && !map.contains_key("preset") {
        r.error(
            Error::new(codes::E402, "a figure needs marks (or a preset)")
                .hint("add `marks:` with at least one mark")
                .at_path("/"),
        );
    }
    if !marks.is_empty()
        && views.is_empty()
        && !map.contains_key("view")
        && !map.contains_key("views")
    {
        r.error(
            Error::new(
                codes::E402,
                "a figure with marks needs a view to draw them in",
            )
            .hint("add `view: { x: { range: [a, b] }, y: { range: [c, d] } }`")
            .at_path("/"),
        );
    }
    check_views_of(r, &marks, &views);

    let readouts = match at("readouts") {
        Some((t, p)) => read_readouts(r, At { tree: t, path: &p }),
        None => vec![],
    };
    let controls = match at("controls") {
        Some((t, p)) => read_controls(r, At { tree: t, path: &p }, &values).unwrap_or_default(),
        None => Controls {
            position: "below".into(),
            ..Default::default()
        },
    };

    Some(Document {
        id,
        title,
        caption,
        alt,
        size,
        uses,
        values,
        lets,
        layout,
        views,
        marks,
        readouts,
        controls,
    })
}

/// Every mark draws in a view that exists; with several views, it says which.
fn check_views_of(r: &mut Reader, marks: &[Mark], views: &[View]) {
    for m in marks {
        match &m.common.view {
            Some(v) if !views.iter().any(|view| &view.id == v) && !views.is_empty() => {
                let ids: Vec<&str> = views.iter().map(|v| v.id.as_str()).collect();
                r.error(
                    Error::new(codes::E409, format!("there is no view \"{v}\""))
                        .options(ids.iter().copied())
                        .at_path(child_path(&m.path, "view")),
                );
            }
            None if views.len() > 1 => {
                r.error(
                    Error::new(
                        codes::E409,
                        "with several views, a mark says which it draws in",
                    )
                    .hint("add `view: <id>`")
                    .options(views.iter().map(|v| v.id.clone()))
                    .at_path(&m.path),
                );
            }
            _ => {}
        }
        if let Arg::Marks(children) = &m.main {
            check_views_of(r, children, views);
        }
    }
}

fn read_size(r: &mut Reader, at: At) -> Option<Size> {
    let map = r.map(
        at,
        &["aspect", "min_height", "max_height", "phone_aspect"],
        &[],
        "the size",
    )?;
    let mut size = Size::default();
    let num = |r: &mut Reader, key: &str| {
        map.get(key).and_then(|t| {
            r.number(
                At {
                    tree: t,
                    path: &child_path(at.path, key),
                },
                key,
            )
        })
    };
    if let Some(a) = num(r, "aspect") {
        size.aspect = a;
    }
    if let Some(h) = num(r, "min_height") {
        size.min_height = h;
    }
    if let Some(h) = num(r, "max_height") {
        size.max_height = h;
    }
    size.phone_aspect = num(r, "phone_aspect");
    if size.aspect <= 0.0 || size.min_height > size.max_height {
        r.error(
            Error::new(
                codes::E403,
                "the size has a non-positive aspect or min_height above max_height",
            )
            .at_path(at.path),
        );
    }
    Some(size)
}

fn read_uses(r: &mut Reader, at: At) -> Vec<Import> {
    let mut out = Vec::new();
    for (i, item) in r
        .list(at, "the imported values")
        .cloned()
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        let path = index_path(at.path, i);
        let here = At {
            tree: item,
            path: &path,
        };
        let (name, alias) = match item {
            Tree::String(s) => (s.clone(), s.replace('.', "_")),
            _ => {
                let Some(m) = r.map(here, &["name", "as"], &["name", "as"], "an import") else {
                    continue;
                };
                let (Some(n), Some(a)) = (
                    r.string(
                        At {
                            tree: &m["name"],
                            path: &child_path(&path, "name"),
                        },
                        "the imported value",
                    ),
                    r.ident(
                        At {
                            tree: &m["as"],
                            path: &child_path(&path, "as"),
                        },
                        "its name here",
                    ),
                ) else {
                    continue;
                };
                (n, a)
            }
        };
        out.push(Import { name, alias });
    }
    out
}

fn read_lets(r: &mut Reader, at: At) -> Vec<(String, Formula)> {
    let Tree::Object(map) = at.tree else {
        r.wrong_type(at, "a map of names to formulas (the lets)");
        return vec![];
    };
    let mut out = Vec::new();
    for (key, value) in map {
        let path = child_path(at.path, key);
        if let Some(f) = formula::formula(
            r,
            At {
                tree: value,
                path: &path,
            },
            "a let",
        ) {
            out.push((key.clone(), f));
        }
    }
    out
}

fn read_layout(r: &mut Reader, at: At) -> Option<Layout> {
    let map = r.map(
        at,
        &["columns", "rows", "phone", "gap", "widths", "heights"],
        &[],
        "the layout",
    )?;
    let mut l = Layout::default();
    let num = |r: &mut Reader, key: &str| {
        map.get(key).and_then(|t| {
            r.number(
                At {
                    tree: t,
                    path: &child_path(at.path, key),
                },
                key,
            )
        })
    };
    if let Some(c) = num(r, "columns") {
        l.columns = c.max(1.0) as usize;
    }
    if let Some(c) = num(r, "rows") {
        l.rows = c.max(1.0) as usize;
    }
    if let Some(g) = num(r, "gap") {
        l.gap = g;
    }
    if let Some(t) = map.get("phone") {
        l.phone_stack = r
            .choice(
                At {
                    tree: t,
                    path: &child_path(at.path, "phone"),
                },
                &["stack", "keep"],
                "phone layout",
            )
            .as_deref()
            == Some("stack");
    }
    for (key, target) in [("widths", &mut l.widths), ("heights", &mut l.heights)] {
        if let Some(t) = map.get(key) {
            let path = child_path(at.path, key);
            for (i, item) in r
                .list(
                    At {
                        tree: t,
                        path: &path,
                    },
                    key,
                )
                .cloned()
                .unwrap_or_default()
                .iter()
                .enumerate()
            {
                if let Some(n) = r.number(
                    At {
                        tree: item,
                        path: &index_path(&path, i),
                    },
                    "a relative size",
                ) {
                    target.push(n);
                }
            }
        }
    }
    Some(l)
}

fn read_readouts(r: &mut Reader, at: At) -> Vec<Readout> {
    let mut out: Vec<Readout> = Vec::new();
    for (i, item) in r
        .list(at, "the readouts")
        .cloned()
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        let path = index_path(at.path, i);
        let here = At {
            tree: item,
            path: &path,
        };
        let Some(map) = r.map(
            here,
            &["id", "label", "value", "format", "unit", "compare", "place"],
            &["id", "value"],
            "a readout",
        ) else {
            continue;
        };
        let p = |key: &str| child_path(&path, key);
        let Some(id) = r.ident(
            At {
                tree: &map["id"],
                path: &p("id"),
            },
            "the readout's id",
        ) else {
            continue;
        };
        if out.iter().any(|o| o.id == id) {
            r.error(
                Error::new(
                    hapto_expr::error::codes::E306,
                    format!("two readouts are named \"{id}\""),
                )
                .at_path(&path),
            );
            continue;
        }
        let Some(value) = formula::formula(
            r,
            At {
                tree: &map["value"],
                path: &p("value"),
            },
            "the readout's value",
        ) else {
            continue;
        };
        let format = map.get("format").and_then(|t| {
            let spec = r.string(
                At {
                    tree: t,
                    path: &p("format"),
                },
                "the format",
            )?;
            if hapto_expr::format::is_valid(&spec) {
                Some(spec)
            } else {
                r.error(
                    Error::new(codes::E403, format!("\"{spec}\" is not a format spec"))
                        .hint("use a Python format spec such as \".2f\"")
                        .at_path(p("format")),
                );
                None
            }
        });
        out.push(Readout {
            id,
            label: map.get("label").and_then(|t| {
                r.string(
                    At {
                        tree: t,
                        path: &p("label"),
                    },
                    "the label",
                )
            }),
            value,
            format,
            unit: map.get("unit").and_then(|t| {
                r.string(
                    At {
                        tree: t,
                        path: &p("unit"),
                    },
                    "the unit",
                )
            }),
            compare: map.get("compare").and_then(|t| {
                formula::formula(
                    r,
                    At {
                        tree: t,
                        path: &p("compare"),
                    },
                    "the comparison",
                )
            }),
            place: map
                .get("place")
                .and_then(|t| {
                    r.choice(
                        At {
                            tree: t,
                            path: &p("place"),
                        },
                        &["below", "inside", "none"],
                        "readout place",
                    )
                })
                .unwrap_or_else(|| "below".into()),
            path: path.clone(),
        });
    }
    out
}

fn read_controls(r: &mut Reader, at: At, values: &[ValueDecl]) -> Option<Controls> {
    let map = r.map(at, &["columns", "position", "groups"], &[], "the controls")?;
    let mut c = Controls {
        position: "below".into(),
        ..Default::default()
    };
    if let Some(t) = map.get("columns") {
        if t.as_str() != Some("auto") {
            c.columns = r
                .number(
                    At {
                        tree: t,
                        path: &child_path(at.path, "columns"),
                    },
                    "columns",
                )
                .map(|n| n.max(1.0) as usize);
        }
    }
    if let Some(t) = map.get("position") {
        c.position = r.choice(
            At {
                tree: t,
                path: &child_path(at.path, "position"),
            },
            &["below", "side"],
            "controls position",
        )?;
    }
    if let Some(t) = map.get("groups") {
        let path = child_path(at.path, "groups");
        for (i, g) in r
            .list(
                At {
                    tree: t,
                    path: &path,
                },
                "the groups",
            )
            .cloned()
            .unwrap_or_default()
            .iter()
            .enumerate()
        {
            let gp = index_path(&path, i);
            let Some(gm) = r.map(
                At { tree: g, path: &gp },
                &["title", "values"],
                &["title", "values"],
                "a control group",
            ) else {
                continue;
            };
            let title = r
                .string(
                    At {
                        tree: &gm["title"],
                        path: &child_path(&gp, "title"),
                    },
                    "the group title",
                )
                .unwrap_or_default();
            let mut names = Vec::new();
            let vp = child_path(&gp, "values");
            for (j, n) in r
                .list(
                    At {
                        tree: &gm["values"],
                        path: &vp,
                    },
                    "the group's values",
                )
                .cloned()
                .unwrap_or_default()
                .iter()
                .enumerate()
            {
                let np = index_path(&vp, j);
                if let Some(name) = r.ident(At { tree: n, path: &np }, "a value") {
                    if !values.iter().any(|v| v.name == name) {
                        r.error(
                            Error::new(
                                hapto_expr::error::codes::E301,
                                format!("there is no value \"{name}\" to group"),
                            )
                            .at_path(&np),
                        );
                    }
                    names.push(name);
                }
            }
            c.groups.push((title, names));
        }
    }
    Some(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes_of(yaml: &str) -> Vec<&'static str> {
        read(yaml)
            .err()
            .unwrap_or_default()
            .iter()
            .map(|e| e.code.0)
            .collect()
    }

    const SMALLEST: &str = r#"
hapto: 2
id: square
alt: "The parabola y = x squared, with a point at x = {x:.1f}."
values:
  - { name: x, type: number, range: [-2, 2], default: 1 }
let:
  "f(t)": "t**2"
view: { x: { range: [-2.5, 2.5] }, y: { range: [-0.5, 4.5] } }
marks:
  - { function: "f(x)", var: x }
  - { point: "[x, f(x)]", drag: { x: "pointer.x" } }
"#;

    #[test]
    fn the_smallest_useful_figure_reads() {
        let doc = read(SMALLEST).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(doc.id.as_deref(), Some("square"));
        assert_eq!(doc.values.len(), 1);
        assert_eq!(doc.lets[0].0, "f(t)");
        assert_eq!(doc.lets[0].1.path, "/let/f(t)");
        assert_eq!(doc.views[0].id, "main");
        assert_eq!(doc.marks.len(), 2);
        assert!((doc.size.aspect - 16.0 / 9.0).abs() < 1e-12);
    }

    #[test]
    fn json_reads_the_same() {
        let json = r#"{"hapto": 2, "alt": "a", "view": {"x": {"range": [0, 1]}, "y": {"range": [0, 1]}}, "marks": [{"point": [0.5, 0.5]}]}"#;
        let doc = read(json).unwrap();
        assert_eq!(doc.marks[0].main.formula().unwrap().text, "[0.5, 0.5]");
    }

    #[test]
    fn the_version_decides() {
        assert_eq!(codes_of("alt: a\nmarks: []"), ["E407"]);
        let legacy = read("{\"kind\": \"plot\", \"curve\": \"x**2\"}").unwrap_err();
        assert!(legacy[0].hint.as_deref().unwrap().contains("legacy"));
        assert_eq!(codes_of("hapto: 3\nalt: a"), ["E403"]);
    }

    #[test]
    fn structure_is_strict() {
        assert_eq!(codes_of("hapto: 2\nalt: a\ntitel: x\nview: { x: { range: [0, 1] }, y: { range: [0, 1] } }\nmarks: [ { point: '[0, 0]' } ]"), ["E401"]);
        assert_eq!(codes_of("hapto: 2\nview: { x: { range: [0, 1] }, y: { range: [0, 1] } }\nmarks: [ { point: '[0, 0]' } ]"), ["E402"]);
        assert_eq!(
            codes_of("hapto: 2\nalt: a\nmarks: [ { point: '[0, 0]' } ]"),
            ["E402"]
        );
        assert_eq!(
            codes_of("hapto: 2\nalt: a\nview: { x: { range: [0, 1] }, y: { range: [0, 1] } }"),
            ["E402"]
        );
        let both = "hapto: 2\nalt: a\nview: { x: { range: [0, 1] }, y: { range: [0, 1] } }\nviews: []\nmarks: [ { point: '[0, 0]' } ]";
        assert!(codes_of(both).contains(&"E406"));
    }

    #[test]
    fn marks_find_their_views() {
        let two = "hapto: 2\nalt: a\nviews:\n  - { id: left, x: { range: [0, 1] }, y: { range: [0, 1] } }\n  - { id: right, x: { range: [0, 1] }, y: { range: [0, 1] } }\nmarks:\n";
        assert_eq!(
            codes_of(&format!("{two}  - {{ point: '[0, 0]' }}")),
            ["E409"]
        );
        assert_eq!(
            codes_of(&format!("{two}  - {{ point: '[0, 0]', view: middle }}")),
            ["E409"]
        );
        assert!(codes_of(&format!("{two}  - {{ point: '[0, 0]', view: left }}")).is_empty());
    }

    #[test]
    fn unbuilt_parts_say_when_they_land() {
        let e = read("hapto: 2\nalt: a\ncompute: {}\nview: { x: { range: [0, 1] }, y: { range: [0, 1] } }\nmarks: [ { point: '[0, 0]' } ]").unwrap_err();
        assert_eq!(e[0].code, codes::E610);
        assert!(e[0].hint.as_deref().unwrap().contains("M4"));
    }

    #[test]
    fn readouts_controls_and_imports() {
        let doc = read(
            "hapto: 2\nalt: a\nuses: [t, { name: fig1.x, as: x1 }]\nvalues: [ { name: k, type: number, range: [0, 1], default: 0 } ]\nview: { x: { range: [0, 1] }, y: { range: [0, 1] } }\nmarks: [ { point: '[k, 0]' } ]\nreadouts: [ { id: s, value: 'k * 2', format: '+.2f' } ]\ncontrols: { position: side, groups: [ { title: Init, values: [k] } ] }",
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(doc.uses[1].alias, "x1");
        assert_eq!(doc.readouts[0].format.as_deref(), Some("+.2f"));
        assert_eq!(doc.controls.position, "side");
        assert_eq!(codes_of("hapto: 2\nalt: a\nview: { x: { range: [0, 1] }, y: { range: [0, 1] } }\nmarks: [ { point: '[0, 0]' } ]\ncontrols: { groups: [ { title: g, values: [nope] } ] }"), ["E301"]);
    }

    #[test]
    fn every_error_is_reported_not_just_the_first() {
        let errors = read("hapto: 2\nalt: a\nvalues: [ { name: x, type: number, range: [1, 0], default: 0 } ]\nview: { x: { range: [0, 1] }, y: { range: [0, 1] } }\nmarks: [ { pont: '[0, 0]' }, { point: '[0, 0]', r: '2^3' } ]").unwrap_err();
        let codes: Vec<&str> = errors.iter().map(|e| e.code.0).collect();
        assert!(
            codes.contains(&"E502") && codes.contains(&"E404"),
            "{codes:?}"
        );
    }
}
