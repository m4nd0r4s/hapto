// hapto.js — the page runtime (SPEC-2.0 §15).
//
// <hapto-figure> renders a figure document into light DOM, so a book's CSS
// can style it:
//
//   <hapto-figure id="tangent" src="figures/tangent.json">
//     <img class="hapto-fallback" src="figures/tangent.svg" width="720" height="405" alt="…">
//   </hapto-figure>
//
// Lifecycle (§15.3): the box is reserved and the fallback shown when the
// element connects; the document is fetched, read and drawn once the figure
// comes within a viewport height of the screen (or at once with `eager`);
// a figure more than three viewport heights away gives up its canvas for an
// image of its last frame; disconnecting frees it.
//
// Rendering (§15.8): one worker (hapto.worker.js) owns the wasm module and
// every figure. Each canvas is transferred to it as an OffscreenCanvas when
// the browser can; otherwise the worker posts ImageBitmaps (or RGBA) that
// the page draws. Every set and drag move of an animation frame goes out
// as one frame request, and a figure has at most one frame in flight, so
// only the newest state is ever drawn.

const HERE = new URL(".", import.meta.url);

// ---------------------------------------------------------------- helpers

/// The page's theme: `html[data-theme]` first, then the system's (§15.7).
function page_theme() {
  const set = document.documentElement.dataset.theme;
  if (set === "light" || set === "dark") return set;
  return matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function refused(why) {
  return Object.assign(new Error(`E403: ${why}`), { code: "E403" });
}

/// `x=3, mode=damped, p=[1, 2], on=true, s="a b"`: the prose-binding
/// literal syntax (§15.4), as [name, value] pairs. A name may carry its
/// figure (`tangent.x`), which is kept for the caller to resolve.
export function parse_assignments(text) {
  const out = [];
  let i = 0;
  const fail = (why) => {
    throw refused(`${why} in "${text}"`);
  };
  const space = () => {
    while (i < text.length && /\s/.test(text[i])) i++;
  };
  const literal = () => {
    space();
    if (text[i] === "[") {
      const end = text.indexOf("]", i);
      if (end < 0) fail("a point is missing its ]");
      const parts = text.slice(i + 1, end).split(",").map((p) => Number(p.trim()));
      if (parts.length !== 2 || parts.some((n) => !Number.isFinite(n))) fail("a point is [x, y]");
      i = end + 1;
      return parts;
    }
    if (text[i] === '"') {
      const end = text.indexOf('"', i + 1);
      if (end < 0) fail("a string is missing its closing quote");
      const s = text.slice(i + 1, end);
      i = end + 1;
      return s;
    }
    const start = i;
    while (i < text.length && text[i] !== ",") i++;
    const word = text.slice(start, i).trim();
    if (word === "") fail("a value is missing");
    if (/[\s=]/.test(word)) fail("assignments are separated by commas");
    if (word === "true" || word === "false") return word === "true";
    const n = Number(word);
    return Number.isFinite(n) && /^[-+.\d]/.test(word) ? n : word;
  };
  for (;;) {
    space();
    if (i >= text.length) break;
    const eq = text.indexOf("=", i);
    if (eq < 0) fail("an assignment needs =");
    const name = text.slice(i, eq).trim();
    if (!/^[A-Za-z_]\w*(\.[A-Za-z_]\w*)?$/.test(name)) fail(`"${name}" is not a name`);
    i = eq + 1;
    out.push([name, literal()]);
    space();
    if (text[i] === ",") i++;
    else if (i < text.length) fail("assignments are separated by commas");
  }
  return out;
}

function element(tag, cls, parent) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  parent?.append(e);
  return e;
}

/// A value as the slider beside it prints it.
function shown(v) {
  if (typeof v === "number") {
    return Number.isInteger(v) ? String(v) : v.toFixed(3).replace(/\.?0+$/, "");
  }
  if (Array.isArray(v)) return `[${v.map(shown).join(", ")}]`;
  return String(v);
}

// ---------------------------------------------------------------- frames

/// The interaction budget of §19.1: a frame slower than this while input
/// is active makes the next ones half resolution.
export const BUDGET_MS = 16;
/// How long after the last input the full-resolution frame comes.
export const SETTLE_MS = 150;

