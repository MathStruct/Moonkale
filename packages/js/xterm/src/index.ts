// @moonkale/xterm — thin wrapper. See PROTOCOL.md.
//
// No application state, no shell knowledge: bytes in, bytes out. Rust owns
// the session; this shows it and reports keystrokes and size changes.

import { Terminal } from "@xterm/xterm"
import { FitAddon } from "@xterm/addon-fit"

type Handlers = { onData: (data: string) => void; onResize: (cols: number, rows: number) => void }
type Entry = { term: Terminal; fit: FitAddon; ro: ResizeObserver }

const terms = new WeakMap<HTMLElement, Entry>()

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
    fontFamily: "'JetBrains Mono', 'Fira Code', Consolas, monospace",
    fontSize: 13,
    theme: { background: "#0b0d12", foreground: "#e6e8ee", cursor: "#6ea8fe", selectionBackground: "rgba(110,168,254,0.3)" },
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

function destroy(el: HTMLElement): void {
  const e = terms.get(el)
  if (e) { e.ro.disconnect(); e.term.dispose(); terms.delete(el) }
}

declare global { interface Window { moonkale?: Record<string, unknown> } }
window.moonkale = window.moonkale ?? {}
window.moonkale.xterm = { mount, write, focus, fit: fitNow, lineAt, destroy }
