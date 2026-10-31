//! A figure's names, together: values, compute nodes, lets and functions.
//!
//! A `Module` is what every formula of a figure is checked against. Building
//! one validates the names (reserved, duplicated, shadowing — §4.1, §4.4),
//! orders the lets by what they read rather than where they are written,
//! refuses cycles and recursion with the path that closes them (§4.5), and
//! compiles each let once. User functions are compiled on demand, once per
//! list of argument types they are called with, so `g(p) = p.x * 2` works
//! for whatever a figure passes it.

use std::collections::HashMap;

use crate::ast::{Comprehension, Expr, ExprKind, Ident};
use crate::check::{check, is_reserved, Checked, Named, Resolution, Scope};
use crate::deps::{walk, Deps};
use crate::error::{codes, Error};
use crate::ir::{FnId, Node};
use crate::lexer::{lex, Tok};
use crate::parser::parse;
use crate::span::Span;
use crate::types::Type;

/// What a figure declares, before anything is checked.
#[derive(Debug, Clone, Default)]
pub struct Declarations {
    /// The figure's values and the page values it imports, in order: a
    /// value's index here is its slot at evaluation.
    pub values: Vec<(String, Type)>,
    /// Compute nodes with their record types (hapto-core supplies them).
    pub compute: Vec<(String, Type)>,
    /// `let:` entries as written: key (`"slope"` or `"f(t)"`) and formula.
    pub lets: Vec<(String, String)>,
}

/// A compiled let.
#[derive(Debug, Clone)]
pub struct Let {
    pub name: String,
    pub formula: String,
    pub checked: Checked,
    /// Everything it reads, through other lets and functions too.
    pub deps: Deps,
}

/// A user function as declared.
#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub params: Vec<Ident>,
    pub formula: String,
    pub body: Expr,
    /// The let key it came from, for error paths.
    pub key: String,
}

/// A user function compiled for one list of argument types.
#[derive(Debug, Clone)]
pub struct Instance {
    pub function: usize,
    pub args: Vec<Type>,
    pub checked: Checked,
}

#[derive(Debug, Clone)]
pub struct Module {
    values: Vec<(String, Type)>,
    compute: Vec<(String, Type)>,
    /// Compiled lets, indexed as `Node::Let` refers to them.
    lets: Vec<Let>,
    /// Let name to its index once compiled.
    let_index: HashMap<String, usize>,
    functions: Vec<Function>,
    instances: Vec<Instance>,
}

/// JSON Pointer of a let entry, for errors (RFC 6901 escaping).
fn let_path(key: &str) -> String {
    format!("/let/{}", key.replace('~', "~0").replace('/', "~1"))
}

/// A let's key: a name, or a function signature `name(a, b)`.
enum Key {
    Plain(Ident),
    Function(Ident, Vec<Ident>),
}

fn parse_key(key: &str) -> Result<Key, Error> {
    let tokens = lex(key)?;
    let bad = || {
        Error::new(codes::E101, format!("\"{key}\" is not a let name"))
            .at(Span::new(0, key.len()))
            .hint("a let is named `slope`, or `f(x)` for a function")
            .in_formula(key)
    };
    let ident = |i: usize| match tokens.get(i) {
        Some(t) => match &t.tok {
            Tok::Ident(name) => Some(Ident {
                name: name.clone(),
                span: t.span,
            }),
            _ => None,
        },
        None => None,
    };
    let name = ident(0).ok_or_else(bad)?;
    if tokens.len() == 1 {
        return Ok(Key::Plain(name));
    }
    if tokens.get(1).map(|t| &t.tok) != Some(&Tok::LParen)
        || tokens.last().map(|t| &t.tok) != Some(&Tok::RParen)
    {
        return Err(bad());
    }
    let mut params = Vec::new();
    let inner = &tokens[2..tokens.len() - 1];
    for (i, token) in inner.iter().enumerate() {
        let want_name = i % 2 == 0;
        match (&token.tok, want_name) {
            (Tok::Ident(p), true) => params.push(Ident {
                name: p.clone(),
                span: token.span,
            }),
            (Tok::Comma, false) => {}
            _ => return Err(bad()),
        }
    }
    if !inner.is_empty() && inner.len() % 2 == 0 {
        return Err(bad());
    }
    Ok(Key::Function(name, params))
}

