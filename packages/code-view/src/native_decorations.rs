//! Revision-bound, view-only decorations in editor-core scalar offsets.
//! Providers describe presentation; they cannot edit the document or history.
use crate::{native_search::SearchHighlights, native_wiki};
use moonkale_ext_api::editor::DocumentRevision;
use moonkale_lsp::Diagnostic;
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Provider {
    Search,
    Diagnostics,
    Wiki,
    #[cfg(feature = "layout-fixture")]
    Fixture,
    #[cfg(feature = "layout-fixture")]
    Markdown,
}

#[derive(Clone, PartialEq)]
pub(crate) enum Mark {
    Search { current: bool },
    Diagnostic(Diagnostic),
    Wiki(native_wiki::Mark),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum InlineStyle {
    Emphasis,
    Strong,
    Code,
}
impl InlineStyle {
    #[cfg(feature = "layout-fixture")]
    pub(crate) fn class(self) -> &'static str {
        match self {
            Self::Emphasis => "mk-markdown-emphasis",
            Self::Strong => "mk-markdown-strong",
            Self::Code => "mk-markdown-code",
        }
    }
}

/// Source-owned presentation. The bounded fixture renders replacement and
/// block kinds; production providers currently emit only marks and lines.
#[derive(Clone, PartialEq)]
pub(crate) enum Kind {
    Mark {
        range: Range<usize>,
        mark: Mark,
    },
    Line {
        line: usize,
        mark: Mark,
    },
    #[allow(dead_code)]
    Replace {
        range: Range<usize>,
        /// None hides source; Some supplies the bounded inline widget label.
        widget: Option<String>,
        style: Option<InlineStyle>,
    },
    #[allow(dead_code)]
    BlockWidget {
        anchor: usize,
        widget: String,
    },
}

#[derive(Clone, PartialEq)]
pub(crate) struct Decoration {
    pub provider: Provider,
    /// Identity is stable within the provider's revision-bound result.
    pub id: usize,
    pub kind: Kind,
}

pub(crate) struct Batch {
    pub revision: DocumentRevision,
    pub values: Vec<Decoration>,
}

#[derive(Clone, Copy)]
pub(crate) struct Window {
    pub start: usize,
    pub end: usize,
    pub first_line: usize,
    pub last_line: usize,
}

/// Overlapping styles coexist. Diagnostic tooltip and wiki action each use
/// the first provider result, preserving the prior surface's precedence.
#[derive(Clone, Default, PartialEq)]
pub(crate) struct CellMarks {
    pub search: bool,
    pub current_search: bool,
    pub diagnostic: Option<Diagnostic>,
    pub wiki: Option<native_wiki::Mark>,
}

impl CellMarks {
    pub(crate) fn classes(&self) -> String {
        format!(
            "{} {} {} {}",
            if self.search {
                "mk-native-search-match"
            } else {
                ""
            },
            if self.current_search {
                "mk-native-search-current"
            } else {
                ""
            },
            self.diagnostic
                .as_ref()
                .map(|value| format!("mk-native-diagnostic-{}", value.severity))
                .unwrap_or_default(),
            self.wiki
                .as_ref()
                .map(|value| if value.resolved {
                    "mk-native-wiki-resolved"
                } else {
                    "mk-native-wiki-unresolved"
                })
                .unwrap_or_default()
        )
    }
}

pub(crate) struct Decorations {
    values: Vec<Decoration>,
}

impl Decorations {
    pub(crate) fn compose(revision: DocumentRevision, window: Window, batches: Vec<Batch>) -> Self {
        let mut values: Vec<_> = batches
            .into_iter()
            .filter(|batch| batch.revision == revision)
            .flat_map(|batch| batch.values)
            .filter(|value| match &value.kind {
                Kind::Mark { range, .. } | Kind::Replace { range, .. } => {
                    if range.is_empty() {
                        range.start >= window.start && range.start <= window.end
                    } else {
                        range.start <= window.end && range.end > window.start
                    }
                }
                Kind::Line { line, .. } => *line >= window.first_line && *line <= window.last_line,
                Kind::BlockWidget { anchor, .. } => {
                    *anchor >= window.start && *anchor <= window.end
                }
            })
            .collect();
        values.sort_by_key(|value| (value.provider, value.id));
        Self { values }
    }

    pub(crate) fn cell(&self, offset: usize) -> CellMarks {
        let mut result = CellMarks::default();
        for value in &self.values {
            let Kind::Mark { range, mark } = &value.kind else {
                continue;
            };
            let hit = if range.is_empty() {
                range.start == offset && matches!(mark, Mark::Diagnostic(_))
            } else {
                range.contains(&offset)
            };
            if !hit {
                continue;
            }
            match mark {
                Mark::Search { current } => {
                    result.search = true;
                    result.current_search |= current;
                }
                Mark::Diagnostic(diagnostic) => {
                    if result.diagnostic.is_none() {
                        result.diagnostic = Some(diagnostic.clone());
                    }
                }
                Mark::Wiki(mark) => {
                    if result.wiki.is_none() {
                        result.wiki = Some(mark.clone());
                    }
                }
            }
        }
        result
    }

