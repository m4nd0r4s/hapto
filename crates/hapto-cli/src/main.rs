//! `hapto`: the figure language from the command line (SPEC-2.0 §17).
//!
//! Every command reads figure documents and prints for people by default,
//! or JSON with `--json` for agents and CI. Exit status is non-zero when a
//! document has errors, so a script or CI step can rely on it.

use std::process::ExitCode;

use hapto_core::check::{self, Report};
use hapto_core::render::Figure;
use hapto_core::scene::{self, Canvas};
use hapto_core::theme::Theme;
use hapto_expr::error::Entry;
use hapto_expr::Error;

/// `println!` that stops quietly when the reader went away (`hapto schema |
/// head`), instead of panicking on a broken pipe.
macro_rules! say {
    ($($t:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stdout(), $($t)*);
    }};
}

const USAGE: &str = "\
hapto — figures that react

usage:
  hapto render <fig> -o <out.png|out.svg> [--set x=3,mode=a] [--width 720]
               [--height H] [--dpr 2] [--theme light|dark] [--svg]
  hapto check <fig>... [--width 720] [--json]
  hapto explain <fig> [--formula /marks/2/at] [--json]
  hapto bake <fig> -o <fig.json>
  hapto schema
  hapto errors [CODE] [--json]

A figure is a YAML or JSON document (SPEC-2.0 §6).";

/// Parsed flags: positional arguments and `--name value` pairs.
struct Args {
    positional: Vec<String>,
    flags: Vec<(String, Option<String>)>,
}

/// Flags that take no value.
const SWITCHES: &[&str] = &["--json", "--svg", "--help", "-h"];

impl Args {
    fn parse(raw: &[String]) -> Result<Args, String> {
        let (mut positional, mut flags) = (Vec::new(), Vec::new());
        let mut it = raw.iter();
        while let Some(a) = it.next() {
            if a.starts_with('-') && a.len() > 1 {
                if let Some((k, v)) = a.split_once('=') {
                    flags.push((k.to_string(), Some(v.to_string())));
                } else if SWITCHES.contains(&a.as_str()) {
                    flags.push((a.clone(), None));
                } else {
                    let v = it.next().ok_or_else(|| format!("{a} needs a value"))?;
                    flags.push((a.clone(), Some(v.clone())));
                }
            } else {
                positional.push(a.clone());
            }
        }
        Ok(Args { positional, flags })
    }

    fn has(&self, name: &str) -> bool {
        self.flags.iter().any(|(k, _)| k == name)
    }

    fn get(&self, names: &[&str]) -> Option<&str> {
        self.flags
            .iter()
            .rev()
            .find(|(k, _)| names.contains(&k.as_str()))
            .and_then(|(_, v)| v.as_deref())
    }

    fn number(&self, name: &str, default: f64) -> Result<f64, String> {
        match self.get(&[name]) {
            Some(v) => v
                .parse()
                .map_err(|_| format!("{name} wants a number, not \"{v}\"")),
            None => Ok(default),
        }
    }
}

fn main() -> ExitCode {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let Some((command, rest)) = raw.split_first() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let args = match Args::parse(rest) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("hapto: {e}");
            return ExitCode::from(2);
        }
    };
    if args.has("--help") || args.has("-h") {
        say!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let result = match command.as_str() {
        "render" => render(&args),
        "check" => check_cmd(&args),
        "explain" => explain(&args),
        "bake" => bake(&args),
        "schema" => {
            let schema = hapto_core::schema::json_schema();
            say!("{}", serde_json::to_string_pretty(&schema).expect("JSON"));
            Ok(true)
        }
        "errors" => errors(&args),
        "help" | "--help" | "-h" => {
            say!("{USAGE}");
            Ok(true)
        }
        other => Err(format!("\"{other}\" is not a command\n\n{USAGE}")),
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("hapto: {e}");
            ExitCode::from(2)
        }
    }
}

fn read(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))
}

fn print_errors(file: &str, errors: &[Error]) {
    for e in errors {
        eprintln!("{file}: {}\n", e.render());
    }
}

/// A canvas from `--width`, `--height` (else the figure's aspect) and `--dpr`.
fn canvas(args: &Args, aspect: f64, dpr: f64) -> Result<Canvas, String> {
    let width = args.number("--width", 720.0)?;
    let height = match args.get(&["--height"]) {
        Some(_) => args.number("--height", 0.0)?,
        None => (width / aspect).round(),
    };
    Ok(Canvas::new(width, height, args.number("--dpr", dpr)?))
}

