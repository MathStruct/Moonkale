#!/usr/bin/env python3
"""Compare trusted Fcitx input in a plain textarea and Moonkale's input sink."""
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


def read_state():
    for _ in range(20):
        try:
            return json.loads(args.state.read_text())["state"]
        except (FileNotFoundError, json.JSONDecodeError):
            time.sleep(0.05)
    raise RuntimeError("isolation state unavailable")


def xdo(*commands):
    subprocess.run(["xdotool", *commands], check=True)


def method(name):
    subprocess.run(["fcitx5-remote", "-s", name], check=True)
    for _ in range(30):
        if subprocess.check_output(["fcitx5-remote", "-n"], text=True).strip() == name:
            return
        time.sleep(0.1)
    raise RuntimeError(f"input method {name} did not activate")


result = json.loads(args.result.read_text())
geometry = json.loads(pathlib.Path(str(args.report) + ".geometry.json").read_text())
owner = int(subprocess.check_output(["xdotool", "getwindowpid", args.window], text=True))
assert owner == geometry["pid"], "stale window/report"
print("Initial measured geometry: " + json.dumps(result["initial"]["geometry"]), flush=True)
previous = subprocess.check_output(["fcitx5-remote", "-n"], text=True).strip()
outcomes = []
try:
    for name, box in (("reference", result["reference"]), ("editor", geometry)):
        xdo("windowactivate", "--sync", args.window, "windowfocus", "--sync", args.window,
            "mousemove", "--window", args.window,
            str(round(box["x"] + min(20, box["width"] * 0.2))),
            str(round(box["y"] + box["height"] / 2 + 27)), "click", "1")
        time.sleep(0.3)
        if not previous:
            previous = subprocess.check_output(["fcitx5-remote", "-n"], text=True).strip()
        method("keyboard-de")
        field = "reference" if name == "reference" else "text"
        baseline = read_state()[field]
        xdo("key", "--clearmodifiers", "q")
        time.sleep(1)
        ascii_state = read_state()
        delivered = ascii_state[field] != baseline
        if not delivered:
            outcomes.append({"target": name, "asciiDelivered": False, "events": ascii_state["events"][-8:]})
            continue
        if name == "reference":
            xdo("key", "--clearmodifiers", "ctrl+a", "BackSpace")
        else:
            xdo("key", "--clearmodifiers", "ctrl+z", "ctrl+Home")
        time.sleep(0.5)
        baseline = read_state()[field]
        count = len(read_state()["events"])
        method("pinyin")
        xdo("key", "--clearmodifiers", "--delay", "150", "n", "i", "h", "a", "o")
        time.sleep(1)
        preedit = read_state()
        xdo("key", "--clearmodifiers", "space")
        time.sleep(1)
        committed = read_state()
        outcomes.append({"target":name,"asciiDelivered":True,
            "preeditActive":preedit["referenceComposing" if name == "reference" else "composing"],
            "sourceUnchangedDuringPreedit":preedit[field] == baseline,
            "committedPrefix":committed[field][:30],
            "sourceChangedAfterCommit":committed[field] != baseline,
            "events":committed["events"][count:]})
        if name == "editor" and committed[field].startswith("你好"):
            assert committed[field] == "你好" + baseline, "IME commit must insert once"
            method("keyboard-de")
            xdo("key", "--clearmodifiers", "ctrl+z")
            time.sleep(0.5)
            assert read_state()[field] == baseline, "IME undo must restore the source"
finally:
    if previous:
        method(previous)
print("IME isolation: " + json.dumps(outcomes, ensure_ascii=False), flush=True)
print("Final measured geometry: " + json.dumps(read_state()["geometry"]), flush=True)
