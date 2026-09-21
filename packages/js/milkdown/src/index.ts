// @moonkale/milkdown — thin wrapper around Milkdown's Crepe editor. See PROTOCOL.md.
//
// Rules (packages/js/README.md): no application state, no DOM outside the
// mount element. Rust owns the markdown; this file shows it and reports edits
// as whole documents (like @moonkale/codemirror in its first milestone).

import { Crepe } from "@milkdown/crepe"
import { $prose, replaceAll } from "@milkdown/kit/utils"
import { WikiState, wikiPlugin, setStatus as wikiSetStatus, complete as wikiComplete, type WikiCandidate } from "./wiki"
import "@milkdown/crepe/theme/common/style.css"
import "@milkdown/crepe/theme/frame-dark.css"

type OnChange = (markdown: string) => void
type OnWikiLink = (target: string) => void
type OnWikiQuery = (id: number, query: string) => void
type MountOptions = { katexMacros?: Record<string, string>; onWikiQuery?: OnWikiQuery }
type Entry = { crepe: Crepe; suppress: boolean; last: string; wiki: WikiState }

const views = new WeakMap<HTMLElement, Entry>()

/** remark escapes `[[` / `]]`; wiki-links must survive the round trip. */
function unescapeWiki(md: string): string {
  return md.replace(/\\\[\\\[/g, "[[").replace(/\\\]\\\]/g, "]]").replace(/\[\[([^\]\n]*?)\\\]\]/g, "[[$1]]")
}

async function mount(el: HTMLElement, markdown: string, onChange: OnChange, onWikiLink?: OnWikiLink, opts: MountOptions = {}): Promise<void> {
  destroy(el)
  const entry: Entry = { crepe: undefined as unknown as Crepe, suppress: false, last: markdown, wiki: new WikiState() }
  const crepe = new Crepe({
    root: el,
    defaultValue: markdown,
    features: {
      // No image upload UI (pictures are spec 008). LaTeX on: inline `$…$`
      // and block `$$…$$` render with KaTeX; its CSS + fonts are the
      // `assets/katex/` folder that Rust links (spec 013).
      [Crepe.Feature.ImageBlock]: false,
      [Crepe.Feature.Latex]: true,
    },
    featureConfigs: {
      [Crepe.Feature.Latex]: { katexOptions: { throwOnError: false, macros: { ...(opts.katexMacros ?? {}) } } },
    },
  })
  entry.crepe = crepe
  // [[wiki-links]]: decorations, click-to-follow, `[[` completion (spec 012).
  crepe.editor.use($prose(() => wikiPlugin(entry.wiki, el, {
    onFollow: (target) => onWikiLink?.(target),
    onQuery: (id, query) => opts.onWikiQuery?.(id, query),
  })))
  crepe.on((listener) => {
    listener.markdownUpdated((_ctx, raw, prev) => {
      if (entry.suppress || raw === prev) return
      const md = unescapeWiki(raw)
      if (md === entry.last) return
      entry.last = md
      onChange(md)
    })
  })
  // Loading is not an edit: Crepe's parse → serialize round trip changes
  // the text a little (bullets, escapes, blank lines), and reporting that
  // would mark the document dirty on open (spec 021). The normalised form
  // becomes the baseline; the first real edit is reported relative to it,
  // and the file is written in the normalised form only when the user
  // actually changed something.
  entry.suppress = true
  try {
    await crepe.create()
    entry.last = unescapeWiki(crepe.getMarkdown())
  } finally {
    entry.suppress = false
  }
  views.set(el, entry)
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
/** Rust → view: which `[[targets]]` resolve (decoration classes). */
function setWikiStatus(el: HTMLElement, entries: { target: string; resolved: boolean }[]): void {
  const e = views.get(el)
  if (e) wikiSetStatus(e.wiki, entries)
}

/** Rust → view: the answer to `onWikiQuery(id, …)`. */
function wikiCandidates(el: HTMLElement, id: number, items: WikiCandidate[]): void {
  const e = views.get(el)
  if (e) wikiComplete(e.wiki, el, id, items)
}

window.moonkale.milkdown = { mount, setText, getText, focus, destroy, setWikiStatus, wikiCandidates }
