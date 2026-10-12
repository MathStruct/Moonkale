//! Document-local search and capture expansion. Decorations never mutate the engine.
use crate::{native_model::NativeModel, L};
use dioxus::prelude::*;
use editor_core::{
    search::{SearchMatch, SearchOptions},
    Command, CursorCommand, EditCommand, Position,
};
use moonkale_ext_api::{editor::DocumentRevision, t, Workspace};
use regex::{Regex, RegexBuilder};
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Found {
    range: SearchMatch,
    start_byte: usize,
    end_byte: usize,
}
#[derive(Clone)]
struct Query {
    pattern: String,
    options: SearchOptions,
    regex: Regex,
}
impl PartialEq for Query {
    fn eq(&self, other: &Self) -> bool {
        self.pattern == other.pattern && self.options == other.options
    }
}
impl Query {
    fn new(pattern: &str, options: SearchOptions) -> Result<Self, String> {
        let source = if options.regex {
            pattern.to_owned()
        } else {
            regex::escape(pattern)
        };
        let regex = RegexBuilder::new(&source)
            .case_insensitive(!options.case_sensitive)
            .multi_line(true)
            .build()
            .map_err(|error| error.to_string())?;
        Ok(Self {
            pattern: pattern.into(),
            options,
            regex,
        })
    }
    fn find(&self, text: &str) -> Vec<Found> {
        if self.pattern.is_empty() {
            return vec![];
        }
        let mut byte = 0;
        let mut scalar = 0;
        let mut found = vec![];
        for hit in self.regex.find_iter(text) {
            scalar += text[byte..hit.start()].chars().count();
            let start = scalar;
            scalar += hit.as_str().chars().count();
            byte = hit.end();
            let word = |ch: char| ch == '_' || ch.is_alphanumeric();
            if self.options.whole_word
                && (hit.is_empty()
                    || text[..hit.start()].chars().next_back().is_some_and(word)
                    || text[hit.end()..].chars().next().is_some_and(word))
            {
                continue;
            }
            found.push(Found {
                range: SearchMatch { start, end: scalar },
                start_byte: hit.start(),
                end_byte: hit.end(),
            });
        }
        found
    }
    /// Expand against the original full source, never a sliced match: anchors and
    /// surrounding context retain the same semantics as the search.
    fn replacement(
        &self,
        text: &str,
        selected: &[Found],
        inserted: &str,
    ) -> Option<(usize, usize, String)> {
        let first = selected.first()?;
        let last = selected.last()?;
        let mut output = String::new();
        let mut cursor = first.start_byte;
        if self.options.regex {
            let mut index = 0;
            for captures in self.regex.captures_iter(text) {
                let hit = captures.get(0)?;
                if hit.start() != selected[index].start_byte
                    || hit.end() != selected[index].end_byte
                {
                    continue;
                }
                output.push_str(&text[cursor..hit.start()]);
                captures.expand(inserted, &mut output);
                cursor = hit.end();
                index += 1;
                if index == selected.len() {
                    break;
                }
            }
            if index != selected.len() {
                return None;
            }
        } else {
            for hit in selected {
                output.push_str(&text[cursor..hit.start_byte]);
                output.push_str(inserted);
                cursor = hit.end_byte;
            }
        }
        Some((
            first.range.start,
            last.range.end - first.range.start,
            output,
        ))
    }
}

#[derive(Clone, Default, PartialEq)]
pub(crate) struct SearchHighlights {
    matches: Rc<Vec<Found>>,
    current: Option<SearchMatch>,
    revision: Option<DocumentRevision>,
}
impl SearchHighlights {
    pub fn valid(&self, revision: DocumentRevision) -> bool {
        self.revision == Some(revision)
    }
    /// Search matches are sorted and nonoverlapping, so only viewport candidates
    /// are materialized even for a document with many search results.
    pub(crate) fn visible_ranges(
        &self,
        start: usize,
        end: usize,
    ) -> Vec<(usize, std::ops::Range<usize>, bool)> {
        let first = self.matches.partition_point(|hit| hit.range.end < start);
        self.matches
            .iter()
            .enumerate()
            .skip(first)
            .take_while(|(_, hit)| hit.range.start <= end)
            .filter(|(_, hit)| hit.range.end > start || hit.range.is_empty())
            .map(|(id, hit)| {
                (
                    id,
                    hit.range.start..hit.range.end,
                    self.current == Some(hit.range),
                )
            })
            .collect()
    }
}
#[derive(Clone, PartialEq)]
struct Results {
    matches: Rc<Vec<Found>>,
    revision: DocumentRevision,
    error: Option<String>,
}