    #[cfg(feature = "layout-fixture")]
    pub(crate) fn replacements(
        &self,
        run: Range<usize>,
    ) -> Vec<crate::native_presentation::Replacement> {
        let mut accepted: Vec<crate::native_presentation::Replacement> = Vec::new();
        for value in &self.values {
            let Kind::Replace {
                range,
                widget,
                style,
            } = &value.kind
            else {
                continue;
            };
            // Cross-run replacements are deliberately not clipped: clipping
            // would invent a second source boundary or duplicate a widget.
            if range.is_empty()
                || range.start < run.start
                || range.end > run.end
                || accepted.iter().any(|old| {
                    old.start < range.end - run.start && range.start - run.start < old.end
                })
            {
                continue;
            }
            accepted.push(crate::native_presentation::Replacement {
                start: range.start - run.start,
                end: range.end - run.start,
                widget: widget.clone(),
                style: *style,
            });
        }
        accepted.sort_by_key(|value| value.start);
        accepted
    }

    #[cfg(feature = "layout-fixture")]
    pub(crate) fn block(&self, offset: usize) -> Option<&str> {
        self.values.iter().find_map(|value| match &value.kind {
            Kind::BlockWidget { anchor, widget } if *anchor == offset => Some(widget.as_str()),
            _ => None,
        })
    }

    pub(crate) fn search_point(&self, offset: usize) -> Option<bool> {
        let mut found = None;
        for value in &self.values {
            if let Kind::Mark {
                range,
                mark: Mark::Search { current },
            } = &value.kind
            {
                if range.is_empty() && range.start == offset {
                    found = Some(found.unwrap_or(false) || *current);
                }
            }
        }
        found
    }

    pub(crate) fn line_diagnostics(&self, line: usize) -> impl Iterator<Item = &Diagnostic> {
        self.values
            .iter()
            .filter_map(move |value| match &value.kind {
                Kind::Line {
                    line: anchor,
                    mark: Mark::Diagnostic(diagnostic),
                } if *anchor == line => Some(diagnostic),
                _ => None,
            })
    }
}

pub(crate) fn search(
    revision: DocumentRevision,
    window: Window,
    highlights: &SearchHighlights,
) -> Batch {
    let values = if highlights.valid(revision) {
        highlights
            .visible_ranges(window.start, window.end)
            .into_iter()
            .map(|(id, range, current)| Decoration {
                provider: Provider::Search,
                id,
                kind: Kind::Mark {
                    range,
                    mark: Mark::Search { current },
                },
            })
            .collect()
    } else {
        Vec::new()
    };
    Batch { revision, values }
}

pub(crate) fn diagnostics(
    revision: DocumentRevision,
    window: Window,
    values: &[Diagnostic],
    mut offset: impl FnMut(u32, u32) -> usize,
) -> Batch {
    let mut decorations = Vec::new();
    for (id, diagnostic) in values.iter().enumerate().filter(|(_, value)| {
        value.line as usize <= window.last_line
            && value.end_line.max(value.line) as usize >= window.first_line
    }) {
        let start = offset(diagnostic.line, diagnostic.col);
        let end = offset(diagnostic.end_line, diagnostic.end_col).max(start);
        decorations.push(Decoration {
            provider: Provider::Diagnostics,
            id: id * 2,
            kind: Kind::Mark {
                range: start..end,
                mark: Mark::Diagnostic(diagnostic.clone()),
            },
        });
        decorations.push(Decoration {
            provider: Provider::Diagnostics,
            id: id * 2 + 1,
            kind: Kind::Line {
                line: diagnostic.line as usize,
                mark: Mark::Diagnostic(diagnostic.clone()),
            },
        });
    }
    Batch {
        revision,
        values: decorations,
    }
}

