//! M0: is rebuilding the scene from state on every change fast enough?
//!
//! ```bash
//! cargo run --release -p hapto-core --example m0_rebuild
//! ```
//!
//! SPEC-2.0 §3.3 rebuilds the whole kinemo scene from state on every change,
//! with every prop a literal. This measures that model before anything is
//! built on it: each frame constructs a fresh `Scene`, then lays it out and
//! rasterises it, and the three phases are timed apart.
//!
//! Three workloads, each at the desktop size (720×405 CSS px at DPR 2) and
//! the phone size (360×203 CSS px at DPR 2):
//!
//! - `simple`: axes, a curve of 400 points whose shape follows the value, a
//!   few dots and ten labels, four of them live. A typical figure.
//! - `labels`: `simple` plus 200 labels, half of them reading the value, so
//!   every frame shapes 100 strings it has never seen. The worst case for text.
//! - `math`: `simple` plus eight TeX formulas, two of them with a number
//!   that follows the value, so two formulas are typeset anew every frame.
//! - `heat`: `simple` plus a heat map at 4 CSS px cells, quantised to 64
//!   levels and merged into one path per level — the batching rule of §3.3.
//! - `underlay`: the same heat map painted as pixels under the scene instead:
//!   cells evaluated once, interpolated bilinearly to full resolution, and the
//!   scene rendered with a transparent background and composited over it.
//! - `arrows`: `simple` plus one 10 000-segment path standing for a vector
//!   field's shafts.
//!
//! Each run writes one PNG per workload, to be looked at.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use kinemo_eval::{Evaluator, TimelineIndex};
use kinemo_ir::{Lerp, Object, Scene, SceneConfig, Signal, Value};
use kinemo_layout::Layout;
use kinemo_render::raster::{encode_png, rasterize};
use kinemo_render::{display_list_with, FrameSize};

const FRAMES: usize = 60;
const WARMUP: usize = 5;
const FRAME_WIDTH_UNITS: f64 = 16.0;

/// Pixel geometry of one render, and the conversions SPEC-2.0 §7.1 asks for:
/// data units for positions, CSS pixels for ink.
#[derive(Clone, Copy)]
struct Canvas {
    css_width: f64,
    css_height: f64,
    dpr: f64,
}

impl Canvas {
    fn pixels(&self) -> (u32, u32) {
        (
            (self.css_width * self.dpr).round() as u32,
            (self.css_height * self.dpr).round() as u32,
        )
    }

    /// The frame's height in scene units, so the frame has the canvas's aspect.
    fn frame_height_units(&self) -> f64 {
        FRAME_WIDTH_UNITS * self.css_height / self.css_width
    }

    /// kinemo scales `stroke_width` by `height / 1080`, not by scene units.
    fn stroke(&self, css_px: f64) -> f64 {
        let (_, height) = self.pixels();
        css_px * self.dpr * 1080.0 / height as f64
    }

    /// kinemo's text `size` is in scene units: `pixels = size * width / frame_w`.
    fn text_size(&self, css_px: f64) -> f64 {
        let (width, _) = self.pixels();
        css_px * self.dpr * FRAME_WIDTH_UNITS / width as f64
    }

    /// A data range onto the frame: x in [-8, 8], y in [-h/2, h/2].
    fn map(&self, x: f64, y: f64, x_range: (f64, f64), y_range: (f64, f64)) -> (f64, f64) {
        let half_w = FRAME_WIDTH_UNITS / 2.0 * 0.9;
        let half_h = self.frame_height_units() / 2.0 * 0.9;
        (
            (x - x_range.0) / (x_range.1 - x_range.0) * 2.0 * half_w - half_w,
            (y - y_range.0) / (y_range.1 - y_range.0) * 2.0 * half_h - half_h,
        )
    }
}

/// A scene assembled from literal props, the way the rebuild model emits one.
struct Literal {
    scene: Scene,
}

