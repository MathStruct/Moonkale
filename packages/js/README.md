# JS interop packages

Each subfolder is **one** TypeScript dependency, isolated, built into a single
self-contained JS file that a Rust crate loads with `asset!()`. Nothing here
holds application state. The protocol is the same for all three:

```text
Rust ──document::eval("moonkale.<pkg>.mount(id, opts)")──►  JS
Rust ──document::eval("moonkale.<pkg>.apply(id, patch)")──►  JS
JS   ──CustomEvent("moonkale:<pkg>", {id, kind, payload})──►  Rust (onmoonkale listener)
```

Rules:
1. A package exposes **only** `mount / apply / set* / destroy` and emits
   events. No package imports another. No package touches the DOM outside its
   mount element.
2. All application semantics (language, links, decorations, LSP) come from
   Rust as data. JS packages contain no language knowledge.
3. Each package has a `PROTOCOL.md` describing every message. That document
   is the spec a Rust-native replacement must satisfy; when one exists, the
   crate's `backend` feature flips and the folder is deleted.

Build: `npm run build` in each folder (esbuild, ESM, single file, no
externals) → `dist/<pkg>.js`, committed so `dx` builds need no Node.

| package    | wraps                              | used by                         |
|------------|------------------------------------|---------------------------------|
| codemirror | @codemirror/state, view, commands  | `packages/editors/code`         |
| milkdown   | @milkdown/core, preset-commonmark, | `packages/editors/markdown`     |
|            | + custom wiki-link/embed plugins   |                                 |
| xterm      | @xterm/xterm, fit, webgl addons    | `packages/editors/terminal`     |
