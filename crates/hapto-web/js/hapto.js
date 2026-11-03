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
// image of its last frame; disconnecting frees it. One wasm module serves
// every figure on the page.
//
// The engine runs on the main thread for now; the render worker
// (hapto.worker.js, §15.8) replaces it behind the same calls.

const HERE = new URL(".", import.meta.url);

// ---------------------------------------------------------------- engine

let engine = null;

/// The wasm module, loaded once for the whole page.
function load_engine() {
  engine ??= (async () => {
    const glue = await import(new URL("hapto_wasm.js", HERE).href);
    await glue.default({ module_or_path: new URL("hapto.wasm", HERE) });
    return glue;
  })();
  return engine;
}

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

// ---------------------------------------------------------------- observers

// Near: within one viewport height, so the first frame is ready on arrival.
const near = new IntersectionObserver(
  (entries) => entries.forEach((e) => e.isIntersecting && e.target._near()),
  { rootMargin: "100% 0px" },
);
// Far: more than three viewport heights away, the canvas may go (§15.3 step 4).
const far = new IntersectionObserver(
  (entries) => entries.forEach((e) => (e.isIntersecting ? e.target._back() : e.target._away())),
  { rootMargin: "300% 0px" },
);
const resized = new ResizeObserver((entries) =>
  entries.forEach((e) => e.target.closest("hapto-figure")?._resize()),
);

// ---------------------------------------------------------------- the element

const live = new Set();

class HaptoFigure extends HTMLElement {
  constructor() {
    super();
    this.figure = null;
    // idle → loading → live ⇄ released, or failed
    this.state = "idle";
    this.syncs = new Map();
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
    resized.observe(this.stage);
    far.observe(this);
    if (this.hasAttribute("eager")) this._near();
    else near.observe(this);
  }

  disconnectedCallback() {
    // A move within the page disconnects and reconnects at once.
    queueMicrotask(() => {
      if (this.isConnected) return;
      near.unobserve(this);
      far.unobserve(this);
      resized.unobserve(this.stage);
      live.delete(this);
      this.figure?.free();
      this.figure = null;
      this.state = "idle";
      if (this.snapshot) URL.revokeObjectURL(this.snapshot);
    });
  }

  async _near() {
    near.unobserve(this);
    if (this.state !== "idle") return;
    this.state = "loading";
    try {
      const text = await this._text();
      const wasm = await load_engine();
      const format = this.source?.type.endsWith("json") ? "json" : "yaml";
      this.figure = wasm.Figure.load(text, format);
      const meta = this.figure.meta();
      if (this.id && meta.id && meta.id !== this.id) {
        throw Object.assign(
          new Error(`E410: this element is "${this.id}" and its document is "${meta.id}"`),
          { code: "E410", hint: "give the element and the document the same id, or drop one" },
        );
      }
      // The document's id names the element, unless another figure has it.
      if (!this.id && meta.id && !document.getElementById(meta.id)) this.id = meta.id;
      this.size = meta.size;
      this.figure.setState({ theme: page_theme() });
      const initial = this.getAttribute("state");
      if (initial) this._apply(parse_assignments(initial));
      this._build_controls();
      this.state = "live";
      this._shape();
      this._draw();
      this._resolve(this);
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
  _apply(pairs) {
    const state = {};
    for (const [name, value] of pairs) {
      const dot = name.indexOf(".");
      if (dot >= 0 && name.slice(0, dot) !== this.id) continue;
      state[dot >= 0 ? name.slice(dot + 1) : name] = value;
    }
    return this.figure.setState(state);
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
    this._schedule();
  }

  /// At most one render per animation frame; only the newest state draws.
  _schedule() {
    this.frame ??= requestAnimationFrame(() => {
      this.frame = null;
      this._draw();
    });
  }

  _draw() {
    if (this.state !== "live") return;
    const f = this.figure;
    const width = this.stage.clientWidth;
    const height = this.stage.clientHeight;
    if (width === 0 || height === 0) return;
    const cap = Number(this.dataset.dprCap) || 2;
    f.resize(width, height, Math.min(devicePixelRatio || 1, cap));
    if (this.canvas.width !== f.pixelWidth) this.canvas.width = f.pixelWidth;
    if (this.canvas.height !== f.pixelHeight) this.canvas.height = f.pixelHeight;
    // Readouts before pixels (§15.8).
    this._show(f.evaluate());
    const rgba = f.render();
    const pixels = new Uint8ClampedArray(rgba.buffer, rgba.byteOffset, rgba.length);
    this.canvas.getContext("2d").putImageData(new ImageData(pixels, f.pixelWidth), 0, 0);
    if (this.canvas.hidden) {
      this.canvas.hidden = false;
      this.fallback.hidden = true;
    }
  }

  _show({ readouts, alt }) {
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

  /// One control per value (§12.1): a basic set, which hapto-hl7.6 refines.
  _build_controls() {
    if (this.getAttribute("controls") === "none") return;
    const { values } = this.figure.values();
    for (const v of values) {
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
      input.value = log ? Math.log(value) : value;
      out.textContent = shown(value) + (v.unit ? ` ${v.unit}` : "");
      input.setAttribute("aria-valuetext", out.textContent);
    };
    sync(v.value);
    input.addEventListener("input", () => {
      const n = Number(input.value);
      const set = this.figure.setState({ [v.name]: log ? Math.exp(n) : n });
      sync(set[v.name].value);
      this._changed(v.name, set[v.name].value, "control");
    });
    this.syncs.set(v.name, sync);
  }

  _buttons(row, v) {
    const group = element("span", "hapto-choice", row);
    const buttons = v.options.map((option) => {
      const b = element("button", "", group);
      b.type = "button";
      b.textContent = v.labels?.[option] ?? option;
      b.addEventListener("click", () => {
        this.figure.setState({ [v.name]: option });
        sync(option);
        this._changed(v.name, option, "control");
      });
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
    box.addEventListener("change", () => {
      this.figure.setState({ [v.name]: box.checked });
      this._changed(v.name, box.checked, "control");
    });
    this.syncs.set(v.name, (value) => (box.checked = value));
  }

  _changed(name, value, source) {
    this._schedule();
    this.dispatchEvent(
      new CustomEvent("hapto:change", { detail: { name, value, source }, bubbles: true }),
    );
  }

  /// Far away: keep the state, give up the canvas for an image of it.
  _away() {
    if (this.state !== "live") return;
    this.state = "released";
    this.canvas.toBlob((blob) => {
      if (this.state !== "released" || !blob) return;
      if (this.snapshot) URL.revokeObjectURL(this.snapshot);
      this.snapshot = URL.createObjectURL(blob);
      this.fallback.src = this.snapshot;
      this.fallback.hidden = false;
      this.canvas.hidden = true;
      this.canvas.width = this.canvas.height = 0;
    });
  }

  _back() {
    if (this.state !== "released") return;
    this.state = "live";
    this._draw();
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

customElements.define("hapto-figure", HaptoFigure);

// ---------------------------------------------------------------- the page

/// Counters for development and the benchmark harness: figures by state
/// and the bytes their canvases hold.
function stats() {
  const states = {};
  let canvasBytes = 0;
  for (const f of live) {
    states[f.state] = (states[f.state] ?? 0) + 1;
    canvasBytes += f.canvas.width * f.canvas.height * 4;
  }
  return { figures: live.size, states, canvasBytes };
}

export const hapto = { stats, ready: load_engine };
