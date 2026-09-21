// @moonkale/codemirror — thin wrapper. See PROTOCOL.md.
//
// Rules (packages/js/README.md): no application state, no language
// knowledge, no DOM outside the mount element. Rust owns the document; this
// file only shows it and reports edits.

import { search, searchKeymap, highlightSelectionMatches } from "@codemirror/search"
import { bracketMatching, foldGutter, foldKeymap, indentOnInput } from "@codemirror/language"
import { languageExtension } from "./languages"
import { autocompletion, completionKeymap, type CompletionContext, type CompletionResult, type Completion } from "@codemirror/autocomplete"
import { EditorState, StateEffect, StateField, RangeSet, Compartment } from "@codemirror/state"
import { gutter, GutterMarker, Decoration, type DecorationSet } from "@codemirror/view"
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
import { defaultKeymap, history, historyKeymap, indentWithTab, toggleComment, undo as cmUndo, redo as cmRedo } from "@codemirror/commands"
import { openSearchPanel } from "@codemirror/search"
import { foldAll, unfoldAll } from "@codemirror/language"
import { oneDark } from "@codemirror/theme-one-dark"
import { setDiagnostics, lintGutter, type Diagnostic } from "@codemirror/lint"
import { hoverTooltip } from "@codemirror/view"

/** Spec 018 / P-037: the view reports *splices* (UTF-16 offsets into the
 *  document before the change, in document order), never the whole text. */
export type Splice = { from: number; to: number; insert: string }
type OnChange = (changes: Splice[], length: number) => void
/** Language-feature hooks: Rust answers hover requests asynchronously and
 *  receives go-to-definition requests. Positions are LSP-style
 *  (0-based line, UTF-16 column). */
type Features = {
  onHover?: (id: number, line: number, col: number) => void
  onDefinition?: (line: number, col: number) => void
  /** Milestone 7 (all optional): completion proposals are answered with
   *  `completionResult(el, id, items)`; F2 asks Rust to rename the word at
   *  the cursor; Ctrl+. asks for code actions on the selection; Shift+F12
   *  for references. */
  onCompletion?: (id: number, line: number, col: number) => void
  onRename?: (line: number, col: number, word: string) => void
  onCodeActions?: (line: number, col: number, endLine: number, endCol: number) => void
  onReferences?: (line: number, col: number) => void
  /** Milestone 9: the cursor moved (throttled to 4/s); presence. */
  onCursor?: (line: number, col: number) => void
  /** Spec 012: `[[query` typed → Rust answers with `completionResult(el, id, items)`
   *  (items insert `[[target]]`); Ctrl/Cmd+click on a decorated link. */
  onWikiQuery?: (id: number, query: string) => void
  onWikiLink?: (target: string) => void
  /** Spec 010: Rust's language id for the document; picks the grammar. */
  language?: string | null
  /** Spec 014: soft-wrap long lines (changed later with `setWrap`). */
  wrap?: boolean
}
/** A `[[link]]` span in UTF-16 offsets, resolved or not (Rust computes both). */
export type WikiSpan = { from: number; to: number; resolved: boolean }
export type PresenceMark = { line: number; label: string }
export type CompletionItem = { label: string; kind?: string; detail?: string; insert?: string; sort?: string }
type Entry = {
  view: EditorView
  wrap: Compartment
  features: Features
  pendingHover: Map<number, (text: string | null) => void>
  nextHover: number
  pendingCompletion: Map<number, (items: CompletionItem[] | null) => void>
}

const views = new WeakMap<HTMLElement, Entry>()

// Presence gutter (Milestone 9): other people's initials next to the line they are on.
class PresenceMarker extends GutterMarker {
  constructor(readonly label: string) { super() }
  eq(o: PresenceMarker) { return o.label === this.label }
  toDOM() { const s = document.createElement("span"); s.className = "cm-presence-mark"; s.textContent = this.label; s.title = `${this.label} is here`; return s }
}
const setPresenceEffect = StateEffect.define<PresenceMark[]>()
const presenceField = StateField.define<RangeSet<GutterMarker>>({
  create: () => RangeSet.empty,
  update(set, tr) {
    set = set.map(tr.changes)
    for (const e of tr.effects) {
      if (e.is(setPresenceEffect)) {
        const doc = tr.state.doc
        const marks = e.value
          .filter((m) => m.line >= 0 && m.line < doc.lines)
          .map((m) => new PresenceMarker(m.label).range(doc.line(m.line + 1).from))
          .sort((a, b) => a.from - b.from)
        set = RangeSet.of(marks, true)
      }
    }
    return set
  },
})
const presenceGutter = [presenceField, gutter({ class: "cm-presence-gutter", markers: (v) => v.state.field(presenceField) })]

