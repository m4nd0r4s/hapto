//! Building a kinemo scene from literal props, every change (SPEC-2.0 §3.3).
//!
//! M0 measured the rebuild: a fresh scene costs a fraction of a millisecond
//! and the raster is the rest, so there is no reconciler. Everything kinemo
//! would otherwise default badly is set explicitly here, because each of its
//! defaults once produced a blank frame silently: presence, fill opacity,
//! stroke width in its own units, and the background (docs/m0.md §5).

use std::collections::BTreeMap;
use std::sync::Arc;

use kinemo_eval::{Evaluator, TimelineIndex};
use kinemo_ir::{Lerp, Object, Scene, SceneConfig, Signal, Value};
use kinemo_layout::Layout;
use kinemo_render::raster::{rasterize, Image};
use kinemo_render::{display_list_with, FrameSize};

/// The scene is 16 units wide; its height follows the canvas's aspect (§7.3).
pub const FRAME_W: f64 = 16.0;

/// Where a figure is drawn: CSS pixels and the device-pixel ratio.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Canvas {
    pub css_width: f64,
    pub css_height: f64,
    pub dpr: f64,
}

impl Canvas {
    pub fn new(css_width: f64, css_height: f64, dpr: f64) -> Self {
        Canvas {
            css_width: css_width.max(1.0),
            css_height: css_height.max(1.0),
            dpr: dpr.clamp(0.5, 4.0),
        }
    }

    pub fn pixels(&self) -> (u32, u32) {
        (
            (self.css_width * self.dpr).round().max(1.0) as u32,
            (self.css_height * self.dpr).round().max(1.0) as u32,
        )
    }

    pub fn frame_h(&self) -> f64 {
        FRAME_W * self.css_height / self.css_width
    }

    /// CSS px to scene units, for positions.
    pub fn units(&self, css_px: f64) -> f64 {
        css_px * FRAME_W / self.css_width
    }

    /// CSS px to kinemo's stroke width, which scales by `height / 1080`.
    pub fn stroke(&self, css_px: f64) -> f64 {
        let (_, h) = self.pixels();
        css_px * self.dpr * 1080.0 / h as f64
    }

    /// CSS px to kinemo's text size, in scene units.
    pub fn text_size(&self, css_px: f64) -> f64 {
        self.units(css_px)
    }

    /// A CSS-pixel position (origin top-left, y down) as scene units (origin
    /// centre, y up).
    pub fn to_scene(&self, x: f64, y: f64) -> [f64; 2] {
        [
            self.units(x) - FRAME_W / 2.0,
            self.frame_h() / 2.0 - self.units(y),
        ]
    }
}

/// How one primitive is painted, already resolved to colours and pixels.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Paint {
    pub stroke: Option<[f64; 4]>,
    /// CSS px.
    pub width: f64,
    pub fill: Option<[f64; 4]>,
    pub fill_opacity: f64,
    pub opacity: f64,
    /// CSS px, on and off.
    pub dash: Option<Vec<f64>>,
}

/// A scene under construction, in CSS-pixel coordinates.
pub struct Builder {
    canvas: Canvas,
    scene: Scene,
    /// Rectangles of the labels placed so far, for collision avoidance.
    pub labels: Vec<crate::geom::Rect>,
}

impl Builder {
    pub fn new(canvas: Canvas, background: [f64; 4]) -> Self {
        let config = SceneConfig {
            frame_w: FRAME_W,
            frame_h: canvas.frame_h(),
            background,
            ..Default::default()
        };
        let mut scene = Scene::new(config);
        scene.duration = 0.0;
        Builder {
            canvas,
            scene,
            labels: Vec::new(),
        }
    }

