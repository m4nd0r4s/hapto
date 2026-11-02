//! Render the demo gallery: every figure in `demo/v2/figures`, swept across
//! its first number value, in both themes, plus the errors a bad document
//! gets. Writes PNG frames and `demo/v2/index.html`.
//!
//! ```bash
//! cargo run --release -p hapto-core --features typeset --example gallery
//! open demo/v2/index.html
//! ```
//!
//! The browser runtime for 2.0 does not exist yet (SPEC-2.0 §15), so the page
//! swaps frames rendered here; every frame is the real engine at that value.

use std::fmt::Write as _;
use std::path::Path;

use hapto_core::document::values::{Kind, Step};
use hapto_core::render::Figure;
use hapto_core::scene::Canvas;
use hapto_core::state::Input;
use hapto_core::theme::Theme;

const WIDTH: f64 = 760.0;
const DPR: f64 = 1.5;
const FRAMES: usize = 41;

/// Documents that are wrong on purpose, to show what an author is told.
const BROKEN: &[(&str, &str)] = &[
    (
        "Uma chave com erro de escrita",
        "hapto: 2\nalt: a\nview: { x: { range: [0, 1] }, y: { range: [0, 1] } }\nmarks:\n  - { point: '[0.5, 0.5]', styel: { fill: hot } }",
    ),
    (
        "Uma grafia que se escreve noutras linguagens",
        "hapto: 2\nalt: a\nlet: { k: '2^3' }\nview: { x: { range: [0, 1] }, y: { range: [0, 1] } }\nmarks: [ { point: '[k, 0]' } ]",
    ),
    (
        "Um tipo errado",
        "hapto: 2\nalt: a\nview: { x: { range: [0, 1] }, y: { range: [0, 1] } }\nmarks: [ { point: 'sin(1)' } ]",
    ),
    (
        "Um nome que não existe",
        "hapto: 2\nalt: a\nvalues: [ { name: theta, type: number, range: [0, 1], default: 0 } ]\nview: { x: { range: [0, 1] }, y: { range: [0, 1] } }\nmarks: [ { point: '[thet, 0]' } ]",
    ),
    (
        "Interpolação dentro de TeX",
        "hapto: 2\nalt: a\nvalues: [ { name: s, type: number, range: [0, 2], default: 1 } ]\nview: { x: { range: [0, 1] }, y: { range: [0, 1] } }\nmarks: [ { math: 'x = {s:.2f}', at: '[0.5, 0.5]' } ]",
    ),
    (
        "Uma marca de um milestone futuro",
        "hapto: 2\nalt: a\nview: { x: { range: [0, 1] }, y: { range: [0, 1] } }\nmarks: [ { heatmap: 'x*y' } ]",
    ),
];

struct Rendered {
    id: String,
    title: String,
    source: String,
    value: String,
    labels: Vec<String>,
    /// The readouts at each frame, as the page prints them below (§13).
    readouts: Vec<String>,
    width: u32,
    height: u32,
    warnings: Vec<String>,
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demo/v2");
    let out = root.join("frames");
    std::fs::create_dir_all(&out).expect("make the frames folder");

