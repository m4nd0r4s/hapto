//! Every command, run as a reader would run it: exit status and output.

use std::process::Command;

fn hapto(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_hapto"))
        .args(args)
        .output()
        .expect("run hapto");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn fixture(name: &str) -> String {
    format!("{}/tests/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn write(name: &str, text: &str) -> String {
    let dir = std::env::temp_dir().join(format!("hapto-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    path.to_string_lossy().into_owned()
}

const GOOD: &str = "hapto: 2\nid: good\nalt: a\nvalues: [ { name: x, type: number, range: [-1, 1], default: 0 } ]\nview: { x: { range: [-2, 2] }, y: { range: [-2, 2] } }\nmarks: [ { point: '[x, x**2]', r: 10 }, { function: 'x**2', var: x } ]\nreadouts: [ { id: y, label: height, value: 'x**2', format: '.2f' } ]";

#[test]
fn render_writes_png_and_svg_and_prints_readouts() {
    let fig = write("good.yaml", GOOD);
    let png = write("out.png", "");
    let (code, out, err) = hapto(&[
        "render", &fig, "-o", &png, "--set", "x=0.5", "--width", "300",
    ]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(out.trim(), "height: 0.25");
    assert!(std::fs::read(&png).unwrap().starts_with(b"\x89PNG"));
    let svg = png.replace(".png", ".svg");
    assert_eq!(hapto(&["render", &fig, "-o", &svg]).0, 0);
    assert!(std::fs::read_to_string(&svg).unwrap().starts_with("<svg"));
}

#[test]
fn check_fails_with_status_1_and_json() {
    let good = write("good2.yaml", GOOD);
    let bad = write("bad.yaml", "hapto: 2\nalt: a\nview: { x: { range: [0, 1] }, y: { range: [0, 1] } }\nmarks: [ { point: '[k, 0]' } ]");
    assert_eq!(hapto(&["check", &good]).0, 0);
    let (code, out, _) = hapto(&["check", &good, &bad, "--json"]);
    assert_eq!(code, 1);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v[0]["ok"], true);
    assert_eq!(v[1]["errors"][0]["code"], "E301");
    assert_eq!(v[1]["errors"][0]["path"], "/marks/0/point");
}

#[test]
fn explain_schema_errors_and_bake() {
    let fig = write("good3.yaml", GOOD);
    let (code, out, _) = hapto(&["explain", &fig, "--formula", "/marks/0/point"]);
    assert_eq!(code, 0);
    assert!(out.contains("[x, x**2]  : vec"), "{out}");
    let (_, schema, _) = hapto(&["schema"]);
    let schema: serde_json::Value = serde_json::from_str(&schema).unwrap();
    assert!(schema["$schema"].is_string());
    let (code, out, _) = hapto(&["errors", "E608"]);
    assert_eq!(code, 0);
    assert!(out.starts_with("E608"));
    assert_eq!(hapto(&["errors", "E999"]).0, 2);
    let baked = write("baked.json", "");
    let (code, _, err) = hapto(&[
        "bake",
        &fixture("../../hapto-core/tests/fixtures/maths.yaml"),
        "-o",
        &baked,
    ]);
    assert_eq!(code, 0, "{err}");
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&baked).unwrap()).unwrap();
    assert!(v["baked"].as_str().unwrap().len() > 1000);
}

#[test]
fn usage_mistakes_are_status_2() {
    assert_eq!(hapto(&[]).0, 2);
    assert_eq!(hapto(&["frobnicate"]).0, 2);
    assert_eq!(hapto(&["render", "x.yaml"]).0, 2);
}
