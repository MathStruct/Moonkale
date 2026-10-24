//! Opt-in ADR-0029 gate 4 measurement; never writes to the input repository.
#[cfg(feature = "sophia")]
#[path = "../tests/replay_support/mod.rs"]
mod replay_support;
#[cfg(feature = "sophia")]
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("usage: history_replay REPO RANGE SCRATCH_DIRECTORY".into());
    }
    let result = replay_support::replay(
        std::path::Path::new(&args[1]),
        &args[2],
        std::path::Path::new(&args[3]),
        50,
        &mut std::io::stdout(),
    )
    .await?;
    if result.errors + result.findings > 0 {
        return Err("replay recorded findings/errors; see JSONL output".into());
    }
    Ok(())
}
#[cfg(not(feature = "sophia"))]
fn main() {
    eprintln!("enable the sophia feature");
}