    let mut paths: Vec<_> = std::fs::read_dir(root.join("figures"))
        .expect("read demo/v2/figures")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "yaml"))
        .collect();
    paths.sort();

    let mut gallery = Vec::new();
    for path in &paths {
        let source = std::fs::read_to_string(path).expect("read a figure");
        let mut figure = Figure::from_text(&source).unwrap_or_else(|errors| {
            for e in errors {
                eprintln!("{}: {}", path.display(), e.render());
            }
            std::process::exit(1);
        });
        let doc = &figure.program.doc;
        let id = doc.id.clone().unwrap_or_else(|| "figure".into());
        let title = doc.title.clone().unwrap_or_else(|| id.clone());
        let height = (WIDTH / doc.size.aspect).round();
        let canvas = Canvas::new(WIDTH, height, DPR);

        // The first number value is the one the slider moves.
        let (name, values) = doc
            .values
            .iter()
            .find_map(|v| match &v.kind {
                Kind::Number {
                    low, high, step, ..
                } => Some((v.name.clone(), sweep(*low, *high, step))),
                _ => None,
            })
            .expect("every demo figure has a number value");

        let mut labels = Vec::new();
        let mut readouts = Vec::new();
        let mut warnings = Vec::new();
        for (i, v) in values.iter().enumerate() {
            let admitted = figure.set(&name, Input::Number(*v)).expect("set the value");
            labels.push(format!("{} = {}", name, trim(admitted.applied.num())));
            let shown = figure.readouts(&Theme::light()).expect("readouts");
            readouts.push(
                shown
                    .iter()
                    .filter(|r| r.place == "below")
                    .map(|r| format!("{}: {}", plain(&r.label), r.text))
                    .collect::<Vec<_>>()
                    .join("   ·   "),
            );
            for theme in ["light", "dark"] {
                let image = figure
                    .render(canvas, &Theme::by_name(theme))
                    .unwrap_or_else(|e| {
                        eprintln!("{}: {}", path.display(), e.render());
                        std::process::exit(1);
                    });
                let file = out.join(format!("{id}-{theme}-{i:02}.png"));
                std::fs::write(file, kinemo_render::raster::encode_png(&image))
                    .expect("write a frame");
            }
            warnings.extend(figure.warnings.iter().map(|w| w.render()));
        }
        warnings.dedup();
        eprintln!("{id}: {} frames", values.len() * 2);
        gallery.push(Rendered {
            id,
            title,
            source,
            value: name,
            labels,
            readouts,
            width: WIDTH as u32,
            height: height as u32,
            warnings,
        });
    }

    let errors: Vec<(String, String, String)> = BROKEN
        .iter()
        .map(|(what, doc)| {
            let shown = match Figure::from_text(doc) {
                Err(errors) => errors
                    .iter()
                    .map(|e| e.render())
                    .collect::<Vec<_>>()
                    .join("\n\n"),
                Ok(mut f) => match f.render(Canvas::new(200.0, 100.0, 1.0), &Theme::light()) {
                    Err(e) => e.render(),
                    Ok(_) => "(nenhum erro)".into(),
                },
            };
            (what.to_string(), doc.to_string(), shown)
        })
        .collect();

    std::fs::write(root.join("index.html"), page(&gallery, &errors)).expect("write the page");
    eprintln!("wrote {}", root.join("index.html").display());
}

/// The values a slider stops at: every integer for integer steps, else FRAMES.
fn sweep(low: f64, high: f64, step: &Step) -> Vec<f64> {
    match step {
        Step::Integer => (low.ceil() as i64..=high.floor() as i64)
            .map(|n| n as f64)
            .collect(),
        _ => (0..FRAMES)
            .map(|i| low + (high - low) * i as f64 / (FRAMES - 1) as f64)
            .collect(),
    }
}

/// A readout label without its TeX dollars, for the HTML page.
fn plain(label: &str) -> String {
    label.replace('$', "").replace("\\", "")
}

