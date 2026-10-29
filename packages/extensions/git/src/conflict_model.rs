//! Bounded, platform-independent diff3 hunk model shared with the fuzz target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    pub ours: String,
    pub base: String,
    pub theirs: String,
    pub marker: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConflictDocument {
    pub hunks: Vec<Hunk>,
    pub result: String,
}
impl ConflictDocument {
    /// Consume Git's diff3 output with a deliberately long marker (32 chars).
    /// Malformed/nested markers are refused, never silently dropped.
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() > 2 * 1024 * 1024 {
            return Err("Conflict exceeds the 2 MiB text limit".into());
        }
        let lines: Vec<_> = text.split_inclusive('\n').collect();
        let mut hunks = Vec::new();
        let mut result = String::new();
        let mut i = 0;
        while i < lines.len() {
            if !lines[i].starts_with(&"<".repeat(32)) {
                result.push_str(lines[i]);
                i += 1;
                continue;
            }
            i += 1;
            let mut sections = [String::new(), String::new(), String::new()];
            for (section, delimiter) in ["|".repeat(32), "=".repeat(32), ">".repeat(32)]
                .iter()
                .enumerate()
            {
                while i < lines.len() && !lines[i].starts_with(delimiter) {
                    if lines[i].starts_with(&"<".repeat(32)) {
                        return Err("Nested conflict marker".into());
                    }
                    sections[section].push_str(lines[i]);
                    i += 1;
                }
                if i == lines.len() {
                    return Err("Incomplete conflict hunk".into());
                }
                i += 1;
            }
            if hunks.len() == 4096 {
                return Err("Too many conflict hunks".into());
            }
            let marker = format!(
                "<<<<<<< hunk {}\n{}||||||| base\n{}=======\n{}>>>>>>> hunk {}\n",
                hunks.len() + 1,
                sections[0],
                sections[1],
                sections[2],
                hunks.len() + 1
            );
            result.push_str(&marker);
            hunks.push(Hunk {
                ours: sections[0].clone(),
                base: sections[1].clone(),
                theirs: sections[2].clone(),
                marker,
            });
        }
        Ok(Self { hunks, result })
    }
    /// Preserve edits already made in the working file. Only recognized unresolved
    /// blocks are exchanged for the model's unique markers. Resolved or changed
    /// blocks stay exactly as they were; their pick buttons are unavailable.
    pub fn initialize(&self, worktree: &str) -> String {
        let lines: Vec<_> = worktree.split_inclusive('\n').collect();
        let mut result = String::new();
        let mut used = vec![false; self.hunks.len()];
        let mut i = 0;
        while i < lines.len() {
            let count = lines[i].chars().take_while(|c| *c == '<').count();
            if count < 7 {
                result.push_str(lines[i]);
                i += 1;
                continue;
            }
            let start = i;
            let base_mark = "|".repeat(count);
            let mid_mark = "=".repeat(count);
            let end_mark = ">".repeat(count);
            let mut base = None;
            let mut mid = None;
            let mut end = None;
            for (j, line) in lines.iter().enumerate().skip(i + 1) {
                if line.starts_with(&base_mark) && mid.is_none() {
                    base = Some(j);
                }
                if line.trim_end_matches(['\r', '\n']) == mid_mark {
                    mid = Some(j);
                }
                if line.starts_with(&end_mark) {
                    end = Some(j);
                    break;
                }
            }
            if let (Some(mid), Some(end)) = (mid, end) {
                let ours = lines[start + 1..base.unwrap_or(mid)].concat();
                let theirs = lines[mid + 1..end].concat();
                if let Some((n, h)) = self
                    .hunks
                    .iter()
                    .enumerate()
                    .find(|(n, h)| !used[*n] && h.ours == ours && h.theirs == theirs)
                {
                    result.push_str(&h.marker);
                    used[n] = true;
                    i = end + 1;
                    continue;
                }
                result.push_str(&lines[start..=end].concat());
                i = end + 1;
                continue;
            }
            result.push_str(&lines[i..].concat());
            break;
        }
        result
    }

    /// Replaces only this unresolved hunk, preserving all other manual edits.
    pub fn pick(&self, result: &str, index: usize, side: usize) -> String {
        let Some(h) = self.hunks.get(index) else {
            return result.into();
        };
        let replacement = match side {
            0 => &h.ours,
            1 => &h.base,
            2 => &h.theirs,
            _ => return result.into(),
        };
        result.replacen(&h.marker, replacement, 1)
    }
    pub fn unresolved(&self, result: &str) -> bool {
        result
            .lines()
            .any(|line| line.starts_with("<<<<<<< ") || line == "<<<<<<<")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn hunk(ours: &str, base: &str, theirs: &str) -> String {
        format!(
            "{} ours\n{ours}{} base\n{base}{}\n{theirs}{} theirs\n",
            "<".repeat(32),
            "|".repeat(32),
            "=".repeat(32),
            ">".repeat(32)
        )
    }
    #[test]
    fn independent_hunks_and_manual_edits() {
        let text = format!(
            "α\n{}middle\n{}tail",
            hunk("local\n", "old\n", "remote\n"),
            hunk("local\n", "old\n", "remote\n")
        );
        let d = ConflictDocument::parse(&text).unwrap();
        assert_eq!(d.hunks.len(), 2);
        let result = d.pick(&d.result.replace("α", "edited α"), 1, 2);
        assert!(d.unresolved(&result));
        let result = d.pick(&result, 0, 0);
        assert_eq!(result, "edited α\nlocal\nmiddle\nremote\ntail");
        assert!(!d.unresolved(&result));
    }
    #[test]
    fn initializes_from_worktree_without_losing_existing_edits() {
        let d = ConflictDocument::parse(&format!(
            "before\n{}after\n{}",
            hunk("local\n", "old\n", "remote\n"),
            hunk("second\n", "base\n", "other\n")
        ))
        .unwrap();
        let original = "manual before\n<<<<<<< HEAD\nlocal\n=======\nremote\n>>>>>>> topic\nmanual after\nalready resolved second\n";
        let result = d.initialize(original);
        assert!(d.unresolved(&result));
        assert!(!result.contains(&d.hunks[1].marker));
        assert_eq!(
            d.pick(&result, 0, 2),
            "manual before\nremote\nmanual after\nalready resolved second\n"
        );
        assert_eq!(d.initialize("fully resolved\n"), "fully resolved\n");
    }
    #[test]
    fn empty_sides_crlf_and_no_final_newline() {
        let d = ConflictDocument::parse(&format!("{}tail", hunk("", "x\r\n", "z\r\n"))).unwrap();
        assert_eq!(d.pick(&d.result, 0, 0), "tail");
        assert_eq!(d.pick(&d.result, 0, 1), "x\r\ntail");
        assert!(ConflictDocument::parse(&format!("{} ours\nx", "<".repeat(32))).is_err());
    }
}