/// One figure's frames: coalesces sets and drag moves into the next
/// animation frame, keeps one frame in flight, and picks its quality
/// (§15.8). `send(request)` posts a frame; `drawn(ms)` reports it back.
export class Scheduler {
  constructor(
    send,
    { now = () => performance.now(), frame = (f) => requestAnimationFrame(f), later = (f, ms) => setTimeout(f, ms), budget = BUDGET_MS } = {},
  ) {
    this.send = send;
    this.budget = budget;
    this.now = now;
    this.frame = frame;
    this.later = later;
    this.state = {};
    this.drag = null;
    this.ending = null;
    this.dirty = false;
    this.inflight = false;
    this.scheduled = false;
    this.input = -Infinity; // when the last interactive input came
    this.last_ms = 0; // how long the last frame took
    this.quality = 1; // of the last frame sent
    this.settle = null;
  }

  /// Values to set with the next frame; `interactive` for pointer input.
  set(state, interactive = false) {
    Object.assign(this.state, state);
    if (interactive) this.input = this.now();
    this.request();
  }

  /// A drag move: only the newest one matters.
  drag_to(mark, x, y) {
    this.drag = { mark, x, y };
    this.input = this.now();
    this.request();
  }

  /// Let go of a drag after its last move, in the same frame.
  release(mark) {
    this.ending = mark;
    this.request();
  }

  /// Something other than state changed (size, theme, a canvas back).
  invalidate() {
    this.request();
  }

  request() {
    this.dirty = true;
    if (this.scheduled) return;
    this.scheduled = true;
    this.frame(() => {
      this.scheduled = false;
      this.flush();
    });
  }

  flush() {
    if (this.inflight || !this.dirty) return;
    const active = this.now() - this.input < SETTLE_MS;
    const quality = active && this.last_ms > this.budget ? 0.5 : 1;
    const request = { state: this.state, drag: this.drag, end: this.ending, quality };
    this.state = {};
    this.drag = null;
    this.ending = null;
    this.ending = null;
    this.dirty = false;
    this.inflight = true;
    this.quality = quality;
    this.send(request);
  }

  /// The frame in flight is on screen; send the next if one is waiting,
  /// and come back at full resolution once the input has settled.
  drawn(ms) {
    this.inflight = false;
    // A half-resolution frame says little about a full one.
    this.last_ms = this.quality < 1 ? ms * 4 : ms;
    if (this.dirty) {
      this.request();
      return;
    }
    if (this.quality < 1) {
      clearTimeout(this.settle);
      const wait = Math.max(0, SETTLE_MS - (this.now() - this.input));
      this.settle = this.later(() => {
        this.settle = null;
        if (this.now() - this.input >= SETTLE_MS) this.request();
        else this.drawn(this.last_ms / 4); // more input came; wait again
      }, wait);
    }
  }
}

// ---------------------------------------------------------------- the worker

/// The page's one render worker, and the figures listening to it.
class Engine {
  constructor() {
    this.worker = new Worker(new URL("hapto.worker.js", HERE), { type: "module" });
    this.figures = new Map();
    this.waiting = new Map();
    this.seq = 0;
    this.worker.onmessage = (e) => this.receive(e.data);
    this.worker.onerror = (e) => console.error("hapto: the render worker failed:", e.message ?? e);
  }

  post(msg, transfer) {
    this.worker.postMessage(msg, transfer ?? []);
  }

  /// A message with a reply: resolves with it.
  ask(msg) {
    const seq = ++this.seq;
    return new Promise((resolve, reject) => {
      this.waiting.set(seq, { resolve, reject });
      this.post({ ...msg, seq });
    });
  }

  receive(msg) {
    const waiting = msg.seq !== undefined && this.waiting.get(msg.seq);
    if (waiting) {
      this.waiting.delete(msg.seq);
      if (msg.op === "error") waiting.reject(Object.assign(new Error(msg.error.message), msg.error));
      else waiting.resolve(msg);
      return;
    }
    this.figures.get(msg.id)?._worker(msg);
  }
}

let engine = null;
const ids = { next: 0 };

// ---------------------------------------------------------------- config

/// `?hapto-render=bitmap` (or `hapto.config.render = "bitmap"`) draws with
/// ImageBitmaps even where OffscreenCanvas exists; `rgba` sends raw pixels.
/// Both paths must work (§15.8); this is how they are tested.
const config = {
  /// The frame budget under input; lower it to see progressive quality.
  budget_ms: BUDGET_MS,
  render: (typeof location !== "undefined" && new URLSearchParams(location.search).get("hapto-render")) || "auto",
};

