#![no_main]
use std::collections::HashMap;
libfuzzer_sys::fuzz_target!(|source: &str| {
    if let Ok(mut e) = moonkale_graph_render::egraph::EGraph::parse(source) {
        let mut g = moonkale_graph_render::graph::Graph::from_input(e.graph().clone());
        let _ = e.ancestry("a");
        if let Ok(d) = e.collapse("", true, &HashMap::new()) {
            g.apply_delta(d);
        }
        if let Ok(d) = e.collapse("", false, &HashMap::new()) {
            g.apply_delta(d);
        }
    }
});
