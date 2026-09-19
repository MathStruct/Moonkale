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
ALL="menubar session graph links-sqlite terminal typst lsp ladybug agent search-trace settings rich agent-writes flow wasm-ext phone palette files replace lsp2 git auth history presence graph3d"
reset() {
  # Back to the committed fixture (the folder is a git repository).
  git -C "$E/m2root" reset -q --hard "$(git -C "$E/m2root" rev-list --max-parents=0 HEAD)" 2>/dev/null; git -C "$E/m2root" clean -fdq 2>/dev/null
  rm -rf "$E/m2root/.moonkale" "$E/m2root"/*.flow.json "$E/m2root/model.jl" "$E/m2root"/untitled* \
    "$E/m2root"/todo.md "$E/m2root"/TODO.md "$E/m2root/notes/drafts" "$E/m2root/notes/TODO.md" "$E/m2root"/note.md "$E/m2root"/notes.md   # files.mjs, history.mjs
  printf '# Home\nSee [[Alpha]] and [[notes/Beta]] and [[Missing]].\n' > "$E/m2root/Home.md"
  printf 'Back to [[Home]]. Code: [lib](src/lib.md)\n' > "$E/m2root/Alpha.md"
  printf 'Beta links [[Alpha]].\n' > "$E/m2root/notes/Beta.md"
  printf '# Sample\n\nEdit me.\n' > "$E/m2root/README.md"
  printf 'fn add(a: u32, b: u32) -> u32 {\n    a + b\n}\n\nfn main() {\n    let total: u32 = add(1, 2);\n    let wrong: String = total;\n    println!("{wrong}");\n}\n' > "$E/m2root/src/main.rs"
}
fail=0
cd "$E/e2e"
for s in ${@:-$ALL}; do
  reset
  if [ "$s" = auth ]; then
    # Token mode needs its own server (port 8091), started and stopped here.
    PORT=8091 MOONKALE_TOKEN=e2e-secret-token "$HERE/serve.sh" start >/dev/null || { echo "== auth: server failed"; fail=1; continue; }
    out=$(PORT=8091 timeout 420 node auth.mjs 2>&1); rc=$?
    PORT=8091 "$HERE/serve.sh" stop
  else
    out=$(timeout 420 node "$s.mjs" 2>&1); rc=$?
  fi
  echo "== $s: rc=$rc $(echo "$out" | grep -E 'PASS|FAIL' | tail -1)"
  if [ $rc -ne 0 ]; then echo "$out" | tail -15; fail=1; fi
done
reset
exit $fail