#[component]
pub(crate) fn NativeSearch(
    ws: Workspace,
    mut model: Signal<NativeModel>,
    mut first_row: Signal<usize>,
    mut highlights: Signal<SearchHighlights>,
    onchange: Callback<()>,
    replacing: bool,
    onclose: Callback<()>,
) -> Element {
    let mut query = use_signal(String::new);
    let mut inserted = use_signal(String::new);
    let mut options = use_signal(SearchOptions::default);
    let mut current = use_signal(|| None::<SearchMatch>);
    // Cursor movement changes the model signal but not this memo's value, so it
    // does not rerun the full-document search on every arrow key.
    let revision = use_memo(move || model.read().revision);
    let compiled = use_memo(move || Query::new(&query(), options()));
    let matches = use_memo(move || {
        let revision = revision();
        match compiled.read().as_ref() {
            Ok(compiled) => Results {
                revision,
                matches: Rc::new(if compiled.pattern.is_empty() {
                    vec![]
                } else {
                    compiled.find(&model.peek().engine.editor().get_text())
                }),
                error: None,
            },
            Err(error) => Results {
                revision,
                matches: Rc::new(vec![]),
                error: Some(error.clone()),
            },
        }
    });
    use_effect(move || {
        let _ = revision();
        current.set(None);
    });
    use_effect(move || {
        let found = matches.read();
        let active = current().filter(|range| found.matches.iter().any(|hit| hit.range == *range));
        let next = SearchHighlights {
            matches: found.matches.clone(),
            current: active,
            revision: Some(found.revision),
        };
        drop(found);
        if *highlights.peek() != next {
            highlights.set(next);
        }
    });
    use_drop(move || {
        if let Ok(mut value) = highlights.try_write() {
            *value = SearchHighlights::default();
        }
    });
    let navigate = Callback::new(move |backwards: bool| {
        let found = matches.peek();
        if found.matches.is_empty() || found.revision != model.peek().revision {
            return;
        }
        let index =
            current().and_then(|active| found.matches.iter().position(|hit| hit.range == active));
        let index = match index {
            Some(index) if backwards => (index + found.matches.len() - 1) % found.matches.len(),
            Some(index) => (index + 1) % found.matches.len(),
            None if backwards => found.matches.len() - 1,
            None => 0,
        };
        let selected = found.matches[index].range;
        drop(found);
        current.set(Some(selected));
        let row = model.with_mut(|state| {
            let (line, column) = state
                .engine
                .editor()
                .line_index()
                .char_offset_to_position(selected.start);
            let start = Position::new(line, column);
            let (line, column) = state
                .engine
                .editor()
                .line_index()
                .char_offset_to_position(selected.end);
            if selected.is_empty() {
                crate::editor_core_spike::place_caret_at(&mut state.engine, start);
            } else {
                let _ = state
                    .engine
                    .execute(Command::Cursor(CursorCommand::SetSelection {
                        start,
                        end: Position::new(line, column),
                    }));
            }
            state.reveal_cursor();
            state
                .engine
                .logical_position_to_visual(start.line, start.column)
                .map(|(row, _)| row)
                .unwrap_or(0)
        });
        first_row.set(row.saturating_sub(3));
        onchange.call(());
    });
    let replace = Callback::new(move |all: bool| {
        // Recompute from the live engine at action time so pending UI updates
        // cannot apply old ranges after a query or external document change.
        let Ok(compiled) = Query::new(&query.peek(), *options.peek()) else {
            return;
        };
        let text = model.peek().engine.editor().get_text();
        let found = compiled.find(&text);
        let selected = if all {
            found
        } else {
            current()
                .and_then(|active| found.iter().find(|hit| hit.range == active).copied())
                .or_else(|| found.first().copied())
                .into_iter()
                .collect()
        };
        if let Some((start, length, text)) =
            compiled.replacement(&text, &selected, &inserted.peek())
        {
            model.with_mut(|state| {
                let _ = state.engine.execute(Command::Edit(EditCommand::Replace {
                    start,
                    length,
                    text,
                }));
            });
            current.set(None);
            onchange.call(());
        }
    });
    let found = matches.read();
    let total = found.matches.len();
    let index = current()
        .and_then(|active| found.matches.iter().position(|hit| hit.range == active))
        .map(|index| index + 1)
        .unwrap_or(0);
    let error = found.error.clone();
    drop(found);
    rsx! {
        div { class: "mk-editor-bar mk-native-search", role: "search",
            onkeydown: move |event| { if event.key() == Key::Escape { event.prevent_default(); event.stop_propagation(); onclose.call(()); } },
            input { class: "mk-search-query", value: query, aria_label: t!(ws, L, "editor-find"), placeholder: t!(ws, L, "editor-find"), aria_invalid: error.is_some().to_string(),
                onmounted: move |event| { spawn(async move { let _ = event.data().set_focus(true).await; }); },
                oninput: move |event| { query.set(event.value()); current.set(None); },
                onkeydown: move |event| { if event.key() == Key::Enter { event.prevent_default(); navigate.call(event.modifiers().shift()); } },
            }
            label { input { r#type: "checkbox", checked: options().case_sensitive, onchange: move |event| { options.with_mut(|options| options.case_sensitive = event.checked()); current.set(None); } } {t!(ws, L, "editor-match-case")} }
            label { input { r#type: "checkbox", checked: options().whole_word, onchange: move |event| { options.with_mut(|options| options.whole_word = event.checked()); current.set(None); } } {t!(ws, L, "editor-whole-word")} }
            label { input { class: "mk-search-regex", r#type: "checkbox", checked: options().regex, onchange: move |event| { options.with_mut(|options| options.regex = event.checked()); current.set(None); } } {t!(ws, L, "editor-search-regex")} }
            span { class: "mk-search-count", aria_live: "polite", "data-current": "{index}", "data-total": "{total}", {t!(ws, L, "editor-search-count", current = index.to_string(), total = total.to_string())} }
            button { class: "mk-btn", disabled: total == 0, onclick: move |_| navigate.call(true), {t!(ws, L, "editor-find-previous")} }
            button { class: "mk-btn", disabled: total == 0, onclick: move |_| navigate.call(false), {t!(ws, L, "editor-find-next")} }
            if replacing {
                input { class: "mk-search-replacement", value: inserted, aria_label: t!(ws, L, "editor-replace"), placeholder: t!(ws, L, "editor-replace"), title: if options().regex { t!(ws, L, "editor-search-captures-help") } else { String::new() }, oninput: move |event| inserted.set(event.value()) }
                button { class: "mk-btn", disabled: total == 0, onclick: move |_| replace.call(false), {t!(ws, L, "editor-replace")} }
                button { class: "mk-btn", disabled: total == 0, onclick: move |_| replace.call(true), {t!(ws, L, "editor-replace-all")} }
            }
            button { class: "mk-btn", onclick: move |_| onclose.call(()), {t!(ws, L, "editor-cancel")} }
            if let Some(ref error) = error { div { class: "mk-search-error", role: "alert", {t!(ws, L, "editor-search-invalid", error = error.clone())} } }
        }
    }
}

#[cfg(test)]
impl SearchHighlights {
    pub fn contains(&self, offset: usize) -> bool {
        let index = self
            .matches
            .partition_point(|hit| hit.range.start <= offset);
        index
            .checked_sub(1)
            .is_some_and(|index| offset < self.matches[index].range.end)
    }
    pub fn current_contains(&self, offset: usize) -> bool {
        self.current
            .is_some_and(|range| range.start <= offset && offset < range.end)
    }
    pub fn zero_at(&self, offset: usize) -> bool {
        self.matches
            .binary_search_by_key(&offset, |hit| hit.range.start)
            .ok()
            .is_some_and(|index| self.matches[index].range.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn make_query(pattern: &str, regex: bool) -> Query {
        Query::new(
            pattern,
            SearchOptions {
                regex,
                ..Default::default()
            },
        )
        .unwrap()
    }
    #[test]
    fn unicode_literal_replacements_and_word_options() {
        let source = "😀 cat\n猫 cat!";
        let query = make_query("cat", false);
        assert_eq!(
            query.replacement(source, &query.find(source), "$1λ"),
            Some((2, 9, "$1λ\n猫 $1λ".into()))
        );
        assert!(query.replacement(source, &[], "x").is_none());
        let query = Query::new(
            "cat",
            SearchOptions {
                case_sensitive: false,
                whole_word: true,
                regex: false,
            },
        )
        .unwrap();
        assert_eq!(query.find("Cat catch cat 猫cat").len(), 2);
        assert!(make_query("", false).find("abc").is_empty());
    }
    #[test]
    fn captures_expand_in_full_source_with_context_and_unicode() {
        let source = "😀猫=12\n中=34\n";
        let query = make_query(r"^(?<name>\p{L}+)=(\d+)$", true);
        let found = query.find(source);
        assert_eq!(found.len(), 1); // emoji before 猫 prevents a line-start match.
        assert_eq!(
            query.replacement(source, &found, "${name}:$2:$$"),
            Some((6, 4, "中:34:$".into()))
        );
        let query = make_query(r"\b(\p{L}+)=(\d+)", true);
        let found = query.find(source);
        assert_eq!(
            query.replacement(source, &found, "$2-${1}"),
            Some((1, 9, "12-猫\n34-中".into()))
        );
        assert_eq!(
            query.replacement(source, &found[1..], "$1"),
            Some((6, 4, "中".into()))
        );
    }
    #[test]
    fn empty_width_regex_matches_are_finite_and_replaceable() {
        let query = make_query("^", true);
        let source = "😀\n中";
        let found = query.find(source);
        assert_eq!(
            found.iter().map(|hit| hit.range).collect::<Vec<_>>(),
            vec![
                SearchMatch { start: 0, end: 0 },
                SearchMatch { start: 2, end: 2 }
            ]
        );
        assert_eq!(
            query.replacement(source, &found, ">"),
            Some((0, 2, ">😀\n>".into()))
        );
        assert!(Query::new(
            "(",
            SearchOptions {
                regex: true,
                ..Default::default()
            }
        )
        .is_err());
    }
    #[test]
    fn capture_replace_is_one_crlf_preserving_workspace_history_edit() {
        use crate::native_model::delta_batch;
        use moonkale_core::NodeId;
        use moonkale_ext_api::editor::{RevisionedDocument, Utf16Selection};
        let original = "😀 猫=12\r\n中=34\r\n";
        let mut document = RevisionedDocument::new(NodeId::fresh("regex-history"), original.into());
        let base = document.snapshot();
        let mut model = NativeModel::new(base, None);
        let text = model.engine.editor().get_text();
        let query = make_query(r"(\p{L}+)=(\d+)", true);
        let (start, length, text) = query
            .replacement(&text, &query.find(&text), "$2-${1}-$$")
            .unwrap();
        model
            .engine
            .execute(Command::Edit(EditCommand::Replace {
                start,
                length,
                text,
            }))
            .unwrap();
        let delta = model.engine.take_last_text_delta().unwrap();
        let batch = delta_batch(base, &delta, Utf16Selection { anchor: 0, head: 0 }).unwrap();
        let expected = "😀 12-猫-$\r\n34-中-$\r\n";
        assert_eq!(document.apply(&batch).unwrap().text, expected);
        assert_eq!(document.undo().unwrap().unwrap().text, original);
        assert_eq!(document.redo().unwrap().unwrap().text, expected);
    }
    #[test]
    fn highlight_ranges_are_scalar_based_and_handle_zero_width() {
        let query = make_query("😀|$", true);
        let highlights = SearchHighlights {
            matches: Rc::new(query.find("a😀中")),
            current: Some(SearchMatch { start: 1, end: 2 }),
            revision: None,
        };
        assert!(!highlights.contains(0));
        assert!(highlights.contains(1));
        assert!(!highlights.contains(2));
        assert!(highlights.current_contains(1));
        assert!(highlights.zero_at(3));
    }
}