    pub fn canvas(&self) -> Canvas {
        self.canvas
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

    fn object(&mut self, kind: &str, props: Vec<(&str, Value)>, paint: &Paint, z: f64) {
        let id = self.scene.objects.len() as u32;
        let mut map = BTreeMap::new();
        let mut all = props;
        if let Some(s) = paint.stroke {
            all.push(("stroke", Value::Color(s)));
            all.push((
                "stroke_width",
                Value::Float(self.canvas.stroke(paint.width)),
            ));
        }
        if let Some(f) = paint.fill {
            all.push(("fill", Value::Color(f)));
            // kinemo's default fill opacity is 0: a fill without it is invisible.
            all.push(("fill_opacity", Value::Float(paint.fill_opacity)));
        }
        if paint.opacity < 1.0 {
            all.push(("opacity", Value::Float(paint.opacity.max(0.0))));
        }
        if let Some(d) = &paint.dash {
            let scaled = d
                .iter()
                .map(|v| Value::Float(self.canvas.stroke(*v)))
                .collect();
            all.push(("dash", Value::List(scaled)));
        }
        all.push(("z", Value::Float(z)));
        for (name, value) in all {
            let s = self.signal(value);
            map.insert(name.to_string(), s);
        }
        self.scene.objects.push(Object {
            id,
            kind: kind.to_string(),
            props: map,
            // kinemo's `present` starts false: an object needs a presence.
            presence: vec![(0.0, true)],
            ..Default::default()
        });
        self.scene.roots.push(id);
    }

    /// An open or closed polyline through CSS-pixel points.
    pub fn polyline(&mut self, points: &[[f64; 2]], closed: bool, paint: &Paint, z: f64) {
        if points.len() < 2
            || points
                .iter()
                .any(|p| !(p[0].is_finite() && p[1].is_finite()))
        {
            return;
        }
        let pts = points
            .iter()
            .map(|p| Value::Vec2(self.canvas.to_scene(p[0], p[1])))
            .collect();
        let kind = if closed { "polygon" } else { "polyline" };
        self.object(kind, vec![("points", Value::List(pts))], paint, z);
    }

    /// Several subpaths as one object, for batched geometry (§3.3). Each
    /// subpath is a polyline in CSS pixels; `closed` closes every one.
    pub fn path(&mut self, subpaths: &[Vec<[f64; 2]>], closed: bool, paint: &Paint, z: f64) {
        use std::fmt::Write;
        let mut d = String::new();
        for sub in subpaths {
            let mut first = true;
            for p in sub {
                if !(p[0].is_finite() && p[1].is_finite()) {
                    continue;
                }
                let [x, y] = self.canvas.to_scene(p[0], p[1]);
                let _ = write!(d, "{}{x:.5} {y:.5}", if first { "M" } else { "L" });
                first = false;
            }
            if closed && !first {
                d.push('Z');
            }
        }
        if d.is_empty() {
            return;
        }
        self.object(
            "path",
            vec![("d", Value::Str(d)), ("closed", Value::Bool(closed))],
            paint,
            z,
        );
    }

    /// A filled disc of radius `r` CSS px.
    pub fn dot(&mut self, at: [f64; 2], r: f64, paint: &Paint, z: f64) {
        if !(at[0].is_finite() && at[1].is_finite() && r > 0.0) {
            return;
        }
        let [x, y] = self.canvas.to_scene(at[0], at[1]);
        let r = self.canvas.units(r);
        self.object(
            "circle",
            vec![
                ("r", Value::Float(r)),
                ("x", Value::Float(x)),
                ("y", Value::Float(y)),
            ],
            paint,
            z,
        );
    }

    /// Text centred on `at`, `size` CSS px, turned `rotate` radians, wrapped
    /// at `wrap` CSS px when given. Laid out at size 1 and scaled, like maths,
    /// so inline `$…$` keeps its baked layouts at any width (§10.4).
    #[allow(clippy::too_many_arguments)]
    pub fn text(
        &mut self,
        text: &str,
        at: [f64; 2],
        size: f64,
        rotate: f64,
        wrap: Option<f64>,
        paint: &Paint,
        z: f64,
    ) {
        if text.is_empty() || !(at[0].is_finite() && at[1].is_finite()) {
            return;
        }
        let [x, y] = self.canvas.to_scene(at[0], at[1]);
        let scale = self.canvas.text_size(size) / crate::maths::SIZE;
        let mut props = vec![
            ("text", Value::Str(text.to_string())),
            ("size", Value::Float(crate::maths::SIZE)),
            ("scale", Value::Float(scale)),
            ("x", Value::Float(x)),
            ("y", Value::Float(y)),
        ];
        if rotate != 0.0 {
            props.push(("rotate", Value::Float(rotate.to_degrees())));
        }
        if let Some(w) = wrap {
            props.push(("wrap", Value::Float(w / size)));
        }
        self.object("text", props, paint, z);
    }

    /// The size of `text` at `size` CSS px: width, height. Inline `$…$` is
    /// measured as the maths it typesets to, on one line.
    pub fn measure(text: &str, size: f64, wrap: Option<f64>) -> [f64; 2] {
        if wrap.is_none() && text.matches('$').count() >= 2 {
            let (mut w, mut h) = (0.0f64, 0.0f64);
            for (i, part) in text.split('$').enumerate() {
                if part.is_empty() {
                    continue;
                }
                let [pw, ph] = if i % 2 == 1 {
                    Self::measure_math(part, size, false)
                } else {
                    Self::measure_plain(part, size, None)
                };
                w += pw;
                h = h.max(ph);
            }
            return [w, h];
        }
        Self::measure_plain(text, size, wrap)
    }

    fn measure_plain(text: &str, size: f64, wrap: Option<f64>) -> [f64; 2] {
        let opts = kinemo_text::TextOptions {
            size: 1.0,
            width: wrap.map(|w| w / size),
            ..Default::default()
        };
        let r = kinemo_text::measure(text, &opts);
        [(r.x1 - r.x0) * size, (r.y1 - r.y0) * size]
    }

    /// The size of a TeX formula at `size` CSS px: width, height. Zero when it
    /// cannot be laid out, which `maths::check` refused at load already.
    pub fn measure_math(tex: &str, size: f64, display: bool) -> [f64; 2] {
        match kinemo_math::layout_math(tex, &crate::maths::options(display)) {
            Ok(lay) => {
                let k = size / crate::maths::SIZE;
                [lay.bbox.width() * k, lay.bbox.height() * k]
            }
            Err(_) => [0.0, 0.0],
        }
    }

    /// A TeX formula centred on `at`, `size` CSS px high. It is laid out at
    /// size 1 and scaled, so baked layouts match at any figure width (§10.4).
    pub fn math(
        &mut self,
        tex: &str,
        at: [f64; 2],
        size: f64,
        display: bool,
        colour: [f64; 4],
        z: f64,
    ) {
        if tex.is_empty() || !(at[0].is_finite() && at[1].is_finite()) {
            return;
        }
        let [x, y] = self.canvas.to_scene(at[0], at[1]);
        let math_id = self.scene.objects.len() as u32;
        let rest_id = math_id + 1;
        let mut props = BTreeMap::new();
        for (name, value) in [
            ("tex", Value::Str(tex.to_string())),
            ("size", Value::Float(crate::maths::SIZE)),
            ("display", Value::Bool(display)),
            (
                "scale",
                Value::Float(self.canvas.text_size(size) / crate::maths::SIZE),
            ),
            ("x", Value::Float(x)),
            ("y", Value::Float(y)),
            ("z", Value::Float(z)),
        ] {
            let s = self.signal(value);
            props.insert(name.to_string(), s);
        }
        // A `math` object draws through one `glyphs` child marked `rest`.
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
        let mut rest = BTreeMap::new();
        for (name, value) in [
            ("rest", Value::Bool(true)),
            ("fill", Value::Color(colour)),
            ("fill_opacity", Value::Float(1.0)),
        ] {
            let s = self.signal(value);
            rest.insert(name.to_string(), s);
        }
        self.scene.objects.push(Object {
            id: rest_id,
            kind: "glyphs".into(),
            parent: Some(math_id),
            props: rest,
            presence: vec![(0.0, true)],
            ..Default::default()
        });
    }

    pub fn finish(self) -> Scene {
        self.scene
    }
}

/// Lay out and rasterise a scene; transparent when an underlay goes below.
pub fn rasterise(scene: &Scene, canvas: Canvas, transparent: bool) -> Image {
    let (width, height) = canvas.pixels();
    let index = Arc::new(TimelineIndex::new(scene));
    let evaluator = Evaluator::with_index(scene, index);
    let list = display_list_with(
        &Layout::new(&evaluator),
        0.0,
        FrameSize { width, height },
        transparent,
    );
    rasterize(&list, true)
}

/// Pixels differing from `bg` by more than 8 levels in any channel, as a
/// fraction of the image: the ink every figure must have (§21.1).
pub fn ink(image: &Image, bg: [f64; 4]) -> f64 {
    let bg = bg.map(|c| (c * 255.0).round() as i32);
    let painted = image
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| (0..3).any(|i| (p[i] as i32 - bg[i]).abs() > 8))
        .count();
    painted as f64 / (image.width as f64 * image.height as f64).max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paint() -> Paint {
        Paint {
            stroke: Some([0.0, 0.0, 0.0, 1.0]),
            width: 3.0,
            fill: None,
            fill_opacity: 1.0,
            opacity: 1.0,
            dash: None,
        }
    }

