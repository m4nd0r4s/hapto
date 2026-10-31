//! What a formula reads, transitively: the dependency graph behind
//! incremental evaluation (SPEC-2.0 §3.3).
//!
//! A change to `x` must re-evaluate only what reads `x`, directly or through
//! lets and user functions. Each let's dependencies are worked out once when
//! the module is built; a formula's are worked out when asked.

use std::collections::BTreeSet;

use crate::ir::{Loop, Node};

/// The state a formula reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Deps {
    pub values: BTreeSet<usize>,
    pub compute: BTreeSet<usize>,
    pub lets: BTreeSet<usize>,
}

impl Deps {
    pub fn extend(&mut self, other: &Deps) {
        self.values.extend(&other.values);
        self.compute.extend(&other.compute);
        self.lets.extend(&other.lets);
    }

    /// Whether a change to any of these values or compute nodes reaches it.
    pub fn touched_by(&self, values: &[usize], compute: &[usize]) -> bool {
        values.iter().any(|v| self.values.contains(v))
            || compute.iter().any(|c| self.compute.contains(c))
    }
}

/// Visit every node of a tree, parents before children.
pub fn walk(node: &Node, visit: &mut dyn FnMut(&Node)) {
    visit(node);
    let lp = |lp: &Loop, visit: &mut dyn FnMut(&Node)| {
        walk(&lp.iter, visit);
        walk(&lp.body, visit);
        if let Some(cond) = &lp.cond {
            walk(cond, visit);
        }
    };
    match node {
        Node::Const(_) | Node::Local(_) | Node::Value(_) | Node::Compute(_) | Node::Let(_) => {}
        Node::Neg(_, a)
        | Node::Not(a)
        | Node::VecX(a)
        | Node::VecY(a)
        | Node::Field(a, _)
        | Node::Coerce(a, _) => walk(a, visit),
        Node::And(a, b)
        | Node::Or(a, b)
        | Node::Binary(_, a, b)
        | Node::MakeVec(a, b)
        | Node::IndexList(a, b)
        | Node::IndexMat(a, b) => {
            walk(a, visit);
            walk(b, visit);
        }
        Node::If(c, a, b) => {
            walk(c, visit);
            walk(a, visit);
            walk(b, visit);
        }
        Node::List(items)
        | Node::Builtin(_, items)
        | Node::BuiltinWithFn(_, _, items)
        | Node::CallUser(_, items) => items.iter().for_each(|i| walk(i, visit)),
        Node::Method(a, _, args) => {
            walk(a, visit);
            args.iter().for_each(|i| walk(i, visit));
        }
        Node::ListComp(l) | Node::Reduce(_, l) => lp(l, visit),
    }
}
