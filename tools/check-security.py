#!/usr/bin/env python3
"""WP-47 dependency gates. No installs or automatic dependency upgrades.

Rust: deny.toml is the single RustSec exception list for deny AND audit.
JS: audit each tracked JS lockfile without installing packages/running scripts.
Registry/RustSec failures fail closed; only vendored checksum checks are offline.
"""
import argparse
import datetime
import json
import pathlib
import re
import subprocess
import tempfile
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[1]


def rust(deny="cargo-deny", audit="cargo-audit", db=None):
    config = (ROOT / "deny.toml").read_text()
    policy = tomllib.loads(config)
    ignores = []
    advisory_ids = set()
    yanked = set()
    for entry in policy["advisories"]["ignore"]:
        if not isinstance(entry, dict):
            raise ValueError("Every exception needs an ID/crate and dated reason")
        datetime.date.fromisoformat(entry["reason"][:10])
        if len(entry["reason"]) < 30:
            raise ValueError("RustSec exception reason is too short")
        if "id" in entry:
            if not re.fullmatch(r"RUSTSEC-\d{4}-\d{4}", entry["id"]):
                raise ValueError("Invalid RustSec advisory ID")
            ignores += ["--ignore", entry["id"]]
            advisory_ids.add(entry["id"])
        else:
            name, version = entry["crate"].split("@")
            yanked.add((name, version))
    # Keep machine-specific database paths out of the reviewed policy.
    with tempfile.TemporaryDirectory(prefix="moonkale-security-") as temporary:
        path = ROOT / "deny.toml"
        if db:
            path = pathlib.Path(temporary) / "deny.toml"
            config = config.replace("[advisories]", "[advisories]\ndb-path = " + json.dumps(str(pathlib.Path(db).resolve() / "deny")))
            path.write_text(config)
        deny_result = subprocess.run([
            deny, "--locked", "--config", str(path), "check", "--hide-inclusion-graph",
        ], cwd=ROOT)
        command = [audit, "audit", "--json", *ignores]
        if db:
            command += ["--db", str(pathlib.Path(db).resolve() / "audit")]
        audit_result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True)
        report = json.loads(audit_result.stdout)
        failed = audit_result.returncode != 0
        for item in report["vulnerabilities"]["list"]:
            print(f'cargo-audit: {item["advisory"]["id"]} {item["package"]["name"]}')
        for kind, items in report["warnings"].items():
            for item in items:
                package = item["package"]
                known = ((package["name"], package["version"]) in yanked if kind == "yanked"
                         else item.get("advisory", {}).get("id") in advisory_ids)
                print(f'cargo-audit: {kind}: {package["name"]} {package["version"]} '
                      f'({"dated exception" if known else "NEW"})')
                failed |= not known
        print(f'cargo-audit {"FAIL" if failed else "PASS"} (shared deny.toml exceptions)')
    return max(deny_result.returncode, int(failed))


def lockfiles():
    files = subprocess.check_output(
        ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"], cwd=ROOT
    ).decode().split("\0")
    return sorted({p for p in files if pathlib.PurePosixPath(p).name in (
        "package-lock.json", "npm-shrinkwrap.json", "pnpm-lock.yaml", "yarn.lock"
    )})


def js_findings(report):
    """Indirect via-name entries duplicate their leaf advisories; inspect leaves."""
    for package, item in report["vulnerabilities"].items():
        for via in item["via"]:
            if isinstance(via, dict):
                yield package, via


def javascript():
    baseline = json.loads((ROOT / "tools/npm-audit-exceptions.json").read_text())
    exceptions = {}
    for entry in baseline["exceptions"]:
        datetime.date.fromisoformat(entry["date"])
        if not entry["reason"]:
            raise ValueError("npm exception needs a reason")
        exceptions[(entry["lockfile"], entry["package"], entry["url"])] = entry
    failed = False
    for lock in lockfiles():
        print(f"Auditing {lock}", flush=True)
        if pathlib.Path(lock).name not in ("package-lock.json", "npm-shrinkwrap.json"):
            print(f"Unsupported lockfile {lock}: add a pinned auditor before merging")
            failed = True
            continue
        result = subprocess.run([
            "npm", "audit", "--omit=dev", "--audit-level=high", "--json",
        ], cwd=(ROOT / lock).parent, capture_output=True, text=True)
        try:
            report = json.loads(result.stdout)
        except ValueError:
            print(f"npm audit failed for {lock}: no JSON report")
            failed = True
            continue
        if result.returncode not in (0, 1) or report.get("error") or "vulnerabilities" not in report:
            print(f"npm audit failed for {lock}: registry/lockfile error")
            failed = True
            continue
        packages = json.loads((ROOT / lock).read_text())["packages"]
        for package, advisory in js_findings(report):
            key = (lock, package, advisory["url"])
            exception = exceptions.get(key)
            versions = {packages[node]["version"] for node in report["vulnerabilities"][package]["nodes"]}
            known = exception and versions <= set(exception["versions"])
            high = advisory["severity"] in ("high", "critical")
            print(f'  {advisory["severity"]}: {package}: {advisory["url"]} '
                  f'({"dated exception" if known else "new finding"})')
            if high and not known:
                failed = True
    return int(failed)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kind", choices=("rust", "js"))
    parser.add_argument("--cargo-deny", default="cargo-deny")
    parser.add_argument("--cargo-audit", default="cargo-audit")
    parser.add_argument("--db-root", help="Writable local directory for RustSec databases")
    args = parser.parse_args()
    return rust(args.cargo_deny, args.cargo_audit, args.db_root) if args.kind == "rust" else javascript()


if __name__ == "__main__":
    raise SystemExit(main())
