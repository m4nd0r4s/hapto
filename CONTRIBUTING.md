# Contributing to hapto

Thanks for helping. Bug reports, figures that confuse you, docs fixes and code are
all welcome. For a larger change, open an issue first so the design can be agreed
before the work.

hapto follows [kinemo](https://github.com/m4nd0r4s/kinemo)'s conventions, because
it shares its crates and its author.

## Set up

Requirements: stable Rust. For the browser build, the `wasm32-unknown-unknown`
target and `wasm-bindgen` at the version `crates/hapto-web/Cargo.toml` pins.

```bash
cargo build
```

kinemo's drawing crates come over a pinned git dependency, so the first build
fetches that repository. The pin is a commit in `Cargo.toml`; moving it is a
deliberate change with its own commit.

## Check your change

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs exactly these on Linux, macOS and Windows. A change to the browser
surface (`crates/hapto-web`) also runs its Node tests, which need Node 20 or
later:

```bash
scripts/test_web.sh
```

## Conventions

- **English everywhere:** code, comments, docs, commit messages.
- **Descriptive names:** full words, no single-letter variables outside tiny
  comprehensions.
- **Small modules:** split a file before it grows past a few hundred lines.
- **Every error says what to do.** A message names the thing that is wrong and
  the way out of it, the way kinemo's diagnostics do. A new mistake a user can
  make gets a test that triggers it.
- **The public API is typed**, and a type that can be wrong is a type to narrow:
  a value's domain is declared once, so a choice outside its options is refused
  with a reason, and a number set by a program is clamped *and told so*
  (`clamped: true`). Only the pointer is clamped quietly.
- **Commits** are small and focused, with a short message in the imperative
  ("Refuse a choice outside its options").

## What belongs here, and what does not

hapto has layers, and each knows less than the one above it
(`docs/SPEC-2.0.md` §3):

- **`hapto-expr` and `hapto-core`** draw a figure from state. They do not know
  about pages, prose, lessons, tasks or progress.
- **The page runtime** (`hapto.js`) wires figures to the prose of any HTML
  page. It does not know about books.
- **`hapto book`** builds whole books: chapters, prose maths, bindings checked
  at build time, tasks graded in the reader's browser, local progress.

What to teach — lessons, sources, prerequisites, review — belongs to whatever
writes the book source (hmu, or a person). hapto builds it; it does not write
it.

Anything about time belongs to kinemo. The engine never advances a clock: if
a change starts to need a cursor, an easing curve or a frame at `t`, it is
probably a kinemo change. A play button is the page setting a value, not the
engine animating. `docs/why.md` explains where the line is and why it is
there.

## Licensing

hapto is dual-licensed under MIT or Apache-2.0. Unless you state otherwise, any
contribution you submit for inclusion is dual-licensed as above, without any
additional terms.
