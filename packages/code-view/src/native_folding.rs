//! Grammar folds for languages whose blocks are not described by braces.
use arborium_tree_sitter::{InputEdit, Node, Parser, Point, Tree};
use dioxus_code::Language;
use std::{cell::RefCell, collections::BTreeSet};

pub(super) fn extend(
    source: &str,
    language: Option<Language>,
    folds: &mut BTreeSet<(usize, usize)>,
) {
    let Some(language) = language else { return };
    if language.slug() == "lean" {
        crate::native_lean_folding::extend(source, folds);
        return;
    }
    let grammar = match language.slug() {
        "html" => arborium_html::language(),
        "markdown" => arborium_markdown::language(),
        "julia" => arborium_julia::language(),
        _ => return,
    };
    let Some(tree) = parsed_tree(source, language.slug(), grammar.into()) else {
        return;
    };
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        if !node.has_error() && !node.is_missing() && foldable(node, language.slug()) {
            let start = node.start_position().row;
            let end = node.end_position();
            // Tree-sitter ranges are exclusive; a node ending at the next
            // line's column zero must not hide that line.
            let end = end.row.saturating_sub(usize::from(end.column == 0));
            if end > start {
                folds.insert((start, end));
            }
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return;
            }
        }
    }
}

struct Parsed {
    language: &'static str,
    source: String,
    parser: Parser,
    tree: Tree,
}
thread_local! {
    // Bound retained parser state even when several editors/languages are open.
    static PARSED: RefCell<Vec<Parsed>> = const { RefCell::new(Vec::new()) };
}
fn parsed_tree(
    source: &str,
    language: &'static str,
    grammar: arborium_tree_sitter::Language,
) -> Option<Tree> {
    PARSED.with(|cache| {
        let mut cache = cache.borrow_mut();
        let previous = cache
            .iter()
            .position(|entry| entry.language == language)
            .map(|index| cache.remove(index));
        let parsed = if let Some(mut previous) = previous {
            if previous.source != source {
                previous.tree.edit(&difference(&previous.source, source));
                previous.tree = previous.parser.parse(source, Some(&previous.tree))?;
                previous.source = source.to_owned();
            }
            previous
        } else {
            let mut parser = Parser::new();
            parser.set_language(&grammar).ok()?;
            let tree = parser.parse(source, None)?;
            Parsed {
                language,
                source: source.to_owned(),
                parser,
                tree,
            }
        };
        let tree = parsed.tree.clone();
        // Keep the latest document of each grammar, evicting least recently used.
        if cache.len() == 3 {
            cache.remove(0);
        }
        cache.push(parsed);
        Some(tree)
    })
}
fn difference(old: &str, new: &str) -> InputEdit {
    let mut start = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(start) || !new.is_char_boundary(start) {
        start -= 1;
    }
    let mut suffix = old[start..]
        .bytes()
        .rev()
        .zip(new[start..].bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix) {
        suffix -= 1;
    }
    let point = |text: &str, byte: usize| Point {
        row: text[..byte].bytes().filter(|byte| *byte == b'\n').count(),
        column: byte - text[..byte].rfind('\n').map_or(0, |byte| byte + 1),
    };
    InputEdit {
        start_byte: start,
        old_end_byte: old.len() - suffix,
        new_end_byte: new.len() - suffix,
        start_position: point(old, start),
        old_end_position: point(old, old.len() - suffix),
        new_end_position: point(new, new.len() - suffix),
    }
}

