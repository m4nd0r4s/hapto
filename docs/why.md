# Why hapto is not a layer on kinemo

The measurements behind this library, taken while trying to build interactive
teaching figures on kinemo and hitting the same wall four times.

## The problem

kinemo's founding sentence is in its own docs:

> The script runs once and produces a timeline. The frame at `t` is a pure
> function of `t`.

Everything follows from that, and it is the right design for an explanatory
video. It is the wrong one for a figure that lives beside a paragraph and
reacts to the reader.

Four symptoms, all the same cause, all met while building a textbook on top
of it:

1. **A playground had to be un-animated.** To let the reader touch a figure
   immediately, every `s.play(k.draw(x))` had to become `s.add(x)` — that is,
   the timeline had to be switched off.

2. **A slider had to be disguised as a question.** Only `s.ask` renders a
   control, so a playground with nothing to grade was emitted as a question
   whose tolerance spanned its whole range.

3. **That disguise broke the slider.** The player derives a slider's step
   from the question's tolerance, so the huge tolerance produced a step of 5
   and a knob with *two positions*. The bug was not in the player; it was in
   forcing a video library to be interactive.

4. **The text cannot drive the figure.** The embed protocol accepts `play`,
   `pause`, `restart` and `goto(question)`. There is no `set(param, value)`,
   so hovering `x = 3` in a sentence cannot move the point on the graph
   beside it. The capability exists in the core — `render_worker.js` calls
   `player.set_signal` — but it is wired only to answering a question.

A library whose mental model is *a film that sometimes asks* cannot easily
become *an object that reacts*.

## What is actually reusable

This is the surprising part, and it is why this should be a sister library
rather than a rewrite. The dependency graph says most of kinemo does not know
what time is:

```
kinemo-text   (997 lines)   no kinemo deps at all
kinemo-math  (2162 lines)   depends only on kinemo-text
kinemo-ir    (1049 lines)   no kinemo deps at all
kinemo-eval  (3020 lines)   depends only on kinemo-ir
kinemo-layout(3356 lines)   ir, eval, text, code, math
kinemo-render(4710 lines)   ir, eval, layout, text, encode
```

Maths typeset without a TeX installation, text shaping, constraint layout and
the whole renderer are time-free in their own right. And inside `kinemo-eval`,
where time does live, it is concentrated: of the evaluator's 1058 lines, only
`timeline.rs` (169) is about time. `ops.rs`, the 265 lines that actually
evaluate expressions, mentions time zero times.

The renderer's entry point makes it explicit:

```rust
pub fn render_frame(scene: &Scene, t: f64, opts: &RenderOptions) -> Image
```

`t` is a parameter, not a mode. A library that never advances it, and instead
rebuilds the scene when a value changes, uses the same machinery.

**So the timeline is a layer, not a foundation.** It can be left out.

## A separate repository, over a git URL

Tested rather than assumed. The crates are `publish = false` and depend on
each other by `path`, which raises a fair doubt about whether an outside
workspace can use them. It can: Cargo rewrites a path dependency inside a git
dependency to point at the same checkout.

```toml
[dependencies]
kinemo-render = { git = "https://github.com/m4nd0r4s/kinemo.git" }
kinemo-layout = { git = "https://github.com/m4nd0r4s/kinemo.git" }
kinemo-math   = { git = "https://github.com/m4nd0r4s/kinemo.git" }
kinemo-eval   = { git = "https://github.com/m4nd0r4s/kinemo.git" }
```

`cargo check` on exactly that: **Finished `dev` profile in 18.17s**, with the
lockfile pinning `kinemo-render v0.21.1 (git+https://github.com/m4nd0r4s/kinemo.git#4cbd012b)`.
`publish = false` only blocks crates.io; a git dependency is unaffected.

So the new library gets its own repository, and takes kinemo as a pinned
dependency. That is better than living in the workspace:

- kinemo's release cadence stays its own, and so does the new one;
- the pin is a commit, so kinemo can move without breaking anything;
- the boundary is explicit, which keeps the timeline from leaking back in.

The cost is that a change needed in a shared crate takes two commits and a pin
bump instead of one commit. That is the right trade at this size.

## The model

Instead of a cursor that advances, a state that changes and a scene that is a
function of it:

```
state:  x = 3           moved by a slider, a drag, a hover, or by the text
scene:  f(state) -> drawing
```

No `play`, no `wait`, no `ask`. One `scene.set("x", 3)` that anything may
call, including a `<span>` in the paragraph beside the figure.

In Python, roughly:

```python
import reactive as r

@r.figure
def slope(f):
    x = f.number("x", 0, 5, default=3)           # a value the reader can move
    ax = f.axes(x=(-1, 5), y=(-1, 10))
    ax.plot(lambda t: t * t)                     # the curve
    ax.point(x, x * x)                           # follows x
    ax.tangent(x, x * x, slope=2 * x)            # follows x
    f.readout("slope", 2 * x)                    # follows x
```

and in the page:

