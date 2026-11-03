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

# The page runtime has no build step; at least it must parse.
node --check crates/hapto-web/js/hapto.js

HAPTO_WEB_PKG="$root/$out" node --test crates/hapto-web/tests/node/*.test.mjs