fn foldable(node: Node<'_>, language: &str) -> bool {
    match language {
        "html" => match node.kind() {
            "element" | "script_element" | "style_element" => {
                // Implicit/unfinished HTML elements are useful while editing,
                // but should not hide the rest of the document.
                node.named_child_count()
                    .checked_sub(1)
                    .and_then(|index| node.named_child(index as u32))
                    .is_some_and(|child| child.kind() == "end_tag" && !child.has_error())
            }
            "comment" => true,
            _ => false,
        },
        "markdown" => match node.kind() {
            "section" => node
                .named_child(0)
                .is_some_and(|child| matches!(child.kind(), "atx_heading" | "setext_heading")),
            "fenced_code_block" | "indented_code_block" | "block_quote" | "list" | "list_item" => {
                true
            }
            _ => false,
        },
        "julia" => matches!(
            node.kind(),
            "function_definition"
                | "macro_definition"
                | "module_definition"
                | "struct_definition"
                | "abstract_definition"
                | "primitive_definition"
                | "if_statement"
                | "for_statement"
                | "while_statement"
                | "try_statement"
                | "let_statement"
                | "compound_statement"
                | "quote_statement"
                | "do_clause"
                | "block_comment"
        ),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incremental_folds_match_fresh_grammar_after_unicode_and_multiline_edits() {
        let versions = [
            "# 猫\n\n- one\n  - two\n\nend\n",
            "# 界\n\n- one\n  - two\n\nend😀\n",
            "# 界\n\n```rust\nfn f() {}\n```\n\nend😀\n",
            "# 界\n\n```rust\nfn f() {}\n\nend😀\n",
            "# Other document\n\n> quote\n> 猫\n",
            "",
        ];
        let grammar: arborium_tree_sitter::Language = arborium_markdown::language().into();
        let mut fresh = Parser::new();
        fresh.set_language(&grammar).unwrap();
        for source in versions {
            let incremental = parsed_tree(source, "markdown", grammar.clone()).unwrap();
            let expected = fresh.parse(source, None).unwrap();
            assert_eq!(
                incremental.root_node().to_sexp(),
                expected.root_node().to_sexp()
            );
        }
    }
    fn ranges(source: &str, slug: &str) -> BTreeSet<(usize, usize)> {
        let mut folds = BTreeSet::new();
        extend(source, Language::from_slug(slug), &mut folds);
        folds
    }
    #[test]
    fn lean_nested_declarations_and_proofs_exclude_following_siblings() {
        let source = "namespace Demo\nsection Inner\ntheorem outer : True := by\n  have h : True := by\n    trivial\n  exact h\nend Inner\nend Demo\ndef next : Nat :=\n  42\n";
        let folds = ranges(source, "lean");
        assert!(folds.contains(&(0, 7)), "{folds:?}");
        assert!(folds.contains(&(1, 6)), "{folds:?}");
        assert!(folds.contains(&(2, 5)), "{folds:?}");
        assert!(folds.contains(&(3, 4)), "{folds:?}");
        assert!(folds.contains(&(8, 9)), "{folds:?}");
        assert!(!folds.iter().any(|(start, end)| *start == 2 && *end >= 6));
    }
    #[test]
    fn lean_multiline_comments_and_strings_do_not_create_fake_proofs() {
        let source = "/- namespace Fake\n theorem fake : True := by\n end Fake -/\ndef text : String :=\n  \"namespace Fake\\nend Fake 😀中\"\n";
        assert_eq!(ranges(source, "lean"), BTreeSet::from([(0, 2), (3, 4)]));
        assert!(ranges("namespace Unfinished\n", "lean").is_empty());
        assert!(ranges("theorem unfinished : True := by\n", "lean").is_empty());
    }
    #[test]
    fn lean_layout_handles_modifiers_nested_comments_and_mutual_scopes() {
        assert_eq!(ranges("namespace A\n@[simp] private theorem helper : True := by -- trailing\n  trivial\nend A\n", "lean"), BTreeSet::from([(0,3),(1,2)]));
        assert_eq!(ranges("/- outer\n /- inner\nnamespace Fake\n -/\nend Fake\n-/\ntheorem real : True := by\n  trivial\n", "lean"), BTreeSet::from([(0,5),(1,3),(6,7)]));
        assert_eq!(
            ranges(
                "mutual\ndef first : Nat :=\n  1\ndef second : Nat :=\n  2\nend\n",
                "lean"
            ),
            BTreeSet::from([(0, 5), (1, 2), (3, 4)])
        );
        assert!(ranges("namespace A\nend B\n", "lean").is_empty());
        assert!(ranges("/- unfinished\nnamespace Fake\nend Fake\n", "lean").is_empty());
    }
    #[test]
    fn lean_multiline_strings_and_char_quotes_remain_opaque_to_layout() {
        let source = "def text : String :=\n  \"namespace Fake\nend Fake\n\"\ndef quote : Char := '\"'\ntheorem next : True := by\n  trivial\n";
        assert_eq!(ranges(source, "lean"), BTreeSet::from([(0, 3), (5, 6)]));
        assert_eq!(
            ranges("def n' : Nat :=\n  1\n", "lean"),
            BTreeSet::from([(0, 1)])
        );
    }
    #[test]
    fn html_nested_elements_comments_and_raw_text() {
        let source = "<main data-note=\"<fake>\">\n  <section>\n    😀中\n    <br>\n  </section>\n  <!--\n    <pretend>\n  -->\n  <script>\n    const text = '<fake>';\n  </script>\n</main>\n<p>next</p>\n";
        assert_eq!(
            ranges(source, "html"),
            BTreeSet::from([(0, 11), (1, 4), (5, 7), (8, 10)])
        );
        assert!(ranges("<main>\n  unfinished\n", "html").is_empty());
    }
    #[test]
    fn markdown_sections_fences_and_quotes_exclude_next_heading() {
        let source = "# Outer 😀\nintro\n## Nested\n```rust\n# fake heading\nfn x() {}\n```\n## Next\n> quoted\n> text\n# Sibling\nlast\n";
        let folds = ranges(source, "markdown");
        assert!(!ranges("plain\ntext\n", "markdown").contains(&(0, 1)));
        assert!(folds.contains(&(0, 9)), "{folds:?}");
        assert!(folds.contains(&(2, 6)), "{folds:?}");
        assert!(folds.contains(&(3, 6)), "{folds:?}");
        assert!(folds.contains(&(7, 9)), "{folds:?}");
        assert!(folds.contains(&(8, 9)), "{folds:?}");
        assert!(
            !folds.iter().any(|(start, _)| *start == 4),
            "fenced heading became a section"
        );
    }
    #[test]
    fn julia_nested_keyword_blocks_ignore_strings_and_comments() {
        let source = "module Demo\nfunction outer(x)\n    if x\n        println(\"end 😀\") # end\n    end\n    #= comment\n       end\n    =#\nend\nend\n";
        assert_eq!(
            ranges(source, "julia"),
            BTreeSet::from([(0, 9), (1, 8), (2, 4), (5, 7)])
        );
        assert!(ranges("function unfinished(x)\n    x\n", "julia").is_empty());
        assert!(ranges(source, "unknown").is_empty());
    }
}