/// Every name an expression reads that it does not bind itself, in order.
fn free_names(expr: &Expr, bound: &mut Vec<String>, out: &mut Vec<Ident>) {
    let comp = |c: &Comprehension, bound: &mut Vec<String>, out: &mut Vec<Ident>| {
        free_names(&c.iter, bound, out);
        bound.push(c.var.name.clone());
        free_names(&c.body, bound, out);
        if let Some(cond) = &c.cond {
            free_names(cond, bound, out);
        }
        bound.pop();
    };
    match &expr.kind {
        ExprKind::Name(name) => {
            if !bound.contains(name) {
                out.push(Ident {
                    name: name.clone(),
                    span: expr.span,
                });
            }
        }
        ExprKind::List(items) => items.iter().for_each(|i| free_names(i, bound, out)),
        ExprKind::ListComp(c) | ExprKind::Generator(c) => comp(c, bound, out),
        ExprKind::Unary { operand, .. } => free_names(operand, bound, out),
        ExprKind::Binary { left, right, .. } => {
            free_names(left, bound, out);
            free_names(right, bound, out);
        }
        ExprKind::If {
            cond,
            then,
            otherwise,
        } => {
            free_names(cond, bound, out);
            free_names(then, bound, out);
            free_names(otherwise, bound, out);
        }
        ExprKind::Call { callee, args } => {
            free_names(callee, bound, out);
            args.iter().for_each(|a| free_names(a, bound, out));
        }
        ExprKind::Index { base, index } => {
            free_names(base, bound, out);
            free_names(index, bound, out);
        }
        ExprKind::Field { base, .. } => free_names(base, bound, out),
        _ => {}
    }
}

