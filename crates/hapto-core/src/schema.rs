//! The JSON Schema of the figure format, generated from the decoder's own
//! tables (`hapto schema`, SPEC-2.0 §17).
//!
//! An agent or an editor validates a document against this before hapto
//! does; because both read the same tables, the schema cannot promise a key
//! the decoder refuses or miss one it accepts. The schema is necessarily
//! looser than the decoder — ranges, references and formulas are checked
//! only by hapto — but never stricter.

use serde_json::{json, Map, Value};

use crate::document::marks::{Spec, Ty, ANCHORS, COMMON, KINDS, STYLE_KEYS};
use crate::document::views::{AXES, AXIS_KEYS, LEGENDS};
use crate::document::{values, KEYS};

fn formula() -> Value {
    json!({ "description": "a formula, or a literal number, flag or list", "type": ["string", "number", "boolean", "array"] })
}

fn ident() -> Value {
    json!({ "type": "string", "pattern": "^[A-Za-z][A-Za-z0-9_]*$" })
}

fn spec(s: &Spec) -> Value {
    match s {
        Spec::F(Ty::Colour) => {
            json!({ "description": "a colour: a theme token, a hex literal or a formula", "type": ["string", "array"] })
        }
        Spec::F(_) | Spec::Px => formula(),
        Spec::Text => json!({ "type": ["string", "number"] }),
        Spec::Ident => ident(),
        Spec::Word(words) => json!({ "enum": words }),
        Spec::Map(keys) => {
            let mut props = Map::new();
            for (k, s, _) in *keys {
                props.insert(k.to_string(), spec(s));
            }
            let required: Vec<&str> = keys
                .iter()
                .filter(|(_, _, r)| *r)
                .map(|(k, _, _)| *k)
                .collect();
            json!({ "type": "object", "properties": props, "required": required, "additionalProperties": keys.is_empty() })
        }
        Spec::OneOf(options) => json!({ "anyOf": options.iter().map(spec).collect::<Vec<_>>() }),
        Spec::List(item) => json!({ "type": "array", "items": spec(item) }),
        Spec::Marks => json!({ "type": "array", "items": { "$ref": "#/$defs/mark" } }),
    }
}

fn style() -> Value {
    let mut props = Map::new();
    for k in STYLE_KEYS {
        props.insert(k.to_string(), formula());
    }
    props.insert("dash".into(), json!({ "anyOf": [{ "enum": ["solid", "dashed", "dotted"] }, { "type": "array", "items": { "type": "number" } }] }));
    props.insert("cap".into(), json!({ "enum": ["round", "butt", "square"] }));
    props.insert(
        "join".into(),
        json!({ "enum": ["round", "miter", "bevel"] }),
    );
    json!({ "type": "object", "properties": props, "additionalProperties": false })
}

fn common() -> Map<String, Value> {
    let mut props = Map::new();
    for k in COMMON {
        let v = match *k {
            "view" | "name" | "focus_group" | "id" => ident(),
            "legend" => json!({ "type": "string" }),
            "z" => json!({ "type": "number" }),
            "style" | "highlight" => json!({ "$ref": "#/$defs/style" }),
            "label" => {
                json!({ "anyOf": [{ "type": "string" }, { "type": "object", "required": ["text"], "properties": {
                "text": { "type": "string" }, "at": { "enum": ["end", "mid", "start"] }, "offset": formula(), "anchor": { "enum": ANCHORS }, "size": formula()
            }, "additionalProperties": false }] })
            }
            "repeat" => {
                json!({ "type": "object", "maxProperties": 2, "additionalProperties": formula() })
            }
            "drag" => {
                json!({ "anyOf": [{ "const": false }, { "type": "object", "additionalProperties": formula() }] })
            }
            _ => formula(),
        };
        props.insert(k.to_string(), v);
    }
    props
}

