//! A YAML draft commits one prefix edit through the canonical editor session.
use dioxus::prelude::*;
use moonkale_ext_api::{
    editor::{DocumentRevision, EditBatch, EditorSnapshot, TextChange, Utf16Selection},
    t, Workspace,
};
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FrontMatter {
    pub end: usize,
    pub yaml: Range<usize>,
    pub closing: usize,
}

/// Match Rich's conservative leading-block recognition, without changing bytes.
pub(crate) fn front_matter(text: &str) -> Option<FrontMatter> {
    let opening = if text.starts_with("---\r\n") {
        5
    } else if text.starts_with("---\n") {
        4
    } else {
        return None;
    };
    let mut offset = opening;
    for line in text[opening..].split_inclusive('\n') {
        if line.trim_end() == "---" {
            let inner = &text[opening..offset];
            if !yaml_like(inner) {
                return None;
            }
            let yaml_end = if inner.ends_with("\r\n") {
                offset - 2
            } else if inner.ends_with('\n') {
                offset - 1
            } else {
                offset
            };
            return Some(FrontMatter {
                end: offset + line.len(),
                yaml: opening..yaml_end,
                closing: offset,
            });
        }
        offset += line.len();
    }
    None
}

fn yaml_like(text: &str) -> bool {
    !text.starts_with(['\n', '\r'])
        && text.lines().all(|line| {
            let line = line.trim_end();
            line.is_empty()
                || line.starts_with([' ', '\t', '#'])
                || line.starts_with("- ")
                || line == "-"
                || line.split_once(':').is_some_and(|(key, _)| {
                    !key.is_empty()
                        && key.chars().all(|ch| {
                            ch.is_alphanumeric() || matches!(ch, '_' | '-' | ' ' | '.' | '"' | '\'')
                        })
                })
        })
}