impl Literal {
    fn new(canvas: &Canvas) -> Self {
        let config = SceneConfig {
            frame_w: FRAME_WIDTH_UNITS,
            frame_h: canvas.frame_height_units(),
            ..Default::default()
        };
        let mut scene = Scene::new(config);
        scene.duration = 0.0;
        Literal { scene }
    }

    fn signal(&mut self, value: Value) -> u32 {
        let id = self.scene.signals.len() as u32;
        self.scene.signals.push(Signal {
            id,
            initial: value,
            lerp: Default::default(),
            timeline: Vec::new(),
            owner: None,
            span: Default::default(),
        });
        id
    }

    fn object(&mut self, kind: &str, props: Vec<(&str, Value)>) {
        let id = self.scene.objects.len() as u32;
        let mut map = BTreeMap::new();
        for (name, value) in props {
            let signal = self.signal(value);
            map.insert(name.to_string(), signal);
        }
        self.scene.objects.push(Object {
            id,
            kind: kind.to_string(),
            props: map,
            // kinemo's `present` starts at false: without this, nothing draws.
            presence: vec![(0.0, true)],
            ..Default::default()
        });
        self.scene.roots.push(id);
    }

    fn stroked(
        &mut self,
        kind: &str,
        mut props: Vec<(&'static str, Value)>,
        colour: [f64; 4],
        width: f64,
    ) {
        props.push(("stroke", Value::Color(colour)));
        props.push(("stroke_width", Value::Float(width)));
        self.object(kind, props);
    }

    fn filled(&mut self, kind: &str, mut props: Vec<(&'static str, Value)>, colour: [f64; 4]) {
        props.push(("fill", Value::Color(colour)));
        // kinemo defaults `fill_opacity` to 0: a fill without it is invisible.
        props.push(("fill_opacity", Value::Float(1.0)));
        self.object(kind, props);
    }

    /// A TeX formula. A childless `math` object draws nothing: it needs a
    /// `glyphs` run marked `rest` as its child, which is what draws the glyphs.
    fn math(&mut self, tex: String, x: f64, y: f64, size: f64, colour: [f64; 4]) {
        let math_id = self.scene.objects.len() as u32;
        let rest_id = math_id + 1;
        let mut props = BTreeMap::new();
        for (name, value) in [
            ("tex", Value::Str(tex)),
            ("size", Value::Float(size)),
            ("display", Value::Bool(false)),
            ("x", Value::Float(x)),
            ("y", Value::Float(y)),
        ] {
            let signal = self.signal(value);
            props.insert(name.to_string(), signal);
        }
        let children = self.signal(Value::List(vec![Value::Object(rest_id)]));
        self.scene.signals[children as usize].lerp = Lerp::Layout;
        self.scene.objects.push(Object {
            id: math_id,
            kind: "math".into(),
            props,
            children: Some(children),
            presence: vec![(0.0, true)],
            ..Default::default()
        });
        self.scene.roots.push(math_id);

        let mut rest_props = BTreeMap::new();
        for (name, value) in [
            ("rest", Value::Bool(true)),
            ("fill", Value::Color(colour)),
            ("fill_opacity", Value::Float(1.0)),
        ] {
            let signal = self.signal(value);
            rest_props.insert(name.to_string(), signal);
        }
        self.scene.objects.push(Object {
            id: rest_id,
            kind: "glyphs".into(),
            parent: Some(math_id),
            props: rest_props,
            presence: vec![(0.0, true)],
            ..Default::default()
        });
    }

    fn text(&mut self, text: String, x: f64, y: f64, size: f64, colour: [f64; 4]) {
        self.filled(
            "text",
            vec![
                ("text", Value::Str(text)),
                ("size", Value::Float(size)),
                ("x", Value::Float(x)),
                ("y", Value::Float(y)),
            ],
            colour,
        );
    }
}

const INK: [f64; 4] = [0.11, 0.106, 0.098, 1.0];
const MUTED: [f64; 4] = [0.54, 0.52, 0.5, 1.0];
const ACCENT: [f64; 4] = [0.627, 0.322, 0.176, 1.0];
const COOL: [f64; 4] = [0.0, 0.447, 0.698, 1.0];

#[derive(Clone, Copy, PartialEq)]
enum Workload {
    Simple,
    Labels,
    Math,
    Heat,
    Underlay,
    Arrows,
}

impl Workload {
    fn name(self) -> &'static str {
        match self {
            Workload::Simple => "simple",
            Workload::Labels => "labels",
            Workload::Math => "math",
            Workload::Heat => "heat",
            Workload::Underlay => "underlay",
            Workload::Arrows => "arrows",
        }
    }
}

/// The whole scene for one state, rebuilt from nothing.
fn build(canvas: &Canvas, workload: Workload, a: f64) -> Scene {
    let mut s = Literal::new(canvas);
    let x_range = (-4.0, 4.0);
    let y_range = (-3.0, 3.0);
    let map = |x: f64, y: f64| canvas.map(x, y, x_range, y_range);
    let hair = canvas.stroke(1.0);
    let line = canvas.stroke(2.0);
    let label = canvas.text_size(13.0);

    if workload == Workload::Heat {
        heat_map(&mut s, canvas, a, x_range, y_range);
    }
    if workload == Workload::Arrows {
        vector_field(&mut s, canvas, a, x_range, y_range);
    }

    // Axes.
    let (x0, y0) = map(x_range.0, 0.0);
    let (x1, _) = map(x_range.1, 0.0);
    s.stroked(
        "line",
        vec![
            ("start", Value::Vec2([x0, y0])),
            ("end", Value::Vec2([x1, y0])),
        ],
        MUTED,
        hair,
    );
    let (ax, ay0) = map(0.0, y_range.0);
    let (_, ay1) = map(0.0, y_range.1);
    s.stroked(
        "line",
        vec![
            ("start", Value::Vec2([ax, ay0])),
            ("end", Value::Vec2([ax, ay1])),
        ],
        MUTED,
        hair,
    );

    // A curve whose shape follows the value: the thing 1.0 could not draw.
    let curve: Vec<Value> = (0..400)
        .map(|i| {
            let x = x_range.0 + (x_range.1 - x_range.0) * i as f64 / 399.0;
            let (px, py) = map(x, a * (x * 1.3).sin() + 0.2 * x);
            Value::Vec2([px, py])
        })
        .collect();
    s.stroked(
        "polyline",
        vec![("points", Value::List(curve))],
        ACCENT,
        line,
    );

    // A few dots riding the curve.
    for k in 0..3 {
        let x = -2.0 + 2.0 * k as f64 + a * 0.3;
        let (px, py) = map(x, a * (x * 1.3).sin() + 0.2 * x);
        s.filled(
            "dot",
            vec![
                ("r", Value::Float(0.08)),
                ("x", Value::Float(px)),
                ("y", Value::Float(py)),
            ],
            ACCENT,
        );
    }

    // Ten labels: six tick labels that never change, four that read the value.
    for k in 0..6 {
        let x = -3.0 + k as f64 * 1.2;
        let (px, py) = map(x, -0.35);
        s.text(format!("{x:.1}"), px, py, label, MUTED);
    }
    for k in 0..4 {
        let (px, py) = map(-3.5 + 2.0 * k as f64, 2.6);
        s.text(format!("a = {:.3}", a + k as f64), px, py, label, INK);
    }

    if workload == Workload::Math {
        let size = canvas.text_size(16.0);
        let fixed = [
            r"\frac{\partial H}{\partial q}",
            r"\dot{x} = (J - R)\nabla H",
            r"\int_a^b f(x)\,dx",
            r"\sum_{k=1}^{n} \frac{\sin(kx)}{k}",
            r"\begin{pmatrix} 0 & 1 \\ -1 & 0 \end{pmatrix}",
            r"\sqrt{x^2 + y^2}",
        ];
        for (k, tex) in fixed.iter().enumerate() {
            let (px, py) = map(-3.0 + (k % 3) as f64 * 3.0, -1.6 - (k / 3) as f64 * 1.0);
            s.math(tex.to_string(), px, py, size, INK);
        }
        // Live: a number inside TeX is a new formula every frame.
        let (px, py) = map(-2.0, 1.6);
        s.math(format!(r"a = {a:.3}"), px, py, size, ACCENT);
        let (px, py) = map(2.0, 1.6);
        s.math(
            format!(r"\frac{{a}}{{2}} = {:.3}", a / 2.0),
            px,
            py,
            size,
            ACCENT,
        );
    }

    if workload == Workload::Labels {
        for k in 0..200 {
            let col = (k % 20) as f64;
            let row = (k / 20) as f64;
            let (px, py) = map(-3.8 + col * 0.4, -2.8 + row * 0.5);
            let text = if k % 2 == 0 {
                format!("n{k}")
            } else {
                // A live readout: a string the cache has never seen.
                format!("{:.4}", a * k as f64)
            };
            s.text(text, px, py, canvas.text_size(11.0), INK);
        }
    }

    s.scene
}

/// A heat map at 4 CSS px cells, quantised to 64 levels, each level one path.
fn heat_map(s: &mut Literal, canvas: &Canvas, a: f64, x_range: (f64, f64), y_range: (f64, f64)) {
    use std::fmt::Write;
    let columns = (canvas.css_width / 4.0) as usize;
    let rows = (canvas.css_height / 4.0) as usize;
    let mut paths = vec![String::new(); 64];
    let cell_x = (x_range.1 - x_range.0) / columns as f64;
    let cell_y = (y_range.1 - y_range.0) / rows as f64;
    for row in 0..rows {
        let y = y_range.0 + (row as f64 + 0.5) * cell_y;
        let mut run_start = 0;
        let mut run_level = usize::MAX;
        for column in 0..=columns {
            // One past the last column closes the final run.
            let level = if column == columns {
                usize::MAX - 1
            } else {
                let x = x_range.0 + (column as f64 + 0.5) * cell_x;
                let value = (x * y * a).sin();
                (((value + 1.0) / 2.0) * 63.0).round().clamp(0.0, 63.0) as usize
            };
            if level != run_level {
                if run_level < 64 {
                    let (left, bottom) = canvas.map(
                        x_range.0 + run_start as f64 * cell_x,
                        y_range.0 + row as f64 * cell_y,
                        x_range,
                        y_range,
                    );
                    let (right, top) = canvas.map(
                        x_range.0 + column as f64 * cell_x,
                        y_range.0 + (row + 1) as f64 * cell_y,
                        x_range,
                        y_range,
                    );
                    let _ = write!(
                        paths[run_level],
                        "M{left:.4} {bottom:.4}H{right:.4}V{top:.4}H{left:.4}Z"
                    );
                }
                run_start = column;
                run_level = level;
            }
        }
    }
    for (level, d) in paths.into_iter().enumerate() {
        if d.is_empty() {
            continue;
        }
        let t = level as f64 / 63.0;
        let colour = [0.2 + 0.7 * t, 0.3, 0.9 - 0.7 * t, 1.0];
        s.filled(
            "path",
            vec![("d", Value::Str(d)), ("closed", Value::Bool(true))],
            colour,
        );
    }
}

/// One path of 10 000 segments: a vector field's shafts, batched.
fn vector_field(
    s: &mut Literal,
    canvas: &Canvas,
    a: f64,
    x_range: (f64, f64),
    y_range: (f64, f64),
) {
    use std::fmt::Write;
    let mut d = String::new();
    for i in 0..100 {
        for j in 0..100 {
            let x = x_range.0 + (x_range.1 - x_range.0) * (i as f64 + 0.5) / 100.0;
            let y = y_range.0 + (y_range.1 - y_range.0) * (j as f64 + 0.5) / 100.0;
            let (fx, fy) = (-y * a, x);
            let norm = (fx * fx + fy * fy).sqrt().max(1e-9) * 30.0;
            let (sx, sy) = canvas.map(x, y, x_range, y_range);
            let (ex, ey) = canvas.map(x + fx / norm, y + fy / norm, x_range, y_range);
            let _ = write!(d, "M{sx:.4} {sy:.4}L{ex:.4} {ey:.4}");
        }
    }
    s.stroked("path", vec![("d", Value::Str(d))], COOL, canvas.stroke(1.0));
}

/// The heat map as pixels: the same 4 CSS px cells, interpolated bilinearly
/// to full resolution, over the theme background outside the plot area.
fn heat_pixels(canvas: &Canvas, a: f64) -> Vec<u8> {
    let (width, height) = canvas.pixels();
    let (x_range, y_range) = ((-4.0, 4.0), (-3.0, 3.0));
    let columns = (canvas.css_width / 4.0) as usize;
    let rows = (canvas.css_height / 4.0) as usize;

    // Evaluate once per cell centre: this is all the formula work there is.
    let mut cells = vec![[0.0f32; 3]; columns * rows];
    for row in 0..rows {
        let y = y_range.0 + (y_range.1 - y_range.0) * (row as f64 + 0.5) / rows as f64;
        for column in 0..columns {
            let x = x_range.0 + (x_range.1 - x_range.0) * (column as f64 + 0.5) / columns as f64;
            let t = (((x * y * a).sin() + 1.0) / 2.0) as f32;
            cells[row * columns + column] = [0.2 + 0.7 * t, 0.3, 0.9 - 0.7 * t];
        }
    }

    // The plot area in pixels: the frame maps data onto 90% of each side.
    let (w, h) = (width as f32, height as f32);
    let (left, right) = (w * 0.05, w * 0.95);
    let (top, bottom) = (h * 0.05, h * 0.95);
    let background = [0.992f32, 0.992, 0.988];

    let mut out = vec![0u8; (width * height * 4) as usize];
    for py in 0..height as usize {
        let fy = py as f32 + 0.5;
        // Data y grows upwards; rows grow with data y.
        let v = ((bottom - fy) / (bottom - top)) * rows as f32 - 0.5;
        for px in 0..width as usize {
            let fx = px as f32 + 0.5;
            let colour = if fx < left || fx > right || fy < top || fy > bottom {
                background
            } else {
                let u = ((fx - left) / (right - left)) * columns as f32 - 0.5;
                let (c0, r0) = (u.floor().max(0.0) as usize, v.floor().max(0.0) as usize);
                let (c1, r1) = ((c0 + 1).min(columns - 1), (r0 + 1).min(rows - 1));
                let (tu, tv) = (
                    (u - c0 as f32).clamp(0.0, 1.0),
                    (v - r0 as f32).clamp(0.0, 1.0),
                );
                let mut colour = [0.0f32; 3];
                for (channel, value) in colour.iter_mut().enumerate() {
                    let top_row = cells[r0 * columns + c0][channel] * (1.0 - tu)
                        + cells[r0 * columns + c1][channel] * tu;
                    let bottom_row = cells[r1 * columns + c0][channel] * (1.0 - tu)
                        + cells[r1 * columns + c1][channel] * tu;
                    *value = top_row * (1.0 - tv) + bottom_row * tv;
                }
                colour
            };
            let i = (py * width as usize + px) * 4;
            out[i] = (colour[0] * 255.0) as u8;
            out[i + 1] = (colour[1] * 255.0) as u8;
            out[i + 2] = (colour[2] * 255.0) as u8;
            out[i + 3] = 255;
        }
    }
    out
}

/// Straight-alpha `scene` over an opaque `under`, in place.
fn composite_over(scene: &mut [u8], under: &[u8]) {
    let pixels = scene.as_chunks_mut::<4>().0.iter_mut();
    for (pixel, below) in pixels.zip(under.as_chunks::<4>().0) {
        let alpha = pixel[3] as u32;
        for channel in 0..3 {
            pixel[channel] = ((pixel[channel] as u32 * alpha
                + below[channel] as u32 * (255 - alpha))
                / 255) as u8;
        }
        pixel[3] = 255;
    }
}

struct Timings {
    build: Vec<f64>,
    layout: Vec<f64>,
    raster: Vec<f64>,
    total: Vec<f64>,
}

fn percentile(values: &[f64], p: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let index = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[index]
}

fn run(canvas: &Canvas, workload: Workload, png: &str) -> Timings {
    let (width, height) = canvas.pixels();
    let mut timings = Timings {
        build: Vec::new(),
        layout: Vec::new(),
        raster: Vec::new(),
        total: Vec::new(),
    };
    for frame in 0..(WARMUP + FRAMES) {
        // A value that moves every frame, as a drag would.
        let a = 0.5 + 1.5 * (frame as f64 / (WARMUP + FRAMES) as f64);

        let underlay = workload == Workload::Underlay;
        let start = Instant::now();
        let scene = build(canvas, workload, a);
        let pixels = underlay.then(|| heat_pixels(canvas, a));
        let built = Instant::now();
        let index = Arc::new(TimelineIndex::new(&scene));
        let evaluator = Evaluator::with_index(&scene, index);
        let list = display_list_with(
            &Layout::new(&evaluator),
            0.0,
            FrameSize { width, height },
            underlay,
        );
        let laid_out = Instant::now();
        let mut image = rasterize(&list, true);
        if let Some(under) = pixels {
            composite_over(&mut image.rgba, &under);
        }
        let done = Instant::now();

        if frame == WARMUP + FRAMES - 1 {
            std::fs::write(png, encode_png(&image)).expect("write png");
        }
        if frame >= WARMUP {
            let ms = |a: Instant, b: Instant| (b - a).as_secs_f64() * 1000.0;
            timings.build.push(ms(start, built));
            timings.layout.push(ms(built, laid_out));
            timings.raster.push(ms(laid_out, done));
            timings.total.push(ms(start, done));
        }
    }
    timings
}

fn main() {
    let sizes = [
        (
            "desktop 720x405@2",
            Canvas {
                css_width: 720.0,
                css_height: 405.0,
                dpr: 2.0,
            },
        ),
        (
            "phone 360x203@2",
            Canvas {
                css_width: 360.0,
                css_height: 203.0,
                dpr: 2.0,
            },
        ),
    ];
    let workloads = [
        Workload::Simple,
        Workload::Labels,
        Workload::Math,
        Workload::Heat,
        Workload::Underlay,
        Workload::Arrows,
    ];

    println!("{FRAMES} frames per case after {WARMUP} warm-up; milliseconds, p50 / p95\n");
    println!(
        "{:<20} {:<8} {:>15} {:>15} {:>15} {:>15}",
        "size", "workload", "build", "layout+list", "raster", "total"
    );
    for (size_name, canvas) in &sizes {
        for workload in workloads {
            let device = if size_name.starts_with("desktop") {
                "desktop"
            } else {
                "phone"
            };
            let png = format!("m0-{}-{device}.png", workload.name());
            let t = run(canvas, workload, &png);
            let cell =
                |v: &[f64]| format!("{:.2} / {:.2}", percentile(v, 0.5), percentile(v, 0.95));
            println!(
                "{:<20} {:<8} {:>15} {:>15} {:>15} {:>15}",
                size_name,
                workload.name(),
                cell(&t.build),
                cell(&t.layout),
                cell(&t.raster),
                cell(&t.total)
            );
        }
    }
}
