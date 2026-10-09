//! Shared grammar and editing defaults. These capabilities do not imply an LSP server.
use dioxus_code::Language;
use editor_core::{
    AutoPair, AutoPairsConfig, Command, CommentConfig, EditorStateManager, IndentStyle,
    IndentationConfig, TabKeyBehavior, ViewCommand,
};

pub(crate) fn language_for_hint(hint: Option<&str>) -> Option<Language> {
    Language::from_slug(match hint? {
        "shell" | "bash" => "bash",
        "pixi" => "toml",
        "postgres" => "sql",
        other => other,
    })
}

pub(crate) fn comments(language: Option<Language>) -> Option<CommentConfig> {
    Some(match language?.slug() {
        "rust" | "c" | "cpp" | "javascript" | "typescript" | "tsx" | "go" | "typst" => {
            CommentConfig::line("//")
        }
        "python" | "julia" | "nix" | "toml" | "yaml" | "bash" | "graphql" => {
            CommentConfig::line("#")
        }
        "sql" => CommentConfig::line("--"),
        "lean" => CommentConfig::line("--"),
        "html" | "markdown" => CommentConfig::block("<!--", "-->"),
        "css" => CommentConfig::block("/*", "*/"),
        _ => return None,
    })
}

pub(crate) fn configure(engine: &mut EditorStateManager, language: Option<Language>) {
    configure_preferences(engine, language, None, None);
    // Quotes require language/token context (e.g. Rust lifetimes). Start with structural pairs.
    let config = AutoPairsConfig {
        enabled: language.is_some(),
        pairs: vec![
            AutoPair::new('(', ')'),
            AutoPair::new('[', ']'),
            AutoPair::new('{', '}'),
        ],
        ..Default::default()
    };
    let _ = engine.execute(Command::View(ViewCommand::SetAutoPairsConfig { config }));
}

/// Apply optional user indentation overrides after language defaults.
pub(crate) fn configure_preferences(
    engine: &mut EditorStateManager,
    language: Option<Language>,
    insert_spaces: Option<bool>,
    width: Option<u8>,
) {
    let (default_spaces, default_width) = indent_defaults(language);
    let width = width.unwrap_or(default_width).clamp(1, 8);
    let spaces = insert_spaces.unwrap_or(default_spaces);
    let config = indentation(language, insert_spaces, Some(width));
    let _ = engine.execute(Command::View(ViewCommand::SetIndentationConfig { config }));
    let _ = engine.execute(Command::View(ViewCommand::SetTabWidth {
        width: width as usize,
    }));
    let _ = engine.execute(Command::View(ViewCommand::SetTabKeyBehavior {
        behavior: if spaces {
            TabKeyBehavior::Spaces
        } else {
            TabKeyBehavior::Tab
        },
    }));
}

pub(crate) fn indentation(
    language: Option<Language>,
    insert_spaces: Option<bool>,
    width: Option<u8>,
) -> IndentationConfig {
    let slug = language.map(Language::slug).unwrap_or_default();
    let (default_spaces, default_width) = indent_defaults(language);
    let width = width.unwrap_or(default_width).clamp(1, 8);
    let spaces = insert_spaces.unwrap_or(default_spaces);
    IndentationConfig {
        style: if spaces {
            IndentStyle::Spaces(width)
        } else {
            IndentStyle::Tabs
        },
        indent_triggers: match slug {
            "python" => vec![':', '[', '(', '{'],
            "rust" | "c" | "cpp" | "javascript" | "typescript" | "tsx" | "go" | "css" | "json"
            | "nix" | "julia" => vec!['{', '[', '('],
            _ => vec![],
        },
        ..Default::default()
    }
}

