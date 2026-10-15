#!/usr/bin/env python3
"""Owned native WebKit/AT-SPI tree and caret/selection context; no speech/OS-input claim."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import tempfile
import shutil

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('binary', type=Path)
parser.add_argument('--report', required=True, type=Path)
args = parser.parse_args()
repo = Path(__file__).resolve().parents[4]
work = repo / 'target' / 'native-accessibility'
report = args.report.resolve()
if report.exists():
    parser.error('choose a new report path')
if not args.binary.resolve().is_file():
    parser.error('build the native fixture first')
# A fresh session/AT-SPI bus contains only this runner's processes, never the user's desktop.
if not os.environ.get('MOONKALE_PRIVATE_A11Y_BUS'):
    with tempfile.TemporaryDirectory(prefix='mk-a11y-', ignore_cleanup_errors=True) as directory:
        runtime = Path(directory) / 'runtime'
        runtime.mkdir(mode=0o700)
        env = dict(os.environ, MOONKALE_PRIVATE_A11Y_BUS='1', MOONKALE_A11Y_RUN_DIR=directory,
                   XDG_RUNTIME_DIR=str(runtime), GSETTINGS_BACKEND='memory', ATSPI_DBUS_IMPLEMENTATION='dbus-daemon')
        for key, name in [('XDG_CACHE_HOME', 'cache'), ('XDG_CONFIG_HOME', 'config'), ('XDG_DATA_HOME', 'data')]:
            path = Path(directory) / name
            path.mkdir()
            env[key] = str(path)
        raise SystemExit(subprocess.call(['dbus-run-session', '--', sys.executable, __file__, *sys.argv[1:]], env=env))
import gi

gi.require_version('Atspi', '2.0')
from gi.repository import Atspi, Gio, GLib

work.mkdir(parents=True, exist_ok=True)
display = Path(os.environ['MOONKALE_A11Y_RUN_DIR']) / 'display'
if (display / 'env').exists() or (display / 'apps.pid').exists():
    parser.error('owned display already exists; stop it first')
dom_report = work / 'dom.json'
dom_report.unlink(missing_ok=True)
report.parent.mkdir(parents=True, exist_ok=True)
env = dict(os.environ, HEADLESS_DIR=str(display))
headless = str(repo / 'tools' / 'headless.sh')
try:
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    bus.call_sync('org.a11y.Bus', '/org/a11y/bus', 'org.freedesktop.DBus.Properties', 'Set',
                  GLib.Variant('(ssv)', ('org.a11y.Status', 'IsEnabled', GLib.Variant('b', True))),
                  None, Gio.DBusCallFlags.NONE, 5000, None)
    Atspi.set_timeout(2000, 5000)
    Atspi.init()
    subprocess.run([headless, 'start', '1600x1100'], cwd=repo, env=env, check=True)
    subprocess.run([headless, 'run', 'env', 'NO_AT_BRIDGE=0', 'GTK_MODULES=atk-bridge', 'GSETTINGS_BACKEND=memory', 'ATSPI_DBUS_IMPLEMENTATION=dbus-daemon',
                    f'MOONKALE_ACCESSIBILITY_PROBE={dom_report}', str(args.binary.resolve())],
                   cwd=repo, env=env, check=True)
    deadline = time.monotonic() + 90
    while not dom_report.exists():
        if time.monotonic() >= deadline:
            raise RuntimeError(f'native DOM readiness timed out; inspect {display}')
        time.sleep(0.1)
    dom = json.loads(dom_report.read_text())
    if not dom.get('passed'):
        raise RuntimeError(f'native DOM context failed: {dom}')
    nodes = []
    while time.monotonic() < deadline:
        while GLib.MainContext.default().pending():
            GLib.MainContext.default().iteration(False)
        queue = [(Atspi.get_desktop(0), 0)]
        nodes = []
        while queue and len(nodes) < 4000 and time.monotonic() < deadline:
            node, depth = queue.pop(0)
            if node is None or depth > 24:
                continue
            try:
                text = node.get_text_iface()
                nodes.append({'role': node.get_role_name(), 'name': node.get_name(),
                              'description': node.get_description(), 'pid': node.get_process_id(),
                              'text': Atspi.Text.get_text(text, 0, min(Atspi.Text.get_character_count(text), 512)) if text else '',
                              'attributes': node.get_attributes()})
                queue.extend((node.get_child_at_index(i), depth + 1) for i in range(min(node.get_child_count(), 1000)))
            except GLib.Error:
                continue
        inputs = [node for node in nodes if 'Code editor' in node['name'] and 'toggles whether Tab' in node['description']]
        if inputs and any(node['role'] == 'status bar' and 'Selection from line 2' in node['text'] and node['attributes'].get('live') == 'polite' for node in nodes):
            report.write_text(json.dumps({'passed': True, 'dom': dom, 'nodes': nodes}, indent=2))
            print(f'NATIVE ACCESSIBILITY: PASS (AT-SPI name, help, caret and selection) — {report}')
            break
        time.sleep(0.25)
    else:
        report.write_text(json.dumps({'passed': False, 'dom': dom, 'nodes': nodes}, indent=2))
        raise RuntimeError(f'AT-SPI did not expose the editor context; evidence in {report}')
finally:
    subprocess.run([headless, 'stop'], cwd=repo, env=env, check=False)
    logs = work / f'{report.stem}-logs'
    logs.mkdir(exist_ok=True)
    for path in display.glob('*.log'):
        shutil.copy2(path, logs / path.name)