    #[test]
    fn units_follow_m0() {
        let c = Canvas::new(720.0, 405.0, 2.0);
        assert_eq!(c.pixels(), (1440, 810));
        assert!((c.frame_h() - 9.0).abs() < 1e-12);
        assert!((c.stroke(2.0) - 2.0 * 2.0 * 1080.0 / 810.0).abs() < 1e-12);
        assert!((c.units(720.0) - 16.0).abs() < 1e-12);
        assert_eq!(c.to_scene(0.0, 0.0), [-8.0, 4.5]);
    }

    #[test]
    fn a_square_canvas_fills_without_letterbox() {
        let c = Canvas::new(400.0, 400.0, 1.0);
        let mut b = Builder::new(c, [1.0; 4]);
        // A frame drawn along the canvas's edges must land on its edges.
        b.polyline(
            &[[2.0, 2.0], [398.0, 2.0], [398.0, 398.0], [2.0, 398.0]],
            true,
            &paint(),
            0.0,
        );
        let img = rasterise(&b.finish(), c, false);
        let px = |x: u32, y: u32| img.rgba[((y * img.width + x) * 4) as usize];
        assert!(px(2, 200) < 128, "left edge");
        assert!(px(398, 200) < 128, "right edge");
        assert!(px(200, 2) < 128, "top edge");
        assert!(px(200, 398) < 128, "bottom edge");
        assert!(px(200, 200) > 200, "inside stays clear");
    }