// [[wiki-links]] in source mode (spec 012): marks from Rust, mapped through edits.
const setWikiEffect = StateEffect.define<WikiSpan[]>()
const wikiMarkResolved = Decoration.mark({ class: "cm-wikilink" })
const wikiMarkUnresolved = Decoration.mark({ class: "cm-wikilink cm-wikilink-unresolved" })
const wikiField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(set, tr) {
    set = set.map(tr.changes)
    for (const e of tr.effects) {
      if (e.is(setWikiEffect)) {
        const len = tr.state.doc.length
        const ranges = e.value
          .filter((w) => w.from >= 0 && w.to <= len && w.from < w.to)
          .map((w) => (w.resolved ? wikiMarkResolved : wikiMarkUnresolved).range(w.from, w.to))
          .sort((a, b) => a.from - b.from)
        set = Decoration.set(ranges, true)
      }
    }
    return set
  },
  provide: (f) => EditorView.decorations.from(f),
})
/** The `[[…]]` text around `pos`, as a target (before `#`/`|`), or null. */
function wikiTargetAt(view: EditorView, pos: number): string | null {
  const line = view.state.doc.lineAt(pos)
  const text = line.text
  const off = pos - line.from
  const open = text.lastIndexOf("[[", off)
  if (open < 0) return null
  const close = text.indexOf("]]", open + 2)
  if (close < 0 || off > close + 2) return null
  const inner = text.slice(open + 2, close)
  return inner.split("|")[0].split("#")[0].trim() || null
}

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
  const entry: Partial<Entry> = { pendingHover: new Map(), nextHover: 1, pendingCompletion: new Map(), wrap: new Compartment(), features }
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
  // Milestone 7: language features answered by Rust.
  // Spec 012: `[[` completion of page names, answered by Rust.
  const wikiComplete = async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    if (!features.onWikiQuery) return null
    const m = ctx.matchBefore(/\[\[[^\]\n]*/)
    if (!m) return null
    const id = entry.nextHover!++
    const items = await new Promise<CompletionItem[] | null>((resolve) => {
      entry.pendingCompletion!.set(id, resolve)
      setTimeout(() => { if (entry.pendingCompletion!.delete(id)) resolve(null) }, 4000)
      features.onWikiQuery!(id, m.text.slice(2))
    })
    if (!items) return null
    // Keep `]]` the user may already have typed.
    const after = ctx.state.sliceDoc(ctx.pos, ctx.pos + 2)
    return {
      from: m.from,
      filter: false,
      options: items.map((it) => ({ label: it.label, detail: it.detail, type: "text", apply: after === "]]" ? `[[${it.label}` : `[[${it.label}]]` })),
    }
  }
  const wikiClick = EditorView.domEventHandlers({
    mousedown: (e, view) => {
      if (!features.onWikiLink || !(e.ctrlKey || e.metaKey)) return false
      const pos = view.posAtCoords({ x: e.clientX, y: e.clientY })
      if (pos == null) return false
      const target = wikiTargetAt(view, pos)
      if (!target) return false
      e.preventDefault()
      features.onWikiLink(target)
      return true
    },
  })
  const complete = async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    if (!features.onCompletion) return null
    const word = ctx.matchBefore(/[\w$]*/)
    if (!ctx.explicit && (!word || word.from === word.to)) return null
    const { line, col } = lspPos(ctx.view!, ctx.pos)
    const id = entry.nextHover!++
    const items = await new Promise<CompletionItem[] | null>((resolve) => {
      entry.pendingCompletion!.set(id, resolve)
      setTimeout(() => { if (entry.pendingCompletion!.delete(id)) resolve(null) }, 4000)
      features.onCompletion!(id, line, col)
    })
    if (!items || !items.length) return null
    const options: Completion[] = items.map((it) => ({
      label: it.label,
      type: it.kind,
      detail: it.detail,
      apply: it.insert ?? it.label,
      boost: it.sort ? -it.sort.length : 0,
    }))
    return { from: word ? word.from : ctx.pos, options, validFor: /^[\w$]*$/ }
  }
  const featureKeys = keymap.of([
    { key: "F2", run: (v) => {
      if (!features.onRename) return false
      const head = v.state.selection.main.head
      const { line, col } = lspPos(v, head)
      const w = v.state.wordAt(head)
      features.onRename(line, col, w ? v.state.sliceDoc(w.from, w.to) : "")
      return true
    } },
    { key: "Mod-.", run: (v) => {
      if (!features.onCodeActions) return false
      const r = v.state.selection.main
      const a = lspPos(v, r.from), b = lspPos(v, r.to)
      features.onCodeActions(a.line, a.col, b.line, b.col)
      return true
    } },
    { key: "Shift-F12", run: (v) => {
      if (!features.onReferences) return false
      const { line, col } = lspPos(v, v.state.selection.main.head)
      features.onReferences(line, col)
      return true
    } },
  ])
  let cursorTimer: number | null = null
  let lastCursor = ""
  const cursorWatch = EditorView.updateListener.of((u) => {
    if (!features.onCursor || !u.selectionSet) return
    if (cursorTimer !== null) return
    cursorTimer = window.setTimeout(() => {
      cursorTimer = null
      const { line, col } = lspPos(u.view, u.view.state.selection.main.head)
      const key = `${line}:${col}`
      if (key === lastCursor) return
      lastCursor = key
      features.onCursor!(line, col)
    }, 250)
  })
  const view = new EditorView({
    state: EditorState.create({
      doc: text,
      extensions: [
        presenceGutter,
        cursorWatch,
        entry.wrap!.of(features.wrap ? EditorView.lineWrapping : []),
        ...(languageExtension(features.language) ? [languageExtension(features.language)!, foldGutter(), bracketMatching(), indentOnInput(), keymap.of([...foldKeymap, { key: "Mod-/", run: toggleComment }])] : []),
        lineNumbers(),
        highlightActiveLineGutter(),
        highlightActiveLine(),
        drawSelection(),
        rectangularSelection(),
        crosshairCursor(),
        history(),
        // Find/replace (Milestone 7): CodeMirror's panel; Ctrl+H opens it too
        // (replacements are ordinary document changes, so Rust sees them
        // through onChange like typing).
        search({ top: true }),
        highlightSelectionMatches(),
        keymap.of([...searchKeymap, { key: "Mod-h", run: openSearchPanel }]),
        autocompletion({ override: [wikiComplete, complete], activateOnTyping: true, maxRenderedOptions: 50 }),
        wikiField,
        wikiClick,
        keymap.of(completionKeymap),
        featureKeys,
        keymap.of([...defaultKeymap, ...historyKeymap, indentWithTab]),
        lintGutter(),
        hover,
        gotoDef,
        oneDark,
        EditorView.updateListener.of((u) => {
          if (u.docChanged) {
            const changes: Splice[] = []
            u.changes.iterChanges((fromA, toA, _fromB, _toB, inserted) => { changes.push({ from: fromA, to: toA, insert: inserted.toString() }) })
            onChange(changes, u.state.doc.length)
          }
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

/** A menu action on the editor (spec 009): the same things the keys do. */
function run(el: HTMLElement, action: string): void {
  const e = views.get(el)
  if (!e) return
  const v = e.view
  const f = e.features
  const { line, col } = lspPos(v, v.state.selection.main.head)
  switch (action) {
    case "find": openSearchPanel(v); break
    case "replace": openSearchPanel(v); break
    case "rename": { if (f.onRename) { const w = v.state.wordAt(v.state.selection.main.head); f.onRename(line, col, w ? v.state.sliceDoc(w.from, w.to) : "") } break }
    case "codeActions": { if (f.onCodeActions) { const s = v.state.selection.main; const a = lspPos(v, s.from), b = lspPos(v, s.to); f.onCodeActions(a.line, a.col, b.line, b.col) } break }
    case "definition": f.onDefinition?.(line, col); break
    case "references": f.onReferences?.(line, col); break
    case "toggleComment": toggleComment(v); break
    case "foldAll": foldAll(v); break
    case "unfoldAll": unfoldAll(v); break
  }
  v.focus()
}

/** Soft wrap on/off (spec 014). */
function setWrap(el: HTMLElement, wrap: boolean): void {
  const e = views.get(el)
  if (e) e.view.dispatch({ effects: e.wrap.reconfigure(wrap ? EditorView.lineWrapping : []) })
}

/** `[[link]]` spans and whether they resolve (spec 012). */
function setWikiLinks(el: HTMLElement, spans: WikiSpan[]): void {
  const view = views.get(el)?.view
  if (view) view.dispatch({ effects: setWikiEffect.of(spans) })
}

/** Other people's positions in this document (Milestone 9). */
function setPresence(el: HTMLElement, marks: PresenceMark[]): void {
  const view = views.get(el)?.view
  if (view) view.dispatch({ effects: setPresenceEffect.of(marks) })
}

/** Rust's answer to a completion request (Milestone 7). */
function completionResult(el: HTMLElement, id: number, items: CompletionItem[] | null): void {
  const e = views.get(el)
  const resolve = e?.pendingCompletion.get(id)
  if (resolve) { e!.pendingCompletion.delete(id); resolve(items) }
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
  // Replace only the changed middle (common prefix/suffix kept) so the
  // cursor and scroll position survive a rename or an agent edit.
  const old = view.state.doc.toString()
  if (old === text) return
  let start = 0
  const max = Math.min(old.length, text.length)
  while (start < max && old.charCodeAt(start) === text.charCodeAt(start)) start++
  let endOld = old.length, endNew = text.length
  while (endOld > start && endNew > start && old.charCodeAt(endOld - 1) === text.charCodeAt(endNew - 1)) { endOld--; endNew-- }
  view.dispatch({ changes: { from: start, to: endOld, insert: text.slice(start, endNew) } })
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
window.moonkale.codemirror = { mount, setText, getText, focus, undo, redo, destroy, setLspDiagnostics, hoverResult, completionResult, setCursor, setPresence, setWikiLinks, setWrap, run }
