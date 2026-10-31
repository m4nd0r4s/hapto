//! Marks (SPEC-2.0 §8): what a figure draws, as a declarative table.
//!
//! Every kind is a row: its main argument and its options, each with the
//! shape it takes. The decoder and the JSON Schema read the same table, so
//! what the schema promises and what the decoder accepts cannot drift.

use serde_json::Value as Tree;

use hapto_expr::Error;

use super::formula::{colour, formula, width, Formula};
use crate::codes;
use crate::tree::{child_path, index_path, nearest, At, Reader};

/// The type a formula slot expects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ty {
    Num,
    Vec,
    Bool,
    Colour,
    ListNum,
    ListVec,
    /// A matrix or a list of rows.
    Any,
}

/// The shape a slot takes in the document.
#[derive(Debug, Clone, Copy)]
pub enum Spec {
    /// A formula (or a literal) of a type.
    F(Ty),
    /// A width in CSS pixels: a number, a formula or a named weight.
    Px,
    /// Text, with interpolation where text is shown.
    Text,
    /// A name the document gives or refers to.
    Ident,
    /// One of a fixed set of words.
    Word(&'static [&'static str]),
    /// A map with these keys; the flag says required.
    Map(&'static [(&'static str, Spec, bool)]),
    /// The first alternative whose shape matches.
    OneOf(&'static [Spec]),
    /// A list, each item of this shape.
    List(&'static Spec),
    /// Marks, nested (a group's children).
    Marks,
}

/// One mark kind: its name, main argument, options, and the milestone in
/// which it draws (the decoder validates every kind now).
#[derive(Debug, Clone, Copy)]
pub struct KindSpec {
    pub name: &'static str,
    pub main: Spec,
    pub options: &'static [(&'static str, Spec)],
    pub draws_in: &'static str,
}

const TEXT_OPTIONS: &[(&str, Spec)] = &[
    ("at", Spec::F(Ty::Vec)),
    ("anchor", Spec::Word(ANCHORS)),
    ("offset", Spec::F(Ty::Vec)),
    ("size", Spec::Px),
    ("weight", Spec::Word(&["regular", "bold"])),
    ("rotate", Spec::F(Ty::Num)),
    ("max_width", Spec::Px),
    ("background", Spec::Word(&["none", "box"])),
];

pub const ANCHORS: &[&str] = &[
    "top-left",
    "top",
    "top-right",
    "left",
    "center",
    "right",
    "bottom-left",
    "bottom",
    "bottom-right",
    "baseline-left",
    "baseline-center",
    "baseline-right",
];

const OVER: (&str, Spec) = ("over", Spec::List(&Spec::Ident));

const ENDPOINT: Spec = Spec::Map(&[("view", Spec::Ident, true), ("at", Spec::F(Ty::Vec), true)]);

const NODE: Spec = Spec::Map(&[
    ("id", Spec::Ident, true),
    ("at", Spec::F(Ty::Vec), false),
    ("label", Spec::Text, false),
    ("r", Spec::Px, false),
    ("name", Spec::Ident, false),
    ("drag", Spec::Map(&[]), false),
]);

const EDGE: Spec = Spec::Map(&[
    ("from", Spec::Ident, true),
    ("to", Spec::Ident, true),
    ("width", Spec::Px, false),
    ("label", Spec::Text, false),
    ("name", Spec::Ident, false),
    ("directed", Spec::F(Ty::Bool), false),
    ("colour", Spec::F(Ty::Colour), false),
]);

