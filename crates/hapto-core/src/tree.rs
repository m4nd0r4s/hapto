//! A document as a tree, and a reader that walks it with paths.
//!
//! JSON and YAML 1.2 read into the same tree (SPEC-2.0 §6.1). Decoding is
//! done by hand over it rather than by derived deserialisers, because the
//! format promises errors a deserialiser cannot give: every unknown key
//! named with the nearest known one, every problem at its JSON Pointer, and
//! every problem reported rather than the first.

use serde_json::{Map, Value as Tree};
use yaml_rust2::{Yaml, YamlLoader};

use hapto_expr::Error;

use crate::codes;

/// Read a document written as JSON or YAML. JSON is tried first only when the
/// text starts like JSON, so YAML's error messages are the ones a YAML author
/// sees.
pub fn parse(text: &str) -> Result<Tree, Error> {
    let trimmed = text.trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        if let Ok(tree) = serde_json::from_str::<Tree>(text) {
            return Ok(tree);
        }
    }
    let docs = YamlLoader::load_from_str(text).map_err(|e| {
        let m = e.marker();
        Error::new(
            codes::E400,
            format!("this is not valid YAML or JSON: {}", e.info()),
        )
        .hint(format!("line {}, column {}", m.line(), m.col() + 1))
    })?;
    match docs.as_slice() {
        [] => Ok(Tree::Object(Map::new())),
        [doc] => yaml_to_tree(doc, ""),
        _ => Err(Error::new(
            codes::E400,
            "a figure is one YAML document, and this has several",
        )
        .hint("remove the `---` separators")),
    }
}

fn yaml_to_tree(y: &Yaml, path: &str) -> Result<Tree, Error> {
    let here = || {
        if path.is_empty() {
            "/".to_string()
        } else {
            path.to_string()
        }
    };
    Ok(match y {
        Yaml::Real(s) => match s.parse::<f64>() {
            Ok(n) => serde_json::Number::from_f64(n)
                .map(Tree::Number)
                .unwrap_or(Tree::Null),
            Err(_) => Tree::String(s.clone()),
        },
        Yaml::Integer(i) => Tree::Number((*i).into()),
        Yaml::String(s) => Tree::String(s.clone()),
        Yaml::Boolean(b) => Tree::Bool(*b),
        Yaml::Array(items) => Tree::Array(
            items
                .iter()
                .enumerate()
                .map(|(i, item)| yaml_to_tree(item, &index_path(path, i)))
                .collect::<Result<_, _>>()?,
        ),
        Yaml::Hash(hash) => {
            let mut map = Map::new();
            for (k, v) in hash {
                let key = match k {
                    Yaml::String(s) => s.clone(),
                    Yaml::Integer(i) => i.to_string(),
                    Yaml::Real(r) => r.clone(),
                    Yaml::Boolean(b) => b.to_string(),
                    _ => {
                        return Err(
                            Error::new(codes::E403, "a key must be a plain word").at_path(here())
                        )
                    }
                };
                let child = yaml_to_tree(v, &child_path(path, &key))?;
                map.insert(key, child);
            }
            Tree::Object(map)
        }
        Yaml::Null => Tree::Null,
        Yaml::Alias(_) | Yaml::BadValue => {
            return Err(Error::new(codes::E400, "this YAML value cannot be read").at_path(here()))
        }
    })
}

/// RFC 6901 escaping of one path segment.
pub fn escape(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

pub fn child_path(path: &str, key: &str) -> String {
    format!("{path}/{}", escape(key))
}

pub fn index_path(path: &str, i: usize) -> String {
    format!("{path}/{i}")
}

/// How a tree value reads in a message.
pub fn describe(t: &Tree) -> &'static str {
    match t {
        Tree::Null => "nothing",
        Tree::Bool(_) => "true/false",
        Tree::Number(_) => "a number",
        Tree::String(_) => "text",
        Tree::Array(_) => "a list",
        Tree::Object(_) => "a map",
    }
}

