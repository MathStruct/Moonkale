//! Bounded pure parsers used by the Git runner and its fuzz target.
use crate::everyday_types::{BlameLine, RecoveryEntry};
const LIMIT: usize = 2 * 1024 * 1024;

pub fn recovery(text: &str) -> Vec<RecoveryEntry> {
    if text.len() > LIMIT {
        return Vec::new();
    }
    text.lines()
        .filter_map(|l| {
            let mut p = l.splitn(3, '\t');
            Some(RecoveryEntry {
                oid: p.next()?.into(),
                selector: p.next()?.into(),
                subject: p.next()?.into(),
            })
        })
        .take(200)
        .collect()
}
pub fn parse_blame(text: &str) -> Result<Vec<BlameLine>, String> {
    if text.len() > LIMIT {
        return Err("Blame text exceeds the limit".into());
    }
    let mut result = Vec::new();
    let mut row = BlameLine {
        line: 0,
        commit: String::new(),
        author: String::new(),
        summary: String::new(),
        previous: None,
        previous_path: None,
    };
    for line in text.lines() {
        if line.starts_with('\t') {
            if result.len() >= 100_000 {
                return Err("Blame has too many lines".into());
            }
            if row.commit.is_empty() {
                return Err("Blame content without a header".into());
            }
            result.push(std::mem::take(&mut row));
            continue;
        }
        if let Some((oid, rest)) = line.split_once(' ') {
            if [40, 64].contains(&oid.len()) && oid.bytes().all(|b| b.is_ascii_hexdigit()) {
                let n = rest
                    .split_whitespace()
                    .nth(1)
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or(1);
                row = BlameLine {
                    line: n.saturating_sub(1),
                    commit: oid.into(),
                    author: String::new(),
                    summary: String::new(),
                    previous: None,
                    previous_path: None,
                };
            } else {
                match oid {
                    "author" => row.author = rest.into(),
                    "summary" => row.summary = rest.into(),
                    "previous" => {
                        if let Some((c, p)) = rest.split_once(' ') {
                            row.previous = Some(c.into());
                            row.previous_path = Some(p.into());
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(result)
}

/// Construct an index patch from server-generated rows. Reversal is done here
/// so omitted additions/removals become the correct context in either direction.
pub fn selection(diff: &str, selected: &[usize], reverse: bool) -> Result<String, String> {
    if diff.len() > LIMIT || selected.len() > 100_000 {
        return Err("Selection too large".into());
    }
    let selected: std::collections::HashSet<_> = selected.iter().copied().collect();
    let lines: Vec<_> = diff.split_inclusive('\n').collect();
    let first = lines
        .iter()
        .position(|l| l.starts_with("@@ "))
        .ok_or("No text hunks")?;
    let header = &lines[..first];
    if header.iter().any(|l| {
        l.starts_with("rename from")
            || ((l.starts_with("index ")
                || l.starts_with("new file mode ")
                || l.starts_with("deleted file mode "))
                && (l.trim_end().ends_with("120000") || l.trim_end().ends_with("160000")))
    }) {
        return Err("Use whole-file staging for symlinks and submodules".into());
    }
    let old = header
        .iter()
        .find_map(|l| l.strip_prefix("--- "))
        .ok_or("Missing old path")?
        .trim_end_matches('\n');
    let new = header
        .iter()
        .find_map(|l| l.strip_prefix("+++ "))
        .ok_or("Missing new path")?
        .trim_end_matches('\n');
    let creation = old == "/dev/null";
    let deletion = new == "/dev/null";
    let changes: Vec<_> = lines
        .iter()
        .enumerate()
        .skip(first)
        .filter(|(_, l)| l.starts_with('+') || l.starts_with('-'))
        .map(|(i, _)| i)
        .collect();
    let all = changes.iter().all(|i| selected.contains(i));
    let mode = header
        .iter()
        .find_map(|l| {
            l.strip_prefix("new file mode ")
                .or_else(|| l.strip_prefix("deleted file mode "))
        })
        .map(str::trim)
        .unwrap_or("100644");
    let creates = (!reverse && creation) || (reverse && deletion);
    let deletes = all && ((!reverse && deletion) || (reverse && creation));
    let canonical = if creation { new } else { old };
    let path_a = if creation {
        canonical.replacen("b/", "a/", 1)
    } else {
        canonical.to_owned()
    };
    let path_b = path_a.replacen("a/", "b/", 1);
    let mut out = header
        .iter()
        .find(|l| l.starts_with("diff --git "))
        .ok_or("Missing diff header")?
        .to_string();
    if creates {
        out.push_str(&format!("new file mode {mode}\n"));
    }
    if deletes {
        out.push_str(&format!("deleted file mode {mode}\n"));
    }
    out.push_str(&format!(
        "--- {}\n+++ {}\n",
        if creates { "/dev/null" } else { &path_a },
        if deletes { "/dev/null" } else { &path_b }
    ));
    let mut i = first;
    let mut changed = false;
    while i < lines.len() {
        let ranges = lines[i].split("@@").nth(1).ok_or("Invalid hunk")?;
        let mut parts = ranges.split_whitespace();
        let old = parts
            .next()
            .and_then(|s| s.strip_prefix('-'))
            .and_then(|s| s.split(',').next())
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or("Invalid old range")?;
        let new = parts
            .next()
            .and_then(|s| s.strip_prefix('+'))
            .and_then(|s| s.split(',').next())
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or("Invalid new range")?;
        i += 1;
        let (mut a, mut b) = (0usize, 0usize);
        let mut body = String::new();
        let mut any = false;
        let mut keep = false;
        while i < lines.len() && !lines[i].starts_with("@@ ") {
            let l = lines[i];
            let chosen = selected.contains(&i);
            let kind = match (l.as_bytes().first(), reverse) {
                (Some(b'+'), true) => b'-',
                (Some(b'-'), true) => b'+',
                (Some(c), _) => *c,
                _ => return Err("Invalid diff line".into()),
            };
            match kind {
                b'-' => {
                    a += 1;
                    keep = true;
                    if chosen {
                        any = true;
                        body.push('-');
                        body.push_str(&l[1..]);
                    } else {
                        b += 1;
                        body.push(' ');
                        body.push_str(&l[1..]);
                    }
                }
                b'+' => {
                    keep = chosen;
                    if chosen {
                        any = true;
                        b += 1;
                        body.push('+');
                        body.push_str(&l[1..]);
                    }
                }
                b' ' => {
                    keep = true;
                    a += 1;
                    b += 1;
                    body.push_str(l);
                }
                b'\\' => {
                    if keep {
                        body.push_str(l);
                    }
                }
                _ => return Err("Invalid diff line".into()),
            }
            i += 1;
        }
        if any {
            changed = true;
            let (old, new) = if reverse { (new, old) } else { (old, new) };
            let start = |s: u64, count: usize| if count == 0 { s } else { s.max(1) };
            out.push_str(&format!(
                "@@ -{},{} +{},{} @@\n{body}",
                start(old, a),
                a,
                start(new, b),
                b
            ));
        }
    }
    if !changed {
        return Err("Select changed lines".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zero_and_oversized_inputs_are_bounded() {
        let text = format!("{} 1 0\nauthor A\n\ttext\n", "a".repeat(40));
        assert_eq!(parse_blame(&text).unwrap()[0].line, 0);
        let huge = "x".repeat(LIMIT + 1);
        assert!(parse_blame(&huge).is_err());
        assert!(selection(&huge, &[], false).is_err());
        assert!(recovery(&huge).is_empty());
    }
    #[test]
    fn malformed_blame_cannot_duplicate_retained_metadata() {
        let text = format!(
            "{} 1 1\nauthor {}\n\tfirst\n\twithout header\n",
            "a".repeat(40),
            "large".repeat(10_000)
        );
        assert!(parse_blame(&text).is_err());
    }
}
