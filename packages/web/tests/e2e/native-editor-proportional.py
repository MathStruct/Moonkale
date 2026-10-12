"""Run native WebKit measured navigation and proportional DOM/layout probes (no OS input)."""
import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    args = parser.parse_args()
    running = subprocess.run(
        ["ps", "-C", "moonkale-native-editor-host", "-o", "pid="],
        capture_output=True, text=True, check=False,
    )
    if running.stdout.strip():
        raise RuntimeError("close the existing native fixture before running this probe")
    with tempfile.TemporaryDirectory(prefix="moonkale-proportional-") as directory:
        report = Path(directory) / "report.json"
        environment = dict(os.environ)
        for key in ("WEBKIT_DISABLE_COMPOSITING_MODE", "WEBKIT_DISABLE_DMABUF_RENDERER", "GDK_BACKEND", "MOONKALE_RENDERER_PROBE", "MOONKALE_NATIVE_REPORT"):
            environment.pop(key, None)
        environment["MOONKALE_PROPORTIONAL_PROBE"] = str(report)
        with (Path(directory) / "native.log").open("w") as log:
            process = subprocess.Popen([str(args.binary.resolve())], env=environment, stdout=log, stderr=log)
            try:
                deadline = time.monotonic() + 30
                while not report.exists():
                    if process.poll() is not None:
                        raise RuntimeError("native fixture exited before reporting")
                    if time.monotonic() >= deadline:
                        raise RuntimeError("native proportional geometry probe timed out")
                    time.sleep(0.1)
                result = json.loads(report.read_text())
                if not result.get("ok"):
                    raise RuntimeError(result.get("error", "native proportional probe failed"))
                print("PASS: native WebKit font invalidation, visual edges, wrap affinity, measured Up/Down, proportional geometry, pixel wrapping and synthetic DOM editing")
                print(result["note"])
            finally:
                if process.poll() is None:
                    process.send_signal(signal.SIGINT)
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.terminate()
                        try:
                            process.wait(timeout=2)
                        except subprocess.TimeoutExpired:
                            process.kill()
                            process.wait()


if __name__ == "__main__":
    main()
