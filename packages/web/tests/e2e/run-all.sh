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
AUTH_PORT="${AUTH_PORT:-8091}"   # the token-mode server of auth.mjs
export PORT="${PORT:-8090}" M1_ROOT="$E/m2root" M1_SHOTS="$E/shots" MOONKALE_REPO="$(cd "$HERE/../../../.." && pwd)"
mkdir -p "$E/e2e" "$E/shots"
cp "$HERE"/*.mjs "$E/e2e/"   # playwright resolves from the working directory
ALL="table-edit proposals sharing agent-display terminal-private density mouse keymaps journey-keyboard journey-mouse shortcut-discovery crash-safety recovery sessions linked-panels overview menubar appearance session graph graph-overview graph-startup graph-camera graph-egraph graph-egraph-ui graph-edges graph-edit graph-layers graph-delta graph-groups graph-higher graph-pulse graph-flow graph-shapes graph-a11y links-sqlite terminal typst lsp ladybug agent search-trace settings rich rich-images codemirror-search responsive-tabs agent-writes flow canvas-layer flow-tree-open wasm-ext phone palette files replace lsp2 lsp-restart live-preview-long live-preview-links live-preview-images live-preview-tables live-preview-math live-preview-callouts live-preview-wiki-hover live-preview-properties doc-preview doc-preview-long git git-sync git-everyday auth history presence graph3d duckdb wiki highlight panels image shell server extensions claude-code agent-server terminal-native code-native agents watch graph-local touch-drag stores roundtrip mtk subflow rxinfer table-grid toolbar-icons"
reset() {
  # Back to the committed fixture (the folder is a git repository).
  git -C "$E/m2root" reset -q --hard "$(git -C "$E/m2root" rev-list --max-parents=0 HEAD)" 2>/dev/null; git -C "$E/m2root" clean -fdq 2>/dev/null
  rm -rf "$E/m2root/.moonkale" "$E/m2root"/*.flow.json "$E/m2root/model.jl" "$E/m2root/inference.jl" "$E/m2root"/untitled* \
    "$E/m2root"/todo.md "$E/m2root"/TODO.md "$E/m2root/notes/drafts" "$E/m2root/notes/TODO.md" "$E/m2root"/note.md "$E/m2root"/notes.md "$E/m2root"/Math.md "$E/m2root"/Missing.md "$E/m2root"/Nowhere.md "$E/m2root"/Alpha2.md "$E/m2root/pics" "$E/m2root"/big.txt "$E/m2root"/Front.md "$E/m2root"/Norm.md "$E/m2root"/Gaps.md "$E/m2root"/Reload.md "$E/m2root"/Outside.md "$E/m2root/later" "$E/m2root/.secret" "$E/m2root/.moonkale/agent-sessions" "$E/m2root/e3"   # files.mjs, history.mjs, rich.mjs, wiki.mjs, roundtrip.mjs
  # The entity log and agent sessions live in the server's store (phase 5.10, 5.15); SQLite lets this
  # second process clear them.
  [ -f "$E/cfg/state.sqlite" ] && sqlite3 "$E/cfg/state.sqlite" "DELETE FROM kv WHERE t IN ('events', 'agent_sessions')" 2>/dev/null
  # The suites drive the Source editor; markdown opens in Rich mode by default (spec 021).
  mkdir -p "$E/m2root/.moonkale" && printf '{"editor":{"markdown_rich":false}}\n' > "$E/m2root/.moonkale/settings.json"
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
  if [ "$s" = sophia-folder ]; then
    cargo run --quiet --locked --manifest-path "$MOONKALE_REPO/Cargo.toml" -p moonkale-sources-sophia --example save_warning_fixture -- "$M1_ROOT/warnings.sophia" || { fail=1; continue; }
  fi
  if [ "$s" = table-grid ]; then
    cargo run --quiet --locked --manifest-path "$MOONKALE_REPO/Cargo.toml" -p moonkale-sources-sql --features duckdb --example seed_scale -- "$M1_ROOT/million.duckdb" || { fail=1; continue; }
  fi
  if [ "$s" = table-edit ]; then
    cargo build --quiet --locked --manifest-path "$MOONKALE_REPO/Cargo.toml" -p moonkale-sources-sql --features duckdb --example change_row || { fail=1; continue; }
  fi
  if [ "$s" = auth ]; then
    # Token mode needs its own server (AUTH_PORT, default 8091), started and stopped here.
    PORT=$AUTH_PORT MOONKALE_TOKEN=e2e-secret-token "$HERE/serve.sh" start >/dev/null || { echo "== auth: server failed"; fail=1; continue; }
    out=$(PORT=$AUTH_PORT timeout 420 node auth.mjs 2>&1); rc=$?
    PORT=$AUTH_PORT "$HERE/serve.sh" stop
  elif [ "$s" = server ]; then
    # The standalone server binary (Milestone 11) — needs `cd packages/web && dx build --platform server`.
    out=$(timeout 300 node server.mjs 2>&1); rc=$?
  else
    suite_timeout=420
    [ "$s" = table-grid ] && suite_timeout=1200
    out=$(timeout "$suite_timeout" node "$s.mjs" 2>&1); rc=$?
  fi
  echo "== $s: rc=$rc $(echo "$out" | grep -E 'PASS|FAIL' | tail -1)"
  if [ $rc -ne 0 ]; then echo "$out" | tail -15; fail=1; fi
done
reset
exit $fail
