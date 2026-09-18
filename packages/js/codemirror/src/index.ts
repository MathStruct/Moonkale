// @moonkale/codemirror — thin wrapper. See PROTOCOL.md.
//
// Rules (packages/js/README.md): no application state, no language
// knowledge, no DOM outside the mount element. Rust owns the document; this
// file only shows it and reports edits.

import { EditorState } from "@codemirror/state"
import {
  EditorView,
  keymap,
  lineNumbers,
  highlightActiveLine,
  highlightActiveLineGutter,
  drawSelection,
  rectangularSelection,
  crosshairCursor,
} from "@codemirror/view"
import { defaultKeymap, history, historyKeymap, indentWithTab, undo as cmUndo, redo as cmRedo } from "@codemirror/commands"
import { oneDark } from "@codemirror/theme-one-dark"
import { setDiagnostics, lintGutter, type Diagnostic } from "@codemirror/lint"
import { hoverTooltip } from "@codemirror/view"

type OnChange = (text: string) => void
/** Language-feature hooks: Rust answers hover requests asynchronously and
 *  receives go-to-definition requests. Positions are LSP-style
 *  (0-based line, UTF-16 column). */
type Features = {
  onHover?: (id: number, line: number, col: number) => void
  onDefinition?: (line: number, col: number) => void
}
type Entry = { view: EditorView; pendingHover: Map<number, (text: string | null) => void>; nextHover: number }

const views = new WeakMap<HTMLElement, Entry>()

function lspPos(view: EditorView, pos: number): { line: number; col: number } {
  const line = view.state.doc.lineAt(pos)
  return { line: line.number - 1, col: pos - line.from }
}

function cmPos(view: EditorView, line: number, col: number): number {
  const l = view.state.doc.line(Math.min(Math.max(line + 1, 1), view.state.doc.lines))
  return Math.min(l.from + col, l.to)
}

function mount(el: HTMLElement, text: string, onChange: OnChange, features: Features = {}): void {
  destroy(el)
  const entry: Partial<Entry> = { pendingHover: new Map(), nextHover: 1 }
  const hover = hoverTooltip(async (v, pos) => {
    if (!features.onHover) return null
    const { line, col } = lspPos(v, pos)
    const id = entry.nextHover!++
    const text = await new Promise<string | null>((resolve) => {
      entry.pendingHover!.set(id, resolve)
      setTimeout(() => { if (entry.pendingHover!.delete(id)) resolve(null) }, 3000)
      features.onHover!(id, line, col)
    })
    if (!text) return null
    return { pos, create: () => { const dom = document.createElement("div"); dom.className = "mk-hover"; dom.textContent = text; return { dom } } }
  }, { hoverTime: 250 })
  const gotoDef = keymap.of([{ key: "F12", run: (v) => { if (!features.onDefinition) return false; const { line, col } = lspPos(v, v.state.selection.main.head); features.onDefinition(line, col); return true } }])
  const view = new EditorView({
    state: EditorState.create({
      doc: text,
      extensions: [
        lineNumbers(),
        highlightActiveLineGutter(),
        highlightActiveLine(),
        drawSelection(),
        rectangularSelection(),
        crosshairCursor(),
        history(),
        keymap.of([...defaultKeymap, ...historyKeymap, indentWithTab]),
        lintGutter(),
        hover,
        gotoDef,
        oneDark,
        EditorView.updateListener.of((u) => {
          if (u.docChanged) onChange(u.state.doc.toString())
        }),
      ],
    }),
    parent: el,
  })
  entry.view = view
  views.set(el, entry as Entry)
}

/** Replace all diagnostics (LSP coordinates → document offsets). */
function setLspDiagnostics(el: HTMLElement, items: { line: number; col: number; endLine: number; endCol: number; severity: string; message: string }[]): void {
  const e = views.get(el)
  if (!e) return
  const diags: Diagnostic[] = items.map((d) => ({
    from: cmPos(e.view, d.line, d.col),
    to: Math.max(cmPos(e.view, d.endLine, d.endCol), cmPos(e.view, d.line, d.col)),
    severity: d.severity === "error" ? "error" : d.severity === "warning" ? "warning" : d.severity === "hint" ? "hint" : "info",
    message: d.message,
  }))
  e.view.dispatch(setDiagnostics(e.view.state, diags))
}

/** Rust's answer to a hover request. */
function hoverResult(el: HTMLElement, id: number, text: string | null): void {
  const e = views.get(el)
  const resolve = e?.pendingHover.get(id)
  if (resolve) { e!.pendingHover.delete(id); resolve(text) }
}

function setCursor(el: HTMLElement, line: number, col: number): void {
  const e = views.get(el)
  if (!e) return
  const pos = cmPos(e.view, line, col)
  e.view.dispatch({ selection: { anchor: pos }, scrollIntoView: true })
  e.view.focus()
}

/** Replace the whole document without going through the user's undo history
 *  boundary semantics (used after reload/revert). Fires onChange like any
 *  other transaction so Rust and the view agree. */
function setText(el: HTMLElement, text: string): void {
  const view = views.get(el)?.view
  if (!view) return
  view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } })
}

function getText(el: HTMLElement): string | undefined {
  return views.get(el)?.view.state.doc.toString()
}

function focus(el: HTMLElement): void {
  views.get(el)?.view.focus()
}

function undo(el: HTMLElement): void {
  const view = views.get(el)?.view
  if (view) cmUndo(view)
}

function redo(el: HTMLElement): void {
  const view = views.get(el)?.view
  if (view) cmRedo(view)
}

function destroy(el: HTMLElement): void {
  const e = views.get(el)
  if (e) {
    e.view.destroy()
    views.delete(el)
  }
}

declare global {
  interface Window {
    moonkale?: Record<string, unknown>
  }
}

window.moonkale = window.moonkale ?? {}
window.moonkale.codemirror = { mount, setText, getText, focus, undo, redo, destroy, setLspDiagnostics, hoverResult, setCursor }
