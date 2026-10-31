# hapto — current state

**Figures that react.** A drawing beside a paragraph that the reader can
touch and the prose can drive.

```
state:   x = 3          set by a slider, a drag, a hover, or by the prose
figure:  f(state) -> drawing
```

No timeline, no cursor, nothing to wait for. A value changes and the
drawing follows. That is the whole library.

This document describes what exists today, what it guarantees, and what
was measured to be missing. Everything in **Part I** is implemented and
tested at commit `781a20f`. **Part II** is the gap analysis against the
port-Hamiltonian baseline; the plan that answers it is `docs/SPEC-2.0.md`.

Companion documents: `docs/SPEC-2.0.md` is the target. `docs/why.md` is the
retrospective argument for why this is a separate library rather than a
layer on kinemo. `README.md` is the one-screen introduction.

---

## 0. Orientation

| | |
|---|---|
| Repository | `~/dev/.personal/hapto` (public: `github.com/m4nd0r4s/hapto`) |
| Language | Rust, plus a WASM wrapper for the browser |
| Crates | `hapto-core` (the library), `hapto-web` (the `wasm-bindgen` surface) |
| Source size | ~2 350 lines across 7 files |
| Tests | 30 (`cargo test -p hapto-core`) |
| Licence | MIT or Apache-2.0 |
| Commit identity | `M4nd0r4s <25236463+m4nd0r4s@users.noreply.github.com>` |

### The dependency on kinemo

hapto does not render anything itself. It builds a **kinemo scene** and
asks kinemo's renderer to draw it. Six kinemo crates are pinned to one
commit in `Cargo.toml`:

```toml
kinemo-ir     = { git = "...kinemo.git", rev = "4cbd012bc8ca29a53696178b9a6c8bff68602144" }
kinemo-eval   = { ... same rev }
kinemo-layout = { ... same rev }
kinemo-text   = { ... same rev }
kinemo-math   = { ... same rev }
kinemo-render = { ... same rev }
```

Why this works: kinemo's drawing crates take time as a *parameter*
(`render_frame(scene, t, opts)`), not as a mode. hapto builds a scene of
duration `0.0`, writes values into its signals, and renders at `t = 0`
forever.

**Which crates `hapto-core` declares** (`crates/hapto-core/Cargo.toml`):
`kinemo-ir`, `kinemo-eval`, `kinemo-layout`, `kinemo-render`. It does
*not* name `kinemo-text` or `kinemo-math`.

**But it links them transitively, and this matters.** `cargo tree -p
hapto-core` shows `kinemo-layout` pulling in `kinemo-text`,
`kinemo-math` and `kinemo-code`, and `kinemo-render` pulling in
`kinemo-text` again. **The whole text and maths stack is already in the
binary.** Drawing a label needs no new dependency and no `Cargo.toml`
change — see 8.1.

**The renderer already understands far more object kinds than hapto
emits.** `kinemo-layout/src/geometry.rs:53-131` dispatches on
`Object::kind`, and the complete list it accepts is:

```
circle  dot  ellipse  rect  polygon  regular_polygon  polyline
line  arrow  arc  sector  path  image  brace  text  glyphs
plot  plot_area  trail
```

hapto's `Build` emits only `dot`, `line`, `arrow`, `rect`, `polygon`
and `polyline` — six of nineteen. **`text`, `arc`, `ellipse`, `path`,
`plot` and `brace` need no renderer work at all**: they need a `Build`
method that emits the object with the right props. That is the single
most important fact in this document for anyone planning Part II.

What is *not* free: whether each kind's props can follow a value
(`Expr`) or must be literal — `points`-based kinds hit the list
limitation in 2.2 — and, for `text`, which font machinery the pinned
commit expects. Check `geometry.rs` for the props each kind reads before
committing to a design.

### Layout of the source

| File | Lines | What it holds |
|---|---|---|
| `hapto-core/src/lib.rs` | 269 | `Value`, `Domain`, `Knob`, `State` — the state model |
| `hapto-core/src/build.rs` | 511 | `Build`, `Num`, `Style` — drawing primitives |
| `hapto-core/src/figure.rs` | 242 | `Figure`, `Size` — binding state to a scene, rendering |
| `hapto-core/src/book.rs` | 443 | `plot`, `columns` — the two figure kinds a lesson can ask for |
| `hapto-core/src/formula.rs` | 391 | `parse` — the written-formula language |
| `hapto-core/src/demos.rs` | 225 | four hard-coded example figures |
| `hapto-web/src/lib.rs` | 265 | `BookFigure`, `Figure` — the browser surface |

