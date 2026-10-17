#!/usr/bin/env python3
"""Own one isolated dx process group; never signal another server by name."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time


def birth(pid):
    try:
        # Linux start ticks survive exec and distinguish reused process IDs.
        return Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()[19]
    except (OSError, IndexError):
        result = subprocess.run(["ps", "-p", str(pid), "-o", "lstart="],
                                capture_output=True, text=True, check=False)
        return result.stdout.strip() or None


def owned(record):
    pid = record["pid"]
    current = birth(pid)
    if current is not None:
        try:
            return current == record["birth"] and os.getpgid(pid) == pid and os.getsid(pid) == pid
        except ProcessLookupError:
            return False
    # A group can outlive dx. Its ID stays reserved while descendants remain.
    # If a new session leader has reused the ID, the birth check above refuses it.
    try:
        os.killpg(pid, 0)
        return True
    except ProcessLookupError:
        return False


def stop(record):
    current = birth(record["pid"])
    if current is not None and not owned(record):
        print("Process ownership changed; no signals sent.", file=sys.stderr)
        return 2
    if not owned(record):
        return
    for sig in (signal.SIGTERM, signal.SIGKILL):
        if not owned(record):
            return
        try:
            os.killpg(record["pid"], sig)
        except ProcessLookupError:
            return
        if sig == signal.SIGTERM:
            end = time.monotonic() + 1
            while time.monotonic() < end:
                if not owned(record):
                    return
                time.sleep(0.05)


def main():
    action, filename, *command = sys.argv[1:]
    path = Path(filename)
    if action == "launch":
        os.setsid()
        record = {"pid": os.getpid(), "birth": birth(os.getpid())}
        path.write_text(json.dumps(record))
        os.execvp(command[0], command)
    else:
        try:
            record = json.loads(path.read_text())
            if not isinstance(record["pid"], int) or record["pid"] <= 1 or not record["birth"]:
                raise ValueError("invalid process owner")
        except (OSError, ValueError, KeyError, TypeError):
            print("No valid process ownership record; no signals sent.", file=sys.stderr)
            return 2
        if action == "stop":
            return stop(record)
        elif action == "diagnose":
            print(f"Tracked process group: {record['pid']}; identity matches: {owned(record)}")
            result = subprocess.run(["ps", "-eo", "pid,ppid,pgid,sid,stat,etime,pcpu,rss,wchan,comm"],
                                    capture_output=True, text=True, check=False)
            print(result.stdout.splitlines()[0] if result.stdout else "Process snapshot unavailable")
            for line in result.stdout.splitlines()[1:]:
                fields = line.split()
                if len(fields) >= 4 and fields[2] == str(record["pid"]):
                    print(line)
        else:
            raise ValueError("unknown process action")


if __name__ == "__main__":
    sys.exit(main() or 0)
