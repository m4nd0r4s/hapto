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