fn render(args: &Args) -> Result<bool, String> {
    let [file] = args.positional.as_slice() else {
        return Err("render takes one figure".into());
    };
    let out = args
        .get(&["-o", "--out"])
        .ok_or("render needs -o <out.png>")?;
    let text = read(file)?;
    let mut figure = match Figure::from_text(&text) {
        Ok(f) => f,
        Err(errors) => {
            print_errors(file, &errors);
            return Ok(false);
        }
    };
    if let Some(set) = args.get(&["--set"]) {
        let set = check::parse_set(set)?;
        if let Err(e) = check::apply(&mut figure, &set) {
            print_errors(file, &[e]);
            return Ok(false);
        }
    }
    let theme = Theme::by_name(args.get(&["--theme"]).unwrap_or("light"));
    let aspect = figure.program.doc.size.aspect;
    let canvas = canvas(args, aspect, 2.0)?;
    let svg = args.has("--svg") || out.ends_with(".svg");
    let bytes = match figure.scene(canvas, &theme) {
        Ok(s) if svg => scene::svg(&s, canvas).into_bytes(),
        Ok(s) => kinemo_render::raster::encode_png(&scene::rasterise(&s, canvas, false)),
        Err(e) => {
            print_errors(file, &[e]);
            return Ok(false);
        }
    };
    std::fs::write(out, bytes).map_err(|e| format!("cannot write {out}: {e}"))?;
    for w in &figure.warnings {
        eprintln!("{file}: {}", w.render());
    }
    // What the page would print beside the figure (§13).
    for r in figure.readouts(&theme).map_err(|e| e.render())? {
        if r.place != "none" {
            say!("{}: {}", r.label, r.text);
        }
    }
    Ok(true)
}

fn check_cmd(args: &Args) -> Result<bool, String> {
    if args.positional.is_empty() {
        return Err("check takes one or more figures".into());
    }
    let json = args.has("--json");
    let width = args.number("--width", 720.0)?;
    let mut all_ok = true;
    let mut reports: Vec<(String, Report)> = Vec::new();
    for file in &args.positional {
        let text = read(file)?;
        let report = check::check(&text, Canvas::new(width, (width * 9.0 / 16.0).round(), 1.0));
        all_ok &= report.ok();
        if !json {
            print_report(file, &report);
        }
        reports.push((file.clone(), report));
    }
    if json {
        let out: Vec<serde_json::Value> = reports
            .iter()
            .map(|(file, r)| {
                let mut v = serde_json::to_value(r).expect("JSON");
                v["file"] = file.clone().into();
                v["ok"] = r.ok().into();
                v
            })
            .collect();
        say!("{}", serde_json::to_string_pretty(&out).expect("JSON"));
    }
    Ok(all_ok)
}

fn print_report(file: &str, r: &Report) {
    let status = if r.ok() { "ok" } else { "FAILED" };
    say!("{file}: {status}");
    for e in r.errors.iter().chain(&r.warnings) {
        say!("  {}", e.render().replace('\n', "\n  "));
    }
    if !r.renders.is_empty() {
        let worst = r.renders.iter().map(|x| x.ms).fold(0.0, f64::max);
        let rest = r
            .renders
            .iter()
            .filter(|x| x.state == "default")
            .map(|x| format!("{} {:.1}%", x.theme, x.ink * 100.0))
            .collect::<Vec<_>>()
            .join(", ");
        say!(
            "  {} renders, slowest {worst:.1} ms; ink at rest: {rest}",
            r.renders.len()
        );
    }
}

fn explain(args: &Args) -> Result<bool, String> {
    let [file] = args.positional.as_slice() else {
        return Err("explain takes one figure".into());
    };
    let text = read(file)?;
    let mut rows = match check::explain(&text) {
        Ok(r) => r,
        Err(errors) => {
            print_errors(file, &errors);
            return Ok(false);
        }
    };
    if let Some(only) = args.get(&["--formula"]) {
        rows.retain(|r| r.path == only);
        if rows.is_empty() {
            return Err(format!("{file} has no formula at {only}"));
        }
    }
    if args.has("--json") {
        say!("{}", serde_json::to_string_pretty(&rows).expect("JSON"));
        return Ok(true);
    }
    for r in rows {
        say!("{}  {}  : {}", r.path, r.formula, r.ty);
        for (name, what) in r.names {
            say!("    {name} → {what}");
        }
    }
    Ok(true)
}

fn bake(args: &Args) -> Result<bool, String> {
    let [file] = args.positional.as_slice() else {
        return Err("bake takes one figure".into());
    };
    let out = args
        .get(&["-o", "--out"])
        .ok_or("bake needs -o <fig.json>")?;
    let text = read(file)?;
    // Refuse what would not load anyway, before baking it.
    if let Err(errors) = Figure::from_text(&text) {
        print_errors(file, &errors);
        return Ok(false);
    }
    match hapto_core::maths::bake_document(&text) {
        Ok(json) => {
            std::fs::write(out, json).map_err(|e| format!("cannot write {out}: {e}"))?;
            Ok(true)
        }
        Err(errors) => {
            print_errors(file, &errors);
            Ok(false)
        }
    }
}

fn errors(args: &Args) -> Result<bool, String> {
    let all = hapto_expr::error::CATALOGUE
        .iter()
        .chain(hapto_core::codes::CATALOGUE);
    let mut chosen: Vec<&Entry> = match args.positional.first() {
        Some(code) => all
            .filter(|e| e.code.0.eq_ignore_ascii_case(code))
            .collect(),
        None => all.collect(),
    };
    if chosen.is_empty() {
        return Err(format!(
            "{} is not a code; `hapto errors` lists them",
            args.positional[0]
        ));
    }
    chosen.sort_by_key(|e| e.code.0);
    if args.has("--json") {
        say!("{}", serde_json::to_string_pretty(&chosen).expect("JSON"));
        return Ok(true);
    }
    for e in chosen {
        say!("{}  {}", e.code.0, e.title);
        for line in e.example.lines() {
            say!("      {line}");
        }
    }
    Ok(true)
}
