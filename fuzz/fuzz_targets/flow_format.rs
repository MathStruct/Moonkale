#![no_main]
libfuzzer_sys::fuzz_target!(|source: &str| {
    moonkale_fuzz::flow_format(source);
});
