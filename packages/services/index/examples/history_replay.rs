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
    // SPEC-010 acceptance 2: MOONKALE_REPLAY_TRUTH_AT=<commits>|half moves
    // the truth into the store after that many commits.
    let truth_at = match std::env::var("MOONKALE_REPLAY_TRUTH_AT").ok().as_deref() {
        None => None,
        Some("half") => {
            let listed = replay_support::git(
                std::path::Path::new(&args[1]),
                &["rev-list", "--count", &args[2], "--"],
            )?;
            Some(
                String::from_utf8(listed)?
                    .trim()
                    .parse::<usize>()?
                    .div_ceil(2),
            )
        }
        Some(n) => Some(n.parse()?),
    };
    let result = replay_support::replay_with(
        std::path::Path::new(&args[1]),
        &args[2],
        std::path::Path::new(&args[3]),
        50,
        truth_at,
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
