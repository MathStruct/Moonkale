#!/usr/bin/env python3
"""Real pointer source reveal, canonical typing and undo in Markdown preview."""
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
    raise AssertionError(f"{message}: {last}")


def xdo(*commands):
    subprocess.run(["xdotool", *commands], check=True)


geometry = json.loads(pathlib.Path(str(args.report) + ".geometry.json").read_text())
assert int(subprocess.check_output(["xdotool", "getwindowpid", args.window], text=True)) == geometry["pid"]
original = json.loads(args.result.read_text())["text"]
state = wait(lambda value: value["ready"] and value["count"] == 3, "Markdown geometry")
rect = state["rect"]
xdo("windowactivate", "--sync", args.window, "windowfocus", "--sync", args.window)
xdo("mousemove", "--window", args.window, str(round(rect["x"] + 2)),
    str(round(rect["y"] + rect["height"] / 2 + 27)), "click", "1")
start = len(original.replace("\r\n", "\n").split("*猫")[0])
wait(lambda value: value["offset"] == start and value["count"] == 2, "local source reveal")
xdo("type", "--clearmodifiers", "Q")
wait(lambda value: value["text"] == original.replace("*猫", "Q*猫", 1), "canonical Markdown typing")
xdo("key", "--clearmodifiers", "ctrl+z")
wait(lambda value: value["text"] == original, "Markdown undo")
xdo("key", "--clearmodifiers", "ctrl+End")
wait(lambda value: value["count"] == 3 and value["ready"], "preview restored")
print("PASS: real OS Markdown pointer reveal, source typing/undo and preview restoration")
