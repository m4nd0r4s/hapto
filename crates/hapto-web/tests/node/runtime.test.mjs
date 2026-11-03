// The page runtime's logic without a browser: the frame scheduler of
// hapto.js and the worker's protocol (SPEC-2.0 §15.8).
//
// HAPTO_RUNTIME names a directory holding hapto.js, hapto.worker.js, the
// web glue (hapto_wasm.js) and hapto.wasm, as scripts/test_web.sh lays it out.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const dir = process.env.HAPTO_RUNTIME;
if (!dir) throw new Error("HAPTO_RUNTIME names the assembled runtime");
const runtime = await import(pathToFileURL(join(dir, "hapto.js")).href);
const worker = await import(pathToFileURL(join(dir, "hapto.worker.js")).href);
await worker.start(readFileSync(join(dir, "hapto.wasm")));

/// A scheduler on a clock and animation frames the test turns by hand.
function scheduler() {
  const clock = { t: 0, frames: [], timers: [] };
  const sent = [];
  const s = new runtime.Scheduler((r) => sent.push(r), {
    now: () => clock.t,
    frame: (f) => clock.frames.push(f),
    later: (f, ms) => clock.timers.push({ at: clock.t + ms, f }),
  });
  const tick = (ms = 16) => {
    clock.t += ms;
    const due = clock.timers.filter((x) => x.at <= clock.t);
    clock.timers = clock.timers.filter((x) => x.at > clock.t);
    due.forEach((x) => x.f());
    const frames = clock.frames.splice(0);
    frames.forEach((f) => f());
  };
  return { s, sent, tick, clock };
}

test("every set of an animation frame goes out as one request", () => {
  const { s, sent, tick } = scheduler();
  for (let i = 0; i < 50; i++) s.set({ x: i });
  s.set({ y: 1 });
  assert.equal(sent.length, 0, "nothing before the frame");
  tick();
  assert.equal(sent.length, 1);
  assert.deepEqual(sent[0].state, { x: 49, y: 1 });
});

test("one frame in flight: a fast drag never queues stale frames", () => {
  const { s, sent, tick } = scheduler();
  s.drag_to("P", 0, 0);
  tick();
  assert.equal(sent.length, 1);
  // The worker is slow: 30 moves over three animation frames.
  for (let i = 1; i <= 30; i++) {
    s.drag_to("P", i, i);
    if (i % 10 === 0) tick();
  }
  assert.equal(sent.length, 1, "nothing more while one frame is in flight");
  s.drawn(40);
  tick();
  assert.equal(sent.length, 2, "then exactly one, with the newest move");
  assert.deepEqual(sent[1].drag, { mark: "P", x: 30, y: 30 });
  s.drawn(40);
  tick();
  assert.equal(sent.length, 2, "and nothing when nothing changed");
});

test("the end of a drag rides after its last move", () => {
  const { s, sent, tick } = scheduler();
  s.drag_to("P", 1, 1);
  s.release("P");
  tick();
  assert.deepEqual([sent[0].drag, sent[0].end], [{ mark: "P", x: 1, y: 1 }, "P"]);
});

test("slow frames under input go half resolution, then settle at full", () => {
  const { s, sent, tick, clock } = scheduler();
  s.drag_to("P", 0, 0);
  tick();
  assert.equal(sent.at(-1).quality, 1);
  s.drawn(30); // over the 16 ms budget
  s.drag_to("P", 1, 0);
  tick();
  assert.equal(sent.at(-1).quality, 0.5);
  s.drawn(9); // a half frame of 9 ms stands for ~36 ms at full
  s.drag_to("P", 2, 0);
  tick();
  assert.equal(sent.at(-1).quality, 0.5);
  s.drawn(9);
  const before = sent.length;
  // The pointer stops: 150 ms later, one full-resolution frame.
  for (let i = 0; i < 6; i++) tick(30);
  assert.equal(sent.length, before + 1);
  assert.equal(sent.at(-1).quality, 1);
  assert.ok(clock.t >= 150);
});

test("a fast figure stays at full resolution", () => {
  const { s, sent, tick } = scheduler();
  for (let i = 0; i < 5; i++) {
    s.drag_to("P", i, 0);
    tick();
    s.drawn(4);
  }
  assert.ok(sent.every((r) => r.quality === 1));
});

const DOC = `hapto: 2
alt: "x = {x:.1f}"
values: [ { name: x, type: number, range: [-2, 2], default: 1 } ]
let: { 'f(t)': 't**2' }
view: { x: { range: [-2, 2] }, y: { range: [-1, 4] } }
marks:
  - { function: 'f(x)', var: x }
  - { point: '[x, f(x)]', drag: { x: 'pointer.x' }, name: P }
readouts: [ { id: y, label: y, value: 'f(x)', format: .2f } ]
`;

