//! The acceptance corpus (SPEC-2.0 §22) against the quality gates of §21:
//! every figure in `examples/corpus` passes `check` with no warnings, and
//! renders as its goldens do at its default and two documented states
//! (`goldens/states.yaml`), in both themes.
//!
//! Regenerate goldens only on purpose, after looking at the new renders:
//!
//! ```bash
//! HAPTO_BLESS=1 cargo test -p hapto-core --features typeset --test corpus
//! ```
//!
//! A failure writes the render and a diff (differing pixels in red) to
//! `target/golden-diffs/`.

// The corpus has TeX in it: without typst it would need baking first.
#![cfg(feature = "typeset")]

use std::path::{Path, PathBuf};

use hapto_core::check::{self, parse_set};
use hapto_core::golden::compare;
use hapto_core::render::Figure;
use hapto_core::scene::Canvas;
use hapto_core::theme::Theme;
use kinemo_render::raster::{encode_png, Image};

const WIDTH: f64 = 720.0;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/corpus")
}

fn figures() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(root())
        .expect("examples/corpus")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "yaml"))
        .collect();
    v.sort();
    v
}

fn decode(path: &Path) -> Option<Image> {
    let file = std::fs::File::open(path).ok()?;
    let mut reader = png::Decoder::new(std::io::BufReader::new(file))
        .read_info()
        .ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    (info.color_type == png::ColorType::Rgba).then_some(Image {
        width: info.width,
        height: info.height,
        rgba: buf,
    })
}

#[test]
fn every_corpus_figure_passes_check() {
    let figs = figures();
    assert!(figs.len() >= 6, "the M2 corpus has six figures");
    for path in figs {
        let text = std::fs::read_to_string(&path).unwrap();
        let r = check::check(&text, Canvas::new(WIDTH, 405.0, 1.0));
        assert!(r.ok(), "{}: {:#?}", path.display(), r.errors);
        assert!(
            r.warnings.is_empty(),
            "{}: {:#?}",
            path.display(),
            r.warnings
        );
    }
}

#[test]
fn every_corpus_figure_matches_its_goldens() {
    let states = hapto_core::tree::parse(
        &std::fs::read_to_string(root().join("goldens/states.yaml")).unwrap(),
    )
    .unwrap();
    let bless = std::env::var_os("HAPTO_BLESS").is_some();
    let goldens = root().join("goldens");
    let diffs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/golden-diffs");
    std::fs::create_dir_all(&goldens).unwrap();
    let mut failures = Vec::new();
    for path in figures() {
        let text = std::fs::read_to_string(&path).unwrap();
        let mut f = Figure::from_text(&text).unwrap();
        let id = f.program.doc.id.clone().expect("corpus figures have ids");
        let documented: Vec<String> = states[&id]
            .as_array()
            .unwrap_or_else(|| panic!("states.yaml has no states for {id}"))
            .iter()
            .map(|s| s.as_str().unwrap().to_string())
            .collect();
        assert_eq!(documented.len(), 2, "{id}: two documented states");
        let height = (WIDTH / f.program.doc.size.aspect).round();
        let canvas = Canvas::new(WIDTH, height, 1.0);
        for (n, set) in std::iter::once(String::new()).chain(documented).enumerate() {
            f.reset();
            check::apply(&mut f, &parse_set(&set).unwrap()).unwrap();
            for theme in ["light", "dark"] {
                let img = f.render(canvas, &Theme::by_name(theme)).unwrap();
                let name = format!("{id}-{n}-{theme}.png");
                let golden = goldens.join(&name);
                if bless {
                    std::fs::write(&golden, encode_png(&img)).unwrap();
                    continue;
                }
                let Some(expected) = decode(&golden) else {
                    failures.push(format!("{name}: no golden (run with HAPTO_BLESS=1)"));
                    continue;
                };
                let d = compare(&img, &expected);
                if !d.passes() {
                    std::fs::create_dir_all(&diffs).unwrap();
                    std::fs::write(diffs.join(&name), encode_png(&img)).unwrap();
                    if let Some(diff) = &d.image {
                        std::fs::write(diffs.join(format!("diff-{name}")), encode_png(diff))
                            .unwrap();
                    }
                    failures.push(format!(
                        "{name}: {:.2}% of pixels differ (state \"{set}\")",
                        d.fraction * 100.0
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "goldens differ:\n{}",
        failures.join("\n")
    );
}
