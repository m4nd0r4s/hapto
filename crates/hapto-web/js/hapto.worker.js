// hapto.worker.js — the render worker (SPEC-2.0 §15.8).
//
// It owns the wasm module and every figure on the page; the main thread
// owns the DOM and sends it messages, handled one at a time in order:
//
//   load     { id, text, format, state }  → loaded { meta, values, warnings } | failed
//   canvas   { id, canvas }               the figure's OffscreenCanvas, transferred
//   frame    { id, frame, state, drag, end, width, height, dpr, quality, lets }
//                                         → evaluated { set, refused, readouts, lets, alt }
//                                         → drawn { ms, bitmap | rgba | (on the canvas) }
//   hit      { id, seq, x, y, pointer }   → hit { hit }
//   drag     { id, mark, phase, x, y }    start, end or cancel; moves ride on frames
//   snapshot { id }                       → snapshot { blob }, and the canvas shrinks
//   svg      { id, seq }                  → svg { svg }
//   free     { id }
//
// A frame posts the readouts before it rasterises, so text leads pixels by
// the raster time. Errors come back as { op: "error", id, seq, error }.

import init, { Figure } from "./hapto_wasm.js";

const figures = new Map();
let started = null;

/// Load the wasm module from a URL, or from its bytes (Node, in tests).
export function start(source) {
  started ??= init({ module_or_path: source });
  return started;
}

/// An error as something postMessage can carry.
function plain(e) {
  const out = { message: String(e?.message ?? e) };
  for (const k of ["code", "path", "hint", "options", "errors", "severity", "formula", "span"]) {
    if (e?.[k] !== undefined) out[k] = e[k];
  }
  return out;
}

let queue = Promise.resolve();

/// Handle one message after every earlier one.
export function receive(msg, post) {
  queue = queue
    .then(() => handle(msg, post))
    .catch((e) => post({ op: "error", id: msg.id, seq: msg.seq, of: msg.op, error: plain(e) }));
  return queue;
}

async function handle(msg, post) {
  await started;
  const { op, id } = msg;
  if (op === "load") {
    let figure;
    try {
      figure = Figure.load(msg.text, msg.format);
      if (msg.state && Object.keys(msg.state).length) figure.setState(msg.state);
    } catch (e) {
      figure?.free();
      post({ op: "failed", id, error: plain(e) });
      return;
    }
    figures.set(id, { figure, canvas: null, ctx: null });
    post({ op: "loaded", id, meta: figure.meta(), values: figure.values(), warnings: figure.warnings() });
    return;
  }
  const entry = figures.get(id);
  if (!entry) return; // freed while the message was on its way
  const f = entry.figure;
  switch (op) {
    case "canvas":
      entry.canvas = msg.canvas;
      entry.ctx = msg.canvas.getContext("2d");
      return;
    case "frame":
      return draw(entry, msg, post);
    case "hit":
      post({ op: "hit", id, seq: msg.seq, hit: f.hitTest(msg.x, msg.y, msg.pointer) });
      return;
    case "drag":
      f.drag(msg.mark, msg.phase, msg.x, msg.y);
      return;
    case "snapshot": {
      if (!entry.canvas) return;
      const blob = await entry.canvas.convertToBlob();
      entry.canvas.width = entry.canvas.height = 0;
      post({ op: "snapshot", id, blob });
      return;
    }
    case "svg":
      post({ op: "svg", id, seq: msg.seq, svg: f.renderSVG() });
      return;
    case "free":
      f.free();
      figures.delete(id);
      return;
    default:
      throw new Error(`the worker has no "${op}"`);
  }
}

async function draw(entry, msg, post) {
  const f = entry.figure;
  const { id, frame } = msg;
  const began = performance.now();
  let set = {};
  let refused = null;
  if (msg.state && Object.keys(msg.state).length) {
    try {
      set = f.setState(msg.state);
    } catch (e) {
      // All or nothing: nothing was set, and the frame still draws.
      refused = plain(e);
    }
  }
  if (msg.drag) Object.assign(set, f.drag(msg.drag.mark, "move", msg.drag.x, msg.drag.y));
  if (msg.end) f.drag(msg.end, "end", 0, 0);
  f.resize(msg.width, msg.height, msg.dpr * msg.quality);
  // Text before pixels (§15.8).
  post({ op: "evaluated", id, frame, set, refused, ...f.evaluate(msg.lets ?? []), values: f.values() });
  const rgba = f.render();
  const width = f.pixelWidth;
  const height = f.pixelHeight;
  const done = (extra, transfer) =>
    post(
      { op: "drawn", id, frame, quality: msg.quality, width, height, ms: performance.now() - began, ...extra },
      transfer,
    );
  if (entry.canvas) {
    if (entry.canvas.width !== width) entry.canvas.width = width;
    if (entry.canvas.height !== height) entry.canvas.height = height;
    entry.ctx.putImageData(image(rgba, width), 0, 0);
    done({});
  } else if (!msg.rgba && typeof createImageBitmap === "function") {
    const bitmap = await createImageBitmap(image(rgba, width));
    done({ bitmap }, [bitmap]);
  } else {
    done({ rgba: rgba.buffer }, [rgba.buffer]);
  }
}

function image(rgba, width) {
  return new ImageData(new Uint8ClampedArray(rgba.buffer, rgba.byteOffset, rgba.length), width);
}

// In a worker: start, and listen. Imported elsewhere (Node), do nothing.
if (typeof WorkerGlobalScope !== "undefined" && globalThis instanceof WorkerGlobalScope) {
  start(new URL("hapto.wasm", import.meta.url));
  globalThis.onmessage = (e) =>
    receive(e.data, (m, transfer) => globalThis.postMessage(m, transfer ?? []));
}
