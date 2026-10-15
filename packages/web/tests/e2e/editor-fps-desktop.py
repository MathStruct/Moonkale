#!/usr/bin/env python3
"""Drive release desktop RAF measurements on this worktree's private Wayland display.

Uses compositor input only. The recorder observes the production panel; no JS
edits or input injection into the WebView. Screen coordinates target the fixed
1500x1100 fixture, and every result must confirm the intended interaction.
"""
import argparse
import json
import os
import pathlib
import subprocess
import time

ROOT = pathlib.Path(__file__).resolve().parents[4]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--binary", type=pathlib.Path, required=True)
parser.add_argument("--corpus", type=pathlib.Path, required=True)
parser.add_argument("--report", type=pathlib.Path, required=True)
parser.add_argument("--gesture", choices=["typing", "held-arrows", "wheel", "small-scroll-deltas", "scrollbar-drag", "selection-drag", "caret-movement"], help="Diagnostic subset only; does not establish the full matrix")
args = parser.parse_args()
args.binary = args.binary.resolve()
args.report = args.report.resolve()
GESTURES = ["typing", "held-arrows", "wheel", "small-scroll-deltas",
            "scrollbar-drag", "selection-drag", "caret-movement"]


def command(*words):
    return subprocess.run([str(ROOT / "tools/headless.sh"), *map(str, words)],
                          cwd=ROOT, check=True, capture_output=True, text=True)


def idle_cpu_sample(label, seconds=1.2):
    # Locks coordinate agents, but cannot account for unrelated host jobs.
    # Sample real utilisation rather than load averages left over from a build.
    def cpu_ticks():
        with open("/proc/stat") as stats:
            values = list(map(int, stats.readline(512).split()[1:9]))
        if len(values) != 8:
            raise RuntimeError("Cannot validate quiet-machine CPU accounting")
        return sum(values), values[3] + values[4]
    consecutive = 0
    for attempt in range(6):
        before = cpu_ticks()
        time.sleep(seconds)
        after = cpu_ticks()
        elapsed = after[0] - before[0]
        if elapsed <= 0:
            raise RuntimeError("Cannot validate quiet-machine CPU accounting")
        active = max(0.0, min(1.0, 1 - (after[1]-before[1])/elapsed))
        quiet = active <= .10
        cpu_samples.append({"phase": label, "attempt": attempt + 1,
                            "activeFraction": active, "quiet": quiet})
        consecutive = consecutive + 1 if quiet else 0
        if consecutive >= 2:
            return
    raise RuntimeError(f"Host did not settle below 10% CPU for two consecutive samples (last {active:.1%}); no timing acceptance")


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


expected_cases = 3 if args.gesture else 21
results = []
cpu_samples = []
try:
    for name, preview in [("FpsSource.rs", False), ("FpsSource.rs", True), ("FpsThreeMb.txt", False)]:
        idle_cpu_sample(f"before {name}/{preview}", 1.0)
        command("start", "1500x1100")
        command("run", f"MOONKALE_FPS_SOURCE={args.corpus.resolve() / name}",
                f"MOONKALE_FPS_REPORT={args.report}", args.binary)
        time.sleep(3)
        horizontal = name.endswith(".txt")
        if preview:
            command("click", 505, 148)
            time.sleep(1)
        for index, gesture in enumerate(GESTURES):
            if args.gesture and gesture != args.gesture:
                continue
            # Reload the memory source, restoring edits and resetting history.
            command("click", 464 if horizontal else 368, 148)
            time.sleep(1)
            command("click", 90, 230)
            command("key", "ctrl+Home")
            command("click", 75, 106)
            command("key", "Home", *(["Down"] * index), "Return")
            before = len(samples())
            command("click", 220, 106)
            # Gather an idle refresh baseline before the first intended input.
            idle_cpu_sample(f"{name}/{preview}/{gesture}")
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
                command("shot", ROOT / "target/headless/fps-missing-result.png")
                raise RuntimeError(f"No recorder result for {name}/{preview}/{gesture}")
            sample = observed[-1]
            if sample.get("gesture") != gesture or sample.get("preview") != preview or name not in sample.get("name", ""):
                command("shot", ROOT / "target/headless/fps-unexpected-case.png")
                raise RuntimeError(f"UI selected an unexpected case: {sample}")
            results.append(sample)
            print(json.dumps({"name": name, "preview": preview, "gesture": gesture,
                              **{key: sample.get(key) for key in ["frames", "p95", "max", "quantized", "gate", "refreshInterval", "droppedFrames", "strictPassed", "interacted", "passed"]}}), flush=True)
        command("shot", ROOT / f"target/headless/fps-{name}-{preview}.png")
        command("stop")
finally:
    command("stop")
    args.report.with_suffix(".summary.json").write_text(json.dumps({
        "platform": "headless-wayland", "compositor": "sway",
        "renderer": "GPU requested (verify driver)" if os.environ.get("HEADLESS_GPU") == "1" else "pixman and Mesa software GL", "softwareThreads": os.environ.get("HEADLESS_GL_THREADS", "Mesa default"), "scope": "production native panel fixture",
        "cpuSamples": cpu_samples, "maximumIdleCpuFraction": .10,
        "requiredQuietSamples": 2, "maximumQuietAttempts": 6,
        "cases": len(results), "expectedCases": expected_cases, "completeMatrix": len(results) == 21,
        "passed": len(results) == expected_cases and all(row["passed"] for row in results),
        "strictPassed": len(results) == expected_cases and all(row.get("strictPassed", False) for row in results),
        "acceptance": "software GL is necessary, not sufficient; real desktop check remains open",
        "results": results,
    }, indent=2))
if len(results) != expected_cases or not all(row["passed"] for row in results):
    raise SystemExit("DESKTOP FPS: GATE NOT MET; raw failures preserved")
print(f"DESKTOP FPS: PASS ({len(results)} isolated headless cases; complete matrix: {len(results) == 21})")
