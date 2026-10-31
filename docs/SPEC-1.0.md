# hapto 1.0 — specification

> **Superseded by `docs/SPEC-2.0.md`.** Kept for the record. 2.0 replaces the
> six closed figure kinds below with a language they become presets of,
> rebuilds the scene from state, and adds the page runtime and the book
> compiler. `SPEC-2.0.md` §24 lists every decision that changed.

**What this library must be when it is done.**

This is a target, not an inventory. It describes the whole of hapto 1.0:
every figure kind, every primitive, every control, the author-facing
format, the browser surface, and the quality bar. A team should be able
to build from this document and know when they have finished.

For what exists today, see `docs/current-state.md`. For why this is a
separate library rather than a layer on kinemo, see `docs/why.md`.

---

## 1. What hapto is for

A **figure that answers to the reader**, living beside a paragraph.

```
state:   x = 3, mode = "damped", p = (1.2, -0.4)
figure:  f(state) -> drawing
```

Nothing advances on its own. A value changes and the drawing follows.
That is the whole model, and 1.0 does not weaken it.

### The test every figure must pass

> **What does the reader understand after touching this that they did
> not understand before?**

If the honest answer is "they watched a number change", the figure
should not exist. A plot of a formula beside the formula teaches
nothing — the reader can already read the formula. A figure earns its
place when touching it reveals a **relation that is hard to hold in the
head**: two quantities trading off, something vanishing exactly where
the text says it breaks, a shape collapsing, a growth that is not what
you would guess.

This is not decoration for the spec. It is the acceptance criterion for
every kind listed in §4, and the reason some obvious-looking features
are excluded in §11.

### Who calls it

Three callers, all through the same door:

| Caller | How it sets a value |
|---|---|
| A control the figure declares (slider, buttons, drag) | directly |
| A word in the prose beside the figure | hovering `x = 3` sets `x` to 3 |
| A compiler that wrote the page | at build time, in the spec |

`set(name, value)` is the entire interface. A `<span>` in a sentence and
a slider are indistinguishable to the figure.

---

## 2. Scope of 1.0

### In

- Six figure kinds (§4), covering the four families a technical book
  actually needs.
- Multiple named values per figure, of four types (§3).
- Text inside the drawing: labels, ticks, readouts, and typeset
  mathematics (§5.1).
- Styles that follow values — a stroke that thickens, a fill that shifts
  (§5.3).
- Trajectories integrated at build time, with a correct default
  integrator (§4.4).
- A stable JSON spec format an agent or a compiler can emit (§6).
- A browser surface that draws to a canvas and reports errors with
  reasons (§7).

### Out

- A timeline. Anything that advances on its own belongs in kinemo.
- 3D.
- A layout engine for prose. The page owns the page; hapto owns a
  rectangle of pixels in it.
- Data loading. Mathematics arrives as written formulas in the spec.
- Authoring UI. Specs are written by a compiler, an agent, or a human
  editing JSON.

### Definition of done

1.0 ships when a technical handbook of the complexity of
`phx/docs/_archive/handbook/` can be produced **entirely through hapto
specs**, with no hand-written canvas code. That baseline — twelve
interactive figures for a port-Hamiltonian systems course — is the
acceptance suite (§10).

---

## 3. State

### 3.1 Value types

```rust
Number { n: f64 }            // a slider, a typed number, a dragged axis
Choice { option: String }    // one of a few named options
Point  { x: f64, y: f64 }    // a position dragged in the plane
Flag   { on: bool }          // on or off
```

All four exist in the model today. 1.0 requires all four to be
**reachable from a spec, bound to a control, and readable by every
figure kind**.

### 3.2 Domains

What a value may be, so a control can be drawn and a bad set refused
with a reason.

```rust
Number { low, high, step: Option<f64>, scale: Scale }
Choice { options: Vec<String> }
Point  { low: (f64, f64), high: (f64, f64) }
Flag
```

**`Scale` is new in 1.0**: `Linear` or `Log`. A figure that asks the
reader to move a resistance across three orders of magnitude needs a
logarithmic slider; a linear one spends 90% of its travel in the top
decade.

