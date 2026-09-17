#!/usr/bin/env bash
# Builds the graph renderer wasm module into packages/editors/graph/assets/.
# Needs: rustup target wasm32-unknown-unknown, and wasm-bindgen 0.2.128
# (the version dx installs under ~/.local/share/.dx/tools; the CLI version must
# match the wasm-bindgen crate version in Cargo.lock).
set -euo pipefail
cd "$(dirname "$0")/../../.."
BINDGEN="${WASM_BINDGEN:-$HOME/.local/share/.dx/tools/wasm-bindgen-0.2.128/wasm-bindgen}"
cargo build -p moonkale-graph-render --target wasm32-unknown-unknown --profile wasm-release
"$BINDGEN" --target web --no-typescript \
  --out-dir packages/editors/graph/assets --out-name graph_render \
  target/wasm32-unknown-unknown/wasm-release/moonkale_graph_render.wasm
ls -la packages/editors/graph/assets/graph_render*