/// Every mark kind of §8.
pub const KINDS: &[KindSpec] = &[
    KindSpec {
        name: "point",
        main: Spec::F(Ty::Vec),
        options: &[
            ("r", Spec::Px),
            (
                "shape",
                Spec::Word(&["circle", "square", "diamond", "cross", "ring"]),
            ),
        ],
        draws_in: "M2",
    },
    KindSpec {
        name: "segment",
        main: Spec::F(Ty::ListVec),
        options: &[("arrows", Spec::Word(&["none", "end", "start", "both"]))],
        draws_in: "M2",
    },
    KindSpec {
        name: "arrow",
        main: Spec::F(Ty::ListVec),
        options: &[("head", Spec::Px), ("double", Spec::F(Ty::Bool))],
        draws_in: "M2",
    },
    KindSpec {
        name: "vector",
        main: Spec::F(Ty::Vec),
        options: &[("at", Spec::F(Ty::Vec)), ("head", Spec::Px)],
        draws_in: "M2",
    },
    KindSpec {
        name: "line",
        main: Spec::OneOf(&[
            Spec::F(Ty::ListVec),
            Spec::Map(&[
                ("through", Spec::F(Ty::Vec), true),
                ("direction", Spec::F(Ty::Vec), false),
                ("slope", Spec::F(Ty::Num), false),
            ]),
        ]),
        options: &[],
        draws_in: "M2",
    },
    KindSpec {
        name: "ray",
        main: Spec::F(Ty::ListVec),
        options: &[],
        draws_in: "M2",
    },
    KindSpec {
        name: "hline",
        main: Spec::F(Ty::Num),
        options: &[],
        draws_in: "M2",
    },
    KindSpec {
        name: "vline",
        main: Spec::F(Ty::Num),
        options: &[],
        draws_in: "M2",
    },
    KindSpec {
        name: "tangent",
        main: Spec::F(Ty::Num),
        options: &[("of", Spec::Ident), ("length", Spec::F(Ty::Num))],
        draws_in: "M2",
    },
    KindSpec {
        name: "secant",
        main: Spec::F(Ty::ListNum),
        options: &[("of", Spec::Ident)],
        draws_in: "M2",
    },
    KindSpec {
        name: "drop",
        main: Spec::F(Ty::Vec),
        options: &[("to", Spec::Word(&["x", "y", "both"]))],
        draws_in: "M2",
    },
    KindSpec {
        name: "polyline",
        main: Spec::F(Ty::ListVec),
        options: &[("closed", Spec::F(Ty::Bool))],
        draws_in: "M2",
    },
    KindSpec {
        name: "polygon",
        main: Spec::F(Ty::ListVec),
        options: &[],
        draws_in: "M2",
    },
    KindSpec {
        name: "circle",
        main: Spec::F(Ty::Vec),
        options: &[("r", Spec::F(Ty::Num))],
        draws_in: "M2",
    },
    KindSpec {
        name: "ellipse",
        main: Spec::F(Ty::Vec),
        options: &[
            ("rx", Spec::F(Ty::Num)),
            ("ry", Spec::F(Ty::Num)),
            ("tilt", Spec::F(Ty::Num)),
        ],
        draws_in: "M2",
    },
    KindSpec {
        name: "arc",
        main: Spec::F(Ty::Vec),
        options: &[
            ("r", Spec::F(Ty::Num)),
            ("from", Spec::F(Ty::Num)),
            ("to", Spec::F(Ty::Num)),
        ],
        draws_in: "M2",
    },
    KindSpec {
        name: "sector",
        main: Spec::F(Ty::Vec),
        options: &[
            ("r", Spec::F(Ty::Num)),
            ("inner", Spec::F(Ty::Num)),
            ("from", Spec::F(Ty::Num)),
            ("to", Spec::F(Ty::Num)),
        ],
        draws_in: "M2",
    },
    KindSpec {
        name: "angle",
        main: Spec::F(Ty::Vec),
        options: &[
            ("from", Spec::F(Ty::Vec)),
            ("to", Spec::F(Ty::Vec)),
            ("r", Spec::Px),
            ("right", Spec::Word(&["auto", "always", "never"])),
            ("reflex", Spec::F(Ty::Bool)),
        ],
        draws_in: "M2",
    },
    KindSpec {
        name: "path",
        main: Spec::Text,
        options: &[("closed", Spec::F(Ty::Bool))],
        draws_in: "M2",
    },
    KindSpec {
        name: "bezier",
        main: Spec::F(Ty::ListVec),
        options: &[],
        draws_in: "M2",
    },
    KindSpec {
        name: "brace",
        main: Spec::F(Ty::ListVec),
        options: &[
            ("side", Spec::Word(&["left", "right"])),
            ("depth", Spec::Px),
        ],
        draws_in: "M2",
    },
    KindSpec {
        name: "spring",
        main: Spec::F(Ty::ListVec),
        options: &[("coils", Spec::F(Ty::Num)), ("width", Spec::Px)],
        draws_in: "M2",
    },
    KindSpec {
        name: "area",
        main: Spec::Map(&[
            ("under", Spec::F(Ty::Num), false),
            ("between", Spec::F(Ty::ListNum), false),
            ("var", Spec::Ident, false),
            ("from", Spec::F(Ty::Num), false),
            ("to", Spec::F(Ty::Num), false),
        ]),
        options: &[("signed", Spec::F(Ty::Bool))],
        draws_in: "M2",
    },
    KindSpec {
        name: "rect",
        main: Spec::OneOf(&[
            Spec::F(Ty::Vec),
            Spec::Map(&[
                ("from", Spec::F(Ty::Vec), true),
                ("to", Spec::F(Ty::Vec), true),
            ]),
        ]),
        options: &[
            ("w", Spec::F(Ty::Num)),
            ("h", Spec::F(Ty::Num)),
            ("radius", Spec::Px),
        ],
        draws_in: "M2",
    },
    KindSpec {
        name: "box",
        main: Spec::F(Ty::Vec),
        options: &[
            ("w", Spec::F(Ty::Num)),
            ("h", Spec::F(Ty::Num)),
            ("text", Spec::Text),
            ("radius", Spec::Px),
            ("padding", Spec::Px),
        ],
        draws_in: "M2",
    },
    KindSpec {
        name: "text",
        main: Spec::Text,
        options: TEXT_OPTIONS,
        draws_in: "M2",
    },
    KindSpec {
        name: "math",
        main: Spec::Text,
        options: TEXT_OPTIONS,
        draws_in: "M2",
    },
    KindSpec {
        name: "function",
        main: Spec::OneOf(&[
            Spec::F(Ty::Num),
            Spec::Map(&[("x", Spec::F(Ty::Num), true), ("y", Spec::F(Ty::Num), true)]),
        ]),
        options: &[
            ("var", Spec::Ident),
            ("domain", Spec::F(Ty::ListNum)),
            (
                "samples",
                Spec::OneOf(&[Spec::Word(&["auto"]), Spec::F(Ty::Num)]),
            ),
        ],
        draws_in: "M2",
    },
    KindSpec {
        name: "group",
        main: Spec::Marks,
        options: &[
            ("translate", Spec::F(Ty::Vec)),
            ("rotate", Spec::F(Ty::Num)),
            ("scale", Spec::F(Ty::Any)),
            ("origin", Spec::F(Ty::Vec)),
        ],
        draws_in: "M2",
    },
    KindSpec {
        name: "scatter",
        main: Spec::F(Ty::ListVec),
        options: &[("r", Spec::Px), ("colour", Spec::F(Ty::Colour))],
        draws_in: "M4",
    },
    KindSpec {
        name: "bars",
        main: Spec::F(Ty::ListNum),
        options: &[
            ("at", Spec::F(Ty::ListNum)),
            ("width", Spec::F(Ty::Num)),
            ("base", Spec::F(Ty::Num)),
        ],
        draws_in: "M4",
    },
    KindSpec {
        name: "histogram",
        main: Spec::F(Ty::ListNum),
        options: &[
            ("bins", Spec::F(Ty::Num)),
            ("range", Spec::F(Ty::ListNum)),
            ("density", Spec::F(Ty::Bool)),
        ],
        draws_in: "M4",
    },
    KindSpec {
        name: "stems",
        main: Spec::F(Ty::ListVec),
        options: &[],
        draws_in: "M4",
    },
    KindSpec {
        name: "trace",
        main: Spec::Ident,
        options: &[
            ("x", Spec::Ident),
            ("y", Spec::Ident),
            ("fade", Spec::F(Ty::Bool)),
            ("colour_by", Spec::Ident),
        ],
        draws_in: "M4",
    },
    KindSpec {
        name: "connector",
        main: Spec::Map(&[("from", ENDPOINT, true), ("to", ENDPOINT, true)]),
        options: &[],
        draws_in: "M6",
    },
    KindSpec {
        name: "graph",
        main: Spec::Map(&[
            ("nodes", Spec::List(&NODE), true),
            ("edges", Spec::List(&EDGE), false),
            ("node_r", Spec::Px, false),
            (
                "layout",
                Spec::Word(&["explicit", "circle", "line", "grid"]),
                false,
            ),
        ]),
        options: &[],
        draws_in: "M5",
    },
    KindSpec {
        name: "heatmap",
        main: Spec::F(Ty::Num),
        options: &[
            ("scale", Spec::Word(hapto_expr::builtins::SCALES)),
            ("range", Spec::F(Ty::ListNum)),
            ("cell", Spec::Px),
            ("zero", Spec::F(Ty::Bool)),
            OVER,
        ],
        draws_in: "M5",
    },
    KindSpec {
        name: "contours",
        main: Spec::F(Ty::Num),
        options: &[
            ("levels", Spec::F(Ty::Any)),
            ("labels", Spec::F(Ty::Bool)),
            ("highlight_level", Spec::F(Ty::Num)),
            OVER,
        ],
        draws_in: "M5",
    },
    KindSpec {
        name: "vector_field",
        main: Spec::F(Ty::Vec),
        options: &[
            ("density", Spec::Px),
            ("normalize", Spec::F(Ty::Bool)),
            ("scale", Spec::F(Ty::Num)),
            OVER,
        ],
        draws_in: "M5",
    },
    KindSpec {
        name: "streamlines",
        main: Spec::F(Ty::Vec),
        options: &[
            ("seeds", Spec::F(Ty::ListVec)),
            ("count", Spec::F(Ty::Num)),
            ("length", Spec::F(Ty::Num)),
            ("arrows", Spec::F(Ty::Bool)),
            OVER,
        ],
        draws_in: "M5",
    },
    KindSpec {
        name: "matrix",
        main: Spec::F(Ty::Any),
        options: &[
            ("at", Spec::F(Ty::Vec)),
            ("cell", Spec::Px),
            ("format", Spec::Text),
            ("fill_by", Spec::Word(&["none", "value"])),
            ("scale", Spec::Word(hapto_expr::builtins::SCALES)),
            ("brackets", Spec::F(Ty::Bool)),
            ("row_labels", Spec::List(&Spec::Text)),
            ("col_labels", Spec::List(&Spec::Text)),
            ("name_cells", Spec::Ident),
        ],
        draws_in: "M5",
    },
];