Guarantees that must survive:

- **Clamping, not rejection**, for numbers and points. A `Choice`
  outside its options *is* rejected, naming the options.
- **A continuous slider is never coarse**: `step: None` resolves to a
  hundredth of the range. (A real defect inherited from kinemo gave
  two-position sliders; the existing test must stay.)

### 3.3 Multiple values per figure

**Every figure kind takes any number of named values.** This is the
single most consequential change from the current state, where each
figure has exactly one.

The core is already a map of named knobs. 1.0 requires the limit to
disappear from the figure constructors, the WASM surface, and the spec
format.

A figure declares its values; the page builds one control per value,
chosen by type:

| Domain | Control |
|---|---|
| `Number`, linear or log | a slider |
| `Choice` | a row of buttons |
| `Point` | dragging inside the figure |
| `Flag` | a checkbox or a toggle |

---

## 4. The figure kinds

Six kinds. They were not invented: they are the families that twelve
real teaching figures collapse into, plus the two that already exist.

### 4.1 `plot` — a curve with decoration

A function over an axis, with things attached to it.

| Field | Meaning |
|---|---|
| `curve` | formula over the figure's values |
| `tangent` | draw the tangent at the point |
| `area` | fill under the curve, between the curve and an axis or a second curve |
| `marks` | drop-lines, projections, labelled points |
| `curves[]` | **several curves on the same axes**, each with its own style and label |

New in 1.0: `area`, `marks`, several curves, and — critically —
**curves that change shape when a value moves**. Today a curve is
sampled once and frozen; 1.0 requires resampling (§8.2).

*Teaches:* how a quantity varies; where it vanishes; how an area
accumulates into a total.

### 4.2 `columns` — vectors and the area they span

Two or more column vectors as arrows, with the parallelogram they span.
The area going to zero **is** the determinant going to zero.

New in 1.0: more than two columns, labels on each, and an optional
readout of the determinant.

*Teaches:* linear dependence, orientation, what a matrix does to space.

### 4.3 `diagram` — named objects positioned by expressions

The most general kind, and the one that unlocks the most.

The author lists objects; every coordinate, size, colour and thickness
is a formula over the figure's values:

```
objects:
  - { shape: arrow, from: [0,0], to: [x, y], colour: hot, label: "Jx" }
  - { shape: box,   at: [0,0], w: 2, h: 1.5, label: "plate" }
  - { shape: node,  at: [-2, h1], label: "x1" }
  - { shape: edge,  from: node1, to: node2, width: "abs(h1 - h2)" }
  - { shape: cell,  at: [i, j], value: "a_ij", fill_by: value }
```

Shapes: `dot`, `line`, `arrow`, `box`, `spring`, `node`, `edge`,
`cell`, `label`, `bracket`, `angle`.

*Teaches:* structure. Port diagrams, block diagrams, networks, matrices
as grids of numbers, free-body sketches. Nothing in a `plot` serves a
chapter about **how things are connected**.

### 4.4 `trajectory` — integrate and trace

An initial state, a step rule as formulas, a step size, a number of
steps. hapto integrates **at build time** and draws the path.

| Field | Meaning |
|---|---|
| `state` | named coordinates and their starting values |
| `rule` | a formula per coordinate for its rate of change |
| `steps`, `dt` | how far and how finely |
| `method` | `symplectic` (default), `explicit`, `implicit`, `midpoint` |
| `traces[]` | what to draw: the path, or a quantity against time |
| `background` | level curves of a conserved quantity, or a direction field |

**The default must be symplectic**, and this is not a preference. A
hand-built baseline hit the same defect three separate times: an
explicit Euler step made the figure contradict its own caption — energy
rising from 2.825 to 3.664 over 2600 steps; a radius growing from 2.60
to 2.87 in a purely rotational system; a conserved total rising in 717
of 1400 steps. A figure whose caption says "the energy is constant"
while the drawing spirals outward is worse than no figure.

