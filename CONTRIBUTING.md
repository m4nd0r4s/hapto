# Contributing to hapto

Thanks for helping. Bug reports, figures that confuse you, docs fixes and code are
all welcome. For a larger change, open an issue first so the design can be agreed
before the work.

hapto follows [kinemo](https://github.com/m4nd0r4s/kinemo)'s conventions, because
it shares its crates and its author.

## Set up

Requirements: stable Rust. (Python and `maturin` will join when the binding exists.)

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

CI runs exactly these on Linux, macOS and Windows.

## Conventions

- **English everywhere:** code, comments, docs, commit messages.
- **Descriptive names:** full words, no single-letter variables outside tiny
  comprehensions.
- **Small modules:** split a file before it grows past a few hundred lines.
- **Every error says what to do.** A message names the thing that is wrong and
  the way out of it, the way kinemo's diagnostics do. A new mistake a user can
  make gets a test that triggers it.
- **The public API is typed**, and a type that can be wrong is a type to narrow:
  a value's domain is declared once so a bad set is refused with a reason rather
  than clamped in silence.
- **Commits** are small and focused, with a short message in the imperative
  ("Refuse a choice outside its options").

## What belongs here, and what does not

hapto draws a figure from state. It does not know about lessons, questions,
scoring or progress — those belong to whatever embeds it.

Anything about time belongs to kinemo. If a change starts to need a cursor, an
easing curve or a frame at `t`, it is probably a kinemo change. `docs/why.md`
explains where the line is and why it is there.

## Licensing

hapto is dual-licensed under MIT or Apache-2.0. Unless you state otherwise, any
contribution you submit for inclusion is dual-licensed as above, without any
additional terms.
