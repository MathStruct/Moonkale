#!/usr/bin/env bash
# Starts/stops the dev server the E2E suites talk to, tracked by PID — never
# `pkill dx`, which also kills the desktop dev server running in another shell.
#   packages/web/tests/e2e/serve.sh start [m2root|m1root]   # waits until it serves
#   packages/web/tests/e2e/serve.sh stop
#   PORT=8091 MOONKALE_TOKEN=e2e-secret-token packages/web/tests/e2e/serve.sh start   # token mode (auth.mjs)
# Each PORT has its own pid/log file, so a token-mode server can run next to the normal one.
# SERVE_FEATURES: the web crate's features for the build (default `julia,rocksdb,helix,crash-test`: flow.mjs
# drives the Lux blocks, which come from MathStruct/moonkale-julia since Milestone 18 phase 6.4;
# stores.mjs opens a RocksDB and a HelixDB store, opt-in features since 2026-10-19).
# Set SERVE_FEATURES= (explicitly empty) to test platform defaults only.
# SERVE_WAIT: total startup seconds across all attempts (default 1200).
# SERVE_BACKEND_WAIT: seconds after dx launches the app to answer HTTP (default 120).
# SERVE_ATTEMPTS: maximum attempts (default 1; CI uses 2); only inactivity retries.
# SERVE_STALL: give up after this many seconds without progress (default 1200): a hung
#   build fails early instead of using up SERVE_WAIT (CI, 2026-10-09: 80 minutes for nothing).
#   Progress is the dx log growing *or* a new file in the target dir: without a terminal dx
#   prints nothing while cargo compiles, so a cold CI build looked hung after 20 min (P-175).
set -uo pipefail
E="${MOONKALE_E2E:-$HOME/.cache/moonkale-e2e}"
PORT="${PORT:-8090}"
SERVE_FEATURES="${SERVE_FEATURES-julia,rocksdb,helix,crash-test}"
REPO="$(cd "$(dirname "$0")/../../../.." && pwd)"
# A relative CARGO_TARGET_DIR would be resolved from packages/web below (it put
# 15 GB of build output under packages/web/target once — P-148): anchor it.
case "${CARGO_TARGET_DIR:-}" in ""|/*) ;; *) export CARGO_TARGET_DIR="$REPO/$CARGO_TARGET_DIR" ;; esac
# Attempts share one deadline. Only inactivity is retryable; build errors are not.
SERVE_ATTEMPTS="${SERVE_ATTEMPTS:-1}"
PROCESSES="$REPO/packages/web/tests/e2e/serve-processes.py"
owner="$E/dx-$PORT.owner"
stop_owned() {
  if [ -f "$owner" ]; then
    python3 "$PROCESSES" stop "$owner" || return 1
  elif [ -f "$E/dx-$PORT.pid" ]; then
    echo "PID file has no ownership record; refusing to signal or replace it: $E/dx-$PORT.pid" >&2
    return 1
  fi
  rm -f "$E/dx-$PORT.pid" "$owner"
}
diagnose() {
  {
    echo "attempt=$attempt reason=$reason time=$(date +%s) deadline=$deadline last_progress=$last_change log_bytes=$last_size"
    echo "target=$target"
    python3 "$PROCESSES" diagnose "$owner"
    df -h "$REPO" "$E"
    echo "dx/server log head ($log):"
    head -n 60 "$log"
    echo "dx/server log tail ($log):"
    tail -n 100 "$log"
  } > "$E/dx-$PORT.attempt-$attempt.diagnostics.log" 2>&1
  cat "$E/dx-$PORT.attempt-$attempt.diagnostics.log"
}
case "${1:-start}" in
  start)
    if ! [[ "$SERVE_ATTEMPTS" =~ ^[1-9][0-9]*$ ]]; then echo "SERVE_ATTEMPTS must be positive" >&2; exit 1; fi
    mkdir -p "$E"
    root="$E/${2:-m2root}"
    stop_owned || exit 1
    rm -f "$E/dx-$PORT".attempt-*.log
    cd "$REPO/packages/web"
    deadline=$(( $(date +%s) + ${SERVE_WAIT:-1200} ))
    target="${CARGO_TARGET_DIR:-$REPO/target}"; mark="$E/dx-$PORT.progress"
    features=()
    if [ -n "$SERVE_FEATURES" ]; then features=(--features "$SERVE_FEATURES"); fi
    flags=()
    if [ "${CI:-}" = true ]; then flags=(--interactive false --open false --verbose --locked); fi
    for ((attempt=1; attempt<=SERVE_ATTEMPTS; attempt++)); do
      if [ "$(date +%s)" -ge "$deadline" ]; then echo "startup deadline exhausted"; exit 1; fi
      log="$E/dx-$PORT.attempt-$attempt.log"
      # Keep the established path for tooling that tails the current attempt.
      ln -sfn "$(basename "$log")" "$E/dx-$PORT.log"
      # A separate session/process group contains dx, cargo and the app even if
      # a parent exits. The ownership record checks PID reuse before signalling.
      MOONKALE_CONFIG_DIR="$E/cfg" MOONKALE_LLM="${MOONKALE_LLM:-mock}" MOONKALE_ROOT="$root" MOONKALE_CLAUDE_BIN="$REPO/packages/services/llm/tests/mock-claude.sh" \
        nohup python3 "$PROCESSES" launch "$owner" dx serve --port "$PORT" "${features[@]}" "${flags[@]}" > "$log" 2>&1 &
      pid=$!; echo "$pid" > "$E/dx-$PORT.pid"
      last_size=0; last_change=$(date +%s); touch "$mark"
      reason=deadline; backend_started=0
      while [ "$(date +%s)" -lt "$deadline" ]; do
        sleep 5
        if [ "$(date +%s)" -ge "$deadline" ]; then break; fi
        if grep -qi "build failed" "$log"; then reason=build; break; fi
        if ! kill -0 "$pid" 2>/dev/null; then reason=exit; break; fi
        code=$(curl -s -o /dev/null -w '%{http_code}' --max-time 3 "http://127.0.0.1:$PORT/login" 2>/dev/null)
        if [ "$(date +%s)" -ge "$deadline" ]; then break; fi
        if [ "$code" = "200" ] || [ "$code" = "302" ] || [ "$code" = "404" ]; then
          echo "serving $root on :$PORT (pid $pid; attempt $attempt)"; exit 0
        fi
        # Backend retry chatter is not evidence that the launched app is healthy.
        # Keep cold compilation separate from the much shorter runtime budget.
        if [ "$backend_started" = 0 ] && grep -q "launching app" "$log"; then
          backend_started=$(date +%s)
        fi
        if [ "$backend_started" != 0 ] && [ $(( $(date +%s) - backend_started )) -ge "${SERVE_BACKEND_WAIT:-120}" ]; then
          reason=backend; break
        fi
        size=$(sed '/Backend not ready, retrying/d' "$log" | wc -c)
        if [ "$size" != "$last_size" ]; then last_size=$size; last_change=$(date +%s); fi
        if [ $(( $(date +%s) - last_change )) -ge 60 ]; then
          if [ -n "$(find "$target" -newer "$mark" -type f -print -quit 2>/dev/null)" ]; then last_change=$(date +%s); fi
          touch "$mark"
        fi
        if [ $(( $(date +%s) - last_change )) -ge "${SERVE_STALL:-1200}" ]; then reason=stall; break; fi
      done
      diagnose
      stop_owned || exit 1
      wait "$pid" 2>/dev/null || true
      echo "startup attempt $attempt failed ($reason) — see $log"
      if [ "$reason" != stall ] || [ "$attempt" -ge "$SERVE_ATTEMPTS" ]; then exit 1; fi
      echo "retrying stalled startup within the original deadline"
    done
    exit 1 ;;
  stop) stop_owned ;;
  *) echo "usage: $0 start [fixture] | stop" >&2; exit 1 ;;
esac