impl Module {
    /// Validate and compile a figure's declarations. Every problem found is
    /// reported, not just the first.
    pub fn new(decls: &Declarations) -> Result<Module, Vec<Error>> {
        let mut errors = Vec::new();
        let mut module = Module {
            values: decls.values.clone(),
            compute: decls.compute.clone(),
            lets: Vec::new(),
            let_index: HashMap::new(),
            functions: Vec::new(),
            instances: Vec::new(),
        };

        // Values and compute nodes: not reserved, not repeated, not each other.
        let mut seen: HashMap<String, &'static str> = HashMap::new();
        for (kind, names) in [("value", &decls.values), ("compute node", &decls.compute)] {
            for (name, _) in names.iter() {
                if is_reserved(name) {
                    errors.push(reserved(name, kind));
                } else if let Some(prev) = seen.get(name) {
                    let code = if *prev == kind {
                        codes::E306
                    } else {
                        codes::E303
                    };
                    errors.push(
                        Error::new(code, format!("\"{name}\" names both a {prev} and a {kind}"))
                            .hint("give each its own name"),
                    );
                } else {
                    seen.insert(name.clone(), kind);
                }
            }
        }

        // Lets: parse keys and bodies.
        struct Plain {
            name: String,
            formula: String,
            key: String,
            body: Expr,
        }
        let mut plains: Vec<Plain> = Vec::new();
        for (key, formula) in &decls.lets {
            let path = let_path(key);
            let parsed_key = match parse_key(key) {
                Ok(k) => k,
                Err(e) => {
                    errors.push(e.at_path(&path));
                    continue;
                }
            };
            let name = match &parsed_key {
                Key::Plain(n) | Key::Function(n, _) => n.name.clone(),
            };
            if is_reserved(&name) {
                errors.push(reserved(&name, "let").at_path(&path));
                continue;
            }
            if let Some(prev) = seen.get(&name) {
                let code = if *prev == "let" {
                    codes::E306
                } else {
                    codes::E303
                };
                errors.push(
                    Error::new(code, format!("the let \"{name}\" has the name of a {prev}"))
                        .hint("a let cannot hide a value or compute node; rename it")
                        .at_path(&path),
                );
                continue;
            }
            seen.insert(name.clone(), "let");
            let body = match parse(formula) {
                Ok(b) => b,
                Err(e) => {
                    errors.push(e.at_path(&path));
                    continue;
                }
            };
            match parsed_key {
                Key::Plain(_) => plains.push(Plain {
                    name,
                    formula: formula.clone(),
                    key: key.clone(),
                    body,
                }),
                Key::Function(_, params) => {
                    let mut ok = true;
                    for (i, p) in params.iter().enumerate() {
                        if is_reserved(&p.name) {
                            errors.push(reserved(&p.name, "parameter").at_path(&path));
                            ok = false;
                        } else if params[..i].iter().any(|q| q.name == p.name) {
                            errors.push(
                                Error::new(
                                    codes::E306,
                                    format!("\"{name}\" has two parameters named \"{}\"", p.name),
                                )
                                .in_formula(key)
                                .at(p.span)
                                .at_path(&path),
                            );
                            ok = false;
                        }
                    }
                    if ok {
                        module.functions.push(Function {
                            name,
                            params,
                            formula: formula.clone(),
                            body,
                            key: key.clone(),
                        });
                    }
                }
            }
        }
        if !errors.is_empty() {
            return Err(errors);
        }

        // Order by dependency. Nodes: plain lets, then functions.
        let n_plain = plains.len();
        let node_of = |name: &str| -> Option<usize> {
            plains.iter().position(|p| p.name == name).or_else(|| {
                module
                    .functions
                    .iter()
                    .position(|f| f.name == name)
                    .map(|i| n_plain + i)
            })
        };
        let label = |i: usize| -> String {
            if i < n_plain {
                plains[i].name.clone()
            } else {
                format!("{}(…)", module.functions[i - n_plain].name)
            }
        };
        let mut edges: Vec<Vec<(usize, Span)>> = Vec::new();
        let bodies = plains.iter().map(|p| (&p.body, Vec::new())).chain(
            module
                .functions
                .iter()
                .map(|f| (&f.body, f.params.iter().map(|p| p.name.clone()).collect())),
        );
        for (body, mut bound) in bodies {
            let mut names = Vec::new();
            free_names(body, &mut bound, &mut names);
            edges.push(
                names
                    .iter()
                    .filter_map(|n| node_of(&n.name).map(|t| (t, n.span)))
                    .collect(),
            );
        }

        // Depth-first, reporting the first cycle with its path.
        let total = edges.len();
        let mut state = vec![0u8; total]; // 0 new, 1 on stack, 2 done
        let mut order = Vec::new();
        let mut stack: Vec<usize> = Vec::new();
        fn visit(
            i: usize,
            edges: &[Vec<(usize, Span)>],
            state: &mut [u8],
            stack: &mut Vec<usize>,
            order: &mut Vec<usize>,
        ) -> Result<(), Vec<usize>> {
            state[i] = 1;
            stack.push(i);
            for (j, _) in &edges[i] {
                match state[*j] {
                    0 => visit(*j, edges, state, stack, order)?,
                    1 => {
                        let from = stack.iter().position(|k| k == j).unwrap_or(0);
                        let mut cycle = stack[from..].to_vec();
                        cycle.push(*j);
                        return Err(cycle);
                    }
                    _ => {}
                }
            }
            stack.pop();
            state[i] = 2;
            order.push(i);
            Ok(())
        }
        for i in 0..total {
            if state[i] == 0 {
                if let Err(cycle) = visit(i, &edges, &mut state, &mut stack, &mut order) {
                    let path: Vec<String> = cycle.iter().map(|&k| label(k)).collect();
                    let all_functions = cycle.iter().all(|&k| k >= n_plain);
                    let first = cycle[0];
                    let key = if first < n_plain {
                        plains[first].key.clone()
                    } else {
                        module.functions[first - n_plain].key.clone()
                    };
                    let err = if all_functions {
                        Error::new(
                            codes::E305,
                            format!("a function calls itself: {}", path.join(" → ")),
                        )
                        .hint("formulas do not recurse; use `iterate` or a comprehension")
                    } else {
                        Error::new(
                            codes::E304,
                            format!(
                                "these lets read each other in a circle: {}",
                                path.join(" → ")
                            ),
                        )
                        .hint("one of them has to be computed from something outside the circle")
                    };
                    return Err(vec![err.at_path(let_path(&key))]);
                }
            }
        }

        // Compile plain lets in dependency order.
        for i in order {
            if i >= n_plain {
                continue;
            }
            let plain = &plains[i];
            let result = check(&plain.body, &mut module, &[], None);
            match result {
                Ok(checked) => {
                    // Lets are compiled in dependency order, so every let this
                    // one reads already knows its own dependencies.
                    let deps = module.deps(&checked.node);
                    module
                        .let_index
                        .insert(plain.name.clone(), module.lets.len());
                    module.lets.push(Let {
                        name: plain.name.clone(),
                        formula: plain.formula.clone(),
                        checked,
                        deps,
                    });
                }
                Err(e) => errors.push(e.in_formula(&plain.formula).at_path(let_path(&plain.key))),
            }
        }
        if errors.is_empty() {
            Ok(module)
        } else {
            Err(errors)
        }
    }

