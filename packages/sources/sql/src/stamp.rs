//! Changes on disk for file-backed SQL sources (PLT-09, live plots).
//!
//! A training run that appends rows to `metrics.csv`, or another program
//! that writes to a SQLite file, changes the data behind an open table. These
//! sources answer [`Source::changes_since`](moonkale_core::Source::changes_since)
//! from a fingerprint of their files (path, existence, length, modification
//! time), without a watcher thread or any state:
//!
//! - the answer's `seq` is the fingerprint (never 0), an opaque token that the
//!   caller passes back;
//! - `reset` is true when the fingerprint differs from `since` (and `since` is
//!   not 0, the first call): something changed, re-read the source;
//! - the call returns at once. It is not a long poll, so a web client does
//!   not hold one connection open per database; the follow loop paces the
//!   calls (`Workspace::follow`).
//!
//! What is fingerprinted: the database file and its journal files (SQLite
//! `-wal`/`-journal`, DuckDB `.wal`), or a data folder's CSV/TSV/Parquet
//! files. Moonkale's own row writes change the files too, so they are
//! reported like anyone else's.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// `path` followed by `path<suffix>` for each suffix (journal files).
pub(crate) fn with_suffixes(path: &Path, suffixes: &[&str]) -> Vec<PathBuf> {
    let mut out = vec![path.to_path_buf()];
    for s in suffixes {
        let mut p = path.as_os_str().to_owned();
        p.push(s);
        out.push(PathBuf::from(p));
    }
    out
}

/// The fingerprint of `files`: nonzero, and different when any file appears,
/// disappears, or changes length or modification time.
pub(crate) fn fingerprint(files: &[PathBuf]) -> u64 {
    let mut h = blake3::Hasher::new();
    for f in files {
        h.update(f.as_os_str().as_encoded_bytes());
        h.update(&[0]);
        match std::fs::metadata(f) {
            Ok(m) => {
                let mtime = m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                h.update(&[1]);
                h.update(&m.len().to_le_bytes());
                h.update(&mtime.to_le_bytes());
            }
            Err(_) => {
                h.update(&[2]);
            }
        }
    }
    let bytes: [u8; 8] = h.finalize().as_bytes()[..8].try_into().expect("8 bytes");
    u64::from_le_bytes(bytes).max(1)
}

/// The answer to `changes_since(since)` for `files`.
pub(crate) fn changes(files: &[PathBuf], since: u64) -> moonkale_core::Changes {
    let seq = fingerprint(files);
    moonkale_core::Changes {
        seq,
        paths: Vec::new(),
        reset: since != 0 && since != seq,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_rewrites_and_removal_change_the_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("metrics.csv");
        std::fs::write(&f, "step,loss\n1,0.9\n").unwrap();
        let files = with_suffixes(&f, &["-wal"]);
        assert_eq!(files[1], dir.path().join("metrics.csv-wal"));

        let first = changes(&files, 0);
        assert!(!first.reset, "the first call is the baseline");
        assert_ne!(first.seq, 0);
        let again = changes(&files, first.seq);
        assert!(!again.reset && again.seq == first.seq, "nothing changed");

        // An append.
        use std::io::Write;
        std::fs::OpenOptions::new()
            .append(true)
            .open(&f)
            .unwrap()
            .write_all(b"2,0.7\n")
            .unwrap();
        let appended = changes(&files, first.seq);
        assert!(appended.reset);
        assert_ne!(appended.seq, first.seq);

        // A journal file appearing.
        std::fs::write(&files[1], "").unwrap();
        let journal = changes(&files, appended.seq);
        assert!(journal.reset);

        // Removal.
        std::fs::remove_file(&f).unwrap();
        let gone = changes(&files, journal.seq);
        assert!(gone.reset);
        assert!(!changes(&files, gone.seq).reset);
    }

    #[test]
    fn no_files_still_gives_a_nonzero_token() {
        assert_ne!(fingerprint(&[]), 0);
        assert!(!changes(&[], fingerprint(&[])).reset);
    }
}
