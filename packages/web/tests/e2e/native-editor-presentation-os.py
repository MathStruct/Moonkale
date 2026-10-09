#!/usr/bin/env python3
"""Real XWayland replacement-boundary focus, canonical copy and optional Pinyin."""
import argparse
import json
import pathlib
import subprocess
import time

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument("window")
parser.add_argument("report",type=pathlib.Path)
parser.add_argument("result",type=pathlib.Path)
parser.add_argument("state",type=pathlib.Path)
parser.add_argument("--ime",action="store_true")
args=parser.parse_args()


def xdo(*commands):
    subprocess.run(["xdotool",*commands],check=True)


def read_state():
    try:
        return json.loads(args.state.read_text())["state"]
    except (FileNotFoundError,json.JSONDecodeError):
        return None


def wait(predicate,message):
    deadline=time.monotonic()+5
    while time.monotonic()<deadline:
        state=read_state()
        if state is not None and predicate(state):
            return state
        time.sleep(.05)
    raise AssertionError(f"{message}: {read_state()}")


bus=["qdbus6","org.kde.klipper","/klipper","org.kde.klipper.klipper."]


def clipboard():
    value=subprocess.check_output(bus[:-1]+[bus[-1]+"getClipboardContents"],text=True)
    return value[:-1] if value.endswith("\n") else value


result=json.loads(args.result.read_text())
geometry=json.loads(pathlib.Path(str(args.report)+".geometry.json").read_text())
owner=int(subprocess.check_output(["xdotool","getwindowpid",args.window],text=True))
assert owner==geometry["pid"],"stale window/report"
original=result["text"]
assert "[[hidden" in original and "[[chip" in original
saved_clipboard=clipboard()
previous_ime=None
try:
    for fraction in (.2,.8):
        state=wait(lambda state:state["ready"] and len(state["widgets"])==1,"preview did not become ready")
        widget=state["widgets"][0]
        expected=widget["start"] if fraction<.5 else widget["end"]
        xdo("windowactivate","--sync",args.window,"windowfocus","--sync",args.window,
            "mousemove","--window",args.window,str(round(widget["x"]+widget["width"]*fraction)),
            str(round(widget["y"]+widget["height"]/2+27)),"click","1")
        wait(lambda state:state["offset"]==expected and not state["widgets"] and "[[hidden" not in state["visible"] and state["ready"],"replacement click did not reveal source at its boundary")
        xdo("type","--clearmodifiers","Q")
        changed=original[:expected]+"Q"+original[expected:]
        wait(lambda state:state["text"]==changed,"typing at replacement boundary failed")
        xdo("key","--clearmodifiers","ctrl+z")
        wait(lambda state:state["text"]==original,"replacement boundary undo failed")
        xdo("key","--clearmodifiers","ctrl+End")
    xdo("key","--clearmodifiers","ctrl+a")
    wait(lambda state:not state["widgets"] and state["ready"],"selection did not reveal source")
    xdo("key","--clearmodifiers","ctrl+c")
    deadline=time.monotonic()+5
    while clipboard()!=original:
        if time.monotonic()>=deadline:
            raise AssertionError("copy must include canonical hidden/replaced markup")
        time.sleep(.05)
    print("PASS: real OS replacement boundaries, source reveal, typing/undo and canonical markup copy",flush=True)
    if args.ime:
        xdo("key","--clearmodifiers","ctrl+End")
        state=wait(lambda state:len(state["widgets"])==1 and state["ready"],"IME preview failed")
        widget=state["widgets"][0]
        ime_offset=widget["start"]
        xdo("mousemove","--window",args.window,str(round(widget["x"]+widget["width"]*.2)),str(round(widget["y"]+widget["height"]/2+27)),"click","1")
        state=wait(lambda state:state["offset"]==ime_offset and not state["widgets"] and state["ready"],"IME construct reveal failed")
        before=state["compositions"]
        previous_ime=subprocess.check_output(["fcitx5-remote","-n"],text=True).strip()
        subprocess.run(["fcitx5-remote","-s","pinyin"],check=True)
        deadline=time.monotonic()+3
        while subprocess.check_output(["fcitx5-remote","-n"],text=True).strip()!="pinyin":
            if time.monotonic()>=deadline:
                raise RuntimeError("Pinyin did not activate")
            time.sleep(.05)
        xdo("key","--clearmodifiers","--delay","150","n","i","h","a","o")
        wait(lambda state:state["text"]==original and not state["widgets"] and "[[hidden" not in state["visible"],"IME preedit changed or hid source")
        xdo("key","--clearmodifiers","space")
        wait(lambda state:state["text"]==original[:ime_offset]+"你好"+original[ime_offset:] and state["compositions"]==before+1,"IME commit did not insert exactly once")
        if previous_ime:
            subprocess.run(["fcitx5-remote","-s",previous_ime],check=True)
        xdo("key","--clearmodifiers","ctrl+z")
        wait(lambda state:state["text"]==original,"IME undo failed")
        print("PASS: real Pinyin source preservation, single commit and undo with presentation enabled",flush=True)
finally:
    if previous_ime:
        subprocess.run(["fcitx5-remote","-s",previous_ime],check=True)
    subprocess.run(bus[:-1]+[bus[-1]+"setClipboardContents",saved_clipboard],check=True)
