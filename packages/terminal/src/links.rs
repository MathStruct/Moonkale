//! File references in terminal output: `path:line[:col]` (rustc, Julia, Go,
//! Python tracebacks). Detected on the plain text xterm reports for a line.

use regex::Regex;
use std::sync::OnceLock;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileLink {
    pub path: String,
    pub line: Option<u32>,
    pub col: Option<u32>,
}

fn re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // `src/main.rs:12:5`, `./a/b.jl:3`, `/abs/path.go:10`, `File "x.py", line 7`
    RE.get_or_init(|| Regex::new(r#"(?:File "([^"]+)", line (\d+))|((?:~|\.{0,2})?/?[\w./-]+\.[A-Za-z0-9]{1,8}):(\d+)(?::(\d+))?"#).unwrap())
}

/// All file references in `text`, in order.
pub fn find(text: &str) -> Vec<FileLink> {
    re().captures_iter(text)
        .map(|c| {
            if let (Some(p), Some(l)) = (c.get(1), c.get(2)) {
                FileLink {
                    path: p.as_str().to_string(),
                    line: l.as_str().parse().ok(),
                    col: None,
                }
            } else {
                FileLink {
                    path: c.get(3).map(|m| m.as_str().to_string()).unwrap_or_default(),
                    line: c.get(4).and_then(|m| m.as_str().parse().ok()),
                    col: c.get(5).and_then(|m| m.as_str().parse().ok()),
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_rustc_julia_and_python_shapes() {
        let out = find("error[E0425]: --> src/main.rs:12:5\n @ Main ~/proj/a.jl:3\n  File \"t/x.py\", line 7, in f");
        assert_eq!(
            out[0],
            FileLink {
                path: "src/main.rs".into(),
                line: Some(12),
                col: Some(5)
            }
        );
        assert_eq!(
            out[1],
            FileLink {
                path: "~/proj/a.jl".into(),
                line: Some(3),
                col: None
            }
        );
        assert_eq!(
            out[2],
            FileLink {
                path: "t/x.py".into(),
                line: Some(7),
                col: None
            }
        );
    }
}
