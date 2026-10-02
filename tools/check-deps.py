#!/usr/bin/env python3
"""Layering rules for the workspace (Milestone 18, phase 0).

Reads `cargo metadata` and checks the rules from
markdown/milestones/Milestone 18 - Library Refactor.md ("Target shape").
Violations that exist today are listed in tools/deps-allow.txt; the check
fails on a violation that is not listed **and** on a listed one that is gone,
so the list can only shrink. Normal (non-dev) dependencies only.

    tools/check-deps.py            # check
    tools/check-deps.py --print    # print every current violation (to refresh the list)
"""
import json, os, subprocess, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ALLOW = os.path.join(ROOT, "tools", "deps-allow.txt")

# Crates that are allowed to assemble everything: the platform entrypoints
# (and, after phase 4, `moonkale-distribution`).
APPS = {"web", "desktop", "mobile", "moonkale-distribution"}
DRIVERS = {"moonkale-sources-sql", "moonkale-sources-graph", "moonkale-sources-kv"}
# Heavy or native-only third-party crates the shell must not pull in.
HEAVY = {"duckdb", "lbug", "rocksdb", "turso", "helix-db", "wasmtime", "reqwest", "rusqlite", "redb"}
EXT_API_MAY = {"moonkale-core", "moonkale-lsp", "moonkale-terminal", "moonkale-ext-abi", "moonkale-llm-types"}


def metadata():
    out = subprocess.run(["cargo", "metadata", "--format-version", "1", "--locked"],
                         cwd=ROOT, check=True, capture_output=True, text=True).stdout
    return json.loads(out)


def violations(meta):
    members = set(meta["workspace_members"])
    pkgs = {p["id"]: p for p in meta["packages"]}
    name = {pid: pkgs[pid]["name"] for pid in pkgs}
    ws_names = {name[m] for m in members}
    # Workspace crates that are extensions: editors/* and extensions/*.
    extensions = {
        name[m] for m in members
        if "/packages/editors/" in pkgs[m]["manifest_path"] or "/packages/extensions/" in pkgs[m]["manifest_path"]
    }
    normal = {}
    for node in meta["resolve"]["nodes"]:
        deps = set()
        for d in node["deps"]:
            if any(k["kind"] is None for k in d["dep_kinds"]):
                deps.add(d["pkg"])
        normal[node["id"]] = deps

    def direct(pid):
        return {name[d] for d in normal[pid]}

    def closure(pid):
        seen, todo = set(), [pid]
        while todo:
            for d in normal[todo.pop()]:
                if d not in seen:
                    seen.add(d)
                    todo.append(d)
        return {name[d] for d in seen}

    found = set()
    for m in members:
        n = name[m]
        ds = direct(m)
        if n == "moonkale-core":
            for d in ds & ws_names:
                found.add(f"core-depends-on-nothing {n} -> {d}")
        if n == "moonkale-ext-api":
            for d in (ds & ws_names) - EXT_API_MAY:
                found.add(f"ext-api-is-a-contract {n} -> {d}")
        if n not in APPS:
            for d in ds & (DRIVERS | extensions):
                if d != n:
                    found.add(f"only-apps-assemble {n} -> {d}")
        if n in ("ui", "moonkale-shell"):
            for d in closure(m) & HEAVY:
                found.add(f"shell-stays-light {n} ~> {d}")
    return found


def main():
    found = violations(metadata())
    if "--print" in sys.argv:
        print("\n".join(sorted(found)))
        return 0
    allowed = set()
    if os.path.exists(ALLOW):
        for line in open(ALLOW):
            line = line.split("#", 1)[0].strip()
            if line:
                allowed.add(line)
    new, gone = sorted(found - allowed), sorted(allowed - found)
    for v in new:
        print(f"NEW VIOLATION: {v}")
    for v in gone:
        print(f"FIXED (remove it from tools/deps-allow.txt): {v}")
    print(f"{len(found)} known violation(s), {len(new)} new, {len(gone)} fixed")
    return 1 if new or gone else 0


if __name__ == "__main__":
    sys.exit(main())
