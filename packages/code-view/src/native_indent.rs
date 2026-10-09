//! Syntax-aware Enter planning; edits still pass through the engine delta pipeline.
use crate::{editor_core_spike::place_caret_at, native_model::NativeModel};
use arborium_tree_sitter::Parser;
use editor_core::{Command, EditCommand, EditorStateManager, IndentStyle, Position};

thread_local! {
    static GRAMMARS: arborium::GrammarStore = arborium::GrammarStore::new();
}

pub(crate) struct IndentEdit {
    start: usize,
    length: usize,
    text: String,
    caret: usize,
}
impl IndentEdit {
    pub fn apply(self, engine: &mut EditorStateManager) {
        if engine
            .execute(Command::Edit(EditCommand::Replace {
                start: self.start,
                length: self.length,
                text: self.text,
            }))
            .is_ok()
        {
            let (line, column) = engine
                .editor()
                .line_index()
                .char_offset_to_position(self.start + self.caret);
            place_caret_at(engine, Position::new(line, column));
        }
    }
}

pub(crate) fn newline(model: &NativeModel) -> IndentEdit {
    let cursor = model.engine.get_cursor_state();
    let (start, end) = cursor
        .selection
        .as_ref()
        .map(|selection| {
            if selection.start <= selection.end {
                (selection.start, selection.end)
            } else {
                (selection.end, selection.start)
            }
        })
        .unwrap_or((cursor.position, cursor.position));
    let index = model.engine.editor().line_index();
    let offset = index.position_to_char_offset(start.line, start.column);
    let end_offset = index.position_to_char_offset(end.line, end.column);
    let line = index.get_line_text(start.line).unwrap_or_default();
    let prefix: String = line.chars().take(start.column).collect();
    let end_line = index.get_line_text(end.line).unwrap_or_default();
    let suffix: String = end_line.chars().skip(end.column).collect();
    let base: String = prefix
        .chars()
        .take_while(|ch| matches!(ch, ' ' | '\t'))
        .collect();
    let config = crate::native_language::indentation(
        model.language,
        model.preferences.insert_spaces,
        model.preferences.indent_width,
    );
    let unit = match config.style {
        IndentStyle::Tabs => "\t".into(),
        IndentStyle::Spaces(width) => " ".repeat(width as usize),
    };
    let byte_start = index.char_offset_to_byte_offset(index.position_to_char_offset(start.line, 0));
    let byte_cursor = byte_start + prefix.len();
    let byte_end = index.char_offset_to_byte_offset(end_offset);
    let mut increase = false;
    let mut split = false;
    if let Some(buffer) = &model.highlight {
        let spans = buffer.spans();
        let excluded = |byte| {
            let i = spans.partition_point(|span| span.end() as usize <= byte);
            spans.get(i).is_some_and(|span| {
                span.start() as usize <= byte && matches!(span.tag(), "s" | "c")
            })
        };
        let inside_string = spans.iter().any(|span| {
            span.tag() == "s"
                && (span.start() as usize) < byte_cursor
                && (span.end() as usize) > byte_cursor
        });
        if !inside_string {
            let last = prefix
                .char_indices()
                .rev()
                .find(|(byte, ch)| !ch.is_whitespace() && !excluded(byte_start + byte));
            if let Some((_, ch)) = last {
                increase = config.indent_triggers.contains(&ch);
                let closer = match ch {
                    '(' => Some(')'),
                    '[' => Some(']'),
                    '{' => Some('}'),
                    _ => None,
                };
                let whitespace = suffix.len() - suffix.trim_start_matches([' ', '\t']).len();
                split = increase
                    && closer.is_some_and(|ch| suffix[whitespace..].starts_with(ch))
                    && !excluded(byte_end + whitespace);
            }
            match model.language.map(|language| language.slug()) {
                Some("html") => {
                    (increase, split) = html_context(
                        buffer.source(),
                        byte_start + prefix.trim_end().len(),
                        byte_end,
                        &suffix,
                    );
                }
                Some("julia") => {
                    increase |= julia_header(buffer.source(), byte_start, byte_cursor);
                }
                _ => {}
            }
        }
    }
    let indent = if increase {
        format!("{base}{unit}")
    } else {
        base.clone()
    };
    let caret = 1 + indent.chars().count();
    IndentEdit {
        start: offset,
        length: end_offset - offset
            + if split {
                suffix
                    .chars()
                    .take_while(|ch| matches!(ch, ' ' | '\t'))
                    .count()
            } else {
                0
            },
        text: if split {
            format!("\n{indent}\n{base}")
        } else {
            format!("\n{indent}")
        },
        caret,
    }
}

