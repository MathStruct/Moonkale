#!/usr/bin/env python3
"""Audit WP-66's optional BLAKE3 graph, pinned to workspace lock versions.

The broader workspace policy belongs to WP-47. This checks the new hashing
implementation and its dependencies without inventing exceptions for unrelated
legacy crates. Requires cargo-deny 0.20.2; build artifacts stay in target/.
"""
import argparse
import pathlib
import subprocess
import tomllib

root = pathlib.Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--cargo-deny", default="cargo-deny")
args = parser.parse_args()
packages = tomllib.loads((root / "Cargo.lock").read_text())["package"]

def pinned(name):
    found = [p for p in packages if p["name"] == name]
    if len(found) != 1:
        raise SystemExit(f"expected exactly one {name} version in Cargo.lock")
    return found[0]["version"]

probe = root / "target/content-id-dependency-audit"
(probe / "src").mkdir(parents=True, exist_ok=True)
# cc and libc predate WP-66. Pin them too, so audit resolution cannot drift
# to newer compiler/platform crates than the production lockfile uses.
manifest = '[package]\nname = "content-id-dependency-audit"\nversion = "0.0.0"\nedition = "2024"\nlicense = "MIT"\n[workspace]\n[dependencies]\n'
for name in ("blake3", "cc", "libc"):
    manifest += f'{name} = "={pinned(name)}"\n'
(probe / "Cargo.toml").write_text(manifest)
(probe / "src/lib.rs").write_text('// Dependency audit only; no production code.\n')
config = probe / "deny.toml"
config.write_text('[advisories]\n[licenses]\nallow = ["MIT", "Apache-2.0", "CC0-1.0"]\n[bans]\nmultiple-versions = "deny"\n[sources]\nunknown-registry = "deny"\nunknown-git = "deny"\n')
subprocess.run([args.cargo_deny, "--manifest-path", str(probe / "Cargo.toml"), "--config", str(config), "check"], cwd=root, check=True)
production = {(p["name"], p["version"], p.get("checksum")) for p in packages}
for package in tomllib.loads((probe / "Cargo.lock").read_text())["package"]:
    if package["name"] == "content-id-dependency-audit":
        continue
    if (package["name"], package["version"], package.get("checksum")) not in production:
        raise SystemExit(f"audit graph differs from production: {package['name']} {package['version']}")
print("Content-id dependency audit PASS; every audited crate version/checksum matches Cargo.lock")
