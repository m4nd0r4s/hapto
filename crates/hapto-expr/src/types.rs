//! The types a formula can have (SPEC-2.0 §4.2).
//!
//! There are no implicit conversions between numbers, booleans and strings.
//! The one ambiguity the language allows on purpose is `[a, b]` with two
//! numbers, which is a vector where a vector is wanted and a list where a list
//! is wanted; the checker gives it the type [`Type::Pair`] until the context
//! decides, and records the decision as an explicit coercion.

use std::fmt;
use std::sync::Arc;

/// The type of a value.
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Num,
    Bool,
    Str,
    /// A 2D vector, a point, or a complex number.
    Vec,
    Colour,
    Mat,
    List(Box<Type>),
    Record(Arc<RecordType>),
    /// `[a, b]` with two numbers, not yet known to be a vector or a list.
    Pair,
    /// The element type of `[]`, which fits any list.
    Any,
}

impl Type {
    pub fn list(of: Type) -> Type {
        Type::List(Box::new(of))
    }

    /// The type a value settles on when nothing asks for more: a pair is a
    /// vector (§4.2), and so is every pair inside a list.
    pub fn settled(&self) -> Type {
        match self {
            Type::Pair => Type::Vec,
            Type::List(inner) => Type::list(inner.settled()),
            other => other.clone(),
        }
    }

    /// Whether a value of this type behaves as a 2D vector.
    pub fn is_vec_like(&self) -> bool {
        matches!(self, Type::Vec | Type::Pair)
    }

    /// The element type, for list types (and `Pair`, a two-number list).
    pub fn element(&self) -> Option<Type> {
        match self {
            Type::List(inner) => Some((**inner).clone()),
            Type::Pair => Some(Type::Num),
            _ => None,
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Num => f.write_str("num"),
            Type::Bool => f.write_str("bool"),
            Type::Str => f.write_str("str"),
            Type::Vec => f.write_str("vec"),
            Type::Colour => f.write_str("colour"),
            Type::Mat => f.write_str("mat"),
            Type::List(inner) => write!(f, "list<{inner}>"),
            Type::Record(r) => write!(f, "{} record", r.name),
            Type::Pair => f.write_str("vec"),
            Type::Any => f.write_str("anything"),
        }
    }
}

/// A record: the result of a compute node, `eig`, `histogram`,
/// `circle_through`. Fields are read with `.name`; methods are called with
/// `.name(args)`, like `orbit.at(s)`.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordType {
    pub name: String,
    pub fields: Vec<(String, Type)>,
    pub methods: Vec<Method>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Method {
    pub name: String,
    pub params: Vec<Type>,
    pub ret: Type,
}

impl RecordType {
    pub fn field(&self, name: &str) -> Option<&Type> {
        self.fields.iter().find(|(n, _)| n == name).map(|(_, t)| t)
    }

    pub fn method(&self, name: &str) -> Option<&Method> {
        self.methods.iter().find(|m| m.name == name)
    }

    /// Every field and method name, for "did you mean".
    pub fn members(&self) -> Vec<String> {
        self.fields
            .iter()
            .map(|(n, _)| n.clone())
            .chain(self.methods.iter().map(|m| format!("{}(…)", m.name)))
            .collect()
    }
}

/// How a value of one type is turned into another the context needs.
#[derive(Debug, Clone, PartialEq)]
pub enum Coercion {
    /// A pair (evaluated as a vector) used where a list of numbers is wanted.
    PairToList,
    /// Apply the inner coercion to every element of a list.
    Elements(Box<Coercion>),
}

/// Whether a value of type `found` can be used where `wanted` is expected,
/// and the coercion that makes it so (`None` inside `Some` means "as is").
pub fn fits(found: &Type, wanted: &Type) -> Option<Option<Coercion>> {
    match (found, wanted) {
        (a, b) if a == b => Some(None),
        (Type::Pair, Type::Vec) => Some(None),
        (Type::Pair, Type::List(inner)) if **inner == Type::Num => Some(Some(Coercion::PairToList)),
        (Type::List(a), Type::List(_)) if **a == Type::Any => Some(None),
        (Type::List(a), Type::List(b)) => match fits(a, b)? {
            None => Some(None),
            Some(inner) => Some(Some(Coercion::Elements(Box::new(inner)))),
        },
        _ => None,
    }
}

/// The type both of two values fit, with the coercion each needs: how the
/// branches of an `if` and the items of a list agree.
pub fn join(a: &Type, b: &Type) -> Option<(Type, Option<Coercion>, Option<Coercion>)> {
    if a == b {
        return Some((a.clone(), None, None));
    }
    if let Some(c) = fits(a, b) {
        return Some((b.clone(), c, None));
    }
    if let Some(c) = fits(b, a) {
        return Some((a.clone(), None, c));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pair_fits_a_vector_as_is_and_a_list_by_coercion() {
        assert_eq!(fits(&Type::Pair, &Type::Vec), Some(None));
        assert_eq!(
            fits(&Type::Pair, &Type::list(Type::Num)),
            Some(Some(Coercion::PairToList))
        );
        assert_eq!(fits(&Type::Pair, &Type::list(Type::Vec)), None);
        assert_eq!(fits(&Type::Num, &Type::Bool), None);
    }

    #[test]
    fn lists_of_pairs_coerce_element_by_element() {
        let rows = Type::list(Type::Pair);
        assert_eq!(
            fits(&rows, &Type::list(Type::list(Type::Num))),
            Some(Some(Coercion::Elements(Box::new(Coercion::PairToList))))
        );
        assert_eq!(fits(&rows, &Type::list(Type::Vec)), Some(None));
        assert_eq!(
            fits(&Type::list(Type::Any), &Type::list(Type::Str)),
            Some(None)
        );
    }

    #[test]
    fn joins_settle_on_the_wider_type() {
        assert_eq!(join(&Type::Pair, &Type::Vec).unwrap().0, Type::Vec);
        assert_eq!(join(&Type::Num, &Type::Str), None);
        assert_eq!(Type::list(Type::Pair).settled(), Type::list(Type::Vec));
    }
}
