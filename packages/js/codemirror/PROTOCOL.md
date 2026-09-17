# @moonkale/codemirror protocol

The bundle sets `window.moonkale.codemirror` with five functions. Rust drives
them through one `document::eval` per mounted editor, using Dioxus's
`dioxus.send()` / `dioxus.recv()` channel. A Rust-native backend must satisfy
the same messages.

## JS API

| function | effect |
|---|---|
| `mount(el, text, onChange)` | create an editor inside `el` showing `text`; `onChange(fullText)` after every document change |
| `setText(el, text)` | replace the whole document (reload / revert); fires `onChange` |
| `getText(el)` | current document text |
| `focus(el)` | focus the editor |
| `destroy(el)` | tear down; safe to call twice |

## Messages Rust → JS (`eval.send`)

```json
{ "kind": "setText", "text": "…" }
{ "kind": "focus" }
{ "kind": "destroy" }
```

## Messages JS → Rust (`dioxus.send`)

```json
{ "kind": "ready" }
{ "kind": "change", "text": "…" }
```

`change` carries the **whole document** in Milestone 1. Splices
(`{start,end,text}` in char offsets) are the planned replacement; the Rust
`TextPatch` type already accepts them.

## Lifecycle

1. Rust renders a `div` with a stable `id` and starts the eval.
2. The eval polls until `window.moonkale.codemirror` exists (the bundle is a
   deferred `<script>`), mounts, sends `ready`.
3. Edits stream as `change`; Rust updates its `Document`.
4. On panel unmount Rust drops the eval; JS receives nothing more. The
   element is removed by Dioxus, which releases the view via the WeakMap.
