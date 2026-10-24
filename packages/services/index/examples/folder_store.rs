//! Measure ADR-0029's optional folder store without starting a UI/server.
#[cfg(feature = "sophia")]
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use moonkale_core::Source;
    use moonkale_index::{
        folder_store::{CodeFile, FolderStore},
        walk,
    };
    use std::{path::PathBuf, time::Instant};
    let root =
        PathBuf::from(std::env::args().nth(1).ok_or("folder path required")?).canonicalize()?;
    for pass in ["build", "reopen"] {
        let start = Instant::now();
        // Reopen the folder and re-read the admitted text on each pass.
        let folder = moonkale_project_fs::FolderSource::open(&root)?;
        let writer = FolderStore::open(&root)?;
        let (entries, truncated) =
            walk::walk(&folder, folder.descriptor().root, &Default::default()).await?;
        if truncated {
            return Err("walk truncated".into());
        }
        let mut files = Vec::new();
        for (_, node) in entries {
            if walk::wants_text(&node, &Default::default())
                && matches!(node.language_hint(), Some("rust" | "python" | "julia"))
            {
                if !writer.admits_path(&node.native_key) {
                    continue;
                }
                let (text, _) = folder.fetch_text(node.id).await?;
                files.push(CodeFile { node, text });
            }
        }
        let snapshot = start.elapsed().as_secs_f64();
        let report = writer.sync(&files)?;
        println!("{pass}: total_seconds={:.3} snapshot_seconds={snapshot:.3} store_seconds={:.3} store_bytes={} files={} source_bytes={} report={report:?}",start.elapsed().as_secs_f64(),start.elapsed().as_secs_f64()-snapshot,std::fs::metadata(writer.path())?.len(),files.len(),files.iter().map(|f| f.text.len()).sum::<usize>());
    }
    Ok(())
}
#[cfg(not(feature = "sophia"))]
fn main() {
    eprintln!("enable the sophia feature");
}
