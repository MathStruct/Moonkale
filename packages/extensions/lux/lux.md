---
title: "ext-lux — implementation notes"
tags: [crate-notes, milestone-6]
---
Notes for `moonkale-ext-lux` (Milestone 6): the **Lux.jl block library** for the flow editor, an **opt-in** extension (`dev.moonkale.ext-lux`) that contributes nothing but `Extension::flow_libraries()`. Daniel's constraint: nothing Lux-specific is loaded unless the user enables it.

- `library.rs` — `library() -> FlowLibrary` (`id: "lux"`, language `julia`): blocks `input` (shape params → `Tensor`), `dense` (units, activation), `conv` (channels, kernel, activation), `maxpool` (window), `flatten`, `dropout` (p), `batchnorm`, `loss` (kind), `optimiser` (kind, learning rate). Ports are tensor-shaped; `Input` is any-rank so the same graph works for images and vectors.
- `codegen.rs` — `generate(flow, lib) -> Generated { file_name: "model.jl", text, run_hint: "julia model.jl" }`: walks the flow topologically from `input`, emits a `Lux.Chain(...)` with `Dense`/`Conv`/`MaxPool`/`FlattenLayer`/`Dropout`/`BatchNorm` and a training scaffold (`Optimisers`, `Zygote`, a random batch, one step) — `Dense` after `Flatten` uses a lazily computed `flat_features` so kernel/pool arithmetic never leaks into the UI. Errors are explicit (`the flow has no layers between Input and Loss`, `chain too long (cycle?)`, an unsupported block by id).
- Tests: a CNN flow generates the expected layers; a wrong graph reports its block; `generated_file_parses_in_julia` runs `julia -e 'Meta.parseall(read(...))'` when `julia` is on PATH (1.12 here; `Lux` itself is not installed, so running is a user action).

Next: an MTK / data-pipeline library as a second contribution, execute-in-place through the terminal with output back into the flow.
