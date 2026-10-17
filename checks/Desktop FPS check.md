# Desktop FPS check for the Rust editor (for Daniel)

**What it decides:** whether the Rust code editor keeps 60 FPS on your real desktop. It's the last gate of WP-02 and one of the two gates (with Android) before the Rust editor becomes the default (WP-03).

**Why you:** the automated runs use a private headless display with software rendering, and in that WebKit both clocks have only millisecond precision. They pass 20 of 21 cases there, but only a real screen, GPU and input can accept it.

**Time:** about 15 to 20 minutes. Everything is prepared. Claude built the test editor and generated the test files on 2026-10-17, outside `/tmp`, so they survive a reboot.

## Before you start

- Close builds and anything heavy (no `cargo`, no Julia docs build, no browser suites). The CPU should be idle.
- Use your normal KDE session. Don't set any special environment variables.
- Note your monitor's refresh rate (60, 144, …) and which GPU drives it. The report records the rest.

## Run 1 and 2: the Rust file (Doc preview off, then on)

```sh
cd ~/.cache/moonkale-fps-real
MOONKALE_FPS_SOURCE=$PWD/FpsSource.rs \
MOONKALE_FPS_REPORT=$PWD/reports/rust.json \
  ./target/release/moonkale-native-editor-host
```

A window opens with a 5,000-line Rust file. For **each of the seven gestures**:

1. Choose the gesture in the recorder's dropdown.
2. Press Reload, and put the caret back at the start (Ctrl+Home).
3. Click **Record trusted input**.
4. **Keep still for 2 seconds.** The recorder measures your screen's idle refresh first.
5. Click into the editor and do the gesture **continuously for at least 5 seconds**.
6. After typing, press Ctrl+Z until the text is back to how it was.

| # | Gesture (as named in the dropdown) | What to do for 5 seconds |
|---|---|---|
| 1 | `typing` | type ordinary text without stopping |
| 2 | `held-arrows` | hold the Down arrow key |
| 3 | `wheel` | scroll with the mouse wheel, steadily |
| 4 | `small-scroll-deltas` | scroll slowly with the touchpad, or the wheel in small notches |
| 5 | `scrollbar-drag` | drag the vertical scrollbar up and down |
| 6 | `selection-drag` | press in the text and drag a selection up and down |
| 7 | `caret-movement` | click repeatedly at different places in the text |

After each recording the status line shows its p95, max and whether it passed, and the recording is appended to the report file (the last 100 are kept), so one file per run is enough.

Then switch **Doc preview on** (its toolbar button) and do the same seven gestures again. **Pay attention to the wheel with Doc preview on:** a 210 ms pause showed up there in the headless runs. Do typing and held arrows first, then the wheel, so you hit the same sequence.

Close the window.

## Run 3: the 3 MB single-line file (Doc preview off)

```sh
cd ~/.cache/moonkale-fps-real
MOONKALE_FPS_SOURCE=$PWD/FpsThreeMb.txt \
MOONKALE_FPS_REPORT=$PWD/reports/threemb.json \
  ./target/release/moonkale-native-editor-host
```

The same seven gestures. The file is one 3 MB line, so scroll **horizontally** (Shift+wheel, or the horizontal scrollbar), and hold Right instead of Down. Close the window.

## What passes

Each recording ends with `interacted: true` and at least 30 frames. Then:

- **Strict gate:** p95 ≤ 16.7 ms and no frame over 50 ms.
- **If the WebView's clocks are whole milliseconds** (likely, as in the headless runs), the report says `gate: "calibrated-vsync"`: p95 ≤ 1.5 × the measured idle refresh, and no frame over 50 ms. `strictPassed` stays `false` then, by design.

You don't have to read the numbers: the reports are in `~/.cache/moonkale-fps-real/reports/`, and Claude will go through them. **What only you can tell:** did anything *feel* stuck or jumpy? A one-line note per noticeable moment helps more than the numbers, especially for the wheel with Doc preview on.

## If something doesn't work

- **The window doesn't open, or the recorder bar is missing:** tell Claude. The test editor is at `~/.cache/moonkale-fps-real/target/release/moonkale-native-editor-host`, and its build log is next to it (`build.log`).
- **The test files are gone:** Claude regenerates them in a minute.

## Also waiting for you when you're back (not part of this check)

- **The phone (S10e):** its sample `main.rs` still has a test line (`    let x`), which Claude removes when the phone is connected. Then the rest of the Android check (selection, scrolling, the toolbar on a small screen).
- **The tablet:** Google Play Protect asks before every USB install ("Send app for a security check?"). Answer it each time, or let Claude turn off the scan of USB installs on that tablet.

*Written by Claude, 2026-10-17, from codex's checklist in its handoff and `packages/web/tests/e2e/README.md`.*