    /// Check a formula against this figure's names: a mark's position, a
    /// readout, a style. `bound` names (a mark's `x`, a drag's `pointer`)
    /// take the first local slots.
    pub fn compile(
        &mut self,
        formula: &str,
        bound: &[(String, Type)],
        expected: Option<&Type>,
    ) -> Result<Checked, Error> {
        let expr = parse(formula)?;
        for (name, _) in bound {
            if crate::lexer::KEYWORDS.contains(&name.as_str()) {
                return Err(reserved(name, "bound variable"));
            }
        }
        let mut checked = check(&expr, self, bound, expected).map_err(|e| e.in_formula(formula))?;
        for w in &mut checked.warnings {
            w.formula = Some(formula.to_string());
        }
        Ok(checked)
    }

    /// Check every function no formula has called yet, with number
    /// parameters (§4.5), so an unused function still has its errors found.
    pub fn check_unused_functions(&mut self) -> Vec<Error> {
        let mut errors = Vec::new();
        for f in 0..self.functions.len() {
            if self.instances.iter().any(|i| i.function == f) {
                continue;
            }
            let args = vec![Type::Num; self.functions[f].params.len()];
            if let Err(e) = self.instantiate(f, &args, Span::default()) {
                errors.push(e);
            }
        }
        errors
    }

    /// What `node` reads, transitively through lets and user functions.
    pub fn deps(&self, node: &Node) -> Deps {
        let mut deps = Deps::default();
        let mut calls = Vec::new();
        walk(node, &mut |n| match n {
            Node::Value(i) => {
                deps.values.insert(*i);
            }
            Node::Compute(i) => {
                deps.compute.insert(*i);
            }
            Node::Let(i) => {
                deps.lets.insert(*i);
            }
            Node::CallUser(f, _) | Node::BuiltinWithFn(_, f, _) => calls.push(*f),
            _ => {}
        });
        for i in deps.lets.clone() {
            if let Some(l) = self.lets.get(i) {
                deps.extend(&l.deps);
            }
        }
        // Functions never recurse (E305), so this descent ends.
        for f in calls {
            let inner = self.deps(&self.instances[f].checked.node);
            deps.extend(&inner);
        }
        deps
    }

    /// The lets a change to these values or compute nodes reaches.
    pub fn lets_touched_by(&self, values: &[usize], compute: &[usize]) -> Vec<usize> {
        (0..self.lets.len())
            .filter(|&i| self.lets[i].deps.touched_by(values, compute))
            .collect()
    }

    pub fn values(&self) -> &[(String, Type)] {
        &self.values
    }

    pub fn compute(&self) -> &[(String, Type)] {
        &self.compute
    }

    pub fn lets(&self) -> &[Let] {
        &self.lets
    }

    pub fn functions(&self) -> &[Function] {
        &self.functions
    }

    pub fn instances(&self) -> &[Instance] {
        &self.instances
    }

    pub fn instance(&self, id: FnId) -> &Instance {
        &self.instances[id]
    }

    /// For `hapto explain`: what every name in every let resolved to.
    pub fn explain(&self) -> Vec<(String, Type, Vec<Resolution>)> {
        self.lets
            .iter()
            .map(|l| {
                (
                    l.name.clone(),
                    l.checked.ty.clone(),
                    l.checked.resolutions.clone(),
                )
            })
            .collect()
    }

