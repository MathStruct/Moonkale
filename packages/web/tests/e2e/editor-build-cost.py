#!/usr/bin/env python3
"""Matched release fixture build costs in a fresh worktree-local target.

Hold bench.lock and run through heavy.sh; reserve both heavy slots for timing.
No network/cargo cache coldness or default-cutover size saving is implied.
"""
import argparse
import gzip
import json
import os
from pathlib import Path
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--target', required=True, type=Path)
parser.add_argument('--report', required=True, type=Path)
parser.add_argument('--prebuilt-theme-assets', action='store_true', help='read-only registry: DOCS_RS=1 skips dioxus-code theme asset regeneration; existing generated CSS must be present')
parser.add_argument('--desktop', action='store_true', help='also build the actual desktop app in this worktree existing, partially cached target')
args = parser.parse_args()
repo = Path(__file__).resolve().parents[4]
target, report = args.target.resolve(), args.report.resolve()
if not target.is_relative_to(repo / 'target') or target.exists():
    parser.error('target must be a new directory under this worktree/target')
if report.exists():
    parser.error('choose a new report path')
manifest = repo / 'packages/web/tests/fixtures/native-editor/Cargo.toml'
logs = report.parent / f'{report.stem}-logs'
logs.mkdir(parents=True, exist_ok=False)
report.parent.mkdir(parents=True, exist_ok=True)
env = dict(os.environ, CARGO_TARGET_DIR=str(target), CARGO_BUILD_JOBS='2')
if args.prebuilt_theme_assets:
    env['DOCS_RS'] = '1'
command = ['cargo', 'build', '--release', '--locked', '--offline', '--manifest-path', str(manifest),
           '--no-default-features', '--features', 'desktop']
result = {'scope': 'native production-panel fixture, release, jobs=2; dependencies downloaded; fresh Cargo target',
          'command': command, 'target': str(target), 'phases': {}, 'artifacts': {},
          'build_environment': {'DOCS_RS': env.get('DOCS_RS'), 'CARGO_BUILD_JOBS': '2'},
          'git_head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip(),
          'rustc': subprocess.check_output(['rustc', '--version'], text=True).strip()}
for phase in ['fresh_target', 'no_change', 'application_rebuild']:
    if phase == 'application_rebuild':
        subprocess.run(['cargo', 'clean', '--release', '-p', 'moonkale-native-editor-host',
                        '--manifest-path', str(manifest)], cwd=repo, env=env, check=True)
    rss = logs / f'{phase}.rss'
    start = time.monotonic()
    with (logs / f'{phase}.log').open('w') as output:
        process = subprocess.run(['/usr/bin/time', '-f', '%M', '-o', str(rss), *command],
                                 cwd=repo, env=env, stdout=output, stderr=subprocess.STDOUT)
    result['phases'][phase] = {'elapsed_seconds': time.monotonic() - start,
                              'max_rss_kib': int(rss.read_text().strip().splitlines()[-1]),
                              'exit_code': process.returncode}
    print(phase, result['phases'][phase], flush=True)
    report.write_text(json.dumps(result, indent=2))
    if process.returncode:
        raise SystemExit(process.returncode)
if args.desktop:
    desktop_env = dict(env, CARGO_TARGET_DIR=str(repo / 'target'))
    # Other native dependencies use DOCS_RS to skip actual library linking.
    # The product build must use its ordinary production environment.
    desktop_env.pop('DOCS_RS', None)
    if args.prebuilt_theme_assets:
        # Seed only this upstream package's build-script output in the product
        # target. DOCS_RS is not watched by its script, so the normal product
        # build reuses its generated Rust and existing CSS without registry writes.
        # Do not pass DOCS_RS to LadybugDB or the product dependency graph.
        # Cargo retains the workspace's configured language features when a
        # dependency package is selected directly; external features cannot be
        # specified through -p. Its build dependency graph is target-independent.
        seed = ['cargo', 'build', '--release', '--locked', '--offline', '-p', 'dioxus-code']
        seed_env = dict(desktop_env, DOCS_RS='1')
        rss = logs / 'desktop_theme_seed.rss'
        start = time.monotonic()
        with (logs / 'desktop_theme_seed.log').open('w') as output:
            process = subprocess.run(['/usr/bin/time', '-f', '%M', '-o', str(rss), *seed],
                                     cwd=repo, env=seed_env, stdout=output, stderr=subprocess.STDOUT)
        result['phases']['desktop_theme_seed'] = {'command': seed,
            'elapsed_seconds': time.monotonic() - start,
            'max_rss_kib': int(rss.read_text().strip().splitlines()[-1]), 'exit_code': process.returncode}
        report.write_text(json.dumps(result, indent=2))
        print('desktop_theme_seed', result['phases']['desktop_theme_seed'], flush=True)
        if process.returncode:
            raise SystemExit(process.returncode)
    desktop_command = ['cargo', 'build', '--release', '--locked', '--offline', '-p', 'desktop', '--features', 'desktop']
    start = time.monotonic()
    rss = logs / 'desktop_release.rss'
    with (logs / 'desktop_release.log').open('w') as output:
        process = subprocess.run(['/usr/bin/time', '-f', '%M', '-o', str(rss), *desktop_command],
                                 cwd=repo, env=desktop_env, stdout=output, stderr=subprocess.STDOUT)
    result['phases']['desktop_existing_target'] = {'command': desktop_command,
        'elapsed_seconds': time.monotonic() - start, 'max_rss_kib': int(rss.read_text().strip().splitlines()[-1]), 'exit_code': process.returncode}
    print('desktop_existing_target', result['phases']['desktop_existing_target'], flush=True)
    report.write_text(json.dumps(result, indent=2))
    if process.returncode:
        raise SystemExit(process.returncode)
for name, path in {
    'native_fixture': target / 'release/moonkale-native-editor-host',
    'web_server_with_both_backends': repo / 'target/dx/web/release/web/server',
    'web_wasm_with_both_backends': repo / 'target/dx/web/release/web/public/wasm/web_bg.wasm',
    'codemirror_bundle': repo / 'packages/code-view/assets/codemirror.js',
}.items():
    data = path.read_bytes()
    result['artifacts'][name] = {'bytes': len(data), 'gzip_bytes': len(gzip.compress(data, mtime=0))}
if args.desktop:
    data = (repo / 'target/release/moonkale').read_bytes()
    result['artifacts']['desktop_app_with_both_backends'] = {'bytes': len(data), 'gzip_bytes': len(gzip.compress(data, mtime=0))}
report.write_text(json.dumps(result, indent=2))
print(f'EDITOR BUILD COST: PASS — {report}')