/// Complete a leading closing token and align it in one edit. Other typing
/// keeps the engine's normal auto-pair and typing-group behavior.
pub(crate) fn closing(model: &NativeModel, ch: char) -> Option<IndentEdit> {
    let language = model.language?;
    if !matches!(ch, ')' | ']' | '}')
        && !(language.slug() == "julia" && ch == 'd')
        && !(language.slug() == "html" && ch == '>')
    {
        return None;
    }
    let cursor = model.engine.get_cursor_state();
    if cursor.selection.is_some() {
        return None;
    }
    let index = model.engine.editor().line_index();
    let line = index.get_line_text(cursor.position.line)?;
    let prefix: String = line.chars().take(cursor.position.column).collect();
    let leading: String = prefix
        .chars()
        .take_while(|ch| matches!(ch, ' ' | '\t'))
        .collect();
    let token = &prefix[leading.len()..];
    let bracket = matches!(ch, ')' | ']' | '}') && token.is_empty();
    let julia = language.slug() == "julia"
        && ch == 'd'
        && token == "en"
        && !line
            .chars()
            .nth(cursor.position.column)
            .is_some_and(|ch| ch == '_' || ch.is_alphanumeric());
    let html = language.slug() == "html" && ch == '>' && token.starts_with("</");
    if !bracket && !julia && !html {
        return None;
    }
    let buffer = model.highlight.as_ref()?;
    let offset = cursor.offset;
    let byte = index.char_offset_to_byte_offset(offset);
    let skip = bracket && line.chars().nth(cursor.position.column) == Some(ch);
    let mut source = buffer.source().to_owned();
    if !skip {
        source.insert(byte, ch);
    }
    let opener_line = if bracket {
        if matches!(language.slug(), "html" | "markdown") {
            return None;
        }
        let grammar = GRAMMARS.with(|store| store.get(language.slug()))?;
        let mut parser = Parser::new();
        parser.set_language(grammar.language()).ok()?;
        let tree = parser.parse(&source, None)?;
        let node = tree
            .root_node()
            .descendant_for_byte_range(byte, byte + ch.len_utf8())?;
        // Highlight queries can omit unfinished strings. Require the actual
        // closing token to belong to a valid local syntax construct as well.
        if node.kind() != ch.to_string()
            || node.parent().is_none_or(|parent| {
                parent.is_error() || parent.has_error() || damaged_prefix(parent)
            })
        {
            return None;
        }
        let candidate = dioxus_code::advanced::Buffer::new(language, &source).ok()?;
        let structure = crate::native_structure::Structure::new(Some(&candidate), Some(language));
        let opener = *structure.pairs.get(&offset)?;
        source.chars().take(opener).filter(|ch| *ch == '\n').count()
    } else {
        let mut parser = Parser::new();
        let grammar = if julia {
            arborium_julia::language()
        } else {
            arborium_html::language()
        };
        parser.set_language(&grammar.into()).ok()?;
        let tree = parser.parse(&source, None)?;
        let mut node = tree
            .root_node()
            .descendant_for_byte_range(byte, byte + ch.len_utf8())?;
        if julia {
            if node.kind() != "end" {
                return None;
            }
            node = node.parent()?;
            if !matches!(
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
            ) || node.end_byte() != byte + 1
                || node.has_error()
                || damaged_prefix(node)
            {
                return None;
            }
            node.start_position().row
        } else {
            while node.kind() != "end_tag" {
                node = node.parent()?;
            }
            if node.has_error() {
                return None;
            }
            let parent = node.parent()?;
            if parent.has_error() || damaged_prefix(parent) {
                return None;
            }
            let open = parent.named_child(0)?;
            if open.kind() != "start_tag" {
                return None;
            }
            open.start_position().row
        }
    };
    if opener_line >= cursor.position.line {
        return None;
    }
    let indent: String = source
        .lines()
        .nth(opener_line)?
        .chars()
        .take_while(|ch| matches!(ch, ' ' | '\t'))
        .collect();
    if leading == indent {
        return None;
    }
    let text = format!("{indent}{token}{ch}");
    let caret = text.chars().count();
    Some(IndentEdit {
        start: index.position_to_char_offset(cursor.position.line, 0),
        length: cursor.position.column + usize::from(skip),
        text,
        caret,
    })
}