---

# PART I — what exists

## 1. The state model

`hapto-core/src/lib.rs`

### 1.1 Values

A figure reads named values. Four kinds exist:

```rust
pub enum Value {
    Number { n: f64 },          // a slider, a typed number, a drag
    Choice  { option: String }, // one of a few named options
    Point   { x: f64, y: f64 }, // a position, from dragging
    Flag    { on: bool },       // on or off
}
```

Serialised internally tagged, so JSON crossing to the browser reads
`{"kind": "number", "n": 3}`. `as_number()` returns `Some` for `Number`
and for `Flag` (1.0 / 0.0), `None` otherwise.

> **Note for implementers:** `Point`, `Choice` and `Flag` exist in the
> model and are honoured by `Domain::admit`, but **no figure reads them
> and `hapto-web` only exposes `set(name, f64)`**. They are plumbing
> waiting for a consumer, not a working feature.

### 1.2 Domains

What a value may be, so a control can be drawn for it and a bad set
refused with a reason:

```rust
pub enum Domain {
    Number { low: f64, high: f64, step: Option<f64> },
    Choice { options: Vec<String> },
    Point  { low: (f64, f64), high: (f64, f64) },
    Flag,
}
```

Two guarantees worth keeping:

- **`admit` clamps rather than rejects** for numbers and points: a value
  outside the bounds becomes the nearest value inside them. A `Choice`
  outside its options *is* rejected, with the options listed in the
  error.
- **`step()` is never coarse.** `None` means continuous, which resolves
  to a hundredth of the range. This is a deliberate correction of a
  kinemo defect where a slider's step came from a question's answer
  tolerance, producing two-position sliders. The test
  `a_continuous_slider_is_not_coarse` guards it.

### 1.3 State

```rust
state.declare(knob);                       // declare a value
state.set("x", Value::number(3.0))?;       // the whole interface
state.number("x")                          // -> Option<f64>
```

`set` is the only operation that matters. A slider, a drag, and a
`<span>` in the prose all call exactly this. Setting an unknown name
returns an error naming it.

---

## 2. Drawing primitives

`hapto-core/src/build.rs`

### 2.1 `Num` — a number that may follow a value

Every shape property takes a `Num`, which is either a literal or an
expression over named values:

```rust
pub enum Num { Fixed(f64), Of(Expr) }
```

Arithmetic: `plus`, `minus`, `times`, `over`, `squared`, `pow`, `min`,
`max`, `negated`, `abs`, `sin`, `cos`, `sqrt`.

Two literals **fold at build time** rather than at every frame.

### 2.2 The shapes

| Method | Signature | Can it follow a value? |
|---|---|---|
| `dot` | `(x, y, radius, style) -> u32` | position yes, radius no |
| `line` | `(from, to, style) -> u32` | both ends yes |
| `arrow` | `(from, to, style) -> u32` | both ends yes |
| `rect` | `(x, y, width, height, style) -> u32` | all yes |
| `polygon` | `(points, style) -> Result<u32>` | **no — fails if any corner moves** |
| `polyline` | `(points, style) -> Result<u32>` | **no — same** |
| `quad` | `([corner; 4], style) -> Vec<u32>` | yes (drawn as 4 lines) |
| `spring` | `(from, to, coils, width, style) -> Vec<u32>` | both ends yes |

**There is no text primitive. There is no circle, arc, ellipse or
curved path. There is no clipping, no gradient, no dashed stroke.**

#### The list limitation, stated precisely

`Expr` can build a `Vec2` but cannot build a `List`. Therefore the
`points` property of a polygon or polyline must be entirely literal.
`point_list` returns this error, verbatim:

> a polygon's corners cannot follow a value: the IR has no expression
> that builds a list. Use `quad` or `line` for a shape that has to move.

`quad` and `spring` exist as workarounds: each draws many `line` objects
whose endpoints are ordinary bound signals.

**Consequence:** a curve that changes shape when a value moves cannot be
drawn as one polyline. `book::plot` sidesteps this by sampling the curve
**once**, as fixed points — which is why the knob in a `plot` moves the
point and the tangent but never the curve.

### 2.3 Style and weight

```rust
Style::fill(colour)              // fill_opacity defaults to 1.0
Style::stroke(colour, width)
    .and_fill(colour)            // fills, opacity gets 1.0 if unset
    .and_opacity(0.3)
```

