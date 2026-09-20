// [[wiki-links]] in the rich editor (spec 012). A ProseMirror plugin that
// (1) decorates `[[target#heading|alias]]` text spans as links — the text
// stays literal so the markdown round trip is exact — hiding the brackets
// unless the caret is inside the link, (2) follows a link on plain click,
// and (3) offers `[[` completion. Everything that needs the index (which
// targets resolve, which pages exist) is asked from Rust; this file only
// draws. Rules of packages/js: no application state, DOM only inside `el`.
import { Plugin, PluginKey, type EditorState, type Transaction } from "@milkdown/kit/prose/state"
import { Decoration, DecorationSet, type EditorView } from "@milkdown/kit/prose/view"

export type WikiCandidate = { target: string; key: string }
export type WikiHooks = {
  /** Plain click on a link: follow (Rust creates the page if it is missing). */
  onFollow: (target: string) => void
  /** `[[` + query typed: ask Rust for candidates; answer with `complete(id, items)`. */
  onQuery: (id: number, query: string) => void
}

const LINK_RE = /\[\[([^\]\[\n|#]+)(#[^\]|\n]*)?(\|[^\]\n]*)?\]\]/g

type Status = Map<string, boolean> // target (as written, lowercased) → resolved

/** Per-editor mutable state the plugin reads; Rust updates it through the wrapper. */
export class WikiState {
  status: Status = new Map()
  nextQuery = 1
  pending: { id: number; from: number; to: number; query: string } | null = null
  items: WikiCandidate[] = []
  selected = 0
  popup: HTMLElement | null = null
  view: EditorView | null = null
}

export const wikiKey = new PluginKey<DecorationSet>("moonkale-wiki")

function decorate(state: EditorState, ws: WikiState): DecorationSet {
  const decos: Decoration[] = []
  const sel = state.selection
  state.doc.descendants((node, pos) => {
    if (!node.isText || !node.text) return
    const text = node.text
    LINK_RE.lastIndex = 0
    let m: RegExpExecArray | null
    while ((m = LINK_RE.exec(text))) {
      const from = pos + m.index
      const to = from + m[0].length
      const target = m[1].trim()
      const resolved = ws.status.get(target.toLowerCase())
      const cls = resolved === false ? "mk-wikilink mk-wikilink-unresolved" : "mk-wikilink"
      const caretInside = sel.from >= from && sel.to <= to
      decos.push(Decoration.inline(from, to, { class: cls, "data-target": target, title: resolved === false ? `${target} — no page yet (click to create)` : target }))
      if (!caretInside) {
        // Hide `[[`, `]]`, and the target when an alias is shown.
        decos.push(Decoration.inline(from, from + 2, { class: "mk-wiki-bracket" }))
        decos.push(Decoration.inline(to - 2, to, { class: "mk-wiki-bracket" }))
        if (m[3]) {
          const aliasStart = from + 2 + m[1].length + (m[2]?.length ?? 0) + 1
          decos.push(Decoration.inline(from + 2, aliasStart, { class: "mk-wiki-bracket" }))
        }
      }
    }
  })
  return DecorationSet.create(state.doc, decos)
}

/** `[[query` before the caret in the current text block, if any. */
function queryBefore(state: EditorState): { from: number; to: number; query: string } | null {
  const { $from } = state.selection
  if (!$from.parent.isTextblock) return null
  const before = $from.parent.textBetween(0, $from.parentOffset, undefined, "￼")
  const i = before.lastIndexOf("[[")
  if (i < 0) return null
  const query = before.slice(i + 2)
  if (query.includes("]]") || query.includes("\n")) return null
  // Inside an already closed link (`[[Al|pha]]`): the caret was placed there
  // by a click, not by typing — no completion.
  const after = $from.parent.textBetween($from.parentOffset, $from.parent.content.size, undefined, "\ufffc")
  const close = after.indexOf("]]"), open = after.indexOf("[[")
  if (close >= 0 && (open < 0 || close < open)) return null
  const start = $from.pos - query.length - 2
  return { from: start, to: $from.pos, query }
}

function closePopup(ws: WikiState): void {
  ws.pending = null
  ws.items = []
  ws.selected = 0
  ws.popup?.remove()
  ws.popup = null
}

function renderPopup(ws: WikiState, root: HTMLElement): void {
  if (!ws.view || !ws.pending) return
  if (!ws.popup) {
    ws.popup = document.createElement("div")
    ws.popup.className = "mk-wiki-popup"
    root.appendChild(ws.popup)
  }
  const coords = ws.view.coordsAtPos(ws.pending.to)
  const box = root.getBoundingClientRect()
  ws.popup.style.left = `${coords.left - box.left}px`
  ws.popup.style.top = `${coords.bottom - box.top + 4}px`
  ws.popup.innerHTML = ""
  if (!ws.items.length) {
    const d = document.createElement("div")
    d.className = "mk-wiki-item mk-wiki-empty"
    d.textContent = "No page — Enter creates it"
    ws.popup.appendChild(d)
    return
  }
  ws.items.forEach((it, i) => {
    const d = document.createElement("div")
    d.className = i === ws.selected ? "mk-wiki-item mk-active" : "mk-wiki-item"
    const t = document.createElement("span"); t.className = "mk-wiki-target"; t.textContent = it.target
    const k = document.createElement("span"); k.className = "mk-wiki-key"; k.textContent = it.key
    d.append(t, k)
    d.addEventListener("mousedown", (e) => { e.preventDefault(); accept(ws, i) })
    ws.popup!.appendChild(d)
  })
}

function accept(ws: WikiState, index: number): void {
  const v = ws.view
  const p = ws.pending
  if (!v || !p) return
  const target = ws.items[index]?.target ?? v.state.doc.textBetween(p.from + 2, p.to).trim()
  if (!target) { closePopup(ws); return }
  // Replace `[[query` with `[[target]]` (keep `]]` if the user typed it already).
  const after = v.state.doc.textBetween(p.to, Math.min(p.to + 2, v.state.doc.content.size))
  const insert = after === "]]" ? `[[${target}` : `[[${target}]]`
  const tr = v.state.tr.insertText(insert, p.from, p.to)
  v.dispatch(tr)
  closePopup(ws)
  v.focus()
}

export function wikiPlugin(ws: WikiState, root: HTMLElement, hooks: WikiHooks): Plugin<DecorationSet> {
  return new Plugin<DecorationSet>({
    key: wikiKey,
    state: {
      init: (_, state) => decorate(state, ws),
      apply: (tr: Transaction, old: DecorationSet, _o, state: EditorState) =>
        tr.docChanged || tr.selectionSet || tr.getMeta(wikiKey) ? decorate(state, ws) : old,
    },
    view: (view) => {
      ws.view = view
      return {
        update: (v) => {
          // Completion: `[[query` before the caret opens/updates the popup.
          const q = queryBefore(v.state)
          if (!q) { if (ws.pending) closePopup(ws); return }
          if (!ws.pending || ws.pending.from !== q.from || ws.pending.query !== q.query) {
            const id = ws.nextQuery++
            ws.pending = { id, from: q.from, to: q.to, query: q.query }
            hooks.onQuery(id, q.query)
          } else {
            ws.pending.to = q.to
          }
        },
        destroy: () => { closePopup(ws); ws.view = null },
      }
    },
    props: {
      decorations: (state) => wikiKey.getState(state) ?? null,
      handleDOMEvents: { blur: () => { closePopup(ws); return false } },
      handleKeyDown: (view, e) => {
        if (!ws.pending || !ws.popup) return false
        if (e.key === "ArrowDown") { ws.selected = Math.min(ws.selected + 1, Math.max(ws.items.length - 1, 0)); renderPopup(ws, root); return true }
        if (e.key === "ArrowUp") { ws.selected = Math.max(ws.selected - 1, 0); renderPopup(ws, root); return true }
        if (e.key === "Enter" || e.key === "Tab") { accept(ws, ws.selected); return true }
        if (e.key === "Escape") { closePopup(ws); return true }
        void view
        return false
      },
      handleClick: (view, pos, e) => {
        const t = (e.target as HTMLElement | null)?.closest?.(".mk-wikilink") as HTMLElement | null
        if (!t) return false
        const target = t.getAttribute("data-target")
        if (!target) return false
        // Plain click follows; a click with the caret already inside the
        // link (editing it) is left to the editor. Ctrl/Cmd always follows.
        const $pos = view.state.doc.resolve(pos)
        const sel = view.state.selection
        if (!(e.ctrlKey || e.metaKey) && sel.from <= $pos.pos && sel.to >= $pos.pos && !sel.empty) return false
        e.preventDefault()
        hooks.onFollow(target)
        return true
      },
    },
  })
}

/** Rust → view: which targets resolve. */
export function setStatus(ws: WikiState, entries: { target: string; resolved: boolean }[]): void {
  ws.status = new Map(entries.map((e) => [e.target.toLowerCase(), e.resolved]))
  const v = ws.view
  if (v) v.dispatch(v.state.tr.setMeta(wikiKey, true))
}

/** Rust → view: completion answer. */
export function complete(ws: WikiState, root: HTMLElement, id: number, items: WikiCandidate[]): void {
  if (!ws.pending || ws.pending.id !== id) return
  ws.items = items
  ws.selected = 0
  renderPopup(ws, root)
}