fn damaged_prefix(node: arborium_tree_sitter::Node<'_>) -> bool {
    node.prev_named_sibling().is_some_and(|previous| {
        previous.is_error() && previous.end_position().row == node.start_position().row
    })
}

fn html_context(source: &str, before: usize, after: usize, suffix: &str) -> (bool, bool) {
    let mut parser = Parser::new();
    if parser
        .set_language(&arborium_html::language().into())
        .is_err()
    {
        return (false, false);
    }
    let Some(tree) = parser.parse(source, None) else {
        return (false, false);
    };
    let Some(mut node) = before
        .checked_sub(1)
        .and_then(|byte| tree.root_node().descendant_for_byte_range(byte, before))
    else {
        return (false, false);
    };
    while node.kind() != "start_tag" {
        let Some(parent) = node.parent() else {
            return (false, false);
        };
        node = parent;
    }
    if node.end_byte() != before || node.has_error() {
        return (false, false);
    }
    let Some(name) = node
        .named_child(0)
        .and_then(|child| source.get(child.byte_range()))
    else {
        return (false, false);
    };
    if matches!(
        name.to_ascii_lowercase().as_str(),
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    ) {
        return (false, false);
    }
    let whitespace = suffix.len() - suffix.trim_start_matches([' ', '\t']).len();
    let split = node
        .parent()
        .and_then(|parent| parent.named_child(parent.named_child_count().saturating_sub(1) as u32))
        .is_some_and(|child| {
            child.kind() == "end_tag"
                && child.start_byte() == after + whitespace
                && !child.has_error()
        });
    (true, split)
}

