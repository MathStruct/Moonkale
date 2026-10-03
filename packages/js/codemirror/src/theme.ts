// The editor's colours as CSS custom properties (spec 030): the shell's theme
// defines --mk-* on <html>; this theme only names them, so switching the theme
// restyles every open editor without reconfiguring it. Same structure as
// @codemirror/theme-one-dark, which it replaces.

import { EditorView } from "@codemirror/view"
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language"
import { tags as t } from "@lezer/highlight"

const v = (name: string) => `var(--mk-${name})`

const editorTheme = EditorView.theme({
  "&": { color: v("code-ink"), backgroundColor: v("code-bg") },
  ".cm-content": { caretColor: v("code-cursor") },
  ".cm-cursor, .cm-dropCursor": { borderLeftColor: v("code-cursor") },
  "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection": { backgroundColor: v("code-selection") },
  ".cm-panels": { backgroundColor: v("code-panel"), color: v("code-ink") },
  ".cm-panels.cm-panels-top": { borderBottom: `1px solid ${v("line")}` },
  ".cm-panels.cm-panels-bottom": { borderTop: `1px solid ${v("line")}` },
  ".cm-searchMatch": { backgroundColor: v("code-match"), outline: `1px solid ${v("code-match-outline")}` },
  ".cm-searchMatch.cm-searchMatch-selected": { backgroundColor: v("code-match-current") },
  ".cm-activeLine": { backgroundColor: v("code-active-line") },
  ".cm-selectionMatch": { backgroundColor: v("code-selection-match") },
  "&.cm-focused .cm-matchingBracket, &.cm-focused .cm-nonmatchingBracket": { backgroundColor: v("code-bracket") },
  ".cm-gutters": { backgroundColor: v("code-bg"), color: v("code-gutter"), border: "none" },
  ".cm-activeLineGutter": { backgroundColor: v("code-active-line") },
  ".cm-foldPlaceholder": { backgroundColor: "transparent", border: "none", color: v("code-gutter") },
  ".cm-tooltip": { border: `1px solid ${v("line")}`, backgroundColor: v("code-panel"), color: v("code-ink") },
  ".cm-tooltip .cm-tooltip-arrow:before": { borderTopColor: "transparent", borderBottomColor: "transparent" },
  ".cm-tooltip .cm-tooltip-arrow:after": { borderTopColor: v("code-panel"), borderBottomColor: v("code-panel") },
  ".cm-tooltip-autocomplete": { "& > ul > li[aria-selected]": { backgroundColor: v("code-selection"), color: v("code-ink") } },
})

const highlight = HighlightStyle.define([
  { tag: t.keyword, color: v("syn-keyword") },
  { tag: [t.name, t.deleted, t.character, t.propertyName, t.macroName], color: v("syn-name") },
  { tag: [t.function(t.variableName), t.labelName], color: v("syn-function") },
  { tag: [t.color, t.constant(t.name), t.standard(t.name)], color: v("syn-constant") },
  { tag: [t.definition(t.name), t.separator], color: v("code-ink") },
  { tag: [t.typeName, t.className, t.number, t.changed, t.annotation, t.modifier, t.self, t.namespace], color: v("syn-type") },
  { tag: [t.operator, t.operatorKeyword, t.url, t.escape, t.regexp, t.link, t.special(t.string)], color: v("syn-operator") },
  { tag: [t.meta, t.comment], color: v("syn-comment") },
  { tag: t.strong, fontWeight: "bold" },
  { tag: t.emphasis, fontStyle: "italic" },
  { tag: t.strikethrough, textDecoration: "line-through" },
  { tag: t.link, color: v("syn-comment"), textDecoration: "underline" },
  { tag: t.heading, fontWeight: "bold", color: v("syn-name") },
  { tag: [t.atom, t.bool, t.special(t.variableName)], color: v("syn-constant") },
  { tag: [t.processingInstruction, t.string, t.inserted], color: v("syn-string") },
  { tag: t.invalid, color: v("danger") },
])

/** The theme for every editor (replaces one-dark). */
export const themeFromTokens = [editorTheme, syntaxHighlighting(highlight)]
