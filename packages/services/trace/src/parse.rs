//! Parsers. Each format yields frames in *listing order*; a "call chain"
//! edge points from a frame to the one listed after it (for Rust and JS
//! that is caller → callee reversed: frame 0 is innermost; for Python the
//! listing is outermost first). We keep listing order and let the graph
//! show the chain either way.

use regex::Regex;
use std::sync::OnceLock;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    /// Function or symbol name when the format gives one.
    pub function: Option<String>,
    pub file: String,
    /// 1-based.
    pub line: u32,
    pub col: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trace {
    /// First line of the block ("thread 'main' panicked at …", "error[E0308]: …").
    pub title: String,
    pub kind: &'static str,
    pub frames: Vec<Frame>,
}

fn re(s: &'static str, cell: &'static OnceLock<Regex>) -> &'static Regex {
    cell.get_or_init(|| Regex::new(s).unwrap())
}

static RUST_FRAME: OnceLock<Regex> = OnceLock::new();
static RUST_AT: OnceLock<Regex> = OnceLock::new();
static RUST_PANIC: OnceLock<Regex> = OnceLock::new();
static CARGO_HEAD: OnceLock<Regex> = OnceLock::new();
static CARGO_ARROW: OnceLock<Regex> = OnceLock::new();
static PY_FILE: OnceLock<Regex> = OnceLock::new();
static JS_AT: OnceLock<Regex> = OnceLock::new();
static BARE: OnceLock<Regex> = OnceLock::new();

/// Everything recognisable in `text`, in order of appearance.
pub fn parse(text: &str) -> Vec<Trace> {
    let rust_frame = re(r"^\s*(\d+):\s+(\S.*)$", &RUST_FRAME);
    let rust_at = re(r"^\s*at\s+(\S+?):(\d+)(?::(\d+))?\s*$", &RUST_AT);
    let rust_panic = re(r"panicked at ([^:\s]+):(\d+):(\d+)", &RUST_PANIC);
    let cargo_head = re(r"^(error|warning)(\[[A-Z0-9]+\])?: (.*)$", &CARGO_HEAD);
    let cargo_arrow = re(r"^\s*-->\s+(\S+?):(\d+)(?::(\d+))?\s*$", &CARGO_ARROW);
    let py_file = re(r#"^\s*File "([^"]+)", line (\d+)(?:, in (\S+))?"#, &PY_FILE);
    let js_at = re(
        r"^\s*at\s+(?:(\S+)\s+\()?([^\s()]+?):(\d+):(\d+)\)?\s*$",
        &JS_AT,
    );
    let bare = re(
        r"(?:^|[\s(])([A-Za-z0-9_./\\-]+\.[A-Za-z0-9]+):(\d+)(?::(\d+))?",
        &BARE,
    );

    let lines: Vec<&str> = text.lines().map(|l| l.trim_end()).collect();
    let mut out: Vec<Trace> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];

        // Rust panic message → its own frame; a backtrace may follow.
        if let Some(c) = rust_panic.captures(line) {
            let mut t = Trace {
                title: line.trim().to_string(),
                kind: "rust-panic",
                frames: vec![Frame {
                    function: Some("panic".into()),
                    file: c[1].to_string(),
                    line: c[2].parse().unwrap_or(1),
                    col: c[3].parse().ok(),
                }],
            };
            i += 1;
            // Skip the message lines until a backtrace frame or blank.
            while i < lines.len()
                && !rust_frame.is_match(lines[i])
                && !lines[i].contains("stack backtrace")
                && !lines[i].trim().is_empty()
            {
                i += 1;
            }
            i = take_rust_frames(&lines, i, &mut t.frames, rust_frame, rust_at);
            out.push(t);
            continue;
        }
        if rust_frame.is_match(line) && i + 1 < lines.len() && rust_at.is_match(lines[i + 1]) {
            let mut t = Trace {
                title: "stack backtrace".into(),
                kind: "rust-backtrace",
                frames: Vec::new(),
            };
            i = take_rust_frames(&lines, i, &mut t.frames, rust_frame, rust_at);
            out.push(t);
            continue;
        }
        // cargo / rustc diagnostic block: header then `--> file:line:col`.
        if let Some(c) = cargo_head.captures(line) {
            let mut t = Trace {
                title: format!("{}: {}", &c[1], &c[3]),
                kind: "cargo",
                frames: Vec::new(),
            };
            let mut j = i + 1;
            while j < lines.len() && j < i + 12 {
                if let Some(a) = cargo_arrow.captures(lines[j]) {
                    t.frames.push(Frame {
                        function: None,
                        file: a[1].to_string(),
                        line: a[2].parse().unwrap_or(1),
                        col: a.get(3).and_then(|m| m.as_str().parse().ok()),
                    });
                    break;
                }
                if cargo_head.is_match(lines[j]) {
                    break;
                }
                j += 1;
            }
            if !t.frames.is_empty() {
                out.push(t);
                i = j + 1;
                continue;
            }
        }
        // Python traceback.
        if line
            .trim_start()
            .starts_with("Traceback (most recent call last)")
        {
            let mut t = Trace {
                title: line.trim().to_string(),
                kind: "python",
                frames: Vec::new(),
            };
            let mut j = i + 1;
            while j < lines.len() {
                if let Some(c) = py_file.captures(lines[j]) {
                    t.frames.push(Frame {
                        function: c.get(3).map(|m| m.as_str().to_string()),
                        file: c[1].to_string(),
                        line: c[2].parse().unwrap_or(1),
                        col: None,
                    });
                } else if !lines[j].starts_with(' ') && !lines[j].trim().is_empty() {
                    // "ValueError: …" ends the block.
                    t.title = format!("{} — {}", t.title, lines[j].trim());
                    j += 1;
                    break;
                }
                j += 1;
            }
            if !t.frames.is_empty() {
                out.push(t);
            }
            i = j;
            continue;
        }
        // JS / Node stack: "Error: msg" then "    at fn (file:line:col)".
        if i + 1 < lines.len()
            && js_at.is_match(lines[i + 1])
            && !line.trim().is_empty()
            && !js_at.is_match(line)
        {
            let mut t = Trace {
                title: line.trim().to_string(),
                kind: "js",
                frames: Vec::new(),
            };
            let mut j = i + 1;
            while j < lines.len() {
                let Some(c) = js_at.captures(lines[j]) else {
                    break;
                };
                t.frames.push(Frame {
                    function: c.get(1).map(|m| m.as_str().to_string()),
                    file: c[2].to_string(),
                    line: c[3].parse().unwrap_or(1),
                    col: c[4].parse().ok(),
                });
                j += 1;
            }
            out.push(t);
            i = j;
            continue;
        }
        i += 1;
    }
    if out.is_empty() {
        // Fallback: every `path:line` mention as one trace.
        let mut frames = Vec::new();
        for c in bare.captures_iter(text) {
            let file = c[1].to_string();
            if file.starts_with("http") || file.contains("://") {
                continue;
            }
            frames.push(Frame {
                function: None,
                file,
                line: c[2].parse().unwrap_or(1),
                col: c.get(3).and_then(|m| m.as_str().parse().ok()),
            });
        }
        if !frames.is_empty() {
            out.push(Trace {
                title: "locations".into(),
                kind: "locations",
                frames,
            });
        }
    }
    out
}