`method` is nonetheless exposed, because **comparing integrators is
itself a lesson** — one baseline figure shows three of them racing the
true orbit.

*Teaches:* dynamics. Phase portraits, decay, oscillation, what a system
does over time, and what a numerical method does to the truth.

### 4.5 `field` — a value at every point of the plane

An expression evaluated across a region and shown as colour, arrows, or
contours.

| Mode | Shows |
|---|---|
| `heat` | colour by value — where a quantity is positive, negative, zero |
| `arrows` | a direction field |
| `contours` | level curves |

*Teaches:* that a property holds **everywhere**, not just at the point
you picked. One baseline figure asks the reader to hunt the whole plane
for a point where a quadratic form is not zero, and fail.

**The `arrows` mode is nearly free.** The renderer has a mass-drawing
path with `vector_field` and `stream_lines` object kinds, taking a field
expression plus a `density` or `seed_count` — direction fields and flow
lines are already implemented and simply not exposed by hapto.

`heat` and `contours` are the genuinely new ones; see §8.3.

### 4.6 `series` — quantities against time or against each other

Several traces on shared axes, with labels, from data the figure
computes (usually from a `trajectory`).

*Teaches:* comparison over a run. Two energies trading while their sum
only falls; a charge redistributing while the total holds flat.

---

## 5. Drawing

### 5.1 Text — the precondition

**A figure that cannot label anything cannot teach.** All twelve
baseline figures carry text; in two of them the text *is* the figure.

1.0 requires:

| Need | Example |
|---|---|
| Fixed labels | axis names, a name on an arrow |
| Tick values | numbers along an axis |
| Live readouts | a value that updates as the reader drags |
| Rotation | a vertical y-axis label |
| Typeset mathematics | a partial derivative, a power, a subscript — not ASCII fallback |
| Alignment | anchor a label by its centre, its corner, or its baseline |

The renderer already draws text and typesets mathematics, and those
crates are already in the binary. What is missing is a primitive that
emits a text object and a figure-level convention for where labels go.

### 5.2 Shapes

| Primitive | Status in 1.0 |
|---|---|
| `dot`, `line`, `arrow`, `rect` | exist; must accept value-following styles |
| `polyline`, `polygon` | exist; must accept **moving points** (§8.2) |
| `spring`, `quad` | exist |
| `text` | **new to hapto** (§5.1) |
| `arc`, `circle`, `ellipse` | **new to hapto** — angle markers, level curves |
| `path` | **new to hapto** — any curve, including closed and filled |
| `bracket` / `brace` | **new to hapto** — "this part is the dissipation" |

**Every one of those was verified to exist in the renderer already.**
`kinemo-layout`'s geometry pass dispatches on `text`, `arc`, `circle`,
`ellipse`, `path`, `brace` and `glyphs` among others; hapto emits six of
the nineteen kinds it understands. **This is exposure work, not renderer
work** — a `Build` method per shape, emitting the object with the right
props.

What still has to be checked per shape: which props it reads, and
whether each may be a signal (and so follow a value) or must be a
literal. The `points`-based kinds hit the list limitation in §8.2.

### 5.3 Style

Every visual property must be able to follow a value:

```
stroke_width: Num     fill: colour or expression
stroke: colour        fill_opacity: Num
dash: pattern         colour_by: expression + scale
```

Today all are fixed numbers in hapto's `Style` struct. One baseline
figure encodes its entire lesson in edge thickness — the dissipation of
a network — and is blocked by exactly this.

**The renderer is not the obstacle.** It reads `stroke_width` and
`fill_opacity` through the same property lookup that resolves signals,
so a style property *can* follow a value the moment hapto stops forcing
it to a literal. `dash` is likewise already a property the renderer
reads. The work is in hapto's `Style`: make its fields `Num` rather than
`f64`, and thread them through `props()`.

### 5.4 Readability

- A **theme** with light and dark variants; a figure must be legible in
  both without the author thinking about it.
- Every figure kind draws **axes with labels and ticks** by default,
  where axes make sense.
