//! LadybugDB as a shared library (Milestone 17, packaging/lbug-shared.sh):
//! the `lbug` build script's rpath does not reach dependent binaries (a
//! dependency's `rustc-link-arg` is ignored), so this crate sets it. A
//! package points it at its install location with `MOONKALE_LBUG_RPATH`
//! (`/usr/lib/Moonkale` in the Arch package); a development build uses the
//! directory `lbug-shared.sh` fetched.
fn main() {
    println!("cargo:rerun-if-env-changed=LBUG_SHARED");
    println!("cargo:rerun-if-env-changed=LBUG_LIBRARY_DIR");
    println!("cargo:rerun-if-env-changed=MOONKALE_LBUG_RPATH");
    let ladybug = std::env::var_os("CARGO_FEATURE_LADYBUG").is_some();
    let shared = std::env::var_os("LBUG_SHARED").is_some();
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if !(ladybug && shared) || !matches!(target_os.as_str(), "linux" | "macos") {
        return;
    }
    let dir = std::env::var("MOONKALE_LBUG_RPATH").or_else(|_| std::env::var("LBUG_LIBRARY_DIR"));
    if let Ok(dir) = dir {
        println!("cargo:rustc-link-arg-bins=-Wl,-rpath,{dir}");
    }
}