/// Keys every mark accepts (§8.1).
pub const COMMON: &[&str] = &[
    "view",
    "name",
    "focus_group",
    "when",
    "style",
    "label",
    "legend",
    "z",
    "repeat",
    "drag",
    "highlight",
    "id",
];

pub const STYLE_KEYS: &[&str] = &[
    "stroke",
    "width",
    "fill",
    "fill_opacity",
    "opacity",
    "dash",
    "cap",
    "join",
    "halo",
];

pub fn kind(name: &str) -> Option<&'static KindSpec> {
    KINDS.iter().find(|k| k.name == name)
}

/// A decoded slot.
#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    Formula(Formula),
    Text(String),
    Ident(String),
    Word(String),
    Map(Vec<(String, Arg)>),
    List(Vec<Arg>),
    Marks(Vec<Mark>),
}

impl Arg {
    pub fn formula(&self) -> Option<&Formula> {
        match self {
            Arg::Formula(f) => Some(f),
            _ => None,
        }
    }

    pub fn get(&self, key: &str) -> Option<&Arg> {
        match self {
            Arg::Map(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Dash {
    Solid,
    Dashed,
    Dotted,
    Pattern(Vec<f64>),
}

/// How a mark is painted; every field a formula (§9.2).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Style {
    pub stroke: Option<Formula>,
    pub width: Option<Formula>,
    pub fill: Option<Formula>,
    pub fill_opacity: Option<Formula>,
    pub opacity: Option<Formula>,
    pub dash: Option<Dash>,
    pub cap: Option<String>,
    pub join: Option<String>,
    pub halo: Option<Formula>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub text: String,
    pub at: Option<String>,
    pub offset: Option<Formula>,
    pub anchor: Option<String>,
    pub size: Option<Formula>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Drag {
    Off,
    Assign(Vec<(String, Formula)>),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Common {
    pub view: Option<String>,
    pub name: Option<String>,
    pub focus_group: Option<String>,
    pub when: Option<Formula>,
    pub style: Style,
    pub label: Option<Label>,
    pub legend: Option<String>,
    pub z: Option<f64>,
    pub repeat: Vec<(String, Formula)>,
    pub drag: Option<Drag>,
    pub highlight: Option<Style>,
    pub id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    pub kind: &'static str,
    pub main: Arg,
    pub options: Vec<(String, Arg)>,
    pub common: Common,
    pub path: String,
}

impl Mark {
    pub fn option(&self, key: &str) -> Option<&Arg> {
        self.options.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
}

/// Whether a tree value has the shape a spec asks for (for `OneOf`).
fn shape_fits(spec: &Spec, t: &Tree) -> bool {
    match spec {
        Spec::Map(_) => t.is_object(),
        Spec::F(_) | Spec::Px => !t.is_object() && !t.is_null(),
        Spec::Text | Spec::Ident | Spec::Word(_) => t.is_string(),
        Spec::List(_) | Spec::Marks => t.is_array(),
        Spec::OneOf(options) => options.iter().any(|o| shape_fits(o, t)),
    }
}

pub fn read_slot(r: &mut Reader, spec: &Spec, at: At, what: &str) -> Option<Arg> {
    Some(match spec {
        Spec::F(Ty::Colour) => Arg::Formula(colour(r, at, what)?),
        Spec::F(_) => Arg::Formula(formula(r, at, what)?),
        Spec::Px => Arg::Formula(width(r, at, what)?),
        Spec::Text => Arg::Text(r.string(at, what)?),
        Spec::Ident => Arg::Ident(r.ident(at, what)?),
        Spec::Word(options) => Arg::Word(r.choice(at, options, what)?),
        Spec::Map(keys) => {
            let known: Vec<&str> = keys.iter().map(|(k, _, _)| *k).collect();
            let required: Vec<&str> = keys
                .iter()
                .filter(|(_, _, req)| *req)
                .map(|(k, _, _)| *k)
                .collect();
            let map = r.map(at, &known, &required, what)?;
            let mut out = Vec::new();
            for (key, value) in map {
                let Some((_, spec, _)) = keys.iter().find(|(k, _, _)| *k == key.as_str()) else {
                    continue;
                };
                let path = child_path(at.path, key);
                if let Some(arg) = read_slot(
                    r,
                    spec,
                    At {
                        tree: value,
                        path: &path,
                    },
                    key,
                ) {
                    out.push((key.clone(), arg));
                }
            }
            Arg::Map(out)
        }
        Spec::OneOf(options) => {
            let spec = options
                .iter()
                .find(|o| shape_fits(o, at.tree))
                .unwrap_or(&options[0]);
            read_slot(r, spec, at, what)?
        }
        Spec::List(item) => {
            let items = r.list(at, what)?;
            let mut out = Vec::new();
            for (i, t) in items.iter().enumerate() {
                let path = index_path(at.path, i);
                out.push(read_slot(
                    r,
                    item,
                    At {
                        tree: t,
                        path: &path,
                    },
                    what,
                )?);
            }
            Arg::List(out)
        }
        Spec::Marks => Arg::Marks(read_marks(r, at)),
    })
}

pub fn read_marks(r: &mut Reader, at: At) -> Vec<Mark> {
    let Some(items) = r.list(at, "the marks") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let path = index_path(at.path, i);
        if let Some(m) = read_mark(
            r,
            At {
                tree: item,
                path: &path,
            },
        ) {
            out.push(m);
        }
    }
    out
}

fn read_mark(r: &mut Reader, at: At) -> Option<Mark> {
    let Tree::Object(map) = at.tree else {
        r.wrong_type(at, "a mark (a map such as `{ point: \"[x, y]\" }`)");
        return None;
    };
    let kinds: Vec<&str> = map
        .keys()
        .filter(|k| kind(k).is_some())
        .map(String::as_str)
        .collect();
    let spec = match kinds.as_slice() {
        [one] => kind(one)?,
        [] => {
            let names: Vec<&str> = KINDS.iter().map(|k| k.name).collect();
            let guess = map
                .keys()
                .filter(|k| !COMMON.contains(&k.as_str()))
                .find_map(|k| nearest(k, &names));
            let mut err =
                Error::new(codes::E404, "this mark does not say what it draws").at_path(at.path);
            err = match guess {
                Some(g) => err.hint(format!("did you mean `{g}`?")),
                None => err
                    .hint("start the mark with its kind, as in `point: \"[x, y]\"`")
                    .options(names),
            };
            r.error(err);
            return None;
        }
        several => {
            r.error(
                Error::new(
                    codes::E408,
                    format!(
                        "a mark draws one thing, and this names {}",
                        several.join(" and ")
                    ),
                )
                .hint("split it into one mark per kind")
                .at_path(at.path),
            );
            return None;
        }
    };

    let mut known: Vec<&str> = vec![spec.name];
    known.extend(spec.options.iter().map(|(k, _)| *k));
    known.extend(COMMON);
    r.map(at, &known, &[], &format!("a {} mark", spec.name))?;

    let main_path = child_path(at.path, spec.name);
    let main = read_slot(
        r,
        &spec.main,
        At {
            tree: &map[spec.name],
            path: &main_path,
        },
        spec.name,
    )?;
    let mut options = Vec::new();
    for (key, slot) in spec.options {
        let written = if map.contains_key(*key) {
            key.to_string()
        } else {
            key.replace("colour", "color")
        };
        if let Some(t) = map.get(&written) {
            let path = child_path(at.path, &written);
            if let Some(arg) = read_slot(
                r,
                slot,
                At {
                    tree: t,
                    path: &path,
                },
                key,
            ) {
                options.push((key.to_string(), arg));
            }
        }
    }
    let common = read_common(r, at, map);
    Some(Mark {
        kind: spec.name,
        main,
        options,
        common,
        path: at.path.to_string(),
    })
}

fn read_common(r: &mut Reader, at: At, map: &serde_json::Map<String, Tree>) -> Common {
    let mut c = Common::default();
    let at_key = |key: &str| child_path(at.path, key);
    for (key, value) in map {
        let path = at_key(key);
        let here = At {
            tree: value,
            path: &path,
        };
        match key.as_str() {
            "view" => c.view = r.ident(here, "the view"),
            "name" => c.name = r.ident(here, "the mark's name"),
            "focus_group" => c.focus_group = r.ident(here, "the focus group"),
            "id" => c.id = r.ident(here, "the mark's id"),
            "when" => c.when = formula(r, here, "when"),
            "legend" => c.legend = r.string(here, "the legend entry"),
            "z" => c.z = r.number(here, "the drawing order"),
            "style" => c.style = read_style(r, here).unwrap_or_default(),
            "highlight" => c.highlight = read_style(r, here),
            "label" => c.label = read_label(r, here),
            "repeat" => c.repeat = read_assignments(r, here, "repeat", 2),
            "drag" => {
                c.drag = match value {
                    Tree::Bool(false) => Some(Drag::Off),
                    _ => Some(Drag::Assign(read_assignments(r, here, "drag", usize::MAX))),
                }
            }
            _ => {}
        }
    }
    c
}

/// `{ name: formula, … }`: drag assignments and repeat variables.
fn read_assignments(r: &mut Reader, at: At, what: &str, max: usize) -> Vec<(String, Formula)> {
    let Tree::Object(map) = at.tree else {
        r.wrong_type(at, &format!("a map of names to formulas ({what})"));
        return Vec::new();
    };
    if map.len() > max {
        r.error(
            Error::new(codes::E403, format!("{what} binds at most {max} variables"))
                .at_path(at.path),
        );
    }
    let mut out = Vec::new();
    for (name, value) in map {
        let path = child_path(at.path, name);
        if !crate::tree::is_ident(name) {
            r.error(
                Error::new(codes::E403, format!("\"{name}\" is not a name ({what})"))
                    .at_path(&path),
            );
            continue;
        }
        if let Some(f) = formula(
            r,
            At {
                tree: value,
                path: &path,
            },
            what,
        ) {
            out.push((name.clone(), f));
        }
    }
    out
}

pub fn read_style(r: &mut Reader, at: At) -> Option<Style> {
    let map = r.map(at, STYLE_KEYS, &[], "a style")?;
    let mut s = Style::default();
    for (key, value) in map {
        let path = child_path(at.path, key);
        let here = At {
            tree: value,
            path: &path,
        };
        match crate::tree::alias(key).as_str() {
            "stroke" => s.stroke = colour(r, here, "the stroke colour"),
            "fill" => s.fill = colour(r, here, "the fill colour"),
            "width" => s.width = width(r, here, "the stroke width"),
            "fill_opacity" => s.fill_opacity = formula(r, here, "the fill opacity"),
            "opacity" => s.opacity = formula(r, here, "the opacity"),
            "halo" => s.halo = formula(r, here, "the halo"),
            "cap" => s.cap = r.choice(here, &["round", "butt", "square"], "line cap"),
            "join" => s.join = r.choice(here, &["round", "miter", "bevel"], "line join"),
            "dash" => {
                s.dash = match value {
                    Tree::String(d) => match d.as_str() {
                        "solid" => Some(Dash::Solid),
                        "dashed" => Some(Dash::Dashed),
                        "dotted" => Some(Dash::Dotted),
                        _ => {
                            r.choice(here, &["solid", "dashed", "dotted"], "dash");
                            None
                        }
                    },
                    Tree::Array(items) => {
                        let mut pattern = Vec::new();
                        for (i, t) in items.iter().enumerate() {
                            if let Some(n) = r.number(
                                At {
                                    tree: t,
                                    path: &index_path(&path, i),
                                },
                                "a dash length in px",
                            ) {
                                pattern.push(n);
                            }
                        }
                        Some(Dash::Pattern(pattern))
                    }
                    _ => {
                        r.wrong_type(here, "solid, dashed, dotted or a list of lengths");
                        None
                    }
                }
            }
            _ => {}
        }
    }
    Some(s)
}

fn read_label(r: &mut Reader, at: At) -> Option<Label> {
    if let Tree::String(text) = at.tree {
        return Some(Label {
            text: text.clone(),
            at: None,
            offset: None,
            anchor: None,
            size: None,
        });
    }
    let map = r.map(
        at,
        &["text", "at", "offset", "anchor", "size"],
        &["text"],
        "a label",
    )?;
    let get = |r: &mut Reader, key: &str, f: fn(&mut Reader, At, &str) -> Option<Formula>| {
        map.get(key).and_then(|t| {
            f(
                r,
                At {
                    tree: t,
                    path: &child_path(at.path, key),
                },
                key,
            )
        })
    };
    Some(Label {
        text: r.string(
            At {
                tree: map.get("text")?,
                path: &child_path(at.path, "text"),
            },
            "the label text",
        )?,
        at: map.get("at").and_then(|t| {
            r.choice(
                At {
                    tree: t,
                    path: &child_path(at.path, "at"),
                },
                &["end", "mid", "start"],
                "label position",
            )
        }),
        offset: get(r, "offset", formula),
        anchor: map.get("anchor").and_then(|t| {
            r.choice(
                At {
                    tree: t,
                    path: &child_path(at.path, "anchor"),
                },
                ANCHORS,
                "anchor",
            )
        }),
        size: get(r, "size", width),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::parse;

    fn read(yaml: &str) -> (Vec<Mark>, Vec<&'static str>) {
        let tree = parse(yaml).unwrap();
        let mut r = Reader::default();
        let marks = read_marks(
            &mut r,
            At {
                tree: &tree,
                path: "/marks",
            },
        );
        (marks, r.errors.iter().map(|e| e.code.0).collect())
    }

    #[test]
    fn marks_read_their_main_argument_and_options() {
        let (m, errors) = read(
            r#"- { point: "[x, f(x)]", r: 4, drag: { x: "pointer.x" }, label: "$P$", style: { fill: hot, width: heavy, dash: [4, 2] } }
- { function: "f(x)", var: x, domain: [a, b], legend: "$f$" }
- { function: { x: "cos(t)", y: "sin(t)" }, var: t }
- { segment: ["[0, 0]", "[x, y]"], arrows: end }
- { line: { through: "[0, 1]", slope: 2 } }
- { group: [ { rect: "[0, 0]", w: 1, h: 1 } ], rotate: theta, when: on }
- { text: "slope = {s:.2f}", at: "[1, 2]", anchor: bottom-left }"#,
        );
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(m.len(), 7);
        assert_eq!(m[0].main.formula().unwrap().text, "[x, f(x)]");
        assert_eq!(
            m[0].common.style.fill.as_ref().unwrap().text,
            "palette(\"hot\")"
        );
        assert_eq!(m[0].common.style.width.as_ref().unwrap().text, "3.5");
        assert_eq!(m[0].common.style.dash, Some(Dash::Pattern(vec![4.0, 2.0])));
        assert!(matches!(&m[0].common.drag, Some(Drag::Assign(a)) if a[0].0 == "x"));
        assert_eq!(
            m[1].option("domain").unwrap().formula().unwrap().text,
            "[a, b]"
        );
        assert!(m[2].main.get("y").is_some());
        assert_eq!(m[3].main.formula().unwrap().text, "[[0, 0], [x, y]]");
        assert!(m[4].main.get("through").is_some());
        assert!(matches!(&m[5].main, Arg::Marks(children) if children.len() == 1));
        assert!(matches!(&m[6].main, Arg::Text(t) if t.contains("{s:.2f}")));
    }

    #[test]
    fn a_mark_names_one_kind() {
        assert_eq!(read("- { pont: '[0, 0]' }").1, ["E404"]);
        assert_eq!(read("- { point: '[0, 0]', circle: '[0, 0]' }").1, ["E408"]);
        assert_eq!(read("- { view: main }").1, ["E404"]);
        // `group` is a kind (§8.14); the focus key is `focus_group` (§8.1).
        let grouped = read("- { group: [ { point: '[0, 0]', focus_group: g } ] }");
        assert!(grouped.1.is_empty(), "{:?}", grouped.1);
    }

    #[test]
    fn options_belong_to_their_kind() {
        assert_eq!(read("- { point: '[0, 0]', head: 3 }").1, ["E401"]);
        assert_eq!(read("- { point: '[0, 0]', shape: star }").1, ["E403"]);
        assert_eq!(
            read("- { point: '[0, 0]', style: { stroke_width: 2 } }").1,
            ["E401"]
        );
        assert_eq!(read("- { line: { slope: 2 } }").1, ["E402"]);
        assert_eq!(
            read("- { point: '[0, 0]', drag: { 'x y': 1 } }").1,
            ["E403"]
        );
        assert_eq!(
            read("- { point: '[0, 0]', repeat: { i: 'range(0, 3)', j: 'xs', k: 'ys' } }").1,
            ["E403"]
        );
    }

    #[test]
    fn colour_is_an_alias_of_colour() {
        assert_eq!(
            read("- { scatter: 'pts', color: hot }").1,
            Vec::<&str>::new()
        );
    }

    #[test]
    fn every_kind_has_a_row() {
        for k in KINDS {
            assert!(!k.name.is_empty());
            assert!(["M2", "M4", "M5", "M6"].contains(&k.draws_in));
        }
        assert_eq!(KINDS.len(), 41);
    }
}
