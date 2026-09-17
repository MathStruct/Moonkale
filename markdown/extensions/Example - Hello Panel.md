---
tags: [extensions, example]
---
# Example — Hello Panel

A wasm extension contributing one panel and one command; the full walkthrough is in [[Writing an Extension]] §2–3. Key points the example demonstrates:

- **Manifest-first**: the "Hello" panel and "Hello: Greet" command appear in the workbench and palette *before* the wasm is instantiated.
- **State outside the panel**: `greetings` lives in the extension struct; docking the panel elsewhere keeps the count.
- **Declarative UI**: `ui::column([...])` is rendered by the host; the same panel works on web (Worker runtime) and desktop (wasmtime).
- **Command = LLM tool**: adding `llm_tool = true` to the manifest makes "greet" callable by the in-app agent, gated by policy (`risk = "read-only"`).

Variations to try: give the panel a `when = "activePanel == 'editor-code'"` so it only shows next to code; store `greetings` with `host.kv_set` so it survives restarts.
