---
title: "ADR-0024 — Block symbols: a kind draws its own schematic"
tags: [adr, flow, graph, julia, modelingtoolkit]
status: accepted
date: 2026-10-14
---
**Status:** accepted 2026-10-14. Daniel's feedback on the first MTK and RxInfer screenshots: blocks were plain filled boxes, and every acausal pin sat on the box's top edge. *"A capacitor should have a nice looking symbol instead of being a bland square … as long as the flow editor can render everything I am fine."* Written by Claude. ADR-0023 is reserved for codex's open work. Related: [[ADR-0020 Undirected pins in the flow model]], [[ADR-0022 Bipartite factor graphs]], spec [[031]] (the graph engine's shapes, ports and overlay), [[Flow Editor]].

## Context

The flow panel drew every block as a filled rounded rect in its category colour, with the label beside it. Pins had fixed places:
- inputs on the left, outputs on the right;
- acausal pins (ADR-0020) spread along the top edge.

A circuit therefore read as a row of coloured boxes with all their wires leaving upwards: a resistor's two pins sat side by side on top, not at its two ends. Each library knows what its parts look like. The editor does not, and it should not have to: a ModelingToolkit capacitor, a Lux dense layer and an RxInfer factor are each drawn in their own field's convention.

The engine already had what a drawing needs:
- a 2D label overlay above the GPU canvas (labels and icons, spec 031 §2 stage 8);
- side-based ports with an offset along the side (spec 031 §2 stage 2);
- rect hit-testing.

## Decision

1. **`BlockKind.symbol: Option<Symbol>`** (`#[serde(default)]`; kinds finished with `..Default::default()` keep compiling). It is a library-level property, like `role`:
   ```rust
   pub struct Symbol { width: f32, height: f32, path: String, anchors: Vec<Anchor> }
   pub struct Anchor { port: String, side: Side, offset: f32 }   // offset 0..1, default 0.5
   pub enum Side { Left, Right, Top, Bottom }
   ```
   - `path` is **SVG path data only**, in the box `0 0 width height` (canvas units). It is stroked in the block's colour: no fills, no other SVG elements, nothing to sanitise beyond what `Path2D` parses.
   - `anchors` place each port on the box's sides.
   - `BlockPorts.symbol` carries the symbol through `Flow::ports_of`. A subflow and a boundary pseudo-block have none.
2. **The engine draws it.** `InNode.symbol` (and `InDeltaNode.symbol`, where absent keeps the old symbol, like `icon`) holds the path in the node's box. The overlay strokes it at ~1.6 px, zooming with the canvas, and in the hover colour (thicker) while hovered or selected. It is skipped below a few pixels (LOD) and in 3D.

   The GPU body stays, but only as a faint tint. It is still the hit area and still shows hover and selection, but the symbol *is* the block. The label sits centred below the symbol, as on a schematic.
3. **A better default for ports without an anchor**, so no kind hangs its pins off the top:
   - inputs on the left, outputs on the right (unchanged);
   - acausal pins alternate left and right, so a two-pin part lies in its circuit's line; a single pin goes on the left;
   - a factor (ADR-0022) has its first channel, the variable it generates, on the right and its parameters on the left;
   - a variable's one pin stays on its circle's top.

   Unanchored ports spread evenly along their side.
4. **ModelingToolkit draws its parts** (moonkale-julia):
   - Resistor: a DIN/IEC box between leads;
   - Capacitor: two plates;
   - Inductor: four turns;
   - Voltage: a circle with + on the `p` side;
   - Ground: three bars under the pin;
   - Mass: a cart on wheels;
   - Spring: a zigzag;
   - Damper: a dashpot;
   - Fixed: a hatched wall.

   A test checks that every kind has a symbol, that every pin is anchored, and that every point of the path stays in its box. Lux and RxInfer keep the box with the new default sides: dataflow left to right, and a factor's parameters in on the left with its variable out on the right.

5. **A block turns and mirrors (amended 2026-10-14, after Daniel's go-ahead).** `Block.turn` counts quarter turns clockwise (0–3), and `Block.mirror` flips the block left to right in its own frame, before the turn. Both are omitted from the file at their defaults, so an unturned block's file is unchanged.
   - The panel turns the box (its width and height swap on an odd turn) and every port: its side and offset map clockwise, after the mirror. The engine gets the turned box plus `InNode.turn`/`mirror`, and only the drawing needs the transform.
   - **R** turns the selected blocks, and **M** mirrors them as seen on the canvas: for a turned block, a mirror after the turn is the opposite turn after the mirror (`Block::flip`). The block editor carries both as buttons (⟳, ⇋). Each is one undo step.
   - A symbol with a pin on its bottom (an upright part) puts its label to its right, so the wire leaving there does not cross it.
   - The voltage source shows only its +: a turned minus reads as a bar.
6. **A kind may caption its blocks (amended 2026-10-14).** `BlockKind.caption` is a template under the block's name. Each `{name}` hole takes a value:
   - for a wired port of that name, the name of the block at the wire's other end;
   - otherwise the parameter's value, or its default;
   - `?` if the hole names neither.

   ModelingToolkit captions its parts with a value and unit (`1.0 Ω`, `100.0 N/m`). RxInfer captions each factor as it is written: `θ ~ Beta(2, 7)`, `y ~ Bernoulli(θ)`. The engine draws `InNode.caption` as a smaller, dimmer second line.

## Consequences

- **The change is additive.** `BlockKind` and the engine's `InNode` gain defaulted fields. `BlockPorts` gains `symbol`: a caller that builds it literally adds `symbol: None` (inside Moonkale, only the flow panel). It shipped in `ext-api-2026-10-14`; moonkale-julia v0.5.0 is its first user.
- **Files don't change.** The symbol belongs to the kind, not to the block. A `.flow.json` keeps its positions, and only the drawing and the pin sides differ.
- **§5–6 are additive too.** `Block` gains `turn` and `mirror`; a literal that lists every field adds `turn: 0, mirror: false`. `BlockKind` gains `caption`, and `BlockPorts` gains `caption` (`caption: None` in a literal). They ship in the next `ext-api` tag.
- **Not decided here:**
  - filled regions;
  - wires snapping to a grid.

  Each can follow when a library needs it.
