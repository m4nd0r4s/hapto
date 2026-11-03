#!/usr/bin/env bash
# Build hapto-web for Node and run the surface tests (SPEC-2.0 §15.10).
#
# Uses the browser's build profile, so what is tested is what ships. Needs
# Node 20 or later, the wasm32-unknown-unknown target and wasm-bindgen at the
# version crates/hapto-web pins (see scripts/build_web.sh).
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

cargo build -p hapto-web --target wasm32-unknown-unknown --profile web
out=target/hapto-web-node
wasm-bindgen target/wasm32-unknown-unknown/web/hapto_web.wasm \
  --out-dir "$out" --target nodejs --no-typescript

# The page runtime as a page loads it: the web glue, hapto.wasm and the
# runtime's own files, side by side.
runtime=target/hapto-runtime
wasm-bindgen target/wasm32-unknown-unknown/web/hapto_web.wasm \
  --out-dir "$runtime" --out-name hapto_wasm --target web --no-typescript
mv "$runtime/hapto_wasm_bg.wasm" "$runtime/hapto.wasm"
cp crates/hapto-web/js/*.js "$runtime/"

HAPTO_WEB_PKG="$root/$out" HAPTO_RUNTIME="$root/$runtime" node --test crates/hapto-web/tests/node/*.test.mjs
