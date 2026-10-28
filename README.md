# hapto

**Figures that react.** A drawing that lives beside a paragraph, that the
reader can touch and the text can drive.

From the Greek *háptō*, to touch — the sister of
[kinemo](https://github.com/m4nd0r4s/kinemo), which is *kínēma*, movement.
kinemo makes explanatory video. hapto makes the figure you poke at.

```
state:   x = 3          set by a slider, a drag, a hover, or by the prose
figure:  f(state) -> drawing
```

There is no timeline, no cursor and nothing to wait for. A value changes and
the drawing follows, which is the one thing this library does.

## Why not kinemo

kinemo's own documentation says it plainly: *"the script runs once and
produces a timeline; the frame at `t` is a pure function of `t`."* That is the
right design for a video and the wrong one for a figure that answers to a
reader. Building teaching figures on it meant turning the animation off,
disguising a slider as a question, and discovering that the disguise made the
slider move in steps of five.

But most of kinemo is not about time. Maths typeset without a TeX
installation, text shaping, constraint layout and the renderer all take time
as a *parameter*, not as a mode — `render_frame(scene, t, opts)`. So hapto
takes those crates over a pinned git dependency and leaves the timeline
behind. `docs/why.md` has the measurements.

## State

Early. `hapto-core` holds the state model — named values, their domains, and
the one operation that matters:

```rust
state.set("x", Value::number(3.0))?;
```

Anything may call it, including a `<span>` in the prose. What a value may be
is declared once, so a control can be drawn for it and a bad set refused with
a reason.

The slider's step belongs to the value, not to anything else. (In kinemo it
was derived from a question's answer tolerance, which gave a two-position
slider whenever the tolerance was wide. There is a test here that will not
let that happen again.)

```bash
cargo test
```

## In the browser

```bash
scripts/build_web.sh
```

Then open <http://localhost:8750/>. A parabola with a point on it: drag the
slider and the point follows, and hovering `x = 0` or `x = 4.5` *in the
sentence* moves it too. That second one is the whole reason the library
exists — a figure the prose can drive.

Each redraw takes 2–5 ms, so a pointer never outruns it. Needs the
`wasm32-unknown-unknown` target and `wasm-bindgen` at the pinned version; the
script says so if either is missing.

## Licence

MIT or Apache-2.0, at your option. Same as kinemo.