fn summary(yaml: &str) -> String {
    yaml.lines()
        .filter(|line| !line.starts_with([' ', '\t', '#']) && line.contains(':'))
        .take(3)
        .map(|line| {
            let (key, value) = line.split_once(':').unwrap();
            let value = value.trim().trim_matches('"');
            let shortened: String = value.chars().take(40).collect();
            if value.is_empty() {
                key.trim().to_string()
            } else {
                format!(
                    "{}: {}{}",
                    key.trim(),
                    shortened,
                    if value.chars().count() > 40 {
                        "…"
                    } else {
                        ""
                    }
                )
            }
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

#[derive(Clone, PartialEq)]
pub(crate) struct Draft {
    node: moonkale_core::NodeId,
    revision: DocumentRevision,
    initial: String,
    pub yaml: String,
}

impl Draft {
    pub fn new(base: &EditorSnapshot) -> Self {
        let initial = front_matter(&base.text)
            .map(|fm| base.text[fm.yaml].replace("\r\n", "\n"))
            .unwrap_or_default();
        Self {
            node: base.node,
            revision: base.revision,
            yaml: initial.clone(),
            initial,
        }
    }

    /// Existing delimiters/body stay byte-exact; textarea newlines follow the file.
    pub fn batch(&self, base: &EditorSnapshot) -> Result<Option<EditBatch>, &'static str> {
        if self.yaml == self.initial {
            return Ok(None);
        }
        if self.node != base.node || self.revision != base.revision {
            return Err("editor-properties-stale");
        }
        let yaml = self.yaml.replace("\r\n", "\n");
        if !yaml.trim().is_empty()
            && (!yaml_like(&yaml) || yaml.lines().any(|line| line.trim_end() == "---"))
        {
            return Err("editor-properties-invalid");
        }
        let fm = front_matter(&base.text);
        let end = fm.as_ref().map_or(0, |fm| fm.end);
        let newline = if fm
            .as_ref()
            .is_some_and(|fm| base.text[..fm.yaml.start].ends_with("\r\n"))
            || (fm.is_none() && base.text.contains("\r\n"))
        {
            "\r\n"
        } else {
            "\n"
        };
        let inserted = if yaml.trim().is_empty() {
            String::new()
        } else {
            let yaml = if newline == "\r\n" {
                yaml.replace('\n', "\r\n")
            } else {
                yaml
            };
            if let Some(fm) = &fm {
                format!(
                    "{}{}{}{}",
                    &base.text[..fm.yaml.start],
                    yaml,
                    newline,
                    &base.text[fm.closing..fm.end]
                )
            } else {
                format!("---{newline}{yaml}{newline}---{newline}")
            }
        };
        let before = base.text[..end].encode_utf16().count() as u32;
        let after = inserted.encode_utf16().count() as u32;
        let map = |offset: u32| {
            if offset >= before {
                offset - before + after
            } else {
                offset.min(after)
            }
        };
        Ok(Some(EditBatch {
            node: base.node,
            base_revision: base.revision,
            changes: vec![TextChange {
                range: 0..end,
                removed_text: base.text[..end].to_string(),
                inserted_text: inserted,
            }],
            selection: base.selection.map(|selection| Utf16Selection {
                anchor: map(selection.anchor),
                head: map(selection.head),
            }),
        }))
    }
}

#[component]
pub(crate) fn Properties(
    ws: Workspace,
    yaml: String,
    has: bool,
    mut draft: Signal<Option<Draft>>,
    mut composing: Signal<bool>,
    error: Option<String>,
    onopen: Callback<()>,
    onapply: Callback<()>,
    oncancel: Callback<()>,
    onsave: Callback<()>,
) -> Element {
    let open = draft.read().is_some();
    rsx! {
        div { class: "mk-native-properties", onmousedown: move |event| event.stop_propagation(),
            button { class: "mk-properties-head", r#type: "button", aria_expanded: open.to_string(),
                onclick: move |_| if open { oncancel.call(()) } else { onopen.call(()) },
                span { if open { "▾" } else { "▸" } }
                span { {t!(ws, crate::L, "editor-properties")} }
                span { class: "mk-properties-summary", if has { "{summary(&yaml)}" } else { {t!(ws, crate::L, "editor-properties-none")} } }
            }
            if open {
                div { class: "mk-properties-edit",
                    textarea { class: "mk-properties-yaml", aria_label: t!(ws, crate::L, "editor-properties-yaml"), rows: "6", spellcheck: "false",
                        onmounted: move |event| { spawn(async move { let _ = event.data().set_focus(true).await; }); },
                        value: draft.read().as_ref().map(|draft| draft.yaml.clone()).unwrap_or_default(),
                        oninput: move |event| { if let Some(draft) = draft.write().as_mut() { draft.yaml = event.value(); } },
                        onkeydown: move |event| {
                            event.stop_propagation();
                            if composing() { return; }
                            if event.key() == Key::Escape { event.prevent_default(); oncancel.call(()); }
                            else if moonkale_ext_api::keys::primary(&event.modifiers()) && matches!(event.key(), Key::Character(key) if key.eq_ignore_ascii_case("s")) { event.prevent_default(); onsave.call(()); }
                        },
                        oncopy: move |event| event.stop_propagation(), oncut: move |event| event.stop_propagation(), onpaste: move |event| event.stop_propagation(),
                        oncompositionstart: move |event| {event.stop_propagation();composing.set(true);}, oncompositionend: move |event| {event.stop_propagation();composing.set(false);},
                    }
                    if let Some(error) = error { div { class: "mk-editor-error", role: "alert", "{error}" } }
                    button { class: "mk-btn mk-properties-apply", r#type: "button", disabled: composing(), onclick: move |_| onapply.call(()), {t!(ws, crate::L, "editor-properties-apply")} }
                    button { class: "mk-btn mk-properties-cancel", r#type: "button", onclick: move |_| oncancel.call(()), {t!(ws, crate::L, "editor-properties-cancel")} }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moonkale_core::NodeId;
    use moonkale_ext_api::editor::RevisionedDocument;
    fn base(text: &str) -> EditorSnapshot {
        EditorSnapshot {
            node: NodeId::fresh("properties-test"),
            revision: DocumentRevision(0),
            text: text.into(),
            selection: None,
        }
    }

    #[test]
    fn recognition_retains_comments_complex_yaml_and_rejects_prose() {
        let text = "---\r\n# private\r\ntitle: 猫\r\naliases:\r\n  - A\r\n  - B\r\n---  \r\nBody\r\n---\r\n";
        let fm = front_matter(text).unwrap();
        assert_eq!(&text[fm.end..], "Body\r\n---\r\n");
        assert_eq!(
            &text[fm.yaml],
            "# private\r\ntitle: 猫\r\naliases:\r\n  - A\r\n  - B"
        );
        for text in [
            "---\nProse\n---\n",
            "---\n\nkey: value\n---\n",
            "---\ntitle: missing",
            "Before\n---\ntitle: later\n---\n",
        ] {
            assert!(front_matter(text).is_none(), "{text}");
        }
        assert!(front_matter("---\ntitle: Notes\n---").is_some());
        assert_eq!(
            summary("title: \"猫\"\ntags: [a]\naliases:\n  - B\nmore: 2"),
            "title: 猫 · tags: [a] · aliases"
        );
    }

    #[test]
    fn prefix_edits_preserve_body_delimiters_newlines_and_history() {
        for newline in ["\n", "\r\n"] {
            let original = format!("---{newline}# comment{newline}title: 猫{newline}{newline}---  {newline}😀 Body{newline}");
            let mut base = base(&original);
            let position = original.encode_utf16().count() as u32;
            base.selection = Some(Utf16Selection {
                anchor: position,
                head: position,
            });
            let mut draft = Draft::new(&base);
            assert!(draft.batch(&base).unwrap().is_none());
            draft.yaml = draft.yaml.replace("猫", "Two 猫");
            let mut doc = RevisionedDocument::new(base.node, base.text.clone());
            doc.set_selection(base.revision, base.selection.unwrap());
            let changed = doc.apply(&draft.batch(&base).unwrap().unwrap()).unwrap();
            assert_eq!(changed.text, original.replace("猫", "Two 猫"));
            assert_eq!(changed.selection.unwrap().head, position + 4);
            assert_eq!(doc.undo().unwrap().unwrap().text, original);
            assert_eq!(doc.redo().unwrap().unwrap().text, changed.text);
            let changed = doc.snapshot().clone();
            let mut remove = Draft::new(&changed);
            remove.yaml.clear();
            assert_eq!(
                doc.apply(&remove.batch(&changed).unwrap().unwrap())
                    .unwrap()
                    .text,
                format!("😀 Body{newline}")
            );
        }
    }

    #[test]
    fn insertion_staleness_and_delimiter_injection_are_guarded() {
        let base = base("Body\r\n");
        let mut draft = Draft::new(&base);
        draft.yaml = "title: Notes\ntags: [a]".into();
        let batch = draft.batch(&base).unwrap().unwrap();
        let mut doc = RevisionedDocument::new(base.node, base.text.clone());
        assert_eq!(
            doc.apply(&batch).unwrap().text,
            "---\r\ntitle: Notes\r\ntags: [a]\r\n---\r\nBody\r\n"
        );
        assert_eq!(draft.batch(doc.snapshot()), Err("editor-properties-stale"));
        draft.yaml = "title: Notes\n---\nInjected".into();
        assert_eq!(draft.batch(&base), Err("editor-properties-invalid"));
        draft.yaml = "Unstructured prose".into();
        assert_eq!(draft.batch(&base), Err("editor-properties-invalid"));
        draft.yaml = "description: |\n  ---\ntitle: Notes".into();
        let mut literal = RevisionedDocument::new(base.node, base.text.clone());
        let changed = literal
            .apply(&draft.batch(&base).unwrap().unwrap())
            .unwrap();
        let fm = front_matter(&changed.text).unwrap();
        assert_eq!(&changed.text[fm.end..], base.text);
        assert!(changed.text[fm.yaml].contains("  ---"));
    }

    #[test]
    fn unchanged_text_and_eof_delimiters_remain_byte_exact() {
        let base = base("---\r\ntitle: \"A\"\r\n# comment\r\n---  ");
        let mut draft = Draft::new(&base);
        assert!(draft.batch(&base).unwrap().is_none());
        draft.yaml = draft.yaml.replace('A', "猫");
        let mut doc = RevisionedDocument::new(base.node, base.text.clone());
        assert_eq!(
            doc.apply(&draft.batch(&base).unwrap().unwrap())
                .unwrap()
                .text,
            base.text.replace('A', "猫")
        );
        let mut other = base.clone();
        other.node = NodeId::fresh("another-note");
        assert_eq!(draft.batch(&other), Err("editor-properties-stale"));
    }
}