/// A position in the document: the value and its JSON Pointer.
#[derive(Clone, Copy)]
pub struct At<'a> {
    pub tree: &'a Tree,
    pub path: &'a str,
}

/// Walks a document, collecting every error it finds.
#[derive(Default)]
pub struct Reader {
    pub errors: Vec<Error>,
    pub warnings: Vec<Error>,
}

fn pointer(path: &str) -> String {
    if path.is_empty() {
        "/".into()
    } else {
        path.into()
    }
}

impl Reader {
    pub fn error(&mut self, err: Error) {
        self.errors.push(err);
    }

    pub fn wrong_type(&mut self, at: At, wanted: &str) {
        self.error(
            Error::new(
                codes::E403,
                format!("this should be {wanted}, and it is {}", describe(at.tree)),
            )
            .at_path(pointer(at.path)),
        );
    }

    /// A map whose keys must all be among `known`, and include `required`.
    /// Unknown keys are refused with the nearest known one (§6.3); `color`
    /// is accepted for `colour`, the format's one alias.
    pub fn map<'a>(
        &mut self,
        at: At<'a>,
        known: &[&str],
        required: &[&str],
        what: &str,
    ) -> Option<&'a Map<String, Tree>> {
        let Tree::Object(map) = at.tree else {
            self.wrong_type(at, &format!("a map ({what})"));
            return None;
        };
        for key in map.keys() {
            if known.contains(&key.as_str()) || known.contains(&alias(key).as_str()) {
                continue;
            }
            let mut err = Error::new(codes::E401, format!("{what} has no key \"{key}\""))
                .at_path(child_path(at.path, key));
            err = match nearest(key, known) {
                Some(near) => err.hint(format!("did you mean `{near}`?")),
                None => err.options(known.iter().copied()),
            };
            self.error(err);
        }
        for key in required {
            if !map.contains_key(*key) {
                self.error(
                    Error::new(codes::E402, format!("{what} needs a \"{key}\""))
                        .at_path(pointer(at.path)),
                );
            }
        }
        Some(map)
    }

    pub fn list<'a>(&mut self, at: At<'a>, what: &str) -> Option<&'a Vec<Tree>> {
        match at.tree {
            Tree::Array(items) => Some(items),
            _ => {
                self.wrong_type(at, &format!("a list ({what})"));
                None
            }
        }
    }

    pub fn string(&mut self, at: At, what: &str) -> Option<String> {
        match at.tree {
            Tree::String(s) => Some(s.clone()),
            // A number or flag written where text is expected is still text.
            Tree::Number(n) => Some(n.to_string()),
            Tree::Bool(b) => Some(b.to_string()),
            _ => {
                self.wrong_type(at, &format!("text ({what})"));
                None
            }
        }
    }

    pub fn number(&mut self, at: At, what: &str) -> Option<f64> {
        match at.tree {
            Tree::Number(n) => n.as_f64(),
            // A ratio such as "16/9" is a number too, where one is wanted.
            Tree::String(s) => match ratio(s) {
                Some(n) => Some(n),
                None => {
                    self.wrong_type(at, &format!("a number ({what})"));
                    None
                }
            },
            _ => {
                self.wrong_type(at, &format!("a number ({what})"));
                None
            }
        }
    }

    pub fn boolean(&mut self, at: At, what: &str) -> Option<bool> {
        match at.tree {
            Tree::Bool(b) => Some(*b),
            _ => {
                self.wrong_type(at, &format!("true or false ({what})"));
                None
            }
        }
    }

    /// An identifier: a name the figure gives to something.
    pub fn ident(&mut self, at: At, what: &str) -> Option<String> {
        let s = self.string(at, what)?;
        if is_ident(&s) {
            return Some(s);
        }
        self.error(
            Error::new(codes::E403, format!("\"{s}\" is not a name ({what})"))
                .hint("a name is letters, digits and `_`, starting with a letter")
                .at_path(pointer(at.path)),
        );
        None
    }

    /// One of a fixed set of words.
    pub fn choice(&mut self, at: At, options: &[&str], what: &str) -> Option<String> {
        let s = self.string(at, what)?;
        if options.contains(&s.as_str()) {
            return Some(s);
        }
        let mut err = Error::new(codes::E403, format!("\"{s}\" is not a {what}"))
            .at_path(pointer(at.path))
            .options(options.iter().copied());
        if let Some(near) = nearest(&s, options) {
            err = err.hint(format!("did you mean `{near}`?"));
        }
        self.error(err);
        None
    }
}

