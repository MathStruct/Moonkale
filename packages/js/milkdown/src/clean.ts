// Markdown clean-up for what the rich view reports (Prompt26). No imports, so
// it can be tested on its own: `node --test test/clean.test.mjs` after building it.

const BR_LINE = /^[ \t]*<br[ \t]*\/?>[ \t]*$/i
const FENCE = /^[ \t]{0,3}(`{3,}|~{3,})/

/**
 * Milkdown's CommonMark preset writes every empty paragraph as a `<br />`
 * line so blank lines survive a round trip (`remarkPreserveEmptyLinePlugin`).
 * Moonkale's files should not fill up with them (Prompt26): outside fenced
 * code, drop lines that are only `<br />` and let runs of blank lines
 * collapse to one, which is all markdown distinguishes anyway. The plugin
 * stays registered so files that already contain such lines still open
 * without showing them.
 */
export function dropEmptyLineBreaks(md: string): string {
  const out: string[] = []
  let fence: string | null = null
  let blank = false
  for (const line of md.split("\n")) {
    if (fence) {
      out.push(line)
      const m = FENCE.exec(line)
      if (m && m[1][0] === fence[0] && m[1].length >= fence.length && line.trim() === m[1]) fence = null
      continue
    }
    const open = FENCE.exec(line)
    if (open) { fence = open[1]; blank = false; out.push(line); continue }
    if (BR_LINE.test(line)) continue
    const isBlank = line.trim() === ""
    if (isBlank && blank) continue
    blank = isBlank
    out.push(line)
  }
  return out.join("\n")
}
