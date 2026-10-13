# TeX presentation protocol

Rust parses Markdown, retains canonical source ranges and owns navigation,
selection, history, macros and the bounded workspace cache. The isolated local
bundle exposes `window.moonkale.math.render(request)`; it never scans Markdown,
reads a vault, navigates, or mutates document/editor state.

Request: `{tex: string, display: boolean, macros: JSON object}`. The caller sends
it through Dioxus eval's JSON channel. Reply: `{html: string | null, error: boolean}`.
Only successful KaTeX-generated HTML/MathML is inserted into a math-owned slot.
Failure displays literal canonical text with a localized error indication.

The renderer pins KaTeX 0.18.7, emits `htmlAndMathml`, clones macro definitions per
request, disables trusted commands, limits expansions to 1000 and requested
sizes to 20 em, accepts at most 16 KiB of UTF-8 TeX and returns at most 1 MiB of
markup. It catches all errors. The loader coalesces script loading and times out
a failed local asset load after 15 seconds. No CDN or external font/image fetch
is part of this protocol.

A Rust replacement must preserve these replies, source ownership and failure
behavior. Build/check/test: `npm ci && npm run check && npm run build && npm test`.
The committed bundle and shared stylesheet/fonts make application builds
independent of Node. Stylesheet/fonts are also used by Rich mode.
