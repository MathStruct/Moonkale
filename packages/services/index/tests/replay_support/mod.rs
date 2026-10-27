//! Shared by the hermetic CI test and the opt-in history replay example.
use moonkale_core::{Query, Source};
use moonkale_index::{
    folder_store::{CodeFile, FolderStore},
    walk,
};
use moonkale_project_fs::FolderSource;
use moonkale_sources_sophia::SophiaSource;
use sophia_store::{sqlite::Sqlite, Store};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    time::Instant,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
use moonkale_index::folder_store::digest::{differences, state};

/// The Sophia source pinned in Cargo.lock, so reports name the store they measured.
fn sophia_pin() -> String {
    let lock = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../Cargo.lock");
    let text = std::fs::read_to_string(lock).unwrap_or_default();
    text.split("[[package]]")
        .find(|p| p.contains("name = \"sophia-store\""))
        .and_then(|p| p.lines().find_map(|l| l.strip_prefix("source = ")))
        .map(|s| {
            s.trim_matches('"')
                .rsplit_once('#')
                .map_or(s, |(_, rev)| rev)
                .to_string()
        })
        .unwrap_or_else(|| "unknown".into())
}

pub fn git(repo: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(output.stdout)
}

struct Blobs {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}
impl Blobs {
    fn open(repo: &Path) -> Result<Self> {
        let mut child = Command::new("git")
            .arg("--no-optional-locks")
            .arg("-C")
            .arg(repo)
            .args(["cat-file", "--batch"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        Ok(Self {
            input: child.stdin.take().unwrap(),
            output: BufReader::new(child.stdout.take().unwrap()),
            child,
        })
    }
    fn read(&mut self, id: &str, size: usize) -> Result<Vec<u8>> {
        writeln!(self.input, "{id}")?;
        self.input.flush()?;
        let mut header = String::new();
        self.output.read_line(&mut header)?;
        if header.trim() != format!("{id} blob {size}") {
            return Err("unexpected cat-file response".into());
        }
        let mut bytes = vec![0; size];
        self.output.read_exact(&mut bytes)?;
        let mut newline = [0];
        self.output.read_exact(&mut newline)?;
        if newline != *b"\n" {
            return Err("invalid cat-file delimiter".into());
        }
        Ok(bytes)
    }
}
impl Drop for Blobs {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn relevant(path: &str) -> bool {
    matches!(
        Path::new(path).extension().and_then(|e| e.to_str()),
        Some("rs" | "jl" | "py")
    ) || matches!(
        Path::new(path).file_name().and_then(|n| n.to_str()),
        Some("Cargo.toml" | "pyproject.toml" | ".gitignore" | ".ignore")
    )
}
fn safe(path: &str) -> bool {
    !path.contains('\\')
        && Path::new(path)
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
        && !path
            .split('/')
            .any(|p| matches!(p, ".git" | ".moonkale" | "target" | "node_modules"))
}

/// Materialize only regular bounded code and naming/ignore metadata, never links,
/// hooks, configuration secrets or executables. Every byte comes from Git blobs.
fn checkout(
    repo: &Path,
    commit: &str,
    root: &Path,
    old: &mut BTreeMap<String, String>,
    blobs: &mut Blobs,
) -> Result<BTreeSet<String>> {
    let listing = git(repo, &["ls-tree", "-r", "-l", "-z", commit])?;
    let mut next = BTreeMap::new();
    let mut changed = BTreeSet::new();
    for entry in listing.split(|b| *b == 0).filter(|e| !e.is_empty()) {
        let entry = std::str::from_utf8(entry)?;
        let (meta, path) = entry.split_once('\t').ok_or("invalid tree entry")?;
        let fields: Vec<_> = meta.split_whitespace().collect();
        if !safe(path) {
            continue;
        }
        if !relevant(path) || !matches!(fields[0], "100644" | "100755") {
            continue;
        }
        let size: usize = fields[3].parse()?;
        if size > 512 * 1024 {
            continue;
        }
        next.insert(path.to_string(), fields[2].to_string());
        if old.get(path).is_some_and(|id| id == fields[2]) {
            continue;
        }
        let bytes = blobs.read(fields[2], size)?;
        let dest = root.join(path);
        std::fs::create_dir_all(dest.parent().unwrap())?;
        std::fs::write(dest, bytes)?;
        changed.insert(path.to_string());
    }
    for path in old.keys().filter(|p| !next.contains_key(*p)) {
        std::fs::remove_file(root.join(path))?;
        changed.insert(path.clone());
    }
    *old = next;
    Ok(changed)
}
async fn inputs(root: &Path) -> Result<Vec<CodeFile>> {
    let source = FolderSource::open(root)?;
    let writer = FolderStore::open(root)?;
    let (entries, truncated) =
        walk::walk(&source, source.descriptor().root, &Default::default()).await?;
    if truncated {
        return Err("snapshot walk truncated".into());
    }
    let mut files = Vec::new();
    for (_, node) in entries {
        if walk::wants_text(&node, &Default::default())
            && matches!(node.language_hint(), Some("rust" | "julia" | "python"))
            && writer.admits_path(&node.native_key)
        {
            let (text, _) = source.fetch_text(node.id).await?;
            files.push(CodeFile { node, text });
        }
    }
    files.sort_by(|a, b| a.node.native_key.cmp(&b.node.native_key));
    Ok(files)
}
fn store(root: &Path) -> Result<Store<Sqlite>> {
    Ok(Store::new(Sqlite::open(
        root.join(moonkale_index::folder_store::STORE_PATH),
    )?))
}

/// Compare live semantics, not append-only records/logs or randomly allocated
/// nominal UUIDs. Generic identities are compared by their names and members.
fn counts(s: &Store<Sqlite>) -> Result<(u64, usize)> {
    Ok(s.read(|tx| {
        let ids = tx.records_of_kind(None, usize::MAX)?;
        let mut facts = 0;
        for id in ids {
            facts += tx.facts_of(&id)?.len();
        }
        Ok((tx.record_count()?, facts))
    })?)
}
fn summary(values: &mut [f64]) -> serde_json::Value {
    values.sort_by(f64::total_cmp);
    if values.is_empty() {
        return serde_json::json!({"n":0});
    }
    serde_json::json!({"n":values.len(), "median_ms":(values[(values.len()-1)/2]+values[values.len()/2])/2.0, "p95_ms":values[(values.len()*95).div_ceil(100)-1]})
}
async fn queries(root: &Path, s: &Store<Sqlite>) -> Result<serde_json::Value> {
    let names = s.read(|tx| tx.names_under("", usize::MAX))?;
    let mut lookup = Vec::new();
    let mut texts = Vec::new();
    let mut ids = Vec::new();
    for path in s.files()? {
        for (id, _, _) in s.file_manifest(&path)?.unwrap().spans {
            ids.push(id);
        }
    }
    ids.sort();
    ids.dedup();
    // Fixed-seed sampling with replacement: exactly 100 reads when nonempty.
    let mut seed = 0x4d6f6f6e6b616c65u64;
    for i in 0..100 {
        if !names.is_empty() {
            let start = Instant::now();
            s.resolve(&names[i % names.len()].path)?;
            lookup.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        if !ids.is_empty() {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let start = Instant::now();
            s.text_source(&ids[seed as usize % ids.len()], None)?;
            texts.push(start.elapsed().as_secs_f64() * 1000.0);
        }
    }
    let source = SophiaSource::open(root.join(moonkale_index::folder_store::STORE_PATH)).await?;
    let mut graph = Vec::new();
    let mut graph_error = None;
    let mut search = Vec::new();
    let mut search_error = None;
    for _ in 0..10 {
        let start = Instant::now();
        if let Err(e) = source
            .query(Query::All {
                limit: 1000,
                kinds: None,
            })
            .await
        {
            graph_error = Some(e.to_string());
        }
        graph.push(start.elapsed().as_secs_f64() * 1000.0);
        let start = Instant::now();
        if let Err(e) = source
            .query(Query::Text {
                dialect: "search".into(),
                text: "main".into(),
            })
            .await
        {
            search_error = Some(e.to_string());
        }
        search.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    Ok(
        serde_json::json!({"lookup":summary(&mut lookup), "text_source":summary(&mut texts), "graph_all_1000":summary(&mut graph), "graph_error":graph_error, "search_main":summary(&mut search), "search_error":search_error}),
    )
}

pub struct Outcome {
    pub commits: usize,
    pub findings: usize,
    pub errors: usize,
}
/// A range is Git rev-list syntax. Adjacent topologically ordered trees are
/// diffed, so merges/side branches are replayed without silently dropping paths.
pub async fn replay(
    repo: &Path,
    range: &str,
    scratch: &Path,
    interval: usize,
    output: &mut impl Write,
) -> Result<Outcome> {
    // Canonicalize before creating anything. Scratch must already exist and
    // cannot be in the input repository, including through a symlink.
    let canonical_repo = repo.canonicalize()?;
    let canonical_scratch = scratch.canonicalize()?;
    if canonical_scratch.starts_with(&canonical_repo) {
        return Err("scratch directory must be outside the replayed repository".into());
    }
    let repo = canonical_repo.as_path();
    let scratch = canonical_scratch.as_path();
    let commits = String::from_utf8(git(
        repo,
        &["rev-list", "--reverse", "--topo-order", range, "--"],
    )?)?;
    let commits: Vec<_> = commits.lines().collect();
    if commits.is_empty() || interval == 0 {
        return Err("empty range or zero checkpoint interval".into());
    }
    let dir = tempfile::Builder::new()
        .prefix("history-")
        .tempdir_in(scratch)?;
    let root = dir.path();
    let writer = FolderStore::open(root)?;
    let mut blobs = Blobs::open(repo)?;
    let mut tree = BTreeMap::new();
    // Ranges starting midway need the first commit's parent as a baseline.
    if let Ok(parent) = git(
        repo,
        &["rev-parse", "--verify", &format!("{}^", commits[0])],
    ) {
        checkout(
            repo,
            std::str::from_utf8(&parent)?.trim(),
            root,
            &mut tree,
            &mut blobs,
        )?;
        writer.sync(&inputs(root).await?)?;
    }
    writeln!(
        output,
        "{}",
        serde_json::json!({"type":"run", "checker":"full-digest-v1", "repo":repo, "range":range, "commits":commits.len(), "first":commits[0], "last":commits.last(), "checkpoint_interval":interval, "normaliser":moonkale_index::folder_store::normaliser(), "sophia":sophia_pin(), "build":"debug", "order":"reverse topo; adjacent tree transitions", "sample_seed":"4d6f6f6e6b616c65"})
    )?;
    let mut last_fresh = None;
    let mut outcome = Outcome {
        commits: 0,
        findings: 0,
        errors: 0,
    };
    for (i, commit) in commits.iter().enumerate() {
        let changed = checkout(repo, commit, root, &mut tree, &mut blobs)?;
        let files = inputs(root).await?;
        let start = Instant::now();
        let report = writer.sync_selected(&files, Some(&changed));
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        let (batch_files, retries, error) = match report {
            Ok(r) => (Some(r.written.files), Some(r.retries), None),
            Err(e) => {
                outcome.errors += 1;
                (None, None, Some(e.to_string()))
            }
        };
        let s = store(root)?;
        let (records, facts) = counts(&s)?;
        writeln!(
            output,
            "{}",
            serde_json::json!({"type":"batch","index":i+1,"commit":commit,"changed_paths":changed.len(),"files":batch_files,"retries":retries,"batch_ms":elapsed,"store_bytes":std::fs::metadata(writer.path())?.len(),"records":records,"facts":facts,"error":error})
        )?;
        output.flush()?;
        if (i + 1) % interval == 0 || i + 1 == commits.len() {
            eprintln!("checkpoint {} {commit}: fresh build and equivalence", i + 1);
            let fresh = tempfile::Builder::new()
                .prefix("fresh-")
                .tempdir_in(scratch)?;
            let mut empty = BTreeMap::new();
            checkout(repo, commit, fresh.path(), &mut empty, &mut blobs)?;
            let clean = FolderStore::open(fresh.path())?.sync(&inputs(fresh.path()).await?);
            match clean {
                Ok(_) => {
                    let expected = state(&store(fresh.path())?)?;
                    let actual = state(&s)?;
                    let mut diff = differences(&actual, &expected);
                    let owners: Vec<_> = s
                        .files()?
                        .into_iter()
                        .chain(store(fresh.path())?.files()?)
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect();
                    for finding in &mut diff {
                        let key = finding.split_once(' ').unwrap().1;
                        let token = key.split_once(':').unwrap().1;
                        let paths: Vec<_> = owners
                            .iter()
                            .filter(|p| {
                                [
                                    actual.get(&format!("manifest:{p}")),
                                    expected.get(&format!("manifest:{p}")),
                                ]
                                .into_iter()
                                .flatten()
                                .any(|m| {
                                    m.contains(token)
                                        || (key.starts_with("membership:")
                                            && token
                                                .split(|c: char| {
                                                    !c.is_alphanumeric() && c != '.' && c != '_'
                                                })
                                                .filter(|t| t.len() >= 8)
                                                .any(|t| m.contains(t)))
                                }) || key == format!("manifest:{p}")
                            })
                            .collect();
                        finding.push_str(&format!(" paths={paths:?}"));
                    }
                    outcome.findings += diff.len();
                    eprintln!(
                        "checkpoint {} {commit}: {} differences; query timings",
                        i + 1,
                        diff.len()
                    );
                    writeln!(
                        output,
                        "{}",
                        serde_json::json!({"type":"checkpoint","commit":commit,"index":i+1,"differences":diff,"queries":queries(root,&s).await?})
                    )?;
                }
                Err(e) => {
                    outcome.errors += 1;
                    writeln!(
                        output,
                        "{}",
                        serde_json::json!({"type":"checkpoint","commit":commit,"index":i+1,"fresh_error":e.to_string()})
                    )?;
                }
            }
            output.flush()?;
            if i + 1 == commits.len() {
                last_fresh = Some(fresh);
            }
        }
        outcome.commits += 1;
    }
    writeln!(
        output,
        "{}",
        serde_json::json!({"type":"summary","commits":outcome.commits,"findings":outcome.findings,"errors":outcome.errors})
    )?;
    // MOONKALE_REPLAY_KEEP=<dir> keeps the final replayed and fresh stores for
    // inspection (replayed.sqlite, fresh.sqlite); otherwise both are removed.
    if let Some(keep) = std::env::var_os("MOONKALE_REPLAY_KEEP") {
        drop(writer);
        let keep = Path::new(&keep);
        std::fs::create_dir_all(keep)?;
        let stores = [Some(root), last_fresh.as_ref().map(|d| d.path())];
        for (name, dir) in ["replayed", "fresh"].into_iter().zip(stores) {
            let Some(dir) = dir else { continue };
            for suffix in ["", "-wal"] {
                let from = dir.join(format!(
                    "{}{suffix}",
                    moonkale_index::folder_store::STORE_PATH
                ));
                if from.exists() {
                    std::fs::copy(&from, keep.join(format!("{name}.sqlite{suffix}")))?;
                }
            }
        }
    }
    Ok(outcome)
}