pub(crate) fn wiki(
    revision: DocumentRevision,
    window: Window,
    values: &[native_wiki::Mark],
) -> Batch {
    Batch {
        revision,
        values: values
            .iter()
            .enumerate()
            .filter(|(_, mark)| mark.start <= window.end && mark.end > window.start)
            .map(|(id, mark)| Decoration {
                provider: Provider::Wiki,
                id,
                kind: Kind::Mark {
                    range: mark.start..mark.end,
                    mark: Mark::Wiki(mark.clone()),
                },
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_marks_coexist_and_stale_batches_are_rejected() {
        let revision = DocumentRevision(2);
        let window = Window {
            start: 5,
            end: 10,
            first_line: 0,
            last_line: 1,
        };
        let mark = native_wiki::Mark {
            start: 2,
            end: 8,
            target: "Note".into(),
            resolved: true,
        };
        let current = Batch {
            revision,
            values: vec![Decoration {
                provider: Provider::Search,
                id: 0,
                kind: Kind::Mark {
                    range: 6..9,
                    mark: Mark::Search { current: true },
                },
            }],
        };
        let stale = Batch {
            revision: DocumentRevision(1),
            values: vec![Decoration {
                provider: Provider::Search,
                id: 0,
                kind: Kind::Mark {
                    range: 0..10,
                    mark: Mark::Search { current: false },
                },
            }],
        };
        let result = Decorations::compose(
            revision,
            window,
            vec![current, stale, wiki(revision, window, &[mark])],
        );
        assert!(result.cell(6).search && result.cell(6).current_search);
        assert_eq!(result.cell(6).wiki.unwrap().target, "Note");
        assert!(!result.cell(5).search);
        assert!(result.cell(8).wiki.is_none());
        assert_eq!(result.values.len(), 2);
    }

    #[test]
    fn zero_width_search_is_a_point_and_window_end_is_included() {
        let revision = DocumentRevision(1);
        let result = Decorations::compose(
            revision,
            Window {
                start: 4,
                end: 8,
                first_line: 1,
                last_line: 1,
            },
            vec![Batch {
                revision,
                values: vec![
                    Decoration {
                        provider: Provider::Search,
                        id: 1,
                        kind: Kind::Mark {
                            range: 8..8,
                            mark: Mark::Search { current: true },
                        },
                    },
                    Decoration {
                        provider: Provider::Search,
                        id: 0,
                        kind: Kind::Mark {
                            range: 9..10,
                            mark: Mark::Search { current: false },
                        },
                    },
                ],
            }],
        );
        assert_eq!(result.search_point(8), Some(true));
        assert!(!result.cell(8).search);
        assert_eq!(result.values.len(), 1);
    }

    #[test]
    fn diagnostic_points_gutter_and_overlap_precedence() {
        let revision = DocumentRevision(3);
        let first = Diagnostic {
            raw: None,
            line: 0,
            col: 2,
            end_line: 0,
            end_col: 2,
            severity: "error",
            message: "first".into(),
        };
        let second = Diagnostic {
            raw: None,
            message: "second".into(),
            ..first.clone()
        };
        // The position after the emoji at UTF-16 column 2 is scalar offset 1. The provider's
        // adapter performs the conversion once, before composition.
        let text = "😀x\n";
        let batch = diagnostics(
            revision,
            Window {
                start: 0,
                end: 2,
                first_line: 0,
                last_line: 0,
            },
            &[first, second],
            |line, column| crate::native_diagnostics::scalar_column(text, line, column),
        );
        let result = Decorations::compose(
            revision,
            Window {
                start: 0,
                end: 2,
                first_line: 0,
                last_line: 0,
            },
            vec![batch],
        );
        assert_eq!(result.cell(1).diagnostic.unwrap().message, "first");
        assert!(result.cell(0).diagnostic.is_none());
        assert_eq!(result.line_diagnostics(0).count(), 2);
        assert_eq!(result.line_diagnostics(1).count(), 0);
    }

    #[test]
    fn all_decoration_kinds_clip_to_the_source_window() {
        let revision = DocumentRevision(1);
        let window = Window {
            start: 5,
            end: 10,
            first_line: 2,
            last_line: 3,
        };
        let batch = Batch {
            revision,
            values: vec![
                Decoration {
                    provider: Provider::Wiki,
                    id: 3,
                    kind: Kind::BlockWidget {
                        anchor: 10,
                        widget: "image".into(),
                    },
                },
                Decoration {
                    provider: Provider::Wiki,
                    id: 2,
                    kind: Kind::Replace {
                        range: 2..7,
                        widget: None,
                        style: None,
                    },
                },
                Decoration {
                    provider: Provider::Wiki,
                    id: 1,
                    kind: Kind::Line {
                        line: 2,
                        mark: Mark::Search { current: false },
                    },
                },
                Decoration {
                    provider: Provider::Wiki,
                    id: 4,
                    kind: Kind::BlockWidget {
                        anchor: 11,
                        widget: "outside".into(),
                    },
                },
                Decoration {
                    provider: Provider::Wiki,
                    id: 5,
                    kind: Kind::Line {
                        line: 4,
                        mark: Mark::Search { current: false },
                    },
                },
            ],
        };
        let result = Decorations::compose(revision, window, vec![batch]);
        assert_eq!(
            result
                .values
                .iter()
                .map(|value| value.id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        // Reserved replacement/widget representations do not alter mark state.
        assert!(!result.cell(6).search);
    }
}
