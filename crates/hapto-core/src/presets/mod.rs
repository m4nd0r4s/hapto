//! Presets: named expansions into the base language (SPEC-2.0 §14).
//!
//! A preset rewrites the document tree before it is read, so a preset never
//! does anything the base language cannot: what it expands to is an
//! ordinary document. Values, lets, marks and readouts written beside the
//! preset are appended to the expansion (§14.1).
//!
//! Only `legacy` exists so far (§14.3); the 2.0 presets land in M6.

mod legacy;

use hapto_expr::Error;
use serde_json::{Map, Value as Tree};

/// Expand a document's preset, if it has one this build knows.
///
/// Returns the tree to read and the warnings expanding it raised. A tree
/// without a preset comes back unchanged; one with a preset this build
/// does not have is left for the reader to refuse with its reason.
pub fn expand(tree: Tree) -> Result<(Tree, Vec<Error>), Vec<Error>> {
    if legacy::applies(&tree) {
        return legacy::expand(&tree);
    }
    Ok((tree, Vec::new()))
}

/// Append what an author wrote beside a preset to its expansion: their
/// values, lets, marks and readouts come after the preset's own (§14.1).
fn append(expansion: &mut Map<String, Tree>, written: &Map<String, Tree>) {
    for key in ["values", "marks", "readouts"] {
        if let Some(Tree::Array(more)) = written.get(key) {
            match expansion.get_mut(key) {
                Some(Tree::Array(items)) => items.extend(more.iter().cloned()),
                _ => {
                    expansion.insert(key.into(), Tree::Array(more.clone()));
                }
            }
        }
    }
    if let Some(Tree::Object(more)) = written.get("let") {
        match expansion.get_mut("let") {
            Some(Tree::Object(lets)) => {
                lets.extend(more.iter().map(|(k, v)| (k.clone(), v.clone())))
            }
            _ => {
                expansion.insert("let".into(), Tree::Object(more.clone()));
            }
        }
    }
    // The document's own keys stay the author's.
    for key in ["id", "title", "caption", "alt", "size", "controls", "baked"] {
        if let Some(v) = written.get(key) {
            expansion.insert(key.into(), v.clone());
        }
    }
}