/// The canonical spelling of a key: `color` is accepted for `colour`.
pub fn alias(key: &str) -> String {
    key.replace("color", "colour")
}

pub fn is_ident(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `"16/9"` or `"1.5"` as a number.
pub fn ratio(s: &str) -> Option<f64> {
    match s.split_once('/') {
        Some((a, b)) => {
            let (a, b): (f64, f64) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
            (b != 0.0).then_some(a / b)
        }
        None => s.trim().parse().ok(),
    }
}

/// The nearest of `known` to `name`, by edit distance, if plausibly a typo.
pub fn nearest<'a>(name: &str, known: &[&'a str]) -> Option<&'a str> {
    let limit = 2.min(name.chars().count().div_ceil(2)).max(1);
    known
        .iter()
        .map(|k| (distance(name, k), *k))
        .filter(|(d, _)| *d <= limit)
        .min_by_key(|(d, k)| (*d, k.len()))
        .map(|(_, k)| k)
}

fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut row = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            row[j] = (prev[j] + 1).min(row[j - 1] + 1).min(prev[j - 1] + cost);
        }
        prev = row;
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_and_yaml_read_into_the_same_tree() {
        let json = parse(r#"{"hapto": 2, "values": [{"name": "x", "range": [0, 1.5]}]}"#).unwrap();
        let yaml = parse("hapto: 2\nvalues:\n  - { name: x, range: [0, 1.5] }\n").unwrap();
        assert_eq!(json, yaml);
    }

    #[test]
    fn yaml_keeps_order_and_types() {
        let t = parse("b: 1\na: yes\nc: '2'\nd: true\n").unwrap();
        let keys: Vec<&String> = t.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["b", "a", "c", "d"]);
        // YAML 1.2: `yes` is a string, not a flag.
        assert_eq!(t["a"], Tree::String("yes".into()));
        assert_eq!(t["d"], Tree::Bool(true));
    }

    #[test]
    fn bad_yaml_says_where() {
        let e = parse("a: [1, 2\nb: 3").unwrap_err();
        assert_eq!(e.code, codes::E400);
        assert!(e.hint.unwrap().starts_with("line"));
        assert_eq!(parse("a: 1\n---\nb: 2").unwrap_err().code, codes::E400);
    }

    #[test]
    fn unknown_keys_name_the_nearest() {
        let t = parse("{\"lable\": 1, \"color\": 2}").unwrap();
        let mut r = Reader::default();
        r.map(
            At { tree: &t, path: "" },
            &["label", "colour"],
            &["name"],
            "a value",
        );
        let codes: Vec<&str> = r.errors.iter().map(|e| e.code.0).collect();
        assert_eq!(codes, ["E401", "E402"]);
        assert_eq!(r.errors[0].hint.as_deref(), Some("did you mean `label`?"));
        assert_eq!(r.errors[0].path.as_deref(), Some("/lable"));
    }

    #[test]
    fn ratios_are_numbers() {
        assert_eq!(ratio("16/9"), Some(16.0 / 9.0));
        assert_eq!(ratio("1.5"), Some(1.5));
        assert_eq!(ratio("1/0"), None);
    }
}
