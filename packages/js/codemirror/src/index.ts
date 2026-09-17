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
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands"
import { oneDark } from "@codemirror/theme-one-dark"

type OnChange = (text: string) => void

const views = new WeakMap<HTMLElement, EditorView>()

function mount(el: HTMLElement, text: string, onChange: OnChange): void {
  destroy(el)
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
        oneDark,
        EditorView.updateListener.of((u) => {
          if (u.docChanged) onChange(u.state.doc.toString())
        }),
      ],
    }),
    parent: el,
  })
  views.set(el, view)
}

/** Replace the whole document without going through the user's undo history
 *  boundary semantics (used after reload/revert). Fires onChange like any
 *  other transaction so Rust and the view agree. */
function setText(el: HTMLElement, text: string): void {
  const view = views.get(el)
  if (!view) return
  view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } })
}

function getText(el: HTMLElement): string | undefined {
  return views.get(el)?.state.doc.toString()
}

function focus(el: HTMLElement): void {
  views.get(el)?.focus()
}

function destroy(el: HTMLElement): void {
  const view = views.get(el)
  if (view) {
    view.destroy()
    views.delete(el)
  }
}

declare global {
  interface Window {
    moonkale?: Record<string, unknown>
  }
}

window.moonkale = window.moonkale ?? {}
window.moonkale.codemirror = { mount, setText, getText, focus, destroy }
