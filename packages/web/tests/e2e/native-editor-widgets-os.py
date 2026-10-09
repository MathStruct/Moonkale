#!/usr/bin/env python3
"""Real OS widget focus/input and return to canonical source, scoped to the owned window."""
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
args = parser.parse_args()


def wait(predicate, message):
    deadline = time.monotonic() + 5
    last = None
    while time.monotonic() < deadline:
        try:
            last = json.loads(args.state.read_text())["state"]
            if predicate(last):
                return last
        except (FileNotFoundError, json.JSONDecodeError):
            pass
        time.sleep(.05)
    if last is not None:
        last = {**last, "text": last["text"][:120]}
    raise AssertionError(f"{message}: {last}")


def xdo(*commands):
    subprocess.run(["xdotool", *commands], check=True)


def click(rect):
    xdo("mousemove", "--window", args.window, str(round(rect["x"] + rect["width"] / 2)),
        str(round(rect["y"] + rect["height"] / 2 + 27)), "click", "1")


geometry = json.loads(pathlib.Path(str(args.report) + ".geometry.json").read_text())
assert int(subprocess.check_output(["xdotool", "getwindowpid", args.window], text=True)) == geometry["pid"]
original = json.loads(args.result.read_text())["text"]
state = wait(lambda value: value["aligned"], "initial widget geometry")
xdo("windowactivate", "--sync", args.window, "windowfocus", "--sync", args.window)
click(state["noteRect"])
wait(lambda value: value["noteFocused"], "OS click did not focus widget input")
xdo("type", "--clearmodifiers", "localnote")
state = wait(lambda value: value["note"] == "localnote", "OS text did not reach local widget input")
assert state["text"] == original and args.report.read_text() == original
xdo("key", "--clearmodifiers", "Home", "Right", "End")
time.sleep(.1)
state = wait(lambda value: value["noteFocused"], "widget keys changed focus")
assert state["text"] == original
xdo("key", "--clearmodifiers", "Escape")
wait(lambda value: value["sourceFocused"] and value["offset"] == value["anchor"], "Escape did not restore source focus/anchor")
xdo("type", "--clearmodifiers", "Q")
wait(lambda value: value["text"] == original.replace("// row 1", "Q// row 1", 1), "source input after widget focus")
xdo("key", "--clearmodifiers", "ctrl+z")
state = wait(lambda value: value["text"] == original, "source undo after widget input")
assert state["note"] == ""  # A new source revision recreates this view-only widget.
click(state["sourceRect"])
wait(lambda value: value["sourceFocused"], "Edit source click did not restore focus")
print("PASS: real OS widget focus/local input/key isolation, Escape source anchor, source typing/undo and Edit source")
