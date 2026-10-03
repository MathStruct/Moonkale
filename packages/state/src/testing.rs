//! The conformance suite every [`StateStore`] backend must pass (Milestone 18
//! phase 3). Call [`conformance`] with a factory for empty stores and
//! [`persistence`] with one that opens a store at a path; a backend's own
//! test file is two lines (see `tests/backends.rs`).
//!
//! What it checks is the whole contract of the trait: missing keys and
//! tables read as empty; put, overwrite and delete; scans are prefix-exact and
//! ordered; tables are separate; a batch applies in order and atomically, as
//! seen by a concurrent reader; records round-trip and refuse newer versions;
//! a reopened store keeps what was committed.

use crate::{decode, encode, Batch, Key, Record, StateError, StateStore, Typed};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Note {
    text: String,
    #[serde(default)]
    tags: Vec<String>,
}

impl Record for Note {
    const TABLE: &'static str = "test_notes";
    const VERSION: u32 = 2;
}

/// Run every check that needs only an empty store.
pub fn conformance(new: impl Fn() -> Arc<dyn StateStore>) {
    missing_is_empty(&*new());
    put_overwrite_delete(&*new());
    scans_are_prefix_exact_and_ordered(&*new());
    tables_are_separate(&*new());
    batches_apply_in_order(&*new());
    records_round_trip_and_version(&*new());
    batches_are_atomic_for_readers(new());
}

/// Run the checks that reopen a store: `open(path)` must open (or create)
/// the store at `path`.
pub fn persistence(open: impl Fn(&std::path::Path) -> Arc<dyn StateStore>) {
    let dir = std::env::temp_dir().join(format!(
        "moonkale-state-conformance-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("state");
    {
        let s = open(&path);
        s.write(Batch::new().put("t", b"kept".to_vec(), b"1".to_vec()))
            .unwrap();
    }
    {
        let s = open(&path);
        assert_eq!(s.get("t", b"kept").unwrap().as_deref(), Some(&b"1"[..]));
        s.write(Batch::new().delete("t", b"kept".to_vec())).unwrap();
    }
    {
        let s = open(&path);
        assert_eq!(
            s.get("t", b"kept").unwrap(),
            None,
            "a delete must persist too"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

fn missing_is_empty(s: &dyn StateStore) {
    assert_eq!(s.get("never", b"k").unwrap(), None);
    assert!(s.scan("never", b"").unwrap().is_empty());
    s.write(Batch::new().delete("never", b"k".to_vec()))
        .unwrap();
    s.write(Batch::new()).unwrap();
}

fn put_overwrite_delete(s: &dyn StateStore) {
    s.write(Batch::new().put("t", b"k".to_vec(), b"1".to_vec()))
        .unwrap();
    assert_eq!(s.get("t", b"k").unwrap().as_deref(), Some(&b"1"[..]));
    s.write(Batch::new().put("t", b"k".to_vec(), b"2".to_vec()))
        .unwrap();
    assert_eq!(s.get("t", b"k").unwrap().as_deref(), Some(&b"2"[..]));
    s.write(Batch::new().delete("t", b"k".to_vec())).unwrap();
    assert_eq!(s.get("t", b"k").unwrap(), None);
    // Empty values are values.
    s.write(Batch::new().put("t", b"e".to_vec(), Vec::new()))
        .unwrap();
    assert_eq!(s.get("t", b"e").unwrap().as_deref(), Some(&b""[..]));
}

fn scans_are_prefix_exact_and_ordered(s: &dyn StateStore) {
    let mut b = Batch::new();
    for (folder, seq) in [("a", 10), ("ab", 1), ("a", 2), ("b", 0), ("a", 300)] {
        b = b.put("events", Key::new().str(folder).u64(seq), seq.to_string());
    }
    s.write(b).unwrap();
    let a: Vec<u64> = s
        .scan("events", Key::new().str("a").as_bytes())
        .unwrap()
        .into_iter()
        .map(|(k, _)| {
            let mut r = Key::reader(&k);
            assert_eq!(r.str().as_deref(), Some("a"));
            r.u64().unwrap()
        })
        .collect();
    assert_eq!(a, [2, 10, 300], "folder `a` only, in sequence order");
    assert_eq!(s.scan("events", b"").unwrap().len(), 5);
}

fn tables_are_separate(s: &dyn StateStore) {
    s.write(Batch::new().put("one", b"k".to_vec(), b"1".to_vec()).put(
        "two",
        b"k".to_vec(),
        b"2".to_vec(),
    ))
    .unwrap();
    assert_eq!(s.get("one", b"k").unwrap().as_deref(), Some(&b"1"[..]));
    assert_eq!(s.get("two", b"k").unwrap().as_deref(), Some(&b"2"[..]));
    assert_eq!(s.scan("one", b"").unwrap().len(), 1);
}

fn batches_apply_in_order(s: &dyn StateStore) {
    s.write(
        Batch::new()
            .put("t", b"k".to_vec(), b"first".to_vec())
            .put("t", b"k".to_vec(), b"second".to_vec())
            .put("t", b"gone".to_vec(), b"x".to_vec())
            .delete("t", b"gone".to_vec()),
    )
    .unwrap();
    assert_eq!(s.get("t", b"k").unwrap().as_deref(), Some(&b"second"[..]));
    assert_eq!(s.get("t", b"gone").unwrap(), None);
}

fn records_round_trip_and_version(s: &dyn StateStore) {
    let typed = Typed::new(s);
    let key = Key::new().str("folder:x");
    let note = Note {
        text: "hello".into(),
        tags: vec!["a".into()],
    };
    typed.put(&key, &note).unwrap();
    assert_eq!(typed.get::<Note>(&key).unwrap(), Some(note));
    // An older version without `tags` still reads (the default migration).
    s.write(Batch::new().put(
        Note::TABLE,
        Key::new().str("old").into_bytes(),
        br#"{"v":1,"data":{"text":"old"}}"#.to_vec(),
    ))
    .unwrap();
    let old: Note = typed.get(&Key::new().str("old")).unwrap().unwrap();
    assert_eq!(old.text, "old");
    // A newer version is refused, not misread.
    let newer = br#"{"v":3,"data":{"text":"x"}}"#;
    assert!(matches!(
        decode::<Note>(newer),
        Err(StateError::TooNew { found: 3, .. })
    ));
    assert!(encode(&old).starts_with(br#"{"v":2"#));
}

/// A writer replaces two keys together, over and over; a reader on another
/// thread must never see one changed without the other.
fn batches_are_atomic_for_readers(s: Arc<dyn StateStore>) {
    s.write(Batch::new().put("pair", b"a".to_vec(), b"0".to_vec()).put(
        "pair",
        b"b".to_vec(),
        b"0".to_vec(),
    ))
    .unwrap();
    let writer = {
        let s = s.clone();
        std::thread::spawn(move || {
            for i in 1..=300u32 {
                let v = i.to_string().into_bytes();
                s.write(Batch::new().put("pair", b"a".to_vec(), v.clone()).put(
                    "pair",
                    b"b".to_vec(),
                    v,
                ))
                .unwrap();
            }
        })
    };
    while !writer.is_finished() {
        let both = s.scan("pair", b"").unwrap();
        assert_eq!(both.len(), 2);
        assert_eq!(both[0].1, both[1].1, "a reader saw half a batch");
    }
    writer.join().unwrap();
}
