#![no_main]
#[path = "../../packages/extensions/git/src/everyday_model.rs"]
mod everyday_model;
#[allow(dead_code)]
#[path = "../../packages/extensions/git/src/everyday_types.rs"]
mod everyday_types;
libfuzzer_sys::fuzz_target!(|text: &str| {
    let selected: Vec<_> = text
        .lines()
        .enumerate()
        .filter(|(i, _)| i % 2 == 0)
        .take(100_000)
        .map(|(i, _)| i)
        .collect();
    if let Ok(patch) = everyday_model::selection(text, &selected, false) {
        assert!(patch.len() <= 4 * 1024 * 1024);
    }
    let _ = everyday_model::selection(text, &selected, true);
    let _ = everyday_model::parse_blame(text);
    let rows = everyday_model::recovery(text);
    assert!(rows.len() <= 200);
});
