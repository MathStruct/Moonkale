#!/usr/bin/env python3
"""Real Wayland input in the production FPS host; canonical text proves undo.

This is correctness evidence, not a frame benchmark. Optional full-text and
passive input reports deliberately add observer work. Uses and stops an owned
headless display; never sends synthetic DOM input or edits document state.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('binary', type=Path)
parser.add_argument('--report', type=Path, required=True)
args = parser.parse_args()
if args.report.exists():
    parser.error('report already exists; choose a new path')
repo = Path(__file__).resolve().parents[4]
work = args.report.resolve().with_suffix('')
work.mkdir(parents=True, exist_ok=True)
env = dict(os.environ, HEADLESS_DIR=str(work / 'display'))
results = []


def command(*words):
    subprocess.run([str(repo / 'tools/headless.sh'), *map(str, words)], cwd=repo, env=env, check=True)


def wait_for(condition, message):
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        try:
            if condition():
                return
        except (FileNotFoundError, json.JSONDecodeError):
            pass
        time.sleep(.05)
    raise AssertionError(message)


try:
    for name, source in [('source.rs', ''.join(f'/// Documentation for item {i}\nfn sample_{i}() {{\n    // body\n}}\n\n' for i in range(1000))),
                         ('three-mb.txt', 'x' * (3 * 1024 * 1024))]:
        path = work / name
        path.write_text(source)
        for preview in ([False, True] if name.endswith('.rs') else [False]):
            text = work / f'{name}-{preview}.txt'
            inputs = work / f'{name}-{preview}.json'
            text.unlink(missing_ok=True)
            inputs.unlink(missing_ok=True)
            env['HEADLESS_DIR'] = str(repo / 'target' / 'n8-input' / str(len(results)))
            command('start', '1500x1100')
            command('run', f'MOONKALE_FPS_SOURCE={path}', f'MOONKALE_FPS_REPORT={work / "frames.json"}',
                    f'MOONKALE_FPS_TEXT_REPORT={text}', f'MOONKALE_FPS_INPUT_REPORT={inputs}',
                    *(['MOONKALE_FPS_DOC_PREVIEW=1'] if preview else []), args.binary.resolve())
            wait_for(lambda: text.read_text() == source and len(json.loads(inputs.read_text()).get('frames', [])) == 120, 'host did not mount')
            time.sleep(.5)
            mounted = json.loads(inputs.read_text())
            geometry = mounted['viewport']
            # Sway tiles this one window over the 1500x1100 output. DOM client
            # coordinates exclude GTK's title/menu chrome above the WebView.
            inset_y = 1100 - mounted['innerHeight']
            geometry['y'] += inset_y
            command('click', round(geometry['x'] + 70), round(geometry['y'] + 40))
            command('key', 'ctrl+Home')
            command('type', 'nativeprobe')
            wait_for(lambda: text.read_text() == 'nativeprobe' + source, 'real typing failed')
            # Each real key is one history transaction. Ctrl+Z must remove exactly
            # the last key, then redo restore it, through the viewport handler.
            command('key', 'ctrl+z')
            wait_for(lambda: text.read_text() == 'nativeprob' + source, 'real Ctrl+Z failed')
            command('key', 'ctrl+shift+z')
            wait_for(lambda: text.read_text() == 'nativeprobe' + source, 'real redo failed')
            # A selected replacement is one transaction and restores exact text.
            command('key', 'ctrl+Home', 'shift+Right')
            command('type', 'Q')
            wait_for(lambda: text.read_text() == 'Qativeprobe' + source, 'real replacement failed')
            command('key', 'ctrl+z')
            wait_for(lambda: text.read_text() == 'nativeprobe' + source, 'replacement undo failed')
            command('hold', 'right', 1)
            horizontal = name.endswith('.txt')
            command('move', round(geometry['x'] + 200), round(geometry['y'] + 150))
            command('wheel', 'horizontal' if horizontal else 'vertical', '.25', '12', '.08')
            time.sleep(.3)
            observed = json.loads(inputs.read_text())
            if name.endswith('.rs'):
                assert observed['preview'] == str(preview).lower(), 'wrong documentation preview mode'
            keys = [e for e in observed['events'] if e['type'] == 'keydown']
            assert any(e.get('ctrl') and e.get('key') == 'z' for e in keys), 'no trusted Ctrl+Z delivered'
            assert sum(e.get('key') == 'ArrowRight' for e in keys) >= 10, 'no held-arrow delivery'
            scrolls = [e for e in observed['events'] if e['type'] == 'scroll' and e['target'] == 'mk-editor-core-viewport']
            assert len(scrolls) >= 5, 'wheel did not scroll document viewport'
            # Exercise the changed recorder with trusted compositor input too.
            # Development/observer timings are diagnostic and cannot pass FPS.
            frames = work / 'frames.json'
            controls = observed['controls']
            for point in controls.values():
                point['y'] += inset_y
            recordings = []
            for gesture, index in [('held-arrows', 1), ('small-scroll-deltas', 3)]:
                command('click', round(geometry['x'] + 70), round(geometry['y'] + 40))
                command('key', 'ctrl+Home')
                command('click', round(controls['gesture']['x']), round(controls['gesture']['y']))
                command('key', 'Escape', 'Home', *(['Down'] * index))
                prior = len(json.loads(frames.read_text())) if frames.exists() else 0
                command('click', round(controls['record']['x']), round(controls['record']['y']))
                time.sleep(1.3)
                if gesture == 'held-arrows':
                    command('click', round(geometry['x'] + 70), round(geometry['y'] + 40))
                    command('key', 'ctrl+Home')
                    command('hold', 'right', 5.3)
                else:
                    command('move', round(geometry['x'] + 200), round(geometry['y'] + 150))
                    command('wheel', 'horizontal' if horizontal else 'vertical', '.25', '70', '.08')
                wait_for(lambda: frames.exists() and len(json.loads(frames.read_text())) > prior, 'recorder did not finish')
                recording = json.loads(frames.read_text())[-1]
                assert recording['gesture'] == gesture and recording['interacted'], {key: recording[key] for key in ['gesture', 'interacted', 'events']}
                recordings.append(recording)
            results.append({'name': name, 'previewRequested': preview, 'undo': True, 'redo': True,
                            'replacementUndo': True, 'recordings': recordings, 'trustedEvents': observed['events']})
            command('shot', work / f'{name}-{preview}.png')
            command('stop')
    args.report.resolve().write_text(json.dumps({'passed': True, 'cases': results}, indent=2))
    print(f'NATIVE FPS INPUT: PASS ({len(results)} cases: real typing, undo/redo, replacement undo, held arrows and wheel)')
finally:
    command('stop')
