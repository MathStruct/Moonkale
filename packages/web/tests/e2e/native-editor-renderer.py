#!/usr/bin/env python3
"""Single-instance native renderer probe; optional PID-scoped OS input/capture.

The DOM probe verifies Dioxus's native adapter and layout, not real OS/IME input.
Dioxus's own Linux defaults remain enabled; external renderer overrides are removed.
"""
import argparse
import json
import os
import pathlib
import signal
import subprocess
import tempfile
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("binary", type=pathlib.Path)
parser.add_argument("--os-input", action="store_true")
parser.add_argument("--markdown", action="store_true", help="test bounded Markdown preview and canonical editing")
parser.add_argument("--widgets", action="store_true", help="test interactive block widget focus, input isolation and source return")
parser.add_argument("--proportional", action="store_true", help="use measured wraps for OS-input acceptance")
parser.add_argument("--presentation", action="store_true", help="test synthetic replacement/widget source mapping with OS input")
parser.add_argument("--multiline", action="store_true", help="use multiline replacements with --presentation")
parser.add_argument("--consumed-runs", action="store_true", help="use long replacement ranges with --presentation")
parser.add_argument("--ime", action="store_true", help="test Fcitx5 Pinyin composition with --os-input --proportional")
parser.add_argument("--x11", action="store_true",
                    help="select GTK X11 before launch for xdotool acceptance; separate from default-renderer acceptance")
parser.add_argument("--screenshot", type=pathlib.Path)
parser.add_argument("--isolate", action="store_true", help="compare visible textarea/editor IME and inspect unready geometry")
args = parser.parse_args()
if args.markdown and (args.widgets or args.presentation or args.proportional or args.isolate or args.ime):
    parser.error("--markdown runs separately from other presentation modes")
if args.widgets and (args.presentation or args.proportional or args.isolate or args.ime):
    parser.error("--widgets runs separately from proportional, presentation, isolation and IME probes")
if args.multiline and (not args.presentation or args.consumed_runs):
    parser.error("--multiline requires --presentation and excludes --consumed-runs")
if args.consumed_runs and not args.presentation:
    parser.error("--consumed-runs requires --presentation")
if args.ime and not (args.os_input and (args.proportional or args.presentation)):
    parser.error("--ime requires --os-input and --proportional or --presentation")
existing = subprocess.run(["ps", "-C", "moonkale-native-editor-host", "-o", "pid="],
                          capture_output=True, text=True)
if existing.stdout.strip():
    raise SystemExit("Close existing native-editor fixture instances before this isolated probe.")

