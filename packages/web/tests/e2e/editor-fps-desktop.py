#!/usr/bin/env python3
"""Drive release desktop RAF measurements on this worktree's private Wayland display.

Uses compositor input only. The recorder observes the production panel; no JS
edits or input injection into the WebView. Screen coordinates target the fixed
1500x1100 fixture, and every result must confirm the intended interaction.
"""
import argparse
import json
import pathlib
import subprocess
import time

ROOT = pathlib.Path(__file__).resolve().parents[4]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--binary", type=pathlib.Path, required=True)
parser.add_argument("--corpus", type=pathlib.Path, required=True)
parser.add_argument("--report", type=pathlib.Path, required=True)
args = parser.parse_args()
args.binary = args.binary.resolve()
args.report = args.report.resolve()
GESTURES = ["typing", "held-arrows", "wheel", "small-scroll-deltas",
            "scrollbar-drag", "selection-drag", "caret-movement"]


def command(*words):
    return subprocess.run([str(ROOT / "tools/headless.sh"), *map(str, words)],
                          cwd=ROOT, check=True, capture_output=True, text=True)


def samples():
    try:
        return json.loads(args.report.read_text())
    except (FileNotFoundError, json.JSONDecodeError):
        return []


def move(x, y):
    command("move", round(x), round(y))


def drag(start, end):
    move(*start)
    time.sleep(.3)
    command("press")
    try:
        for step in range(1, 71):
            move(*(a + (b-a)*step/70 for a, b in zip(start, end)))
            time.sleep(.08)
    finally:
        command("release")


results = []
try:
    for name, preview in [("FpsSource.rs", False), ("FpsSource.rs", True), ("FpsThreeMb.txt", False)]:
        command("start", "1500x1100")
        command("run", f"MOONKALE_FPS_SOURCE={args.corpus.resolve() / name}",
                f"MOONKALE_FPS_REPORT={args.report}", args.binary)
        time.sleep(3)
        horizontal = name.endswith(".txt")
        if preview:
            command("click", 505, 148)
            time.sleep(1)
        for index, gesture in enumerate(GESTURES):
            # Reload the memory source, restoring edits and resetting history.
            command("click", 368, 148)
            time.sleep(1)
            command("click", 90, 230)
            command("key", "ctrl+Home")
            command("click", 75, 106)
            command("key", "Home", *(["Down"] * index), "Return")
            before = len(samples())
            command("click", 220, 106)
            if gesture in ("typing", "held-arrows"):
                command("click", 90, 230)
                command("key", "ctrl+Home")
                if gesture == "typing":
                    command("type", "x" * 90)
                else:
                    command("hold", "right", "5.6")
            elif gesture in ("wheel", "small-scroll-deltas"):
                move(750, 500)
                command("wheel", "horizontal" if horizontal else "vertical",
                        ".25" if gesture == "small-scroll-deltas" else "80", "70", ".08")
            elif gesture == "scrollbar-drag":
                drag((30, 1039), (1450, 1039)) if horizontal else drag((1487, 235), (1487, 1020))
            elif gesture == "selection-drag":
                drag((150, 230), (1400, 230)) if horizontal else drag((150, 250), (1400, 1000))
            else:
                for step in range(60):
                    command("click", 100 + step % 10 * 12, 230 if horizontal else 250 + step % 15 * 22)
                    time.sleep(.09)
            deadline = time.monotonic() + 46
            while len(samples()) <= before and time.monotonic() < deadline:
                time.sleep(.2)
            observed = samples()
            if len(observed) <= before:
                raise RuntimeError(f"No recorder result for {name}/{preview}/{gesture}")
            sample = observed[-1]
            if sample.get("gesture") != gesture or sample.get("preview") != preview or name not in sample.get("name", ""):
                command("shot", ROOT / "target/headless/fps-unexpected-case.png")
                raise RuntimeError(f"UI selected an unexpected case: {sample}")
            results.append(sample)
            print(json.dumps({"name": name, "preview": preview, "gesture": gesture,
                              **{key: sample.get(key) for key in ["frames", "p95", "max", "interacted", "passed"]}}), flush=True)
        command("shot", ROOT / f"target/headless/fps-{name}-{preview}.png")
        command("stop")
finally:
    command("stop")
    args.report.with_suffix(".summary.json").write_text(json.dumps({
        "platform": "headless-wayland", "compositor": "sway",
        "renderer": "pixman and Mesa software GL", "scope": "production native panel fixture",
        "cases": len(results), "passed": len(results) == 21 and all(row["passed"] for row in results),
        "results": results,
    }, indent=2))
if len(results) != 21 or not all(row["passed"] for row in results):
    raise SystemExit("DESKTOP FPS: GATE NOT MET; raw failures preserved")
print("DESKTOP FPS: PASS (isolated headless Wayland)")
