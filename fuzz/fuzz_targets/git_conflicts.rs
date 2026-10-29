#![no_main]
#[path = "../../packages/extensions/git/src/conflict_model.rs"]
mod conflict_model;
libfuzzer_sys::fuzz_target!(|text: &str| {
    if let Ok(doc) = conflict_model::ConflictDocument::parse(text) {
        let _ = doc.initialize(text);
        let mut result = doc.result.clone();
        for i in 0..doc.hunks.len() {
            result = doc.pick(&result, i, i % 3);
        }
        assert!(result.len() <= doc.result.len());
        let _ = doc.unresolved(&result);
    }
});
