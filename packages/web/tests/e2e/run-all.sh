#!/usr/bin/env bash
# Runs the browser suites against a dev server, resetting the fixture between them.
# Working directory (node_modules, fixture, config dir, screenshots) defaults to
# ~/.cache/moonkale-e2e — outside /tmp, so it survives a reboot.
#
#   E=~/.cache/moonkale-e2e
#   mkdir -p $E/e2e && (cd $E/e2e && npm i playwright && npx playwright install firefox)   # once
#   packages/web/tests/e2e/fixture.sh $E/m2root                                            # once
#   MOONKALE_CONFIG_DIR=$E/cfg packages/extensions/wordcount/build.sh                      # once, for wasm-ext
#   (cd packages/web && MOONKALE_CONFIG_DIR=$E/cfg MOONKALE_LLM=mock MOONKALE_ROOT=$E/m2root dx serve --port 8090)
#   packages/web/tests/e2e/run-all.sh [suite …]
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
E="${MOONKALE_E2E:-$HOME/.cache/moonkale-e2e}"
export PORT="${PORT:-8090}" M1_ROOT="$E/m2root" M1_SHOTS="$E/shots"
mkdir -p "$E/e2e" "$E/shots"
cp "$HERE"/*.mjs "$E/e2e/"   # playwright resolves from the working directory
ALL="menubar session graph links-sqlite terminal typst lsp ladybug agent search-trace settings rich agent-writes flow wasm-ext phone"
reset() {
  rm -rf "$E/m2root/.moonkale" "$E/m2root"/*.flow.json "$E/m2root/model.jl" "$E/m2root"/untitled*
  printf '# Home\nSee [[Alpha]] and [[notes/Beta]] and [[Missing]].\n' > "$E/m2root/Home.md"
}
fail=0
cd "$E/e2e"
for s in ${@:-$ALL}; do
  reset
  out=$(timeout 420 node "$s.mjs" 2>&1); rc=$?
  echo "== $s: rc=$rc $(echo "$out" | grep -E 'PASS|FAIL' | tail -1)"
  if [ $rc -ne 0 ]; then echo "$out" | tail -15; fail=1; fi
done
reset
exit $fail
