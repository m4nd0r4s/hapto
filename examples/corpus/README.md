# The acceptance corpus

SPEC-2.0 §22: hapto 2.0 is done when every figure listed there is a
document here, drawn with no hand-written code, passing `hapto check` and
matching its goldens.

| File | From | Shows |
|---|---|---|
| `a-rotation.yaml` | phx handbook, `demoRotation` | J turns every vector a right angle |
| `a-domains.yaml` | phx handbook, `demoDomains` | effort × flow is power in four domains |
| `b-tangent.yaml` | SPEC-2.0 Appendix A.1 | secants become the tangent |
| `b-projection.yaml` | §22.2 | the residual is perpendicular |
| `b-inscribed-angle.yaml` | §22.2 | the inscribed angle is half the central one |
| `b-complex-multiply.yaml` | §22.2 | angles add, lengths multiply |

Milestones add the rest of the 32 as their marks land.

## Checking

```bash
cargo run -p hapto-cli -- check examples/corpus/*.yaml
cargo test -p hapto-core --features typeset --test corpus
```

The test renders every figure at its default and at the two states
documented in `goldens/states.yaml`, in both themes, and compares each with
`goldens/<id>-<n>-<theme>.png`: a pixel differs above 16 levels, and more
than 0.2% differing pixels fails (§21.3). Differences are written to
`target/golden-diffs/` with a diff image, differing pixels in red.

## Changing a golden

Only on purpose, after looking at the new renders:

```bash
HAPTO_BLESS=1 cargo test -p hapto-core --features typeset --test corpus
```