function offscreen() {
  return config.render === "auto" && "transferControlToOffscreen" in HTMLCanvasElement.prototype;
}

// ---------------------------------------------------------------- observers

let observers = null;

function watch() {
  observers ??= {
    // Near: within one viewport height, so the first frame is ready on arrival.
    near: new IntersectionObserver(
      (entries) => entries.forEach((e) => e.isIntersecting && e.target._near()),
      { rootMargin: "100% 0px" },
    ),
    // Far: more than three viewport heights away, the canvas may go (§15.3 step 4).
    far: new IntersectionObserver(
      (entries) => entries.forEach((e) => (e.isIntersecting ? e.target._back() : e.target._away())),
      { rootMargin: "300% 0px" },
    ),
    resized: new ResizeObserver((entries) =>
      entries.forEach((e) => e.target.closest("hapto-figure")?._resize()),
    ),
  };
  return observers;
}

// ---------------------------------------------------------------- the element

const live = new Set();

class HaptoFigure extends (globalThis.HTMLElement ?? class {}) {
  constructor() {
    super();
    this.key = ++ids.next;
    // idle → loading → live ⇄ released, or failed
    this.state = "idle";
    this.syncs = new Map();
    this.drawn = false;
    this.frame_count = 0;
    this.ready = new Promise((resolve, reject) => {
      this._resolve = resolve;
      this._reject = reject;
    });
    // A rejected `ready` nobody awaits is not an unhandled error: the
    // notice and `hapto:error` already said what went wrong.
    this.ready.catch(() => {});
  }

  connectedCallback() {
    if (this.stage) return; // moved within the page: keep what it has
    this.classList.add("hapto");
    this.source = this.querySelector('script[type^="application/hapto+"]');

    this.stage = element("div", "hapto-stage");
    this.canvas = element("canvas", "hapto-canvas", this.stage);
    this.canvas.setAttribute("role", "img");
    this.canvas.hidden = true;
    const fallback = this.querySelector("img.hapto-fallback");
    if (fallback) {
      this.fallback = fallback;
      // The box keeps the fallback's shape until the document says better.
      const w = Number(fallback.getAttribute("width"));
      const h = Number(fallback.getAttribute("height"));
      if (w > 0 && h > 0) this.stage.style.aspectRatio = `${w} / ${h}`;
    } else {
      this.fallback = element("img", "hapto-fallback");
      this.fallback.alt = "";
      this.fallback.hidden = true;
    }
    this.stage.append(this.fallback);
    this.readouts = element("div", "hapto-readouts");
    this.readouts.setAttribute("aria-live", "polite");
    this.controls = element("div", "hapto-controls");
    this.error = element("div", "hapto-error");
    this.error.hidden = true;
    this.prepend(this.stage);
    this.append(this.readouts, this.controls, this.error);

    live.add(this);
    const o = watch();
    o.resized.observe(this.stage);
    o.far.observe(this);
    if (this.hasAttribute("eager")) this._near();
    else o.near.observe(this);
  }

  disconnectedCallback() {
    // A move within the page disconnects and reconnects at once.
    queueMicrotask(() => {
      if (this.isConnected) return;
      const o = watch();
      o.near.unobserve(this);
      o.far.unobserve(this);
      o.resized.unobserve(this.stage);
      live.delete(this);
      if (engine?.figures.has(this.key)) {
        engine.post({ op: "free", id: this.key });
        engine.figures.delete(this.key);
      }
      this.state = "idle";
      if (this.snapshot) URL.revokeObjectURL(this.snapshot);
    });
  }

  async _near() {
    watch().near.unobserve(this);
    if (this.state !== "idle") return;
    this.state = "loading";
    try {
      const text = await this._text();
      const state = {};
      const initial = this.getAttribute("state");
      if (initial) Object.assign(state, this._own(parse_assignments(initial)));
      state.theme = page_theme();
      engine ??= new Engine();
      engine.figures.set(this.key, this);
      const format = this.source?.type.endsWith("json") ? "json" : "yaml";
      const loaded = await new Promise((resolve, reject) => {
        this._loading = { resolve, reject };
        engine.post({ op: "load", id: this.key, text, format, state });
      });
      const { meta, values } = loaded;
      if (this.id && meta.id && meta.id !== this.id) {
        throw Object.assign(
          new Error(`E410: this element is "${this.id}" and its document is "${meta.id}"`),
          { code: "E410", hint: "give the element and the document the same id, or drop one" },
        );
      }
      // The document's id names the element, unless another figure has it.
      if (!this.id && meta.id && !document.getElementById(meta.id)) this.id = meta.id;
      this.size = meta.size;
      this.values = values.values;
      this.warnings = loaded.warnings;
      this._build_controls();
      this.frames = new Scheduler((request) => this._send(request), { budget: config.budget_ms });
      if (offscreen()) {
        const canvas = this.canvas.transferControlToOffscreen();
        engine.post({ op: "canvas", id: this.key, canvas }, [canvas]);
        this.transferred = true;
      }
      this._pointer();
      this.state = "live";
      this._shape();
      this.frames.invalidate();
    } catch (e) {
      this._fail(e);
    }
  }

