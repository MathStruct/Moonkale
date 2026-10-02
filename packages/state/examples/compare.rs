//! The store comparison of Milestone 18 phase 5 (ADR-0014): every backend
//! behind `StateStore`, the same workload, one table of numbers.
//!
//!     cargo run --release -p moonkale-state --example compare \
//!         --features redb,sqlite,turso,rocksdb,helix -- [DIR] [EVENTS]
//!
//! `COMPARE_ONLY=redb,sqlite` limits the run to those backends;
//! `COMPARE_MODE=relaxed` opens them with `Durability::Relaxed` (default:
//! `Durable`, an fsync per commit).
//!
//! DIR must be on a real disk (not a tmpfs) — fsync is part of what is
//! measured. EVENTS defaults to 100 000. Prints a markdown table.
//!
//! Workload, modelled on Moonkale's state (`moonkale_state::tables`):
//! - open: create an empty store; reopen it after the run (cold open of a full store);
//! - log append: EVENTS entity-log events of ~300 bytes, one batch each (an
//!   edit is one event), keyed `folder · seq`;
//! - log append, batched: the same in batches of 1 000 (an import);
//! - replay: one prefix scan over a folder's events;
//! - settings: 200 writes of a ~2 KB record (each layout change), p50/p99;
//! - get: 10 000 random event reads;
//! - size on disk after the run;
//! - crash: a child process writes two-key batches until it is killed (-9);
//!   the store must reopen and no batch may be half there;
//! - second process: can another process open the store while it is open?

use moonkale_state::{Batch, Key, StateStore};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

type Open = fn(&Path) -> Arc<dyn StateStore>;

fn relaxed() -> bool {
    std::env::var("COMPARE_MODE").as_deref() == Ok("relaxed")
}

fn mode() -> moonkale_state::Durability {
    if relaxed() {
        moonkale_state::Durability::Relaxed
    } else {
        moonkale_state::Durability::Durable
    }
}

fn backends() -> Vec<(&'static str, Open)> {
    let mut v: Vec<(&'static str, Open)> = Vec::new();
    #[cfg(feature = "redb")]
    v.push(("redb", |p| {
        Arc::new(moonkale_state::RedbStore::open(p).unwrap())
    }));
    #[cfg(feature = "sqlite")]
    v.push(("sqlite", |p| {
        Arc::new(moonkale_state::SqliteStore::open(p).unwrap())
    }));
    #[cfg(feature = "turso")]
    v.push(("turso", |p| {
        Arc::new(moonkale_state::TursoStore::open(p).unwrap())
    }));
    #[cfg(feature = "rocksdb")]
    v.push(("rocksdb", |p| {
        Arc::new(moonkale_state::RocksStore::open(p).unwrap())
    }));
    #[cfg(feature = "helix")]
    v.push(("helix", |p| {
        Arc::new(moonkale_state::HelixStore::open(p).unwrap())
    }));
    v
}

fn event(seq: u64) -> Vec<u8> {
    format!(
        r#"{{"v":1,"data":{{"id":"0190f{seq:027x}","at":{seq},"actor":"user:daniel","kind":{{"Content":{{"node":"6f1c2a3b-4d5e-4f60-8a7b-9c0d1e2f3a4b","patch":"@@ -1,3 +1,4 @@ line {seq}"}}}},"tx":{seq}}}}}"#
    )
    .into_bytes()
}

fn ms(d: Duration) -> String {
    format!("{:.1}", d.as_secs_f64() * 1e3)
}

fn du(path: &Path) -> u64 {
    fn tree(p: &Path) -> u64 {
        if p.is_dir() {
            std::fs::read_dir(p)
                .map(|it| it.flatten().map(|e| tree(&e.path())).sum())
                .unwrap_or(0)
        } else {
            p.metadata().map(|m| m.len()).unwrap_or(0)
        }
    }
    if path.is_dir() {
        return tree(path);
    }
    // A file store and its companions (SQLite's -wal and -shm).
    let base = path.file_name().unwrap().to_string_lossy().into_owned();
    std::fs::read_dir(path.parent().unwrap())
        .map(|it| {
            it.flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with(&base))
                .map(|e| tree(&e.path()))
                .sum()
        })
        .unwrap_or(0)
}

/// Child mode: write two-key batches until killed.
fn crash_child(name: &str, path: &Path) {
    let open = backends().into_iter().find(|b| b.0 == name).unwrap().1;
    let s = open(path);
    println!("ready");
    for i in 0u64.. {
        let v = i.to_be_bytes().to_vec();
        s.write(
            Batch::new()
                .put("pair", Key::new().str("a").u64(i), v.clone())
                .put("pair", Key::new().str("b").u64(i), v),
        )
        .unwrap();
    }
}