pub(crate) fn indent_defaults(language: Option<Language>) -> (bool, u8) {
    let slug = language.map(Language::slug).unwrap_or_default();
    let width = match slug {
        "javascript" | "typescript" | "tsx" | "json" | "yaml" | "html" | "css" | "nix" => 2,
        _ => 4,
    };
    (slug != "go", width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor_core::{CursorCommand, EditCommand, Position};
    fn engine(text: &str, hint: &str) -> EditorStateManager {
        let mut engine = EditorStateManager::new(text, 80);
        configure(&mut engine, language_for_hint(Some(hint)));
        engine
    }
    fn edit(engine: &mut EditorStateManager, command: EditCommand) {
        engine.execute(Command::Edit(command)).unwrap();
    }
    #[test]
    fn pairs_wrap_skip_delete_and_plain_text_stays_literal() {
        let mut rust = engine("", "rust");
        edit(&mut rust, EditCommand::TypeChar { ch: '(' });
        assert_eq!(rust.editor().get_text(), "()");
        edit(&mut rust, EditCommand::TypeChar { ch: ')' });
        assert_eq!(rust.editor().get_text(), "()");
        assert_eq!(rust.get_cursor_state().position.column, 2);
        rust.execute(Command::Cursor(CursorCommand::MoveTo {
            line: 0,
            column: 1,
        }))
        .unwrap();
        edit(&mut rust, EditCommand::Backspace);
        assert_eq!(rust.editor().get_text(), "");
        edit(
            &mut rust,
            EditCommand::InsertText {
                text: "😀".into()
            },
        );
        rust.execute(Command::Cursor(CursorCommand::SetSelection {
            start: Position::new(0, 0),
            end: Position::new(0, 1),
        }))
        .unwrap();
        edit(&mut rust, EditCommand::TypeChar { ch: '[' });
        assert_eq!(rust.editor().get_text(), "[😀]");
        let mut plain = engine("", "unknown");
        edit(&mut plain, EditCommand::TypeChar { ch: '{' });
        assert_eq!(plain.editor().get_text(), "{");
    }
    #[test]
    fn language_indentation_and_comment_defaults() {
        let mut rust = engine("{}", "rust");
        rust.execute(Command::Cursor(CursorCommand::MoveTo {
            line: 0,
            column: 1,
        }))
        .unwrap();
        edit(&mut rust, EditCommand::InsertNewline { auto_indent: true });
        assert_eq!(rust.editor().get_text(), "{\n    \n}");
        let mut python = engine("if ready:", "python");
        python
            .execute(Command::Cursor(CursorCommand::MoveToLineEnd))
            .unwrap();
        edit(
            &mut python,
            EditCommand::InsertNewline { auto_indent: true },
        );
        assert_eq!(python.editor().get_text(), "if ready:\n    ");
        let mut go = engine("", "go");
        edit(&mut go, EditCommand::InsertTab);
        assert_eq!(go.editor().get_text(), "\t");
        assert!(comments(language_for_hint(Some("json"))).is_none());
        assert_eq!(
            comments(language_for_hint(Some("postgres")))
                .unwrap()
                .line
                .as_deref(),
            Some("--")
        );
    }
    #[test]
    fn comments_and_indent_preserve_unicode_selected_lines() {
        let mut rust = engine("😀\n中\n", "rust");
        rust.execute(Command::Cursor(CursorCommand::SetSelection {
            start: Position::new(0, 0),
            end: Position::new(1, 1),
        }))
        .unwrap();
        edit(
            &mut rust,
            EditCommand::ToggleComment {
                config: comments(Some(Language::Rust)).unwrap(),
            },
        );
        assert_eq!(rust.editor().get_text(), "// 😀\n// 中\n");
        edit(
            &mut rust,
            EditCommand::ToggleComment {
                config: comments(Some(Language::Rust)).unwrap(),
            },
        );
        assert_eq!(rust.editor().get_text(), "😀\n中\n");
        edit(&mut rust, EditCommand::Indent);
        assert_eq!(rust.editor().get_text(), "    😀\n    中\n");
        edit(&mut rust, EditCommand::Outdent);
        assert_eq!(rust.editor().get_text(), "😀\n中\n");
    }
    #[test]
    fn comment_selection_excludes_trailing_line_in_both_directions() {
        for (start, end) in [
            (Position::new(0, 0), Position::new(2, 0)),
            (Position::new(2, 0), Position::new(0, 0)),
        ] {
            let mut rust = engine("😀\n中\n", "rust");
            rust.execute(Command::Cursor(CursorCommand::SetSelection { start, end }))
                .unwrap();
            edit(
                &mut rust,
                EditCommand::ToggleComment {
                    config: comments(Some(Language::Rust)).unwrap(),
                },
            );
            assert_eq!(rust.editor().get_text(), "// 😀\n// 中\n");
            edit(
                &mut rust,
                EditCommand::ToggleComment {
                    config: comments(Some(Language::Rust)).unwrap(),
                },
            );
            assert_eq!(rust.editor().get_text(), "😀\n中\n");
        }
    }
}
