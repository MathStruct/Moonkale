//! Coverage on a real Julia tree: `MOONKALE_JULIA_CORPUS=<dir> cargo test
//! -p moonkale-content-id --test corpus -- --nocapture`. Skipped without it.
//! References are left unresolved, as the crate has no resolver.
use moonkale_content_id::{definition, is_definition, parse, Budget, ContentIdError};
use std::collections::{BTreeMap, HashMap};

fn files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            files(&path, out);
        } else if path.extension().is_some_and(|e| e == "jl") {
            out.push(path);
        }
    }
}

#[test]
fn julia_corpus_coverage() {
    let Some(dir) = std::env::var_os("MOONKALE_JULIA_CORPUS") else {
        return;
    };
    let mut paths = Vec::new();
    files(std::path::Path::new(&dir), &mut paths);
    let (mut ok, mut refused) = (0usize, BTreeMap::<String, usize>::new());
    for path in paths {
        let text = std::fs::read_to_string(&path).unwrap();
        let mut budget = Budget::new();
        let Ok(tree) = parse("julia", &text, &mut budget) else {
            *refused.entry("file: parse".into()).or_default() += 1;
            continue;
        };
        let mut stack = vec![tree.root_node()];
        while let Some(node) = stack.pop() {
            let mut cursor = node.walk();
            if is_definition("julia", node) && node.kind() != "module_definition" {
                let mut budget = Budget::new();
                match definition("julia", node, &text, &HashMap::<_, u8>::new(), &mut budget) {
                    Ok(_) => ok += 1,
                    Err(e) => *refused.entry(reason(e, node, &text)).or_default() += 1,
                }
                continue;
            }
            stack.extend(node.named_children(&mut cursor));
        }
    }
    let total = ok + refused.values().sum::<usize>();
    println!("{ok} of {total} definitions get ids");
    let mut by_count: Vec<_> = refused.into_iter().collect();
    by_count.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (why, n) in by_count.into_iter().take(25) {
        println!("{n:6}  {why}");
    }
}

/// The first refused construct, found by re-running on smaller pieces.
fn reason(e: ContentIdError, node: moonkale_content_id::Syntax, _: &str) -> String {
    let mut kinds = Vec::new();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        kinds.push(n.kind());
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
    let suspects = [
        "global_statement",
        "function_definition",
        "macro_definition",
        "quote_expression",
        "named_argument",
        "macrocall_expression",
        "if_clause",
        "juxtaposition_expression",
    ];
    let hit = kinds.iter().skip(1).find(|k| suspects.contains(k));
    format!("{e:?} ({} {})", node.kind(), hit.unwrap_or(&"?"))
}