with tempfile.TemporaryDirectory(prefix="moonkale-renderer-") as directory:
    directory = pathlib.Path(directory)
    result_path = directory / "result.json"
    report_path = directory / "workspace.txt"
    env = os.environ.copy()
    for name in ("WEBKIT_DISABLE_COMPOSITING_MODE", "WEBKIT_DISABLE_DMABUF_RENDERER", "GDK_BACKEND"):
        env.pop(name, None)
    if args.x11:
        env["GDK_BACKEND"] = "x11"
    env["MOONKALE_RENDERER_PROBE"] = str(result_path)
    env["MOONKALE_NATIVE_REPORT"] = str(report_path)
    env.pop("MOONKALE_OS_PROPORTIONAL", None)
    env.pop("MOONKALE_OS_MULTILINE", None)
    if args.multiline:
        env["MOONKALE_OS_MULTILINE"] = "1"
    env.pop("MOONKALE_OS_CONSUMED_RUNS", None)
    if args.consumed_runs:
        env["MOONKALE_OS_CONSUMED_RUNS"] = "1"
    env.pop("MOONKALE_OS_PRESENTATION", None)
    if args.presentation:
        env["MOONKALE_OS_PRESENTATION"] = "1"
    env.pop("MOONKALE_ISOLATION_PROBE", None)
    if args.isolate:
        env["MOONKALE_ISOLATION_PROBE"] = "1"
    if args.proportional:
        env["MOONKALE_OS_PROPORTIONAL"] = "1"
    env.pop("MOONKALE_WIDGET_PROBE", None)
    if args.widgets:
        env["MOONKALE_WIDGET_PROBE"] = "1"
    env.pop("MOONKALE_MARKDOWN_PROBE", None)
    if args.markdown:
        env["MOONKALE_MARKDOWN_PROBE"] = "1"
    previous_ime = None
    if args.ime or args.isolate:
        env["GTK_IM_MODULE"] = "fcitx"
        env["XMODIFIERS"] = "@im=fcitx"
        previous_ime = subprocess.check_output(["fcitx5-remote", "-n"], text=True).strip()
    with (directory / "runtime.log").open("w+") as log:
        process = subprocess.Popen([str(args.binary.resolve())], env=env, stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 20
            while not result_path.exists():
                if process.poll() is not None:
                    log.seek(0)
                    raise RuntimeError(f"fixture exited with {process.returncode}: " + log.read()[-2000:])
                if time.monotonic() >= deadline:
                    log.seek(0)
                    raise TimeoutError("native renderer did not complete initialization: " + log.read()[-2000:])
                time.sleep(0.1)
            result = json.loads(result_path.read_text())
            if not result.get("ok"):
                raise AssertionError(result)
            # The DOM probe resizes the panel after its last text mutation.
            # Text-triggered reports can therefore describe the old toolbar layout.
            geometry = result.get("geometry")
            if geometry is not None:
                geometry["pid"] = process.pid
                pathlib.Path(str(report_path) + ".geometry.json").write_text(json.dumps(geometry))
            backend = "explicit GTK X11" if args.x11 else "default Dioxus"
            checks = "Markdown styles/source mapping (DOM probe)" if args.markdown else "interactive widget focus/input, resize and source return (DOM probe)" if args.widgets else "isolation fixture initialized" if args.isolate else "focus, editing, selection, undo and resize (DOM probe)"
            print(f"PASS: {backend} native {checks}", flush=True)
            if args.os_input or args.screenshot:
                # Titles are insufficient: a stale/different fixture can share them.
                window = None
                deadline = time.monotonic() + 3
                while window is None and time.monotonic() < deadline:
                    found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(process.pid)],
                                           capture_output=True, text=True)
                    for candidate in found.stdout.split():
                        owner = subprocess.run(["xdotool", "getwindowpid", candidate],
                                               capture_output=True, text=True)
                        if owner.returncode == 0 and int(owner.stdout) == process.pid:
                            window = candidate
                            break
                    if window is None:
                        time.sleep(0.1)
                if window is None:
                    raise RuntimeError("owned native window was not found on the X display; DOM probe passed but "
                                       "OS/pixel acceptance did not run. A Wayland window is not discoverable by "
                                       "xdotool; use --x11 for separate XWayland acceptance.")
                if args.os_input:
                    try:
                        script = "native-editor-markdown-os.py" if args.markdown else "native-editor-widgets-os.py" if args.widgets else "native-editor-isolation.py" if args.isolate else "native-editor-presentation-os.py" if args.presentation else "native-editor-proportional-os.py" if args.proportional else "native-editor-desktop.py"
                        command = ["python3", str(pathlib.Path(__file__).with_name(script)), window, str(report_path)]
                        if args.proportional or args.presentation or args.isolate or args.widgets or args.markdown:
                            command.extend([str(result_path), str(result_path) + ".state.json"])
                            if args.ime:
                                command.append("--ime")
                        subprocess.run(command, check=True, timeout=60)
                    except (subprocess.CalledProcessError, subprocess.TimeoutExpired):
                        events_path = pathlib.Path(str(result_path) + ".events.json")
                        observed = events_path.read_text() if events_path.exists() else "no trusted DOM input events observed"
                        print(f"OS-input delivery observations: {observed}", flush=True)
                        log.seek(0)
                        print("Native runtime log: " + log.read()[-4000:], flush=True)
                        raise
                if args.screenshot:
                    subprocess.run(["magick", "import", "-window", window, str(args.screenshot.resolve())],
                                   check=True, timeout=10)
                    print(f"Captured owned fixture: {args.screenshot}", flush=True)
        finally:
            if previous_ime:
                subprocess.run(["fcitx5-remote", "-s", previous_ime], check=True)
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
