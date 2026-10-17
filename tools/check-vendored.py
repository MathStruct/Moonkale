#!/usr/bin/env python3
"""Check reviewed vendored bytes without network access. See Security.md (WP-47)."""
import hashlib
import json
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "tools/vendored-assets.json"


def inventory():
    paths = subprocess.check_output(
        ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"], cwd=ROOT
    ).decode().split("\0")
    return {p for p in paths if p and (
        "vendor" in pathlib.PurePosixPath(p).parts
        or p.startswith("packages/ext-api/assets/katex/")
    )}


def check():
    manifest = json.loads(MANIFEST.read_text())
    expected = {}
    for group in manifest["groups"]:
        if not all(group.get(k) for k in ("root", "version", "upstream", "provenance")):
            raise ValueError("Each group needs version, upstream URL and provenance")
        for relative, digest in group["sha256"].items():
            path = f'{group["root"]}/{relative}'
            if path in expected:
                raise ValueError(f"Duplicate manifest entry: {path}")
            expected[path] = digest
    actual = inventory()
    errors = [f"Unmanifested vendored file: {p}" for p in sorted(actual - expected.keys())]
    errors += [f"Missing vendored file: {p}" for p in sorted(expected.keys() - actual)]
    for path in sorted(actual & expected.keys()):
        file = ROOT / path
        if file.is_symlink() or not file.is_file():
            errors.append(f"Vendored path must be a regular file: {path}")
        elif hashlib.sha256(file.read_bytes()).hexdigest() != expected[path]:
            errors.append(f"Vendored checksum changed: {path}")
    if errors:
        raise SystemExit("\n".join(errors))
    print(f"Vendored assets PASS ({len(expected)} SHA-256 checks; no downloads)")


if __name__ == "__main__":
    check()
