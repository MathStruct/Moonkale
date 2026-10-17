#![no_main]
libfuzzer_sys::fuzz_target!(|source: &str| {
    moonkale_fuzz::math_delimiters(source);
});
