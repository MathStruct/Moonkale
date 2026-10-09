#!/usr/bin/env python3
"""Real X11 input against measured wraps; DOM observation owns no editing state."""
import argparse
import json
import pathlib
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("window")
parser.add_argument("report", type=pathlib.Path)
parser.add_argument("result", type=pathlib.Path)
parser.add_argument("state", type=pathlib.Path)
parser.add_argument("--menu-height", type=int, default=27)
parser.add_argument("--ime", action="store_true", help="test configured Fcitx5 Pinyin and restore the active input method")
args = parser.parse_args()


def xdo(*commands):
    subprocess.run(["xdotool", *commands], check=True)


def keys(*sequence):
    xdo("key", "--clearmodifiers", *sequence)


def observe():
    try:
        return json.loads(args.state.read_text())["state"]
    except (FileNotFoundError, json.JSONDecodeError):
        return None


def wait(predicate, message):
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        state = observe()
        if state is not None and predicate(state):
            return state
        time.sleep(0.05)
    raise AssertionError(f"{message}: {observe()}")


def expect_text(text):
    return wait(lambda state: state["text"] == text and args.report.read_text() == text,
                "canonical text mismatch")


result = json.loads(args.result.read_text())
geometry = json.loads(pathlib.Path(str(args.report) + ".geometry.json").read_text())
owner = int(subprocess.check_output(["xdotool", "getwindowpid", args.window], text=True))
assert owner == geometry["pid"], "window and reports must belong to the same process"
edge = result["edge"]
original = result["text"]
assert edge > 0 and original.startswith("Wi 😀"), "use the measured navigation fixture"
xdo("windowactivate", "--sync", args.window, "windowfocus", "--sync", args.window,
    "mousemove", "--window", args.window,
    str(round(geometry["x"] + geometry["width"] * 0.2)),
    str(round(geometry["y"] + geometry["height"] / 2 + args.menu_height)), "click", "1")
keys("ctrl+Home")
start = wait(lambda state: state["offset"] == 0 and state["ready"] and state["caret"],
             "pointer/keyboard focus did not reach measured source")
top = start["caret"]["y"]
keys("End")
wait(lambda state: state["offset"] == edge and state["caret"] and abs(state["caret"]["y"] - top) < 2,
     "End did not retain backward wrap affinity")
keys("Right")
wait(lambda state: state["offset"] == edge and state["caret"] and state["caret"]["y"] > top + 10,
     "Right did not switch wrap affinity at the same source offset")
keys("Left")
wait(lambda state: state["offset"] == edge and state["caret"] and abs(state["caret"]["y"] - top) < 2,
     "Left did not return to the preceding visual row")
keys("Home")
wait(lambda state: state["offset"] == 0, "visual Home failed")
keys("Down")
wait(lambda state: state["offset"] > 0 and state["caret"] and state["caret"]["y"] > top + 10,
     "measured Down failed")
keys("Up")
wait(lambda state: state["offset"] == 0, "measured Up failed")
keys("shift+End")
xdo("type", "--clearmodifiers", "Q")
expect_text("Q" + original[edge:])
keys("ctrl+z")
expect_text(original)
keys("ctrl+Home")
xdo("type", "--clearmodifiers", "--delay", "50", "nativewrap")
expect_text("nativewrap" + original)
keys("ctrl+a")
xdo("type", "--clearmodifiers", "R")
expect_text("R")
keys("ctrl+z")
expect_text("nativewrap" + original)
print("PASS: OS proportional pointer focus, Home/End, wrap affinity, Up/Down, selection, typing and Workspace undo")
if args.ime:
    previous = subprocess.check_output(["fcitx5-remote", "-n"], text=True).strip()
    if not previous:
        raise RuntimeError("Fcitx5 did not report its active input method")
    base = args.report.read_text()
    before = observe()["compositions"]
    try:
        keys("ctrl+Home")
        wait(lambda state: state["offset"] == 0, "IME caret reset failed")
        subprocess.run(["fcitx5-remote", "-s", "pinyin"], check=True)
        deadline = time.monotonic() + 3
        while subprocess.check_output(["fcitx5-remote", "-n"], text=True).strip() != "pinyin":
            if time.monotonic() >= deadline:
                raise RuntimeError("configured Pinyin input method did not activate")
            time.sleep(0.05)
        xdo("key", "--clearmodifiers", "--delay", "150", "n", "i", "h", "a", "o")
        # Native WebKit's visible textarea control exposes no start/update
        # events here. Require source preservation rather than inventing preedit.
        wait(lambda state: state["text"] == base,
             "real IME preedit changed canonical source")
        keys("space")
        committed = wait(lambda state: not state["composing"] and state["compositions"] == before + 1
                         and state["text"].endswith(base) and state["text"] != base,
                         "real IME did not commit exactly once")["text"]
        prefix = committed[:-len(base)]
        assert prefix == "你好", f"expected one Pinyin candidate commit, received {prefix!r}"
        subprocess.run(["fcitx5-remote", "-s", previous], check=True)
        keys("ctrl+z")
        expect_text(base)
        print("PASS: real Fcitx5 Pinyin source preservation, single commit and Workspace undo (no start/update events on this WebKit)")
    finally:
        subprocess.run(["fcitx5-remote", "-s", previous], check=True)