fn trim(n: f64) -> String {
    let s = format!("{n:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\'', "&#39;")
}

fn page(gallery: &[Rendered], errors: &[(String, String, String)]) -> String {
    let mut figs = String::new();
    for f in gallery {
        let mid = f.labels.len() / 2;
        let labels = esc(&serde_json::to_string(&f.labels).unwrap());
        let readouts = esc(&serde_json::to_string(&f.readouts).unwrap());
        let warn = if f.warnings.is_empty() {
            String::new()
        } else {
            format!("<pre class=\"warn\">{}</pre>", esc(&f.warnings.join("\n")))
        };
        let _ = write!(
            figs,
            r#"<section class="fig" data-id="{id}" data-labels='{labels}' data-readouts='{readouts}'>
  <h2>{title}</h2>
  <img width="{w}" height="{h}" alt="{title}" src="frames/{id}-light-{mid:02}.png">
  <div class="controls">
    <label>{value}</label>
    <input type="range" min="0" max="{max}" value="{mid}">
    <output></output>
    <button class="play" title="varrer">▶</button>
  </div>
  <div class="readouts"></div>
  {warn}
  <details><summary>documento YAML</summary><pre>{source}</pre></details>
</section>
"#,
            id = f.id,
            title = esc(&f.title),
            w = f.width,
            h = f.height,
            value = esc(&f.value),
            max = f.labels.len() - 1,
            source = esc(&f.source),
        );
    }
    let mut errs = String::new();
    for (what, doc, shown) in errors {
        let _ = writeln!(
            errs,
            "<div class=\"err\"><h3>{}</h3><pre class=\"doc\">{}</pre><pre class=\"msg\">{}</pre></div>",
            esc(what),
            esc(doc),
            esc(shown)
        );
    }
    TEMPLATE
        .replace("{{FIGURES}}", &figs)
        .replace("{{ERRORS}}", &errs)
        .replace("{{COUNT}}", &gallery.len().to_string())
}

const TEMPLATE: &str = r#"<!doctype html>
<html lang="pt">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>hapto 2.0 — o que já desenha</title>
<style>
  :root { --bg:#fdfdfc; --fg:#1c1b19; --muted:#6b6762; --rule:#e4e1dc; --accent:#a0522d; --soft:#f6efe9; }
  :root[data-theme="dark"] { --bg:#1a1918; --fg:#e8e6e3; --muted:#9b958a; --rule:#34322f; --accent:#d98a5f; --soft:#241d16; }
  * { box-sizing: border-box; }
  body { margin:0; background:var(--bg); color:var(--fg);
    font:16px/1.6 Georgia, "Iowan Old Style", serif; display:flex; justify-content:center; padding:3rem 1rem 5rem; }
  main { width:100%; max-width:50rem; }
  h1 { font-size:1.8rem; margin:0 0 .2rem; }
  .sub, .controls, details, .err h3, button { font-family:system-ui, sans-serif; }
  .sub { color:var(--muted); font-size:14px; margin:0 0 1.5rem; }
  .bar { display:flex; gap:.6rem; align-items:center; margin-bottom:2rem; font:14px system-ui; color:var(--muted); }
  button { background:var(--soft); color:var(--accent); border:1px solid var(--rule); border-radius:6px; padding:.25rem .7rem; cursor:pointer; font-size:14px; }
  .fig { margin:0 0 3.2rem; }
  .fig h2 { font-size:1.2rem; margin:0 0 .6rem; }
  .fig img { width:100%; height:auto; display:block; border:1px solid var(--rule); border-radius:8px; }
  .controls { display:flex; align-items:center; gap:.9rem; margin-top:.7rem; font-size:14px; color:var(--muted); }
  input[type=range] { flex:1; accent-color:var(--accent); }
  output { font-variant-numeric:tabular-nums; color:var(--fg); min-width:7rem; }
  details { margin-top:.5rem; font-size:13px; color:var(--muted); }
  pre { background:var(--soft); border:1px solid var(--rule); border-radius:6px; padding:.7rem .9rem; overflow:auto; font:12.5px/1.5 ui-monospace, Menlo, monospace; color:var(--fg); }
  pre.warn { border-color:#e69f00; }
  .readouts { font:14px/1.5 ui-monospace, Menlo, monospace; color:var(--fg); margin-top:.4rem; min-height:1.5em; }
  .err { margin-bottom:1.6rem; }
  .err h3 { font-size:14px; margin:0 0 .3rem; }
  .err pre.msg { border-left:3px solid #cc3d3d; }
  .at { color:var(--accent); background:var(--soft); padding:0 .3em; border-radius:4px; cursor:pointer; border-bottom:1px dashed var(--accent); }
  ul { padding-left:1.2rem; }
</style>
</head>
<body>
<main>
  <h1>hapto 2.0 — o que já desenha</h1>
  <p class="sub">{{COUNT}} figuras, cada uma um documento YAML. Cada fotograma foi desenhado pelo motor real (hapto-core → kinemo) para esse valor.</p>
  <p>Uma figura hapto é uma função do seu estado: valores com domínio, <i>lets</i> e funções na linguagem de fórmulas,
  vistas com eixos, e marcas. Mude o valor e o desenho segue — sem linha temporal. A prosa também mexe no valor:
  leve a tangente para <span class="at" data-fig="tangent" data-i="0">o extremo esquerdo</span>,
  para <span class="at" data-fig="tangent" data-i="11">o máximo local</span> ou
  para <span class="at" data-fig="tangent" data-i="29">o mínimo local</span>.</p>
  <div class="bar">tema <button id="theme">claro / escuro</button></div>
  <ul class="sub">
    <li><b>Linguagem de fórmulas</b>: lets, funções do utilizador, derivadas, integrais, raízes, listas e compreensões, vetores, matrizes, aleatoriedade, histogramas, cores.</li>
    <li><b>Vistas</b>: eixos cross / box / left-bottom / x com ticks (auto, passo, lista, rotulados, π em TeX, log), títulos de eixo e de vista, grelha major/minor, aspeto igual, intervalos que seguem valores, várias vistas numa grelha.</li>
    <li><b>Marcas</b>: point, segment, arrow, vector, line, ray, hline, vline, tangent, secant, drop, polyline, polygon, circle, ellipse, arc, sector, angle, path, bezier, brace, spring, area (com sinal), function (adaptativa e paramétrica), rect, box, group (rodar, escalar, mover), text, math, e <code>repeat</code>.</li>
    <li><b>Texto</b>: interpolação <code>{x:.2f}</code>, TeX tipografado (typst) e etiquetas que evitam colidir, sempre por cima das linhas.</li>
    <li><b>Readouts</b>: números calculados pelas mesmas fórmulas que o desenho, com formato, unidade e comparação com o exato (por baixo, ou dentro da figura).</li>
    <li><b>CLI</b>: <code>hapto render | check | explain | bake | schema | errors</code>; o corpus de aceitação passa <code>hapto check</code> e as imagens golden.</li>
  </ul>
{{FIGURES}}
  <h2>Erros para pessoas</h2>
  <p class="sub">Um documento errado nunca dá uma figura em branco: dá um erro com código, caminho JSON Pointer e sugestão.</p>
{{ERRORS}}
</main>
<script>
const root = document.documentElement;
let theme = matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
const frame = (id, t, i) => `frames/${id}-${t}-${String(i).padStart(2, "0")}.png`;
const figs = [...document.querySelectorAll(".fig")].map(sec => {
  const id = sec.dataset.id, labels = JSON.parse(sec.dataset.labels), readouts = JSON.parse(sec.dataset.readouts);
  const img = sec.querySelector("img"), range = sec.querySelector("input"), out = sec.querySelector("output");
  const show = () => { const i = +range.value; img.src = frame(id, theme, i); out.textContent = labels[i]; sec.querySelector(".readouts").textContent = readouts[i]; };
  range.addEventListener("input", show);
  // Preload every frame so the slider is instant.
  for (const t of ["light", "dark"]) labels.forEach((_, i) => { new Image().src = frame(id, t, i); });
  let timer = null;
  sec.querySelector(".play").addEventListener("click", e => {
    if (timer) { clearInterval(timer); timer = null; e.target.textContent = "▶"; return; }
    e.target.textContent = "❚❚";
    let dir = 1;
    timer = setInterval(() => {
      let i = +range.value + dir;
      if (i > +range.max || i < 0) { dir = -dir; i += 2 * dir; }
      range.value = i; show();
    }, 70);
  });
  return { id, range, show };
});
const apply = () => { root.dataset.theme = theme; figs.forEach(f => f.show()); };
document.getElementById("theme").onclick = () => { theme = theme === "dark" ? "light" : "dark"; apply(); };
document.querySelectorAll(".at").forEach(span => {
  const go = () => { const f = figs.find(f => f.id === span.dataset.fig); f.range.value = span.dataset.i; f.show(); };
  span.addEventListener("mouseenter", go); span.addEventListener("click", go);
});
apply();
</script>
</body>
</html>
"#;
