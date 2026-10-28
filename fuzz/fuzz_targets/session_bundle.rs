#![no_main]
libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = moonkale_state::session_bundle::parse::<serde_json::Value>(text);
    }
});
