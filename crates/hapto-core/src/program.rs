//! A document compiled against its own names: the program a figure runs.
//!
//! Every formula slot is type-checked against the type its field expects
//! (SPEC-2.0 §4.7), with the variables its context binds — a function mark's
//! `x`, a field's `x` and `y`, a repeat's `i`, a drag's `pointer` (§4.4) —
//! and every mark knows which values it reads, so a change re-evaluates only
//! what it reaches (§3.3).

use std::collections::BTreeSet;

use hapto_expr::check::Checked;
use hapto_expr::deps::Deps;
use hapto_expr::error::codes as expr_codes;
use hapto_expr::{Declarations, Error, Module, Type};

use crate::codes;
use crate::document::marks::{kind, Spec, Ty};
use crate::document::{Arg, Document, Drag, Formula, Mark, Readout, Style};
use crate::state::{type_of, State};

/// A compiled formula, with the names bound in its first local slots.
#[derive(Debug, Clone)]
pub struct Compiled {
    pub checked: Checked,
    pub bound: Vec<String>,
    pub path: String,
}

#[derive(Debug, Clone)]
pub enum Slot {
    Formula(Compiled),
    Text(String),
    Ident(String),
    Word(String),
    Map(Vec<(String, Slot)>),
    List(Vec<Slot>),
    Marks(Vec<CompiledMark>),
}

impl Slot {
    pub fn formula(&self) -> Option<&Compiled> {
        match self {
            Slot::Formula(c) => Some(c),
            _ => None,
        }
    }