**`fill_opacity` must be set explicitly.** kinemo's renderer defaults it
to `0.0`, so a shape with a colour and nothing else is painted fully
transparent. `Style::fill` and `and_fill` set it for you; a hand-built
`Style { fill: Some(c), ..Default::default() }` draws nothing.

**No style property can follow a value.** Every field of `Style` is a
plain `f64` or `[f64; 4]`, never a `Num`. A figure cannot make a stroke
thicken, or a fill shift colour, as a knob moves — only geometry
responds. This is a real limit, not an oversight to route around: one
baseline figure encodes its entire lesson in edge thickness and is
blocked by exactly this. See 10.3.

**Stroke width is not in scene units.** The renderer scales it
separately from positions, so a line 0.05 "wide" is invisible. Use the
named weights:

```rust
weight::HAIR  = 1.5   // axis, guide, construction line
weight::LINE  = 2.5   // an ordinary line
weight::HEAVY = 4.0   // what the figure is about
```

Palette: `colour::{RED, BLUE, YELLOW, GREEN, GREY, FG}`, matching
kinemo's.

### 2.4 Three defaults that silently produce a blank frame

All three were real defects, found by looking at rendered PNGs while
every test passed. They are fixed inside `Build`, and anyone adding a
primitive must preserve the fixes:

1. **`Object::presence` starts empty and `Scene::present` starts
   `false`** — an object with no presence entry is never drawn.
   `Build::object` writes `presence: vec![(0.0, true)]`.
2. **`fill_opacity` defaults to 0.0** — see above.
3. **Stroke width is not in scene units** — see above.

`tests/shows.rs` counts painted pixels for this reason.
`tests/draws.rs` compares frames to each other, which two blank frames
would pass; a new figure needs a `shows`-style test.

### 2.5 `probe`

```rust
pub fn probe(&mut self, n: Num) -> f64
```

What a `Num` works out to right now. Binds it to a signal and asks
kinemo's evaluator with `NoResolver` — the same machinery that draws, so
a figure and a number printed beside it cannot disagree about what a
formula says.

---

## 3. The formula language

`hapto-core/src/formula.rs`

```rust
pub fn parse(text: &str, values: &BTreeMap<String, Num>) -> Result<Num, String>
```

A lesson writes its mathematics as a string; this turns it into a `Num`
over named values. Lexer plus recursive-descent parser, 10 tests.

**Accepts:** numbers, named values, `+ - * / **`, unary minus,
parentheses, and the functions `abs`, `sqrt`, `sin`, `cos`, `min`,
`max`.

**Refuses `^`** with:

> in the formula "{text}": ^ is exclusive-or, not a power. Write a power
> as \*\*.

This is deliberate. A lesson once wrote `2^3` and a CAS read it as
exclusive-or, yielding 1. The gate catches it before a figure is built.

Names may carry subscripts and digits (`x_f`, `e_R`, `q_b`), which
matters because the prose-hover feature matches the variable as the
prose spells it.

---

## 4. Figure and rendering

`hapto-core/src/figure.rs`

```rust
Figure::built((scene, values))            // from a Build
Figure::from_json(ir)                     // from kinemo JSON
figure.bound(name, low, high)?            // give a value its range
figure.set(name, Value::number(3.0))?
figure.draw_now(Size { width, height })?  // -> Image { rgba }
```

Rendering is **CPU-side RGBA bytes**, row by row — not a canvas 2D
context, not SVG. The browser blits them into a canvas with
`putImageData`. One consequence: **redrawing is a full re-render**, so
cost scales with pixel count, not with what changed.

---

## 5. The two book figures

`hapto-core/src/book.rs`

This is the surface a lesson actually reaches. Everything a book can
draw today is one of these two.

### 5.1 The frame

kinemo's frame is 16×9 with the origin at the centre. `Frame` maps a
lesson's own units onto it once:

```rust
const HALF_W: f64 = 6.6;   // the drawable area inside the frame
const HALF_H: f64 = 3.5;
```

Axes are drawn through zero where zero is in view, at the edge where it
is not. **Axes carry no labels and no tick marks** — there is no text.

### 5.2 `plot`

```rust
pub fn plot(
    curve: &str, variable: &str, at: f64,
    knob: Range, x_range: Range, y_range: Range,
    tangent: bool,
) -> Result<Figure, String>
```

A curve with a point riding it, optionally with the tangent.

- The curve is sampled at **160 fixed points** and drawn as one
  polyline. It cannot move (see 2.2).
