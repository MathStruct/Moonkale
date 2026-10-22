# Canvas component layer

Shared UI layer for 2D graph surfaces; see **Components on the canvas**
spec in `markdown/specifications/README.md` and Project Structure.

`CanvasCamera` consumes the engine's CSS-pixel camera event. `CanvasLayer` clips
one world-space container above both canvases, using one translate/scale transform.
`CanvasItem` takes `[left, top, width, height]` in world coordinates and unmounts
its children when off screen, in 3D, or below its projected minimum size (default
100 × 32 CSS pixels). The layer passes pointer events through; items stop clicks,
double clicks, pointer presses and wheel events. Keyboard policy stays with the host.

`CanvasHeader` captures a primary pointer and reports world-space deltas, including
completion/cancellation, to the host. Rust owns drag state; the only JavaScript is
pointer capture by element id, passed as serialized data through Dioxus eval.
The host owns node positions, document synchronization and undo, including closing an active gesture if LOD removes its header. There is no
extension, shell, model or renderer dependency, so graph, flow and future canvas
editors can use the same layer without depending on another editor extension.

Flow is the first user: one selected block's parameter editor is anchored beside
its block at `(x + 60, y - 18)`. It scales with the camera, preserves field keyboard
handling, and its header moves the block with one undo step. Engine nodes remain
available when the card unmounts. `InNode.embed` is available for a future view
that replaces a node's label/symbol with content inside its rectangle; flow's
parameter card remains a sidecar and retains the block's schematic.

A future node-kind contribution can resolve a view factory in the host, wrap the
returned `Element` in `CanvasItem`, and mark the corresponding engine node
`embed:true`. Neither that registry nor extension permissions belong in this crate.

Verification: crate visibility tests and `canvas-layer.mjs` against the production
flow panel; `graph-camera.mjs` exercises committed engine assets separately.