    pub fn get(&self, key: &str) -> Option<&Slot> {
        match self {
            Slot::Map(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CompiledStyle {
    pub stroke: Option<Compiled>,
    pub width: Option<Compiled>,
    pub fill: Option<Compiled>,
    pub fill_opacity: Option<Compiled>,
    pub opacity: Option<Compiled>,
    pub halo: Option<Compiled>,
}

/// A piece of compiled text: literal, or a formula printed with a spec.
#[derive(Debug, Clone)]
pub enum TextPiece {
    Lit(String),
    Expr(Compiled, Option<String>),
}

#[derive(Debug, Clone)]
pub struct CompiledMark {
    /// The mark as written: kind, words, dash, label text.
    pub source: Mark,
    pub main: Slot,
    pub options: Vec<(String, Slot)>,
    pub when: Option<Compiled>,
    pub style: CompiledStyle,
    pub highlight: Option<CompiledStyle>,
    /// Repeat variables and the lists they run over.
    pub repeat: Vec<(String, Compiled)>,
    /// Drag assignments: the value's index and its formula.
    pub drag: Vec<(usize, Compiled)>,
    /// Everything this mark reads, transitively.
    pub deps: Deps,
    /// Text templates: `main` (text marks), `label`, `text` (boxes).
    pub texts: Vec<(String, Vec<TextPiece>)>,
}

impl CompiledMark {
    pub fn option(&self, key: &str) -> Option<&Slot> {
        self.options.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
}

#[derive(Debug, Clone)]
pub struct CompiledAxis {
    pub range: [Compiled; 2],
}

#[derive(Debug, Clone)]
pub struct CompiledReadout {
    pub source: Readout,
    pub value: Compiled,
    pub compare: Option<Compiled>,
}

#[derive(Debug)]
pub struct Program {
    pub doc: Document,
    pub module: Module,
    pub state: State,
    pub marks: Vec<CompiledMark>,
    /// Per view: the x and y axis ranges.
    pub axes: Vec<(CompiledAxis, CompiledAxis)>,
    pub readouts: Vec<CompiledReadout>,
    pub warnings: Vec<Error>,
}

fn expected(ty: Ty) -> Option<Type> {
    Some(match ty {
        Ty::Num => Type::Num,
        Ty::Vec => Type::Vec,
        Ty::Bool => Type::Bool,
        Ty::Colour => Type::Colour,
        Ty::ListNum => Type::list(Type::Num),
        Ty::ListVec => Type::list(Type::Vec),
        Ty::Any => return None,
    })
}

/// Names a drag formula may read (§12.4).
const DRAG_CONTEXT: &[&str] = &["pointer", "start", "anchor0", "delta"];

struct Compiler<'a> {
    module: &'a mut Module,
    state: &'a State,
    errors: Vec<Error>,
    warnings: Vec<Error>,
}

impl Compiler<'_> {
    /// Compile a text template's interpolations (§10.2).
    fn template(
        &mut self,
        template: &str,
        path: &str,
        bound: &[(String, Type)],
    ) -> Option<Vec<TextPiece>> {
        let pieces = match crate::text::parse(template, path) {
            Ok(p) => p,
            Err(e) => {
                self.errors.push(e);
                return None;
            }
        };
        let mut out = Vec::new();
        for piece in pieces {
            out.push(match piece {
                crate::text::Piece::Lit(l) => TextPiece::Lit(l),
                crate::text::Piece::Expr { formula, spec } => TextPiece::Expr(
                    self.formula(&Formula::new(formula, path), None, bound)?,
                    spec,
                ),
            });
        }
        Some(out)
    }

    fn formula(
        &mut self,
        f: &Formula,
        ty: Option<Type>,
        bound: &[(String, Type)],
    ) -> Option<Compiled> {
        match self.module.compile(&f.text, bound, ty.as_ref()) {
            Ok(checked) => {
                for mut w in checked.warnings.clone() {
                    w.path = Some(f.path.clone());
                    self.warnings.push(w);
                }
                Some(Compiled {
                    checked,
                    bound: bound.iter().map(|(n, _)| n.clone()).collect(),
                    path: f.path.clone(),
                })
            }
            Err(e) => {
                self.errors.push(e.at_path(&f.path));
                None
            }
        }
    }

    fn slot(&mut self, spec: &Spec, arg: &Arg, bound: &[(String, Type)]) -> Option<Slot> {
        Some(match (spec, arg) {
            (Spec::F(ty), Arg::Formula(f)) => {
                Slot::Formula(self.formula(f, expected(*ty), bound)?)
            }
            (Spec::Px, Arg::Formula(f)) => {
                Slot::Formula(self.formula(f, Some(Type::Num), bound)?)
            }
            (_, Arg::Text(t)) => Slot::Text(t.clone()),
            (_, Arg::Ident(i)) => Slot::Ident(i.clone()),
            (_, Arg::Word(w)) => Slot::Word(w.clone()),
            (Spec::Map(keys), Arg::Map(entries)) => {
                let mut out = Vec::new();
                for (k, v) in entries {
                    let spec = keys
                        .iter()
                        .find(|(name, _, _)| name == k)
                        .map(|(_, s, _)| *s)?;
                    out.push((k.clone(), self.slot(&spec, v, bound)?));
                }
                Slot::Map(out)
            }
            (Spec::OneOf(options), arg) => {
                let spec = options
                    .iter()
                    .find(|o| matches_arg(o, arg))
                    .unwrap_or(&options[0]);
                self.slot(spec, arg, bound)?
            }
            (Spec::List(item), Arg::List(items)) => Slot::List(
                items
                    .iter()
                    .map(|i| self.slot(item, i, bound))
                    .collect::<Option<_>>()?,
            ),
            (Spec::Marks, Arg::Marks(marks)) => {
                Slot::Marks(marks.iter().filter_map(|m| self.mark(m, bound)).collect())
            }
            _ => return None,
        })
    }

    fn style(&mut self, s: &Style, bound: &[(String, Type)]) -> CompiledStyle {
        let mut f = |f: &Option<Formula>, ty: Type| {
            f.as_ref().and_then(|f| self.formula(f, Some(ty), bound))
        };
        CompiledStyle {
            stroke: f(&s.stroke, Type::Colour),
            width: f(&s.width, Type::Num),
            fill: f(&s.fill, Type::Colour),
            fill_opacity: f(&s.fill_opacity, Type::Num),
            opacity: f(&s.opacity, Type::Num),
            halo: f(&s.halo, Type::Bool),
        }
    }

    /// The variables a kind binds for its own formulas (§4.4).
    fn bound_for(&self, m: &Mark, key: &str) -> Vec<(String, Type)> {
        let var = |default: &str| match m.option("var").or_else(|| m.main.get("var")) {
            Some(Arg::Ident(v)) => v.clone(),
            _ => default.to_string(),
        };
        match (m.kind, key) {
            ("function", "main") => vec![(
                var(if matches!(m.main, Arg::Map(_)) {
                    "t"
                } else {
                    "x"
                }),
                Type::Num,
            )],
            ("area", "main") => vec![(var("x"), Type::Num)],
            ("scatter", "r" | "colour") => vec![("i".into(), Type::Num), ("p".into(), Type::Vec)],
            ("heatmap" | "contours" | "vector_field" | "streamlines", "main") => {
                let names = match m.option("over") {
                    Some(Arg::List(items)) => items
                        .iter()
                        .filter_map(|a| match a {
                            Arg::Ident(i) => Some(i.clone()),
                            _ => None,
                        })
                        .collect(),
                    _ => vec!["x".to_string(), "y".to_string()],
                };
                names.into_iter().map(|n| (n, Type::Num)).collect()
            }
            _ => vec![],
        }
    }

    fn mark(&mut self, m: &Mark, outer: &[(String, Type)]) -> Option<CompiledMark> {
        let errors_before = self.errors.len();
        let spec = kind(m.kind)?;

        // Repeat variables first: their lists decide their types.
        let mut bound = outer.to_vec();
        let mut repeat = Vec::new();
        for (var, f) in &m.common.repeat {
            if let Some(c) = self.formula(f, None, &bound) {
                let elem = match &c.checked.ty {
                    Type::List(inner) => (**inner).clone(),
                    other => {
                        self.errors.push(
                            Error::new(
                                expr_codes::E201,
                                format!("a repeat runs over a list, and this is a {other}"),
                            )
                            .hint("use `range(0, n)` or a list of points")
                            .at_path(&f.path),
                        );
                        continue;
                    }
                };
                bound.push((var.clone(), elem.settled()));
                repeat.push((var.clone(), c));
            }
        }

        let main_bound = [bound.clone(), self.bound_for(m, "main")].concat();
        let main = self.slot(&spec.main, &m.main, &main_bound);
        let mut options = Vec::new();
        for (key, arg) in &m.options {
            let Some(slot_spec) = spec.options.iter().find(|(k, _)| k == key).map(|(_, s)| *s)
            else {
                continue;
            };
            if key == "over" || key == "var" {
                options.push((key.clone(), self.slot(&slot_spec, arg, &bound)?));
                continue;
            }
            let b = [bound.clone(), self.bound_for(m, key)].concat();
            if let Some(slot) = self.slot(&slot_spec, arg, &b) {
                options.push((key.clone(), slot));
            }
        }
        self.check_functions(m);
        // A tangent or secant reads its function through formulas built here,
        // so drawing never calls a function outside the evaluator.
        if let (Some(Arg::Ident(f)), Arg::Formula(main)) = (m.option("of"), &m.main) {
            if self.module.functions().iter().any(|func| &func.name == f) {
                let x = &main.text;
                let synthesized: Vec<(&str, String, Type)> = match m.kind {
                    "tangent" => vec![
                        ("_y", format!("{f}(({x}))"), Type::Num),
                        ("_slope", format!("derivative({f}, ({x}))"), Type::Num),
                    ],
                    "secant" => vec![(
                        "_ys",
                        format!("[{f}(({x})[0]), {f}(({x})[1])]"),
                        Type::list(Type::Num),
                    )],
                    _ => vec![],
                };
                for (key, text, ty) in synthesized {
                    if let Some(c) =
                        self.formula(&Formula::new(text, &main.path), Some(ty), &main_bound)
                    {
                        options.push((key.to_string(), Slot::Formula(c)));
                    }
                }
            }
        }

        let when = m
            .common
            .when
            .as_ref()
            .and_then(|f| self.formula(f, Some(Type::Bool), &bound));
        let style = self.style(&m.common.style, &bound);
        let highlight = m.common.highlight.as_ref().map(|s| self.style(s, &bound));

        let mut drag = Vec::new();
        if let Some(Drag::Assign(assignments)) = &m.common.drag {
            let mut ctx = bound.clone();
            ctx.extend(DRAG_CONTEXT.iter().map(|n| (n.to_string(), Type::Vec)));
            if m.kind == "vector" {
                ctx.push(("at".into(), Type::Vec));
            }
            for (name, f) in assignments {
                let Some(i) = self.state.index(name) else {
                    self.errors.push(
                        Error::new(
                            codes::E505,
                            format!("this drag sets \"{name}\", and there is no such value"),
                        )
                        .options(self.state.decls().iter().map(|d| d.name.clone()))
                        .at_path(&f.path),
                    );
                    continue;
                };
                let ty = type_of(&self.state.decls()[i].kind);
                if let Some(c) = self.formula(f, Some(ty), &ctx) {
                    drag.push((i, c));
                }
            }
        }

        let mut texts = Vec::new();
        match (m.kind, &m.main) {
            ("text", Arg::Text(t)) => texts.push((
                "main".to_string(),
                self.template(t, &format!("{}/text", m.path), &bound),
            )),
            ("math", Arg::Text(t)) => {
                // TeX is static: refuse interpolation in it (E107).
                if let Err(e) = crate::text::parse(&format!("${t}$"), &format!("{}/math", m.path)) {
                    self.errors.push(e);
                }
                texts.push(("main".to_string(), Some(vec![TextPiece::Lit(t.clone())])));
            }
            _ => {}
        }
        if let Some(label) = &m.common.label {
            texts.push((
                "label".to_string(),
                self.template(&label.text, &format!("{}/label", m.path), &bound),
            ));
        }
        if let Some(Arg::Text(t)) = m.option("text") {
            texts.push((
                "text".to_string(),
                self.template(t, &format!("{}/text", m.path), &bound),
            ));
        }
        let texts: Vec<(String, Vec<TextPiece>)> = texts
            .into_iter()
            .filter_map(|(k, t)| t.map(|t| (k, t)))
            .collect();
        if self.errors.len() > errors_before {
            return None;
        }
        let mut deps = Deps::default();
        let mut add = |c: &Compiled| deps.extend(&self.module.deps(&c.checked.node));
        walk_slot(main.as_ref()?, &mut add);
        for (_, s) in &options {
            walk_slot(s, &mut add);
        }
        for c in when
            .iter()
            .chain(repeat.iter().map(|(_, c)| c))
            .chain(drag.iter().map(|(_, c)| c))
        {
            add(c);
        }
        for c in [
            &style.stroke,
            &style.width,
            &style.fill,
            &style.fill_opacity,
            &style.opacity,
            &style.halo,
        ]
        .into_iter()
        .flatten()
        {
            add(c);
        }
        Some(CompiledMark {
            source: m.clone(),
            main: main?,
            options,
            when,
            style,
            highlight,
            repeat,
            drag,
            deps,
            texts,
        })
    }

    /// `tangent: x, of: f` names a function the figure declares.
    fn check_functions(&mut self, m: &Mark) {
        if let Some(Arg::Ident(f)) = m.option("of") {
            let known = self.module.functions().iter().any(|func| &func.name == f);
            if !known {
                self.errors.push(
                    Error::new(
                        expr_codes::E301,
                        format!("there is no function \"{f}\" to take the {} of", m.kind),
                    )
                    .hint(format!("declare it in `let` as \"{f}(x)\": …"))
                    .at_path(format!("{}/of", m.path)),
                );
            }
        }
    }
}

fn matches_arg(spec: &Spec, arg: &Arg) -> bool {
    matches!(
        (spec, arg),
        (Spec::F(_) | Spec::Px, Arg::Formula(_))
            | (Spec::Map(_), Arg::Map(_))
            | (Spec::Text, Arg::Text(_))
            | (Spec::Ident, Arg::Ident(_))
            | (Spec::Word(_), Arg::Word(_))
            | (Spec::List(_), Arg::List(_))
            | (Spec::Marks, Arg::Marks(_))
    )
}

/// Every compiled formula of a mark, its children's included: a group reads
/// what its children read.
fn walk_mark(m: &CompiledMark, f: &mut dyn FnMut(&Compiled)) {
    walk_slot(&m.main, f);
    for (_, o) in &m.options {
        walk_slot(o, f);
    }
    let style = [
        &m.style.stroke,
        &m.style.width,
        &m.style.fill,
        &m.style.fill_opacity,
        &m.style.opacity,
        &m.style.halo,
    ];
    for c in m
        .when
        .iter()
        .chain(m.repeat.iter().map(|(_, c)| c))
        .chain(m.drag.iter().map(|(_, c)| c))
        .chain(style.into_iter().flatten())
    {
        f(c);
    }
}

fn walk_slot(s: &Slot, f: &mut dyn FnMut(&Compiled)) {
    match s {
        Slot::Formula(c) => f(c),
        Slot::Map(entries) => entries.iter().for_each(|(_, v)| walk_slot(v, f)),
        Slot::List(items) => items.iter().for_each(|v| walk_slot(v, f)),
        Slot::Marks(marks) => marks.iter().for_each(|m| walk_mark(m, f)),
        _ => {}
    }
}

impl Program {
    /// Every compiled formula outside the lets: marks (children included),
    /// axis ranges and readouts, for `hapto explain`.
    pub fn formulas(&self) -> Vec<&Compiled> {
        let mut out: Vec<&Compiled> = Vec::new();
        for (x, y) in &self.axes {
            out.extend(x.range.iter().chain(y.range.iter()));
        }
        // Walk with indices into owned data, then borrow back: the walker
        // takes a callback, and the borrows outlive it.
        fn collect<'a>(m: &'a CompiledMark, out: &mut Vec<&'a Compiled>) {
            fn slot<'a>(s: &'a Slot, out: &mut Vec<&'a Compiled>) {
                match s {
                    Slot::Formula(c) => out.push(c),
                    Slot::Map(e) => e.iter().for_each(|(_, v)| slot(v, out)),
                    Slot::List(i) => i.iter().for_each(|v| slot(v, out)),
                    Slot::Marks(ms) => ms.iter().for_each(|m| collect(m, out)),
                    _ => {}
                }
            }
            slot(&m.main, out);
            m.options.iter().for_each(|(_, o)| slot(o, out));
            out.extend(m.when.iter());
            out.extend(m.repeat.iter().map(|(_, c)| c));
            out.extend(m.drag.iter().map(|(_, c)| c));
            let st = &m.style;
            for c in [
                &st.stroke,
                &st.width,
                &st.fill,
                &st.fill_opacity,
                &st.opacity,
                &st.halo,
            ]
            .into_iter()
            .flatten()
            {
                out.push(c);
            }
            for (_, pieces) in &m.texts {
                for p in pieces {
                    if let TextPiece::Expr(c, _) = p {
                        out.push(c);
                    }
                }
            }
        }
        for m in &self.marks {
            collect(m, &mut out);
        }
        for r in &self.readouts {
            out.push(&r.value);
            out.extend(r.compare.iter());
        }
        out
    }

    /// Compile a document; on failure, every error with its path.
    pub fn new(doc: Document) -> Result<Program, Vec<Error>> {
        let state = State::new(&doc.values);
        let decls = Declarations {
            values: state.types(),
            compute: vec![],
            lets: doc
                .lets
                .iter()
                .map(|(k, f)| (k.clone(), f.text.clone()))
                .collect(),
        };
        let mut module = Module::new(&decls).map_err(|errors| {
            errors
                .into_iter()
                .map(|mut e| {
                    // Point the let's errors at the document's own pointer.
                    if let Some(p) = &e.path {
                        let key = p.trim_start_matches("/let/");
                        if let Some((_, f)) =
                            doc.lets.iter().find(|(k, _)| crate::tree::escape(k) == key)
                        {
                            e.path = Some(f.path.clone());
                        }
                    }
                    e
                })
                .collect::<Vec<_>>()
        })?;
        let mut c = Compiler {
            module: &mut module,
            state: &state,
            errors: Vec::new(),
            warnings: Vec::new(),
        };
        let marks: Vec<CompiledMark> = doc.marks.iter().filter_map(|m| c.mark(m, &[])).collect();
        let mut axes = Vec::new();
        for v in &doc.views {
            let range = |c: &mut Compiler, a: &crate::document::Axis| {
                let lo = c.formula(&a.range[0], Some(Type::Num), &[]);
                let hi = c.formula(&a.range[1], Some(Type::Num), &[]);
                Some(CompiledAxis { range: [lo?, hi?] })
            };
            if let (Some(x), Some(y)) = (range(&mut c, &v.x), range(&mut c, &v.y)) {
                axes.push((x, y));
            }
        }
        let mut readouts = Vec::new();
        for r in &doc.readouts {
            let value = c.formula(&r.value, None, &[]);
            let compare = r
                .compare
                .as_ref()
                .and_then(|f| c.formula(f, Some(Type::Num), &[]));
            if let Some(value) = value {
                readouts.push(CompiledReadout {
                    source: r.clone(),
                    value,
                    compare,
                });
            }
        }
        let mut errors = c.errors;
        let warnings = c.warnings;
        errors.extend(module.check_unused_functions().into_iter().map(|e| {
            let path = e.path.clone().unwrap_or_default();
            let key = path.trim_start_matches("/let/").to_string();
            match doc.lets.iter().find(|(k, _)| crate::tree::escape(k) == key) {
                Some((_, f)) => e.at_path(&f.path),
                None => e,
            }
        }));
        if !errors.is_empty() {
            return Err(errors);
        }
        Ok(Program {
            doc,
            module,
            state,
            marks,
            axes,
            readouts,
            warnings,
        })
    }

    /// The top-level marks a change to these values reaches.
    pub fn marks_touched_by(&self, values: &[usize]) -> BTreeSet<usize> {
        self.marks
            .iter()
            .enumerate()
            .filter(|(_, m)| m.deps.touched_by(values, &[]))
            .map(|(i, _)| i)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::read;

    fn program(yaml: &str) -> Result<Program, Vec<Error>> {
        Program::new(read(yaml).unwrap_or_else(|e| panic!("{e:?}")))
    }

    fn codes_of(yaml: &str) -> Vec<(String, String)> {
        program(yaml)
            .err()
            .unwrap_or_default()
            .into_iter()
            .map(|e| (e.code.0.to_string(), e.path.unwrap_or_default()))
            .collect()
    }

    const HEAD: &str = "hapto: 2\nalt: a\nvalues:\n  - { name: x, type: number, range: [-2, 2], default: 1 }\n  - { name: h, type: number, range: [0.01, 2], default: 1 }\n  - { name: p, type: point, range: [[-1, -1], [1, 1]], default: [0, 0] }\nlet:\n  'f(t)': 't**2'\nview: { x: { range: [-2.5, 2.5] }, y: { range: [-0.5, 4.5] } }\n";

    #[test]
    fn the_smallest_figure_compiles() {
        let p = program(&format!("{HEAD}marks:\n  - {{ function: 'f(x)', var: x }}\n  - {{ point: '[x, f(x)]', drag: {{ x: 'pointer.x' }} }}"))
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(p.marks.len(), 2);
        // The function's own `x` shadows the value: the curve reads no value.
        assert!(p.marks[0].deps.values.is_empty());
        assert_eq!(
            p.marks[1].deps.values.iter().copied().collect::<Vec<_>>(),
            vec![0]
        );
        assert_eq!(p.marks[1].drag[0].0, 0);
    }

    #[test]
    fn a_change_reaches_only_the_marks_that_read_it() {
        let p = program(&format!(
            "{HEAD}marks:\n  - {{ point: '[x, 0]' }}\n  - {{ point: '[0, h]' }}\n  - {{ point: 'p' }}\n  - {{ point: '[1, 1]', style: {{ width: '1 + h' }} }}"
        ))
        .unwrap();
        assert_eq!(
            p.marks_touched_by(&[0]).into_iter().collect::<Vec<_>>(),
            vec![0]
        );
        assert_eq!(
            p.marks_touched_by(&[1]).into_iter().collect::<Vec<_>>(),
            vec![1, 3]
        );
        assert_eq!(
            p.marks_touched_by(&[2]).into_iter().collect::<Vec<_>>(),
            vec![2]
        );
    }

    #[test]
    fn slots_are_checked_against_their_types_with_paths() {
        let errs = codes_of(&format!(
            "{HEAD}marks:\n  - {{ point: 'x' }}\n  - {{ circle: '[0, 0]', r: 'p' }}"
        ));
        assert_eq!(
            errs,
            [
                ("E201".into(), "/marks/0/point".into()),
                ("E201".into(), "/marks/1/r".into())
            ]
        );
        let errs = codes_of(&format!(
            "{HEAD}marks:\n  - {{ point: '[x, 0]', when: 'x' }}"
        ));
        assert_eq!(errs[0].1, "/marks/0/when");
        let errs = codes_of(&format!(
            "{HEAD}marks:\n  - {{ point: '[0, 0]', style: {{ fill: 'x' }} }}"
        ));
        assert_eq!(errs[0].1, "/marks/0/style/fill");
    }

    #[test]
    fn contexts_bind_their_names() {
        let ok = program(&format!(
            "{HEAD}marks:\n  - {{ scatter: '[[0, 0], [1, 1]]', r: '2 + i', colour: 'mix(palette(\"hot\"), palette(\"cool\"), p.x)' }}\n  - {{ heatmap: 'x * y' }}\n  - {{ heatmap: 'u * v + x', over: [u, v] }}\n  - {{ point: '[i, 0]', repeat: {{ i: 'range(0, 3)' }} }}\n  - {{ function: {{ x: 'cos(t)', y: 'sin(t)' }}, var: t }}"
        ));
        assert!(ok.is_ok(), "{:?}", ok.err());
        // `i` outside a repeat is nothing.
        assert_eq!(
            codes_of(&format!("{HEAD}marks:\n  - {{ point: '[i, 0]' }}"))[0].0,
            "E301"
        );
    }

    #[test]
    fn drags_set_values_that_exist_with_their_types() {
        assert_eq!(
            codes_of(&format!(
                "{HEAD}marks:\n  - {{ point: '[x, 0]', drag: {{ q: 'pointer.x' }} }}"
            ))[0]
                .0,
            "E505"
        );
        assert_eq!(
            codes_of(&format!(
                "{HEAD}marks:\n  - {{ point: '[x, 0]', drag: {{ x: 'pointer' }} }}"
            ))[0]
                .0,
            "E201"
        );
        assert!(program(&format!(
            "{HEAD}marks:\n  - {{ point: 'p', drag: {{ p: 'anchor0 + delta' }} }}"
        ))
        .is_ok());
    }

    #[test]
    fn tangents_name_a_declared_function() {
        assert!(program(&format!("{HEAD}marks:\n  - {{ tangent: x, of: f }}")).is_ok());
        let errs = codes_of(&format!("{HEAD}marks:\n  - {{ tangent: x, of: g }}"));
        assert_eq!(errs[0], ("E301".into(), "/marks/0/of".into()));
    }

    #[test]
    fn let_errors_point_at_the_document() {
        let errs = codes_of("hapto: 2\nalt: a\nlet: { k: '2^3' }\nview: { x: { range: [0, 1] }, y: { range: [0, 1] } }\nmarks: [ { point: '[k, 0]' } ]");
        assert_eq!(errs[0], ("E102".into(), "/let/k".into()));
    }

    #[test]
    fn readouts_and_axes_compile() {
        let p = program(&format!("{HEAD}marks:\n  - {{ point: '[x, 0]' }}\nreadouts:\n  - {{ id: s, value: 'derivative(f, x)', compare: '2 * x' }}")).unwrap();
        assert_eq!(p.readouts.len(), 1);
        assert_eq!(p.axes.len(), 1);
    }
}
