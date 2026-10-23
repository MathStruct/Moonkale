#![no_main]
libfuzzer_sys::fuzz_target!(|path: &str| {
    if path.len() > 65536 {
        return;
    }
    for region in moonkale_graph_render::graph::filled_regions(path).take(256) {
        assert!(path.contains(region));
    }
});
