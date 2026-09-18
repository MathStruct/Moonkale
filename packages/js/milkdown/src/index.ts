// @moonkale/milkdown — thin wrapper around Milkdown's Crepe editor. See PROTOCOL.md.
//
// Rules (packages/js/README.md): no application state, no DOM outside the
// mount element. Rust owns the markdown; this file shows it and reports edits
// as whole documents (like @moonkale/codemirror in its first milestone).

import { Crepe } from "@milkdown/crepe"
import { replaceAll } from "@milkdown/kit/utils"
import "@milkdown/crepe/theme/common/style.css"
import "@milkdown/crepe/theme/frame-dark.css"

type OnChange = (markdown: string) => void
type OnWikiLink = (target: string) => void
type Entry = { crepe: Crepe; suppress: boolean; last: string }

const views = new WeakMap<HTMLElement, Entry>()

/** remark escapes `[[` / `]]`; wiki-links must survive the round trip. */
function unescapeWiki(md: string): string {
  return md.replace(/\\\[\\\[/g, "[[").replace(/\\\]\\\]/g, "]]").replace(/\[\[([^\]\n]*?)\\\]\]/g, "[[$1]]")
}

async function mount(el: HTMLElement, markdown: string, onChange: OnChange, onWikiLink?: OnWikiLink): Promise<void> {
  destroy(el)
  const entry: Entry = { crepe: undefined as unknown as Crepe, suppress: false, last: markdown }
  const crepe = new Crepe({
    root: el,
    defaultValue: markdown,
    features: {
      // Keep the surface small: no image upload UI, no LaTeX.
      [Crepe.Feature.ImageBlock]: false,
      [Crepe.Feature.Latex]: false,
    },
  })
  entry.crepe = crepe
  crepe.on((listener) => {
    listener.markdownUpdated((_ctx, raw, prev) => {
      if (entry.suppress || raw === prev) return
      const md = unescapeWiki(raw)
      if (md === entry.last) return
      entry.last = md
      onChange(md)
    })
  })
  await crepe.create()
  views.set(el, entry)
  // Ctrl/Cmd+click on a [[wiki-link]]: find the brackets around the caret.
  el.addEventListener("click", (e) => {
    if (!(e.ctrlKey || e.metaKey) || !onWikiLink) return
    const target = wikiLinkAt(e.clientX, e.clientY)
    if (target) { e.preventDefault(); onWikiLink(target) }
  })
}

function wikiLinkAt(x: number, y: number): string | null {
  const doc = document as Document & { caretPositionFromPoint?: (x: number, y: number) => { offsetNode: Node; offset: number } | null }
  let node: Node | null = null
  let offset = 0
  if (doc.caretPositionFromPoint) {
    const p = doc.caretPositionFromPoint(x, y)
    if (p) { node = p.offsetNode; offset = p.offset }
  } else if (document.caretRangeFromPoint) {
    const r = document.caretRangeFromPoint(x, y)
    if (r) { node = r.startContainer; offset = r.startOffset }
  }
  if (!node || node.nodeType !== Node.TEXT_NODE) return null
  const text = node.textContent ?? ""
  const open = text.lastIndexOf("[[", offset)
  const close = text.indexOf("]]", open + 2)
  if (open < 0 || close < 0 || offset > close + 2) return null
  const inner = text.slice(open + 2, close)
  return inner.split("|")[0].split("#")[0].trim() || null
}

/** Replace the document (reload / revert / switched from source mode). */
function setText(el: HTMLElement, markdown: string): void {
  const e = views.get(el)
  if (!e || markdown === e.last) return
  e.suppress = true
  try { e.crepe.editor.action(replaceAll(markdown)); e.last = markdown } finally { e.suppress = false }
}

function getText(el: HTMLElement): string | undefined {
  const e = views.get(el)
  return e ? unescapeWiki(e.crepe.getMarkdown()) : undefined
}

function focus(el: HTMLElement): void {
  const pm = el.querySelector(".ProseMirror") as HTMLElement | null
  pm?.focus()
}

function destroy(el: HTMLElement): void {
  const e = views.get(el)
  if (e) { e.crepe.destroy(); views.delete(el) }
}

declare global { interface Window { moonkale?: Record<string, unknown> } }
window.moonkale = window.moonkale ?? {}
window.moonkale.milkdown = { mount, setText, getText, focus, destroy }
