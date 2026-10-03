// @moonkale/xterm — thin wrapper. See PROTOCOL.md.
//
// No application state, no shell knowledge: bytes in, bytes out. Rust owns
// the session; this shows it and reports keystrokes and size changes.

import { Terminal } from "@xterm/xterm"
import { FitAddon } from "@xterm/addon-fit"

type Handlers = { onData: (data: string) => void; onResize: (cols: number, rows: number) => void }
type Entry = { term: Terminal; fit: FitAddon; ro: ResizeObserver }

const terms = new WeakMap<HTMLElement, Entry>()
const live = new Set<HTMLElement>()

// Spec 030: the colours come from the theme's tokens (CSS custom properties
// the shell defines on <html>), read when a terminal mounts and again
// whenever the theme changes — `data-theme` on <html>, or the system's
// light/dark preference while the theme follows it.
const ANSI = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"]
function themeFrom(el: HTMLElement): Record<string, string> {
  const css = getComputedStyle(el)
  const v = (name: string, fallback: string) => css.getPropertyValue(name).trim() || fallback
  const theme: Record<string, string> = {
    background: v("--mk-term-bg", "#0b0d12"),
    foreground: v("--mk-term-fg", "#e6e8ee"),
    cursor: v("--mk-term-cursor", "#6ea8fe"),
    selectionBackground: v("--mk-term-selection", "rgba(110,168,254,0.3)"),
  }
  ANSI.forEach((name, i) => {
    const normal = v(`--mk-ansi-${i}`, "")
    const bright = v(`--mk-ansi-${i + 8}`, "")
    if (normal) theme[name] = normal
    if (bright) theme["bright" + name[0].toUpperCase() + name.slice(1)] = bright
  })
  return theme
}
function retheme(): void {
  for (const el of live) {
    const e = terms.get(el)
    if (e) e.term.options.theme = themeFrom(el)
  }
}
new MutationObserver(retheme).observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] })
window.matchMedia?.("(prefers-color-scheme: light)").addEventListener?.("change", retheme)
// The theme's stylesheet may arrive after the first terminal: re-read once it has.
window.addEventListener("moonkale-theme", retheme)

function b64decode(s: string): Uint8Array {
  const bin = atob(s)
  const out = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i)
  return out
}

function b64encode(s: string): string {
  // Keystrokes are UTF-8 text; encode as bytes so Rust gets exact bytes.
  const bytes = new TextEncoder().encode(s)
  let bin = ""
  for (const b of bytes) bin += String.fromCharCode(b)
  return btoa(bin)
}

function mount(el: HTMLElement, handlers: Handlers): { cols: number; rows: number } {
  destroy(el)
  const term = new Terminal({
    cursorBlink: true,
    fontFamily: "'JetBrains Mono', 'Fira Code', Consolas, 'Noto Sans Mono CJK SC', monospace",
    fontSize: 13,
    theme: themeFrom(el),
    scrollback: 5000,
    allowProposedApi: true,
  })
  const fit = new FitAddon()
  term.loadAddon(fit)
  term.open(el)
  fit.fit()
  term.onData((d) => handlers.onData(b64encode(d)))
  term.onResize(({ cols, rows }) => handlers.onResize(cols, rows))
  const ro = new ResizeObserver(() => { try { fit.fit() } catch { /* hidden */ } })
  ro.observe(el)
  terms.set(el, { term, fit, ro })
  live.add(el)
  return { cols: term.cols, rows: term.rows }
}

function write(el: HTMLElement, b64: string): void {
  terms.get(el)?.term.write(b64decode(b64))
}

function focus(el: HTMLElement): void {
  terms.get(el)?.term.focus()
}

function fitNow(el: HTMLElement): void {
  try { terms.get(el)?.fit.fit() } catch { /* hidden */ }
}

/** Plain text of the line under a clientY point, plus its neighbours
 *  (row maths from xterm's own rows element; callers fall back to the
 *  neighbours when the exact row has no link). */
function lineAt(el: HTMLElement, y: number): string[] | null {
  const e = terms.get(el)
  if (!e) return null
  const rowsEl = el.querySelector(".xterm-rows") as HTMLElement | null
  const rect = (rowsEl ?? el).getBoundingClientRect()
  const rowHeight = rect.height / e.term.rows
  const row = Math.floor((y - rect.top) / rowHeight) + e.term.buffer.active.viewportY
  const line = (r: number) => e.term.buffer.active.getLine(r)?.translateToString(true) ?? ""
  return [line(row), line(row - 1), line(row + 1)]
}

/** The whole buffer (scrollback + screen) as plain text, for trace parsing. */
function allText(el: HTMLElement): string {
  const e = terms.get(el)
  if (!e) return ""
  const buf = e.term.buffer.active
  const out: string[] = []
  for (let r = 0; r < buf.length; r++) out.push(buf.getLine(r)?.translateToString(true) ?? "")
  return out.join("\n")
}

function destroy(el: HTMLElement): void {
  const e = terms.get(el)
  if (e) { e.ro.disconnect(); e.term.dispose(); terms.delete(el) }
  live.delete(el)
}

declare global { interface Window { moonkale?: Record<string, unknown> } }
window.moonkale = window.moonkale ?? {}
window.moonkale.xterm = { mount, write, focus, fit: fitNow, lineAt, allText, destroy }
