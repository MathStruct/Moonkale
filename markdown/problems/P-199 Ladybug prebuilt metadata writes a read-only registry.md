---
title: "P-199 — Ladybug prebuilt metadata writes a read-only registry"
tags: [problem, build, dependencies]
status: open
phase: WP-03 M5 build evidence
---
**Status:** open upstream; local normal-build retry works · **Related:** [[Rust Code Editor Accessibility and Cutover]], [[Problem Log|P-144 bundled-symbol collisions]]

## Problem

The installed `lbug 0.21.0` build script correctly downloads a pinned prebuilt
archive into Cargo's writable `OUT_DIR/lbug-prebuilt/version-0.21.0-compat/lib`.
Its `scripts/download_lbug.sh` then writes the default metadata file
`<registry crate>/.cache/lbug-prebuilt.env`. In this sandbox the registry is
read-only. The complete archive/header installation succeeds, but metadata
writing returns exit1. The caller reports a failed download and falls back to
building Ladybug from source, which fails here because `cmake` is absent.

## Options

| | approach | pros | cons |
| --- | --- | --- | --- |
| A | Rerun the identical normal build once after the archive is installed | The build script accepts the existing archive, including configured bundled-symbol localization; no registry/environment edits | The first build of another fresh OUT_DIR can fail again |
| B | Give downloader metadata a writable destination upstream | Fixes fresh read-only-registry builds | Requires an upstream change or reviewed dependency patch |
| C | Build Ladybug from source | Avoids the prebuilt downloader | Needs CMake/toolchain support and additional build cost |

## Measurements

NEXT-5's actual desktop phase failed after **1108.119s**, exit101, max
compiler/linker RSS **2566292KiB**. It includes native DuckDB/RocksDB and Rust
dependency compilation, so that total is not Ladybug-only time. The downloader
installed the pinned archive successfully before the env-file error. Report:
`target/next5-build-cost.json`; complete diagnostic
`target/next5-build-cost-logs/desktop_release.log`.

The unchanged retry uses the same worktree target, release profile, jobs2,
locked/offline command and ordinary native environment (`DOCS_RS` unset).
`try_download_prebuilt_lbug` immediately accepts the now-existing archive, then
`use_prebuilt_lbug` applies `LBUG_LOCALIZE_BUNDLED_SYMBOLS=1` as configured.
Retry evidence is kept separately in `target/next5-desktop-retry.json` and
`target/next5-desktop-retry.log`, not substituted for the failed phase.

## Decision

Use A for this measurement and leave B open. Do not change registry source or
permissions, install tools or change production dependencies. Record the failed
initial phase, the retry's different cache state and both elapsed times. The
fixture and artifact results are persisted before the optional product phase.

## Lesson

A successful archive download can still be reported as failure by a later
metadata write. Read the complete build-script output before installing a source
build toolchain. Reuse the normal prebuilt path: linking a raw archive through
`LBUG_LIBRARY_DIR`/`LBUG_INCLUDE_DIR` skips this crate's bundled-symbol localization
and can reintroduce the collisions documented in P-144.
