#!/usr/bin/env bash
# Builds the E2E fixture folder used by every suite except milestone1.mjs.
# Usage: packages/web/tests/e2e/fixture.sh /path/to/m2root
set -euo pipefail
ROOT="${1:?fixture directory}"
REPO="$(cd "$(dirname "$0")/../../../.." && pwd)"
rm -rf "$ROOT"; mkdir -p "$ROOT/notes" "$ROOT/src"
printf '# Home\nSee [[Alpha]] and [[notes/Beta]] and [[Missing]].\n' > "$ROOT/Home.md"
printf 'Back to [[Home]]. Code: [lib](src/lib.md)\n' > "$ROOT/Alpha.md"
printf 'Beta links [[Alpha]].\n' > "$ROOT/notes/Beta.md"
printf '# Sample\n\nEdit me.\n' > "$ROOT/README.md"
printf 'pub struct Thing;\nimpl Thing { pub fn go(&self) {} }\npub fn free() {}\n' > "$ROOT/src/lib.rs"
printf 'fn add(a: u32, b: u32) -> u32 {\n    a + b\n}\n\nfn main() {\n    let total: u32 = add(1, 2);\n    let wrong: String = total;\n    println!("{wrong}");\n}\n' > "$ROOT/src/main.rs"
printf '[package]\nname = "m2root"\nversion = "0.1.0"\nedition = "2021"\n\n[dependencies]\n' > "$ROOT/Cargo.toml"
printf '#set page(width: 10cm, height: auto)\n= Report\nA paragraph with *emphasis*.\n' > "$ROOT/report.typ"
python3 - "$ROOT/data.sqlite" <<'PY'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
c.executescript("""
CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT, score REAL);
INSERT INTO users(name, score) VALUES ('ann', 1.5), ('bob', NULL), ('cid', 3.25);
CREATE TABLE tags(id INTEGER PRIMARY KEY, label TEXT);
""")
c.commit()
PY
(cd "$REPO" && cargo run -q -p moonkale-sources-graph --features ladybug --example seed_people -- "$ROOT/people.lbug" small >/dev/null)
echo "fixture ready at $ROOT"
