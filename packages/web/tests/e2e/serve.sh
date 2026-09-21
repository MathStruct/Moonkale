#!/usr/bin/env bash
# Starts/stops the dev server the E2E suites talk to, tracked by PID — never
# `pkill dx`, which also kills the desktop dev server running in another shell.
#   packages/web/tests/e2e/serve.sh start [m2root|m1root]   # waits until it serves
#   packages/web/tests/e2e/serve.sh stop
#   PORT=8091 MOONKALE_TOKEN=e2e-secret-token packages/web/tests/e2e/serve.sh start   # token mode (auth.mjs)
# Each PORT has its own pid/log file, so a token-mode server can run next to the normal one.
set -uo pipefail
E="${MOONKALE_E2E:-$HOME/.cache/moonkale-e2e}"
PORT="${PORT:-8090}"
REPO="$(cd "$(dirname "$0")/../../../.." && pwd)"
case "${1:-start}" in
  start)
    root="$E/${2:-m2root}"
    "$0" stop
    cd "$REPO/packages/web"
    MOONKALE_CONFIG_DIR="$E/cfg" MOONKALE_LLM="${MOONKALE_LLM:-mock}" MOONKALE_ROOT="$root" \
      nohup dx serve --port "$PORT" > "$E/dx-$PORT.log" 2>&1 &
    echo $! > "$E/dx-$PORT.pid"
    # dx prints its banner before the build is done: wait until the server answers.
    for _ in $(seq 1 240); do
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