fn take_rust_frames(
    lines: &[&str],
    mut i: usize,
    frames: &mut Vec<Frame>,
    rust_frame: &Regex,
    rust_at: &Regex,
) -> usize {
    while i < lines.len() {
        let l = lines[i];
        if l.contains("stack backtrace:") || l.trim().is_empty() {
            i += 1;
            if l.trim().is_empty()
                && !frames.is_empty()
                && !lines.get(i).is_some_and(|n| rust_frame.is_match(n))
            {
                break;
            }
            continue;
        }
        let Some(c) = rust_frame.captures(l) else {
            break;
        };
        let function = c[2].trim().to_string();
        i += 1;
        if i < lines.len() {
            if let Some(a) = rust_at.captures(lines[i]) {
                frames.push(Frame {
                    function: Some(function),
                    file: a[1].to_string(),
                    line: a[2].parse().unwrap_or(1),
                    col: a.get(3).and_then(|m| m.as_str().parse().ok()),
                });
                i += 1;
                continue;
            }
        }
        // Frames without a location (std internals) are kept as chain links
        // only when they have a file; skip otherwise.
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_panic_with_backtrace() {
        let text = "thread 'main' panicked at src/main.rs:7:5:\nboom\nstack backtrace:\n   0: sample::inner\n             at ./src/lib.rs:3:5\n   1: sample::main\n             at ./src/main.rs:7:5\n   2: core::ops::function::FnOnce::call_once\n             at /rustc/abc/library/core/src/ops/function.rs:250:5\nnote: Some details are omitted\n";
        let t = parse(text);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].kind, "rust-panic");
        let files: Vec<&str> = t[0].frames.iter().map(|f| f.file.as_str()).collect();
        assert_eq!(
            files,
            [
                "src/main.rs",
                "./src/lib.rs",
                "./src/main.rs",
                "/rustc/abc/library/core/src/ops/function.rs"
            ]
        );
        assert_eq!(t[0].frames[1].function.as_deref(), Some("sample::inner"));
        assert_eq!(t[0].frames[1].line, 3);
    }

    #[test]
    fn cargo_errors_and_python_and_js() {
        let text = "error[E0308]: mismatched types\n --> src/main.rs:7:25\n  |\n7 |     let wrong: String = total;\nwarning: unused variable: `x`\n  --> src/lib.rs:2:9\n\nTraceback (most recent call last):\n  File \"app.py\", line 10, in <module>\n    main()\n  File \"app.py\", line 6, in main\n    boom()\nValueError: nope\n\nTypeError: x is not a function\n    at run (src/index.js:12:3)\n    at file:///srv/app.js:4:1\n";
        let t = parse(text);
        let kinds: Vec<&str> = t.iter().map(|t| t.kind).collect();
        assert_eq!(kinds, ["cargo", "cargo", "python", "js"]);
        assert_eq!(t[0].frames[0].file, "src/main.rs");
        assert_eq!(t[0].frames[0].col, Some(25));
        assert_eq!(t[2].frames.len(), 2);
        assert_eq!(t[2].frames[1].function.as_deref(), Some("main"));
        assert!(t[2].title.contains("ValueError"));
        assert_eq!(t[3].frames[0].function.as_deref(), Some("run"));
        assert_eq!(t[3].frames[1].file, "file:///srv/app.js");
    }

    #[test]
    fn bare_locations_fallback() {
        let t = parse("see notes/Beta.md:3 and src/lib.rs:2:9 (https://x.y/z:1)");
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].kind, "locations");
        assert_eq!(t[0].frames.len(), 2);
        assert!(parse("nothing here").is_empty());
    }
}