fn mark() -> Value {
    let variants: Vec<Value> = KINDS
        .iter()
        .map(|k| {
            let mut props = common();
            props.insert(k.name.to_string(), spec(&k.main));
            for (name, s) in k.options {
                props.insert(name.to_string(), spec(s));
            }
            json!({ "type": "object", "required": [k.name], "properties": props, "additionalProperties": false })
        })
        .collect();
    json!({ "anyOf": variants })
}

fn axis() -> Value {
    let mut props = Map::new();
    for k in AXIS_KEYS {
        props.insert(k.to_string(), json!({}));
    }
    props.insert(
        "range".into(),
        json!({ "type": "array", "minItems": 2, "maxItems": 2, "items": formula() }),
    );
    props.insert("label".into(), json!({ "type": "string" }));
    props.insert("scale".into(), json!({ "enum": ["linear", "log"] }));
    json!({ "type": "object", "required": ["range"], "properties": props, "additionalProperties": false })
}

fn view() -> Value {
    json!({ "type": "object", "required": ["x", "y"], "additionalProperties": false, "properties": {
        "id": ident(), "x": axis(), "y": axis(),
        "aspect": { "enum": ["free", "equal"] }, "axes": { "enum": AXES }, "grid": { "enum": [false, "major", "minor"] },
        "area": { "type": "array", "items": { "type": "integer", "minimum": 0 } }, "clip": { "type": "boolean" },
        "background": { "enum": ["none", "panel"] }, "title": { "type": "string" }, "legend": { "enum": LEGENDS },
        "coords": { "enum": ["cartesian", "polar"] }, "padding": { "type": "number" }
    } })
}

fn value_decl() -> Value {
    let mut props = Map::new();
    for k in values::KEYS {
        props.insert(k.to_string(), json!({}));
    }
    props.insert("name".into(), ident());
    props.insert(
        "type".into(),
        json!({ "enum": ["number", "choice", "point", "flag"] }),
    );
    json!({ "type": "object", "required": ["name", "type", "default"], "properties": props, "additionalProperties": false })
}

/// The schema of a `hapto: 2` figure document.
pub fn json_schema() -> Value {
    let mut props = Map::new();
    for k in KEYS {
        props.insert(k.to_string(), json!({}));
    }
    props.insert("hapto".into(), json!({ "const": 2 }));
    props.insert("id".into(), ident());
    props.insert("alt".into(), json!({ "type": "string" }));
    props.insert(
        "values".into(),
        json!({ "type": "array", "items": value_decl() }),
    );
    props.insert(
        "let".into(),
        json!({ "type": "object", "additionalProperties": formula() }),
    );
    props.insert("view".into(), view());
    props.insert("views".into(), json!({ "type": "array", "items": view() }));
    props.insert(
        "marks".into(),
        json!({ "type": "array", "items": { "$ref": "#/$defs/mark" } }),
    );
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://github.com/m4nd0r4s/hapto/schema/figure-2.json",
        "title": "hapto figure document, format 2",
        "type": "object",
        "required": ["hapto", "alt"],
        "properties": props,
        "additionalProperties": false,
        "$defs": { "mark": mark(), "style": style() }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_schema_covers_the_tables() {
        let schema = json_schema();
        let top = schema["properties"].as_object().unwrap();
        for k in KEYS {
            assert!(top.contains_key(*k), "top-level key {k} missing");
        }
        let marks = schema["$defs"]["mark"]["anyOf"].as_array().unwrap();
        assert_eq!(marks.len(), KINDS.len());
        for (variant, kind) in marks.iter().zip(KINDS) {
            let props = variant["properties"].as_object().unwrap();
            assert!(props.contains_key(kind.name));
            for c in COMMON {
                assert!(props.contains_key(*c), "{} lacks {c}", kind.name);
            }
        }
        // It serialises: what `hapto schema` prints.
        assert!(serde_json::to_string_pretty(&schema).unwrap().len() > 1000);
    }
}
