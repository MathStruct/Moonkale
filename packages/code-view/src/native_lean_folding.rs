//! Lean layout folds. The bundled grammar can recover tactic blocks across a
//! sibling `end`, so proof boundaries use indentation and explicit scopes.
use std::collections::BTreeSet;

struct Line {
    code: String,
    string_continuation: bool,
}

pub(super) fn extend(source: &str, folds: &mut BTreeSet<(usize, usize)>) {
    let lines = code_lines(source, folds);
    let mut scopes: Vec<(usize, String)> = Vec::new();
    let mut suites: Vec<(usize, usize)> = Vec::new();
    let mut last_content = 0;
    for (number, (raw, line)) in source.split('\n').zip(lines).enumerate() {
        let code = line.code.trim();
        if code.is_empty() {
            continue;
        }
        if line.string_continuation {
            last_content = number;
            continue;
        }
        let indent = raw
            .chars()
            .take_while(|ch| matches!(ch, ' ' | '\t'))
            .fold(0, |width, ch| {
                if ch == '\t' {
                    width + 8 - width % 8
                } else {
                    width + 1
                }
            });
        while suites.last().is_some_and(|(_, width)| *width >= indent) {
            let (header, _) = suites.pop().unwrap();
            if last_content > header {
                folds.insert((header, last_content));
            }
        }
        let mut words = code.split_whitespace();
        let first = words.next().unwrap();
        match first {
            "namespace" | "section" | "mutual" => {
                scopes.push((number, words.collect::<Vec<_>>().join(" ")))
            }
            "end" => {
                let name = words.collect::<Vec<_>>().join(" ");
                if let Some((header, open)) = scopes.pop() {
                    if name.is_empty() || name == open {
                        folds.insert((header, number));
                    } else {
                        scopes.clear();
                    } // A mismatched name must not hide siblings.
                }
            }
            _ => {
                let declaration = declaration(code);
                let block = matches!(
                    code.split_whitespace().last(),
                    Some("by" | "do" | "where" | "=>")
                );
                if declaration || block {
                    suites.push((number, indent));
                }
            }
        }
        last_content = number;
    }
    for (header, _) in suites {
        if last_content > header {
            folds.insert((header, last_content));
        }
    }
    // Unclosed namespace/section scopes deliberately do not fold to EOF.
}

fn declaration(mut code: &str) -> bool {
    // Inline attributes and modifiers precede the declaration keyword.
    while let Some(rest) = code.strip_prefix("@[") {
        let Some((_, rest)) = rest.split_once(']') else {
            return false;
        };
        code = rest.trim_start();
    }
    let keyword = code.split_whitespace().find(|word| {
        !matches!(
            *word,
            "private"
                | "protected"
                | "noncomputable"
                | "unsafe"
                | "partial"
                | "nonrec"
                | "local"
                | "scoped"
        )
    });
    matches!(
        keyword,
        Some(
            "def"
                | "abbrev"
                | "theorem"
                | "lemma"
                | "example"
                | "instance"
                | "axiom"
                | "constant"
                | "constants"
                | "opaque"
                | "inductive"
                | "coinductive"
                | "structure"
                | "class"
        )
    )
}

/// Mask strings and comments before recognizing keywords. Nested block comments
/// and incomplete literals must remain opaque even when parser recovery omits
/// their highlighting spans. Newlines are retained for source line identity.
fn code_lines(source: &str, folds: &mut BTreeSet<(usize, usize)>) -> Vec<Line> {
    let mut lines = vec![Line {
        code: String::new(),
        string_continuation: false,
    }];
    let mut chars = source.chars().peekable();
    let mut comments: Vec<usize> = Vec::new();
    let mut string = false;
    let mut escaped = false;
    let mut line_comment = false;
    while let Some(ch) = chars.next() {
        let number = lines.len() - 1;
        if ch == '\n' {
            lines.push(Line {
                code: String::new(),
                string_continuation: string,
            });
            line_comment = false;
            escaped = false;
            continue;
        }
        let out = &mut lines.last_mut().unwrap().code;
        if line_comment {
            continue;
        }
        if string {
            out.push('x');
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                string = false;
            }
            continue;
        }
        if comments.is_empty() && ch == '\'' {
            let mut look = chars.clone();
            if let Some(value) = look.next() {
                if value == '\\' {
                    look.next();
                }
                if look.next() == Some('\'') {
                    chars = look;
                    out.push('x');
                    continue;
                }
            }
        }
        if ch == '/' && chars.peek() == Some(&'-') {
            chars.next();
            comments.push(number);
            out.push(' ');
            continue;
        }
        if !comments.is_empty() {
            if ch == '-' && chars.peek() == Some(&'/') {
                chars.next();
                let start = comments.pop().unwrap();
                if number > start {
                    folds.insert((start, number));
                }
                out.push(' ');
            }
            continue;
        }
        if ch == '-' && chars.peek() == Some(&'-') {
            chars.next();
            line_comment = true;
        } else if ch == '"' {
            string = true;
            out.push('x');
        } else {
            out.push(ch);
        }
    }
    lines
}
