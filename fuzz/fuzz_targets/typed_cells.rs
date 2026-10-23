#![no_main]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    if data.len() > 65536 {
        return;
    }
    let _ = serde_json::from_slice::<moonkale_core::Value>(data);
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = moonkale_core::DateTimeCell::parse(text);
    }
});