```html
<p>For <span data-set="x=3">x = 3</span> the slope is
   <span data-set="x=3">6</span>.</p>
<figure data-scene="slope"></figure>
```

Hovering the span sets `x`, the figure recomputes natively, and the point and
tangent move. Nothing is a question; nothing is graded. Grading belongs to the
book, which already stores progress.

## Why Rust is worth it here

Not for drawing speed. For **evaluating the expression in the browser**.

`2 * x` has to recompute on every pointer move, at 60 frames per second,
without a round trip. kinemo already solves this: it traces Python lambdas to
native code so they can run in WebAssembly. That tracer (`kinemo-eval`'s
`ops.rs` and `expr.rs`) is exactly what a reactive figure needs, and it is the
piece that would be most painful to rebuild in JavaScript.

The part to leave behind — `timeline.rs`, easing, the cursor — is the small
part.

## What belongs where

| | |
|---|---|
| **kinemo** | explanatory video. Timeline, easing, quiz videos, MP4. Unchanged. |
| **the new repo** | reactive figures. State in, drawing out, DOM events. Depends on kinemo's `text`, `math`, `layout`, `render` and the expression half of `eval`, over a pinned git URL. |
| **hmu** | what to teach. A vocabulary of domain demos (mass-spring, circuit, pendulum), and the glue that lets a sentence drive a figure. |

The third row is ours and genuinely new: it is about teaching, not drawing.

## What the first figure taught

Building it confirmed the reuse and turned up one constraint worth knowing.

**The IR needs no timeline.** Every property of every object is a signal id,
and a signal is an initial value plus a list of keyframes. The parabola scene
has 806 signals and 694 of them sit on an empty timeline: they simply hold
their initial value. So a figure is drawn by writing the state into those
initial values and rendering at `t = 0`. `render_frame`'s `t` is never
advanced.

**A figure has to be built around a free signal.** Binding the state to the
dot's own `x` did nothing, and a test caught it: `place(at=curve.point_at(3.0))`
is a constraint that recomputes `x` from the constant 3.0, discarding whatever
was written there. The value has to be a bare `k.signal(...)` that no object
owns, so everything derived from it recomputes. `Figure::bind_free` finds it
by that absence of an owner.

That works for one knob and does not generalise: kinemo gives a bare signal no
name, so two of them cannot be told apart. It is the first concrete reason for
hapto to build its own scenes rather than import kinemo's, and it belongs in
the authoring API rather than in a workaround here.

## What building the scenes taught

`Build` answered that: every value is declared with a name, so a mass-spring
can carry an `x` and a `k` at once. The demos in `demos.rs` are four figures
a textbook actually wants — a mass on a spring, two columns and the area they
span, a tangent on a curve, a ray and its components.

Three of kinemo's defaults are tuned for a scene its own Python assembled, and
each one produced a *silently blank* picture rather than an error:

1. **`presence` is empty by default and `present` starts at `false`.** An
   object with no toggle is never on screen. All six demos rendered an empty
   frame while the suite stayed green.

2. **`fill_opacity` defaults to `0.0`.** A shape given a fill colour and
   nothing else is painted fully transparent. The dot on the parabola was
   absent for this reason, and only the block survived because it happened to
   set an opacity explicitly.

3. **Stroke width is not in scene units.** The renderer scales it separately,
   so kinemo's own scenes use 1.5 to 4 where positions run -8 to 8. Widths
   chosen on the scale of the coordinates come out invisible. `weight::HAIR`,
   `LINE` and `HEAVY` are those numbers under names.

The lesson is about verification, not about kinemo. A green suite that
compares one frame to another passes two blank frames just as happily, and
every one of these was found by *looking at a PNG*. `tests/shows.rs` now
counts painted pixels and asserts that setting a value changes them; the
`demo` example exists to be looked at.

The remaining gap in the builder is `Expr`: it can build a `Vec2` but not a
list, so a polygon's corners cannot each follow a value. `point_list` returns
an error saying so, and `quad` draws a four-sided shape as four lines whose
endpoints are ordinary bound signals. That is what the parallelogram is.

## The open questions

These need answering before much more code:

1. **Does `kinemo-eval` need splitting?** The clean move is `kinemo-expr` (the
   tracer and the ops, time-free) and `kinemo-timeline` on top. Taking all of
   `kinemo-eval` works today and drags `timeline.rs` along, which is 169
   unused lines — cheap enough to defer the split until it hurts.

2. **How does a figure reach the page?** A custom element like
   `<kinemo-lesson>`, or a canvas the host owns and drives directly? The
   second is more flexible and makes the host responsible for more.

3. **What is the authoring surface?** The Python above is a sketch. It has to
   be writable by an agent — one way to do each thing, errors that carry the
   fix — which is the discipline kinemo already has and should keep.

4. **Does a figure animate at all?** A mass-spring that *oscillates* needs
   time back, as a simulation rather than a script. `k.simulate` already
   exists in kinemo. This is the one place the two libraries genuinely
   overlap, and the one worth thinking hardest about.
