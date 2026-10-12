#!/usr/bin/env python3
"""OS input acceptance for the desktop fixture (Linux/XWayland + Klipper).

Pass the fixture's X window ID and MOONKALE_NATIVE_REPORT path. This sends
real X input, checks canonical Workspace text, and restores the text clipboard.
Run a single fixture instance. The default Dioxus renderer is accepted; no
external renderer overrides are required.
"""
import argparse
import json
import pathlib
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("window")
parser.add_argument("report", type=pathlib.Path)
parser.add_argument("--menu-height", type=int, default=27, help="GTK menu height in pixels")
args = parser.parse_args()


def xdo(*commands):
    subprocess.run(["xdotool", *commands], check=True)


def keys(*sequence):
    xdo("key", "--clearmodifiers", *sequence)


def expect(text):
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        if args.report.read_text() == text:
            return
        time.sleep(0.05)
    raise AssertionError(f"Workspace text mismatch: {args.report.read_text()!r} != {text!r}. "
                         "Check window mapping and input delivery before attributing this to rendering.")


bus = ["qdbus6", "org.kde.klipper", "/klipper", "org.kde.klipper.klipper."]
previous = subprocess.check_output(bus[:-1] + [bus[-1] + "getClipboardContents"], text=True)
# qdbus appends one output newline to the clipboard string.
previous = previous[:-1] if previous.endswith("\n") else previous
geometry = json.loads(pathlib.Path(str(args.report) + ".geometry.json").read_text())
window_pid = int(subprocess.check_output(["xdotool", "getwindowpid", args.window], text=True))
if window_pid != geometry.get("pid"):
    raise AssertionError("window and report belong to different fixture processes; use a fresh report and its PID")
x = str(round(geometry["x"]))
y = str(round(geometry["y"] + geometry["height"] / 2 + args.menu_height))
end_x = str(round(geometry["x"] + geometry["width"] * 4))
original = args.report.read_text()
assert "😀中" in original, "use the fresh Unicode fixture"
try:
    xdo("windowactivate", "--sync", args.window,
        "windowfocus", "--sync", args.window,
        "mousemove", "--window", args.window,
        str(round(geometry["x"] + geometry["width"] / 2)), y,
        "click", "1", "sleep", "0.3")
    keys("ctrl+Home")
    xdo("type", "--clearmodifiers", "--delay", "50", "nativeprobe")
    expect("nativeprobe" + original)
    # Replace a real keyboard selection; undo must restore both source and caret.
    keys("ctrl+Home", "shift+Right")
    xdo("type", "--clearmodifiers", "X")
    expect("Xativeprobe" + original)
    keys("ctrl+z")
    expect("nativeprobe" + original)
    text = args.report.read_text()
    # Copy, cut, paste use the real arboard system clipboard, including emoji.
    keys("ctrl+a", "ctrl+c", "Delete")
    expect("")
    keys("ctrl+v")
    expect(text)
    keys("ctrl+a", "ctrl+x")
    expect("")
    keys("ctrl+v")
    expect(text)
    keys("ctrl+z")
    expect("")
    keys("ctrl+z")
    expect(text)
    keys("ctrl+shift+z")
    expect("")
    keys("ctrl+shift+z")
    expect(text)
    # Toolbar wrapping changes after dirty state/selection changes.
    time.sleep(0.3)
    geometry = json.loads(pathlib.Path(str(args.report) + ".geometry.json").read_text())
    x = str(round(geometry["x"]))
    y = str(round(geometry["y"] + geometry["height"] / 2 + args.menu_height))
    end_x = str(round(geometry["x"] + geometry["width"] * 4))
    # Use the cell geometry observed by the native WebView, not screen guesses.
    for backward in (False, True):
        start, end = (end_x, x) if backward else (x, end_x)
        xdo("mousemove", "--window", args.window, start, y, "mousedown", "1",
            "sleep", "0.3", "mousemove", "--window", args.window, end, y,
            "sleep", "0.3", "mouseup", "1")
        xdo("type", "--clearmodifiers", "Q")
        expect("Q" + text[4:])
        keys("ctrl+z")
        expect(text)
    xdo("windowsize", args.window, "780", "600")
    keys("ctrl+End")
    xdo("type", "--clearmodifiers", "t")
    expect(text + "t")
    keys("ctrl+z")
    expect(text)
    print("PASS: OS pointer focus/drag selection, typing, keyboard selection, Unicode copy/cut/paste, undo/redo and resized input")
finally:
    subprocess.run(bus[:-1] + [bus[-1] + "setClipboardContents", previous], check=True)