    #[test]
    fn the_background_is_the_themes() {
        let c = Canvas::new(100.0, 50.0, 1.0);
        let light = rasterise(
            &Builder::new(c, crate::theme::Theme::light().bg()).finish(),
            c,
            false,
        );
        assert!(light.rgba[0] > 240, "light background");
        let dark = rasterise(
            &Builder::new(c, crate::theme::Theme::dark().bg()).finish(),
            c,
            false,
        );
        assert!(dark.rgba[0] < 40, "dark background");
    }

    #[test]
    fn fills_dots_paths_and_text_all_paint() {
        let c = Canvas::new(300.0, 150.0, 1.0);
        let mut b = Builder::new(c, [1.0; 4]);
        let fill = Paint {
            fill: Some([0.8, 0.2, 0.0, 1.0]),
            fill_opacity: 1.0,
            opacity: 1.0,
            ..Default::default()
        };
        b.dot([50.0, 75.0], 20.0, &fill, 0.0);
        b.path(
            &[vec![[100.0, 20.0], [140.0, 20.0], [140.0, 60.0]]],
            true,
            &fill,
            0.0,
        );
        let ink_paint = Paint {
            fill: Some([0.0, 0.0, 0.0, 1.0]),
            fill_opacity: 1.0,
            opacity: 1.0,
            ..Default::default()
        };
        b.text("hapto", [220.0, 75.0], 24.0, 0.0, None, &ink_paint, 0.0);
        let img = rasterise(&b.finish(), c, false);
        assert!(ink(&img, [1.0; 4]) > 0.03, "ink {}", ink(&img, [1.0; 4]));
    }

    /// A baked formula draws the same number of pixels high at any width.
    #[cfg(feature = "typeset")]
    #[test]
    fn maths_keeps_its_pixel_size_at_two_widths() {
        let height_of = |css_width: f64| {
            let c = Canvas::new(css_width, 150.0, 1.0);
            let mut b = Builder::new(c, [1.0; 4]);
            b.math(
                r"\frac{\partial H}{\partial q}",
                [css_width / 2.0, 75.0],
                24.0,
                true,
                [0.0, 0.0, 0.0, 1.0],
                0.0,
            );
            let img = rasterise(&b.finish(), c, false);
            let rows: Vec<u32> = (0..img.height)
                .filter(|y| {
                    (0..img.width).any(|x| img.rgba[((y * img.width + x) * 4) as usize] < 128)
                })
                .collect();
            rows.last().unwrap() - rows.first().unwrap()
        };
        let (narrow, wide) = (height_of(300.0), height_of(600.0));
        assert!(narrow > 20, "the formula drew ({narrow} px)");
        assert!(narrow.abs_diff(wide) <= 1, "{narrow} vs {wide}");
    }
}