    fn instantiate(&mut self, f: usize, args: &[Type], span: Span) -> Result<(FnId, Type), Error> {
        let args: Vec<Type> = args.iter().map(Type::settled).collect();
        // A figure has a handful of instances: a scan is the cheapest lookup.
        if let Some(id) = self
            .instances
            .iter()
            .position(|i| i.function == f && i.args == args)
        {
            return Ok((id, self.instances[id].checked.ty.clone()));
        }
        let function = self.functions[f].clone();
        if function.params.len() != args.len() {
            return Err(Error::new(
                codes::E203,
                format!(
                    "\"{}\" takes {} argument(s), not {}",
                    function.name,
                    function.params.len(),
                    args.len()
                ),
            )
            .at(span)
            .hint(format!("it is declared as \"{}\"", function.key)));
        }
        let bound: Vec<(String, Type)> = function
            .params
            .iter()
            .zip(&args)
            .map(|(p, t)| (p.name.clone(), t.clone()))
            .collect();
        let checked = check(&function.body, self, &bound, None).map_err(|e| {
            let typed: Vec<String> = args.iter().map(|t| t.to_string()).collect();
            let mut e = e
                .in_formula(&function.formula)
                .at_path(let_path(&function.key));
            e.message = format!("in {}({}): {}", function.name, typed.join(", "), e.message);
            e
        })?;
        let ty = checked.ty.clone();
        let id = self.instances.len();
        self.instances.push(Instance {
            function: f,
            args,
            checked,
        });
        Ok((id, ty))
    }
}

fn reserved(name: &str, what: &str) -> Error {
    Error::new(
        codes::E302,
        format!("\"{name}\" is reserved and cannot name a {what}"),
    )
    .hint("reserved: keywords, pi, tau, e, library function names, and pointer, start, anchor0, delta, focus, theme")
}

impl Scope for Module {
    fn name(&mut self, name: &str) -> Option<Named> {
        if let Some(i) = self.let_index.get(name) {
            return Some(Named::Let(*i, self.lets[*i].checked.ty.clone()));
        }
        if self.functions.iter().any(|f| f.name == name) {
            return Some(Named::Function);
        }
        if let Some(i) = self.compute.iter().position(|(n, _)| n == name) {
            return Some(Named::Compute(i, self.compute[i].1.clone()));
        }
        if let Some(i) = self.values.iter().position(|(n, _)| n == name) {
            return Some(Named::Value(i, self.values[i].1.clone()));
        }
        None
    }

    fn function(
        &mut self,
        name: &str,
        args: &[Type],
        span: Span,
    ) -> Option<Result<(FnId, Type), Error>> {
        let f = self.functions.iter().position(|f| f.name == name)?;
        Some(self.instantiate(f, args, span))
    }