- The point sits at `(v, curve(v))`, both reading the same value, so it
  cannot drift off the curve.
- The tangent is a **secant over `x_range.span() / 400`**, read from the
  same formula as the curve — the derivative in the limit, with no
  symbolic algebra, and it cannot disagree with the curve it touches.
- A dropped hairline runs from the point to the axis.
- **Refuses early**: a curve nowhere inside `y_range` returns an error
  naming the range, rather than drawing an empty picture.

`knob` is separate from `x_range` on purpose: the axes are drawn over
the area the curve needs, while the lesson decides how far the reader
may move the value.

### 5.3 `columns`

```rust
pub fn columns(
    first: (&str, &str), second: (&str, &str),
    variable: &str, at: f64,
    knob: Range, x_range: Range, y_range: Range,
) -> Result<Figure, String>
```

Two column vectors as arrows and the parallelogram they span, drawn with
`quad`. Each column is a pair of written formulas, so
`first: ["2","1"], second: ["x","2"]` gives a figure where moving `x`
shears the second column and flattens the area. **The area is the
determinant**; watching it vanish is the lesson.

### 5.4 The hard limit on both

**Each figure has exactly one knob.** `plot` and `columns` take one
`variable`, and `BookFigure` exposes one `variable()`. A figure with two
independent values is not expressible.

---

## 6. The browser surface

`hapto-web/src/lib.rs`

### 6.1 `BookFigure` — what the book compiler uses

```js
const figure = BookFigure.fromSpec(JSON.stringify({
  kind: "plot",          // or "columns"
  variable: "x",
  at: 1,
  curve: "x**2 - x*(2)", // plot only
  tangent: true,         // plot only
  first:  ["2", "1"],    // columns only
  second: ["x", "2"],    // columns only
  x_range: [-2, 4],
  y_range: [-2.35, 9.35],
  knob: [-1, 5],         // optional; defaults to x_range
}), 960, 540);

figure.set("x", 3);                 // a slider, or a word in the prose
const rgba = figure.draw();         // Uint8Array, RGBA row by row
```

| Method | Returns |
|---|---|
| `fromSpec(spec, width, height)` | a figure, or **throws with a reason** |
| `variable()` | the one value name |
| `set(name, number)` | `()`, or throws |
| `get(name)` | `Option<f64>`, after bounds had their say |
| `step(name)` | how finely a control should move |
| `bounds(name)` | `[low, high]` |
| `resize(w, h)`, `width()`, `height()` | size |
| `draw()` | RGBA bytes |

**A bad spec throws, never draws blank.** An unknown `kind` returns:

> "sphere" is not a kind of figure. There are: plot, columns

### 6.2 `Figure` — the lower surface

Takes raw kinemo JSON plus the name of one free signal (`bind_free`).
Used by the examples; the book does not use it.

### 6.3 Building

```bash
scripts/build_web.sh --build      # wasm-pack -> demo/pkg
```

The book copies `demo/pkg` into its output.

---

## 7. What the book compiler sends

For context, the producer side lives in **hmu**, not here:

- `hmu/compiler/demo.py` — `to_playground(lesson)` stages a lesson's
  short `demo` block with SymPy, deciding the curve, the vectors, the
  roots, and whether a tangent belongs.
- `hmu/compiler/figure.py` — `to_spec(playground)` emits the JSON above.

An author writing a lesson has exactly two choices,
`kind: "function"` or `kind: "matrix"`, which stage into `plot` and
`columns`.

---

# PART II — what is missing

Nothing here is implemented. The measurements below stand; the **ranking
and the remedies** in §9–§10 are superseded by `docs/SPEC-2.0.md`, which
answers the same gaps with a figure language instead of new kinds (its §24
lists what changed). In particular, "What not to build" in §9 no longer
holds for fields: 2.0 draws them as batched paths, which removes the cost
argument.

Both the gaps and their ranking come from a **measured** comparison
against a hand-built baseline: 12 interactive figures (13 instances —
`phase` appears in two chapters) at
`~/dev/.personal/phx/docs/_archive/handbook/demos.js`, written to teach
the same subject the generated book teaches.

**Of those 12 figures, the current pipeline can produce zero.** Two come
out partially and lose exactly the element their caption promises.

## 8. The three structural gaps

Counting how many of the 12 baseline figures each missing capability
blocks:

| Missing capability | Figures needing it | Count |
|---|---|---|
| **Text inside the drawing** | all | **12 / 12** |
| **More than one knob per figure** | all but `rotation` | **11 / 12** |
| **An integrated trajectory** | phase, atoms, passive, caps, integrators | **5 / 12** |
| A `Choice` knob (mode buttons) | skew, coenergy, domains, laplacian | 4 |
| A `Point` knob (drag in the plane) | rotation, skew, coenergy, phase, atoms | 5 |
| Dashed strokes | rotation, coenergy, caps, integrators | 4 |
| Fill that follows a value | skew, coenergy, domains, motor | 4 |
| Scalar field / heat map | skew | 1 |
| Direction field | atoms | 1 |
| Level curves | phase | 1 |
| Stroke width that follows a value | laplacian | 1 |
| Logarithmic slider | caps | 1 |

Read the top three rows as preconditions, not as items on a list. The
exotic capabilities each appear once.

### 8.1 No text — blocks 12 of 12

A figure cannot label anything: no axis labels, no tick values, no name
on an arrow, no units, no legend. In two baseline figures (`motor`,
`ports`) the text **is** the figure — one is numbers in cells, the other
is a diagram of labelled ports.

**This is cheaper than it looks.** The renderer already draws text:
`kinemo-layout/src/geometry.rs:127` dispatches `"text"`, the whole
`kinemo-text` crate does shaping and glyph outlines, and it is already
in the binary transitively (see 0). What is missing is that
**`Build` never emits a `text` object** — the public methods are `dot`,
`line`, `arrow`, `rect`, `polygon`, `polyline`, `quad`, `spring`,
`probe`, and that is all.

The work is a `Build::text()` going through the existing `object()`
path, then teaching `book.rs::axes()` (currently two grey lines, no
labels) to write the axis names and ticks.

### 8.2 One knob per figure — blocks 11 of 12

`hapto-web/src/lib.rs:123` exposes `variable()`, singular;
`hmu/compiler/figure.py` emits one `<input type="range">`; and
`book.py` reads one name. The baseline's `motor` has three sliders,
`domains` has two draggable axes plus a four-way selector.

**The core is already multi-knob.** `State` is a
`BTreeMap<String, Knob>` (`lib.rs:160`), `Build::value()` names as many
values as asked, and `demos::mass_spring` already declares two (`x` and
`k`). `Value::Point`, `Value::Choice` and `Value::Flag` exist with
`admit()` implemented and tested.

**The narrowing happens only in the hmu and hapto-web layers.** That is
the best news in this document: the hard part is done.

### 8.3 No integrated trajectory — blocks 5 of 12

Five baseline figures are trajectories: an initial state, a step rule, N
iterations, and the result drawn as a polyline. `figure.rs:9-11` states
that hapto never advances time.

**The contradiction is only apparent.** The integration would happen
when the scene is *built*, not against a clock; the result is still
`f(state) -> drawing`, with the polyline recomputed when a knob moves.

**If this is built, it must default to a symplectic step.** The baseline
documents three separate occasions where an explicit Euler made the
figure contradict its own caption, with measured numbers: energy going
from 2.825 to 3.664 over 2600 steps; a radius going from 2.60 to 2.87 in
a purely rotational system; a conserved total rising in 717 of 1400
steps. A figure whose caption says "the energy is constant" while the
drawing spirals outward is worse than no figure.

## 9. The four families, and the smallest useful set

The 12 figures collapse into four families:

