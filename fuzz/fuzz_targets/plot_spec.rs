#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    moonkale_fuzz::plot_spec(data);
});
