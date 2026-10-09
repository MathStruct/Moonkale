//! Derived structural pairs and conservative fold ranges over normalized engine text.
use dioxus_code::{advanced::Buffer, Language};
use editor_core::FoldRegion;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(crate) struct Structure {
    pub pairs: BTreeMap<usize, usize>,
    pub folds: Vec<FoldRegion>,
}
impl Structure {
    pub fn new(buffer: Option<&Buffer>, language: Option<Language>) -> Self {
        let Some(buffer) = buffer else {
            return Self::default();
        };
        let text = buffer.source();
        let spans = buffer.spans();
        let excluded_at = |byte| {
            let index = spans.partition_point(|value| value.end() as usize <= byte);
            spans.get(index).is_some_and(|value| {
                value.start() as usize <= byte && matches!(value.tag(), "s" | "c")
            })
        };
        let mut span = 0;
        let mut line = 0;
        let mut stack: Vec<(char, usize, usize)> = Vec::new();
        let mut pairs = BTreeMap::new();
        let mut folds = BTreeSet::new();
        for (offset, (byte, ch)) in text.char_indices().enumerate() {
            while span < spans.len() && spans[span].end() as usize <= byte {
                span += 1;
            }
            let excluded = spans.get(span).is_some_and(|value| {
                value.start() as usize <= byte && matches!(value.tag(), "s" | "c")
            });
            if !excluded {
                match ch {
                    '(' | '[' | '{' => stack.push((ch, offset, line)),
                    ')' | ']' | '}' => {
                        let expected = match ch {
                            ')' => '(',
                            ']' => '[',
                            _ => '{',
                        };
                        if stack.last().is_some_and(|(open, _, _)| *open == expected) {
                            let (_, start, start_line) = stack.pop().unwrap();
                            pairs.insert(start, offset);
                            pairs.insert(offset, start);
                            if line > start_line
                                && !matches!(
                                    language.map(Language::slug),
                                    Some("html" | "markdown")
                                )
                            {
                                folds.insert((start_line, line));
                            }
                        } else {
                            // Broken nesting must not produce a misleading pair/fold.
                            stack.clear();
                        }
                    }
                    _ => {}
                }
            }
            if ch == '\n' {
                line += 1;
            }
        }
        if language.map(Language::slug) == Some("python") {
            // Indentation folds supplement delimiter folds for Python suites. Only
            // headers ending in an actual code colon qualify, not comments/strings.
            let mut suites: Vec<(usize, usize)> = Vec::new();
            let mut byte_start = 0;
            let mut last_code_line = 0;
            for (line, text_line) in text.split('\n').enumerate() {
                // Comments may follow the suite colon. Strip only parser-classified
                // comment spans, leaving '#' inside strings untouched.
                let span_start = spans.partition_point(|value| value.end() as usize <= byte_start);
                let code_end = spans[span_start..]
                    .iter()
                    .take_while(|value| (value.start() as usize) < byte_start + text_line.len())
                    .find(|value| value.tag() == "c")
                    .map(|value| (value.start() as usize).saturating_sub(byte_start))
                    .unwrap_or(text_line.len());
                let trimmed = text_line[..code_end].trim_end();
                let indent = text_line
                    .chars()
                    .take_while(|ch| matches!(ch, ' ' | '\t'))
                    .map(|ch| if ch == '\t' { 8 } else { 1 })
                    .sum::<usize>();
                let first_byte = byte_start + text_line.len() - text_line.trim_start().len();
                let code = !trimmed.trim_start().is_empty() && !excluded_at(first_byte);
                if code {
                    while suites.last().is_some_and(|(_, width)| *width >= indent) {
                        let (header, _) = suites.pop().unwrap();
                        if last_code_line > header {
                            folds.insert((header, last_code_line));
                        }
                    }
                    let colon_byte = byte_start + trimmed.len().saturating_sub(1);
                    let code_colon = trimmed.ends_with(':') && !excluded_at(colon_byte);
                    if code_colon {
                        suites.push((line, indent));
                    }
                    last_code_line = line;
                }
                byte_start += text_line.len() + 1;
            }
            for (header, _) in suites {
                if last_code_line > header {
                    folds.insert((header, last_code_line));
                }
            }
        }
        crate::native_folding::extend(text, language, &mut folds);
        // One gutter control per line. Prefer the widest enclosing range.
        let mut regions: Vec<FoldRegion> = Vec::new();
        for (start, end) in folds {
            if let Some(previous) = regions
                .last_mut()
                .filter(|region| region.start_line == start)
            {
                previous.end_line = end;
            } else {
                regions.push(FoldRegion::new(start, end));
            }
        }
        Self {
            pairs,
            folds: regions,
        }
    }
    pub fn at_caret(&self, offset: usize) -> Option<(usize, usize)> {
        let offset = offset
            .checked_sub(1)
            .filter(|previous| self.pairs.contains_key(previous))
            .unwrap_or(offset);
        self.pairs.get(&offset).map(|other| (offset, *other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grammar_folds_merge_widest_header_and_do_not_fold_prose_braces() {
        for (slug, source) in [
            (
                "markdown",
                "# Title\nprose {\n}\n## Inner\n```\nx\n```\n# Next\ntext\n",
            ),
            ("html", "<main>\n<section>\n😀\n</section>\n</main>\n"),
            (
                "julia",
                "function f()\n    if ready\n        work()\n    end\nend\n",
            ),
        ] {
            let language = Language::from_slug(slug).unwrap();
            let buffer = Buffer::new(language, source).unwrap();
            let structure = Structure::new(Some(&buffer), Some(language));
            assert!(
                structure.folds.iter().any(|fold| fold.start_line == 0),
                "{slug}"
            );
            if slug == "markdown" {
                assert!(!structure.folds.iter().any(|fold| fold.start_line == 1));
                assert_eq!(
                    structure
                        .folds
                        .iter()
                        .find(|fold| fold.start_line == 3)
                        .unwrap()
                        .end_line,
                    6
                );
            }
        }
    }
    #[test]
    fn unicode_nested_pairs_ignore_comments_strings_and_bad_nesting() {
        let source = "fn f() {\n let x = \"} 😀\"; // {\n if true {\n }\n}\n";
        let buffer = Buffer::new(Language::Rust, source).unwrap();
        let structure = Structure::new(Some(&buffer), Some(Language::Rust));
        assert_eq!(
            structure
                .folds
                .iter()
                .map(|f| (f.start_line, f.end_line))
                .collect::<Vec<_>>(),
            vec![(0, 4), (2, 3)]
        );
        assert_eq!(structure.at_caret(8), Some((7, source.chars().count() - 2)));
        let buffer = Buffer::new(Language::Rust, "fn f() { ([)] }").unwrap();
        let structure = Structure::new(Some(&buffer), Some(Language::Rust));
        assert!(structure.folds.is_empty());
        assert!(structure.at_caret(10).is_none());
    }
    #[test]
    fn python_suites_and_plain_fallback() {
        let source = "if ready:\n    if other:\n        work()\n    more()\ndone()\n";
        let language = Language::from_slug("python").unwrap();
        let buffer = Buffer::new(language, source).unwrap();
        let structure = Structure::new(Some(&buffer), Some(language));
        assert_eq!(
            structure
                .folds
                .iter()
                .map(|f| (f.start_line, f.end_line))
                .collect::<Vec<_>>(),
            vec![(0, 3), (1, 2)]
        );
        assert!(Structure::new(None, None).pairs.is_empty());
    }
    #[test]
    fn outer_functions_in_real_rust_components_have_fold_ranges() {
        let source = include_str!("native_panel.rs");
        let buffer = Buffer::new(Language::Rust, source).unwrap();
        let structure = Structure::new(Some(&buffer), Some(Language::Rust));
        for name in [
            "pub fn RustCodeEditorPanel",
            "fn publish_selection",
            "mod tests",
        ] {
            let line = source.lines().position(|line| line.contains(name)).unwrap();
            assert!(
                structure.folds.iter().any(|fold| fold.start_line == line),
                "missing outer fold for {name} at {line}; regions: {:?}",
                structure.folds
            );
        }
    }

    #[test]
    fn outer_python_headers_with_trailing_comments_are_foldable() {
        let source = "def outer(): # outer block\n    if ready: # nested block\n        while work:\n            run()\n    finish()\n";
        let language = Language::from_slug("python").unwrap();
        let buffer = Buffer::new(language, source).unwrap();
        let structure = Structure::new(Some(&buffer), Some(language));
        assert_eq!(
            structure
                .folds
                .iter()
                .map(|fold| (fold.start_line, fold.end_line))
                .collect::<Vec<_>>(),
            vec![(0, 4), (1, 3), (2, 3)]
        );
    }
}
