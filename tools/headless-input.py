#!/usr/bin/env python3
"""Input for a headless sway (tools/headless.sh): one virtual pointer and one
virtual keyboard that stay attached while this runs.

Why not wlrctl and wtype alone: each call adds a device and removes it right
after, and the app loses events while devices and keymaps come and go (clicks
vanish, typed letters drop out at random). Why not wayvnc: its screen capture
crashes WebKit on the NVIDIA driver.

Speaks the Wayland wire protocol directly (standard library only).

  headless-input.py serve SOCKET    keep the devices, take commands on SOCKET
  headless-input.py send SOCKET CMD  run one command and wait for it:
      type TEXT        type printable ASCII text (US keymap)
      key COMBO…       press keys, e.g. Return, Tab, ctrl+a, ctrl+shift+p
      hold KEY SECONDS hold one key for up to 10 seconds
      wheel AXIS AMOUNT COUNT GAP  bounded pixel scrolling, up to 15 seconds
      motion X Y WIDTH HEIGHT     absolute pointer position within output
      button press|release        left pointer button

Clicks go through sway's own `seat - cursor` commands, which need the pointer
this keeps attached.
"""
import os
import selectors
import socket
import struct
import subprocess
import sys
import time

# Wayland object ids, allocated by us. The server accepts new ids only in
# order, so they are created in exactly this order.
DISPLAY, REGISTRY, CALLBACK, SEAT, PTR_MGR, KBD_MGR, POINTER, KEYBOARD = range(1, 9)

# Linux evdev key codes for a US layout.
KEYS = {c: k for k, c in zip(
    [30, 48, 46, 32, 18, 33, 34, 35, 23, 36, 37, 38, 50, 49, 24, 25, 16, 19, 31, 20, 22, 47, 17, 45, 21, 44],
    "abcdefghijklmnopqrstuvwxyz")}
KEYS.update({str(d): 2 + (d - 1) % 10 for d in range(10)})
KEYS.update({"-": 12, "=": 13, "[": 26, "]": 27, ";": 39, "'": 40, "`": 41, "\\": 43,
             ",": 51, ".": 52, "/": 53, " ": 57})
SHIFTED = dict(zip('!@#$%^&*()_+{}:"~|<>?', "1234567890-=[];'`\\,./"))
SHIFTED.update({c.upper(): c for c in "abcdefghijklmnopqrstuvwxyz"})
NAMED = {"return": 28, "enter": 28, "tab": 15, "backspace": 14, "escape": 1, "esc": 1,
         "delete": 111, "home": 102, "end": 107, "up": 103, "down": 108, "left": 105,
         "right": 106, "pageup": 104, "pagedown": 109, "space": 57, "insert": 110,
         **{f"f{i}": 58 + i for i in range(1, 11)}, "f11": 87, "f12": 88}
# Seconds between key events. Faster typing loses keys in the app's controlled
# inputs (a round trip through WebKit per key), so this is about human speed.
GAP = float(os.environ.get("HEADLESS_KEY_GAP", "0.03"))

# Modifier key code and its bit in the keymap's modifier mask.
MODS = {"shift": (42, 1), "ctrl": (29, 4), "control": (29, 4), "alt": (56, 8),
        "super": (125, 64), "logo": (125, 64)}


def msg(obj, opcode, payload=b""):
    return struct.pack("<II", obj, (8 + len(payload)) << 16 | opcode) + payload


def string(s):
    s += b"\0"
    return struct.pack("<I", len(s)) + s + b"\0" * (-len(s) % 4)