  async _text() {
    if (this.source) return this.source.textContent;
    const src = this.getAttribute("src");
    if (!src) {
      throw Object.assign(
        new Error('E402: a figure needs src= or an inline <script type="application/hapto+yaml">'),
        { code: "E402" },
      );
    }
    const reply = await fetch(new URL(src, document.baseURI));
    if (!reply.ok) throw new Error(`could not fetch ${src}: ${reply.status} ${reply.statusText}`);
    return reply.text();
  }

  /// Assignments by name; a `figure.name` for another figure is not ours.
  _own(pairs) {
    const state = {};
    for (const [name, value] of pairs) {
      const dot = name.indexOf(".");
      if (dot >= 0 && name.slice(0, dot) !== this.id) continue;
      state[dot >= 0 ? name.slice(dot + 1) : name] = value;
    }
    return state;
  }

  /// The box's shape from the document's size rules (§7.6).
  _shape() {
    const s = this.size;
    if (!s) return;
    const width = this.stage.clientWidth;
    const aspect = s.phone_aspect && width < 480 ? s.phone_aspect : s.aspect;
    this.stage.style.aspectRatio = String(aspect);
    this.stage.style.minHeight = `${s.min_height}px`;
    this.stage.style.maxHeight = `${s.max_height}px`;
  }

  _resize() {
    if (this.state !== "live") return;
    this._shape();
    this.frames.invalidate();
  }

  _dpr() {
    const cap = Number(this.dataset.dprCap) || 2;
    return Math.min(globalThis.devicePixelRatio || 1, cap);
  }

  /// Post the scheduler's frame with the box's size at this moment.
  _send(request) {
    const width = this.stage.clientWidth;
    const height = this.stage.clientHeight;
    if (this.state !== "live" || width === 0 || height === 0) {
      this.frames.drawn(0);
      return;
    }
    engine.post({
      op: "frame",
      id: this.key,
      frame: ++this.frame_count,
      ...request,
      width,
      height,
      dpr: this._dpr(),
      rgba: config.render === "rgba",
    });
  }

  /// What the worker says about this figure.
  _worker(msg) {
    switch (msg.op) {
      case "loaded":
        this._loading?.resolve(msg);
        return;
      case "failed":
        this._loading?.reject(Object.assign(new Error(msg.error.message), msg.error));
        return;
      case "evaluated":
        this._evaluated(msg);
        return;
      case "drawn":
        this._drawn(msg);
        return;
      case "snapshot":
        this._snapshot(msg.blob);
        return;
      case "error":
        console.error(`hapto: ${this.id || "a figure"}: ${msg.of} failed:`, msg.error);
        this.dispatchEvent(new CustomEvent("hapto:error", { detail: msg.error, bubbles: true }));
        if (msg.of === "frame") this.frames?.drawn(0);
        return;
    }
  }

  _evaluated({ set, refused, readouts, alt, values }) {
    if (refused) {
      console.error(`hapto: ${this.id || "a figure"} refused a set:`, refused);
      this.dispatchEvent(new CustomEvent("hapto:error", { detail: refused, bubbles: true }));
    }
    this.values = values.values;
    for (const v of this.values) this.syncs.get(v.name)?.(v.value);
    for (const [name, { value }] of Object.entries(set)) {
      if (name === "theme" || name === "focus") continue;
      this.dispatchEvent(
        new CustomEvent("hapto:change", { detail: { name, value, source: this.source_of(name) }, bubbles: true }),
      );
    }
    this.sources = {};
    this.canvas.setAttribute("aria-label", alt);
    if (this.getAttribute("readouts") === "none") return;
    this.readouts.replaceChildren(
      ...readouts
        .filter((r) => r.place !== "none" && r.place !== "inside")
        .map((r) => {
          const row = element("span", "hapto-readout");
          element("span", "hapto-readout-label", row).textContent = r.label;
          element("output", "hapto-readout-value", row).textContent = r.text;
          return row;
        }),
    );
  }