- A palette with semantic names (`hot`, `cool`, `neutral`, `accent`) so
  an author says what a colour *means*.
- Figures must remain legible at phone width.

---

## 6. The spec format

A figure is JSON. This is the contract between whoever writes the
content and hapto.

```json
{
  "kind": "trajectory",
  "values": [
    { "name": "b", "type": "number", "low": 0, "high": 0.6,
      "at": 0, "label": "friction" },
    { "name": "p", "type": "point", "low": [-4,-4], "high": [4,4],
      "at": [2.2, 0.9] },
    { "name": "method", "type": "choice",
      "options": ["symplectic", "explicit"], "at": "symplectic" }
  ],
  "state": { "q": "p.x", "p": "p.y" },
  "rule":  { "q": "p", "p": "-q - b*p" },
  "steps": 2600, "dt": 0.01,
  "background": { "contours": "0.5*(q**2 + p**2)" },
  "readouts": [
    { "label": "H", "formula": "0.5*(q**2 + p**2)", "digits": 3 }
  ],
  "axes": { "x": "q", "y": "p" }
}
```

Requirements on the format:

1. **Every number may be a formula** over the figure's values.
2. **A spec that cannot be drawn is refused with a reason**, naming what
   was wrong and what the options are. Never a blank canvas in a
   reader's browser.
3. **Formulas use `**` for powers.** A caret is refused with an
   explanation — it means exclusive-or in the languages the content
   pipeline uses, and a lesson was once graded wrongly because of it.
4. **Names may carry subscripts** (`x_f`, `e_R`, `q_b`), because the
   prose-hover feature matches the variable as the prose spells it.
5. **The format is versioned**, and 1.0 commits to it.

### Readouts

Numbers printed beside the figure, computed from the same formulas the
drawing uses — so a figure and the number next to it cannot disagree.

---

## 7. The browser surface

```js
const fig = Figure.fromSpec(spec, width, height);

fig.values();                  // [{name, type, domain, label}, ...]
fig.set("x", 3);               // a slider, a drag, or a word in the prose
fig.set("mode", "damped");
fig.set("p", {x: 1.2, y: -0.4});
fig.get("x");
fig.readouts();                // [{label, value}, ...] for the page to print
fig.draw();                    // pixels
fig.hitTest(px, py);           // which value a drag at this pixel moves
fig.resize(w, h);
```

Requirements:

- **`values()` replaces `variable()`.** The page builds controls from
  the list, not from one name.
- **`hitTest` makes dragging possible** without the page knowing the
  figure's internal geometry.
- **Redraw must be fast enough to drag against.** A full re-render per
  frame is acceptable only if it stays under roughly 16 ms at typical
  size; otherwise 1.0 needs incremental redraw or a lighter path.
- **Errors throw with a reason**, in the style already established.

---

## 8. The three hard problems

Everything else in this document is exposure work. These three need
design.

### 8.1 Multiple values, end to end

The core is already a map of named knobs, and one built-in example
already declares two. The narrowing happens in the figure constructors
and the WASM surface. The work is to remove it everywhere and define how
a page lays out several controls.

### 8.2 Shapes that deform

A curve is currently sampled once into fixed points, because the
underlying IR has no expression that builds a *list* — so a polygon's
corners cannot each follow a value. This is why a `plot`'s knob moves
the point and the tangent but never the curve.

Two routes:

- **Resample and rebuild** the scene whenever a value changes. Cheaper
  to implement; turns `set` from "write a signal" into "rebuild", which
  is a performance decision.
- **Add a list expression** to the IR. Deeper, and it is kinemo's
  codebase, not hapto's.

1.0 needs one of them. The first is probably right.

### 8.3 Heat maps and contours

The `arrows` mode of `field` is already solved by the renderer (§4.5).
The other two are not.

A heat map drawn as one object per cell would be thousands of objects
with properties, re-evaluated per frame — the hand-built baseline uses
roughly 2 400 filled cells for one figure. Contours need marching
squares over a sampled grid.

