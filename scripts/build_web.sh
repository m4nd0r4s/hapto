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

# The scene the demo draws, which is also the test fixture.
cp crates/hapto-core/tests/parabola.json demo/parabola.json

echo "built: demo/pkg/hapto_web_bg.wasm ($(du -h demo/pkg/hapto_web_bg.wasm | cut -f1))"

if [ "${1:-}" != "--build" ]; then
  echo "serving http://localhost:8750/ — Ctrl-C to stop"
  cd demo && python3 -m http.server 8750
fi
