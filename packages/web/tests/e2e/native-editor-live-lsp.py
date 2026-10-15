#!/usr/bin/env python3
"""Production Rust panel + live rust-analyzer on an owned headless WebKit display.

DOM events establish native adapter/LSP integration, not real OS input, IME or FPS.
Build the native-editor fixture with --no-default-features --features desktop first.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('binary', type=Path)
parser.add_argument('--report', type=Path, required=True, help='new result path (never overwritten)')
args = parser.parse_args()
repo = Path(__file__).resolve().parents[4]
binary = args.binary.resolve()
report = args.report.resolve()
if not binary.is_file():
    parser.error('build the desktop fixture first')
if report.exists():
    parser.error('report already exists; choose a new path')
work = repo / 'target' / 'native-live-lsp'
display = work / 'display'
if (display / 'env').exists() or (display / 'apps.pid').exists():
    parser.error(f'owned display already exists: {display}; stop it before retrying')
root = work / 'corpus'
(root / 'src').mkdir(parents=True, exist_ok=True)
(root / 'Cargo.toml').write_text('[workspace]\n[package]\nname="native_live_lsp"\nversion="0.1.0"\nedition="2021"\n')
source = 'fn add(a: u32, b: u32) -> u32 {\n    a + b\n}\n\nfn main() {\n    let total: u32 = /* 😀中 */ add(1, 2);\n    let wrong: String = total;\n    println!("{wrong}");\n}\n'
(root / 'src/main.rs').write_bytes(source.replace('\n', '\r\n').encode())
report.parent.mkdir(parents=True, exist_ok=True)
env = dict(os.environ, HEADLESS_DIR=str(display))
headless = str(repo / 'tools' / 'headless.sh')
try:
    subprocess.run([headless, 'start', '1600x1100'], cwd=repo, env=env, check=True)
    subprocess.run([headless, 'run', 'env', f'MOONKALE_LSP_ROOT={root}',
                    f'MOONKALE_LSP_REPORT={report}', f'CARGO_TARGET_DIR={work / "cargo"}',
                    'CARGO_BUILD_JOBS=2', str(binary)], cwd=repo, env=env, check=True)
    deadline = time.monotonic() + 240
    while not report.exists():
        if time.monotonic() >= deadline:
            raise RuntimeError(f'native LSP timed out; inspect {display}')
        time.sleep(0.25)
    result = json.loads(report.read_text())
    if not result.get('passed'):
        raise RuntimeError(f'native LSP failed: {result}')
    if (root / 'src/main.rs').read_bytes() != source.replace('\n', '\r\n').encode():
        raise RuntimeError('unsaved LSP edits changed the fixture on disk')
    print(f'NATIVE LIVE LSP: PASS ({", ".join(result["checks"])}) — {report}')
finally:
    subprocess.run([headless, 'stop'], cwd=repo, env=env, check=False)
