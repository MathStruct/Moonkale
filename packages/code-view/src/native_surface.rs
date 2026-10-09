//! Virtualized Dioxus input/rendering surface for the Rust-owned editor model.
#[cfg(feature = "native-desktop")]
use crate::editor_core_spike::with_native_clipboard;
use crate::editor_core_spike::{contains, move_cursor, place_caret_at, write_clipboard_event};
use crate::native_decorations::{self, CellMarks, Decorations, Window};
use crate::native_layout::{RowHeights, UniformLayout};
use crate::native_model::NativeModel;
use crate::native_search::SearchHighlights;
use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;
use editor_core::{Command, CursorCommand, EditCommand, EditorStateManager, Position};
use std::{cell::RefCell, rc::Rc};

#[component]
pub(crate) fn RustEditorSurface(
    completion_items: Vec<moonkale_lsp::CompletionItem>,
    mut completion_selected: Signal<usize>,
    oncompletion: Callback<()>,
    oncompletion_accept: Callback<usize>,
    oncompletion_close: Callback<()>,
    completion_label: String,
    wiki_enabled: bool,
    onactions: Callback<()>,
    onreferences: Callback<()>,
    oncancel_tools: Callback<()>,
    ondismiss_tools: Callback<()>,
    ondismiss_rename: Callback<()>,
    onrename: Callback<()>,
    oncancel_rename: Callback<()>,
    ondefinition: Callback<()>,
    oncancel_definition: Callback<()>,
    model: Signal<NativeModel>,
    highlights: Signal<SearchHighlights>,
    diagnostics: Memo<Vec<moonkale_lsp::Diagnostic>>,
    presence: Vec<moonkale_ext_api::presence::Member>,
    wiki_marks: Vec<crate::native_wiki::Mark>,
    onwiki: Callback<String>,
    mut hover_target: Signal<Option<(usize, usize)>>,
    hover_text: Option<String>,
    hover_label: String,
    mut first_row: Signal<usize>,
    mut first_column: Signal<usize>,
    mut scroll_displacement: Signal<f64>,
    source_anchor: Signal<Option<crate::native_layout::ViewAnchor>>,
    run_heights: Signal<std::collections::BTreeMap<usize, crate::native_layout::CachedRunHeight>>,
    onchange: Callback<()>,
    onprepare: Callback<()>,
    onhistory: Callback<bool>,
    focus_request: Signal<u64>,
    onfold: Callback<(usize, bool)>,
    fold_label: String,
    unfold_label: String,
    label: String,
    theme_class: String,
) -> Element {
    let completion_id = use_hook(|| {
        format!(
            "mk-native-completions-{}",
            dioxus::core::current_scope_id().0
        )
    });
    let completion_nodes = use_hook(|| {
        Rc::new(RefCell::new(std::collections::HashMap::<
            usize,
            Rc<MountedData>,
        >::new()))
    });
    let completion_keys = completion_nodes.clone();
    // Component props can replace root-owned signal handles without remounting.
    // Periodic geometry callbacks must resolve the current handles at invocation.
    let current_view = use_memo(use_reactive!(|model, first_row, first_column| (
        model,
        first_row,
        first_column
    )));
    let mut cell_pixels = use_signal(|| 8.4f64);
    let mut row_pixels = use_signal(|| 22.0f64);
    #[cfg(feature = "layout-fixture")]
    let current_runs = use_memo(use_reactive!(|run_heights| run_heights));
    #[cfg(feature = "layout-fixture")]
    let measured_runs = use_hook(|| {
        Rc::new(RefCell::new(std::collections::BTreeMap::<
            usize,
            crate::native_proportional_run::RunHeight,
        >::new()))
    });
    #[cfg(feature = "layout-fixture")]
    let navigation_runs = measured_runs.clone();
    #[cfg(feature = "layout-fixture")]
    let mut vertical_intent = use_signal(|| None::<crate::native_proportional_run::VerticalIntent>);
    #[cfg(feature = "layout-fixture")]
    let mut caret_affinity =
        use_signal(|| None::<crate::native_proportional_navigation::CaretAffinity>);
    #[cfg(feature = "layout-fixture")]
    let mut font_epoch = use_signal(|| 0u64);
    #[cfg(feature = "layout-fixture")]
    let mut font_loading = use_signal(|| false);
    let viewport_id =
        use_hook(|| format!("mk-native-viewport-{}", dioxus::core::current_scope_id().0));
    let mut viewport_width_pixels = use_signal(|| 0.0f64);
    let mut viewport_pixels = use_signal(|| 880.0f64);
    #[cfg(feature = "layout-fixture")]
    let fixture = use_hook(try_consume_context::<Signal<crate::LayoutFixture>>);
    #[cfg(not(feature = "layout-fixture"))]
    let fixture: Option<Signal<()>> = None;
    #[cfg(feature = "layout-fixture")]
    let current_scroll = use_memo(use_reactive!(|scroll_displacement| scroll_displacement));
    #[cfg(feature = "layout-fixture")]
    let mut measured_block = use_signal(|| {
        None::<(
            crate::LayoutFixture,
            moonkale_ext_api::editor::DocumentRevision,
            f64,
        )>
    });
    let mut composing = use_signal(|| false);
    let heights = use_memo(use_reactive!(|model, run_heights| {
        #[cfg(not(feature = "layout-fixture"))]
        let _ = run_heights;
        let state = model.read();
        let rows = state.engine.get_viewport_state().total_visual_lines;
        #[cfg(feature = "layout-fixture")]
        let presentation = presentation_state(fixture, model);
        #[cfg(feature = "layout-fixture")]
        let mapped = (presentation != 0 && proportional_enabled(fixture, model, 0, false))
            .then(|| {
                crate::native_presentation::mapped_runs(
                    presentation_mode(fixture, model),
                    &state.engine,
                    state.revision,
                    presentation,
                )
            })
            .flatten();
        #[cfg(feature = "layout-fixture")]
        let mut overrides = fixture
            .map(|value| {
                let config = value();
                state
                    .engine
                    .editor()
                    .logical_position_to_visual(config.line, 0)
                    .filter(|(row, _)| state.engine.visual_to_logical_line(*row).0 == config.line)
                    .map(|(row, _)| {
                        vec![(
                            row,
                            row_pixels()
                                + config.extra_height.max(0.0)
                                + measured_block()
                                    .filter(|(key, revision, _)| {
                                        *key == config && *revision == state.revision
                                    })
                                    .map(|(_, _, height)| height)
                                    .unwrap_or(config.block_height.max(0.0)),
                        )]
                    })
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        #[cfg(feature = "layout-fixture")]
        if let Some(line) = fixture.and_then(|value| value().proportional_line) {
            let index = state.engine.editor().line_index();
            let length = index
                .get_line(line)
                .map(|value| value.char_count)
                .unwrap_or(usize::MAX);
            if state.preferences.wrap && length <= 4096 {
                if let Some((first, _)) = state
                    .engine
                    .logical_position_to_visual(line, 0)
                    .filter(|(row, _)| state.engine.visual_to_logical_line(*row).0 == line)
                {
                    let last_line = if fixture.is_some_and(|value| {
                        (value().presentation || markdown_enabled(fixture, model))
                            && value().proportional_line == Some(0)
                    }) {
                        crate::native_presentation::last_line(
                            presentation_mode(fixture, model),
                            &state.engine,
                            state.revision,
                        )
                    } else {
                        line
                    };
                    let last_length = index
                        .get_line(last_line)
                        .map(|value| value.char_count)
                        .unwrap_or(length);
                    let last = state
                        .engine
                        .logical_position_to_visual(last_line, last_length)
                        .map(|(row, _)| row)
                        .unwrap_or(first);
                    for row in first..=last {
                        let cache = run_heights.read();
                        let measured = cache.values().find(|entry| {
                            entry.revision == state.revision
                                && entry.row == row
                                && entry.presentation == presentation
                                && entry.viewport_width == viewport_width_pixels()
                                && mapped
                                    .as_ref()
                                    .and_then(|runs| runs.lines.get(row))
                                    .map(|run| run.char_offset_start == entry.start)
                                    .unwrap_or_else(|| {
                                        let (source_line, column) =
                                            index.char_offset_to_position(entry.start);
                                        source_line >= line
                                            && source_line <= last_line
                                            && state
                                                .engine
                                                .logical_position_to_visual(source_line, column)
                                                .is_some_and(|(mapped, _)| mapped == row)
                                    })
                        });
                        let height = measured.map(|entry| entry.height).unwrap_or(32.0);
                        if let Some(value) = overrides.iter_mut().find(|(index, _)| *index == row) {
                            value.1 += height - row_pixels();
                        } else {
                            overrides.push((row, height));
                        }
                    }
                }
            }
        }
        #[cfg(feature = "layout-fixture")]
        if let Some(mapped) = mapped {
            for row in mapped.collapsed {
                overrides.retain(|(index, _)| *index != row);
                overrides.push((row, 0.0));
            }
        }
        #[cfg(not(feature = "layout-fixture"))]
        let overrides = Vec::new();
        RowHeights::new(rows, row_pixels(), &overrides)
    }));
    // Preserve source position only across view changes. Text revisions and
    // explicit row changes establish a new anchor instead.
    use_effect(use_reactive!(|model, first_row, source_anchor| {
        let mut first_row = first_row;
        let mut source_anchor = source_anchor;
        let state = model.read();
        let row = first_row();
        let previous = *source_anchor.peek();
        let next_row = previous
            .filter(|anchor| anchor.row == row && anchor.revision == state.revision)
            .and_then(|anchor| {
                state
                    .engine
                    .logical_position_to_visual(anchor.source.line, anchor.source.column)
            })
            .map(|(row, _)| row)
            .unwrap_or(row);
        let source = previous
            .filter(|anchor| anchor.row == row && anchor.revision == state.revision)
            .map(|anchor| anchor.source)
            .or_else(|| state.engine.visual_position_to_logical(next_row, 0))
            .unwrap_or(Position::new(0, 0));
        let anchor = crate::native_layout::ViewAnchor {
            row: next_row,
            source,
            revision: state.revision,
        };
        if previous != Some(anchor) {
            source_anchor.set(Some(anchor));
        }
        if next_row != row {
            first_row.set(next_row);
        }
    }));
    let mut viewport_ready = use_signal(|| false);

    let mut dragging = use_signal(|| false);
    let mut hovered_position = hover_target;
    let mut composition_preview = use_signal(String::new);
    let mut input_epoch = use_signal(|| 0u64);
    let mut refocus_input = use_signal(|| false);
    let input_mounted = use_hook(|| Rc::new(RefCell::new(None::<Rc<MountedData>>)));
    let viewport_mounted = use_hook(|| Rc::new(RefCell::new(None::<Rc<MountedData>>)));
    let probe_mounted = use_hook(|| Rc::new(RefCell::new(None::<Rc<MountedData>>)));
    #[cfg(feature = "layout-fixture")]
    let block_mounted = use_hook(|| Rc::new(RefCell::new(None::<Rc<MountedData>>)));
    #[cfg(feature = "layout-fixture")]
    let block_measure = {
        let mounted = block_mounted.clone();
        let requests = use_hook(|| Rc::new(std::cell::Cell::new(0u64)));
        Callback::new(move |_: ()| {
            let Some(element) = mounted.borrow().clone() else {
                return;
            };
            let Some(config) = fixture.map(|value| *value.peek()) else {
                return;
            };
            let revision = model.peek().revision;
            let width = model.peek().engine.editor().viewport_width();
            let generation = heights.peek().clone();
            let request = requests.get().wrapping_add(1);
            requests.set(request);
            let requests = requests.clone();
            let mounted = mounted.clone();
            spawn(async move {
                let Ok(rect) = element.get_client_rect().await else {
                    return;
                };
                if config.measurement_delay_ms > 0 {
                    futures_timer::Delay::new(std::time::Duration::from_millis(
                        config.measurement_delay_ms,
                    ))
                    .await;
                }
                let Ok(state) = model.try_peek() else {
                    return;
                };
                let Ok(current_heights) = heights.try_peek() else {
                    return;
                };
                let Ok(current) = measured_block.try_peek() else {
                    return;
                };
                let Some(value) = fixture else {
                    return;
                };
                let Ok(key) = value.try_peek() else {
                    return;
                };
                if *current_heights != generation
                    || requests.get() != request
                    || *key != config
                    || state.revision != revision
                    || state.engine.editor().viewport_width() != width
                    || !mounted
                        .borrow()
                        .as_ref()
                        .is_some_and(|node| Rc::ptr_eq(node, &element))
                    || !rect.size.height.is_finite()
                    || rect.size.height <= 0.0
                {
                    return;
                }
                let next = Some((config, revision, rect.size.height));
                let changed = *current != next;
                drop(current);
                drop(current_heights);
                drop(state);
                if changed {
                    measured_block.set(next);
                }
            });
        })
    };
    #[cfg(feature = "layout-fixture")]
    use_effect(move || {
        if let Some(value) = fixture {
            let _ = value();
        }
        let _ = heights.read();
        let _ = model.read().revision;
        let _ = model.read().engine.editor().viewport_width();
        block_measure.call(());
    });
    let measure = {
        let viewport = viewport_mounted.clone();
        let probe = probe_mounted.clone();
        Callback::new(move |_: ()| {
            let Ok(view) = current_view.try_peek() else {
                return;
            };
            let (model, first_row, first_column) = *view;
            drop(view);
            let Some(element) = viewport.borrow().clone() else {
                return;
            };
            let probe = probe.borrow().clone();
            spawn(async move {
                let Ok(rect) = element.get_client_rect().await else {
                    return;
                };
                // Closing a document releases its root model before pending
                // browser geometry promises necessarily finish.
                if model.try_peek().is_err() {
                    return;
                }
                if rect.size.width <= 0.0 || rect.size.height <= 0.0 {
                    return;
                }
                if let Some(probe) = probe {
                    if let Ok(size) = probe.get_client_rect().await {
                        if model.try_peek().is_err()
                            || cell_pixels.try_peek().is_err()
                            || row_pixels.try_peek().is_err()
                        {
                            return;
                        }
                        if size.size.width > 0.0 && size.size.height > 0.0 {
                            if *cell_pixels.peek() != size.size.width {
                                cell_pixels.set(size.size.width);
                            }
                            if *row_pixels.peek() != size.size.height {
                                row_pixels.set(size.size.height);
                            }
                        }
                    }
                }
                if model.try_peek().is_err()
                    || viewport_width_pixels.try_peek().is_err()
                    || viewport_pixels.try_peek().is_err()
                    || heights.try_peek().is_err()
                {
                    return;
                }
                let layout = UniformLayout::new(row_pixels(), cell_pixels());
                if *viewport_width_pixels.peek() != rect.size.width {
                    viewport_width_pixels.set(rect.size.width);
                }
                if *viewport_pixels.peek() != rect.size.height {
                    viewport_pixels.set(rect.size.height);
                }
                let width = layout
                    .column_at_x(rect.size.width)
                    .saturating_sub(9)
                    .max(10);
                if model.peek().engine.editor().viewport_width() != width {
                    let _ = mutate(model, onchange, |editor| {
                        editor.execute(Command::View(editor_core::ViewCommand::SetViewportWidth {
                            width,
                        }))
                    });
                }
                if !*viewport_ready.peek() {
                    let initial_top = heights.read().row_top(first_row()) + scroll_displacement();
                    let _ = element
                        .scroll(
                            dioxus::html::geometry::PixelsVector2D::new(
                                UniformLayout::new(row_pixels(), cell_pixels())
                                    .column_x(first_column()),
                                initial_top,
                            ),
                            ScrollBehavior::Instant,
                        )
                        .await;
                    if viewport_ready.try_peek().is_ok() {
                        viewport_ready.set(true);
                    }
                }
            });
        })
    };
    let focus_on_request = input_mounted.clone();
    use_effect(move || {
        if focus_request() == 0 {
            return;
        }
        if let Some(element) = focus_on_request.borrow().as_ref().cloned() {
            spawn(async move {
                let _ = element.set_focus(true).await;
            });
        }
    });
    let mount_on_mount_input = input_mounted.clone();
    let viewport_on_mount = viewport_mounted.clone();
    let focus_input_on_down = input_mounted.clone();

    // Search/reveal can request a row beyond the scrollable range of a short
    // document. The browser clamps scrollTop without necessarily firing scroll;
    // keep the virtual window aligned too. Wait for measurement on remount so
    // the initial row estimate cannot discard a retained large-file viewport.
    use_effect(use_reactive!(|first_row, scroll_displacement| {
        let mut first_row = first_row;
        let mut scroll_displacement = scroll_displacement;
        if !viewport_ready() {
            return;
        }
        let limit = heights.read().max_start(viewport_pixels());
        if first_row() > limit {
            scroll_displacement.set(0.0);
            first_row.set(limit);
        }
    }));
    let scroll_on_change = viewport_mounted.clone();
    use_effect(use_reactive!(
        |first_row, first_column, scroll_displacement| {
            let top = heights.read().row_top(first_row()) + scroll_displacement();
            let left = UniformLayout::new(row_pixels(), cell_pixels()).column_x(first_column());
            if let Some(element) = scroll_on_change.borrow().as_ref().cloned() {
                spawn(async move {
                    let _ = element
                        .scroll(
                            dioxus::html::geometry::PixelsVector2D::new(left, top),
                            ScrollBehavior::Instant,
                        )
                        .await;
                });
            }
        }
    ));

    use_effect(use_reactive!(|model, first_column| {
        let mut first_column = first_column;
        let state = model.read();
        let cursor = state.engine.get_cursor_state();
        if state.preferences.wrap {
            return;
        }
        let Some((_, x)) = state
            .engine
            .logical_position_to_visual(cursor.position.line, cursor.position.column)
        else {
            return;
        };
        let width = state.engine.editor().viewport_width().max(1);
        let start = *first_column.peek();
        let next = UniformLayout::reveal_start(start, width, x);
        drop(state);
        if start != next {
            first_column.set(next);
        }
    }));
    let (grid, cursor, selected_text, tags) = model.with(|state| {
        let editor = &state.engine;
        #[allow(unused_mut)]
        let mut grid = if state.preferences.wrap {
            editor.get_viewport_content_styled(
                first_row(),
                heights.read().visible_rows(first_row(), viewport_pixels()) + 3,
            )
        } else {
            editor.get_viewport_content_window(
                first_row(),
                heights.read().visible_rows(first_row(), viewport_pixels()) + 3,
                first_column().saturating_sub(2),
                editor.editor().viewport_width() + 6,
            )
        };
        #[cfg(feature = "layout-fixture")]
        if state.preferences.wrap
            && presentation_state(fixture, model) != 0
            && proportional_enabled(fixture, model, 0, false)
        {
            if let Some(mapped) = crate::native_presentation::mapped_runs(
                presentation_mode(fixture, model),
                editor,
                state.revision,
                presentation_state(fixture, model),
            ) {
                for (index, line) in grid.lines.iter_mut().enumerate() {
                    if let Some(value) = mapped.lines.get(first_row() + index) {
                        *line = value.clone();
                    }
                }
            }
        }

        let cursor = editor.get_cursor_state();
        let selected_text = cursor
            .selection
            .as_ref()
            .map(|selection| {
                let index = editor.editor().line_index();
                let a = index.position_to_char_offset(selection.start.line, selection.start.column);
                let b = index.position_to_char_offset(selection.end.line, selection.end.column);
                editor.editor().text_range(a.min(b), a.abs_diff(b))
            })
            .unwrap_or_default();
        let tags: Vec<Vec<String>> = grid
            .lines
            .iter()
            .map(|line| {
                let mut byte = editor
                    .editor()
                    .line_index()
                    .char_offset_to_byte_offset(line.char_offset_start);
                line.cells
                    .iter()
                    .map(|cell| {
                        let tag = state
                            .highlight
                            .as_ref()
                            .and_then(|buffer| {
                                let spans = buffer.spans();
                                let i = spans.partition_point(|span| span.end() as usize <= byte);
                                spans
                                    .get(i)
                                    .filter(|span| span.start() as usize <= byte)
                                    .map(|span| format!("a-{}", span.tag()))
                            })
                            .unwrap_or_default();
                        byte += cell.ch.len_utf8();
                        tag
                    })
                    .collect()
            })
            .collect();
        (grid, cursor, selected_text, tags)
    });
    let layout = UniformLayout::new(row_pixels(), cell_pixels());

    let wrap_on = model.read().preferences.wrap;
    let canvas_width = if wrap_on {
        "100%".to_string()
    } else {
        format!("{}ch", model.read().max_columns + 18)
    };
    let folds = model.with(|state| state.engine.get_folding_state().regions);
    let window = Window {
        start: grid
            .lines
            .first()
            .map(|line| line.char_offset_start)
            .unwrap_or(0),
        end: grid
            .lines
            .last()
            .map(|line| line.char_offset_end)
            .unwrap_or(0),
        first_line: grid
            .lines
            .first()
            .map(|line| line.logical_line_index)
            .unwrap_or(0),
        last_line: grid
            .lines
            .last()
            .map(|line| line.logical_line_index)
            .unwrap_or(0),
    };
    let decorations = model.with(|state| {
        let values = diagnostics.read();
        let text = if values.is_empty() {
            String::new()
        } else {
            state.engine.editor().get_text()
        };
        #[allow(unused_mut)]
        let mut batches = vec![
            native_decorations::search(state.revision, window, &highlights.read()),
            native_decorations::diagnostics(state.revision, window, &values, |line, column| {
                state.engine.editor().line_index().position_to_char_offset(
                    line as usize,
                    crate::native_diagnostics::scalar_column(&text, line, column),
                )
            }),
            native_decorations::wiki(state.revision, window, &wiki_marks),
        ];
        #[cfg(feature = "layout-fixture")]
        if fixture.is_some_and(|value| value().presentation) || markdown_enabled(fixture, model) {
            let length = crate::native_presentation::source_length(&state.engine);
            batches.push(crate::native_presentation::provider(
                presentation_mode(fixture, model),
                state.revision,
                &state.engine.editor().text_range(0, length),
            ));
        }
        Decorations::compose(state.revision, window, batches)
    });
    #[cfg(feature = "layout-fixture")]
    let show_presentation = presentation_state(fixture, model);
    #[cfg(feature = "layout-fixture")]
    let presentation_values = model.with(|state| {
        crate::native_presentation::replacements(
            presentation_mode(fixture, model),
            &state.engine,
            state.revision,
        )
    });
    let bracket_pair = model.with(|state| state.structure.at_caret(cursor.offset));
    let selection = cursor.selection.clone();
    let hovered_position_text = hovered_position()
        .map(|(line, column)| format!("{line}:{column}"))
        .unwrap_or_default();
    let copy_selected_text = selected_text.clone();
    let cut_selected_text = selected_text.clone();
    #[cfg(feature = "native-desktop")]
    let keyboard_selected_text = selected_text.clone();
    let preedit = composition_preview();
    let sink_label = label.clone();

    rsx! {
        div {
            class: "mk-native-surface {theme_class}",
            "data-engine": "editor-core",
            "data-cursor-offset": "{cursor.offset}",

            "data-composing": "{composing}",
            "data-preedit": "{preedit}",
            "data-wrap": wrap_on.to_string(),
            "data-first-column": first_column().to_string(),
            "data-first-row": "{first_row}",
            "data-hover-position": "{hovered_position_text}",
            tabindex: 0,
            onmouseleave: move |_| hovered_position.set(None),
            onmousedown: move |_| {
                #[cfg(feature = "layout-fixture")]
                if !font_loading() { vertical_intent.set(None); caret_affinity.set(None); }
                hovered_position.set(None);
                if let Some(element) = focus_input_on_down.borrow().as_ref().cloned() {
                    spawn(async move {
                        let _ = element.set_focus(true).await;
                    });
                }
            },
            oncopy: move |event| {
                if copy_selected_text.is_empty()
                    || !write_clipboard_event(&event, &copy_selected_text)
                {
                    return;
                }
                event.prevent_default();
            },
            oncut: move |event| {
                if cut_selected_text.is_empty()
                    || !write_clipboard_event(&event, &cut_selected_text)
                {
                    return;
                }
                event.prevent_default();
                let _ = mutate(
                    model,
                    onchange,
                    |editor| editor.execute(Command::Edit(EditCommand::Backspace)),
                );
            },
            onkeydown: move |event| {
                onprepare.call(());
                if composing() {
                    return;
                }
                if event.key() == Key::Escape { oncompletion_close.call(()); oncancel_definition.call(()); oncancel_rename.call(()); oncancel_tools.call(()); }
                if event.key() == Key::Escape && hovered_position().is_some() {
                    hovered_position.set(None);
                    event.prevent_default();
                    event.stop_propagation();
                    return;
                }
                hovered_position.set(None);
                let key = event.key();
                if key == Key::F2 && event.modifiers().is_empty() { event.prevent_default(); event.stop_propagation(); oncompletion_close.call(()); oncancel_definition.call(()); ondismiss_tools.call(()); onrename.call(()); return; }
                if key == Key::F12 && event.modifiers().is_empty() { event.prevent_default(); event.stop_propagation(); oncompletion_close.call(()); ondismiss_tools.call(()); ondismiss_rename.call(()); ondefinition.call(()); return; }
                let modifiers = event.modifiers();
                let primary = moonkale_ext_api::keys::primary(&modifiers);
                let shift = modifiers.contains(Modifiers::SHIFT);
                if (primary && !shift && !modifiers.contains(Modifiers::ALT) && key == Key::Character(".".into())) || (key == Key::F12 && modifiers == Modifiers::SHIFT) {
                    event.prevent_default(); event.stop_propagation(); oncompletion_close.call(()); oncancel_definition.call(()); ondismiss_rename.call(());
                    if key == Key::F12 { onreferences.call(()); } else { onactions.call(()); }
                    return;
                }
                if (primary || modifiers.contains(Modifiers::CONTROL)) && key == Key::Character(" ".into()) {
                    event.prevent_default(); event.stop_propagation(); oncompletion.call(()); return;
                }
                if !completion_items.is_empty() {
                    match key {
                        Key::ArrowDown | Key::ArrowUp => {
                            let length = completion_items.len();
                            let selected = completion_selected().min(length - 1);
                            let next = if key == Key::ArrowDown { (selected + 1) % length } else { (selected + length - 1) % length };
                            completion_selected.set(next);
                            if let Some(element) = completion_keys.borrow().get(&next).cloned() {
                                spawn(async move {
                                    let _ = element.scroll_to_with_options(ScrollToOptions { behavior: ScrollBehavior::Instant, vertical: ScrollLogicalPosition::Nearest, horizontal: ScrollLogicalPosition::Nearest }).await;
                                });
                            }
                            event.prevent_default(); event.stop_propagation(); return;
                        }
                        Key::Enter | Key::Tab => { event.prevent_default(); event.stop_propagation(); oncompletion_accept.call(completion_selected()); return; }
                        Key::Escape => { event.prevent_default(); event.stop_propagation(); oncompletion_close.call(()); return; }
                        _ => {}
                    }
                }
                if primary {
                    if let Key::Character(value) = &key {
                        if value.eq_ignore_ascii_case("z") || value.eq_ignore_ascii_case("y") {
                            event.prevent_default();
                            event.stop_propagation();
                            onhistory.call(value.eq_ignore_ascii_case("z") && !shift);
                            return;
                        }
                    }
                }
                #[cfg(feature = "layout-fixture")]
                {
                    use crate::native_proportional_navigation as navigation;
                    let plain = modifiers.is_empty() || modifiers == Modifiers::SHIFT;
                    let state = model.read();
                    let cursor = state.engine.get_cursor_state();
                    if font_loading() && plain && proportional_enabled(fixture,model,cursor.position.line,false) && matches!(key,Key::ArrowUp|Key::ArrowDown|Key::ArrowLeft|Key::ArrowRight|Key::Home|Key::End) {
                        event.prevent_default(); event.stop_propagation(); return;
                    }
                    let valid_key = |stamp: crate::native_proportional_run::RunKey| stamp.model == model && stamp.revision == state.revision && stamp.viewport_width == viewport_width_pixels() && stamp.font_epoch == font_epoch() && stamp.presentation == presentation_state(fixture,model) && !font_loading();
                    let affinity = caret_affinity.peek().filter(|value| valid_key(value.key) && value.offset == cursor.offset);
                    let preferred = vertical_intent.peek().filter(|old| valid_key(old.key) && old.offset == cursor.offset).map(|old| old.x);
                    let runs = navigation_runs.borrow();
                    let row = state.engine.logical_position_to_visual(cursor.position.line, cursor.position.column).map(|(row,_)| row);
                    let measured = runs.values().find(|run| valid_key(run.key) && affinity.map(|value| value.key == run.key).unwrap_or_else(|| row == Some(run.key.row)) && proportional_enabled(fixture, model, state.engine.editor().line_index().char_offset_to_position(run.key.start).0, false));
                    let origin = measured.and_then(|run| navigation::origin(run,cursor.offset,affinity,&heights.read()));
                    let mut keep_offset = false;
                    let target = if plain && key == Key::ArrowRight && affinity.is_some() {
                        keep_offset = true;
                        None
                    } else if plain && key == Key::ArrowLeft && affinity.is_none() {
                        measured.zip(origin).and_then(|(run,origin)| {
                            runs.values().filter(|candidate| valid_key(candidate.key) && candidate.key.row.abs_diff(run.key.row)<=1)
                                .flat_map(|candidate| navigation::positions(candidate,&heights.read()))
                                .filter(|target| target.offset==cursor.offset && target.affinity.is_some() && target.y<origin.y-0.5)
                                .max_by(|a,b| a.y.total_cmp(&b.y)).map(|target| (run.key,target,target.x))
                        })
                    } else if plain && matches!(key,Key::Home|Key::End) {
                        measured.zip(origin).and_then(|(run,origin)| navigation::edge(run,origin,key==Key::End,&heights.read())).map(|target| (measured.unwrap().key,target,target.x))
                    } else if plain && matches!(key,Key::ArrowUp|Key::ArrowDown) && fixture.is_some_and(|value| value().proportional_line.is_some()) {
                        let down=key==Key::ArrowDown;
                        if let Some((run,origin)) = measured.zip(origin) {
                            let x=preferred.unwrap_or(origin.x);
                            navigation::vertical(&runs,run.key,origin,down,x,&heights.read()).or_else(|| {
                                let adjacent=heights.read().adjacent_visible(run.key.row,down)?;
                                navigation::uniform(&state.engine,adjacent,x,cell_pixels())
                            }).map(|target| (run.key,target,x))
                        } else {
                            row.and_then(|row| {
                                let adjacent=heights.read().adjacent_visible(row,down)?;
                                let x=preferred.unwrap_or_else(|| state.engine.logical_position_to_visual(cursor.position.line,cursor.position.column).map(|(_,column)| column as f64*cell_pixels()).unwrap_or(0.0));
                                if let Some(run)=runs.values().find(|run| valid_key(run.key) && run.key.row==adjacent && proportional_enabled(fixture,model,state.engine.editor().line_index().char_offset_to_position(run.key.start).0,false)) {
                                    navigation::enter(run,down,x,&heights.read()).map(|target| (run.key,target,x))
                                } else if preferred.is_some() {
                                    let stamp=vertical_intent.peek().as_ref()?.key;
                                    navigation::uniform(&state.engine,adjacent,x,cell_pixels()).map(|target| (stamp,target,x))
                                } else { None }
                            })
                        }
                    } else { None };
                    drop(runs);
                    drop(state);
                    if keep_offset {
                        caret_affinity.set(None); vertical_intent.set(None);
                        event.prevent_default(); event.stop_propagation(); return;
                    }
                    if let Some((stamp,target,x)) = target {
                        mutate(model,onchange,|editor| {
                            let (line,column)=editor.editor().line_index().char_offset_to_position(target.offset);
                            move_cursor(editor,CursorCommand::MoveTo {line,column},shift);
                        });
                        caret_affinity.set(target.affinity);
                        vertical_intent.set(matches!(key,Key::ArrowUp|Key::ArrowDown).then_some(crate::native_proportional_run::VerticalIntent {key:stamp,offset:target.offset,x}));
                        event.prevent_default(); event.stop_propagation(); return;
                    }
                    caret_affinity.set(None); vertical_intent.set(None);
                }
                let newline = (key == Key::Enter).then(|| model.with(crate::native_indent::newline));
                let closing = match &key {
                    Key::Character(value) if !primary && !modifiers.contains(Modifiers::ALT) && value.chars().count() == 1 => model.with(|state| crate::native_indent::closing(state, value.chars().next().unwrap())),
                    _ => None,
                };
                let comment_config = crate::native_language::comments(model.peek().language);
                let handled = mutate(
                    model,
                    onchange,
                    |editor| {
                        #[cfg(feature = "native-desktop")]
                        if primary {
                            match key {
                                Key::Character(ref value) if value.eq_ignore_ascii_case("c") => {
                                    if !keyboard_selected_text.is_empty()
                                        && with_native_clipboard(|clipboard| {
                                                clipboard.set_text(keyboard_selected_text.clone()).ok()
                                            })
                                            .is_some()
                                    {
                                        return true;
                                    }
                                }
                                Key::Character(ref value) if value.eq_ignore_ascii_case("x") => {
                                    if !keyboard_selected_text.is_empty()
                                        && with_native_clipboard(|clipboard| {
                                                clipboard.set_text(keyboard_selected_text.clone()).ok()
                                            })
                                            .is_some()
                                    {
                                        let _ = editor
                                            .execute(Command::Edit(EditCommand::Backspace));
                                        return true;
                                    }
                                }
                                Key::Character(ref value) if value.eq_ignore_ascii_case("v") => {
                                    if let Some(text) = with_native_clipboard(|clipboard| {
                                        clipboard.get_text().ok()
                                    }) {
                                        if !text.is_empty() {
                                            let _ = editor
                                                .execute(Command::Edit(EditCommand::InsertText { text }));
                                            return true;
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        if primary {
                            match key {
                                Key::Character(ref value) if value.eq_ignore_ascii_case("a") => {
                                    let last_line = editor
                                        .editor()
                                        .line_count()
                                        .saturating_sub(1);
                                    let end_column = editor.editor().char_count();
                                    let _ = editor
                                        .execute(
                                            Command::Cursor(CursorCommand::SetSelection {
                                                start: Position::new(0, 0),
                                                end: Position::new(last_line, end_column),
                                            }),
                                        );
                                    return true;
                                }
                                _ => {}
                            }
                        }
                        if primary && matches!(&key, Key::Character(value) if value == "/") {
                            if let Some(config) = comment_config {
                                let _ = editor.execute(Command::Edit(EditCommand::ToggleComment { config }));
                            }
                            return true;
                        }
                        let movement = match key {
                            Key::ArrowLeft => Some(CursorCommand::MoveGraphemeLeft),
                            Key::ArrowRight => Some(CursorCommand::MoveGraphemeRight),
                            Key::ArrowUp => {
                                Some(CursorCommand::MoveVisualBy { delta_rows: -1 })
                            }
                            Key::ArrowDown => {
                                Some(CursorCommand::MoveVisualBy { delta_rows: 1 })
                            }
                            Key::Home if primary => {
                                Some(CursorCommand::MoveTo {
                                    line: 0,
                                    column: 0,
                                })
                            }
                            Key::End if primary => {
                                Some(CursorCommand::MoveTo {
                                    line: editor.editor().line_count().saturating_sub(1),
                                    column: usize::MAX,
                                })
                            }
                            Key::Home => Some(CursorCommand::MoveToLineStart),
                            Key::End => Some(CursorCommand::MoveToLineEnd),
                            _ => None,
                        };
                        if let Some(command) = movement {
                            move_cursor(editor, command, shift);
                            return true;
                        }
                        let edit = match key {
                            Key::Backspace => Some(EditCommand::Backspace),
                            Key::Delete => Some(EditCommand::DeleteForward),
                            Key::Enter => {
                                if let Some(newline) = newline { newline.apply(editor); }
                                return true;
                            }
                            Key::Tab if shift => Some(EditCommand::Outdent),
                            Key::Tab if editor.get_cursor_state().selection.is_some() => Some(EditCommand::Indent),
                            Key::Tab => Some(EditCommand::InsertTab),
                            Key::Character(
                                ref value,
                            ) if !primary && !modifiers.contains(Modifiers::ALT)
                                && !value.is_empty() => {
                                if let Some(closing) = closing { closing.apply(editor); return true; }
                                if value.chars().count() == 1 {
                                    Some(EditCommand::TypeChar { ch: value.chars().next().unwrap() })
                                } else {
                                    Some(EditCommand::InsertText { text: value.clone() })
                                }
                            }
                            _ => None,
                        };
                        if let Some(command) = edit {
                            let _ = editor.execute(Command::Edit(command));
                            return true;
                        }
                        false
                    },
                );
                if handled {
                    if !primary && matches!(&key, Key::Character(value) if wiki_enabled || value.chars().all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '.')) { oncompletion.call(()); }
                    if (wiki_enabled || !completion_items.is_empty()) && key == Key::Backspace { oncompletion.call(()); }
                    event.prevent_default();
                    event.stop_propagation();
                    let cursor_line = model
                        .with(|state| {
                            state
                                .engine
                                .editor()
                                .logical_position_to_visual(
                                    state.engine.get_cursor_state().position.line,
                                    state.engine.get_cursor_state().position.column,
                                )
                                .map(|(row, _)| row)
                                .unwrap_or(0)
                        });
                    let current_start = first_row();
                    let next_start = heights.read().reveal_start(current_start, viewport_pixels(), cursor_line);
                    if next_start != current_start {
                        scroll_displacement.set(0.0);
                        first_row.set(next_start);

                    }
                }
            },
            onmouseup: move |_| dragging.set(false),
            span {
                class: "mk-native-cell-measure",
                aria_hidden: "true",
                onmounted: move |event: MountedEvent| {
                    *probe_mounted.borrow_mut() = Some(event.data.clone());
                    measure.call(());
                },
            }
            div {
                class: "mk-editor-core-viewport",
                id: viewport_id.clone(),
                role: "textbox",
                aria_label: label.clone(),
                aria_multiline: "true",

                onmounted: move |event: MountedEvent| {
                    *viewport_on_mount.borrow_mut() = Some(event.data.clone());
                    #[cfg(feature = "layout-fixture")]
                    {
                        let id=viewport_id.clone();
                        spawn(async move {
                            crate::native_browser_geometry::watch_fonts(&id,Callback::new(move |notice: crate::native_browser_geometry::FontNotice| {
                                if font_epoch.try_peek().is_err() || font_loading.try_peek().is_err() { return; }
                                if *font_epoch.peek()!=notice.epoch { font_epoch.set(notice.epoch); }
                                if *font_loading.peek()!=notice.loading { font_loading.set(notice.loading); }
                            })).await;
                        });
                    }
                    measure.call(());
                    // P-112: WebViews can miss resize events when a dock tile grows.
                    spawn(async move {
                        loop {
                            futures_timer::Delay::new(std::time::Duration::from_millis(600)).await;
                            measure.call(());
                        }
                    });
                },
                onresize: move |_| measure.call(()),
                onscroll: move |event| {
                    oncompletion_close.call(());
                    hovered_position.set(None);
                    if !*viewport_ready.peek() {
                        return;
                    }
                    let y = event.data().scroll_top().max(0.0);
                    let row = heights.read().row_at_y(y);
                    scroll_displacement.set(y - heights.read().row_top(row));
                    first_row.set(row);
                    if !model.peek().preferences.wrap {
                        first_column.set(UniformLayout::new(row_pixels(), cell_pixels()).column_at_x(event.data().scroll_left()));
                    }
                },
                div { style: "height: {heights.read().total_height()}px; width: {canvas_width}; position: relative;",
                    div { style: "position: absolute; top: {heights.read().row_top(first_row())}px; left: 0; right: 0;",
                        for (row_index, line) in grid.lines.iter().enumerate() {
                            {
                                let logical_line = line.logical_line_index;
                                let end_offset = line.char_offset_end;
                                let visual_row = first_row() + row_index;
                                #[cfg(feature = "layout-fixture")]
                                let block_on_mount = block_mounted.clone();
                                let block_height = {
                                    #[cfg(feature = "layout-fixture")]
                                    { fixture.map(|value| value()).filter(|config| config.line == logical_line && !line.is_wrapped_part).map(|config| measured_block().filter(|(key, revision, _)| *key == config && *revision == model.peek().revision).map(|(_, _, height)| height).unwrap_or(config.block_height.max(0.0))).unwrap_or(0.0) }
                                    #[cfg(not(feature = "layout-fixture"))]
                                    { 0.0 }
                                };
                                let text_height = heights.read().row_height(visual_row) - block_height;
                                let block_css_height = {
                                    #[cfg(feature = "layout-fixture")]
                                    { fixture.map(|value| value().block_height.max(0.0)).unwrap_or(0.0) }
                                    #[cfg(not(feature = "layout-fixture"))]
                                    { 0.0 }
                                };
                                let interactive_widget = {
                                    #[cfg(feature = "layout-fixture")]
                                    { fixture.is_some_and(|value| value().interactive_widget) }
                                    #[cfg(not(feature = "layout-fixture"))]
                                    { false }
                                };
                                #[cfg(feature = "layout-fixture")]
                                let widget_revision = model.peek().revision;
                                rsx! {
                                    if text_height > 0.0 || block_height > 0.0 {
                                    if block_height > 0.0 {
                                        div {
                                            class: "mk-layout-fixture-block mk-native-block-widget",
                                            "data-source-anchor": "{line.char_offset_start}",
                                            contenteditable: "false",
                                            style: if interactive_widget { format!("min-height: {block_css_height}px; white-space: normal;") } else { format!("height: {block_css_height}px; overflow: hidden; background: #345; color: white;") },
                                            onmounted: move |_event: MountedEvent| {
                                                #[cfg(feature = "layout-fixture")]
                                                { *block_on_mount.borrow_mut() = Some(_event.data.clone()); block_measure.call(()); }
                                            },
                                            onresize: move |_| {
                                                #[cfg(feature = "layout-fixture")]
                                                block_measure.call(());
                                            },
                                            onmousedown: move |event| {
                                                event.prevent_default();
                                                mutate(model, onchange, |editor| place_caret_at(editor, Position::new(logical_line, 0)));
                                            },
                                            {
                                                #[cfg(feature = "layout-fixture")]
                                                {
                                                    if interactive_widget {
                                                        rsx! {
                                                            crate::native_widgets::PreviewBlock {
                                                                key: "fixture-block-{line.char_offset_start}-{widget_revision.0}",
                                                                identity: format!("{completion_id}-fixture-block-{}", line.char_offset_start),
                                                                title: decorations.block(line.char_offset_start).unwrap_or("Preview").to_string(),
                                                                show_label: "Show details".to_string(),
                                                                hide_label: "Hide details".to_string(),
                                                                source_label: "Edit source".to_string(),
                                                                onsource: move |_| {
                                                                    if model.peek().revision != widget_revision || composing() { return; }
                                                                    onprepare.call(());
                                                                    if model.peek().revision != widget_revision { return; }
                                                                    mutate(model, onchange, |editor| place_caret_at(editor, Position::new(logical_line, 0)));
                                                                    let mut request = focus_request;
                                                                    request.with_mut(|value| *value = value.wrapping_add(1));
                                                                },
                                                                crate::native_widgets::FixtureWidgetBody {}
                                                            }
                                                        }
                                                    } else {
                                                        let title = decorations.block(line.char_offset_start).unwrap_or("Fixture block (view only)");
                                                        rsx! { "{title}" }
                                                    }
                                                }
                                                #[cfg(not(feature = "layout-fixture"))]
                                                { "Fixture block (view only)" }
                                            }
                                        }
                                    }
                                    div {
                                        class: "mk-editor-core-row",
                                        "data-line": "{line.logical_line_index}",
                                        style: "height: {text_height}px; line-height: {layout.row_height()}px; position: relative;",
                                        if !line.is_wrapped_part {
                                            if let Some(region) = folds.iter().find(|region| region.start_line == logical_line) {
                                                button {
                                                    class: "mk-native-fold-toggle",
                                                    "data-fold-line": "{logical_line}",
                                                    aria_label: if region.is_collapsed { unfold_label.clone() } else { fold_label.clone() },
                                                    aria_expanded: (!region.is_collapsed).to_string(),
                                                    // Let native button Enter/Space/Tab behavior run without
                                                    // routing its keystrokes into document editing.
                                                    onkeydown: move |event| event.stop_propagation(),
                                                    onmousedown: move |event| { event.prevent_default(); event.stop_propagation(); },
                                                    onclick: {
                                                        let collapsed = region.is_collapsed;
                                                        move |event| { event.stop_propagation(); onfold.call((logical_line, !collapsed)); }
                                                    },
                                                    if region.is_collapsed { "▸" } else { "▾" }
                                                }
                                            } else { span { class: "mk-native-fold-slot", " " } }
                                        } else { span { class: "mk-native-fold-slot", " " } }
                                        span {
                                            class: "mk-editor-core-gutter",
                                            onmousemove: move |_| hovered_position.set(None),
                                            title: decorations.line_diagnostics(logical_line).map(|d| d.message.as_str()).collect::<Vec<_>>().join("\n"),
                                            if decorations.line_diagnostics(logical_line).next().is_some() {
                                                span { class: "mk-native-diagnostic-marker", "●" }
                                            }
                                            "{line.logical_line_index + 1:>4} "
                                            span { class: "mk-native-presence-slot",
                                            if !line.is_wrapped_part && presence.iter().any(|member| member.line == Some(logical_line as u32)) {
                                                span {
                                                    class: "mk-native-presence",
                                                    "data-presence-line": "{logical_line}",
                                                    title: presence.iter().filter(|member| member.line == Some(logical_line as u32)).map(|member| member.name.as_str()).collect::<Vec<_>>().join(", "),
                                                    for member in presence.iter().filter(|member| member.line == Some(logical_line as u32)) {
                                                        span { class: "mk-native-presence-member", "data-presence-window": "{member.window}", "{member.initials()}" }
                                                    }
                                                }
                                            }
                                            }
                                        }
                                        if proportional_enabled(fixture, model, logical_line, line.is_fold_placeholder_appended) && !line.cells.is_empty() {
                                            {
                                                #[cfg(feature = "layout-fixture")]
                                                {
                                                    let stamp = crate::native_proportional_run::RunKey { model, revision: model.read().revision, start: line.char_offset_start, row: visual_row, viewport_width: viewport_width_pixels(), font_epoch: font_epoch(), presentation: show_presentation };
                                                    let include_end = grid.lines.get(row_index + 1).is_none_or(|next| next.char_offset_start != line.char_offset_start + line.cells.len());
                                                    rsx! { crate::native_proportional_run::ProportionalRun {
                                                        text: line.cells.iter().map(|cell| cell.ch).collect::<String>(), stamp,
                                                        replacements: crate::native_presentation::in_run(&presentation_values, show_presentation, line.char_offset_start..line.char_offset_start+line.cells.len()),
                                                        marks: (0..line.cells.len()).map(|i| decorations.cell(line.char_offset_start + i)).collect::<Vec<_>>(),
                                                        tags: tags[row_index].iter().enumerate().map(|(i, tag)| if bracket_pair.is_some_and(|(a,b)| a == line.char_offset_start+i || b == line.char_offset_start+i) { format!("{tag} mk-native-bracket-match") } else { tag.clone() }).collect::<Vec<_>>(),
                                                        points: (0..=line.cells.len()).filter(|i| *i < line.cells.len() || include_end).filter_map(|i| decorations.search_point(line.char_offset_start+i).map(|current| (i,current))).collect::<Vec<_>>(),
                                                        include_end, affinity: caret_affinity, dragging, hovered_position, onwiki, onchange,
                                                        onheight: {
                                                        let measured_runs = measured_runs.clone();
                                                        move |result: crate::native_proportional_run::RunHeight| {
                                                            let view = current_view.peek();
                                                            if view.0 != result.key.model || result.key.model.read().revision != result.key.revision || viewport_width_pixels() != result.key.viewport_width || font_epoch()!=result.key.font_epoch || result.key.presentation != presentation_state(fixture,model) || font_loading() || !result.height.is_finite() || result.height <= 0.0 { return; }
                                                            measured_runs.borrow_mut().retain(|_, old| old.key.model == result.key.model && old.key.revision == result.key.revision && old.key.viewport_width == result.key.viewport_width && old.key.font_epoch==result.key.font_epoch);
                                                            measured_runs.borrow_mut().insert(result.key.start, result.clone());
                                                            let previous=*caret_affinity.peek();
                                                            if let Some(previous)=previous.filter(|value| value.key.model==result.key.model && value.key.revision==result.key.revision && value.offset>result.key.start && value.offset<=result.key.start+result.length) {
                                                                let local=previous.offset-result.key.start;
                                                                let backward=result.geometry.caret_backward(local);
                                                                let wrapped=backward.zip(result.geometry.caret(local)).is_some_and(|(back,forward)| (back.1-forward.1).abs()>0.5) || local==result.length && !result.include_end;
                                                                caret_affinity.set(wrapped.then_some(crate::native_proportional_navigation::CaretAffinity {key:result.key,offset:previous.offset}));
                                                            }
                                                            let mut cache = current_runs();
                                                            let entry = crate::native_layout::CachedRunHeight { revision: result.key.revision, row: result.key.row, start: result.key.start, viewport_width: result.key.viewport_width, height: result.height, presentation: result.key.presentation };
                                                            if cache.read().get(&entry.start) == Some(&entry) { return; }
                                                            cache.with_mut(|values| { values.retain(|_, old| old.revision == entry.revision && old.viewport_width == entry.viewport_width); values.insert(entry.start, entry); });
                                                        }
                                                        },
                                                        oninvalidated: {
                                                            let measured_runs=measured_runs.clone();
                                                            move |key:crate::native_proportional_run::RunKey| {
                                                                let mut runs=measured_runs.borrow_mut();
                                                                if runs.get(&key.start).is_some_and(|run| run.key==key) { runs.remove(&key.start); }
                                                            }
                                                        },
                                                        oncaret: move |(key, y, height): (crate::native_proportional_run::RunKey, f64, f64)| {
                                                            let view = current_view.peek();
                                                            if view.0 != key.model || key.model.read().revision != key.revision || viewport_width_pixels() != key.viewport_width || font_epoch()!=key.font_epoch || key.presentation != presentation_state(fixture,model) || font_loading() { return; }
                                                            let index = heights.read();
                                                            let mut row = view.1;
                                                            let mut displacement = current_scroll();
                                                            let top = index.row_top(row()) + displacement();
                                                            let caret_top = index.row_top(key.row) + block_height + y;
                                                            let target = if caret_top < top { caret_top } else if caret_top + height > top + viewport_pixels() { (caret_top + height - viewport_pixels()).max(0.0) } else { return; };
                                                            let target_row = index.row_at_y(target);
                                                            row.set(target_row);
                                                            displacement.set(target - index.row_top(target_row));
                                                        },
                                                        font_loading: font_loading(), preedit: preedit.clone(), delay_ms: fixture.map(|value| value().measurement_delay_ms).unwrap_or_default(),
                                                    } }
                                                }
                                                #[cfg(not(feature = "layout-fixture"))]
                                                { rsx! {} }
                                            }
                                        } else {
                                        if line.segment_x_start_cells > 0 {
                                            span { style: "display: inline-block; width: {line.segment_x_start_cells}ch;", aria_hidden: "true" }
                                        }
                                        for (column, cell) in line.cells.iter().enumerate() {
                                            if line.char_offset_start + column <= line.char_offset_end && decorations.search_point(line.char_offset_start + column).is_some() {
                                                span { class: if decorations.search_point(line.char_offset_start + column) == Some(true) { "mk-native-search-zero mk-native-search-current" } else { "mk-native-search-zero" }, aria_hidden: "true" }
                                            }
                                            if cursor.offset == line.char_offset_start + column {
                                                span {
                                                    class: "mk-editor-core-caret",
                                                    style: "display: inline-block; position: relative; width: 0; height: {layout.row_height()}px; vertical-align: top; pointer-events: none;",
                                                    span {
                                                        class: "mk-editor-core-caret-bar",
                                                        style: "position: absolute; top: 0; left: 0; width: 1px; height: {layout.row_height()}px; background: currentColor;",
                                                    }
                                                }
                                                if !preedit.is_empty() {
                                                    span { class: "mk-editor-core-preedit", "{preedit}" }
                                                }
                                            }
                                            if cell.styles.contains(&editor_core::FOLD_PLACEHOLDER_STYLE_ID) {
                                                span {
                                                    class: "mk-native-fold-placeholder",
                                                    onmousedown: move |event| { event.prevent_default(); event.stop_propagation(); onfold.call((logical_line, false)); },
                                                    "{cell.ch}"
                                                }
                                            } else {
                                            NativeCell {
                                                model,
                                                onchange,
                                                tag: tags[row_index][column].clone(),
                                                marks: decorations.cell(line.char_offset_start + column),
                                                onwiki,
                                                dragging,
                                                hovered_position,
                                                line: line.logical_line_index,
                                                column: model
                                                    .with(|state| {
                                                        state
                                                            .engine
                                                            .editor()
                                                            .line_index()
                                                            .char_offset_to_position(line.char_offset_start + column)
                                                            .1
                                                    }),
                                                matched: bracket_pair.is_some_and(|(a, b)| a == line.char_offset_start + column || b == line.char_offset_start + column),
                                                selected: selection
                                                    .as_ref()
                                                    .is_some_and(|range| contains(
                                                        range,
                                                        Position::new(
                                                            line.logical_line_index,
                                                            model
                                                                .with(|state| {
                                                                    state
                                                                        .engine
                                                                        .editor()
                                                                        .line_index()
                                                                        .char_offset_to_position(line.char_offset_start + column)
                                                                        .1
                                                                }),
                                                        ),
                                                    )),
                                                ch: cell.ch,
                                                width: cell.width,
                                                layout,
                                            }
                                            }
                                        }
                                        if decorations.search_point(line.char_offset_end).is_some() && !line.is_fold_placeholder_appended && grid.lines.get(row_index + 1).is_none_or(|next| next.char_offset_start != line.char_offset_end) {
                                            span { class: if decorations.search_point(line.char_offset_end) == Some(true) { "mk-native-search-zero mk-native-search-current" } else { "mk-native-search-zero" }, aria_hidden: "true" }
                                        }
                                        if !line.is_fold_placeholder_appended && line.char_offset_end == cursor.offset
                                            && grid
                                                .lines
                                                .get(row_index + 1)
                                                .is_none_or(|next| next.char_offset_start != cursor.offset)
                                        {
                                            span {
                                                class: "mk-editor-core-caret",
                                                style: "display: inline-block; position: relative; width: 0; height: {layout.row_height()}px; vertical-align: top; pointer-events: none;",
                                                span {
                                                    class: "mk-editor-core-caret-bar",
                                                    style: "position: absolute; top: 0; left: 0; width: 1px; height: {layout.row_height()}px; background: currentColor;",
                                                }
                                            }
                                            if !preedit.is_empty() {
                                                span { class: "mk-editor-core-preedit", "{preedit}" }
                                            }
                                        }
                                        span {
                                            class: "mk-editor-core-row-trailing-space",
                                            onmousemove: move |_| hovered_position.set(None),
                                            style: "position: absolute; left: {(line.logical_line_index + 1).to_string().len().max(4) + 5 + line.segment_x_start_cells + line.cells.iter().map(|cell| cell.width).sum::<usize>()}ch; right: 0; top: 0; height: {layout.row_height()}px;",
                                            onmousedown: move |_| {
                                                dragging.set(true);
                                                mutate(
                                                    model,
                                                    onchange,
                                                    |editor| {
                                                        place_caret_at(
                                                            editor,
                                                            Position::new(
                                                                logical_line,
                                                                editor
                                                                    .editor()
                                                                    .line_index()
                                                                    .char_offset_to_position(end_offset)
                                                                    .1,
                                                            ),
                                                        )
                                                    },
                                                );
                                            },
                                        }
                                        }
                                    }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if !completion_items.is_empty() {
                div { class: "mk-native-completions", id: "{completion_id}", role: "listbox", aria_label: completion_label,
                    onmousedown: move |event| { event.prevent_default(); event.stop_propagation(); },
                    for (index, item) in completion_items.iter().enumerate() {
                        button {
                            class: if completion_selected() == index { "mk-native-completion mk-native-completion-selected" } else { "mk-native-completion" },
                            id: "{completion_id}-{index}",
                            onmounted: {
                                let nodes = completion_nodes.clone();
                                move |event| { nodes.borrow_mut().insert(index, event.data()); }
                            },
                            role: "option", aria_selected: (completion_selected() == index).to_string(),
                            tabindex: -1,
                            onclick: move |event| { event.stop_propagation(); oncompletion_accept.call(index); },
                            span { class: "mk-native-completion-label", "{item.label}" }
                            span { class: "mk-native-completion-kind", "{item.kind}" }
                            if let Some(detail) = &item.detail { span { class: "mk-native-completion-detail", "{detail}" } }
                        }
                    }
                }
            }
            if let Some(text) = hover_text {
                div { class: "mk-native-hover mk-hover", role: "tooltip", aria_label: hover_label,
                    onmousedown: move |event| event.stop_propagation(),
                    oncopy: move |event| event.stop_propagation(),
                    oncut: move |event| event.stop_propagation(),
                    onpaste: move |event| event.stop_propagation(),
                    onkeydown: move |event| {
                        event.stop_propagation();
                        if event.key() == Key::Escape { hover_target.set(None); }
                    },
                    tabindex: 0,
                    "{text}"
                }
            }
            // A keyed list forces the input sink to remount after consumption.
            // A key on a static nested textarea does not replace its live value.
            for epoch in [input_epoch()] {
                textarea {
                    class: "mk-editor-core-input-sink",
                    aria_autocomplete: "list",
                    aria_controls: if !completion_items.is_empty() { Some(completion_id.clone()) } else { None },
                    aria_activedescendant: if !completion_items.is_empty() { Some(format!("{completion_id}-{}", completion_selected())) } else { None },
                    aria_label: sink_label.clone(),
                    tabindex: -1,
                    key: "{epoch}",
                    value: "",
                    style: "position: fixed; left: -10000px; top: 0; width: 1px; height: 1px; opacity: 0;",
                    onmounted: {
                        let mounted = mount_on_mount_input.clone();
                        move |event: MountedEvent| {
                            *mounted.borrow_mut() = Some(event.data.clone());
                            if *refocus_input.peek() {
                                refocus_input.set(false);
                                let element = event.data.clone();
                                spawn(async move {
                                    let _ = element.set_focus(true).await;
                                });
                            }
                        }
                    },
                    oncompositionstart: move |_| {
                        onprepare.call(());
                        composing.set(true);
                        composition_preview.set(String::new());
                    },
                    oncompositionupdate: move |event| composition_preview.set(event.data().data()),
                    oncompositionend: move |event| {
                        // Native WebKit can send input before compositionend
                        // without compositionstart. The input already consumed
                        // this sink; its trailing end event must not insert twice.
                        if epoch != input_epoch() {
                            return;
                        }
                        onprepare.call(());
                        let text = event.data().data();
                        if !text.is_empty() {
                            let _ = mutate(
                                model,
                                onchange,
                                |editor| {
                                    editor
                                        .execute(
                                            Command::Edit(EditCommand::InsertText {
                                                text: text.clone(),
                                            }),
                                        )
                                },
                            );
                        }
                        composition_preview.set(String::new());
                        composing.set(false);
                        if wiki_enabled { oncompletion.call(()); }
                        refocus_input.set(true);
                        input_epoch += 1;
                    },
                    spellcheck: false,
                    oninput: move |event| {
                        onprepare.call(());
                        let text = event.value();
                        if text.is_empty() || composing() {
                            return;
                        }
                        if epoch != input_epoch() {
                            return;
                        }
                        let _ = mutate(
                            model,
                            onchange,
                            |editor| editor.execute(Command::Edit(EditCommand::InsertText { text })),
                        );
                        if wiki_enabled { oncompletion.call(()); }
                        refocus_input.set(true);
                        input_epoch += 1;
                    },
                }
            }

        }
    }
}

#[component]
fn NativeCell(
    model: Signal<NativeModel>,
    onchange: Callback<()>,
    tag: String,
    marks: CellMarks,
    onwiki: Callback<String>,
    dragging: Signal<bool>,
    hovered_position: Signal<Option<(usize, usize)>>,
    line: usize,
    column: usize,
    selected: bool,
    matched: bool,

    ch: char,
    width: usize,
    layout: UniformLayout,
) -> Element {
    let mark_classes = marks.classes();
    let CellMarks {
        diagnostic, wiki, ..
    } = marks;
    rsx! {
        span {
            title: diagnostic.as_ref().map(|d| d.message.clone()).or_else(|| wiki.as_ref().map(|mark| mark.target.clone())).unwrap_or_default(),
            class: format!("mk-editor-core-cell {} {} {tag} {mark_classes}", if selected { "mk-editor-core-selected" } else { "" }, if matched { "mk-native-bracket-match" } else { "" }),
            style: "display: inline-block; width: {width}ch;",

            onmousedown: move |event| {
                if event.trigger_button() == Some(MouseButton::Primary) && moonkale_ext_api::keys::primary(&event.modifiers()) {
                    if let Some(mark) = &wiki { event.prevent_default(); event.stop_propagation(); onwiki.call(mark.target.clone()); return; }
                }
                dragging.set(true);
                let target_column = layout.hit_column(column, width, event.element_coordinates().x);
                mutate(
                    model,
                    onchange,
                    |editor| place_caret_at(editor, Position::new(line, target_column)),
                );
            },
            onmousemove: move |event| {
                let target_column = layout.hit_column(column, width, event.element_coordinates().x);
                if !dragging() && !event.held_buttons().contains(MouseButton::Primary) && hovered_position() != Some((line, column)) {
                    hovered_position.set(Some((line, column)));
                }
                if dragging() && event.held_buttons().contains(MouseButton::Primary) {
                    let _ = mutate(
                        model,
                        onchange,
                        |editor| {
                            editor
                                .execute(
                                    Command::Cursor(CursorCommand::ExtendSelection {
                                        to: Position::new(line, target_column),
                                    }),
                                )
                        },
                    );
                } else {
                    if dragging() {
                        dragging.set(false);
                    }
                }
            },
            "{ch}"
        }
    }
}

pub(crate) fn mutate<R>(
    mut model: Signal<NativeModel>,
    onchange: Callback<()>,
    f: impl FnOnce(&mut EditorStateManager) -> R,
) -> R {
    let result = model.with_mut(|state| f(&mut state.engine));
    onchange.call(());
    result
}

#[cfg(feature = "layout-fixture")]
fn markdown_enabled(
    fixture: Option<Signal<crate::LayoutFixture>>,
    model: Signal<NativeModel>,
) -> bool {
    fixture.is_some_and(|value| value().markdown_preview)
        && model.peek().language == Some(dioxus_code::Language::Markdown)
}

#[cfg(feature = "layout-fixture")]
fn presentation_mode(
    fixture: Option<Signal<crate::LayoutFixture>>,
    model: Signal<NativeModel>,
) -> crate::native_presentation::Mode {
    if markdown_enabled(fixture, model) {
        crate::native_presentation::Mode::Markdown
    } else {
        crate::native_presentation::Mode::Fixture
    }
}

#[cfg(feature = "layout-fixture")]
fn presentation_state(
    fixture: Option<Signal<crate::LayoutFixture>>,
    model: Signal<NativeModel>,
) -> u64 {
    if !fixture.is_some_and(|value| value().presentation) && !markdown_enabled(fixture, model) {
        return 0;
    }
    model.with(|state| {
        crate::native_presentation::state(
            presentation_mode(fixture, model),
            &state.engine,
            state.revision,
        )
    })
}

#[cfg(feature = "layout-fixture")]
fn proportional_enabled(
    fixture: Option<Signal<crate::LayoutFixture>>,
    model: Signal<NativeModel>,
    line: usize,
    folded: bool,
) -> bool {
    let state = model.read();
    !folded
        && state.preferences.wrap
        && fixture.is_some_and(|value| {
            value().proportional_line == Some(line)
                || (value().presentation || markdown_enabled(fixture, model))
                    && value().proportional_line == Some(0)
                    && line
                        <= crate::native_presentation::last_line(
                            presentation_mode(fixture, model),
                            &state.engine,
                            state.revision,
                        )
        })
        && state
            .engine
            .editor()
            .line_index()
            .get_line(line)
            .is_some_and(|value| value.char_count <= 4096)
}
#[cfg(not(feature = "layout-fixture"))]
fn proportional_enabled(_: Option<Signal<()>>, _: Signal<NativeModel>, _: usize, _: bool) -> bool {
    false
}
