//! Measured source runs within the shared virtualized Workspace surface.
use crate::{
    editor_core_spike::place_caret_at,
    native_browser_geometry::{self, Geometry},
    native_decorations::CellMarks,
    native_model::NativeModel,
    native_surface::mutate,
};
use dioxus::prelude::*;
use editor_core::{Command, CursorCommand, Position};
use moonkale_ext_api::editor::DocumentRevision;
use std::{cell::Cell, rc::Rc};

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct RunKey {
    pub model: Signal<NativeModel>,
    pub revision: DocumentRevision,
    pub start: usize,
    pub row: usize,
    pub viewport_width: f64,
    pub font_epoch: u64,
    /// Bit mask of fixture constructs still in preview.
    pub presentation: u64,
}

#[derive(Clone)]
pub(crate) struct RunHeight {
    pub key: RunKey,
    pub height: f64,
    pub geometry: Geometry,
    pub length: usize,
    pub include_end: bool,
}

#[component]
pub(crate) fn ProportionalRun(
    text: String,
    stamp: RunKey,
    replacements: Vec<crate::native_presentation::Replacement>,
    marks: Vec<CellMarks>,
    tags: Vec<String>,
    points: Vec<(usize, bool)>,
    include_end: bool,
    affinity: Signal<Option<crate::native_proportional_navigation::CaretAffinity>>,
    mut dragging: Signal<bool>,
    mut hovered_position: Signal<Option<(usize, usize)>>,
    onwiki: Callback<String>,
    onchange: Callback<()>,
    onheight: Callback<RunHeight>,
    oninvalidated: Callback<RunKey>,
    oncaret: Callback<(RunKey, f64, f64)>,
    preedit: String,
    delay_ms: u64,
    font_loading: bool,
) -> Element {
    let replacements = crate::native_presentation::valid_replacements(&text, replacements);
    let id = use_hook(|| format!("mk-proportional-run-{}", dioxus::core::current_scope_id().0));
    let mut measured = use_signal(|| {
        None::<(
            RunKey,
            String,
            Vec<CellMarks>,
            Vec<String>,
            Vec<crate::native_presentation::Replacement>,
            Geometry,
        )>
    });
    let current = use_memo(use_reactive!(
        |stamp, text, marks, tags, delay_ms, include_end, font_loading, replacements| (
            stamp,
            text,
            marks,
            tags,
            delay_ms,
            include_end,
            font_loading,
            replacements
        )
    ));
    let requests = use_hook(|| Rc::new(Cell::new(0u64)));
    let mut measurement_status = use_signal(|| "pending".to_string());
    let measure = {
        let id = id.clone();
        let requests = requests.clone();
        Callback::new(move |_: ()| {
            let (key, source, marks, tags, delay, include_end, loading, replacements) =
                current.peek().clone();
            if loading {
                measurement_status.set("fonts-loading".to_string());
                return;
            }
            let request = requests.get().wrapping_add(1);
            requests.set(request);
            measurement_status.set("measuring".to_string());
            let id = id.clone();
            let requests = requests.clone();
            spawn(async move {
                let result =
                    native_browser_geometry::measure_presented(&id, &source, &replacements).await;
                if delay > 0 {
                    futures_timer::Delay::new(std::time::Duration::from_millis(delay)).await;
                }
                let Ok(props) = current.try_peek() else {
                    return;
                };
                if requests.get() != request
                    || props.0 != key
                    || props.1 != source
                    || props.2 != marks
                    || props.3 != tags
                    || props.5 != include_end
                    || props.6
                    || props.7 != replacements
                {
                    if requests.get() == request && measurement_status.try_peek().is_ok() {
                        measurement_status.set("stale-props".to_string());
                    }
                    return;
                }
                drop(props);
                if measured.try_peek().is_err() {
                    return;
                }
                if let Ok(geometry) = result {
                    let height = geometry.height;
                    let length = source.chars().count();
                    onheight.call(RunHeight {
                        key,
                        height,
                        geometry: geometry.clone(),
                        length,
                        include_end,
                    });
                    measured.set(Some((key, source, marks, tags, replacements, geometry)));
                    measurement_status.set("accepted".to_string());
                } else if let Err(reason) = result {
                    measurement_status.set(reason);
                }
            });
        })
    };
    use_effect(move || {
        let _ = current();
        measure.call(());
    });
    let geometry = measured
        .read()
        .as_ref()
        .filter(|(key, source, old_marks, old_tags, old_replacements, _)| {
            !font_loading
                && *key == stamp
                && *source == text
                && *old_marks == marks
                && *old_tags == tags
                && *old_replacements == replacements
        })
        .map(|(_, _, _, _, _, geometry)| geometry.clone());
    let ready = geometry.is_some() && !font_loading;
    let cursor = stamp.model.read().engine.get_cursor_state();
    let active_affinity = affinity().filter(|value| {
        value.offset == cursor.offset
            && value.key.model == stamp.model
            && value.key.revision == stamp.revision
            && value.key.viewport_width == stamp.viewport_width
            && value.key.font_epoch == stamp.font_epoch
    });
    let caret = cursor.offset.checked_sub(stamp.start).and_then(|offset| {
        if let Some(value) = active_affinity {
            if value.key != stamp {
                return None;
            }
            geometry
                .as_ref()
                .and_then(|geometry| geometry.caret_backward(offset))
        } else if offset < text.chars().count() || include_end {
            geometry
                .as_ref()
                .and_then(|geometry| geometry.caret(offset))
        } else {
            None
        }
    });
    let selected_boxes = stamp.model.with(|state| {
        let Some(selection) = cursor.selection.as_ref() else {
            return Vec::new();
        };
        let index = state.engine.editor().line_index();
        let a = index.position_to_char_offset(selection.start.line, selection.start.column);
        let b = index.position_to_char_offset(selection.end.line, selection.end.column);
        geometry
            .as_ref()
            .map(|geometry| {
                geometry
                    .boxes
                    .iter()
                    .filter(|rect| {
                        stamp.start + rect.start < a.max(b) && stamp.start + rect.end > a.min(b)
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    });
    use_effect(use_reactive!(|stamp, caret| {
        if let Some((_, y, height)) = caret {
            oncaret.call((stamp, y, height));
        }
    }));
    let move_geometry = geometry.clone();
    let point_boxes: Vec<_> = points
        .iter()
        .filter_map(|(offset, current)| {
            geometry
                .as_ref()
                .and_then(|geometry| geometry.caret(*offset))
                .map(|rect| (rect, *current))
        })
        .collect();
    let move_marks = marks.clone();
    rsx! {
        div {
            class: "mk-proportional-run", "data-ready": ready.to_string(), "data-measurement-status": "{measurement_status}", "data-source-start": stamp.start.to_string(), "data-font-epoch": stamp.font_epoch.to_string(), "data-font-loading": font_loading.to_string(),
            style: "display:inline-block; vertical-align:top; width:calc(100% - 9ch); position:relative;",
            div {
                id, class: "mk-proportional-text",
                style: "font:24px/32px serif; white-space:pre-wrap; overflow-wrap:anywhere; tab-size:4; min-height:32px; direction:ltr; user-select:none;",
                onmounted: move |_| measure.call(()), onresize: move |event| {
                    let unchanged=event.data().get_border_box_size().ok().is_some_and(|size| measured.peek().as_ref().is_some_and(|(_,_,_,_,_,layout)| (layout.width-size.width).abs()<0.000001 && (layout.height-size.height).abs()<0.000001));
                    if !unchanged { measured.set(None); oninvalidated.call(stamp); measure.call(()); }
                },
                onmousedown: move |event| {
                    event.prevent_default();
                    if !ready { return; }
                    if let Some(value) = &geometry {
                        if moonkale_ext_api::keys::primary(&event.modifiers()) {
                            if let Some(mark) = value.box_at(event.element_coordinates().x, event.element_coordinates().y)
                                .and_then(|rect| marks.get(rect.start)).and_then(|mark| mark.wiki.as_ref()) {
                                event.stop_propagation(); onwiki.call(mark.target.clone()); return;
                            }
                        }
                        if let Some(offset) = value.hit(event.element_coordinates().x, event.element_coordinates().y).or_else(|| text.is_empty().then_some(0)) {
                            dragging.set(true);
                            mutate(stamp.model, onchange, |editor| {
                                let (line, column) = editor.editor().line_index().char_offset_to_position(stamp.start + offset);
                                place_caret_at(editor, Position::new(line, column));
                            });
                        }
                    }
                },
                onmousemove: move |event| {
                    if let Some(value) = &move_geometry {
                        if let Some(offset) = value.hit(event.element_coordinates().x, event.element_coordinates().y) {
                            let position = stamp.model.with(|state| state.engine.editor().line_index().char_offset_to_position(stamp.start + offset));
                            if dragging() && event.held_buttons().contains(dioxus::html::input_data::MouseButton::Primary) {
                                let _ = mutate(stamp.model, onchange, |editor| editor.execute(Command::Cursor(CursorCommand::ExtendSelection { to: Position::new(position.0, position.1) })));
                            } else {
                                hovered_position.set(Some(position));
                            }
                        }
                    }
                },
                for (index, ch) in text.chars().enumerate() {
                    if let Some(replacement) = replacements.iter().find(|value| value.start==index) {
                        span {
                            class: "mk-native-inline-widget",
                            "data-source-start": "{replacement.start}", "data-source-end": "{replacement.end}",
                            contenteditable: "false",
                            style: if replacement.widget.is_some() { "display:inline; background:#dce8fa; border-radius:4px;" } else { "display:inline;" },
                            {replacement.widget.clone().unwrap_or_default()}
                        }
                    } else if !replacements.iter().any(|value| value.start<=index && index<value.end) {
                    span {
                        "data-source-start": "{index}", "data-source-end": "{index+1}",
                        class: "mk-editor-core-cell {tags.get(index).cloned().unwrap_or_default()} {move_marks.get(index).map(CellMarks::classes).unwrap_or_default()}",
                        title: move_marks.get(index).and_then(|mark| mark.diagnostic.as_ref().map(|value| value.message.clone()).or_else(|| mark.wiki.as_ref().map(|value| value.target.clone()))).unwrap_or_default(),
                        style: "display:inline;", "{ch}"
                    }
                    }
                }
            }
            for rect in selected_boxes {
                span { class: "mk-editor-core-selected", style: "position:absolute; pointer-events:none; left:{rect.x}px; top:{rect.y}px; width:{rect.width}px; height:{rect.height}px;" }
            }
            for ((x, y, height), current) in point_boxes {
                span { class: if current { "mk-native-search-zero mk-native-search-current" } else { "mk-native-search-zero" }, style: "position:absolute; pointer-events:none; left:{x}px; top:{y}px; height:{height}px;" }
            }
            if let Some((x, y, height)) = caret {
                span { class: "mk-editor-core-caret", style: "position:absolute; pointer-events:none; left:{x}px; top:{y}px; height:{height}px; border-left:1px solid currentColor;",
                    if !preedit.is_empty() { span { class: "mk-editor-core-preedit", "{preedit}" } }
                }
            }
        }
    }
}

/// Remember the desired pixel column only during uninterrupted vertical motion.
#[derive(Clone, Copy)]
pub(crate) struct VerticalIntent {
    pub key: RunKey,
    pub offset: usize,
    pub x: f64,
}