    fn known_names(&self) -> Vec<String> {
        self.values
            .iter()
            .map(|(n, _)| n.clone())
            .chain(self.compute.iter().map(|(n, _)| n.clone()))
            .chain(self.lets.iter().map(|l| l.name.clone()))
            .chain(self.functions.iter().map(|f| f.name.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decls(lets: &[(&str, &str)]) -> Declarations {
        Declarations {
            values: vec![("x".into(), Type::Num), ("p".into(), Type::Vec)],
            compute: vec![],
            lets: lets
                .iter()
                .map(|(k, f)| (k.to_string(), f.to_string()))
                .collect(),
        }
    }

    fn module(lets: &[(&str, &str)]) -> Module {
        Module::new(&decls(lets)).unwrap_or_else(|e| panic!("{e:?}"))
    }

    fn errors(lets: &[(&str, &str)]) -> Vec<Error> {
        Module::new(&decls(lets)).unwrap_err()
    }

    #[test]
    fn lets_are_ordered_by_what_they_read_not_where_they_are_written() {
        let m = module(&[
            ("slope", "derivative_free + 1"),
            ("derivative_free", "2 * x"),
        ]);
        assert_eq!(m.lets()[0].name, "derivative_free");
        assert_eq!(m.lets()[1].name, "slope");
    }

    #[test]
    fn functions_are_compiled_per_argument_type() {
        let mut m = module(&[("f(t)", "t * 2"), ("a", "f(x)"), ("b", "f(p)")]);
        assert_eq!(m.lets()[0].checked.ty, Type::Num);
        assert_eq!(m.lets()[1].checked.ty, Type::Vec);
        assert_eq!(m.instances().len(), 2);
        // A second call with the same types reuses the instance.
        m.compile("f(3) + f(x)", &[], None).unwrap();
        assert_eq!(m.instances().len(), 2);
    }

    #[test]
    fn a_function_may_read_lets_and_values() {
        let mut m = module(&[("f(t)", "t + k * x"), ("k", "3"), ("a", "f(1)")]);
        let checked = m.compile("f(2)", &[], Some(&Type::Num)).unwrap();
        assert_eq!(checked.ty, Type::Num);
    }

    #[test]
    fn parameters_shadow_values() {
        let mut m = module(&[("g(x)", "x.y")]);
        assert_eq!(m.compile("g(p)", &[], None).unwrap().ty, Type::Num);
    }

    #[test]
    fn a_cycle_names_its_path() {
        let e = &errors(&[("a", "b + 1"), ("b", "a * 2")])[0];
        assert_eq!(e.code, codes::E304);
        assert!(e.message.contains("a → b → a"), "{}", e.message);
        assert_eq!(e.path.as_deref(), Some("/let/a"));
    }

    #[test]
    fn recursion_is_refused() {
        let e = &errors(&[("f(n)", "if n < 1 then 1 else n * f(n - 1)")])[0];
        assert_eq!(e.code, codes::E305);
        let e = &errors(&[("f(t)", "g(t)"), ("g(t)", "f(t)")])[0];
        assert_eq!(e.code, codes::E305);
    }

    #[test]
    fn reserved_names_are_refused() {
        assert_eq!(errors(&[("pi", "3")])[0].code, codes::E302);
        assert_eq!(errors(&[("sqrt", "3")])[0].code, codes::E302);
        assert_eq!(errors(&[("f(pointer)", "3")])[0].code, codes::E302);
        let bad_value = Declarations {
            values: vec![("sqrt".into(), Type::Num)],
            ..Default::default()
        };
        assert_eq!(Module::new(&bad_value).unwrap_err()[0].code, codes::E302);
    }

    #[test]
    fn duplicates_and_shadowing_are_refused() {
        assert_eq!(errors(&[("x", "3")])[0].code, codes::E303);
        assert_eq!(errors(&[("a", "1"), ("a", "2")])[0].code, codes::E306);
        assert_eq!(errors(&[("f(t, t)", "t")])[0].code, codes::E306);
        let clash = Declarations {
            values: vec![("q".into(), Type::Num)],
            compute: vec![("q".into(), Type::Num)],
            ..Default::default()
        };
        assert_eq!(Module::new(&clash).unwrap_err()[0].code, codes::E303);
    }

    #[test]
    fn every_error_is_reported_with_its_let() {
        let all = errors(&[("a", "1 +"), ("b", "2^3")]);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].path.as_deref(), Some("/let/a"));
        assert_eq!(all[1].code, codes::E102);
        assert_eq!(all[1].path.as_deref(), Some("/let/b"));
    }

    #[test]
    fn a_bad_key_says_what_a_key_looks_like() {
        let e = &errors(&[("f(", "1")])[0];
        assert_eq!(e.code, codes::E101);
        assert!(e.hint.as_deref().unwrap().contains("f(x)"));
        assert_eq!(errors(&[("f(a b)", "1")])[0].code, codes::E101);
        assert_eq!(errors(&[("2", "1")])[0].code, codes::E101);
    }

    #[test]
    fn wrong_arity_points_at_the_declaration() {
        let e = &errors(&[("f(a, b)", "a + b"), ("c", "f(1)")])[0];
        assert_eq!(e.code, codes::E203);
        assert_eq!(e.hint.as_deref(), Some("it is declared as \"f(a, b)\""));
    }

    #[test]
    fn unused_functions_are_checked_with_numbers() {
        let mut m = module(&[("h(t)", "t.x")]);
        let errs = m.check_unused_functions();
        assert_eq!(errs[0].code, codes::E207);
        let mut m = module(&[("h(t)", "t * 2")]);
        assert!(m.check_unused_functions().is_empty());
    }

    #[test]
    fn functions_reach_calculus_builtins_by_name() {
        // `derivative` lands with the calculus library; the plumbing is the
        // function-instance lookup, which `check_unused_functions` exercises.
        let mut m = module(&[("f(t)", "t * t")]);
        assert!(matches!(m.name("f"), Some(Named::Function)));
        assert!(m
            .function("f", &[Type::Num], Span::default())
            .unwrap()
            .is_ok());
    }

    #[test]
    fn explain_lists_what_each_let_reads() {
        let m = module(&[("k", "x * 2"), ("j", "k + pi")]);
        let explained = m.explain();
        let j = explained.iter().find(|(n, _, _)| n == "j").unwrap();
        assert!(j.2.iter().any(|r| r.name == "k" && r.what == "let: num"));
    }
}
