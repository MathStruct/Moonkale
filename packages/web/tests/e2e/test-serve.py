#!/usr/bin/env python3
"""Exercise startup recovery with real owned process trees and fake time/HTTP."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent


class Startup(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="moonkale-serve-")
        self.root = Path(self.tmp.name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.target = self.root / "target"
        self.target.mkdir()
        (self.root / "clock").write_text("1000")
        self.env = dict(os.environ, MOONKALE_E2E=str(self.root), CARGO_TARGET_DIR=str(self.target),
                        PORT="18892", CI="true", SERVE_ATTEMPTS="2", SERVE_WAIT="200", SERVE_STALL="10",
                        PATH=f"{self.bin}:{os.environ['PATH']}", TEST_ROOT=str(self.root), TEST_CASE="ready")
        self.stub("date", '''from pathlib import Path
import os
print((Path(os.environ["TEST_ROOT"]) / "clock").read_text())
''')
        self.stub("sleep", '''from pathlib import Path
import os, time
p = Path(os.environ["TEST_ROOT"]) / "clock"
p.write_text(str(int(p.read_text()) + 5))
time.sleep(0.15)
''')
        self.stub("curl", '''from pathlib import Path
import os
p = Path(os.environ["TEST_ROOT"]) / "attempts"
n = int(p.read_text()) if p.exists() else 0
case = os.environ["TEST_CASE"]
print("200" if case in ("ready", "build") or (case == "recover" and n == 2) else "000")
''')
        self.stub("dx", '''from pathlib import Path
import json, os, subprocess, sys, time
root = Path(os.environ["TEST_ROOT"])
p = root / "attempts"
n = int(p.read_text()) + 1 if p.exists() else 1
p.write_text(str(n))
(root / f"args-{n}").write_text(json.dumps(sys.argv[1:]))
child = subprocess.Popen([sys.executable, "-c", "import signal,time;signal.signal(signal.SIGTERM,signal.SIG_IGN);time.sleep(120)"])
with (root / "children").open("a") as f: f.write(str(child.pid) + "\\n")
if os.environ["TEST_CASE"] == "exit": sys.exit(3)
if os.environ["TEST_CASE"] == "build": print("Build failed", flush=True)
else: print("tooling ready", flush=True)
time.sleep(120)
''')
        self.unrelated = subprocess.Popen([sys.executable, "-c", "import time;time.sleep(120)"], start_new_session=True)

    def stub(self, name, script):
        path = self.bin / name
        path.write_text(f"#!{sys.executable}\n" + script)
        path.chmod(0o755)

    def run_server(self, action="start"):
        return subprocess.run([str(HERE / "serve.sh"), action], env=self.env,
                              capture_output=True, text=True, timeout=20)

    def attempts(self):
        path = self.root / "attempts"
        return int(path.read_text()) if path.exists() else 0

    def assert_stopped(self):
        self.assertFalse((self.root / "dx-18892.pid").exists())
        for line in (self.root / "children").read_text().splitlines():
            path = Path(f"/proc/{line}/stat")
            if path.exists():
                self.assertEqual(path.read_text().rsplit(")", 1)[1].split()[0], "Z")
        self.assertIsNone(self.unrelated.poll(), "unrelated server was signalled")

    def tearDown(self):
        self.run_server("stop")
        self.unrelated.terminate()
        self.unrelated.wait(timeout=5)
        self.tmp.cleanup()

    def test_ready_and_ci_flags(self):
        result = self.run_server()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.attempts(), 1)
        args = json.loads((self.root / "args-1").read_text())
        self.assertEqual(args[-6:], ["--interactive", "false", "--open", "false", "--verbose", "--locked"])
        self.run_server("stop")
        self.assert_stopped()

    def test_stall_retries_once_preserving_logs(self):
        self.env["TEST_CASE"] = "recover"
        result = self.run_server()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.attempts(), 2)
        self.assertTrue((self.root / "dx-18892.attempt-1.diagnostics.log").exists())
        self.assertIn("tooling ready", (self.root / "dx-18892.attempt-1.log").read_text())
        self.run_server("stop")
        self.assert_stopped()

    def test_repeated_stall_stops_after_two_attempts(self):
        self.env["TEST_CASE"] = "stall"
        self.assertNotEqual(self.run_server().returncode, 0)
        self.assertEqual(self.attempts(), 2)
        self.assert_stopped()

    def test_build_error_never_retries_even_if_http_answers(self):
        self.env["TEST_CASE"] = "build"
        self.assertNotEqual(self.run_server().returncode, 0)
        self.assertEqual(self.attempts(), 1)
        self.assert_stopped()

    def test_process_exit_never_retries_and_reaps_orphan_group(self):
        self.env["TEST_CASE"] = "exit"
        self.assertNotEqual(self.run_server().returncode, 0)
        self.assertEqual(self.attempts(), 1)
        self.assert_stopped()

    def test_deadline_is_shared_across_attempts(self):
        self.env.update(TEST_CASE="stall", SERVE_WAIT="20")
        result = self.run_server()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.attempts(), 2)
        self.assertLessEqual(int((self.root / "clock").read_text()), 1020)
        self.assert_stopped()

    def test_default_is_one_attempt(self):
        self.env.update(TEST_CASE="stall")
        del self.env["SERVE_ATTEMPTS"]
        self.assertNotEqual(self.run_server().returncode, 0)
        self.assertEqual(self.attempts(), 1)
        self.assert_stopped()

    def test_wrong_identity_cannot_signal_unrelated_process(self):
        (self.root / "dx-18892.owner").write_text(json.dumps({"pid": self.unrelated.pid, "birth": "not-the-owner"}))
        self.assertNotEqual(self.run_server("stop").returncode, 0)
        self.assertIsNone(self.unrelated.poll())

    def test_legacy_pid_without_identity_is_preserved_and_refused(self):
        pid = self.root / "dx-18892.pid"
        pid.write_text(str(self.unrelated.pid))
        self.assertNotEqual(self.run_server("stop").returncode, 0)
        self.assertTrue(pid.exists())
        self.assertIsNone(self.unrelated.poll())


if __name__ == "__main__":
    unittest.main()