Either could follow the pattern the existing mass-drawing path already
sets: one object whose draw call expands into many items internally,
rather than many objects in the scene. **That is the recommended route**,
and it is the one place in 1.0 where the work may land inside kinemo
rather than hapto.

Both modes serve one baseline figure each. If the schedule is tight,
ship `field` with `arrows` only and defer the rest.

---

## 9. Build order

Each stage should leave the library more useful than it found it.

| # | Stage | Unlocks |
|---|---|---|
| 1 | **Text** — labels, ticks, readouts, maths | a precondition for all twelve baseline figures |
| 2 | **Multiple values** + controls per type | eleven of twelve |
| 3 | **Styles that follow values**, dashes | the thickness and emphasis lessons |
| 4 | **`diagram`** | five figures, no new drawing primitives needed |
| 5 | **`trajectory`** + `series` | five figures, the dynamics chapters |
| 6 | **Deforming shapes** (§8.2) | curves that change, areas that fill |
| 7 | **`field`** | the "find a counterexample and fail" gesture |

Stages 1 and 2 are preconditions: little else is worth building before
them.

---

## 10. Acceptance

1.0 is done when **all twelve figures of the baseline are reproducible
from specs alone**, with no hand-written drawing code.

The baseline is at `phx/docs/_archive/handbook/demos.js`. Each figure
maps to a kind:

| Baseline figure | Kind | What it must show |
|---|---|---|
| `rotation` | diagram | a vector, its rotation, the right angle between them, labels |
| `skew` | field + diagram | a quadratic form over the whole plane; mode buttons |
| `coenergy` | plot | area under a curve = energy, height = voltage; two curves to switch between |
| `phase` | trajectory | level curves, a path that closes, two perpendicular arrows |
| `ports` | diagram | a box, three labelled ports, arrows sized by power, a conservation constraint |
| `atoms` | trajectory + field | a direction field and a path, blending between two behaviours |
| `domains` | diagram | a rectangle whose area is power, four unit systems |
| `motor` | diagram | three matrices as grids of numbers, three sliders |
| `passive` | series | three traces, two rising while their sum falls |
| `caps` | trajectory + series | two charges redistributing, a conserved total, a log slider |
| `laplacian` | diagram | nodes, edges whose thickness is the dissipation, buttons |
| `integrators` | trajectory | three methods against the true orbit, one step-size slider |

### Quality gates

1. **Every figure kind has a test that counts painted pixels.** Tests
   that compare frames to each other pass when both frames are blank —
   that defect shipped once.
2. **Render and look.** Three separate blank-frame defects passed every
   test that existed when they were written. A rendered PNG is part of
   reviewing a new kind.
3. **Any simulated invariant is checked before the figure ships.**
   Compare the conserved quantity at the start and the end; a drift
   above roughly 1% is a defect, not a rounding artefact.
4. **A bad spec names what was wrong.** Never silence, never blank.
5. **Light and dark, desktop and phone**, for every kind.

---

## 11. Explicitly not in 1.0

Stated so nobody builds them by accident:

- **A timeline.** The one-instant scene is the design. Integration in
  `trajectory` happens at build time, against no clock.
- **3D.**
- **Animation on load.** A figure is still until touched.
- **Interactive editing of the figure by the reader** beyond setting
  declared values — no drawing, no annotation.
- **Data fetching.**
- **A general plotting library.** hapto draws figures that teach a
  specific idea. If a request is "plot this dataset", that is a
  different tool.

---

## 12. Open questions for the team

1. **Resample or extend the IR** (§8.2)? This is the first fork in the
   road and affects the performance budget everywhere.
2. **What is the redraw budget?** Whether a full re-render per drag
   frame is acceptable determines how much of §7 needs rework.
3. **How does a page lay out several controls** without the author
   designing a form each time?
4. **Does `field` need a renderer primitive**, or is a coarse
   interpolated grid enough (§8.3)?
5. **How far does `diagram` go** before it becomes a general drawing
   language? A diagram of five labelled boxes is in scope; a flowchart
   engine is not. The boundary needs a rule.
