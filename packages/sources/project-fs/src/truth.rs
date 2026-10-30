//! Folder truth marker shared by folder detection and the derived-store guard.
use moonkale_core::SourceTruth;
use std::{io, io::Read, path::Path};

/// The marker is metadata, never an instruction to migrate a store.
pub const MARKER_PATH: &str = ".moonkale/truth.json";
const MAX_MARKER_BYTES: u64 = 4096;

/// Missing markers mean file truth. Invalid, unreadable or linked metadata
/// fails closed: callers must not treat a potentially authored store as cache.
pub fn detect(root: &Path) -> io::Result<SourceTruth> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "invalid folder truth marker");
    for path in [root.join(".moonkale"), root.join(MARKER_PATH)] {
        match path.symlink_metadata() {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(SourceTruth::Files),
            Err(e) => return Err(e),
            Ok(m) if m.file_type().is_symlink() => return Err(invalid()),
            Ok(m) if path == root.join(".moonkale") && !m.is_dir() => {
                return Ok(SourceTruth::Files);
            }
            Ok(m) if path == root.join(MARKER_PATH) && !m.is_file() => return Err(invalid()),
            Ok(_) => (),
        }
    }
    let file = std::fs::File::open(root.join(MARKER_PATH))?;
    if !file.metadata()?.is_file() {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    file.take(MAX_MARKER_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_MARKER_BYTES {
        return Err(invalid());
    }
    let marker: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    match marker.get("truth").and_then(|v| v.as_str()) {
        Some("store") => Ok(SourceTruth::Store),
        Some("files") => Ok(SourceTruth::Files),
        _ => Err(invalid()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moonkale_core::Source;

    #[test]
    fn folder_descriptor_detects_truth_and_only_files_truth_grants_plain_writes() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(detect(dir.path()).unwrap(), SourceTruth::Files);
        std::fs::create_dir(dir.path().join(".moonkale")).unwrap();
        for (json, truth) in [
            (
                r#"{"truth":"store","since":"2026-10-27"}"#,
                SourceTruth::Store,
            ),
            (r#"{"truth":"files"}"#, SourceTruth::Files),
        ] {
            std::fs::write(dir.path().join(MARKER_PATH), json).unwrap();
            let descriptor = crate::FolderSource::open(dir.path()).unwrap().descriptor();
            assert_eq!(descriptor.truth, truth);
            assert_eq!(descriptor.capabilities.write, truth == SourceTruth::Files);
        }
        for invalid in ["{", r#"{"truth":"unknown"}"#, &" ".repeat(4097)] {
            std::fs::write(dir.path().join(MARKER_PATH), invalid).unwrap();
            assert!(detect(dir.path()).is_err());
            assert!(crate::FolderSource::open(dir.path()).is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn linked_marker_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".moonkale")).unwrap();
        let target = dir.path().join("truth.json");
        std::fs::write(&target, r#"{"truth":"store"}"#).unwrap();
        std::os::unix::fs::symlink(target, dir.path().join(MARKER_PATH)).unwrap();
        assert!(detect(dir.path()).is_err());
    }
}
