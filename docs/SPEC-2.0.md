# hapto 2.0 — specification

**Any figure a book needs, and the book around it.**

| | |
|---|---|
| Status | Draft for agreement. Supersedes `docs/SPEC-1.0.md`. |
| Date | 2026-10-31 |
| Baseline code | commit `781a20f` (see `docs/current-state.md`) |
| Pinned kinemo | `4cbd012bc8ca29a53696178b9a6c8bff68602144` |
| Format version defined here | `hapto: 2` |

The key words **MUST**, **MUST NOT**, **SHOULD**, **SHOULD NOT** and **MAY**
mean what RFC 2119 says they mean. Claims about kinemo were confirmed in
code during milestone M0 (`docs/m0.md`, Appendix C.5); the measurements that
decided the rebuild model, maths and heat maps are there too.

---

## Contents

0. [How to read this document](#0-how-to-read-this-document)
1. [Purpose and principles](#1-purpose-and-principles)
2. [Scope](#2-scope)
3. [Architecture](#3-architecture)
4. [The formula language](#4-the-formula-language)
5. [Values and state](#5-values-and-state)
6. [The figure document](#6-the-figure-document)
7. [Views, coordinates and layout](#7-views-coordinates-and-layout)
8. [Marks](#8-marks)
9. [Style and theme](#9-style-and-theme)
10. [Text and mathematics](#10-text-and-mathematics)
11. [Computation](#11-computation)
12. [Interaction](#12-interaction)
13. [Readouts](#13-readouts)
14. [Presets](#14-presets)
15. [The page runtime](#15-the-page-runtime)
16. [hapto-book](#16-hapto-book)
17. [Command line](#17-command-line)
18. [Errors](#18-errors)
19. [Performance and limits](#19-performance-and-limits)
20. [Accessibility](#20-accessibility)
21. [Quality gates and testing](#21-quality-gates-and-testing)
22. [Acceptance corpus](#22-acceptance-corpus)
23. [Milestones](#23-milestones)
24. [Decisions that change 1.0 and `why.md`](#24-decisions-that-change-10-and-whymd)
25. [Non-goals](#25-non-goals)
26. [Open questions](#26-open-questions)
- [Appendix A — worked examples](#appendix-a--worked-examples)
- [Appendix B — formula grammar](#appendix-b--formula-grammar)
- [Appendix C — what kinemo already provides](#appendix-c--what-kinemo-already-provides)

---

## 0. How to read this document

This is a target, not an inventory. It describes hapto 2.0 as a whole: the
figure language, the browser runtime that puts figures on a page, and the
book compiler that produces whole interactive books. A team should be able to
build from it and know when it has finished (§22, §23).

- **If you are building the engine:** §3, §4, §5, §8, §11, §19, Appendix C.
- **If you are writing figures (human or agent):** §5–§14 and Appendix A.
- **If you are integrating with a page or a book:** §15, §16.
- **If you are reviewing scope:** §1, §2, §24, §25, §26.

What changed from 1.0, in one paragraph: 1.0 was a catalogue of six figure
kinds reverse-engineered from twelve figures of one course. 2.0 is a
**language**: values, derived quantities, views, and layers of marks, from
which any of those kinds — and the ones no single course anticipated — can be
written. The kinds survive as **presets** that expand into the language. The
scene is **rebuilt from state** on every change, which removes the list
limitation that blocked moving curves, polygons and dynamic counts. And
hapto now owns the **page**: a runtime that wires prose to figures, and a
book compiler that emits whole books.

---

## 1. Purpose and principles

### 1.1 What hapto is for

A **figure that answers to the reader**, living beside the paragraph that
explains it, in a book made of such paragraphs.

```
state:   x = 3, mode = "damped", p = (1.2, -0.4)
figure:  f(state) -> drawing
page:    prose  <-> state <-> figures
```

Nothing advances on its own. A value changes and every drawing and every
sentence that reads it follows.

### 1.2 The test every figure must pass

> **What does the reader understand after touching this that they did not
> understand before?**

If the honest answer is "they watched a number change", the figure should
not exist. A figure earns its place when touching it reveals a **relation
that is hard to hold in the head**: two quantities trading off, something
vanishing exactly where the text says it breaks, a shape collapsing, a
growth that is not what you would guess, a property that holds everywhere
and not only at the point you picked.

This test is not enforced by the engine. It is enforced by review, and it is
the reason some obvious-looking features are excluded in §25.

### 1.3 Principles

These decide disputes. When a design choice is unclear, the option that
better honours them wins.

1. **State is the only input.** A drawing is a pure function of the values,
   the theme and the size. Two equal states MUST produce identical pixels on
   the same build.
2. **The page is the clock.** The engine never advances time. Anything that
   moves on its own (a play button) is the page calling `set` repeatedly.
3. **One way to do each thing.** The format is written by agents as much as
   by people. Where two spellings would do, there is one. Unknown keys are
   errors, not ignored.
4. **Refuse at build, degrade at runtime.** Anything that can be known
   before a reader opens the page (a typo, a type error, a blank figure, a
   drifting invariant) MUST fail the build with a reason. Anything that can
   only happen at runtime (a division by zero at one slider position) MUST
   degrade locally — skip the mark, print "—" — and never blank the figure.
5. **Formulas over numbers.** Everything drawn and everything printed reads
   the same formulas, so a figure and the sentence beside it cannot
   disagree.
6. **Every error says what to do.** It names the thing that is wrong, where
   it is, and the way out, with the valid options when there are few.
7. **Look at it.** Three blank-frame defects passed every test that existed
   when they were written. A rendered image is part of reviewing anything
   visual, and the test suite counts painted pixels.
8. **Screen units for ink, data units for meaning.** Positions are in the
   figure's own coordinates; stroke widths, text sizes, dot radii and hit
   areas are in CSS pixels. An author never guesses how wide a line is.

---

## 2. Scope

### 2.1 In

- A formula language with types, user functions, comprehensions, and a
  standard library for calculus, linear algebra, geometry, probability,
  complex numbers and colour (§4).
- Values of four types — number, choice, point, flag — any number per
  figure, plus values shared across a page (§5).
- One or more **views** per figure, each a coordinate system with axes,
  ticks and labels; equal-aspect, logarithmic and polar options (§7).
- A complete set of **marks**: geometric primitives, function plots, areas,
  text and typeset mathematics, scatter and histograms, fields, contours,
  matrices as grids, graphs of nodes and edges, groups with transforms, and
  repetition (§8).
- Styles that follow values; a light and dark theme with semantic colours
  (§9).
- **Computation at rebuild time**: ODE integration with a safe default,
  discrete iteration, roots and extrema, numerical derivatives and
  integrals, seeded randomness (§11).
- **Interaction**: generated controls, dragging any anchored mark with
  arbitrary formulas mapping the pointer to values, hover focus linking
  marks and prose, play buttons, reset (§12).
- **Presets** for common figures, each expanding to the base language (§14).
- **A page runtime** (`hapto.js` + `hapto.wasm`): a custom element, prose
  bindings in both directions, shared state, lazy loading, workers,
  device-pixel-ratio rendering, print, no-JS fallback (§15).
- **A book compiler** (`hapto book`): Markdown with extensions in, a static
  interactive book out, with typeset maths in prose, cross-references,
  tasks checked against figure state, local progress, and a build that is
  the quality gate (§16).
- A command line that renders, checks, expands and explains (§17).

### 2.2 Out

- **A timeline.** Anything that advances on its own belongs in kinemo, or in
  the page calling `set` (§12.6).
- **3D**, including projected 3D surfaces. A 2D figure of a 3D idea
  (contours, cross-sections, a slider for the third coordinate) is in scope.
- **Algorithm visualisation with discrete program state** — a sorting array
  stepping through swaps, a tree being balanced. These need a model of
  "state produced by running a program", which is a different library.
  *Numeric* iteration (Newton's method, a logistic map, gradient descent) is
  in scope through `iterate` (§11.3).
- **Automatic graph layout.** Node positions are formulas or one of a few
  deterministic arrangements (§8.10). Force-directed layout is out.
- **Data loading.** Mathematics arrives as formulas in the document. A small
  literal table (§4.6, `interp`) is allowed; fetching a CSV is not.
- **Reader-authored content** beyond setting declared values: no drawing,
  no annotation, no comments.
- **Accounts, servers, synchronisation of progress.** A book is static files;
  progress lives in the reader's browser.
- **A general plotting library.** If the request is "plot this dataset", that
  is a different tool.
- **A theming engine** for books. One theme with tokens an author may
  override (§16.11).

---

## 3. Architecture

### 3.1 Layers

```
                     ┌─────────────────────────────────────────────┐
  author / agent ──▶ │ book source: book.yaml + chapters/*.md      │
  hmu (compiler) ──▶ │   prose, maths, ```hapto figures, bindings  │
                     └───────────────┬─────────────────────────────┘
                                     │ hapto book build
                     ┌───────────────▼─────────────────────────────┐
                     │ hapto-book (Rust, CLI)                      │
                     │  parse Markdown, typeset prose maths,       │
                     │  validate bindings, check every figure,     │
                     │  render fallbacks, emit static site         │
                     └───────────────┬─────────────────────────────┘
                                     │ static HTML + hapto.js + hapto.wasm
                     ┌───────────────▼─────────────────────────────┐
  any HTML page ───▶ │ hapto-page (JS runtime, `hapto.js`)         │
                     │  <hapto-figure>, controls, prose bindings,  │
                     │  page state, worker, lazy, print            │
                     └───────────────┬─────────────────────────────┘
                                     │ set / render messages
                     ┌───────────────▼─────────────────────────────┐
                     │ hapto-web (wasm-bindgen surface)            │
                     └───────────────┬─────────────────────────────┘
                     ┌───────────────▼─────────────────────────────┐
                     │ hapto-core: document model, validation,     │
                     │  compute, marks → scene, hit geometry       │
                     │ hapto-expr: formula language and stdlib     │
                     └───────────────┬─────────────────────────────┘
                                     │ kinemo Scene (all literal props)
                     ┌───────────────▼─────────────────────────────┐
                     │ kinemo-layout, -text, -math, -render        │
                     │  (pinned git dependency)                    │
                     └─────────────────────────────────────────────┘
```

Each layer is usable without the ones above it:

- `hapto-core` renders a figure document to pixels or SVG with no browser.
- `hapto-page` puts figures on **any** HTML page; a hand-written page with
  `<hapto-figure>` and `data-hapto-*` attributes is a first-class user.
- `hapto-book` is one producer of such pages, not the only one.

### 3.2 Crates and artefacts

| Crate / artefact | Language | Responsibility |
|---|---|---|
| `hapto-expr` | Rust | Lexer, parser, type checker, evaluator and standard library of the formula language. **No kinemo dependency.** |
| `hapto-core` | Rust | The figure document model (serde), validation, compute nodes, views, marks, the scene builder, hit geometry, presets, rendering to RGBA and SVG. |
| `hapto-web` | Rust → wasm | `wasm-bindgen` surface over `hapto-core` (§15.10). |
| `hapto-page` | JavaScript (ES module, no build step required by users) | The page runtime: `hapto.js`, its worker `hapto.worker.js`, and `hapto.css`. Distributed alongside `hapto.wasm`. |
| `hapto-book` | Rust | The `hapto` binary: `render`, `check`, `expand`, `explain`, `schema`, `book build`, `book serve`, `book check` (§17). |

The 1.0 crates `hapto-core` and `hapto-web` keep their names. `book.rs` and
`demos.rs` are replaced by presets (§14) and the acceptance corpus (§22).

### 3.3 The rebuild model

1.0 drew a figure by writing state into the initial values of a fixed
scene's signals. That cannot express a curve whose shape follows a value, a
polygon whose corners move, or a number of marks that depends on a slider,
because kinemo's expression IR cannot build a list.

2.0 changes the model. **The scene is a function of the state, rebuilt on
every change:**

```
document ──parse/validate──▶ Program   (once, on load)
Program + State + Theme + Size ──evaluate──▶ Scene   (every change)
Scene ──layout + raster──▶ pixels      (every change)
```

- **Program** is the compiled, type-checked document: formulas as typed
  expression trees, a static dependency graph from each value to every
  derived quantity, compute node and mark that reads it.
- **Evaluate** runs in `hapto-expr` natively in Rust. Every mark is reduced
  to kinemo objects whose props are **literal values** — no kinemo `Expr`,
  no signals bound to state.
- **Dirty tracking.** Evaluation MUST be incremental by dependency: a change
  to `x` re-evaluates only the lets, compute nodes and marks that read `x`,
  directly or transitively. A trajectory that does not read `x` is not
  re-integrated when `x` moves.

#### The reconciler

The semantics are "rebuild"; the implementation MAY patch. To keep kinemo's
caches (text shaping, maths layout) warm, the builder SHOULD keep one
persistent kinemo `Scene` and reconcile:

- Every emitted object has a **stable key**: the mark's path in the
  document plus its repeat indices (`marks/3[i=4]`).
- A key present before and after: write changed props in place.
- A key that disappears: remove the object. A new key: add it.
- Objects whose text, TeX or font size did not change MUST NOT be re-shaped.

**M0 decided: no reconciler** (`docs/m0.md`). Building a scene costs
0.01–0.1 ms and layout 0.1–1.5 ms, while raster is 75–95% of every frame,
and kinemo's text and maths caches are global, so a fresh scene does not
re-shape strings it has seen. The scene is rebuilt from nothing on every
change. The stable keys above are still assigned, because hit testing and
focus need them.

#### Batching rule

A mark that produces many vector primitives — a contour set, a vector
field, a histogram, a grid — MUST emit **one kinemo `path` object per
distinct style**, not one object per primitive. This is the rule that makes
fields affordable without new renderer work.

**Heat maps are the exception**, and are painted as a **raster underlay**
(§8.11): batched paths measured 26 ms per frame and drew visible seams
between cells, where an underlay measured 5 ms and is seamless (`docs/m0.md`
§2, §5).

### 3.4 What kinemo provides, and what hapto owns

hapto owns: the formula language and its evaluation, the document format,
values and state, compute, coordinate mapping, every mark's geometry,
interaction, hit testing, presets, the page and the book.

kinemo provides, unchanged and pinned: text shaping, TeX maths layout,
object geometry for the kinds hapto emits, style resolution, z-ordering,
rasterisation, and SVG export. The verified list is in Appendix C. hapto
emits only literal props, so it depends on kinemo's **geometry and paint**,
not on its evaluator or timeline.

A change needed inside kinemo is a separate commit there and a pin bump
here, as today. The one kinemo change anticipated for 2.0 is font
subsetting (§10.5, §26 question 5); nothing else on the critical path needs
kinemo to change.

### 3.5 Determinism

- All arithmetic is IEEE-754 `f64`.
- Transcendental functions MUST come from one implementation on every
  target (the `libm` crate), so a native test and the browser compute the
  same digits.
- Randomness is seeded and counter-based (§11.6); there is no ambient
  random source.
- Golden-image tests compare with a tolerance (§21.3) because rasteriser
  anti-aliasing may differ between native and wasm by a few levels.

---

## 4. The formula language

Formulas are strings inside the document. They are parsed once, type-checked
against the figure's values, lets and compute nodes, and evaluated natively.
The dialect extends the one hmu lessons already use (`x**2 - x*(2)`) and
stays close to Python where that is unambiguous, because both authors and
agents already read Python.

The full grammar is in Appendix B.

### 4.1 Lexical rules

| Element | Rule | Examples |
|---|---|---|
| Number | decimal, optional fraction, optional exponent | `3`, `0.5`, `.5`, `1e-3`, `6.02e23` |
| String | double or single quotes, with `\"`, `\'` and `\\` escapes | `"damped"`, `'damped'` |
| Boolean | `true`, `false` | |
| Colour | `#rgb`, `#rrggbb`, `#rrggbbaa` | `#d55e00` |
| Identifier | `[A-Za-z][A-Za-z0-9_]*` | `x`, `x_f`, `e_R`, `q_b`, `theta2` |
| Whitespace | insignificant | |

**Refused, with a hint:**

| Input | Error | Hint |
|---|---|---|
| `2^3` | `^` is exclusive-or, not a power | write a power as `2**3` |
| `2x` | implicit multiplication | write `2*x` |
| `x²` | non-ASCII operator | write `x**2` |
| `3,5` | comma as decimal separator (when unambiguous) | write `3.5` |
| `==` between floats | allowed, but `check` warns | compare with `abs(a - b) < tol` |

**Reserved words**: `if then else and or not for in true false pi tau e`
plus every standard library function name (§4.6) and the context names
`pointer`, `start`, `anchor0`, `delta`, `focus`, `theme`. A value, let or
compute id MUST NOT use a reserved word; the error lists the reserved words
it collided with.

### 4.2 Types

| Type | Written as | Notes |
|---|---|---|
| `num` | `3`, `x * 2` | `f64`. |
| `bool` | `true`, `x > 0` | Not a number. `num(b)` converts to 1/0. |
| `str` | `"damped"` | Choice values, labels, formatted text. |
| `vec` | `[1, 2]`, `vec(a, b)`, a point value | A 2D vector or point. Also the complex number type (§4.6.8). |
| `colour` | `#d55e00`, `palette("hot")`, `mix(a, b, t)` | Linear RGBA internally, mixed in OKLab. |
| `list<T>` | `[1, 2, 3]`, `range(0, n)`, comprehensions | Homogeneous. |
| `mat` | `mat([a, b], [c, d])` | Row-major, 1×1 to 4×4. |
| record | results of compute nodes and `eig` | Field access only (`traj.q`, `eig(M).values`). |

**List or vector?** `[a, b]` with exactly two `num` elements is a `vec` when
the context requires a `vec` and a `list<num>` when the context requires a
list. Where the context accepts either (a `let` with no consumer), it is a
`vec`. `list(...)` and `vec(...)` force the choice. The type checker reports
which reading it chose in `hapto explain` (§17).

**No implicit conversions** between `num`, `bool` and `str`. `if` requires a
`bool`; arithmetic requires `num` or `vec`.

### 4.3 Operators

From loosest to tightest binding:

| Level | Operators | Associativity | Types |
|---|---|---|---|
| 1 | `if c then a else b` | — | `c: bool`, `a` and `b` the same type |
| 2 | `or` | left | `bool` |
| 3 | `and` | left | `bool` |
| 4 | `not` | prefix | `bool` |
| 5 | `< <= > >= == !=` | **non-associative** (`a < b < c` is an error) | `num`; `==`/`!=` also `str`, `bool` |
| 6 | `+ -` | left | `num`, `vec` (elementwise), `mat` |
| 7 | `* / %` | left | see below |
| 8 | unary `-` | prefix | `num`, `vec`, `mat` |
| 9 | `**` | right; right operand may be unary (`2**-1`) | `num`, `mat ** int` |
| 10 | call `f(...)`, index `xs[i]`, field `.name` | left | |

`-x**2` is `-(x**2)`, as in Python and as every lesson expects.

Multiplication and division by type:

| Left | Right | `*` | `/` |
|---|---|---|---|
| `num` | `num` | product | quotient |
| `num` | `vec` | scaled vector | error |
| `vec` | `num` | scaled vector | scaled vector |
| `vec` | `vec` | **error** — use `dot`, `cross` or `cmul` | error |
| `mat` | `vec` | matrix applied to vector | error |
| `mat` | `mat` | matrix product | error |
| `num` | `mat` | scaled matrix | error |
| `mat` | `num` | scaled matrix | scaled matrix |

`%` is the mathematical modulo: the result has the sign of the divisor
(`-1 % 3 == 2`), which is what angle wrapping wants.

### 4.4 Names and scope

A formula may read, in this order of lookup:

1. **Bound variables** of the enclosing construct: a comprehension variable,
   a `repeat` variable (§8.13), a function mark's variable (§8.6), a field
   mark's variables (§8.11), a per-point name (§8.7), a user function's
   parameters, an interaction's context names (§12.4).
2. **Lets** of the figure (§4.5).
3. **Compute nodes** of the figure (§11).
4. **Values** of the figure, then values the figure imports from the page
   (§5.6).

A bound variable **shadows** a let, compute node or value of the same name
inside its own construct. This is deliberate: the classic figure has a
slider called `x` and a curve `f(x)` over the same `x`. Shadowing anywhere
else (a let named like a value, a value named like a compute node) is an
error.

`hapto explain` prints, for each formula, what each name resolved to.

### 4.5 Lets and user functions

```yaml
let:
  slope: "derivative(f, x)"
  "f(t)": "t**2 - 2*t"
  "g(u, v)": "u*cos(v)"
  tip: "[x, f(x)]"
  underdamped: "zeta < 1"
```

- A key without parentheses is a **derived quantity**.
- A key `name(p1, p2, ...)` is a **user function**. Parameters are `num`
  unless used otherwise; the type checker infers them, and reports an
  ambiguity as an error asking for a call that disambiguates.
- Lets are ordered by dependency, not by position. A cycle is an error that
  prints the cycle (`slope → f → slope`).
- **Recursion is not allowed**, directly or through other functions.
  Iteration is what `iterate` (§11.3) and comprehensions are for.
- User functions are not values: they can be called and passed to the
  calculus functions (`derivative(f, x)`, `integral(f, a, b)`,
  `roots(f, a, b)`), and nothing else. There are no lambdas.

### 4.6 Standard library

Every function listed here MUST exist in 2.0, with a test. Angles are in
**radians** everywhere unless a name says otherwise.

#### 4.6.1 Constants

`pi`, `tau` (= 2π), `e`.

#### 4.6.2 Arithmetic

| Function | Meaning |
|---|---|
| `abs(x)`, `sign(x)` | |
| `floor(x)`, `ceil(x)`, `trunc(x)` | |
| `round(x)`, `round(x, digits)` | half away from zero |
| `min(a, b, ...)`, `max(a, b, ...)` | variadic; also `min(list)`, `max(list)` |
| `clamp(x, lo, hi)` | |
| `lerp(a, b, t)` | `num`, `vec` or `colour` (colour mixes in OKLab) |
| `remap(x, a, b, c, d)` | maps `[a, b]` onto `[c, d]` linearly |
| `sqrt(x)`, `cbrt(x)`, `hypot(a, b)` | |
| `exp(x)`, `ln(x)`, `log10(x)`, `log2(x)`, `log(x, base)` | |
| `num(b)` | `bool` → 1 or 0 |
| `int(x)` | asserts an integer: `round(x)`, and `check` warns if `x` was not within 1e-9 of one |

#### 4.6.3 Trigonometry

`sin cos tan asin acos atan atan2(y, x) sinh cosh tanh`, `deg(x)` (degrees →
radians, so `deg(90) == pi/2`), `to_deg(x)` (radians → degrees),
`wrap(angle)` (into `(-pi, pi]`).

#### 4.6.4 Vectors and geometry

| Function | Meaning |
|---|---|
| `vec(x, y)` | construct |
| `v.x`, `v.y` | components |
| `norm(v)`, `dist(a, b)` | lengths |
| `dot(a, b)`, `cross(a, b)` | `cross` is the scalar z-component |
| `unit(v)` | `v / norm(v)`; `[0, 0]` stays `[0, 0]` and `check` warns |
| `perp(v)` | rotated +90° |
| `rotate(v, angle)`, `rotate(v, angle, about)` | |
| `angle(v)`, `angle_between(a, b)` | `angle_between` is signed, in `(-pi, pi]` |
| `polar(r, theta)` | `[r cos θ, r sin θ]` |
| `mid(a, b)` | midpoint |
| `proj(v, onto)` | vector projection |
| `reflect(v, normal)` | |
| `intersect(p1, d1, p2, d2)` | intersection of two lines given point+direction; no value if parallel |
| `circle_through(a, b, c)` | record `{center, radius}` |

#### 4.6.5 Lists

| Function / form | Meaning |
|---|---|
| `[expr for v in list]` | comprehension |
| `[expr for v in list if cond]` | filtered comprehension |
| `sum(expr for v in list)`, `prod(...)` | generator forms |
| `sum(list)`, `prod(list)`, `mean(list)`, `min(list)`, `max(list)` | reductions |
| `range(a, b)`, `range(a, b, step)` | **end-exclusive**, as in Python; integers or not |
| `linspace(a, b, n)` | `n` points including both ends |
| `count(list)` | length |
| `xs[i]` | zero-based; out of range is a runtime "no value" (§4.8) |
| `first(xs)`, `last(xs)` | |
| `cumsum(xs)`, `diffs(xs)` | `diffs` has one fewer element |
| `zip(xs, ys)` | `list<vec>` from two `list<num>` |
| `xs_of(points)`, `ys_of(points)` | the inverse |
| `sort(xs)`, `reverse(xs)` | |
| `interp(x, xs, ys)` | piecewise-linear over a literal table |
| `histogram(xs, lo, hi, bins)` | record `{edges, counts, density}` |

There are no nested comprehensions over more than two variables; write a
let. Length limits are in §19.3.

#### 4.6.6 Calculus

All take a **user function** name as their first argument.

| Function | Method | Notes |
|---|---|---|
| `derivative(f, x)` | central difference, step `h = 1e-5·max(1, abs(x))` | |
| `derivative2(f, x)` | second central difference | |
| `integral(f, a, b)` | adaptive Simpson, tolerance `1e-9` relative, max depth 30 | `a > b` gives the negative |
| `roots(f, a, b)` | scan 512 subintervals for sign changes, refine each with Brent to `1e-12` | returns `list<num>`, sorted; tangential roots without a sign change are not found, and the docs say so |
| `argmin(f, a, b)`, `argmax(f, a, b)` | scan 512, refine with golden section | returns the `x` |
| `newton_step(f, x)` | `x - f(x)/derivative(f, x)` | convenience for teaching |

#### 4.6.7 Matrices

`mat(row, row, ...)`, `M[i][j]`, `M * v`, `M * N`, `M ** k`, `transpose(M)`,
`det(M)`, `inverse(M)` (singular → runtime "no value"), `trace(M)`,
`identity(n)`, `rows(M)`, `cols(M)`, `col(M, j)`, `row(M, i)`.

`eig(M)` for 2×2 only in 2.0. It returns
`{real: bool, values: list<num>, vectors: list<vec>, re: num, im: num}`:
when the eigenvalues are complex, `real` is false, `values` and `vectors`
are empty, and `re ± i·im` are the pair. Larger eigenproblems are out of
scope.

#### 4.6.8 Complex numbers

A `vec` is also a complex number `x + iy`. Arithmetic `+ -` already works.

`cmul(z, w)`, `cdiv(z, w)`, `conj(z)`, `cabs(z)` (= `norm`), `carg(z)`
(= `angle`), `cexp(z)`, `cpow(z, n)` (integer `n`), `cpolar(r, theta)`.

#### 4.6.9 Probability and randomness

| Function | Meaning |
|---|---|
| `rand(seed, i)` | uniform in `[0, 1)`, counter-based (§11.6) |
| `randn(seed, i)` | standard normal |
| `samples_normal(mu, sigma, n, seed)` | `list<num>` |
| `samples_uniform(a, b, n, seed)` | `list<num>` |
| `normal_pdf(x, mu, sigma)`, `normal_cdf(x, mu, sigma)` | |
| `binom_pmf(k, n, p)`, `poisson_pmf(k, lambda)` | |
| `choose(n, k)`, `factorial(n)` | `n ≤ 170` |

#### 4.6.10 Colour

`rgb(r, g, b)`, `rgba(r, g, b, a)` (components 0–1), `hsl(h, s, l)` (`h` in
degrees), `palette("hot")` (§9.1), `mix(a, b, t)` (OKLab),
`with_alpha(c, a)`, `colour_scale(name, v, lo, hi)` (§9.3).

#### 4.6.11 Text

`format(x, spec)` with a Python format spec (`".2f"`, `"+.3g"`, `".1%"`,
`"e"`). Text marks and readouts use interpolation instead (§10.2); `format`
exists for building strings in lets.

### 4.7 Type checking

- Every formula is checked when the document is loaded. A type error is a
  load error (§18), with the formula, the span, the type found, the type
  expected, and — when the fix is mechanical — the fix (`"vec * vec": did
  you mean dot(a, b)?`).
- A value's type is fixed by its declaration. A let's type is inferred.
- A field that expects a type gets that type or an error: `at` is `vec`,
  `r` is `num`, `when` is `bool`, `stroke` is `colour`, `text` is `str`.

### 4.8 Runtime numeric failures

Some failures depend on the state: `1/x` at `x = 0`, `sqrt(-1)`,
`inverse` of a singular matrix, an index out of range. These evaluate to
**no value** (internally NaN, or an absent value for non-numeric types).

| Where no value appears | What happens |
|---|---|
| A mark's geometry | the mark (or the one repeat instance) is skipped this frame |
| A point of a function plot | the polyline breaks there (§8.6) |
| A style property | the theme default is used |
| A readout or `show` | prints `—` |
| A `when` condition | treated as `false` |
| A value assignment from a drag | the assignment is ignored; the value keeps its previous setting |

The figure is never blank because of a runtime numeric failure. In
development mode (§15.11) each skipped mark is reported once per state
change to the console, with its document path.

---

## 5. Values and state

### 5.1 Types

```
number   { n: f64 }              a slider, a typed number, a dragged coordinate
choice   { option: String }      one of a few named options
point    { x: f64, y: f64 }      a position dragged in the plane
flag     { on: bool }            on or off
```

In formulas they have the types `num`, `str`, `vec` and `bool` respectively.

### 5.2 Declaration

```yaml
values:
  - name: x
    type: number
    range: [-1, 5]
    default: 1
    step: 0.01          # optional; omitted = continuous
    scale: linear       # linear | log
    label: "$x$"        # TeX allowed between $…$; defaults to the name
    unit: "s"           # optional, printed after the number
    format: ".2f"       # optional; default chosen from step and range
    control: slider     # optional; see §12.1
  - name: mode
    type: choice
    options: [free, damped, driven]
    labels: { free: "free", damped: "damped", driven: "driven" }   # optional
    default: damped
  - name: p
    type: point
    range: [[-4, -4], [4, 4]]
    default: [2.2, 0.9]
  - name: show_energy
    type: flag
    default: true
```

Field rules:

| Field | Types | Required | Rules |
|---|---|---|---|
| `name` | all | yes | identifier, unique in the figure, not reserved |
| `type` | all | yes | `number`, `choice`, `point`, `flag` |
| `range` | number, point | yes | `low < high` per axis; for `log`, `low > 0` |
| `default` | all | yes | inside the range / among the options |
| `step` | number | no | `> 0`, or the string `"integer"` (= 1, and the value is an integer); omitted means continuous |
| `scale` | number | no | `log` changes the control and the step semantics (§5.4) |
| `options` | choice | yes | 2–12 strings, unique, each an identifier-like token |
| `labels` | choice | no | display text per option |
| `label`, `unit`, `format` | all / number / number | no | |
| `control` | all | no | §12.1 |
| `play` | number | no | §12.6 |
| `shared` | all | no | §5.6 |

Range and default are **literal** in 2.0. A range that follows another value
(a slider whose maximum is another slider) is out of scope; derive a clamped
quantity in a let instead.

### 5.3 Admission: clamp, snap, refuse

Setting a value goes through its domain:

- **Number**: clamp into the range, then snap to the step grid anchored at
  `low` (`low + round((n - low)/step)·step`), then clamp again.
- **Point**: clamp each coordinate. Points have no step in 2.0.
- **Choice**: an option not in the list is **refused**; the error lists the
  options.
- **Flag**: accepts booleans only.
- **A value of the wrong type** is refused.

`set` reports what happened, so both 1.0 rules are honoured — the pointer is
clamped quietly, a program learns that it was:

```js
fig.set("x", 99)             // → { applied: 5, clamped: true }
fig.set("mode", "spinning")  // throws: "spinning" is not one of: free, damped, driven
```

A continuous number MUST NOT be coarse: an omitted step resolves to
`(high - low) / 1000` for keyboard and slider granularity, and drags are
not snapped at all. (1.0 used a hundredth; the existing guard test stays and
tightens.)

### 5.4 Logarithmic numbers

With `scale: log`:

- The slider position is linear in `ln(n)`.
- `step` (if given) is a **multiplicative** factor > 1: `step: 1.1` moves in
  10% increments. `"integer"` is allowed and snaps to integers after the
  logarithmic mapping (for counts like the `n` of a Riemann sum).
- Keyboard arrows move one step; shift+arrow moves to the next decade mark.
- Tick marks on the slider fall on 1, 2, 5 × 10ᵏ.

### 5.5 Reserved state

Each figure also has read-only state the engine maintains and formulas may
read:

| Name | Type | Meaning |
|---|---|---|
| `focus` | `str` | the name of the focused mark or group, or `""` (§12.5) |
| `theme` | `str` | `"light"` or `"dark"` |

They are reserved names (§4.1). They take part in dirty tracking like
values.

### 5.6 Page state and shared values

A page MAY declare values at page level (§15.5, §16.4). A figure reads one
only if it **imports** it:

```yaml
uses: [t, omega]
```

- An imported value behaves as if declared in the figure, except that its
  declaration (range, control) belongs to the page and the page draws one
  control for it, not one per figure.
- Every figure that imports a page value re-renders when it changes. Two
  figures that import `t` show the same instant.
- A figure value with the same name as a page value it does **not** import
  is fine (they are different values); one it **does** import is an error.

A figure value MAY instead be marked `shared: true`, which promotes it to
the page under the name `<figure-id>.<name>` so that other figures can
`uses: [fig1.x]` (and read it in formulas as `fig1_x`; the importing
figure MAY rename it: `uses: [{ name: fig1.x, as: x1 }]`). This is how two
figures stay linked without inventing a page-level declaration.

### 5.7 Defaults, reset, and URL state

- Every value has a default. **Reset** (§12.7) restores the defaults of one
  figure, or of the page.
- A page MAY opt into URL persistence (`persist: url`, §15.5): the current
  non-default values are written to the URL fragment
  (`#hapto:fig1.x=3,fig1.mode=damped`) with `history.replaceState`, so a
  reader can share a configuration. Reading such a fragment on load applies
  it. Default: off.

---

## 6. The figure document

### 6.1 Encodings

A figure document is a tree of maps, lists, strings, numbers and booleans.
It MAY be written as **JSON** or **YAML 1.2** (core schema; anchors and
aliases allowed, tags not). The two are equivalent; the published JSON
Schema (§17) describes both. Inside a book, figures are YAML fenced blocks
(§16.4).

**Where a formula is expected, a literal number, boolean or list of numbers
is also accepted**, and means the constant. `r: 4` and `r: "4"` are the same.

### 6.2 Top level

```yaml
hapto: 2                       # format version, required
id: tangent                    # required inside a book; identifier
title: "The slope at a point"  # optional, used by the page and in errors
caption: null                  # optional; shown under the figure in a book
alt: "A parabola with a point at x = {x:.1f} and its tangent, of slope {slope:.2f}."
size: { aspect: 16/9 }         # §7.6
uses: []                       # page values imported (§5.6)
values: []                     # §5
let: {}                        # §4.5
compute: {}                    # §11
layout: null                   # §7.7, only with several views
views: []                      # §7; or the single-view shorthand below
marks: []                      # §8
readouts: []                   # §13
controls: {}                   # §12.1 layout options
preset: null                   # §14; mutually exclusive with views
```

**Single-view shorthand.** When the figure has one view, the top-level key
`view:` (one view object, without `id`) MAY be used instead of `views:`. A
document MUST NOT mix `view:` with `views:`. With several views, each mark
names its view (§8.1).

**Required:** `hapto`, `alt`, and either a preset or at least one mark.
`id` is required inside a book and on any page with more than one figure.

### 6.3 Strictness

- **Unknown keys are errors**, with a suggestion when a known key is within
  edit distance 2. The format spells `colour`; `color` is accepted as an
  alias everywhere — the one alias in the format, because both spellings
  will be typed.
- **Wrong types are errors** naming the expected type.
- **Missing required keys are errors** naming the key and the object.

### 6.4 Versioning

- `hapto: 2` is the format defined here. The format is frozen at 2.0 release:
  2.x releases MAY add optional keys, functions, marks and presets; they
  MUST NOT change the meaning of a valid document.
- A document **without** `hapto:` whose `kind` is `plot` or `columns` is a
  1.0 `BookFigure` spec and is translated by the `legacy` preset (§14.3),
  so hmu keeps working through the migration.
- Any other document without `hapto:` is refused, naming the key to add.

### 6.5 The smallest useful figure

```yaml
hapto: 2
id: square
alt: "The parabola y = x squared, with a point at x = {x:.1f}."
values:
  - { name: x, type: number, range: [-2, 2], default: 1 }
let:
  "f(t)": "t**2"
view: { x: { range: [-2.5, 2.5] }, y: { range: [-0.5, 4.5] } }
marks:
  - { function: "f(x)", var: x }
  - { point: "[x, f(x)]", drag: { x: "pointer.x" } }
```

Two marks, one value, a draggable point that stays on the curve, axes and
ticks by default, light and dark, keyboard-accessible slider generated.

---

## 7. Views, coordinates and layout

### 7.1 Units

| Quantity | Unit |
|---|---|
| positions, lengths along data (`r` of a circle, `w` of a rect) | **data units** of the view |
| stroke width, dot radius, arrowhead size, text size, offsets, dash pattern, hit radius | **CSS pixels** |
| angles in formulas | radians |
| angle props of marks (`from`, `to` of an arc) | radians (hapto converts to kinemo's degrees, Appendix C) |

This is principle 8. hapto converts CSS pixels to kinemo's scene units at
build time using the current size and device-pixel ratio, so a `2px` line is
2 CSS pixels at any figure size, on any screen.

### 7.2 A view

```yaml
views:
  - id: main
    x: { range: [-2, 6], label: "$x$", ticks: auto, scale: linear }
    y: { range: [-7, 3], label: "$f(x)$" }
    aspect: free        # free | equal
    axes: cross         # cross | box | left-bottom | x | y | none
    grid: false         # false | major | minor
    area: [0, 0]        # cell in the layout grid (§7.7)
    clip: true          # marks are clipped to the plot area
    background: none    # none | panel
    title: null         # optional text above the view
    legend: top-right   # §8.8
```

| Key | Default | Notes |
|---|---|---|
| `id` | `main` | identifier |
| `x.range`, `y.range` | required | **formulas** allowed: a zoom may follow a value |
| `x.label`, `y.label` | none | TeX allowed in `$…$` |
| `x.ticks`, `y.ticks` | `auto` | §7.4 |
| `x.scale`, `y.scale` | `linear` | `linear` or `log` |
| `aspect` | `free` | `equal` keeps one data unit the same length on both axes (§7.5) |
| `axes` | `cross` | `cross`: through zero, pinned to the nearest edge when zero is out of view (1.0 behaviour); `box`: a frame; `left-bottom`: L-shaped at the low edges; `x` / `y`: only one axis (number line); `none` |
| `grid` | `false` | |
| `coords` | `cartesian` | `polar` (§7.8) |
| `clip` | `true` | marks outside the plot area are cut at its edge; `false` lets labels overhang |
| `padding` | auto | CSS px around the plot area for tick labels and axis labels; computed from them when omitted |

### 7.3 Mapping

The **plot area** is the view's layout cell minus its padding. Data
coordinates map linearly (or logarithmically) onto it, y upwards. Marks
reference a view by `view: id`; in a single-view figure the key is omitted.

hapto, not kinemo, owns the mapping: every mark's geometry is computed in
data units, mapped to CSS pixels in the plot area, and emitted to kinemo in
scene units. hapto sets kinemo's `config.frame_w` to 16 and `frame_h` to
`16 × height / width`, so the frame has the canvas's aspect and fills it with
no letterbox (confirmed in M0). Conversions, with `W×H` the canvas in device
pixels:

| hapto | kinemo prop | formula |
|---|---|---|
| position (scene units) | `x`, `y`, points | `units = px × 16 / W` |
| stroke width, dash (CSS px) | `stroke_width`, `dash` | `value = css_px × dpr × 1080 / H` |
| text size (CSS px) | `size` | `value = css_px × dpr × 16 / W` |

### 7.4 Ticks

| `ticks` value | Meaning |
|---|---|
| `auto` | "nice" steps of 1, 2 or 5 × 10ᵏ, chosen so that ticks are about 80 CSS px apart (60 on phones); labels formatted with the fewest digits that distinguish them |
| a number | that step, anchored at 0 |
| a list | exactly those positions |
| `{ at: [...], labels: [...] }` | positions with their own labels (TeX allowed) |
| `pi` | multiples of π/2 (or π/4 when there is room), labelled `$\frac{\pi}{2}$`, `$\pi$`… |
| `none` | no ticks |

On a `log` axis, `auto` places ticks at decades with minor ticks at 2 and 5.

Tick labels MUST NOT overlap: when they would, every other label is dropped,
repeatedly, until they fit.

### 7.5 Equal aspect

With `aspect: equal`, the requested ranges are **minimums**. The range on
the axis with more pixels per unit is widened symmetrically about its centre
until both axes have the same pixels per unit. A circle is drawn as a
circle at every figure size. Rotation, geometry, phase portraits, complex
plane and vector figures SHOULD use it; presets that need it set it.

### 7.6 Figure size

```yaml
size: { aspect: 16/9, min_height: 240, max_height: 560, phone_aspect: 4/3 }
```

- Width is the container's CSS width. Height is `width / aspect`, clamped to
  `[min_height, max_height]` CSS px.
- `aspect` is a number or a ratio string (`"1/1"`, `"4/3"`, `16/9`).
  Default `16/9`.
- `phone_aspect` applies when the width is below 480 CSS px. Default: the
  same as `aspect`, except for multi-view figures that stack (§7.7), whose
  height is the sum of the stacked views at their own aspects.
- The page reserves this box before the figure loads (CSS `aspect-ratio`),
  so loading causes no layout shift.

### 7.7 Several views

```yaml
layout: { columns: 2, rows: 1, phone: stack, gap: 16 }
views:
  - { id: circle, area: [0, 0], aspect: equal, ... }
  - { id: wave,   area: [1, 0], ... }
```

- `area: [column, row]` or `area: [column, row, column_span, row_span]`.
- Column widths are equal unless `widths: [2, 1]` gives relative weights;
  `heights` likewise.
- `phone: stack` (default) puts views in reading order (row-major) one under
  the other below 480 CSS px; `phone: keep` keeps the grid.
- Marks may **connect views**: a `connector` mark (§8.9) draws a dashed line
  from a point in one view to a point in another, which is how the unit
  circle and the sine wave are tied together.

### 7.8 Polar views

`coords: polar` interprets mark positions as `[r, theta]`, draws circular
grid lines and radial spokes, and maps `x.range` to `r` and `y.range` to
`theta` (default `[0, tau]`). Function marks in a polar view take `r` as a
function of `theta` (§8.6). Polar views always have equal aspect.

---

## 8. Marks

### 8.1 Common keys

Every mark is a map whose **first key names its kind and carries its main
argument** (`point: "[x, y]"`, `function: "f(x)"`), followed by options.
This keeps the common case one line long. A mark map MUST contain exactly
one kind key.

Keys every mark accepts:

| Key | Type | Default | Meaning |
|---|---|---|---|
| `view` | id | the only view | which view it draws in; required when there are several |
| `name` | identifier | none | makes the mark focusable and addressable from prose (§12.5) |
| `group` | identifier | none | a focus group several marks share |
| `when` | `bool` formula | `true` | drawn only when true |
| `style` | style map (§9) | theme default for the kind | |
| `label` | text map or string (§10.3) | none | a label attached to the mark's anchor |
| `legend` | string | none | adds an entry to the view's legend |
| `z` | number | document order | drawing order; higher on top |
| `repeat` | map (§8.13) | none | draws one instance per element |
| `drag` | map or `false` (§12.4) | none | makes the mark's anchor draggable |
| `highlight` | style map | theme highlight | style when focused |
| `id` | identifier | none | lets other marks refer to this one (braces, connectors) |

Every **property** listed in the tables below is a formula of the stated
type unless marked *literal*.

### 8.2 Points and lines

| Kind | Main argument | Options |
|---|---|---|
| `point` | `vec` position | `r` (px, default 5), `shape` (*literal*: `circle`, `square`, `diamond`, `cross`, `ring`) |
| `segment` | `[vec, vec]` | `arrows` (*literal*: `none`, `end`, `start`, `both`) |
| `arrow` | `[vec, vec]` from, to | `head` (px, default 10), `double` (bool) |
| `vector` | `vec` components | `at` (`vec`, default `[0, 0]`); sugar for an arrow from `at` to `at + v` |
| `line` | `[vec, vec]` two points it passes through | infinite, clipped to the view; alternatively `{through: vec, direction: vec}` or `{through: vec, slope: num}` |
| `ray` | `[vec, vec]` origin, a point it passes through | |
| `hline`, `vline` | `num` | a horizontal / vertical line across the view |
| `tangent` | `num` the `x` at which to touch | `of` (user function name, required), `length` (data units, default: across the view); sugar for `line {through, slope: derivative}` |
| `secant` | `[num, num]` two `x` values | `of` (user function name) |
| `drop` | `vec` point | `to` (*literal*: `x`, `y`, `both`): dashed hairlines from the point to the axes, the 1.0 "dropped line" |

### 8.3 Curves and areas

| Kind | Main argument | Options |
|---|---|---|
| `polyline` | `list<vec>` | `closed` (bool) |
| `polygon` | `list<vec>` | filled by default |
| `circle` | `vec` centre | `r` (data units, required) |
| `ellipse` | `vec` centre | `rx`, `ry`, `angle` |
| `arc` | `vec` centre | `r`, `from`, `to` (radians, counter-clockwise) |
| `sector` | `vec` centre | `r`, `inner` (default 0), `from`, `to` |
| `angle` | `vec` vertex | `from` (`vec`, a point on the first arm), `to` (`vec`, on the second arm), `r` (px, default 22), `right` (*literal*: `auto`, `always`, `never` — `auto` draws a square marker when the angle is within 0.5° of 90°), `reflex` (bool, default false) |
| `path` | *literal* SVG path data, in data units | `closed` |
| `bezier` | `list<vec>` control points (3 or 4) | |
| `brace` | `[vec, vec]` ends | `side` (*literal*: `left`, `right` relative to the direction from first to second end), `depth` (px) |
| `spring` | `[vec, vec]` ends | `coils` (default 8), `width` (px, default 12) |
| `area` | see below | |

`area` fills a region:

```yaml
- area: { under: "f(x)", var: x, from: a, to: b }        # between the curve and y = 0
- area: { between: ["f(x)", "g(x)"], var: x, from: a, to: b }
  signed: true                                            # parts above and below styled apart
```

With `signed: true` the parts where the first curve is above the second
(or above zero) use `palette("positive")` and the others
`palette("negative")`, unless the style says otherwise, and the negative
part is also hatched (§20.1). This is how an integral's sign is taught.
Sampling follows §8.6. A filled shape given by explicit corners is a
`polygon`.

### 8.4 Rectangles and boxes

| Kind | Main argument | Options |
|---|---|---|
| `rect` | `vec` lower-left corner | `w`, `h` (data units), `radius` (px); or `rect: { from: vec, to: vec }` with two corners |
| `box` | `vec` centre | `w`, `h` (data units), `text` (§10), `radius` (px, default 6), `padding` (px). A box with text MAY omit `w`/`h` and size to its text. |

`box` is the building block for block diagrams and port diagrams.

### 8.5 Text

`text` and `math` marks are specified in §10.

### 8.6 Function plots

```yaml
- function: "f(x)"          # y as a formula of the mark's variable
  var: x                    # default x
  domain: [a, b]            # formulas; default: the view's x range
  samples: auto             # auto | integer
- function: { x: "cos(3*t)", y: "sin(2*t)" }   # parametric
  var: t
  domain: [0, tau]
- function: "1 + cos(theta)"  # in a polar view: r(theta)
  var: theta
```

**Sampling MUST be adaptive**: start from 128 evenly spaced samples (or the
given integer); subdivide an interval when the midpoint deviates from the
chord by more than 0.5 CSS px, up to depth 10 and 4 096 points total.

**Discontinuities**: the polyline breaks where a sample is no value (§4.8),
and where two consecutive samples jump by more than the view's height while
the midpoint lies outside the view (the `tan` case). Vertical asymptotes
are never joined.

**Clipping**: a curve leaving the plot area is cut exactly at the edge, as
kinemo's own `plot` kind does (Appendix C); hapto reproduces it for its
polylines.

The function's **variable shadows** a value of the same name (§4.4): with a
slider `x`, `function: "x**2"` is the parabola, and `point: "[x, x**2]"`
(not a function mark) is the point at the slider's `x`.

### 8.7 Data-like marks

| Kind | Main argument | Options |
|---|---|---|
| `scatter` | `list<vec>` | `r` (px, formula over `i` and `p` per point, default 3.5), `colour` (formula over `i`, `p`) |
| `bars` | `list<num>` heights | `at` (`list<num>` positions, default `0..n-1`), `width` (data units, default 0.8 of spacing), `base` (default 0) |
| `histogram` | `list<num>` samples | `bins` (default 20), `range` (default: min–max), `density` (bool: normalise so the area is 1) |
| `stems` | `list<vec>` | lollipops from y = 0, for discrete distributions and sequences |
| `trace` | a compute node id (§11) | `x`, `y` (field names), `fade` (bool: older part lighter), `colour_by` (`t` or a field name) |

Per-point formulas in `scatter` read the bound names `i` (index) and `p`
(the point).

### 8.8 Legends

A view draws a legend when at least two of its marks have `legend:`. The
view's `legend` key places it: `top-left`, `top-right` (default),
`bottom-left`, `bottom-right`, `outside-right`, `outside-bottom`, `none`.
On phones, `outside-*` legends move below the view.

### 8.9 Connectors

```yaml
- connector: { from: { view: circle, at: "polar(1, theta)" },
               to:   { view: wave,   at: "[theta, sin(theta)]" } }
  style: { dash: dashed }
```

A segment whose ends are in different views, drawn above both.

### 8.10 Graphs

```yaml
- graph:
    nodes:
      - { id: a, at: [-2, 0], label: "$x_1$" }
      - { id: b, at: [ 2, 0], label: "$x_2$" }
      - { id: c, at: [ 0, "h"], label: "$x_3$" }
    edges:
      - { from: a, to: b, width: "abs(xa - xb)*4", label: "$R_{12}$", name: R12 }
      - { from: b, to: c, directed: true }
    node_r: 14          # px
    layout: explicit    # explicit | circle | line | grid
```

- With `layout: explicit` every node needs `at`. With `circle`, `line` or
  `grid`, nodes are placed deterministically in declaration order and `at`
  is forbidden.
- `nodes` and `edges` MAY carry `repeat` (§8.13) for families of graphs.
- Edges attach at the node boundary, not its centre. A directed edge has an
  arrowhead at `to`. Two edges between the same nodes in opposite
  directions are drawn as two curves.
- Every node and edge property is a formula; edge `width` in px is how a
  dissipation lesson is drawn.
- Nodes and edges with `name` are focusable; nodes MAY have `drag`.

### 8.11 Fields

All field marks evaluate a formula over the view. Their variables default to
`x`, `y` and are renamed with `over: [u, v]` when the figure has values
called `x` or `y` it needs inside the formula (§4.4).

| Kind | Main argument | Options |
|---|---|---|
| `heatmap` | `num` formula of the field variables | `scale` (§9.3, default `diverging` when the formula takes both signs at the default state, else `sequential`), `range` (`[lo, hi]`, default symmetric about 0 for diverging, else min–max at the default state — **fixed**, not re-fitted every frame, so colours mean the same thing while dragging), `cell` (px, default 4), `zero` (bool: draw the zero contour) |
| `contours` | `num` formula | `levels` (`list<num>` or a count, default 10), `labels` (bool), `highlight_level` (`num`: one level drawn heavier) |
| `vector_field` | `vec` formula | `density` (px between arrows, default 32), `normalize` (bool, default true: same length, colour by magnitude), `scale` (data units per unit when not normalised) |
| `streamlines` | `vec` formula | `seeds` (`list<vec>`) or `count` (default 24, seeded on a jittered grid with a fixed seed), `length` (integration time), `arrows` (bool) |

Implementation rules:

- `heatmap`: sample at cell centres into a grid of RGBA cells (a 720×405
  figure at 4 px cells is 180×101 = 18 180 samples) and paint it as a
  **raster underlay** beneath the vector scene, interpolated bilinearly.
  Not vector paths (§3.3). In the browser the page draws the cell grid with
  `drawImage` and image smoothing into a canvas under the figure's
  transparent canvas, so the GPU interpolates and composites (§15.8); in
  native renders hapto interpolates and composites in Rust. A figure has at
  most one heat map per view; marks drawn with `z` below it are an error.
  The underlay is part of SVG output as an embedded `<image>`.
- `contours`: marching squares on a grid of the same resolution, with
  linear interpolation on cell edges and the saddle cases resolved by the
  cell-centre value; segments joined into polylines; one path per level.
- `vector_field`: one path for all shafts and one for all heads per colour
  bucket.
- `streamlines`: RK4 with a step of 2 CSS px in screen space, stopping at the
  view edge, a no-value, a near-zero velocity, or the length.

### 8.12 Matrices and grids

```yaml
- matrix: "M"                 # a mat, or list<list<num>>
  at: [0, 0]                  # centre, data units
  cell: 36                    # px
  format: ".2f"
  fill_by: value              # none | value: colour each cell by its value
  scale: diverging
  brackets: true              # draw [ ] around it
  row_labels: ["$q$", "$p$"]
  col_labels: null
  name_cells: "M"             # cells are focusable as M[0][1]
```

The `motor` figure is three of these. `name_cells` makes each cell
addressable from prose (`[the coupling term]{mark M[0][1]}`).

### 8.13 Repeat

Any mark MAY be repeated:

```yaml
- rect: "[a + i*dx, 0]"
  w: dx
  h: "f(a + i*dx + dx/2)"
  repeat: { i: "range(0, n)" }

- arrow: "[p, p + F(p)]"
  repeat: { p: "grid_pts" }
```

- `repeat` binds one or two variables, each to a list. Two variables iterate
  as a product (`{i: "range(0, 3)", j: "range(0, 3)"}`).
- The count MAY depend on values; that is the point.
- Each instance's stable key is the mark path plus its indices (§3.3).
- A `name` on a repeated mark gets the index appended (`bar[3]`), and
  `group` stays shared, so prose can focus one bar or all of them.
- The total number of instances in a figure is limited (§19.3); exceeding
  it is an error at check time and a truncation plus console error at
  runtime.

### 8.14 Groups and transforms

```yaml
- group:
    - { rect: "[-1, -0.5]", w: 2, h: 1 }
    - { arrow: "[[0, 0], [1.5, 0]]" }
  translate: "p"
  rotate: "theta"           # radians, about `origin`
  scale: 1                  # num or vec
  origin: [0, 0]
  when: "show_body"
```

A group's children inherit `view` and `when` (a child's `when` is combined
with *and*). Transforms apply to positions; widths and text sizes in px are
not scaled.

---

## 9. Style and theme

### 9.1 Colour tokens

Authors name what a colour **means**. Tokens resolve per theme. Initial
values follow the Okabe–Ito colour-blind-safe palette; they are
recommendations to finalise in M2 with contrast checks (§20).

| Token | Meaning | Light | Dark |
|---|---|---|---|
| `fg` | ink: text, axis labels | `#1c1b19` | `#e8e6e3` |
| `muted` | axes, grid, construction lines | `#8a857f` | `#7d7872` |
| `faint` | minor grid, fills of context | `#e4e1dc` | `#34322f` |
| `bg` | figure background | `#fdfdfc` | `#1a1918` |
| `accent` | what the figure is about | `#a0522d` | `#d98a5f` |
| `hot` | heat, energy, "this one" | `#d55e00` | `#ff8a3d` |
| `cool` | the counterpart of hot | `#0072b2` | `#56b4e9` |
| `positive` | positive values, gains | `#009e73` | `#3ccf9f` |
| `negative` | negative values, losses | `#cc3d3d` | `#ff6b6b` |
| `highlight` | focus from prose or hover | `#e69f00` | `#ffc04d` |
| `c1` … `c8` | categorical series | Okabe–Ito order: `#0072b2 #d55e00 #009e73 #cc79a7 #e69f00 #56b4e9 #f0e442 #000000` | lightened equivalents (black becomes `fg`) |

`accent` and `bg` default to the page's CSS custom properties when present
(§15.7), so a figure matches its book.

A style colour is a token name, a hex literal, or a formula of type
`colour`. A literal hex colour SHOULD be avoided in books; `check` warns
when one has insufficient contrast against `bg` in either theme.

### 9.2 Style keys

| Key | Type | Default | Notes |
|---|---|---|---|
| `stroke` | colour | per kind | |
| `width` | px | per kind | number, or `hair` (1), `line` (2), `heavy` (3.5) |
| `fill` | colour | none, except kinds that fill by default: `polygon`, `area`, `point`, `box`, `bars`, `histogram` | |
| `fill_opacity` | 0–1 | **1 when `fill` is set**, kind default otherwise | kinemo's default of 0 MUST never leak through (Appendix C) |
| `opacity` | 0–1 | 1 | whole mark |
| `dash` | `solid`, `dashed`, `dotted`, or `[on, off, ...]` px | `solid` | |
| `cap`, `join` | `round`, `butt`, `square` / `round`, `miter`, `bevel` | `round` | |
| `halo` | bool | true for text | a background-coloured outline behind text |

Every key accepts a formula. A stroke that thickens with dissipation is
`width: "2 + 6*abs(i_R)"`; a fill that shifts from cool to hot is
`fill: "mix(palette('cool'), palette('hot'), s)"`.

### 9.3 Colour scales

`colour_scale(name, v, lo, hi)` and the `scale:` key of fields and matrices
accept:

| Name | Kind | Endpoints |
|---|---|---|
| `sequential` | perceptually uniform, light → dark in light theme, inverted in dark | viridis-like |
| `diverging` | `cool` → neutral → `hot` | centred on 0 unless `mid:` is given |
| `cyclic` | for phases and angles | |
| `categorical` | `c1`…`c8` by integer | |

Interpolation is in OKLab.

### 9.4 Kind defaults

| Kind | Default style |
|---|---|
| axes, ticks, grid | `stroke: muted, width: hair` |
| `function`, `polyline`, `trace` | `stroke: accent, width: line`; second and later series in a view use `c1`, `c2`… |
| `point` | `fill: accent, stroke: bg, width: 1.5` |
| `arrow`, `vector` | `stroke: fg, width: line`, head filled with the stroke colour |
| `area`, `polygon` | `fill: accent, fill_opacity: 0.18, stroke: none` |
| `box` | `fill: bg, stroke: fg, width: line` |
| `text`, `math` | `fill: fg`, `halo: true` |
| focused (`highlight`) | `stroke: highlight`, width × 1.6; other marks in the same view at opacity 0.35 when the theme's `dim_others` is true (default) |

### 9.5 Themes

The figure is drawn in the page's theme (`theme` reserved state, §5.5).
Changing the theme re-renders. Colours chosen by token change with it;
literal colours do not. Both themes are part of the quality gates (§21).

hapto MUST set kinemo's `config.background` to the theme's `bg` on every
build (or render with a transparent background over the page's). kinemo's
default background is dark, and a light theme's `fg` text vanishes on it —
found by looking at M0's renders.

---

## 10. Text and mathematics

**A figure that cannot label anything cannot teach.** Text is not an
optional extra in 2.0; axes, ticks and labels are on by default.

### 10.1 The text mark

```yaml
- text: "slope = {slope:.2f}"
  at: "[x, f(x)]"
  anchor: bottom-left       # 9 positions: top-left … center … bottom-right, plus baseline-left/-center/-right
  offset: [6, -6]           # px; +x right, +y up
  size: 14                  # px; default 14 (13 on phones)
  weight: regular           # regular | bold
  rotate: 0                 # radians
  max_width: null           # px; wraps at spaces when set
  background: none          # none | box (a filled rounded box behind it)
```

- `text` is a string with **interpolation** (§10.2) and **inline maths**
  between `$…$`.
- `math:` instead of `text:` takes the whole string as TeX:
  `math: "\\frac{\\partial H}{\\partial q}"`.
- **TeX is static** (§10.4): a live number goes in text beside it, as in
  `text: "$\\frac{\\partial H}{\\partial q}$ = {dHdq:.2f}"`.

### 10.2 Interpolation

`{expr}` or `{expr:spec}` inside text, labels, alt text and readout labels
evaluates `expr` (any formula) and formats it with a Python format spec.
Without a spec, numbers use the shortest representation that round-trips at
4 significant digits. `{{` and `}}` are literal braces.

Interpolation happens **outside** `$…$` only. Inside TeX, `{` is TeX
grouping and nothing is evaluated; a formula-looking `{=…}` or `{x:.2f}`
inside `$…$` or a `math:` string is refused (E107), with a hint showing the
text-beside-maths form above.

### 10.3 Labels on marks

`label:` on any mark attaches text at the mark's **label anchor**:

| Kind | Label anchor | Default placement |
|---|---|---|
| point | the point | above-right, 6 px |
| segment, arrow, vector | the end (`at: end`) or the middle (`at: mid`) | perpendicular, on the outer side |
| function, polyline | the last visible point | right |
| area, polygon, box | centroid | centred |
| angle | bisector at `r + 8` px | |
| brace | the brace tip | outside |
| node, edge | centre / midpoint | |

`label: "$J_x$"` is the short form; `label: { text: ..., at: mid, offset: ... }`
the long one.

**Collision avoidance (2.0 minimum):** labels of one view are placed in
`z` order; a label that would overlap an earlier label by more than 30% of
its area tries the other three diagonal placements before giving up. Full
label layout is out of scope.

### 10.4 Mathematics

- **Typeset natively, ahead of time, and baked.** `kinemo-math` typesets
  with typst, which compiles to wasm but takes the bundle from 1.3 MB to
  11.4 MB brotli (M0). So typst never ships to the browser. Every TeX string
  of a figure is typeset when the document is **baked** — by `hapto book
  build`, or by `hapto bake` for a hand-written page (§17) — and the glyph
  layouts travel with the document (`baked` key, written by the tool, never
  by an author). In the browser they are registered with
  `kinemo_math::baked::register`, which is how kinemo's own WASM player
  works. A baked layout stores glyphs as SVG path data, so the maths font
  never reaches the browser.
- **Bake at one size, scale at draw time.** kinemo keys a baked layout by
  `(tex, size, display)`, and hapto's text size in scene units changes with
  the figure's width and device-pixel ratio (§7.3). So hapto MUST always lay
  maths out at `size: 1` and reach the wanted size with the object's
  `scale` prop; otherwise every resize would miss the baked layouts.
  `display` is part of the key too: inline (`$…$` in text) bakes with
  `display: false`, a `math:` mark with `display: true`.
- Hence **TeX in figures is static** (§10.2). A document that reaches the
  browser with TeX that is not baked is refused (E608), naming the formula
  and the command that bakes it — never drawn blank, which is what kinemo
  does silently with an unbaked formula.
- **Supported TeX** (confirmed in M0): anything in mitex's default spec,
  held to ≥ 98% of kinemo's maths corpus — fractions, roots, scripts,
  `\left…\right`, over/under braces, `\mathbf \mathbb \mathcal \text
  \operatorname`, accents `\dot \ddot \hat \vec \bar`, Greek, big operators,
  `\partial \nabla` and the usual relations, and the environments `aligned
  array bmatrix cases pmatrix vmatrix`. An unsupported command is refused by
  name (E606).
- The **same engine** typesets maths in book prose (§16.6), so a symbol in
  the figure and the same symbol in the sentence beside it look identical.

### 10.5 Fonts

- **Text**: DejaVu Sans, embedded by `kinemo-text` (regular, bold, oblique,
  and DejaVu Sans Mono). They are 1.03 MB of the 1.29 MB brotli bundle (M0),
  so hapto MUST subset them to Latin, Latin-1 supplement, Greek, and the
  mathematical operators and arrows blocks, and drop any face the format
  cannot ask for. Target: fonts ≤ 0.5 MB brotli. Subsetting is a change in
  kinemo (or a font override hook in it), and is scheduled in M2.
- **Maths**: New Computer Modern, used by typst when baking. Glyph outlines
  travel in the baked layouts, so the maths font is not in the browser
  bundle at all.
- `hapto-book` ships the text faces as `woff2` and uses them for prose, so
  figure text and page text match. A page outside hapto-book SHOULD load
  them too; the CSS variables in §15.7 make that one line.
- Text sizes have a floor of 11 CSS px after scaling.
- **Text has a per-label cost** (M0: about 30 µs native, ~150 µs projected
  in wasm on the reference laptop). `check` warns (W602) when a figure can
  show more than 60 labels at once.

---

## 11. Computation

Lets (§4.5) cover anything that is one formula. **Compute nodes** cover the
things that need a loop: integrating, iterating, sampling. They run during
evaluation, are cached by dependency like everything else, and expose their
results as records.

```yaml
compute:
  orbit:
    integrate: { ... }
  steps:
    iterate: { ... }
```

### 11.1 `integrate`

```yaml
compute:
  orbit:
    integrate:
      vars:  { q: "p0.x", p: "p0.y" }          # initial values, formulas
      rates: { q: "p", p: "-sin(q) - b*p" }    # dq/dt, dp/dt; may read vars, t, lets, values
      pairs: [[q, p]]                           # position/momentum pairs (enables symplectic)
      t: [0, 30]                                # start and end time
      dt: 0.01
      method: auto
      until: "q < -10"                          # optional stop condition
      invariant:
        formula: "0.5*p**2 - cos(q)"
        expect: decreasing                      # conserved | decreasing | any
        tolerance: 0.01                         # relative unless `absolute: true`
        reason: null                            # required when expect: any
```

Inside `rates`, `until` and `invariant.formula`, the integrated variables
and `t` are bound names and shadow values of the same name (§4.4). In
the example, the point value is called `p0` precisely so that `p` is free
for the momentum.

**Methods:**

| `method` | Scheme | When |
|---|---|---|
| `auto` | `symplectic` if `pairs` is given, else `rk4` | the default |
| `symplectic` | symplectic Euler: `p ← p + dt·ṗ(q, p)`, then `q ← q + dt·q̇(q, p_new)` | Hamiltonian and weakly damped mechanical systems |
| `verlet` | Störmer–Verlet (velocity Verlet), second order | separable systems where each position's rate depends only on momenta; **refused** otherwise, naming the offending dependency (E605) |
| `rk4` | classical fourth-order Runge–Kutta | general first-order systems |
| `midpoint` | explicit midpoint (RK2) | comparison lessons |
| `euler` | explicit Euler | **only when asked for by name**; `check` warns (W601) unless an `invariant` with `expect: any` and a `reason` is present |
| `implicit_euler` | backward Euler, solved by Newton with a numerical Jacobian, at most 8 iterations per step | stiff decay lessons |

**Why `auto` is not explicit Euler**: a hand-built baseline hit the same
defect three times — energy rising from 2.825 to 3.664 over 2 600 steps; a
radius growing from 2.60 to 2.87 in a purely rotational system; a conserved
total rising in 717 of 1 400 steps. A figure whose caption says "the energy
is constant" while the drawing spirals outward is worse than no figure.

**Output record:**

| Field | Type | Meaning |
|---|---|---|
| `t` | `list<num>` | sample times |
| each var (`q`, `p`) | `list<num>` | |
| `n` | `num` | number of samples |
| `at(time)` | record | each var interpolated linearly at `time` (clamped to the run) |
| `last` | record | final state |
| `invariant` | `list<num>` | the invariant along the run, when declared |
| `drift` | `num` | relative change of the invariant, start → end |
| `stopped` | `bool` | true when `until` fired |

`until` locates the crossing by linear interpolation within the last step,
so a projectile's range is not off by a step.

**Invariant checks:**

- `conserved`: `max |I(t) - I(0)| / max(|I(0)|, 1e-12) ≤ tolerance`.
- `decreasing`: `I(t_{k+1}) ≤ I(t_k) + tolerance·|I(0)|` for every `k`.
- `any`: not checked; `reason` is required and appears in `check` output.
- **Where they are checked**: by `hapto check` and `hapto book build` at the
  default state, at every corner of the number values' ranges the compute
  node reads (up to 64 combinations), and at 16 random states (seed 0). A
  violation **fails the build** (E602), naming the state, the drift and the
  method. At runtime the result is shown regardless, and development mode
  logs violations.

### 11.2 Time as a value

To show "where the system is now" along a run, declare a number value for
time and read the record at it:

```yaml
values:
  - { name: s, type: number, range: [0, 30], default: 0, play: { duration: 10 } }
marks:
  - { trace: orbit, x: q, y: p }
  - { point: "[orbit.at(s).q, orbit.at(s).p]" }
```

The play button (§12.6) advances `s`; the engine still never advances time.

### 11.3 `iterate`

```yaml
compute:
  newton:
    iterate:
      vars: { x: "x0" }
      step: { x: "x - f(x)/derivative(f, x)" }
      n: "steps"                 # formula; integer; ≤ limit §19.3
      until: "abs(f(x)) < 1e-12" # optional early stop
```

Output: each var as `list<num>` of length `n + 1` (the initial value
included), `n` (the number actually taken), `last`, `stopped`. This covers
Newton's method, fixed-point and logistic maps, cobweb diagrams, gradient
descent (`vars: {w1: ..., w2: ...}`), and Markov chains on small vectors.

### 11.4 `sample`

For a quantity expensive enough that several marks should share it:

```yaml
compute:
  pdf:
    sample: { formula: "normal_pdf(u, mu, sigma)", var: u, from: -4, to: 4, n: 256 }
```

Output: `xs`, `ys`, `points` (`list<vec>`).

### 11.5 Calculus functions

`derivative`, `integral`, `roots`, `argmin`, `argmax` are library functions
(§4.6.6), not compute nodes, because they are one call each.

### 11.6 Randomness

`rand(seed, i)` is a counter-based generator (SplitMix64 of the pair
`(seed, i)`, mapped to `[0, 1)` with 53 bits). It is a pure function: the
same seed and index give the same number everywhere, forever. To let a
reader draw new samples, declare a value:

```yaml
values:
  - { name: seed, type: number, range: [0, 1000000], default: 1, step: integer, control: reroll }
```

The `reroll` control (§12.1) is a button that sets a new random seed.

### 11.7 Cost

Compute nodes count against the evaluation budget (§19.3). `check` reports
each node's cost at the default state.

---

## 12. Interaction

### 12.1 Controls

The page builds one control per value the figure declares (and one per page
value, once, §15.5), in declaration order, below the figure.

| Value | `control` options | Default |
|---|---|---|
| number | `slider`, `stepper`, `input`, `reroll`, `none` | `slider` (`stepper` when `step: integer` and the range has ≤ 10 values) |
| choice | `buttons`, `dropdown`, `none` | `buttons` with ≤ 5 options, else `dropdown` |
| point | `handle`, `inputs`, `none` | `handle` (dragging in the figure) **plus** visually hidden numeric inputs for keyboard and screen readers |
| flag | `toggle`, `checkbox`, `none` | `toggle` |

`control` may also be a map:
`{ type: slider, marks: [0, 1, 2], mark_labels: ["0", "$\\omega_0$", "2"] }`.

- **A slider shows** its label, its current value formatted, the unit, and
  optional marks.
- **`none` hides the control**, but the value stays settable from prose and
  drag. A value with `control: none` and no drag and no prose binding is a
  `check` warning W301 (the reader can never change it).
- **Layout**: `controls: { columns: auto, position: below, groups: [...] }`.
  `auto` packs sliders two per row above 640 CSS px and one per row below.
  `position: side` puts them right of the figure above 960 CSS px.
  `groups` gives headings: `[{ title: "Initial state", values: [p0] }]`.

### 12.2 Input semantics

- Pointer input on controls is clamped and snapped silently (§5.3).
- All sets triggered in one animation frame are **coalesced** into one
  evaluation and one render.
- Sets are applied in the main thread's state store immediately; the render
  follows asynchronously (§15.8). Controls and prose readouts never wait for
  pixels.

### 12.3 Keyboard

- Sliders are native `<input type="range">` with `aria-valuetext` formatted
  with the unit.
- Arrow keys move one step; Page Up/Down move 10 steps; Home/End go to the
  bounds. On a log slider, shift+arrow moves a decade.
- Draggable marks are focusable in the figure (Tab order: after the
  figure's controls). Arrow keys move the anchor by 1% of the view per
  press (shift: 10%) **through the same `drag` formulas** as a pointer, so a
  point constrained to a curve stays on it.

### 12.4 Dragging

Any mark with an anchor — `point`, the tip of `arrow`/`vector`, the centre
of `circle`/`box`/`sector`, a graph node — MAY be draggable:

```yaml
- point: "[x, f(x)]"
  drag: { x: "pointer.x" }                 # constrained to the curve: only x is set

- vector: "v"
  drag: { v: "pointer - at" }              # the tip follows the pointer (at is the vector's origin)

- point: "polar(1, theta)"
  drag: { theta: "atan2(pointer.y, pointer.x)" }   # constrained to the circle

- point: "p"                               # a point value used directly:
                                           # drag is automatic and relative (no jump on grab)
```

`drag` maps **value names to formulas** evaluated on every pointer move.
Context names available in those formulas:

| Name | Type | Meaning |
|---|---|---|
| `pointer` | `vec` | the pointer, in the mark's view data coordinates |
| `start` | `vec` | the pointer at grab time |
| `anchor0` | `vec` | the mark's anchor at grab time |
| `delta` | `vec` | `pointer - start` |
| `at` | `vec` | for `vector` marks, its origin |

Rules:

- **Automatic drag**: a `point` (or a `vector`'s tip) whose position is
  exactly a point value's name is draggable with `{ p: "anchor0 + delta" }`
  unless `drag: false`.
- The result of each formula goes through the value's admission (§5.3).
- All assignments of one move are applied as **one** set.
- **Hit testing** is done by hapto against the mark's anchor: within
  `max(r + 6, 12)` CSS px for mouse, `22` CSS px for touch (44 px targets).
  Topmost `z` wins. Hit geometry comes from hapto's own evaluated geometry,
  never from pixels.
- Cursor: `grab` on hover, `grabbing` while dragging. A draggable mark shows
  a halo ring on hover and focus.
- A drag captures the pointer; leaving the figure does not drop it. Touch
  drags call `preventDefault` only once the drag has started on a handle,
  so scrolling the page over a figure still works.

### 12.5 Focus: hover linking between marks and prose

A mark with `name` (or `group`) is **focusable**. Focus is state: the
reserved `focus` (§5.5) holds the focused name or `""`.

- **Hovering a focusable mark** in the figure sets `focus` to its name.
  Prose spans bound to it (§15.4) get the class `hapto-lit`.
- **Hovering a prose span** bound with `mark` sets the figure's `focus`.
- A focused mark draws with its `highlight` style; with `dim_others`
  (theme default) other marks of the same view fade to 0.35 opacity.
- Formulas may read `focus` for custom effects
  (`width: "if focus == 'R12' then 6 else 2"`).
- Focus hit testing uses the mark's geometry: within 8 CSS px of a stroke,
  or inside a filled shape.

### 12.6 Play

```yaml
- { name: t, type: number, range: [0, 10], default: 0,
    play: { duration: 8, loop: wrap } }
```

- A play/pause button appears beside the value's slider.
- While playing, the **page** advances the value with
  `requestAnimationFrame` so that the full range takes `duration` seconds;
  `loop` is `once`, `wrap` or `bounce`.
- Touching the slider, dragging, or hovering prose that sets the value
  pauses it.
- There is no `autoplay` (§25): a figure is still until touched.
- Off-screen figures pause.

### 12.7 Reset

A reset button appears when any value of the figure differs from its default.
It restores the defaults (not page-level values; the page has its own reset,
§15.5) and clears focus.

---

## 13. Readouts

Numbers printed with the figure, computed by the same formulas the drawing
reads.

```yaml
readouts:
  - { id: slope, label: "slope", value: "derivative(f, x)", format: "+.2f" }
  - { id: energy, label: "$H$", value: "0.5*(q**2 + p**2)", unit: "J" }
  - { id: regime, label: "regime",
      value: "if zeta < 1 then 'underdamped' else if zeta == 1 then 'critical' else 'overdamped'" }
  - { id: area, label: "area", value: "integral(f, a, b)", format: ".3f", compare: "exact_area" }
```

| Key | Meaning |
|---|---|
| `id` | identifier; how prose shows it (`[..]{show slope}`) |
| `label` | text, TeX allowed |
| `value` | formula of type `num`, `str` or `bool` |
| `format` | Python format spec for numbers |
| `unit` | appended |
| `compare` | optional second formula shown as "≈ value (exact: …)", for approximation lessons |
| `place` | `below` (default), `inside` (top-left of the first view, as a text block), `none` (prose only) |

Readouts update on every state change, before the pixels (§12.2). A readout
whose value is no value prints `—`. (Formulas accept both quote styles for
strings, §4.1, so a formula inside a YAML double-quoted scalar uses single
quotes, as `'underdamped'` above does.)

---

## 14. Presets

### 14.1 What a preset is

A preset is a **named expansion** into the base language. It exists so that
the common figure is one short block, written the same way every time.

- `preset:` and its parameters (as top-level keys) replace `views` in the
  document. Additional `values`, `let`, `compute`, `marks` and `readouts`
  MAY be given and are **appended** to the expansion (marks drawn on top).
- `hapto expand doc.yaml` prints the expansion. An author can paste it back
  and edit freely ("eject"). Presets therefore never do anything the base
  language cannot.
- Each preset documents the names it introduces (values, lets, compute
  ids), so additions can read them.
- Each preset has a page in the reference docs with its parameters, its
  expansion and a rendered image.

### 14.2 The 2.0 presets

| Preset | Parameters (required in bold) | Expands to |
|---|---|---|
| `plot` | **`curve`** (formula over `x`), `variable` (default `x`), `at`, `range`, `x_range`, `y_range`, `tangent`, `area: {from, to, signed}`, `drop` | view with axes, function, draggable point on the curve, optional tangent, area, readouts `value` and `slope` |
| `vectors` | **`columns`** (list of `[formula, formula]`), `labels`, `span` (bool: parallelogram), `det_readout` | equal-aspect view, vectors from origin, draggable tips when a column is a point value, parallelogram, `det` readout |
| `transform2d` | **`matrix`** (2×2 formulas), `grid` (int, default 5), `show: [unit_square, eigenvectors, image_of]` | equal-aspect view, the transformed grid (repeat of lines), unit square → parallelogram (area = det), real eigenvectors |
| `phase` | **`rates`**, `vars` (default `[x, y]`), `start` (point value name, draggable), `t`, `dt`, `pairs`, `method`, `field: arrows\|streamlines\|none`, `levels` (formula for contours), `invariant` | equal-aspect view, field, contours, an `integrate` node named `orbit`, its trace and the start handle |
| `timeseries` | **`compute`** (an integrate node id) or **`series`** (list of `{x, y, label}`), `stack` (bool) | a time axis and one line per series with a legend |
| `riemann` | **`curve`**, **`from`**, **`to`**, `n` (value, default 1–100, log, integer), `rule` (choice value: `left`, `right`, `mid`, `trapezoid`) | function, repeated rectangles or trapezoids, readouts sum, exact integral, error |
| `unit_circle` | `angle` (value name, default `theta`), `show: [sin, cos, tan, wave]` | equal-aspect circle view with a draggable point and projections; with `wave`, a second view and connectors |
| `distribution` | **`pdf`** (formula over `u`) or **`kind`** (`normal`, `uniform`, `binomial`, `poisson`) with parameters as values, `samples` (n value), `seed` | pdf curve, sample histogram (density), mean/σ readouts, reroll |
| `number_line` | **`range`**, `points` (list of formulas with labels), `intervals` | a single-axis view |
| `cobweb` | **`map`** (formula over `x`), `x0`, `n` | `y = map(x)`, `y = x`, iterate node, cobweb path |
| `legacy` | a 1.0 `BookFigure` spec | the 1.0 `plot` and `columns` figures (§14.3) |

Presets are added after 2.0 only when a figure family appears in **three or
more** corpus figures and its expansion is longer than ~15 lines.

### 14.3 1.0 compatibility

The `legacy` preset accepts the 1.0 spec unchanged:

```json
{ "kind": "plot", "variable": "x", "at": 1, "curve": "x**2 - x*(2)",
  "tangent": true, "x_range": [-2, 4], "y_range": [-2.35, 9.35], "knob": [-1, 5] }
```

A document without `hapto:` whose `kind` is `plot` or `columns` is treated as
`{ hapto: 2, preset: legacy, ... }` with a deprecation warning, and `alt` is
generated from the curve. The legacy preset gains labels and ticks but
otherwise draws what 1.0 drew. It is removed in 3.0.

---

## 15. The page runtime

### 15.1 Distribution

| File | Contents |
|---|---|
| `hapto.js` | ES module, the runtime. No dependencies. |
| `hapto.worker.js` | the render worker |
| `hapto.wasm` | `hapto-web` |
| `hapto.css` | control and figure styles, all through CSS custom properties |
| `fonts/*.woff2` | the text and maths faces (§10.5) |

Usage on any page:

```html
<link rel="stylesheet" href="hapto/hapto.css">
<script type="module" src="hapto/hapto.js"></script>

<hapto-figure id="tangent" src="figures/tangent.yaml"></hapto-figure>
<!-- or inline -->
<hapto-figure id="tangent">
  <script type="application/hapto+yaml"> hapto: 2 ... </script>
  <img class="hapto-fallback" src="figures/tangent.svg" alt="…">
</hapto-figure>
```

`hapto.js` finds its siblings by its own URL (`import.meta.url`); no
configuration is required. Loading from `file://` is not supported (browsers
refuse wasm there); `hapto book serve` exists for local reading.

A document that contains TeX must be **baked** before it reaches the browser
(§10.4): `hapto book build` does it for books, and `hapto bake fig.yaml -o
fig.json` for a hand-written page. A document without TeX needs no baking
and may be written by hand inline.

### 15.2 The element

`<hapto-figure>` is a custom element rendering into **light DOM**, so book
CSS can style controls:

```
<hapto-figure id="tangent" class="hapto">
  <div class="hapto-stage" style="aspect-ratio: 16/9">
    <canvas class="hapto-canvas" role="img" aria-label="…"></canvas>
    <img class="hapto-fallback">            (until the first frame, and for print)
  </div>
  <div class="hapto-readouts" aria-live="polite">…</div>
  <div class="hapto-controls">…</div>
  <div class="hapto-error" hidden>…</div>
</hapto-figure>
```

Attributes:

| Attribute | Meaning |
|---|---|
| `id` | figure id; MUST match the document's `id` when both exist |
| `src` | URL of a YAML or JSON document |
| `controls` | `auto` (default), `none`, `side` |
| `readouts` | `auto`, `none` |
| `eager` | render on load instead of when near the viewport |
| `state` | initial values overriding defaults, in prose-binding syntax: `x=3, mode=damped` |
| `data-dpr-cap` | overrides the device-pixel-ratio cap (§15.8) |

### 15.3 Lifecycle

1. **Connected**: reserve the box (aspect ratio from the document or the
   fallback image), show the fallback image.
2. **Near the viewport** (IntersectionObserver, root margin one viewport
   height) or `eager`: fetch and parse the document in the worker, validate,
   build controls, render the first frame, swap the fallback for the canvas.
3. **Interaction**: sets go to the page state store (§15.5), which notifies
   subscribers (this figure, other figures importing the value, prose).
4. **Far from the viewport** (more than three viewport heights away): the
   figure's render surface MAY be released and replaced by the last frame
   as an image, to bound memory on long chapters. State is kept.
5. **Disconnected**: unsubscribe and free.

One wasm module and one worker serve every figure on the page.

### 15.4 Prose bindings

Prose talks to figures through attributes on any inline or block element.
hapto-book generates them from Markdown (§16.5); a hand-written page uses
them directly.

| Attribute | Behaviour |
|---|---|
| `data-hapto-set="tangent.x=3"` | **hover or focus**: preview — the value is set while the pointer is over the element and restored on leave. **click / tap / Enter**: commit — the value stays. Several assignments: `"tangent.x=3, tangent.mode=damped"`. |
| `data-hapto-show="tangent.slope"` | live text: the element's `textContent` is replaced by the formatted readout, let or value, on every change. Optional `data-hapto-format=".3f"`. |
| `data-hapto-mark="laplacian.R12"` | hover / focus focuses the named mark; when the mark is focused from the figure, the element gets `hapto-lit`. |
| `data-hapto-when="damping.underdamped"` | the element is shown only while the boolean let, readout or flag is true (`hidden` otherwise). Block elements included. |

**Reference syntax**: `figure.name`. The figure part MAY be omitted when
exactly one figure on the page declares (or imports) that name; otherwise
the runtime logs an ambiguity error naming the candidates and the binding
does nothing. hapto-book makes ambiguity a **build error**.

**Literal syntax** in `set`: numbers, `"strings"` or bare option names,
`[x, y]` for points, `true`/`false`.

Bound elements get the classes `hapto-set`, `hapto-show`, `hapto-mark`,
`hapto-when` so the book can style them (the default style: a dotted
underline in `accent` for `set` and `mark`, tabular figures for `show`).

Accessibility: elements with `data-hapto-set` or `data-hapto-mark` are
made focusable (`tabindex="0"`, `role="button"`) and given an
`aria-description` stating what they do ("sets x to 3 in the figure
'The slope at a point'").

### 15.5 Page state

The runtime keeps one **state store** per page, on the main thread. It is
authoritative for values; the worker only evaluates and renders.

Page-level values are declared once per page:

```html
<script type="application/hapto+yaml" data-hapto-page>
values:
  - { name: t, type: number, range: [0, 10], default: 0, play: { duration: 8 } }
persist: none          # none | url
</script>
```

- A page-level value gets one control, placed where the page puts
  `<hapto-controls for="t"></hapto-controls>`, or, when absent, above the
  first figure that imports it.
- `persist: url` enables §5.7.
- `<hapto-reset></hapto-reset>` renders a page-level reset button.

### 15.6 JavaScript API

```js
import { hapto } from "./hapto/hapto.js";

const fig = document.getElementById("tangent");   // the element
await fig.ready;                                   // resolves after the first frame
fig.values();      // [{ name, type, domain, label, unit, default, value }, …]
fig.set("x", 3);                     // → { applied: 3, clamped: false }
fig.setMany({ x: 3, mode: "damped" });
fig.get("x");                        // 3
fig.readouts();                      // [{ id, label, value, text }, …]
fig.reset();
fig.focusMark("R12");                // or fig.focusMark(null)
fig.toSVG();                         // Promise<string>: the current state as SVG
fig.toPNG({ scale: 2 });             // Promise<Blob>
fig.addEventListener("hapto:change", e => e.detail /* { name, value, source } */);
fig.addEventListener("hapto:error",  e => e.detail /* error object, §18.1 */);

hapto.page.set("t", 2.5);            // page-level value
hapto.page.reset();
hapto.devMode(true);                 // §15.11
```

`source` in `hapto:change` is one of `control`, `drag`, `prose`, `api`,
`play`, `url`, `reset`. (`focusMark` avoids clashing with the DOM's own
`focus()`.)

### 15.7 Styling hooks

`hapto.css` uses only custom properties a book may override:

```
--hapto-fg  --hapto-muted  --hapto-bg  --hapto-accent  --hapto-highlight
--hapto-font-text  --hapto-font-math  --hapto-radius  --hapto-control-gap
```

When the page defines `--hapto-accent` or `--hapto-bg`, the figure's
`accent` and `bg` tokens take those values (§9.1). Theme detection reads
`html[data-theme]` first, then `prefers-color-scheme`.

### 15.8 Rendering pipeline

- **Worker**: documents are parsed, evaluated and rendered in
  `hapto.worker.js`. When `OffscreenCanvas` is available, each figure's
  canvas is transferred to the worker and drawn there. Otherwise the worker
  posts an `ImageBitmap` (or RGBA buffer) that the main thread draws. Both
  paths MUST be supported.
- **Latest state wins**: at most one render per figure is in flight. Sets
  that arrive meanwhile replace the pending state; intermediate states are
  never rendered.
- **Device pixel ratio**: the backing store is CSS size ×
  `min(devicePixelRatio, 2)` by default.
- **Progressive quality**: if the last frame took longer than the budget
  (§19.1) while a drag or play is active, subsequent frames render at half
  resolution and are scaled up; 150 ms after the last input, one full
  resolution frame is rendered.
- **Resize**: a ResizeObserver re-lays out on width change, at most one
  render per animation frame. Controls reflow by CSS.
- **Heat-map underlay**: a figure with a `heatmap` gets a second canvas
  stacked **under** its main canvas. The worker posts the cell grid (e.g.
  180×101 RGBA, not full resolution) as an `ImageBitmap`; the page draws it
  with `drawImage` and `imageSmoothingQuality = "high"` over the view's plot
  area, and the main canvas is rendered with a transparent background. The
  GPU interpolates and composites; wasm evaluates cells only (M0: the
  full-resolution upscale in wasm projected to ~27 ms, over budget).
- **Readouts and `show`** are computed in the worker with the frame, but
  **before** rasterising, and posted as soon as evaluation finishes, so text
  leads pixels by the raster time. (A main-thread evaluator mirror was
  considered and rejected: two evaluators could disagree.)

### 15.9 Print and no-JS

- **No JS**: the fallback image (SVG at the default state, emitted by
  hapto-book) is the figure; controls are absent; readouts at default state
  are printed as static text by hapto-book.
- **Print**: on `beforeprint`, every figure that has been touched is
  replaced by an SVG of its **current** state (via `toSVG`), and controls are
  hidden by `@media print`; readouts print. On `afterprint`, canvases
  return.

### 15.10 The wasm surface

`hapto-web` exposes to the worker (not to page authors directly):

```
Figure.load(document_text, format) -> Figure | Error
figure.values()           -> JSON
figure.setState(json)     -> JSON (admitted values, per name)
figure.resize(css_w, css_h, dpr)
figure.evaluate()         -> JSON (readouts, lets referenced by prose, alt)
figure.render()           -> RGBA | draws to an OffscreenCanvas context
figure.underlay()         -> null | { view_rect, cols, rows, rgba } (the heat-map cell grid, §15.8)
figure.renderSVG()        -> string
figure.hitTest(css_x, css_y, pointer_kind) -> JSON { mark, drag: bool, focusable: bool } | null
figure.drag(mark, phase, css_x, css_y) -> JSON (value assignments)
```

The 1.0 `BookFigure` and `Figure` classes are removed; the `legacy` preset
covers their documents.

### 15.11 Development mode

`hapto.devMode(true)`, or `?hapto-dev` in the URL, or `hapto book serve`:

- runtime numeric failures (§4.8) logged once per state with their paths;
- invariant violations at the current state logged;
- render times overlaid in a corner of each figure;
- binding errors shown inline next to the offending prose element.

### 15.12 Errors on the page

A figure that fails to load keeps its fallback image (never a blank box),
and shows a small notice beneath it: "This figure could not be loaded" plus
the error code. The full error object goes to the console and to
`hapto:error`. Readers never see a stack trace.

---

## 16. hapto-book

### 16.1 Why hapto owns the book

hapto was born to put reactive figures in interactive HTML books. Three
things only work if the same tool owns figures **and** prose:

1. **Bindings checked at build time.** A sentence that hovers `x = 3` must
   name a value that exists, in a figure on that page, with 3 inside its
   range. Only the tool that reads both can refuse a broken link before a
   reader finds it.
2. **One maths engine.** A symbol in the figure and the same symbol in the
   paragraph must look the same. The book prose today has no real maths
   typesetting; kinemo-math is already in the binary.
3. **The build as quality gate.** Every figure rendered and checked for ink,
   every invariant checked, every alt text present — on every build of every
   book.

What hapto-book deliberately does **not** own: what to teach, how a lesson
is written, sourcing, prerequisites, review. That is hmu's job (§16.13). The
boundary is the **book source format**: hmu writes it, hapto builds it.

### 16.2 Source layout

```
mybook/
  book.yaml
  chapters/
    01-slope.md
    02-area.md
  figures/            # optional: figures kept in their own files
    tangent.yaml
  assets/             # optional: images referenced from prose
  glossary.md         # optional
```

### 16.3 `book.yaml`

```yaml
hapto-book: 1
title: "Port-Hamiltonian systems, by touching them"
subtitle: null
authors: ["…"]
language: en            # en | pt (UI strings, hyphenation, number format)
id: phs-handbook        # identifies progress in localStorage; stable across builds
chapters:               # order; files relative to chapters/
  - 01-slope.md
  - 02-area.md
  - part: "Dynamics"    # optional part headings
    chapters: [03-phase.md, 04-integrators.md]
theme:
  accent: "#a0522d"     # overrides tokens; anything else is the default theme
  fonts: default        # default | system (system-ui for prose; figures keep theirs)
numbering: { chapters: true, sections: true, figures: true, equations: true }
progress: true          # local progress ticks and resume
tasks: true             # §16.8
base_url: null          # for absolute links and sitemap, optional
```

### 16.4 Chapter source

CommonMark plus GitHub tables, footnotes and strikethrough, with these
extensions — each one has a single spelling:

| Construct | Syntax |
|---|---|
| Front matter | `---` YAML block at the top: `title`, `slug`, `summary`, page `values`, `persist` |
| Inline maths | `$…$` |
| Display maths | `$$…$$`, optionally followed by `{#eq:name}` for numbering |
| Figure | a fenced block ```` ```hapto ```` containing a figure document in YAML (`hapto: 2` may be omitted inside a book), or ```` ```hapto src=figures/tangent.yaml ```` referencing one |
| Prose binding | `[text]{set x=3}`, `[text]{show slope}`, `[text]{mark R12}`, `[text]{when underdamped}` — with an optional figure prefix `tangent.x`; see §16.5 |
| Conditional block | `::: when underdamped` … `:::` |
| Callout | `::: note`, `::: warning`, `::: definition`, `::: theorem`, `::: example` … `:::`, optional title after the kind |
| Task | `::: task` … `:::` (§16.8) |
| Cross-reference | `@fig:tangent`, `@eq:energy`, `@sec:slug`, `@ch:slug` — rendered as "Figure 2.1", "(3)", linked |
| Section anchor | headings get slugs; `## Title {#sec:custom}` overrides |
| Glossary term | `[[port]]` links to the glossary entry `port` |

Raw HTML in chapters is allowed but discouraged; `check` warns (W401).

### 16.5 Bindings in Markdown

```markdown
For [x = 3]{set x=3} the slope is [6]{show slope}, and the
[dissipation]{mark R12} is what makes the [energy fall]{when damped}.
```

- The verb grammar is the runtime's (§15.4): `set` takes assignments,
  `show` a readout, let or value name, `mark` a mark name or group, `when`
  a boolean let, readout or flag. Formulas are not allowed in prose; name
  them in the figure.
- Name resolution, at build time: (1) a page value; (2) the unique figure in
  the **same section** declaring the name; (3) the unique figure in the
  chapter declaring it. Otherwise the build fails (E701/E702), listing the
  candidates and showing how to qualify (`tangent.x`).
- A `set` literal outside the value's range **fails the build** (E703): the
  reader would hover "x = 7" and see 5.
- `show` text in the source (`[6]{show slope}`) is the **no-JS text**; the
  build checks that it matches the readout at the default state, formatted,
  and warns when it does not (W701).
- hmu's automatic mention detection (`mark_mentions`) stays in hmu as a
  source-to-source pass that produces these bindings explicitly.

### 16.6 Maths in prose

- Typeset at build time with kinemo-math (typst, natively) to **inline
  SVG**, with `role="math"` and `aria-label` set to the TeX source (2.0
  minimum). Aligned to the text baseline with `vertical-align` from the
  layout's depth: kinemo-math's `MathLayout` carries `bbox` and `baseline`,
  so depth = `baseline − bbox.y0` (confirmed in M0).
- Identical expressions are rendered once and reused.
- An unsupported TeX command fails the build (E606) naming the command and
  the line.
- **No KaTeX** (decided in M0): kinemo-math's coverage (§10.4) is the
  coverage, for prose and figures alike, so the two cannot drift apart.

### 16.7 Figures in the book

For every figure the build:

1. parses and type-checks it (§4.7);
2. renders it at the default state, light and dark, at 720 CSS px width;
3. **fails if the default render has fewer painted pixels than the
   threshold** (§21.1, E601), naming the figure;
4. runs invariant checks (§11.1);
5. emits `figures/<id>.svg` and `figures/<id>.dark.svg` as fallbacks;
6. numbers it ("Figure 2.1") and renders its caption;
7. emits the `<hapto-figure>` element with the document inlined.

### 16.8 Tasks

A task asks the reader to do something and checks it locally. Grading lives
here, in the book layer, never in hapto-core.

```markdown
::: task {id=flatten}
Move $x$ until the two columns are dependent.

check: columns.dependent
hint: The area of the parallelogram is the determinant.
:::

::: task {id=slope-at-2 kind=numeric}
What is the slope at $x = 2$?

answer: 2
tolerance: 0.01
:::

::: task {id=regime kind=choice}
With $\zeta = 0.3$ the system is…

options: [underdamped, critical, overdamped]
answer: underdamped
:::
```

| Kind | Checked against |
|---|---|
| `figure` (default when `check:` is present) | a **boolean let, readout or flag** of a figure, evaluated live: when it becomes true, the task completes |
| `numeric` | a typed number, `tolerance` absolute unless `relative: true`; the answer field accepts formulas (`**`, no `^`, §4.1) |
| `choice` | one option |

- Tasks are numbered and listed in the chapter's progress.
- Completion is stored in `localStorage` under the book `id`.
- A `figure` task's `check` MUST be false at the default state (otherwise it
  is already solved); the build fails (E706) if it is true.
- Tasks never block reading.

### 16.9 Output

```
_site/
  index.html             cover, table of contents, progress
  01-slope.html
  02-area.html
  glossary.html          when present
  hapto/                 hapto.js, hapto.worker.js, hapto.wasm, hapto.css, fonts/
  figures/               fallback SVGs
  book.css
  book.js                navigation, progress, tasks, theme toggle
```

Every page has: the book title, a table of contents (sidebar on desktop,
drawer on phones) with progress ticks, the chapter, previous/next links, a
theme toggle (light/dark/auto), and a "resume where you stopped" prompt on
the cover. All UI strings come from a strings table per `language`; 2.0 ships
`en` and `pt`.

The output is static files with relative links. It works from any static
host and from `hapto book serve`.

### 16.10 Build validation

`hapto book build` fails on any of:

- a figure that does not parse, type-check, or render with ink (§16.7);
- an invariant violation (§11.1);
- a binding that does not resolve, is ambiguous, or sets an out-of-range
  literal (§16.5);
- a cross-reference to nothing;
- unsupported TeX in prose or figures;
- a figure without `alt`;
- a `figure` task already solved at the default state;
- duplicate ids (figures, equations, sections, tasks).

`hapto book check` runs the same without writing output, and prints
warnings too (raw HTML, literal colours with low contrast, values nobody can
change, `show` text disagreeing with the default readout, explicit Euler
without a reason).

### 16.11 Theme

One theme, light and dark, defined by the tokens of §9.1 plus typographic
settings (measure about 68 characters, line height 1.6, body face chosen by
`fonts`). `book.yaml` may override token values; anything more is out of
scope (§2.2).

### 16.12 Serving

`hapto book serve` builds into a temporary directory, serves it on
`localhost:8750`, rebuilds on change, reloads the open page, and enables
development mode (§15.11). Build errors are shown in the page instead of
the chapter.

### 16.13 Migration of hmu

| Today in hmu | In 2.0 |
|---|---|
| `compiler/book.py` writes HTML, CSS, JS | removed; hmu writes `book.yaml` and `chapters/*.md` |
| `compiler/figure.py` `to_spec` → 1.0 JSON | emits ```` ```hapto ```` blocks in `hapto: 2` (or `legacy` during transition) |
| `compiler/figure.py` `mark_mentions` | stays; emits `[x = 3]{set x=3}` |
| `compiler/quiz.py` | emits `::: task` blocks |
| progress and resume in `book.py` JS | provided by hapto-book |
| `_tidy_math` / `_inline_math` | removed; `$…$` is typeset by hapto-book |
| the author's demo schema (`pipeline/write.py`) | widened to the 2.0 figure language once M6 lands, not before |

---

## 17. Command line

One binary, `hapto`:

| Command | Does |
|---|---|
| `hapto render fig.yaml -o out.png [--set x=3,mode=damped] [--width 720] [--dpr 2] [--theme dark] [--svg]` | render a figure; the agent's way to look at its own output |
| `hapto check fig.yaml...` | parse, type-check, render at default and corner states, count ink, check invariants, report warnings and per-node costs; non-zero exit on errors |
| `hapto expand fig.yaml` | print the document with presets expanded |
| `hapto bake fig.yaml -o fig.json` | typeset every TeX string natively and write the document with its baked layouts (§10.4), for pages that do not go through `hapto book` |
| `hapto explain fig.yaml [--formula /marks/2/at]` | print every formula's resolved names and types |
| `hapto schema` | print the JSON Schema of the figure format (`--book` for `book.yaml`) |
| `hapto errors [CODE]` | print the error catalogue or one entry with examples |
| `hapto book build [dir] [-o _site]` | §16 |
| `hapto book check [dir]` | §16.10 |
| `hapto book serve [dir] [--port 8750]` | §16.12 |
| `hapto bench fig.yaml` | evaluation and render times at the default state and while sweeping each value, native |

Output is human-readable by default and JSON with `--json` (for agents and
CI).

---

## 18. Errors

### 18.1 Shape

```json
{
  "code": "E204",
  "severity": "error",
  "message": "\"vec * vec\" is not defined",
  "path": "/marks/3/at",
  "formula": "p * q",
  "span": [0, 5],
  "hint": "for the dot product write dot(p, q); for complex multiplication cmul(p, q)",
  "options": null,
  "figure": "tangent",
  "source": { "file": "chapters/01-slope.md", "line": 42, "column": 7 }
}
```

- `path` is a JSON Pointer into the figure document.
- `span` indexes into `formula`.
- `source` is present when the document came from a file or a book.
- The human rendering prints the formula with a caret under the span.

### 18.2 Policy

| When | What happens |
|---|---|
| A document error (parse, type, reference, structure) | load fails; `check`/`build` fail; on a page, the fallback stays and a notice appears (§15.12) |
| A runtime numeric failure | degrade locally (§4.8); dev mode logs |
| An over-limit at runtime (§19.3) | the previous frame stays; one console error per figure |
| A refused `set` from the API | throws with the error object |
| A refused prose binding at runtime | the binding does nothing; dev mode shows it inline |

### 18.3 Catalogue

Codes are stable. Every code has a test that triggers it (CONTRIBUTING:
"a new mistake a user can make gets a test").

| Range | Area | Codes |
|---|---|---|
| E1xx | formula syntax | `E101` unexpected token · `E102` `^` used for power · `E103` implicit multiplication · `E104` unclosed bracket · `E105` chained comparison · `E106` non-ASCII operator · `E107` interpolation inside TeX (TeX is static, §10.2) |
| E2xx | types | `E201` type mismatch · `E202` unknown function · `E203` wrong argument count · `E204` operator not defined for these types · `E205` `if` branches differ · `E206` condition not bool · `E207` no such field or method |
| E3xx | names and scope | `E301` unknown name (with nearest names) · `E302` reserved word used as a name · `E303` illegal shadowing · `E304` cycle in lets · `E305` recursion · `E306` duplicate name |
| E4xx | document structure | `E401` unknown key (with suggestion) · `E402` missing required key · `E403` wrong value type · `E404` unknown mark kind · `E405` unknown preset · `E406` mixing `view` and `views` · `E407` missing `hapto:` version · `E408` a mark with zero or several kind keys · `E409` mark references an unknown view |
| E5xx | values and domains | `E501` default outside range · `E502` empty or inverted range · `E503` log range not positive · `E504` option not among options · `E505` drag assigns a value that does not exist |
| E6xx | rendering and computation | `E601` figure renders without ink · `E602` invariant violated · `E603` repeat over limit · `E604` evaluation over budget · `E605` `verlet` on a non-separable system · `E606` unsupported TeX command · `E607` integrator step count over limit · `E608` TeX not baked (run `hapto bake`) · `E609` a mark drawn below a heat map |
| E7xx | book | `E701` binding does not resolve · `E702` ambiguous binding · `E703` `set` literal out of range · `E704` cross-reference to nothing · `E705` duplicate id · `E706` figure task solved at default · `E707` missing alt text |
| Wxxx | warnings | `W101` float equality · `W301` value nobody can change · `W401` raw HTML · `W402` low-contrast literal colour · `W601` explicit Euler without reason · `W602` more than 60 labels on screen · `W701` `show` text disagrees with readout · `W801` legacy 1.0 document |

---

## 19. Performance and limits

### 19.1 Budgets

Reference devices: **laptop** = Apple M1 / recent Intel i5 class, current
Chrome, Firefox and Safari; **phone** = Pixel 6a / iPhone 12 class.

| Measure | Laptop | Phone |
|---|---|---|
| Interaction frame (evaluate + build + render + present), p95 across the corpus | ≤ 16 ms at 720×405 CSS px, DPR 2 | ≤ 33 ms at 360×203 CSS px, DPR 2 |
| Heavy figures (fields at full resolution), p95 | ≤ 33 ms, progressive below that (§15.8) | ≤ 66 ms, progressive |
| First interactive frame after a figure enters the viewport, wasm cached | ≤ 150 ms | ≤ 400 ms |
| Readout and prose `show` update after a set | before the raster of the same frame | same |
| `hapto.wasm` transfer size, brotli | ≤ 2.0 MB; M0 measured 1.29 MB, of which fonts are 1.03 MB; target after subsetting ≤ 0.8 MB | |
| Memory per live figure | ≤ 24 MB | |

**What M0 measured** (`docs/m0.md`; native, Apple M4 Pro, desktop size):
a typical figure rebuilds and renders in 0.8 ms, 200 labels with 100 new
strings per frame in 7.6 ms, eight TeX formulas in 2.5 ms, a heat map as an
underlay in 5.2 ms. Raster is 75–95% of every frame. wasm (under Node) was
3.5× native at desktop size. Projected onto the reference laptop (×3.5 wasm,
×1.5 slower machine), a typical figure takes ~4–5 ms; text-heavy figures and
heat maps are the ones that need the label warning (W602) and the
compositor underlay (§15.8). The browser benchmark of M3 replaces these
projections.

### 19.2 Measurement

`hapto bench` (native) runs in CI on every corpus figure; a regression of
more than 20% in p95 fails CI. A headless-browser benchmark of the demo
book runs nightly and records wasm timings.

### 19.3 Limits

Enforced in evaluation; exceeding one at check time is an error, at runtime
the previous frame stays (§18.2).

| Limit | Value |
|---|---|
| Mark instances after repeat, per figure | 5 000 |
| Comprehension / list length | 100 000 |
| `integrate` steps per node | 200 000 |
| `iterate` steps per node | 100 000 |
| Heat map / contour samples | 300 000 (cells coarsen automatically before this) |
| Function plot points per mark | 4 096 |
| Formula nesting depth | 64 |
| Evaluation budget per state change | 50 million operations (fuel counted by the evaluator) |
| Views per figure | 6 |
| Values per figure | 24 |
| Figures per page | no limit; memory bounded by §15.3 step 4 |

---

## 20. Accessibility

### 20.1 Requirements

- **Every figure has `alt`**, interpolated with the current state, exposed
  as the canvas `aria-label`. Updates are throttled to one per second while
  dragging, and only the readouts region is `aria-live`.
- **Every value is reachable by keyboard**: through its control, even when
  dragging is the primary gesture (point values get hidden numeric inputs,
  §12.1).
- **Prose bindings are focusable** and announce what they do (§15.4).
- **Colour is never the only carrier** of meaning in the default styles:
  signed areas also differ by hatching; categorical series also differ in
  dash when there are more than three in a view.
- **Contrast**: theme tokens meet WCAG 2.2 AA (4.5:1 for text, 3:1 for
  strokes and UI) against `bg` in both themes.
- **Reduced motion**: nothing moves unless the reader presses play.
- **Touch targets** for handles and controls are at least 44×44 CSS px.
- Prose maths has an accessible name (§16.6).

### 20.2 Checks

- `check` computes contrast of every literal colour (W402).
- The demo book is audited with axe-core in CI; violations of level A or AA
  fail.

---

## 21. Quality gates and testing

### 21.1 Ink

Every mark kind, every preset and every corpus figure has a test that
**counts painted pixels** (pixels differing from the background by more than
8 levels in any channel) and asserts:

- the default render has at least 0.5% of its pixels painted (threshold
  tunable per figure with a reason);
- moving each value to its range ends changes the painted pixels (a value
  that changes nothing is a defect).

Tests that compare one frame to another pass when both are blank; they are
never sufficient on their own.

### 21.2 Look at it

A new mark kind, preset or corpus figure is reviewed with rendered PNGs at
default state, in light and dark, at desktop and phone widths.
`hapto render` makes this one command.

### 21.3 Golden images

Each corpus figure has goldens at its default state and at two documented
states, light and dark, desktop width. Comparison: per-pixel difference
above 16 levels on more than 0.2% of pixels fails. Goldens are regenerated
only by an explicit command, and diff images are attached to CI failures.

### 21.4 Numerics

- Every stdlib function has unit tests including edge cases (zero, negative,
  huge, no value).
- Every integrator is tested on the harmonic oscillator: `symplectic` and
  `verlet` keep energy within 1% over 10 000 steps at `dt = 0.01`; `rk4`
  within 0.1%; `euler` is tested to **drift** (so the comparison lesson stays
  honest).
- `roots`, `integral`, `argmin` are tested against closed forms.

### 21.5 Platforms

Corpus figures are rendered in CI natively; the demo book is loaded in
headless Chromium, Firefox and WebKit at 1280 px and 390 px widths, light
and dark, and each figure's first frame is checked for ink.

### 21.6 Errors

Every error code (§18.3) has at least one test triggering it and asserting
the code, the path and that the message contains the hint.

### 21.7 CI

`cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace`
on Linux, macOS and Windows (as today), plus: `hapto check` on the corpus,
golden comparison, `hapto bench` regression, `hapto book check` on the demo
book, and the browser matrix (nightly).

---

## 22. Acceptance corpus

2.0 is done when **every figure below is written as a hapto document, with
no hand-written drawing code, passes `hapto check`, and appears in a demo
book that passes `hapto book check`** (`examples/book/`, committed in this
repository with its goldens).

The corpus has two parts. Part A proves the original target. Part B proves
generality: it covers domains the original course never touched.

### 22.1 Part A — the port-Hamiltonian handbook (12)

From `phx/docs/_archive/handbook/demos.js`. The documents are written into
`examples/corpus/a-*.yaml`, so the acceptance suite does not depend on
another repository.

| Figure | What it must show | Exercises |
|---|---|---|
| `a-rotation` | a vector, its rotation by J, the right angle between them, labels | point value drag, `angle` with right marker, equal aspect, labels |
| `a-skew` | a quadratic form over the plane is zero everywhere; mode buttons | `heatmap` with `zero`, choice value, drag |
| `a-coenergy` | area under a curve = energy, height = voltage; two curves to switch between | `area`, choice, `drop`, readouts |
| `a-phase` | level curves, a path that closes, two perpendicular arrows | `phase` preset, `contours`, `integrate` symplectic, invariant `conserved` |
| `a-ports` | a box, three labelled ports, arrows sized by power, a conservation readout | `box`, arrows with `width` formulas, readouts |
| `a-atoms` | a direction field and a path, blending two behaviours | `vector_field`, `integrate`, a value blending rates |
| `a-domains` | a rectangle whose area is power, four unit systems | `rect`, choice of 4, labelled ticks |
| `a-motor` | three matrices as grids, three sliders | `matrix` ×3, `fill_by: value`, named cells |
| `a-passive` | three traces, two rising while their sum falls | `timeseries`, invariant `decreasing` |
| `a-caps` | two charges redistributing, a conserved total, a log slider | `integrate`, log value, `timeseries` |
| `a-laplacian` | nodes, edges whose thickness is the dissipation, buttons | `graph`, `width` formulas, choice, focus from prose |
| `a-integrators` | three methods against the true orbit, one step-size slider | three `integrate` nodes with `method`, `expect: any` with reason, legend |

### 22.2 Part B — general (20)

| Figure | Domain | What touching it teaches | Exercises |
|---|---|---|---|
| `b-tangent` | calculus | the slope is the limit of secants | drag along curve, `secant` with `h` value, readouts (Appendix A.1) |
| `b-riemann` | calculus | sums converge to the integral, from both sides | `riemann` preset, log integer value, choice, `compare` readout |
| `b-taylor` | calculus | a polynomial hugs a function near a point and leaves it far away | comprehension `sum`, integer value, two curves, `area` between as error band |
| `b-epsilon-delta` | analysis | for every ε there is a δ | bands (`rect` with opacity), lets, conditional text |
| `b-transform` | linear algebra | a matrix moves the whole plane; det is area; eigenvectors stay on their line | `transform2d` preset, four values, `eig` |
| `b-projection` | linear algebra | the residual is perpendicular | two point values, `proj`, `angle` right marker |
| `b-unit-circle` | trigonometry | sine is the height of a turning point | `unit_circle` preset with `wave`, two views, connector |
| `b-inscribed-angle` | geometry | the inscribed angle is half the central angle, wherever the point is | points constrained to a circle, `angle` marks, readouts |
| `b-normal-samples` | probability | samples approach the curve as n grows | `distribution` preset, reroll, histogram density |
| `b-clt` | statistics | means of anything become normal | comprehension of sample means, histogram, choice of source distribution |
| `b-regression` | statistics | least squares minimises the area of the squares | five point values, lets for slope/intercept, `rect` per residual via repeat |
| `b-fourier` | signals | a square wave from sines; the overshoot does not go away | `sum` comprehension, integer log value, overshoot readout |
| `b-phasor` | signals | adding sinusoids is adding rotating vectors | two views, `t` value with play, `vector`, connector, page value shared with `b-unit-circle` |
| `b-damped-oscillator` | physics | three regimes from one parameter | two views (x(t), phase), `integrate`, `when` prose, regime readout |
| `b-projectile` | physics | 45° maximises range only without drag | drag a launch vector, `integrate` with `until`, flag for air drag |
| `b-pendulum` | physics | the small-angle approximation breaks at large amplitude | `phase` preset, energy contours, streamlines, comparison curve (Appendix A.2) |
| `b-field-lines` | electromagnetism | field lines start and end on charges | two draggable charges, `streamlines`, `heatmap` of potential, flag |
| `b-newton` | numerical methods | Newton converges fast — or not at all | `iterate`, tangent steps via repeat, drag `x0` |
| `b-gradient-descent` | optimisation | the learning rate decides between crawling and diverging | `contours`, `iterate` in 2D, log value, draggable start |
| `b-complex-multiply` | complex numbers | multiplying adds angles and multiplies lengths | `cmul`, two point values, `angle` marks, equal aspect |

### 22.3 The demo book

`examples/book/` assembles all 32 figures into chapters grouped by domain,
with prose that uses every binding verb at least once, at least one task of
each kind, page-level shared values in at least one chapter (`b-phasor` and
`b-unit-circle` sharing `theta`), cross-references to figures, equations and
sections, and both languages' UI strings exercised by a second `book.yaml`
(`language: pt`).

---

## 23. Milestones

Each milestone leaves the library more useful than it found it, and has
exit criteria that can be checked.

### M0 — Measure and verify (done, 2026-10-31)

- Prototype the rebuild model: build a scene of the 1.0 parabola plus 200
  text labels from literal props on every frame; measure build, layout,
  raster at the §19.1 sizes, native and wasm.
- Confirm every **(verify)** in this document (collected in Appendix C.5).
- Measure `hapto.wasm` brotli size; list the largest contributors.
- **Exit**: `docs/m0.md` with the numbers, and decisions on (a) reconciler
  needed or not, (b) Canvas2D backend needed or not, (c) prose maths engine.
- **Outcome**: (a) no reconciler; (b) no Canvas2D backend for now; (c)
  kinemo-math natively at build time, baked, for figures and prose, no
  KaTeX. Also: heat maps as a raster underlay, fonts to be subset, the theme
  sets kinemo's background. Every change is folded into this document.
- **Not done in M0**: timing in a real browser (Node stood in for it) and
  on the reference phone. Both move to M3's browser benchmark.

M2 gains three items from M0: a `typeset` feature on hapto-core (on for the
CLI, off for hapto-web) with baking (§10.4, `hapto bake`); font subsetting
(§10.5); and the theme background (§9.5).

### M1 — `hapto-expr`

- Lexer, parser (Appendix B), type checker, evaluator with fuel and dirty
  tracking, the full standard library (§4.6), errors E1xx–E3xx.
- **Exit**: every stdlib function and error code tested; a property test
  that printing and re-parsing a formula round-trips.

### M2 — The figure language, single view

- Document model and validation (§6), values of all four types (§5), lets,
  views with axes/ticks/labels (§7, except polar and multi-view), marks of
  §8.2–§8.6 and §8.14, text and maths (§10), style and theme (§9),
  readouts (§13), `hapto render/check/explain/schema/errors`.
- **Exit**: `b-tangent`, `b-projection`, `b-inscribed-angle`,
  `b-complex-multiply`, `a-rotation`, `a-domains` pass §21 natively.

### M3 — The page runtime

- `hapto.js`, worker, element, controls (§12.1–§12.3), drag (§12.4), focus
  (§12.5), reset, prose bindings (§15.4), page state (§15.5), lazy loading,
  DPR, progressive quality, fallbacks, print, dev mode, `hapto-web` surface
  (§15.10), the `legacy` preset.
- hmu switches its book to `hapto.js` (still writing HTML itself).
- **Exit**: the M2 figures run in the browser matrix within §19.1; a
  hand-written HTML page using only §15 attributes works; hmu's existing
  book works unchanged through `legacy`.

### M4 — Computation

- `integrate`, `iterate`, `sample`, randomness, invariant checks (§11);
  marks `trace`, `scatter`, `bars`, `histogram`, `stems` (§8.7); play
  (§12.6); `repeat` (§8.13).
- **Exit**: `a-passive`, `a-caps`, `a-integrators`, `b-damped-oscillator`,
  `b-projectile`, `b-newton`, `b-normal-samples`, `b-clt`, `b-fourier`,
  `b-riemann` (without preset), `b-taylor`, `b-regression`.

### M5 — Fields and structures

- `heatmap` as a raster underlay, with the browser compositing it (§8.11,
  §15.8); `contours`, `vector_field`, `streamlines` under the batching
  rule; `matrix` (§8.12); `graph` (§8.10).
- **Exit**: `a-phase`, `a-skew`, `a-atoms`, `a-motor`, `a-laplacian`,
  `a-ports`, `a-coenergy`, `b-field-lines`, `b-gradient-descent`,
  `b-pendulum` (without preset), `b-epsilon-delta`; heavy figures meet the
  heavy budget.

### M6 — Several views and presets

- Multi-view layout, phone stacking, connectors, polar views (§7.7, §7.8);
  all presets (§14) with `hapto expand`.
- **Exit**: `b-unit-circle`, `b-phasor`, `b-transform`, and every corpus
  figure that has a preset form rewritten with it, goldens identical to the
  expanded form.

### M7 — hapto-book

- §16 entire: source format, prose maths, bindings with build-time
  validation, tasks, progress, navigation, strings `en`/`pt`, `serve`.
- hmu emits book source instead of HTML (§16.13).
- **Exit**: the demo book (§22.3) builds and passes `book check`; an
  hmu-generated book builds with hapto-book.

### M8 — Hardening and release

- Accessibility audit (§20), browser matrix, performance budgets on the
  reference phone, error catalogue complete, reference docs for every key,
  mark, preset, function and error (generated from the code where
  possible), format frozen at `hapto: 2`.
- **Exit**: §22 complete. Tag 2.0.

---

## 24. Decisions that change 1.0 and `why.md`

| Topic | Before | 2.0 | Why |
|---|---|---|---|
| Figure model | six closed kinds | a language; kinds become presets | three acceptance figures combined two kinds, which the 1.0 format could not express |
| Scene update | write state into a fixed scene's signals | rebuild the scene from state (patching allowed) | removes the list limitation: moving curves, polygons, dynamic counts |
| Expression evaluation | kinemo's `Expr` and evaluator | `hapto-expr`, native, literal props to kinemo | rebuild needs evaluation on the host; formulas are strings, not traced Python. `why.md`'s "Why Rust is worth it here" still holds — the evaluator is Rust — but it is hapto's, and small |
| kinemo dependency | IR, eval, layout, render | layout, text, maths, render (IR and eval remain transitive) | hapto emits literal scenes |
| Units | scene units, stroke widths guessed | data units for meaning, CSS px for ink | the "invisible stroke" defect cannot recur |
| Slider granularity | step `None` = range/100 | range/1000 for keys and sliders, unsnapped drags | smoother; the guard test stays |
| Clamping | 1.0 spec: clamp; CONTRIBUTING: refuse | clamp for input, report to programs (`clamped: true`), refuse choices | resolves the contradiction |
| Repository scope | "hapto does not know about lessons" (CONTRIBUTING) | `hapto-core` does not; `hapto-book` does, including tasks | the book is where bindings can be validated and maths unified (§16.1) |
| How a figure reaches the page (`why.md` open question 2) | open | a custom element in light DOM, plus attributes on prose | |
| Authoring surface (`why.md` open question 3) | a Python sketch | YAML/JSON documents with a JSON Schema, presets, `check`/`explain` | written by agents and compilers; one way to do each thing |
| Does a figure animate (`why.md` open question 4) | open | no; the page may play a value; simulations integrate at rebuild | the engine stays timeless |
| `kinemo-eval` split (`why.md` open question 1) | open | moot: hapto no longer evaluates through it | |

`CONTRIBUTING.md`, `why.md` and `README.md` MUST be updated when this
specification is accepted, and `SPEC-1.0.md` marked as superseded.

---

## 25. Non-goals

Stated so nobody builds them by accident:

- **A timeline** in the engine, or `autoplay`. A figure is still until
  touched.
- **3D**.
- **Algorithm visualisation with discrete program state** (§2.2).
- **Automatic graph layout** beyond `circle`, `line`, `grid`.
- **Constraints between values** (`a + b ≤ 1` enforced by projection). Derive
  a clamped quantity instead.
- **Ranges that follow other values.**
- **Data fetching**, CSV, external images in figures.
- **Reader drawing or annotation.**
- **Accounts, sync, analytics, comments.**
- **A theming engine**, plugin system, or arbitrary JS in documents.
- **Server-side anything** at reading time.
- **A general plotting library.**

---

## 26. Open questions

Each has a default so work is not blocked. Questions 1–5 were answered by
M0 (`docs/m0.md`).

| # | Question | Answer or default |
|---|---|---|
| 1 | Is a full rebuild per frame within budget, or is the reconciler required? | **Answered: rebuild, no reconciler.** Build and layout are 5–25% of a frame; raster dominates. |
| 2 | Does the CPU rasteriser meet §19.1, or do we need a Canvas2D backend that replays kinemo's display list in the browser? | **Answered for now: CPU raster** plus progressive quality and the compositor underlay for heat maps. Reopen only if a corpus figure misses budget in M3's browser benchmark. |
| 3 | Is kinemo-math's TeX coverage enough for book prose? | **Answered: yes**, natively at build time and baked; no typst in the browser (11.4 MB brotli), no KaTeX. TeX in figures is static. |
| 4 | Can kinemo render a non-16:9 frame directly? | **Answered: yes**, by setting `config.frame_h` to match the aspect (§7.3). |
| 5 | Which fonts are embedded, and can they be subset? | **Answered: DejaVu Sans ×3 and Sans Mono**, 1.03 MB of the 1.29 MB bundle. Subset in M2 (§10.5); the mechanism (a change in kinemo-text or a font override hook) is still to choose. |
| 6 | Should page values support `persist: local` (remember a reader's last configuration)? | no in 2.0; `url` only |
| 7 | Search across the book? | not in 2.0; 2.1 |
| 8 | Should diagram-heavy chapters get box/port auto-layout? | no; explicit positions with `repeat` and lets |
| 9 | Do tasks need progressive hints and solutions? | one `hint:` in 2.0; `solution:` reserved |
| 10 | Should a figure be exportable as a standalone HTML snippet (blogs, slides)? | yes via `hapto render --html` referencing `hapto.js` from a configurable base URL; after M8 unless it falls out earlier |

---

## Appendix A — worked examples

### A.1 The slope at a point (base language)

```yaml
hapto: 2
id: tangent
title: "The slope at a point"
alt: "The curve y = x² − 2x with a point at x = {x:.2f}; its tangent has slope {slope:.2f}."
values:
  - { name: x, type: number, range: [-1, 3], default: 2, label: "$x$" }
  - { name: h, type: number, range: [0.01, 2], default: 1, scale: log, label: "$h$" }
let:
  "f(t)": "t**2 - 2*t"
  slope: "derivative(f, x)"
  secant_slope: "(f(x + h) - f(x)) / h"
  flat: "abs(slope) < 0.02"
view:
  x: { range: [-1.5, 3.5], label: "$x$" }
  y: { range: [-1.5, 3.5], label: "$y$" }
marks:
  - { function: "f(x)", var: x, legend: "$f(x) = x^2 - 2x$" }
  - { secant: "[x, x + h]", of: f, style: { stroke: cool, dash: dashed }, legend: "secant" }
  - { tangent: "x", of: f, style: { stroke: hot }, legend: "tangent", name: tangent }
  - { point: "[x + h, f(x + h)]", r: 4, style: { fill: cool }, drag: { h: "pointer.x - x" } }
  - { point: "[x, f(x)]", drag: { x: "pointer.x" }, label: "$P$" }
readouts:
  - { id: slope, label: "tangent slope", value: "slope", format: "+.3f" }
  - { id: secant, label: "secant slope", value: "secant_slope", format: "+.3f" }
```

Note the function mark's `x` (bound, §8.6) and the slider `x` (in the point
and the tangent) coexist by the shadowing rule of §4.4.

### A.2 A damped pendulum's phase portrait (preset plus additions)

```yaml
hapto: 2
id: pendulum
alt: "Phase portrait of a pendulum with damping {b:.2f}, starting at angle {p0.x:.2f}."
values:
  - { name: b, type: number, range: [0, 0.6], default: 0, label: "damping $b$" }
  - { name: p0, type: point, range: [[-3.1, -3], [3.1, 3]], default: [2.2, 0] }
preset: phase
vars: [q, p]
rates: { q: "p", p: "-sin(q) - b*p" }
pairs: [[q, p]]
start: p0
t: 40
dt: 0.01
field: streamlines
levels: "0.5*p**2 - cos(q)"
invariant: { formula: "0.5*p**2 - cos(q)", expect: decreasing, tolerance: 0.005 }
readouts:
  - { id: energy, label: "$H$ at the end",
      value: "0.5*orbit.last.p**2 - cos(orbit.last.q)", format: ".3f" }
```

The preset names its `integrate` node `orbit` (§14.2), which is why the
readout can read it; `hapto expand` shows it.

### A.3 A chapter

````markdown
---
title: "The slope at a point"
slug: slope
---

## Secants become the tangent

Take $f(x) = x^2 - 2x$ and a point $P$ on it. A secant through $P$ and a
second point $h$ to its right has slope $\frac{f(x+h) - f(x)}{h}$. Shrink
[$h$ to $0.01$]{set h=0.01} and the secant all but lies on the
[tangent]{mark tangent}, whose slope here is [+2.000]{show slope}.

```hapto src=figures/tangent.yaml
```

At [the vertex]{set x=1} the tangent is flat: the slope is zero exactly
where the curve turns, as @eq:vertex says.

$$ f'(x) = 2x - 2 = 0 \iff x = 1 $$ {#eq:vertex}

::: task {id=flat}
Drag $P$ until the tangent is horizontal.

check: tangent.flat
:::
````

---

## Appendix B — formula grammar

EBNF. Terminals in quotes. Whitespace between tokens is ignored.

```ebnf
formula      = expr ;
expr         = conditional ;
conditional  = "if" expr "then" expr "else" expr | or_expr ;
or_expr      = and_expr { "or" and_expr } ;
and_expr     = not_expr { "and" not_expr } ;
not_expr     = "not" not_expr | comparison ;
comparison   = additive [ comp_op additive ] ;          (* non-associative *)
comp_op      = "<" | "<=" | ">" | ">=" | "==" | "!=" ;
additive     = term { ( "+" | "-" ) term } ;
term         = unary { ( "*" | "/" | "%" ) unary } ;
unary        = "-" unary | power ;
power        = postfix [ "**" unary ] ;                 (* right-associative *)
postfix      = atom { call | index | field } ;
call         = "(" [ arguments ] ")" ;
arguments    = generator | expr { "," expr } ;
generator    = expr "for" ident "in" expr [ "if" expr ] ;
index        = "[" expr "]" ;
field        = "." ident ;
atom         = number | string | colour | "true" | "false"
             | ident
             | "(" expr ")"
             | "[" [ list_body ] "]" ;
list_body    = expr "for" ident "in" expr [ "if" expr ]   (* comprehension *)
             | expr { "," expr } ;
ident        = letter { letter | digit | "_" } ;
number       = ( digits [ "." digits ] | "." digits ) [ ( "e" | "E" ) [ "+" | "-" ] digits ] ;
string       = '"' { char | '\"' | '\\' } '"'
             | "'" { char | "\'" | '\\' } "'" ;
colour       = "#" hex hex hex [ hex hex hex [ hex hex ] ] ;
```

Notes:

- `generator` is only valid as the sole argument of `sum`, `prod`, `min`,
  `max`, `mean`, `count`.
- `^`, implicit multiplication, and non-ASCII operators are lexed only to
  produce E102, E103 and E106.
- Interpolation in text (`{expr:spec}`, outside `$…$` only) is parsed by the
  text layer, which hands `expr` to this grammar.
- Both quote styles delimit strings, so formulas sit comfortably inside
  YAML and JSON strings of either kind.

---

## Appendix C — what kinemo already provides

Verified at the pinned commit `4cbd012`; the items M0 confirmed are in
C.5, with details in `docs/m0.md` §4. Paths are relative to the kinemo
repository.

### C.1 Object kinds the geometry pass draws

`crates/kinemo-layout/src/geometry.rs`, `Layout::parts`:

| Kind | Props read (defaults) |
|---|---|
| `circle`, `dot` | `r` (1.0; dot 0.08) |
| `ellipse` | `w` (2), `h` (1) |
| `rect` | `w` (2), `h` (1), `radius` (0) — centred on the object's position |
| `polygon`, `polyline` | `points` (a list; literal under hapto's model) |
| `regular_polygon` | `sides`, `r` |
| `line` | `start`, `end` |
| `arrow` | `start`, `end`, `tip` (0.25) |
| `arc` | `r`, `start_angle`, `angle` — **degrees** |
| `sector` | `r`, `inner`, `start_angle`, `angle` — degrees |
| `path` | `d` (SVG path data), `closed` |
| `image` | `src` (a path), `w`, `h`; bytes registered under any name with `media::register_bitmap` (PNG, JPEG) — hapto uses it only if native heat-map underlays move into the scene |
| `brace` | target-based |
| `text`, `glyphs` | glyph sources `text`, `math` (prop `tex`), `code`; props include `size`, `anchor`, `align`, `rotate`, `scale` (`glyphs.rs`, `math.rs`, `transform.rs`) |
| `plot`, `plot_area` | data `points` segments, `x_range`, `y_range`, `size`, `clip` (`plot.rs`) — cuts curves exactly at the axes box |
| `trail` | |
| mass kinds `points`, `vector_field`, `stream_lines` | `crates/kinemo-layout/src/mass/` — per-point props, `field`, `density`, `seeds`, `seed_count`, `steps`, `step` (RK4), `color_low`, `color_high` |

hapto 1.0 emits six of these. 2.0 uses `path` (batched geometry, curves,
areas, fields), `circle`, `ellipse`, `arc`/`sector` (or paths), `rect`,
`line`, `arrow`, `text`/`math` and `brace`; the mass kinds are optional
optimisations, not dependencies.

### C.2 Style resolution

`crates/kinemo-render/src/frame/style.rs`:

- `fill` and `stroke` colours; **`fill_opacity` defaults to 0.0** and
  **`stroke_width` defaults to 0.0** and is scaled by a stroke scale, not by
  scene units — the two defects hapto already guards against.
- `opacity` multiplies down the parent chain.
- `dash` is read as a pattern scaled like stroke width.
- `z` (`crates/kinemo-render/src/frame/collect.rs`) orders drawing,
  accumulated through parents.

### C.3 Expression IR (for reference; hapto 2.0 does not depend on it)

`crates/kinemo-ir/src/expr.rs`: `Const`, `Sig`, `Time`, `Age`, `Derived`,
`ToWorld`, `Un`, `Bin`, `Where`, `Clamp`, `Mix` (colours in OKLab), `Vec2`,
`Format` (Python format spec), `Concat`, `Interp`, `Spline`, `Table`,
`Noise`, `Point`. Unary ops include `Tan`, `Exp`, `Ln`, `Floor`, `Ceil`,
`Round`, `Len`; binary ops include `Mod`, `Pow`, comparisons, `And`, `Or`,
`Min`, `Max`, `Atan2`. hapto-expr's semantics for overlapping operations
(OKLab mixing, Python format specs) are chosen to match these, so a future
return to kinemo-evaluated props would not change results.

### C.4 Output

`crates/kinemo-render/src/svg.rs`: `render_svg(scene, t, transparent)` and
`display_list_to_svg` — vector output from the same display list as the
raster. hapto uses it for fallbacks, print and `toSVG`.

### C.5 Confirmed in M0

Everything that was marked **(verify)** in the first draft. Details and
line references in `docs/m0.md` §4.

| Item | Finding |
|---|---|
| Non-16:9 frames | `config.frame_w`/`frame_h` set the frame; `FrameSize::scale = width / frame_w`. A matching `frame_h` fills any aspect. |
| Text and maths caching | Both caches are global (4 096 text layouts, 2 048 maths layouts), so a scene rebuilt every frame does not re-shape seen strings. |
| Text object | A bare `text` leaf draws; `fill_opacity` must be 1. |
| Maths object | Needs a `math` group with a `glyphs` child marked `rest`; draws nothing without kinemo-math's `typeset` feature unless baked. |
| TeX coverage, baseline | mitex's default spec, ≥ 98% of kinemo's corpus; `MathLayout` has `bbox` and `baseline`. |
| Fonts | DejaVu Sans (regular, bold, oblique), DejaVu Sans Mono: 1.03 MB brotli. |
| 10 000-segment path | 22 ms raster at 1440×810 native: a stress case, far above the spec's default arrow density. |
| `image` kind | `src` path, `w`, `h`; bytes via `register_bitmap`. |
| Stroke and text scale | Stroke and dash scale by `height / 1080`; text `size` is in scene units. |
| SVG export | Text becomes path outlines, not `<text>`. |
| Background | Dark by default; the theme must set it. |
