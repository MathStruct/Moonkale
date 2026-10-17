#!/usr/bin/env python3
"""Offline regression tests for dependency gates failing closed (WP-47)."""
import contextlib
import importlib.util
import io
import json
import pathlib
import subprocess
import tempfile
import unittest
from unittest.mock import patch


def module(name):
    spec = importlib.util.spec_from_file_location(name, pathlib.Path(__file__).with_name(name + ".py"))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


security = module("check-security")
vendored = module("check-vendored")


class DependencyGates(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = pathlib.Path(self.temporary.name)
        (self.root / "tools").mkdir()
        self.addCleanup(patch.stopall)
        patch.object(security, "ROOT", self.root).start()
        patch.object(security, "lockfiles", return_value=["package-lock.json"]).start()
        self.output = io.StringIO()
        self.redirect = contextlib.redirect_stdout(self.output)
        self.redirect.__enter__()
        self.addCleanup(self.redirect.__exit__, None, None, None)

    def npm(self, url="https://github.com/advisories/GHSA-old", version="1.0.0", error=False):
        baseline = {"exceptions": [{"lockfile": "package-lock.json", "package": "dep",
            "url": "https://github.com/advisories/GHSA-old", "versions": ["1.0.0"],
            "date": "2026-10-17", "reason": "Existing dependency; deferred."}]}
        (self.root / "tools/npm-audit-exceptions.json").write_text(json.dumps(baseline))
        (self.root / "package-lock.json").write_text(json.dumps({"packages": {
            "node_modules/dep": {"version": version}}}))
        report = {"vulnerabilities": {"dep": {"nodes": ["node_modules/dep"], "via": [
            {"severity": "high", "url": url}]}}}
        if error:
            report = {"error": {"code": "ENETUNREACH"}}
        with patch.object(security.subprocess, "run", return_value=subprocess.CompletedProcess([], 1, json.dumps(report), "")):
            return security.javascript()

    def test_existing_npm_advisory_has_explicit_exception(self):
        self.assertEqual(self.npm(), 0)

    def test_new_npm_advisory_blocks(self):
        self.assertEqual(self.npm(url="https://github.com/advisories/GHSA-new"), 1)

    def test_changed_npm_version_does_not_inherit_exception(self):
        self.assertEqual(self.npm(version="2.0.0"), 1)

    def test_npm_network_error_blocks(self):
        self.assertEqual(self.npm(error=True), 1)

    def rust(self, warning):
        (self.root / "deny.toml").write_text('''[advisories]
ignore = [
 {id="RUSTSEC-2026-0001", reason="2026-10-17: Existing advisory; upgrade deferred."},
 {crate="dep@1.0.0", reason="2026-10-17: Existing yanked crate; upgrade deferred."},
]
''')
        report = {"vulnerabilities": {"list": []}, "warnings": warning}
        with patch.object(security.subprocess, "run", side_effect=[
            subprocess.CompletedProcess([], 0),
            subprocess.CompletedProcess([], 0, json.dumps(report), ""),
        ]) as run:
            result = security.rust()
            self.assertIn("RUSTSEC-2026-0001", run.call_args.args[0])
            self.assertNotIn("--no-yanked", run.call_args.args[0])
            return result

    def test_known_yank_is_shared_with_audit(self):
        self.assertEqual(self.rust({"yanked": [{"package": {"name": "dep", "version": "1.0.0"}}]}), 0)

    def test_new_yank_blocks(self):
        self.assertEqual(self.rust({"yanked": [{"package": {"name": "dep", "version": "2.0.0"}}]}), 1)

    def test_new_informational_advisory_blocks(self):
        self.assertEqual(self.rust({"unsound": [{"package": {"name": "dep", "version": "1.0.0"},
            "advisory": {"id": "RUSTSEC-2026-0002"}}]}), 1)

    def test_new_rust_vulnerability_blocks(self):
        (self.root / "deny.toml").write_text('[advisories]\nignore = []\n')
        report = {"vulnerabilities": {"list": [{"advisory": {"id": "RUSTSEC-2026-0002"},
            "package": {"name": "dep"}}]}, "warnings": {}}
        with patch.object(security.subprocess, "run", side_effect=[
            subprocess.CompletedProcess([], 1),
            subprocess.CompletedProcess([], 1, json.dumps(report), ""),
        ]):
            self.assertEqual(security.rust(), 1)

    def test_vendored_new_file_and_tampering_block(self):
        import hashlib
        manifest = self.root / "tools/vendored-assets.json"
        (self.root / "vendor").mkdir()
        file = self.root / "vendor/file"
        file.write_bytes(b"reviewed")
        manifest.write_text(json.dumps({"groups": [{"root": "vendor", "version": "1.0",
            "upstream": "https://example.org/upstream", "provenance": "reviewed local fork",
            "sha256": {"file": hashlib.sha256(b"reviewed").hexdigest()}}]}))
        with patch.object(vendored, "ROOT", self.root), patch.object(vendored, "MANIFEST", manifest):
            with patch.object(vendored, "inventory", return_value={"vendor/file"}):
                vendored.check()
                file.write_bytes(b"tampered")
                with self.assertRaisesRegex(SystemExit, "checksum changed"):
                    vendored.check()
            with patch.object(vendored, "inventory", return_value={"vendor/file", "vendor/new"}):
                with self.assertRaisesRegex(SystemExit, "Unmanifested"):
                    vendored.check()


if __name__ == "__main__":
    unittest.main()