class Wayland:
    def __init__(self):
        name = os.environ.get("WAYLAND_DISPLAY") or sys.exit("headless-input: WAYLAND_DISPLAY unset")
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.connect(name if name.startswith("/") else os.path.join(os.environ["XDG_RUNTIME_DIR"], name))
        self.buf = b""
        self.next_id = KEYBOARD + 1

    def events(self):
        data = self.sock.recv(4096)
        if not data:
            sys.exit("headless-input: compositor went away")
        self.buf += data
        while len(self.buf) >= 8:
            obj, word = struct.unpack("<II", self.buf[:8])
            size = word >> 16
            if len(self.buf) < size:
                break
            body, self.buf = self.buf[8:size], self.buf[size:]
            if obj == DISPLAY and word & 0xFFFF == 0:
                eobj, code, n = struct.unpack("<III", body[:12])
                sys.exit(f"headless-input: protocol error on object {eobj}, code {code}: {body[12:12 + n - 1].decode()}")
            yield obj, word & 0xFFFF, body

    def roundtrip(self):
        cb = self.next_id
        self.next_id += 1
        self.sock.sendall(msg(DISPLAY, 0, struct.pack("<I", cb)))
        while True:
            for obj, op, _ in self.events():
                if obj == cb and op == 0:
                    return

    def setup(self):
        self.sock.sendall(msg(DISPLAY, 1, struct.pack("<I", REGISTRY)) + msg(DISPLAY, 0, struct.pack("<I", CALLBACK)))
        globals_, done = {}, False
        while not done:
            for obj, op, body in self.events():
                if obj == REGISTRY and op == 0:
                    gname, length = struct.unpack("<II", body[:8])
                    iface = body[8:8 + length - 1]
                    version = struct.unpack("<I", body[8 + length + (-length % 4):][:4])[0]
                    globals_.setdefault(iface, (gname, version))
                if obj == CALLBACK and op == 0:
                    done = True
        need = [b"wl_seat", b"zwlr_virtual_pointer_manager_v1", b"zwp_virtual_keyboard_manager_v1"]
        missing = [n.decode() for n in need if n not in globals_]
        if missing:
            sys.exit(f"headless-input: compositor lacks {', '.join(missing)}")
        out = b""
        for iface, oid in zip(need, (SEAT, PTR_MGR, KBD_MGR)):
            gname, _ = globals_[iface]
            out += msg(REGISTRY, 0, struct.pack("<I", gname) + string(iface) + struct.pack("<II", 1, oid))
        out += msg(PTR_MGR, 0, struct.pack("<II", 0, POINTER))  # seat null: the default seat
        out += msg(KBD_MGR, 0, struct.pack("<II", SEAT, KEYBOARD))
        self.sock.sendall(out)
        self.roundtrip()
        keymap = subprocess.run(["xkbcli", "compile-keymap", "--layout", "us"],
                                check=True, capture_output=True).stdout + b"\0"
        fd = os.memfd_create("keymap")
        os.write(fd, keymap)
        # keymap(format = 1: xkb v1, fd, size); the fd travels as ancillary data.
        socket.send_fds(self.sock, [msg(KEYBOARD, 0, struct.pack("<II", 1, len(keymap)))], [fd])
        os.close(fd)
        self.t0 = time.monotonic()

    def key(self, code, pressed):
        ms = int((time.monotonic() - self.t0) * 1000)
        self.sock.sendall(msg(KEYBOARD, 1, struct.pack("<III", ms, code, 1 if pressed else 0)))

    def mods(self, mask):
        self.sock.sendall(msg(KEYBOARD, 2, struct.pack("<IIII", mask, 0, 0, 0)))

    def chord(self, mods, code, gap=None):
        gap = GAP if gap is None else gap
        mask = 0
        for m in mods:
            mcode, bit = MODS[m]
            self.key(mcode, True)
            mask |= bit
            self.mods(mask)
            time.sleep(gap)
        self.key(code, True)
        time.sleep(gap)
        self.key(code, False)
        time.sleep(gap)
        for m in reversed(mods):
            mcode, bit = MODS[m]
            self.key(mcode, False)
            mask &= ~bit
            self.mods(mask)
            time.sleep(gap)