  source_of(name) {
    return this.sources?.[name] ?? "drag";
  }

  _drawn(msg) {
    if (!this.transferred && this.state === "live") {
      const ctx = this.canvas.getContext("2d");
      if (msg.bitmap) {
        // Full resolution backing store; a half-resolution frame scales up.
        const full = [Math.round(this.stage.clientWidth * this._dpr()), Math.round(this.stage.clientHeight * this._dpr())];
        if (this.canvas.width !== full[0]) this.canvas.width = full[0];
        if (this.canvas.height !== full[1]) this.canvas.height = full[1];
        ctx.imageSmoothingQuality = "high";
        ctx.drawImage(msg.bitmap, 0, 0, this.canvas.width, this.canvas.height);
        msg.bitmap.close();
      } else if (msg.rgba) {
        if (this.canvas.width !== msg.width) this.canvas.width = msg.width;
        if (this.canvas.height !== msg.height) this.canvas.height = msg.height;
        ctx.putImageData(new ImageData(new Uint8ClampedArray(msg.rgba), msg.width), 0, 0);
      }
    }
    this.last = { ms: msg.ms, quality: msg.quality, width: msg.width, height: msg.height };
    this.frames_drawn = (this.frames_drawn ?? 0) + 1;
    if (this.state === "live" && this.canvas.hidden) {
      this.canvas.hidden = false;
      this.fallback.hidden = true;
    }
    if (!this.drawn) {
      this.drawn = true;
      this._resolve(this);
    }
    this.frames.drawn(msg.ms);
  }

  /// Set values from the page: a control, the prose, the API.
  _set(state, source, interactive = false) {
    this.sources = { ...this.sources };
    for (const name of Object.keys(state)) this.sources[name] = source;
    this.frames.set(state, interactive);
  }

  /// One control per value (§12.1): a basic set, which hapto-hl7.6 refines.
  _build_controls() {
    if (this.getAttribute("controls") === "none") return;
    for (const v of this.values) {
      if (v.control?.type === "none" || v.type === "point") continue; // points drag (hl7.8)
      const row = element("label", "hapto-control", this.controls);
      element("span", "hapto-control-label", row).textContent = (v.label ?? v.name).replace(/\$/g, "");
      if (v.type === "number") this._slider(row, v);
      else if (v.type === "choice") this._buttons(row, v);
      else this._checkbox(row, v);
    }
  }

  _slider(row, v) {
    const input = element("input", "", row);
    input.type = "range";
    const log = v.scale === "log";
    const [lo, hi] = v.range;
    input.min = log ? Math.log(lo) : lo;
    input.max = log ? Math.log(hi) : hi;
    input.step = log ? (Math.log(hi) - Math.log(lo)) / 1000 : v.step;
    const out = element("output", "hapto-control-value", row);
    const sync = (value) => {
      // Not while the reader holds it: the thumb is theirs.
      if (document.activeElement !== input || !this.holding) input.value = log ? Math.log(value) : value;
      out.textContent = shown(value) + (v.unit ? ` ${v.unit}` : "");
      input.setAttribute("aria-valuetext", out.textContent);
    };
    sync(v.value);
    input.addEventListener("pointerdown", () => (this.holding = true));
    input.addEventListener("pointerup", () => (this.holding = false));
    input.addEventListener("input", () => {
      const n = Number(input.value);
      this._set({ [v.name]: log ? Math.exp(n) : n }, "control", this.holding);
    });
    this.syncs.set(v.name, sync);
  }

  _buttons(row, v) {
    const group = element("span", "hapto-choice", row);
    const buttons = v.options.map((option) => {
      const b = element("button", "", group);
      b.type = "button";
      b.textContent = v.labels?.[option] ?? option;
      b.addEventListener("click", () => this._set({ [v.name]: option }, "control"));
      return [option, b];
    });
    const sync = (value) =>
      buttons.forEach(([option, b]) => b.setAttribute("aria-pressed", String(option === value)));
    sync(v.value);
    this.syncs.set(v.name, sync);
  }

