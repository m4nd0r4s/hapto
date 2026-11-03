#!/usr/bin/env bash
# Build the figure for the browser and serve the demo.
#
#   scripts/build_web.sh          build and serve on 8750
#   scripts/build_web.sh --build  build only
#
# Needs the wasm32-unknown-unknown target and wasm-bindgen at the version
# crates/hapto-web pins:
#
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-bindgen-cli --version 0.2.129
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

pinned=$(grep -o 'wasm-bindgen = "=[0-9.]*"' crates/hapto-web/Cargo.toml | grep -o '[0-9][0-9.]*')
found=$(wasm-bindgen --version 2>/dev/null | grep -o '[0-9][0-9.]*' || true)
if [ "$found" != "$pinned" ]; then
  echo "wasm-bindgen $pinned is needed (found ${found:-none}):" >&2
  echo "  cargo install wasm-bindgen-cli --version $pinned" >&2
  exit 1
fi

cargo build -p hapto-web --target wasm32-unknown-unknown --profile web
wasm-bindgen target/wasm32-unknown-unknown/web/hapto_web.wasm \
  --out-dir demo/pkg --target web --no-typescript

# The figure the demo draws, with its TeX baked: the browser build has none.
cargo build -q -p hapto-cli
target/debug/hapto bake examples/corpus/b-tangent.yaml -o demo/tangent.json

# The page runtime as a page loads it (SPEC-2.0 §15.1): hapto.js, hapto.css,
# the module's glue and hapto.wasm, side by side.
mkdir -p demo/hapto
wasm-bindgen target/wasm32-unknown-unknown/web/hapto_web.wasm \
  --out-dir demo/hapto --out-name hapto_wasm --target web --no-typescript
mv demo/hapto/hapto_wasm_bg.wasm demo/hapto/hapto.wasm
cp crates/hapto-web/js/hapto.js crates/hapto-web/js/hapto.css demo/hapto/

# The runtime's test page draws the corpus, baked.
mkdir -p demo/runtime/figures
for f in examples/corpus/*.yaml; do
  n=$(basename "$f" .yaml)
  target/debug/hapto bake "$f" -o "demo/runtime/figures/$n.json"
  # The fallback a book would ship: the default state as SVG (§15.9).
  target/debug/hapto render "$f" -o "demo/runtime/figures/$n.svg" --width 720
done

echo "built: demo/pkg/hapto_web_bg.wasm ($(du -h demo/pkg/hapto_web_bg.wasm | cut -f1)), demo/hapto/"

if [ "${1:-}" != "--build" ]; then
  echo "serving http://localhost:8750/ — Ctrl-C to stop"
  cd demo && python3 -m http.server 8750
fi