| Family | Figures | What it is |
|---|---|---|
| **A. A plane of objects that follow values** | rotation, ports, domains, motor, laplacian | Arrows, boxes, nodes, cells. No curve. Each position, size or colour is an expression over the knobs. |
| **B. A curve with decoration** | coenergy (+ today's `plot`) | A function over an axis, with the area under it, projections, a point. |
| **C. Trajectory or time series** | phase, atoms, passive, caps, integrators | Integrate a dynamical system and trace the result. |
| **D. A field over the plane** | skew, part of atoms | The value of an expression at every point: colour, arrow or isoline. |

**Family A needs no new drawing primitive at all** — `dot`, `line`,
`arrow` and `rect` already accept `Num`. It needs text, multi-knob, and
styles that can follow a value. Five figures, the best return per unit
of work.

**The answer to "what is the smallest set of figure kinds":** two new
kinds — a **`diagram`** (family A: named objects positioned by
expressions over several knobs) and a **`trajectory`** (family C: an
initial state, a step rule, N steps) — plus the three cross-cutting
capabilities both need. That covers 10 of the 12.

### What not to build

1. **`kind: "region"` as a priority.** It serves one known chapter and
   zero of the 12 baseline figures. It falls out for free once fills can
   follow a value.
2. **Heat map / scalar field.** One figure in twelve. The baseline draws
   it with ~2400 `fillRect` calls; in hapto's scene model that would be
   2400 objects with props.
3. **Direction field.** One figure, and it only ever appears *beside* a
   trajectory — solving C delivers the part that teaches.
4. **Level curves.** One figure, and it needs marching squares.
5. **More flavours of `plot`.** The problem is not a third kind of
   graph; it is the absence of text, vectors and multi-knob. Another
   scalar `kind` widens the wrong end of the funnel.
6. **Replacing the renderer or leaving kinemo.** kinemo already has more
   primitives than hapto uses. The work is exposure, not replacement.

## 10. Ranked work

### 1 — `Build::text()`, and labelled axes on top of it
Emit `Object { kind: "text", props: { text, size, x, y, fill } }` through
the existing `object()` path; then make `book.rs::axes()` write axis
names and tick values.
**Why first:** a precondition for 12 of 12. It also fixes the complaint
already on record — that the `2-4` figure is undecipherable without
labels even once its quadrants are drawn.

### 2 — More than one knob, end to end
Widen `variable()` to a knob list in `hapto-web`, emit one control per
knob in `hmu/compiler/figure.py`, and iterate in `book.py`. The core
needs no change.
**Why second:** a precondition for 11 of 12, and the difference between
"watch a number change" and "watch two things trade" — which is exactly
the test the author's prompt already demands and the pipeline then
cannot honour.

### 3 — `kind: "diagram"`
A third kind where the author lists objects (`arrow`, `box`, `dot`,
`label`) with every coordinate, size and colour as an expression over
the knobs. Also requires `Style` to become parametric: `stroke_width` is
`Option<f64>` today (`build.rs:150`), so no visual property can follow a
value — which alone blocks `laplacian`, whose whole point is edge
thickness carrying the dissipation.
**Unblocks:** family A, five figures, with no new drawing primitive.

### 4 — `kind: "trajectory"`
Initial state, step rule as expressions, `dt`, number of steps; integrate
at build time and emit the polyline. Symplectic by default (see 8.3).
**Unblocks:** five figures, including the one the baseline's own
comments call the demo that justifies the whole thing being interactive.

### 5 — `Choice` knobs and fills that follow a value
Expose the already-implemented `Value::Choice` / `Domain::Choice` up
through the DSL and the markup; and solve the moving fill —
`point_list` refuses a polygon whose corners follow a value, with `quad`
as a four-sided workaround, while an area under a curve needs N sides.
**Unblocks:** completes both partial figures, and delivers `region`
for free.

### 6 — Widen the author's demo schema — last
Replace the one-`expression`, one-`variable`,
`kind ∈ {matrix, function}` schema in `hmu/pipeline/write.py`, and
update the reviewer's checklist.
**Why last:** the author is measurably not the bottleneck. The
regenerated `2-4` chapter wrote twelve sentences about its sign
condition and named nine hoverable values, and still produced
`demo: null` — correctly, because neither available shape would show
what it was teaching. Widening the prompt before new shapes exist would
only produce requests the compiler refuses.

## 11. Non-goals

- **No timeline, ever.** If something must advance on its own, it
  belongs in kinemo. Note that 8.3 does not breach this: integration
  happens at build time, not against a clock.
- **No 3D.**
- **No layout engine for prose.** The page owns the page; hapto owns a
  rectangle of pixels in it.
- **No data loading.** A figure's mathematics arrives as written
  formulas in the spec.

## 12. Conventions for anyone extending this

1. **A new figure kind gets a `shows`-style test** that counts painted
   pixels. `draws.rs` compares frames to each other, and two blank
   frames compare equal — that defect shipped once.
2. **Render a PNG and look at it.** All three blank-frame defects in 2.4
   passed every test that existed when they were written.
3. **A bad spec throws with a reason.** Never draw an empty canvas in a
   reader's browser; name what was wrong and what the options are.
4. **Formulas over numbers.** A figure and the number printed beside it
   should read the same formula through `probe`, so they cannot
   disagree.
5. **Run `cargo test -p hapto-core` and `scripts/build_web.sh --build`**
   before claiming a change works in the browser.
6. **If anything is simulated, check the invariant before showing it.**
   See 8.3 for the three measured occasions this went wrong.