  _checkbox(row, v) {
    const box = element("input", "", row);
    box.type = "checkbox";
    box.checked = v.value;
    box.addEventListener("change", () => this._set({ [v.name]: box.checked }, "control"));
    this.syncs.set(v.name, (value) => (box.checked = value));
  }

  /// Dragging marks (§12.4): hapto tests the hit against its own geometry;
  /// moves ride on frames, so a fast pointer never queues stale ones.
  /// Keyboard and touch details come with hapto-hl7.8.
  _pointer() {
    const where = (e) => {
      const r = this.stage.getBoundingClientRect();
      return [e.clientX - r.left, e.clientY - r.top];
    };
    let held = null;
    let hovering = false;
    this.stage.addEventListener("pointerdown", async (e) => {
      if (this.state !== "live") return;
      const [x, y] = where(e);
      const { hit } = await engine.ask({ op: "hit", id: this.key, x, y, pointer: e.pointerType });
      if (!hit?.drag) return;
      held = hit.mark;
      engine.post({ op: "drag", id: this.key, mark: held, phase: "start", x, y });
      this.stage.setPointerCapture?.(e.pointerId);
      this.stage.dataset.cursor = "grabbing";
    });
    this.stage.addEventListener("pointermove", async (e) => {
      if (this.state !== "live") return;
      const [x, y] = where(e);
      if (held) {
        this.frames.drag_to(held, x, y);
        return;
      }
      if (hovering || e.pointerType === "touch") return; // one hover test at a time
      hovering = true;
      const { hit } = await engine.ask({ op: "hit", id: this.key, x, y, pointer: e.pointerType });
      hovering = false;
      if (!held) this.stage.dataset.cursor = hit?.drag ? "grab" : "";
    });
    const release = () => {
      if (!held) return;
      // The end rides on the next frame, after the last move.
      this.frames.release(held);
      held = null;
      this.stage.dataset.cursor = "grab";
    };
    this.stage.addEventListener("pointerup", release);
    this.stage.addEventListener("pointercancel", release);
  }

  /// Far away: keep the state, give up the canvas for an image of it.
  _away() {
    if (this.state !== "live") return;
    this.state = "released";
    if (this.transferred) {
      engine.post({ op: "snapshot", id: this.key });
      return;
    }
    this.canvas.toBlob((blob) => {
      if (this.state !== "released" || !blob) return;
      this._snapshot(blob);
      this.canvas.width = this.canvas.height = 0;
    });
  }

  _snapshot(blob) {
    if (this.state !== "released") return;
    if (this.snapshot) URL.revokeObjectURL(this.snapshot);
    this.snapshot = URL.createObjectURL(blob);
    this.fallback.src = this.snapshot;
    this.fallback.hidden = false;
    this.canvas.hidden = true;
  }

  _back() {
    if (this.state !== "released") return;
    this.state = "live";
    this.frames.invalidate();
  }

  /// The fallback stays, a notice says so, the console has the rest (§15.12).
  _fail(e) {
    this.state = "failed";
    const code = e?.code;
    this.error.hidden = false;
    this.error.textContent = `This figure could not be loaded${code ? ` (${code})` : ""}.`;
    console.error(`hapto: ${this.id || "a figure"} could not be loaded:`, e);
    this.dispatchEvent(new CustomEvent("hapto:error", { detail: e, bubbles: true }));
    this._reject(e);
  }
}

if (globalThis.customElements && !customElements.get("hapto-figure")) {
  customElements.define("hapto-figure", HaptoFigure);
}

// ---------------------------------------------------------------- the page

/// Counters for development and the benchmark harness: figures by state,
/// the bytes their page-side canvases hold, and how the last frames went.
function stats() {
  const states = {};
  let canvasBytes = 0;
  for (const f of live) {
    states[f.state] = (states[f.state] ?? 0) + 1;
    if (!f.transferred) canvasBytes += f.canvas.width * f.canvas.height * 4;
  }
  return {
    figures: live.size,
    states,
    canvasBytes,
    path: config.render === "auto" ? (globalThis.HTMLCanvasElement && offscreen() ? "offscreen" : "bitmap") : config.render,
  };
}

/// What a pointer at CSS (x, y) would take hold of in a figure: the hit
/// test a click makes, for tests and development tools.
async function hit(figure, x, y, pointer = "mouse") {
  if (!engine || figure.state !== "live") return null;
  return (await engine.ask({ op: "hit", id: figure.key, x, y, pointer })).hit;
}

export const hapto = { stats, config, dev: { hit } };
