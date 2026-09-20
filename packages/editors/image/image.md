---
title: "editor-image — implementation notes"
tags: [crate-notes, milestone-10]
---
Notes for `moonkale-editor-image` (Milestone 10, spec 008). An **optional** extension (`dev.moonkale.editor-image`) that opens png / jpg / jpeg / gif / webp / svg / bmp / ico / avif files.

- `lib.rs`: `IMAGE_EXTENSIONS`, `is_image(node)`, `mime_of(key)`, `MAX_BYTES` (24 MB). One unit test.
- `extension.rs`: one `Main` panel per image node in `Workspace::views` (blobs open as views, not documents — `Workspace::open_node`); closing the panel closes the view.
- `panel.rs`: `ImagePanel` fetches the bytes once (`Workspace::fetch_bytes` → `Source::fetch_bytes`, new on the trait with a default `Unsupported`; the folder source reads the file, `RemoteSource` gets base64 from `/api/sources/fetch_bytes`) and shows them as a `data:` URL in an `<img>` — the webview decodes, SVG scripts never run. Toolbar: path, `w × h` (read after mount with `img.decode()` — `load` does not bubble to dioxus's delegated listener, P-094), size, **−** / zoom label / **+** (steps 10 %–800 %), **Fit**, **100 %**, and **Source** for SVGs (`Workspace::open_as_text`). Ctrl+wheel zooms, dragging pans (scrolls the stage through a small eval), keys `+ - 0 1`. Files above `MAX_BYTES` show a message instead of a 30 MB data URL.
- The Explorer opens blobs whose `ContentRef::Blob.mime` starts with `image/` (the folder source now sets a MIME for known extensions; `svg`, `bmp`, `avif` moved to the binary list so they are blobs).
- Not yet: a streaming route for very large images, EXIF rotation, animated-GIF controls, PDF.
