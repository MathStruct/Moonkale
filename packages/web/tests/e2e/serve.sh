#!/usr/bin/env bash
# Starts/stops the dev server the E2E suites talk to, tracked by PID — never
# `pkill dx`, which also kills the desktop dev server running in another shell.
#   packages/web/tests/e2e/serve.sh start [m2root|m1root]   # waits until it serves
#   packages/web/tests/e2e/serve.sh stop
#   PORT=8091 MOONKALE_TOKEN=e2e-secret-token packages/web/tests/e2e/serve.sh start   # token mode (auth.mjs)
# Each PORT has its own pid/log file, so a token-mode server can run next to the normal one.
# SERVE_FEATURES: the web crate's features for the build (default `julia`: flow.mjs drives the Lux
# blocks, which come from MathStruct/moonkale-julia since Milestone 18 phase 6.4).
# SERVE_WAIT: how many 5 s rounds to wait for the first build (default 240 = 20 min; CI sets more).
set -uo pipefail
E="${MOONKALE_E2E:-$HOME/.cache/moonkale-e2e}"
PORT="${PORT:-8090}"
SERVE_FEATURES="${SERVE_FEATURES:-julia}"
REPO="$(cd "$(dirname "$0")/../../../.." && pwd)"
# A relative CARGO_TARGET_DIR would be resolved from packages/web below (it put
# 15 GB of build output under packages/web/target once — P-148): anchor it.
case "${CARGO_TARGET_DIR:-}" in ""|/*) ;; *) export CARGO_TARGET_DIR="$REPO/$CARGO_TARGET_DIR" ;; esac
case "${1:-start}" in
  start)
    root="$E/${2:-m2root}"
    "$0" stop
    cd "$REPO/packages/web"
    # MOONKALE_CLAUDE_BIN: the mock `claude` for claude-code.mjs (Milestone 12).
    MOONKALE_CONFIG_DIR="$E/cfg" MOONKALE_LLM="${MOONKALE_LLM:-mock}" MOONKALE_ROOT="$root" MOONKALE_CLAUDE_BIN="$REPO/packages/services/llm/tests/mock-claude.sh" \
      nohup dx serve --port "$PORT" --features "$SERVE_FEATURES" > "$E/dx-$PORT.log" 2>&1 &
    echo $! > "$E/dx-$PORT.pid"
    # dx prints its banner before the build is done: wait until the server answers.
    for _ in $(seq 1 "${SERVE_WAIT:-240}"); do
      sleep 5
      code=$(curl -s -o /dev/null -w '%{http_code}' --max-time 3 "http://127.0.0.1:$PORT/login" 2>/dev/null)
      if [ "$code" = "200" ] || [ "$code" = "302" ] || [ "$code" = "404" ]; then echo "serving $root on :$PORT (pid $(cat "$E/dx-$PORT.pid"))"; exit 0; fi
      if grep -q "Build failed" "$E/dx-$PORT.log"; then echo "build failed — see $E/dx-$PORT.log"; exit 1; fi
    done
    echo "timeout — see $E/dx-$PORT.log"; exit 1 ;;
  stop)
    if [ -f "$E/dx-$PORT.pid" ]; then
      pid=$(cat "$E/dx-$PORT.pid")
      # dx and the server binary it spawned (which otherwise outlives it)
      pkill -P "$pid" 2>/dev/null; kill "$pid" 2>/dev/null
      rm -f "$E/dx-$PORT.pid"
    fi ;;
esac