def run(wl, line):
    cmd, _, arg = line.partition(" ")
    if cmd == "motion":
        try:
            x, y, width, height = map(int, arg.split())
        except ValueError:
            return "error: motion X Y WIDTH HEIGHT"
        if not 0 <= x <= width <= 16384 or not 0 <= y <= height <= 16384 or not width or not height:
            return "error: motion coordinates exceed output bounds"
        ms = int((time.monotonic() - wl.t0) * 1000)
        wl.sock.sendall(msg(POINTER, 1, struct.pack("<IIIII", ms, x, y, width, height)) + msg(POINTER, 4))
        return "ok"
    if cmd == "button":
        if arg not in ("press", "release"):
            return "error: button press|release"
        ms = int((time.monotonic() - wl.t0) * 1000)
        wl.sock.sendall(msg(POINTER, 2, struct.pack("<III", ms, 0x110, int(arg == "press"))) + msg(POINTER, 4))
        return "ok"
    if cmd == "hold":
        parts = arg.split()
        if len(parts) != 2:
            return "error: hold KEY SECONDS"
        code = NAMED.get(parts[0].lower(), KEYS.get(parts[0]))
        try:
            duration = float(parts[1])
        except ValueError:
            return "error: invalid hold duration"
        if code is None or not 0 < duration <= 10:
            return "error: unknown key or duration outside (0, 10]"
        wl.key(code, True)
        try:
            time.sleep(duration)
        finally:
            wl.key(code, False)
        return "ok"
    if cmd == "wheel":
        parts = arg.split()
        if len(parts) != 4 or parts[0] not in ("vertical", "horizontal"):
            return "error: wheel vertical|horizontal AMOUNT COUNT GAP"
        try:
            amount, count, gap = float(parts[1]), int(parts[2]), float(parts[3])
        except ValueError:
            return "error: invalid wheel values"
        if not -1000 <= amount <= 1000 or not 1 <= count <= 200 or not 0.01 <= gap <= 1 or count * gap > 15:
            return "error: wheel values exceed bounds"
        axis = 0 if parts[0] == "vertical" else 1
        # zwlr_virtual_pointer_v1: axis_source(5), axis(3), frame(4).
        # Finger source preserves small pixel deltas through the compositor.
        for _ in range(count):
            ms = int((time.monotonic() - wl.t0) * 1000)
            wl.sock.sendall(msg(POINTER, 5, struct.pack("<I", 1))
                            + msg(POINTER, 3, struct.pack("<IIi", ms, axis, round(amount * 256)))
                            + msg(POINTER, 4))
            time.sleep(gap)
        ms = int((time.monotonic() - wl.t0) * 1000)
        wl.sock.sendall(msg(POINTER, 6, struct.pack("<II", ms, axis)) + msg(POINTER, 4))
        return "ok"
    if cmd == "type":
        for c in arg:
            if c in KEYS:
                wl.chord([], KEYS[c])
            elif c in SHIFTED:
                wl.chord(["shift"], KEYS[SHIFTED[c]])
            else:
                return f"error: can't type {c!r} (printable ASCII only)"
        return "ok"
    if cmd == "key":
        for combo in arg.split():
            *mods, name = combo.lower().split("+")
            code = NAMED.get(name, KEYS.get(name))
            if code is None or any(m not in MODS for m in mods):
                return f"error: unknown key {combo!r}"
            wl.chord(mods, code)
        return "ok"
    return f"error: unknown command {cmd!r}"


def serve(path):
    wl = Wayland()
    wl.setup()
    if os.path.exists(path):
        os.unlink(path)
    ctl = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    ctl.bind(path)
    os.chmod(path, 0o600)
    ctl.listen()
    sel = selectors.DefaultSelector()
    sel.register(wl.sock, selectors.EVENT_READ, "wl")
    sel.register(ctl, selectors.EVENT_READ, "ctl")
    print("headless-input: pointer and keyboard attached", flush=True)
    while True:
        for key, _ in sel.select():
            if key.data == "wl":
                for _ in wl.events():
                    pass
            else:
                conn, _ = ctl.accept()
                with conn:
                    line = conn.makefile().readline().rstrip("\n")
                    conn.sendall((run(wl, line) + "\n").encode())


def send(path, words):
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.connect(path)
    s.sendall((" ".join(words) + "\n").encode())
    reply = s.makefile().readline().strip()
    if reply != "ok":
        sys.exit(reply)


if __name__ == "__main__":
    if len(sys.argv) >= 3 and sys.argv[1] == "serve":
        serve(sys.argv[2])
    elif len(sys.argv) >= 4 and sys.argv[1] == "send":
        send(sys.argv[2], sys.argv[3:])
    else:
        sys.exit(__doc__)
