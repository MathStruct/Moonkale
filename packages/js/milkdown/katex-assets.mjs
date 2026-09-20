// Copies KaTeX's stylesheet and woff2 fonts to editors/markdown/assets/katex/
// (a dx folder asset: the folder name is hashed, the files inside keep their
// names, so `fonts/…` inside katex.min.css keeps resolving). Only woff2 —
// every webview we target prefers it; woff/ttf fallbacks would never load.
import { mkdirSync, readdirSync, copyFileSync, readFileSync, writeFileSync } from "node:fs"
const src = "node_modules/katex/dist", out = "../../editors/markdown/assets/katex"
mkdirSync(`${out}/fonts`, { recursive: true })
let n = 0
for (const f of readdirSync(`${src}/fonts`)) if (f.endsWith(".woff2")) { copyFileSync(`${src}/fonts/${f}`, `${out}/fonts/${f}`); n++ }
// Drop the woff/ttf sources so no request is ever made for files we do not ship.
const css = readFileSync(`${src}/katex.min.css`, "utf8").replace(/,url\(fonts\/[^)]+\.(woff|ttf)\) format\("(woff|truetype)"\)/g, "")
writeFileSync(`${out}/katex.min.css`, css)
const ver = JSON.parse(readFileSync(`${src}/../package.json`, "utf8")).version
console.log(`katex ${ver}: ${n} woff2 fonts + katex.min.css → ${out}`)
