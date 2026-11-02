//! §21.1: every mark kind this build draws paints pixels, alone, in both
//! themes. A test that compares two frames passes when both are blank; this
//! one cannot.

use hapto_core::render::Figure;
use hapto_core::scene::{ink, Canvas};
use hapto_core::theme::Theme;

/// One mark of every kind of SPEC-2.0 §8.2–8.6, §8.14 and §10.
const MARKS: &[&str] = &[
    "{ point: '[0, 0]', r: 12 }",
    "{ segment: '[[-3, -1], [3, 1]]', arrows: both }",
    "{ arrow: '[[-3, -1], [3, 1]]' }",
    "{ vector: '[3, 1]', at: '[-1, -1]' }",
    "{ line: '[[0, 0], [1, 0.3]]' }",
    "{ ray: '[[0, 0], [1, 0.3]]' }",
    "{ hline: 0.5 }",
    "{ vline: 0.5 }",
    "{ tangent: 0.5, of: f }",
    "{ secant: '[-1, 1.5]', of: f }",
    "{ drop: '[1.5, 1.5]', to: both }",
    "{ polyline: '[[-3, -1], [0, 1.5], [3, -1]]' }",
    "{ polygon: '[[-3, -1], [0, 1.5], [3, -1]]' }",
    "{ circle: '[0, 0]', r: 1.5 }",
    "{ ellipse: '[0, 0]', rx: 3, ry: 1, tilt: 0.3 }",
    "{ arc: '[0, 0]', r: 1.5, from: 0, to: 3 }",
    "{ sector: '[0, 0]', r: 1.5, from: 0, to: 2 }",
    "{ angle: '[0, 0]', from: '[1, 0]', to: '[0, 1]', r: 40 }",
    "{ path: 'M -3 -1 L 0 1.5 L 3 -1 Z', closed: true }",
    "{ bezier: '[[-3, -1], [-1, 2], [1, -2], [3, 1]]' }",
    "{ brace: '[[-3, -1], [3, -1]]', depth: 16 }",
    "{ spring: '[[-3, 0], [3, 0]]' }",
    "{ area: { under: 'f(x)', var: x, from: -1, to: 1.5 } }",
    "{ rect: '[-2, -1]', w: 4, h: 2 }",
    "{ box: '[0, 0]', text: 'a box with text' }",
    "{ function: 'f(x)', var: x }",
    "{ function: { x: 'cos(t)', y: 'sin(2*t)' }, var: t, domain: [0, tau] }",
    "{ text: 'some text {pi:.3f}', at: '[0, 0]', size: 18 }",
    "{ group: [ { rect: '[-1, -0.5]', w: 2, h: 1 } ], rotate: 0.5, scale: 2 }",
];

#[test]
fn every_kind_paints_in_both_themes() {
    for mark in MARKS {
        let doc = format!("hapto: 2\nalt: a\nlet: {{ 'f(t)': 't**2 - 1' }}\nview: {{ x: {{ range: [-4, 4] }}, y: {{ range: [-2, 2] }}, axes: none }}\nmarks: [ {mark} ]");
        let mut f = Figure::from_text(&doc).unwrap_or_else(|e| panic!("{mark}: {e:?}"));
        for theme in [Theme::light(), Theme::dark()] {
            let img = f.render(Canvas::new(400.0, 200.0, 1.0), &theme).unwrap();
            let painted = ink(&img, theme.bg());
            assert!(painted > 0.001, "{mark} painted {:.3}%", painted * 100.0);
        }
    }
}
