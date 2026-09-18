//! # moonkale-ext-lux
//!
//! The Lux.jl block library for the flow editor (Milestone 6): layers
//! (`Dense`, `Conv`, `MaxPool`, `Flatten`, `Dropout`, `BatchNorm`),
//! activations, an `Input`, a `Loss` and an `Optimiser`, with tensor-shape
//! ports so a shape mismatch is a red wire, and **codegen** to a Julia file
//! (`Lux.Chain(...)` plus a training scaffold) run with `julia model.jl`.
//!
//! This extension is **opt-in**: it contributes nothing until enabled in
//! Settings → Extensions (Daniel's rule for niche/heavy features). It has no
//! Julia dependency itself; the generated file needs `Lux`, `Optimisers`,
//! `Zygote` and `Random` in the user's Julia environment.
//!
//! Vault: `editors/Flow Editor.md`.

mod codegen;
mod library;

use dioxus::prelude::*;
use moonkale_ext_api::prelude::*;

pub use library::library;

pub struct LuxExtension;

impl Extension for LuxExtension {
    fn manifest(&self) -> Manifest {
        Manifest::opt_in(
            "dev.moonkale.ext-lux",
            "Lux.jl library",
            "Blocks for building Lux.jl models in the flow editor and generating a Julia training script.",
        )
    }

    fn panels(&self, _ws: Workspace) -> Vec<PanelContribution> {
        Vec::new()
    }

    fn render(&self, panel_id: &str, _ws: Workspace) -> Element {
        rsx! { "unknown panel {panel_id}" }
    }

    fn flow_libraries(&self) -> Vec<moonkale_ext_api::flow::FlowLibrary> {
        vec![library()]
    }
}