/// Send messages to the worker; collect what it posts.
async function talk(messages) {
  const posted = [];
  for (const m of messages) await worker.receive(m, (msg) => posted.push(msg));
  return posted;
}

test("the worker loads, and refuses a bad document with its code", async () => {
  const out = await talk([
    { op: "load", id: 1, text: DOC, format: "yaml", state: { x: 0.5, theme: "dark" } },
    { op: "load", id: 2, text: "hapto: 2\nalt: a\nmarks: []", format: "yaml" },
  ]);
  assert.equal(out[0].op, "loaded");
  assert.equal(out[0].values.values[0].value, 0.5);
  assert.equal(out[1].op, "failed");
  assert.equal(out[1].error.code, "E402");
});

test("a frame posts its text before its pixels", async () => {
  const out = await talk([
    { op: "frame", id: 1, frame: 7, state: { x: 1.5 }, width: 320, height: 180, dpr: 2, quality: 1, rgba: true },
  ]);
  assert.deepEqual(out.map((m) => m.op), ["evaluated", "drawn"]);
  const [evaluated, drawn] = out;
  assert.equal(evaluated.frame, 7);
  assert.deepEqual(evaluated.set, { x: { value: 1.5, clamped: false } });
  assert.equal(evaluated.readouts[0].text, "2.25");
  assert.equal(evaluated.alt, "x = 1.5");
  assert.equal(drawn.frame, 7);
  assert.equal(drawn.rgba.byteLength, 640 * 360 * 4);
  assert.ok(drawn.ms >= 0);
});

test("half quality renders half the pixels", async () => {
  const [, drawn] = await talk([
    { op: "frame", id: 1, frame: 8, state: {}, width: 320, height: 180, dpr: 2, quality: 0.5, rgba: true },
  ]);
  assert.deepEqual([drawn.width, drawn.height, drawn.quality], [320, 180, 0.5]);
});

test("a refused set leaves the state and still draws", async () => {
  const out = await talk([
    { op: "frame", id: 1, frame: 9, state: { x: 0, nope: 1 }, width: 320, height: 180, dpr: 1, quality: 1, rgba: true },
  ]);
  assert.equal(out[0].refused.code, "E301");
  assert.equal(out[0].values.values[0].value, 1.5);
  assert.equal(out[1].op, "drawn");
});

test("drags: start, a move on a frame, the end after it", async () => {
  // Find P at x = 1.5 by asking for hits along its row.
  let at = null;
  for (let y = 0; y < 180 && !at; y += 2) {
    for (let x = 0; x < 320 && !at; x += 2) {
      const [m] = await talk([{ op: "hit", id: 1, seq: 1, x, y, pointer: "mouse" }]);
      if (m.hit?.mark === "P") at = [x, y];
    }
  }
  assert.ok(at, "P can be hit");
  const out = await talk([
    { op: "drag", id: 1, mark: "P", phase: "start", x: at[0], y: at[1] },
    { op: "frame", id: 1, frame: 10, state: {}, drag: { mark: "P", x: at[0] - 400, y: at[1] }, end: "P", width: 320, height: 180, dpr: 1, quality: 1, rgba: true },
    { op: "frame", id: 1, frame: 11, state: {}, drag: { mark: "P", x: 0, y: 0 }, width: 320, height: 180, dpr: 1, quality: 1, rgba: true },
  ]);
  assert.deepEqual(out[0].set, { x: { value: -2, clamped: true } });
  // After the end, a move is an error the page hears about, not a crash.
  assert.equal(out.at(-1).op, "error");
  assert.equal(out.at(-1).error.code, "E400");
});

test("svg, and free", async () => {
  const out = await talk([{ op: "svg", id: 1, seq: 3 }, { op: "free", id: 1 }, { op: "svg", id: 1, seq: 4 }]);
  assert.equal(out.length, 1, "a freed figure says nothing");
  assert.ok(out[0].svg.startsWith("<svg"));
});

test("the prose literal syntax", () => {
  const p = runtime.parse_assignments;
  assert.deepEqual(p('x=3, mode=damped, p=[1, -2.5], on=true, s="a, b"'), [
    ["x", 3], ["mode", "damped"], ["p", [1, -2.5]], ["on", true], ["s", "a, b"],
  ]);
  assert.deepEqual(p("tangent.x = -0.5"), [["tangent.x", -0.5]]);
  for (const bad of ["x", "p=[1]", "2x=1", "x=1 y=2", 's="open'])
    assert.throws(() => p(bad), (e) => e.code === "E403", bad);
});
