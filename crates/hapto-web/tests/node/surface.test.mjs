// The 2.0 wasm surface from JavaScript (SPEC-2.0 §15.10): each call, and
// the digits of parity.yaml against the native build's parity.json.
//
// Run with scripts/test_web.sh, which builds the module for Node first.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const pkg = process.env.HAPTO_WEB_PKG;
if (!pkg) throw new Error("HAPTO_WEB_PKG names the wasm-bindgen output for Node");
const { Figure } = createRequire(import.meta.url)(join(pkg, "hapto_web.js"));

const DOC = `hapto: 2
alt: "A parabola with a point at x = {x:.1f}."
values:
  - { name: x, type: number, range: [-2, 2], default: 1, step: 0.5 }
let:
  'f(t)': 't**2'
  slope: '2*x'
view: { x: { range: [-2, 2] }, y: { range: [-1, 4] } }
marks:
  - { function: 'f(x)', var: x }
  - { point: '[x, f(x)]', drag: { x: 'pointer.x' }, name: P }
readouts:
  - { id: s, label: slope, value: slope, format: .2f }
`;

function figure() {
  const f = Figure.load(DOC, "yaml");
  f.resize(400, f.heightFor(400), 2);
  return f;
}

test("load throws an Error with the code and every error", () => {
  assert.throws(
    () => Figure.load("hapto: 2\nalt: a\nview: { x: { range: [0, 1] }, y: { range: [0, 1] } }\nmarks: [ { point: '[q, 0]' } ]", "yaml"),
    (e) => e instanceof Error && e.code === "E301" && e.errors.length >= 1 && e.path.startsWith("/marks/0"),
  );
});

test("values and setState", () => {
  const f = figure();
  const v = f.values();
  assert.equal(v.values[0].name, "x");
  assert.deepEqual(v.values[0].range, [-2, 2]);
  assert.equal(v.at_default, true);
  assert.deepEqual(f.setState({ x: 9 }), { x: { value: 2, clamped: true } });
  assert.equal(f.values().at_default, false);
  assert.throws(() => f.setState({ y: 1 }), (e) => e.code === "E301" && /did you mean/.test(e.hint ?? "x"));
  assert.deepEqual(f.setState({ theme: "dark", focus: "P" }), { theme: "dark", focus: "P" });
  f.reset();
  assert.equal(f.values().focus, "");
});

test("evaluate: readouts, lets, alt", () => {
  const f = figure();
  f.setState({ x: 1.5 });
  const out = f.evaluate(["slope"]);
  assert.equal(out.readouts[0].text, "3.00");
  assert.equal(out.lets.slope, 3);
  assert.equal(out.alt, "A parabola with a point at x = 1.5.");
  assert.deepEqual(f.evaluate().lets, {});
});

test("render, renderSVG, underlay, warnings", () => {
  const f = figure();
  assert.equal(f.pixelWidth, 800);
  const rgba = f.render();
  assert.ok(rgba instanceof Uint8Array);
  assert.equal(rgba.length, f.pixelWidth * f.pixelHeight * 4);
  assert.ok(rgba.some((b, i) => i % 4 !== 3 && b < 128), "something is drawn");
  assert.ok(f.renderSVG().startsWith("<svg"));
  assert.equal(f.underlay(), null);
  assert.deepEqual(f.warnings(), []);
});

test("hitTest and drag keep the point on its curve", () => {
  const f = figure();
  // Find the point by scanning: hit geometry is hapto's, the page only asks.
  let at = null;
  for (let y = 0; y < f.heightFor(400) && !at; y += 2)
    for (let x = 0; x < 400 && !at; x += 2) {
      const hit = f.hitTest(x, y, "mouse");
      if (hit?.mark === "P") at = [x, y];
    }
  assert.ok(at, "the point can be hit");
  assert.deepEqual(f.hitTest(at[0], at[1], "touch"), { mark: "P", drag: true, focusable: true });
  f.drag("P", "start", at[0], at[1]);
  const moved = f.drag("P", "move", at[0] - 40, at[1] - 300);
  assert.ok(moved.x.value < 1);
  const { lets } = f.evaluate(["slope"]);
  assert.equal(lets.slope, 2 * moved.x.value);
  f.drag("P", "end", 0, 0);
  assert.throws(() => f.drag("P", "move", 0, 0), (e) => e.code === "E400");
  assert.throws(() => f.drag("nothing", "start", 0, 0), (e) => e.code === "E505");
});

test("the browser computes the native digits", () => {
  const doc = readFileSync(join(here, "..", "parity.yaml"), "utf8");
  const pinned = readFileSync(join(here, "..", "parity.json"), "utf8").trimEnd().split(/\r?\n/);
  const f = Figure.load(doc, "json-or-yaml");
  const lets = ["trig", "hyper", "exps", "special", "random", "colour", "series"];
  [0.7, -2.25, 2.9].forEach((x, i) => {
    f.setState({ x });
    assert.deepEqual(f.evaluate(lets), JSON.parse(pinned[i]), `x = ${x}`);
  });
});

test("a 1.0 BookFigure spec loads through the legacy preset", () => {
  const spec = { kind: "plot", variable: "x", at: 1, curve: "x**2 - x*(2)", tangent: true,
    x_range: [-2, 4], y_range: [-2.35, 9.35], knob: [-2, 4] };
  const f = Figure.load(JSON.stringify(spec), "json");
  f.resize(480, f.heightFor(480), 1);
  assert.equal(f.render().length, 480 * f.pixelHeight * 4);
  assert.deepEqual(f.setState({ x: 3 }), { x: { value: 3, clamped: false } });
  assert.match(f.evaluate().alt, /x = 3\.00, where its tangent has slope 4\.00/);
  assert.deepEqual(f.warnings().map((w) => w.code), ["W801"]);
});
