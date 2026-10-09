//! The engine's wasm/JS are build OUTPUTS (graph-render's build.sh writes
//! them into assets/) — cargo rebuilds this crate only when its sources
//! change, so a rebuilt engine kept serving under the previous asset URL
//! hash (P-159, P-168). Emitting the bytes' hash into a generated file
//! the library includes makes every engine rebuild recompile the crate
//! and re-expand `asset!`, so the hash dx serves always matches.

use std::path::Path;

fn fnv(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn main() {
    println!("cargo:rerun-if-changed=assets/graph_render.js");
    println!("cargo:rerun-if-changed=assets/graph_render_bg.wasm");
    let js = std::fs::read(Path::new("assets/graph_render.js")).unwrap_or_default();
    let wasm = std::fs::read(Path::new("assets/graph_render_bg.wasm")).unwrap_or_default();
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let src = format!(
        "/// The engine build's hash (build.rs) — forces this crate to\n\
         /// recompile whenever the wasm changes.\n\
         pub const ASSET_VERSION: u64 = {};\n",
        fnv(&js) ^ fnv(&wasm).rotate_left(17)
    );
    let path = out.join("asset_version.rs");
    // Only rewrite when it differs, or every build would dirty the crate.
    if std::fs::read_to_string(&path)
        .map(|old| old != src)
        .unwrap_or(true)
    {
        std::fs::write(&path, src).expect("write asset_version.rs");
    }
}
