// The two libc symbols tree-sitter's wasm build wants and nothing provides
// (see csrc/wasm_stubs.c, P-114). Native targets have a real libc.
fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    println!("cargo:rerun-if-changed=csrc/wasm_stubs.c");
    if target.starts_with("wasm32") {
        cc::Build::new()
            .file("csrc/wasm_stubs.c")
            .warnings(false)
            .compile("moonkale_wasm_stubs");
    }
}
