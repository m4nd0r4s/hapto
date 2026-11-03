//! Native evaluation of `parity.yaml`, pinned in `parity.json`: the Node
//! tests hold the wasm build to the same digits (SPEC-2.0 §4, hapto-p0n.6).
//!
//! `HAPTO_BLESS=1 cargo test -p hapto-web --test parity` rewrites the file.

use hapto_web::session::Session;

#[test]
fn native_digits_are_pinned() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let doc = std::fs::read_to_string(dir.join("parity.yaml")).unwrap();
    let mut s = Session::load(&doc, "yaml").unwrap_or_else(|e| panic!("{e}"));
    let lets: Vec<String> = [
        "trig", "hyper", "exps", "special", "random", "colour", "series",
    ]
    .map(String::from)
    .to_vec();
    let mut got = String::new();
    for x in [0.7, -2.25, 2.9] {
        s.set_state(&format!("{{ \"x\": {x} }}")).unwrap();
        let out = s.evaluate(&lets).unwrap_or_else(|e| panic!("{e}"));
        got.push_str(&serde_json::to_string(&out).unwrap());
        got.push('\n');
    }
    let pinned = dir.join("parity.json");
    if std::env::var_os("HAPTO_BLESS").is_some() || !pinned.exists() {
        std::fs::write(&pinned, &got).unwrap();
    }
    // A Windows checkout may have turned the newlines into CRLF.
    let pinned = std::fs::read_to_string(pinned)
        .unwrap()
        .replace("\r\n", "\n");
    assert_eq!(got, pinned);
}
