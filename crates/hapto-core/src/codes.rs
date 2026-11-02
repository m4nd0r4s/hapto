//! The figure format's error codes (SPEC-2.0 §18.3), beyond the formula
//! language's own (in `hapto_expr::error::codes`).

use hapto_expr::Code;

// E4xx — document structure
pub const E400: Code = Code("E400"); // not valid JSON or YAML
pub const E401: Code = Code("E401"); // unknown key
pub const E402: Code = Code("E402"); // missing required key
pub const E403: Code = Code("E403"); // wrong value type
pub const E404: Code = Code("E404"); // unknown mark kind
pub const E405: Code = Code("E405"); // unknown preset
pub const E406: Code = Code("E406"); // mixing `view` and `views`
pub const E407: Code = Code("E407"); // missing `hapto:` version
pub const E408: Code = Code("E408"); // a mark with zero or several kind keys
pub const E409: Code = Code("E409"); // mark references an unknown view

// E5xx — values and domains
pub const E501: Code = Code("E501"); // default outside range
pub const E502: Code = Code("E502"); // empty or inverted range
pub const E503: Code = Code("E503"); // log range not positive
pub const E504: Code = Code("E504"); // option not among options
pub const E505: Code = Code("E505"); // drag assigns a value that does not exist

// E6xx — rendering
pub const E601: Code = Code("E601"); // figure renders without ink
pub const E606: Code = Code("E606"); // unsupported TeX command
pub const E608: Code = Code("E608"); // TeX not baked
pub const E609: Code = Code("E609"); // a mark drawn below a heat map
pub const E610: Code = Code("E610"); // part of the format this build does not draw yet

// Warnings
pub const W301: Code = Code("W301"); // value nobody can change
pub const W602: Code = Code("W602"); // more than 60 labels on screen
pub const W801: Code = Code("W801"); // legacy 1.0 document

use hapto_expr::error::Entry;

/// The figure-format part of the catalogue `hapto errors` prints, beside
/// the formula language's own (`hapto_expr::error::CATALOGUE`).
pub const CATALOGUE: &[Entry] = &[
    Entry {
        code: E400,
        title: "not valid JSON or YAML",
        example: "hapto: 2\nmarks: [ { point: '[0, 0]' }",
    },
    Entry {
        code: E401,
        title: "unknown key (with a suggestion)",
        example: "marks: [ { point: '[0, 0]', colr: hot } ]",
    },
    Entry {
        code: E402,
        title: "missing required key",
        example: "hapto: 2\nmarks: [ { point: '[0, 0]' } ]   # no alt",
    },
    Entry {
        code: E403,
        title: "wrong value type",
        example: "values: [ { name: x, type: number, range: 5, default: 1 } ]",
    },
    Entry {
        code: E404,
        title: "unknown mark kind",
        example: "marks: [ { pointt: '[0, 0]' } ]",
    },
    Entry {
        code: E405,
        title: "unknown preset",
        example: "preset: phaze",
    },
    Entry {
        code: E406,
        title: "mixing `view` and `views`",
        example: "view: { ... }\nviews: [ ... ]",
    },
    Entry {
        code: E407,
        title: "missing `hapto:` version",
        example: "alt: a\nmarks: [ { point: '[0, 0]' } ]",
    },
    Entry {
        code: E408,
        title: "a mark with zero or several kind keys",
        example: "marks: [ { point: '[0, 0]', circle: '[1, 1]' } ]",
    },
    Entry {
        code: E409,
        title: "mark references an unknown view",
        example: "marks: [ { point: '[0, 0]', view: nowhere } ]",
    },
    Entry {
        code: E501,
        title: "default outside range",
        example: "values: [ { name: x, type: number, range: [0, 1], default: 3 } ]",
    },
    Entry {
        code: E502,
        title: "empty or inverted range",
        example: "values: [ { name: x, type: number, range: [1, 0], default: 0.5 } ]",
    },
    Entry {
        code: E503,
        title: "log range not positive",
        example: "values: [ { name: x, type: number, range: [0, 10], scale: log, default: 1 } ]",
    },
    Entry {
        code: E504,
        title: "option not among options",
        example: "values: [ { name: m, type: choice, options: [a, b], default: c } ]",
    },
    Entry {
        code: E505,
        title: "drag assigns a value that does not exist",
        example: "marks: [ { point: '[x, 0]', drag: { y: 'pointer.x' } } ]",
    },
    Entry {
        code: E601,
        title: "figure renders without ink",
        example: "view: { x: { range: [0, 1] }, y: { range: [0, 1] }, axes: none }\nmarks: [ { point: '[5, 5]' } ]",
    },
    Entry {
        code: E606,
        title: "unsupported TeX command",
        example: "marks: [ { math: '\\notacommand{x}', at: '[0, 0]' } ]",
    },
    Entry {
        code: E608,
        title: "TeX not baked (run `hapto bake`)",
        example: "a page loading a figure with `$x^2$` that was never baked",
    },
    Entry {
        code: E609,
        title: "a mark drawn below a heat map",
        example: "marks: [ { point: '[0, 0]', z: -1 }, { heatmap: 'x*y' } ]",
    },
    Entry {
        code: E610,
        title: "part of the format this build does not support yet",
        example: "marks: [ { heatmap: 'x*y' } ]",
    },
    Entry {
        code: W301,
        title: "value nobody can change (or that changes nothing)",
        example: "values: [ { name: k, ... } ]   # and no formula reads k",
    },
    Entry {
        code: W602,
        title: "more than 60 labels on screen",
        example: "marks: [ { point: '[i, 0]', label: '{i}', repeat: { i: 'range(0, 100)' } } ]",
    },
    Entry {
        code: W801,
        title: "legacy 1.0 document",
        example: "{ kind: plot, curve: 'x**2', ... }",
    },
];