/// Spawn the child, let it write, kill it, reopen, check pairs.
fn crash_test(name: &str, open: Open, path: &Path) -> String {
    let exe = std::env::current_exe().unwrap();
    let mut child = Command::new(exe)
        .args(["--crash-child", name, &path.to_string_lossy()])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::BufRead;
        let out = child.stdout.take().unwrap();
        let mut line = String::new();
        std::io::BufReader::new(out).read_line(&mut line).unwrap();
    }
    std::thread::sleep(Duration::from_millis(1500));
    child.kill().unwrap(); // SIGKILL on Unix
    child.wait().unwrap();
    let reopened = std::panic::catch_unwind(|| open(path));
    let s = match reopened {
        Ok(s) => s,
        Err(_) => return "**did not reopen**".into(),
    };
    let a = s.scan("pair", Key::new().str("a").as_bytes()).unwrap();
    let b = s.scan("pair", Key::new().str("b").as_bytes()).unwrap();
    if a.len() == b.len() {
        format!("ok ({} batches kept)", a.len())
    } else {
        format!("**half a batch** ({} vs {})", a.len(), b.len())
    }
}

/// Can a second process open the store while this one holds it?
fn second_process(name: &str, path: &Path) -> &'static str {
    let exe = std::env::current_exe().unwrap();
    let st = Command::new(exe)
        .args(["--open-only", name, &path.to_string_lossy()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    if st.success() {
        "yes"
    } else {
        "no (locked)"
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--crash-child") {
        crash_child(&args[2], Path::new(&args[3]));
        return;
    }
    if args.get(1).map(String::as_str) == Some("--open-only") {
        let open = backends().into_iter().find(|b| b.0 == args[2]).unwrap().1;
        let s = open(Path::new(&args[3]));
        let _ = s.get("t", b"k").unwrap();
        return;
    }
    let dir = PathBuf::from(args.get(1).cloned().unwrap_or_else(|| "compare-out".into()));
    let events: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(100_000);
    std::fs::create_dir_all(&dir).unwrap();
    println!(
        "events: {events}; durability: {:?}; dir: {}\n",
        mode(),
        dir.display()
    );
    println!("| backend | create | append 1/batch | append 1000/batch | replay | settings p50 / p99 | 10k gets | reopen full | size | crash (kill -9) | 2nd process |");
    println!("|---|---|---|---|---|---|---|---|---|---|---|");
    // COMPARE_ONLY=redb,sqlite runs only those (one process per engine
    // keeps a slow one from holding up the table).
    let only: Option<Vec<String>> = std::env::var("COMPARE_ONLY")
        .ok()
        .map(|s| s.split(',').map(str::to_string).collect());
    for (name, open) in backends() {
        if only.as_ref().is_some_and(|o| !o.iter().any(|n| n == name)) {
            continue;
        }
        let path = dir.join(format!("{name}.store"));
        let _ = std::fs::remove_dir_all(&path);
        let _ = std::fs::remove_file(&path);
        let t = Instant::now();
        let s = open(&path);
        let create = t.elapsed();

        let t = Instant::now();
        for seq in 0..events {
            s.write(Batch::new().put("events", Key::new().str("folder:a").u64(seq), event(seq)))
                .unwrap();
        }
        let append1 = t.elapsed();

        let t = Instant::now();
        let mut seq = 0;
        while seq < events {
            let mut b = Batch::new();
            for _ in 0..1000 {
                b = b.put("events", Key::new().str("folder:b").u64(seq), event(seq));
                seq += 1;
            }
            s.write(b).unwrap();
        }
        let append_b = t.elapsed();

        let t = Instant::now();
        let n = s
            .scan("events", Key::new().str("folder:a").as_bytes())
            .unwrap()
            .len();
        let replay = t.elapsed();
        assert_eq!(n as u64, events);

        let settings = vec![b'x'; 2048];
        let mut lat = Vec::new();
        for i in 0..200u64 {
            let t = Instant::now();
            s.write(Batch::new().put(
                "settings",
                Key::new().str("user").u64(i % 3),
                settings.clone(),
            ))
            .unwrap();
            lat.push(t.elapsed());
        }
        lat.sort();

        let t = Instant::now();
        let mut x = 12345u64;
        for _ in 0..10_000 {
            x = x
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let seq = (x >> 33) % events;
            assert!(s
                .get("events", Key::new().str("folder:a").u64(seq).as_bytes())
                .unwrap()
                .is_some());
        }
        let gets = t.elapsed();

        let second = second_process(name, &path);
        drop(s);
        let t = Instant::now();
        let s = open(&path);
        let _ = s
            .get("events", Key::new().str("folder:a").u64(0).as_bytes())
            .unwrap();
        let reopen = t.elapsed();
        drop(s);
        let size = du(&path);

        let cpath = dir.join(format!("{name}.crash"));
        let _ = std::fs::remove_dir_all(&cpath);
        let _ = std::fs::remove_file(&cpath);
        let crash = crash_test(name, open, &cpath);

        println!(
            "| {name} | {} ms | {} ms ({:.0}/s) | {} ms | {} ms | {} / {} ms | {} ms | {} ms | {:.1} MB | {crash} | {second} |",
            ms(create),
            ms(append1),
            events as f64 / append1.as_secs_f64(),
            ms(append_b),
            ms(replay),
            ms(lat[lat.len() / 2]),
            ms(lat[lat.len() * 99 / 100]),
            ms(gets),
            ms(reopen),
            size as f64 / 1e6,
        );
    }
}