fn julia_header(source: &str, start: usize, end: usize) -> bool {
    let mut parser = Parser::new();
    if parser
        .set_language(&arborium_julia::language().into())
        .is_err()
    {
        return false;
    }
    let Some(tree) = parser.parse(source, None) else {
        return false;
    };
    let mut cursor = tree.walk();
    let mut increase = false;
    loop {
        let node = cursor.node();
        if node.child_count() == 0
            && node.start_byte() >= start
            && node.end_byte() <= end
            && !node.is_missing()
        {
            match node.kind() {
                "function" | "macro" | "module" | "baremodule" | "struct" | "for" | "while"
                | "if" | "elseif" | "else" | "try" | "catch" | "finally" | "let" | "begin"
                | "quote" | "do" => increase = true,
                "end" => increase = false,
                _ => {}
            }
        }
        if node.start_byte() < end && node.end_byte() > start && cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return increase;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_model::{delta_batch, Preferences};
    use moonkale_core::NodeId;
    use moonkale_ext_api::editor::{RevisionedDocument, Utf16Selection};
    fn make_model(source: &str, slug: &str, offset: usize) -> NativeModel {
        let document = RevisionedDocument::new(NodeId::fresh("indent"), source.into());
        let mut model = NativeModel::new(
            document.snapshot(),
            crate::native_language::language_for_hint(Some(slug)),
        );
        let (line, column) = model
            .engine
            .editor()
            .line_index()
            .char_offset_to_position(offset);
        place_caret_at(&mut model.engine, Position::new(line, column));
        model
    }
    fn enter(source: &str, slug: &str, offset: usize) -> (String, usize) {
        let mut model = make_model(source, slug, offset);
        newline(&model).apply(&mut model.engine);
        (
            model.engine.editor().get_text(),
            model.engine.get_cursor_state().offset,
        )
    }
    #[test]
    fn closing_delimiters_align_nested_openers_and_skip_existing_closers() {
        for (source, ch, expected) in [
            (
                "fn f() {\n    if ready {\n        ",
                '}',
                "fn f() {\n    if ready {\n    }",
            ),
            ("let x = [\n    ", ']', "let x = [\n]"),
            ("  call(\n      ", ')', "  call(\n  )"),
            ("\tif ready {\n\t\t", '}', "\tif ready {\n\t}"),
        ] {
            let mut model = make_model(source, "rust", source.chars().count());
            closing(&model, ch).unwrap().apply(&mut model.engine);
            assert_eq!(model.engine.editor().get_text(), expected);
            assert_eq!(
                model.engine.get_cursor_state().offset,
                expected.chars().count()
            );
        }
        let source = "fn f() {\n    }";
        let mut model = make_model(source, "rust", source.chars().count() - 1);
        closing(&model, '}').unwrap().apply(&mut model.engine);
        assert_eq!(model.engine.editor().get_text(), "fn f() {\n}");
        assert_eq!(model.engine.get_cursor_state().offset, 10);
    }
    #[test]
    fn julia_end_and_html_end_tags_align_their_actual_parent() {
        for (source, slug, ch, expected) in [
            (
                "module Demo\n    function f()\n        en",
                "julia",
                'd',
                "module Demo\n    function f()\n    end",
            ),
            (
                "<main>\n  <section>\n      </section",
                "html",
                '>',
                "<main>\n  <section>\n  </section>",
            ),
        ] {
            let mut model = make_model(source, slug, source.chars().count());
            closing(&model, ch).unwrap().apply(&mut model.engine);
            assert_eq!(model.engine.editor().get_text(), expected);
        }
    }
    #[test]
    fn closing_alignment_excludes_strings_comments_invalid_tokens_and_selections() {
        for (source, slug, ch) in [
            ("let x = r#\"{\n    ", "rust", '}'),
            ("fn f() {\n    // ", "rust", '}'),
            ("let x = [\n    ", "rust", '}'),
            ("    ", "unknown", '}'),
            ("#= function f()\n    en", "julia", 'd'),
            ("    en", "julia", 'd'),
            ("<main>\n    </other", "html", '>'),
            ("<!-- <main>\n    </main", "html", '>'),
        ] {
            let model = make_model(source, slug, source.chars().count());
            assert!(closing(&model, ch).is_none(), "{slug}: {source}");
        }
        let mut model = make_model("fn f() {\n    text", "rust", 0);
        model
            .engine
            .execute(Command::Cursor(editor_core::CursorCommand::SetSelection {
                start: Position::new(1, 4),
                end: Position::new(1, 8),
            }))
            .unwrap();
        assert!(closing(&model, '}').is_none());
    }
    #[test]
    fn closing_alignment_is_one_crlf_preserving_undo_edit() {
        let source = "fn f() { // 😀\r\n    ";
        let mut document = RevisionedDocument::new(NodeId::fresh("closing-history"), source.into());
        let base = document.snapshot();
        let mut model = NativeModel::new(base, Some(dioxus_code::Language::Rust));
        place_caret_at(&mut model.engine, Position::new(1, 4));
        closing(&model, '}').unwrap().apply(&mut model.engine);
        let delta = model.engine.take_last_text_delta().unwrap();
        let batch = delta_batch(base, &delta, Utf16Selection { anchor: 0, head: 0 }).unwrap();
        let expected = "fn f() { // 😀\r\n}";
        assert_eq!(document.apply(&batch).unwrap().text, expected);
        assert_eq!(document.undo().unwrap().unwrap().text, source);
        assert_eq!(document.redo().unwrap().unwrap().text, expected);
    }
    #[test]
    fn trailing_comments_use_code_context_and_strings_do_not_trigger() {
        for (source, slug, indent) in [
            ("fn f() { // 😀 comment", "rust", "    "),
            ("if ready: # 中 comment", "python", "    "),
            ("# fake:", "python", ""),
            ("text = 'fake:'", "python", ""),
            ("// fake {", "rust", ""),
            ("let text = r#\"{\"#;", "rust", ""),
        ] {
            assert_eq!(
                enter(source, slug, source.chars().count()).0,
                format!("{source}\n{indent}"),
                "{source}"
            );
        }
        let source = "let text = \"{abc\";";
        assert_eq!(enter(source, "rust", 16).0, "let text = \"{abc\n\";");
    }
    #[test]
    fn structural_pair_splits_align_closer_and_place_caret_inside() {
        assert_eq!(enter("{}", "rust", 1), ("{\n    \n}".into(), 6));
        assert_eq!(
            enter("fn f() {   }", "rust", 8),
            ("fn f() {\n    \n}".into(), 13)
        );
        let mut model = make_model("fn f() {}", "rust", 8);
        model.set_preferences(Preferences {
            insert_spaces: Some(false),
            indent_width: Some(2),
            ..Default::default()
        });
        newline(&model).apply(&mut model.engine);
        assert_eq!(model.engine.editor().get_text(), "fn f() {\n\t\n}");
        let mut model = make_model("if ready: # hi", "python", 14);
        model.set_preferences(Preferences {
            insert_spaces: Some(true),
            indent_width: Some(2),
            ..Default::default()
        });
        newline(&model).apply(&mut model.engine);
        assert_eq!(model.engine.editor().get_text(), "if ready: # hi\n  ");
    }
    #[test]
    fn julia_keywords_are_parser_tokens_including_incomplete_headers() {
        for source in [
            "function outer(x)",
            "module Demo",
            "mutable struct Item",
            "for x in items",
            "map(items) do x",
            "if ready # comment",
        ] {
            assert_eq!(
                enter(source, "julia", source.chars().count()).0,
                format!("{source}\n    "),
                "{source}"
            );
        }
        for source in [
            "println(\"function end\")",
            "# function fake",
            "if x; work(); end",
        ] {
            assert_eq!(
                enter(source, "julia", source.chars().count()).0,
                format!("{source}\n"),
                "{source}"
            );
        }
    }
    #[test]
    fn html_tags_split_and_void_tags_remain_flat() {
        assert_eq!(
            enter("<main></main>", "html", 6),
            ("<main>\n  \n</main>".into(), 9)
        );
        for source in ["<main data-note=\"> 😀\">", "  <section>"] {
            let base = if source.starts_with(' ') {
                "    "
            } else {
                "  "
            };
            assert_eq!(
                enter(source, "html", source.chars().count()).0,
                format!("{source}\n{base}")
            );
        }
        for source in [
            "<br>",
            "<img src=\"x\">",
            "<widget />",
            "<!-- <main> -->",
            "<script>const x = '<main>';",
        ] {
            assert_eq!(
                enter(source, "html", source.chars().count()).0,
                format!("{source}\n"),
                "{source}"
            );
        }
    }
    #[test]
    fn backward_multiline_selection_and_plain_fallback() {
        let mut model = make_model("if ready:discard\nold😀tail", "python", 0);
        model
            .engine
            .execute(Command::Cursor(editor_core::CursorCommand::SetSelection {
                start: Position::new(1, 4),
                end: Position::new(0, 9),
            }))
            .unwrap();
        newline(&model).apply(&mut model.engine);
        assert_eq!(model.engine.editor().get_text(), "if ready:\n    tail");
        assert_eq!(enter("  literal {", "unknown", 11).0, "  literal {\n  ");
        assert_eq!(enter("    text", "unknown", 2).0, "  \n    text");
    }
    #[test]
    fn failed_highlighting_recovers_for_subsequent_syntax_edits() {
        let mut document = RevisionedDocument::new(NodeId::fresh("syntax-recovery"), "".into());
        let base = document.snapshot();
        let mut model = NativeModel::new(base, Some(dioxus_code::Language::Rust));
        model.highlight = None;
        model
            .engine
            .execute(Command::Edit(EditCommand::TypeChar { ch: '{' }))
            .unwrap();
        let delta = model.engine.take_last_text_delta().unwrap();
        let batch = delta_batch(base, &delta, Utf16Selection { anchor: 1, head: 1 }).unwrap();
        let snapshot = document.apply(&batch).unwrap();
        model.update_highlight(&snapshot, &batch);
        assert!(model.highlight.is_some());
        newline(&model).apply(&mut model.engine);
        assert_eq!(model.engine.editor().get_text(), "{\n    \n}");
    }
    #[test]
    fn newline_indent_is_one_canonical_crlf_history_edit() {
        let source = "if ready: # 😀\r\n    run()\r\n";
        let mut document = RevisionedDocument::new(NodeId::fresh("indent-history"), source.into());
        let base = document.snapshot();
        let mut model = NativeModel::new(
            base,
            crate::native_language::language_for_hint(Some("python")),
        );
        place_caret_at(&mut model.engine, Position::new(0, 13));
        newline(&model).apply(&mut model.engine);
        let delta = model.engine.take_last_text_delta().unwrap();
        let batch = delta_batch(base, &delta, Utf16Selection { anchor: 0, head: 0 }).unwrap();
        let expected = "if ready: # 😀\r\n    \r\n    run()\r\n";
        assert_eq!(document.apply(&batch).unwrap().text, expected);
        assert_eq!(document.undo().unwrap().unwrap().text, source);
        assert_eq!(document.redo().unwrap().unwrap().text, expected);
    }
}
