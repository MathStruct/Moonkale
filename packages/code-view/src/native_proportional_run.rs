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
    /// Provider and revealed source interval.
    pub presentation: crate::native_layout::PresentationState,
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
    onlink: Callback<(RunKey, crate::native_links::Target)>,
    onimage: Callback<(RunKey, crate::native_image_parser::Image)>,
    onchange: Callback<()>,
    onheight: Callback<RunHeight>,
    oninvalidated: Callback<RunKey>,
    oncaret: Callback<(RunKey, f64, f64)>,
    preedit: String,
    delay_ms: u64,
    font_loading: bool,
) -> Element {
    let wiki_hover = try_use_context::<crate::native_wiki_hover::Control>();
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
            let reused = measured.peek().as_ref().and_then(
                |(old, old_source, old_marks, old_tags, old_replacements, geometry)| {
                    let mut comparable = *old;
                    comparable.revision = key.revision;
                    // A reveal elsewhere does not change this run's local
                    // geometry. Exact source, marks and replacement equality
                    // below still reject changes within the measured run.
                    comparable.presentation = key.presentation;
                    (*old != key
                        && comparable == key
                        && *old_source == source
                        && *old_marks == marks
                        && *old_tags == tags
                        && *old_replacements == replacements)
                        .then(|| geometry.clone())
                },
            );
            if let Some(geometry) = reused {
                // Identical run content/layout can be bound to the new revision.
                // Pending browser responses still carry their original request
                // and revision and are rejected below; resize clears this cache.
                onheight.call(RunHeight {
                    key,
                    height: geometry.height,
                    length: source.chars().count(),
                    geometry: geometry.clone(),
                    include_end,
                });
                measured.set(Some((key, source, marks, tags, replacements, geometry)));
                measurement_status.set("reused".to_string());
                return;
            }
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
    let media_changed = {
        let requests = requests.clone();
        use_callback(move |_: ()| {
            requests.set(requests.get().wrapping_add(1));
            measured.set(None);
            oninvalidated.call(current.peek().0);
            measure.call(());
        })
    };
    let resized = use_callback(move |notice: native_browser_geometry::ResizeNotice| {
        let unchanged = measured
            .peek()
            .as_ref()
            .is_some_and(|(_, _, _, _, _, layout)| {
                (layout.width - notice.width).abs() < 1e-6
                    && (layout.height - notice.height).abs() < 1e-6
            });
        if !unchanged {
            media_changed.call(());
        }
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
    #[cfg(feature = "markdown-preview")]
    let math_pointer = use_callback(move |(id, event): (String, MouseEvent)| {
        let current = current.peek();
        let stamp = current.0;
        let measured = measured.peek();
        let Some((key, _, _, _, _, geometry)) = measured.as_ref().filter(|(key, _, _, _, _, _)| {
            *key == stamp && !current.6 && stamp.model.peek().revision == stamp.revision
        }) else {
            event.prevent_default();
            return;
        };
        let _ = key;
        let Some(owner) = geometry
            .links
            .iter()
            .find(|rect| rect.id == format!("math-hit:{id}"))
        else {
            event.prevent_default();
            return;
        };
        let point = event.element_coordinates();
        let x = owner.x + point.x;
        let y = owner.y + point.y;
        if geometry.links.iter().any(|rect| {
            rect.id == format!("math-scroll:{id}")
                && x >= rect.x
                && x <= rect.x + rect.width
                && y >= rect.y
                && y <= rect.y + rect.height
        }) {
            return;
        }
        event.prevent_default();
        if let Some(start) = id
            .strip_prefix("math:")
            .and_then(|id| id.split(':').next())
            .and_then(|id| id.parse::<usize>().ok())
        {
            dragging.set(true);
            mutate(stamp.model, onchange, |editor| {
                let (line, column) = editor
                    .editor()
                    .line_index()
                    .char_offset_to_position(stamp.start + start);
                place_caret_at(editor, Position::new(line, column));
            });
        }
    });
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
    let hover_replacements = replacements.clone();
    let resize_id = id.clone();
    rsx! {
        div {
            class: "mk-proportional-run", "data-ready": ready.to_string(), "data-measurement-status": "{measurement_status}", "data-source-start": stamp.start.to_string(), "data-font-epoch": stamp.font_epoch.to_string(), "data-font-loading": font_loading.to_string(),
            style: if stamp.presentation.provider==3 {format!("display:inline-block; vertical-align:top; width:calc({}px - 9ch); position:relative;",stamp.viewport_width)} else {"display:inline-block; vertical-align:top; width:calc(100% - 9ch); position:relative;".to_string()},
            div {
                id, class: "mk-proportional-text",
                style: if stamp.presentation.provider==3 {format!("font:14px/22px var(--mk-code-font, monospace); white-space:{}; overflow-wrap:{}; tab-size:4; min-height:22px; direction:ltr; user-select:none;",if stamp.model.peek().preferences.wrap {"pre-wrap"}else{"pre"},if stamp.model.peek().preferences.wrap {"anywhere"}else{"normal"})} else {"font:24px/32px serif; white-space:pre-wrap; overflow-wrap:anywhere; tab-size:4; min-height:32px; direction:ltr; user-select:none;".to_string()},
                onmounted: move |_| {
                    measure.call(());
                    let id = resize_id.clone();
                    spawn(async move { native_browser_geometry::watch_resize(&id,resized).await; });
                },
                onmousedown: move |event| {
                    if let Some(hover)=wiki_hover {hover.dismiss();}
                    if geometry.as_ref().and_then(|value|value.link_at(event.element_coordinates().x,event.element_coordinates().y)).is_some_and(|id|id.starts_with("math-scroll:")) {return;}
                    event.prevent_default();
                    if !ready || stamp.model.peek().revision != stamp.revision { return; }
                    if let Some(value) = &geometry {
                        if moonkale_ext_api::keys::primary(&event.modifiers()) && event.held_buttons().contains(dioxus::html::input_data::MouseButton::Primary) {
                            if let Some(id) = value.link_at(event.element_coordinates().x,event.element_coordinates().y) {
                                if let Some(image) = preview_images(&replacements).into_iter().find_map(|(key,image)| (key==id).then_some(image)) {
                                    event.stop_propagation();onimage.call((stamp,image));return;
                                }
                                if let Some(target) = preview_targets(&replacements).into_iter().find_map(|(key,target)| (key == id).then_some(target)) {
                                    event.stop_propagation(); onlink.call((stamp,target)); return;
                                }
                            }
                            if let Some(mark) = value.box_at(event.element_coordinates().x, event.element_coordinates().y)
                                .filter(|rect| !replacements.iter().any(|replacement| replacement.start <= rect.start && rect.end <= replacement.end))
                                .and_then(|rect| marks.get(rect.start)).and_then(|mark| mark.wiki.as_ref()) {
                                event.stop_propagation(); onwiki.call(mark.target.clone()); return;
                            }
                        }
                        let image_start=value.link_at(event.element_coordinates().x,event.element_coordinates().y).and_then(|id|id.strip_prefix("image:")).and_then(|start|start.parse::<usize>().ok());
                        if let Some(offset) = image_start.or_else(||value.hit(event.element_coordinates().x, event.element_coordinates().y)).or_else(|| text.is_empty().then_some(0)) {
                            dragging.set(true);
                            mutate(stamp.model, onchange, |editor| {
                                let (line, column) = editor.editor().line_index().char_offset_to_position(stamp.start + offset);
                                place_caret_at(editor, Position::new(line, column));
                            });
                        }
                    }
                },
                onmouseleave:move |_|{if let Some(hover)=wiki_hover {hover.leave_text();}},
                onmousemove: move |event| {
                    if let Some(hover)=wiki_hover {
                        let request=if ready && stamp.model.peek().revision==stamp.revision && !dragging() {
                            move_geometry.as_ref().and_then(|value|value.link_at(event.element_coordinates().x,event.element_coordinates().y)).and_then(|id|preview_targets(&hover_replacements).into_iter().find(|(key,target)|key==id&&matches!(target,crate::native_links::Target::Wiki{..}))).map(|(id,target)|crate::native_wiki_hover::Request{revision:stamp.revision,identity:format!("preview:{}:{id}",stamp.start),target})
                        } else {None};
                        let position=if ready && stamp.model.peek().revision==stamp.revision && !dragging() {move_geometry.as_ref().and_then(|value|value.box_at(event.element_coordinates().x,event.element_coordinates().y)).map(|rect|stamp.model.peek().engine.editor().line_index().char_offset_to_position(stamp.start+rect.start))} else {None};
                        hover.pointer(request,(event.client_coordinates().x,event.client_coordinates().y),position);
                    }
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
                            class: "mk-native-inline-widget {replacement.style.as_ref().map(|style| style.class()).unwrap_or_default()}",
                            "data-source-start": "{replacement.start}", "data-source-end": "{replacement.end}",
                            "data-heading-level": replacement.style.as_ref().and_then(|style| if let crate::native_decorations::InlineStyle::Heading(level, _) = style { Some(level.to_string()) } else { None }),
                            "data-block-preview": matches!(replacement.style, Some(crate::native_decorations::InlineStyle::Fenced { .. } | crate::native_decorations::InlineStyle::ListItem { .. } | crate::native_decorations::InlineStyle::Image(_) | crate::native_decorations::InlineStyle::Table(_) | crate::native_decorations::InlineStyle::Callout(_) | crate::native_decorations::InlineStyle::Documentation(_) | crate::native_decorations::InlineStyle::Math(crate::native_math_parser::Math{display:true,..}))).to_string(),
                            "data-code-language": replacement.style.as_ref().and_then(|style| if let crate::native_decorations::InlineStyle::Fenced { language, .. } = style { Some(language.clone()) } else { None }),
                            contenteditable: "false",
                            style: if replacement.widget.is_some() && replacement.style.is_none() { "display:inline; background:#dce8fa; border-radius:4px;" } else if matches!(replacement.style, Some(crate::native_decorations::InlineStyle::Fenced { .. } | crate::native_decorations::InlineStyle::ListItem { .. } | crate::native_decorations::InlineStyle::Image(_) | crate::native_decorations::InlineStyle::Table(_) | crate::native_decorations::InlineStyle::Callout(_) | crate::native_decorations::InlineStyle::Documentation(_) | crate::native_decorations::InlineStyle::Math(crate::native_math_parser::Math{display:true,..}))) { "display:inline-block; width:100%; max-width:100%; vertical-align:top; pointer-events:none;" } else if replacement.style.is_some() { "display:inline-block; max-width:100%; vertical-align:top; pointer-events:none;" } else { "display:inline;" },
                            if let Some(style @ (crate::native_decorations::InlineStyle::Nested(_) | crate::native_decorations::InlineStyle::Heading(_, _) | crate::native_decorations::InlineStyle::Link(_))) = &replacement.style {
                                for (fragment_index, fragment) in fragments(style).iter().enumerate() {
                                    MarkdownFragment { fragment: fragment.clone(), identity: format!("{}:{fragment_index}",replacement.start), resolved: fragment_resolved(fragment,&marks),onlayout:media_changed,onpointer:math_pointer }
                                }
                            } else if let Some(crate::native_decorations::InlineStyle::ListItem { depth, marker, checked, fragments }) = &replacement.style {
                                span {
                                    class: "mk-markdown-list-content", style: "padding-left: {*depth as f64 * 20.0}px;",
                                    "data-list-depth": "{depth}", "data-task-checked": checked.map(|checked| checked.to_string()),
                                    span { class: "mk-markdown-list-marker", "{marker} " }
                                    for (fragment_index, fragment) in fragments.iter().enumerate() {
                                        MarkdownFragment { fragment: fragment.clone(), identity: format!("{}:{fragment_index}",replacement.start), resolved: fragment_resolved(fragment,&marks),onlayout:media_changed,onpointer:math_pointer }
                                    }
                                }
                            } else if let Some(crate::native_decorations::InlineStyle::Documentation(doc)) = &replacement.style {
                                DocumentationView {doc:doc.clone(),start:replacement.start, onlayout:media_changed,onpointer:math_pointer}
                            } else if let Some(crate::native_decorations::InlineStyle::Callout(callout)) = &replacement.style {
                                CalloutView {key:"callout:{stamp.start}:{replacement.start}",callout:callout.clone(), start:replacement.start, marks:marks.clone(),onlayout:media_changed,onpointer:math_pointer}
                            } else if let Some(crate::native_decorations::InlineStyle::Table(table)) = &replacement.style {
                                div {class:"mk-table",role:"table",
                                    for (row_index, row) in table.rows.iter().enumerate() {
                                        div {class:"mk-table-row",role:"row",style:format!("grid-template-columns:repeat({},minmax(0,1fr));",table.alignments.len()),
                                            for (column, cell) in row.iter().enumerate() {
                                                div {class:"mk-table-cell",role:if row_index == 0 {"columnheader"} else {"cell"},style:format!("text-align:{};",table.alignments[column]),
                                                    for fragment_index in cell.clone() {
                                                        MarkdownFragment {fragment:table.fragments[fragment_index].clone(),identity:format!("{}:{fragment_index}",replacement.start),resolved:fragment_resolved(&table.fragments[fragment_index],&marks),onlayout:media_changed,onpointer:math_pointer}
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            } else if let Some(crate::native_decorations::InlineStyle::Math(math)) = &replacement.style {
                                crate::native_math::MathView {math:math.clone(),label:replacement.widget.clone().unwrap_or_default(),identity:format!("math:{}",replacement.start),onlayout:media_changed,onpointer:math_pointer}
                            } else if let Some(crate::native_decorations::InlineStyle::Image(image)) = &replacement.style {
                                crate::native_images::ImageBlock { image:image.clone(), identity:format!("image:{}",replacement.start), onlayout:media_changed }
                            } else if let Some(crate::native_decorations::InlineStyle::Fenced { fragments, .. }) = &replacement.style {
                                for fragment in fragments {
                                    span { class: "mk-markdown-code-fragment {fragment.class}", "{fragment.text}" }
                                }
                            } else {
                                {replacement.widget.clone().unwrap_or_default()}
                            }
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

fn fragments(
    style: &crate::native_decorations::InlineStyle,
) -> &[crate::native_decorations::InlineFragment] {
    use crate::native_decorations::InlineStyle;
    match style {
        InlineStyle::Nested(values) | InlineStyle::Heading(_, values) => values,
        InlineStyle::Link(values) => values,
        InlineStyle::ListItem { fragments, .. } => fragments,
        InlineStyle::Table(table) => &table.fragments,
        InlineStyle::Callout(callout) => &callout.fragments,
        InlineStyle::Documentation(doc) => &doc.fragments,
        _ => &[],
    }
}

pub(crate) fn preview_targets(
    values: &[crate::native_presentation::Replacement],
) -> Vec<(String, crate::native_links::Target)> {
    values
        .iter()
        .flat_map(|value| {
            value.style.as_ref().into_iter().flat_map(move |style| {
                fragments(style)
                    .iter()
                    .enumerate()
                    .filter_map(move |(index, fragment)| {
                        fragment
                            .link
                            .as_ref()
                            .map(|link| (format!("{}:{index}", value.start), link.target.clone()))
                    })
            })
        })
        .collect()
}

/// Resolve a keyboard follow action from canonical scalar coordinates, including
/// a replacement temporarily revealed by the caret. No DOM geometry is needed.
pub(crate) fn preview_target_at(
    values: &[crate::native_presentation::Replacement],
    offset: usize,
) -> Option<crate::native_links::Target> {
    values.iter().find_map(|value| {
        fragments(value.style.as_ref()?)
            .iter()
            .find_map(|fragment| {
                let link = fragment.link.as_ref()?;
                (value.start + link.range.start <= offset && offset < value.start + link.range.end)
                    .then(|| link.target.clone())
            })
    })
}

pub(crate) fn preview_image_at(
    values: &[crate::native_presentation::Replacement],
    offset: usize,
) -> Option<crate::native_image_parser::Image> {
    values.iter().find_map(|value| match &value.style {
        Some(crate::native_decorations::InlineStyle::Image(image))
            if value.start <= offset && offset < value.end =>
        {
            Some(image.clone())
        }
        _ => None,
    })
}

fn fragment_resolved(
    fragment: &crate::native_decorations::InlineFragment,
    marks: &[CellMarks],
) -> bool {
    match fragment.link.as_ref().map(|link| &link.target) {
        Some(crate::native_links::Target::Wiki { page, .. }) => {
            page.is_empty()
                || marks.iter().any(|mark| {
                    mark.wiki
                        .as_ref()
                        .is_some_and(|mark| &mark.target == page && mark.resolved)
                })
        }
        _ => true,
    }
}

#[component]
fn DocumentationCode(
    language: String,
    text: String,
    cached: Rc<std::cell::RefCell<Option<Vec<crate::native_decorations::CodeFragment>>>>,
) -> Element {
    let fragments = use_memo(use_reactive!(|language, text| {
        if let Some(fragments) = cached.borrow().as_ref() {
            return fragments.clone();
        }

        let result = crate::native_markdown::highlighted_code(&text, &language);

        match result {
            crate::native_decorations::InlineStyle::Fenced { fragments, .. } => {
                *cached.borrow_mut() = Some(fragments.clone());
                fragments
            }
            _ => Vec::new(),
        }
    }));
    rsx! {div {class:"mk-doc-code dxc", "data-code-language":language,
        for fragment in fragments.read().iter() {span {class:fragment.class.clone(),"{fragment.text}"}}
    }}
}

#[component]
fn DocumentationView(
    doc: Rc<crate::native_doc_comments::Document>,
    start: usize,
    onlayout: Callback<()>,
    onpointer: Callback<(String, MouseEvent)>,
) -> Element {
    use crate::native_doc_comments::Block;
    rsx! {
        div { class: "mk-doc-body", role: "document",
            for block in &doc.blocks {
                match block {
                    Block::Text { heading, quote, marker, fragments } => rsx! {
                        div { class: "mk-doc-row", "data-heading-level": heading.to_string(), "data-quote-depth": quote.to_string(),
                            "data-list-marker": marker.as_ref().map(|(_, marker)|marker.clone()),
                            style: format!("padding-left:{}px;",marker.as_ref().map_or(0,|(depth,_)|depth*16)+quote*12),
                            for index in fragments.clone() {
                                MarkdownFragment {fragment:doc.fragments[index].clone(),identity:format!("{start}:{index}"),resolved:true,onlayout,onpointer}
                            }
                        }
                    },
                    Block::Code { language, text, cached } => rsx! { DocumentationCode {language:language.clone(),text:text.clone(),cached:cached.clone()} },
                    Block::Table { alignments, rows } => rsx! {
                        div { class: "mk-table", role: "table",
                            for (row_index, row) in rows.iter().enumerate() {
                                div { class: "mk-table-row", role: "row", style: format!("grid-template-columns:repeat({},minmax(0,1fr));",alignments.len()),
                                    for (column, cell) in row.iter().enumerate() {
                                        div { class: "mk-table-cell", role: if row_index==0 {"columnheader"}else{"cell"}, style: format!("text-align:{};",alignments[column]),
                                            for index in cell.clone() {
                                                MarkdownFragment {fragment:doc.fragments[index].clone(),identity:format!("{start}:{index}"),resolved:true,onlayout,onpointer}
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}

#[component]
fn MarkdownFragment(
    fragment: crate::native_decorations::InlineFragment,
    identity: String,
    resolved: bool,
    onlayout: Callback<()>,
    onpointer: Callback<(String, MouseEvent)>,
) -> Element {
    let link = fragment.link.as_ref();
    rsx! { span {
        class: "mk-markdown-fragment", class: if link.is_some() { "mk-preview-link" },
        "data-preview-link": link.map(|_| identity.clone()),
        "data-link-resolved": link.map(|_| resolved.to_string()),
        title: link.map(|link| if link.title.is_empty() {link.target.description()} else {link.title.clone()}),
        "data-emphasis": fragment.emphasis.to_string(), "data-strong": fragment.strong.to_string(),
        "data-code": fragment.code.to_string(),
        if let Some(math)=fragment.math {crate::native_math::MathView {math,label:fragment.text,identity:format!("math:{identity}"),onlayout,onpointer}}
        else {"{fragment.text}"}
    } }
}

pub(crate) fn preview_images(
    values: &[crate::native_presentation::Replacement],
) -> Vec<(String, crate::native_image_parser::Image)> {
    values
        .iter()
        .filter_map(|value| match &value.style {
            Some(crate::native_decorations::InlineStyle::Image(image)) => {
                Some((format!("image:{}", value.start), image.clone()))
            }
            _ => None,
        })
        .collect()
}

#[cfg(feature = "markdown-preview")]
pub(crate) fn math_owners(
    values: &[crate::native_presentation::Replacement],
) -> Vec<serde_json::Value> {
    values.iter().flat_map(|value| {
        let mut owners=Vec::new();
        if let Some(crate::native_decorations::InlineStyle::Math(_))=&value.style {owners.push(serde_json::json!({"id":format!("math:{}",value.start),"label":value.widget,"start":value.start,"end":value.end}));}
        if let Some(style)=&value.style {
            owners.extend(fragments(style).iter().enumerate().filter(|(_,fragment)|fragment.math.is_some()).map(|(index,fragment)|serde_json::json!({"id":format!("math:{}:{index}",value.start),"label":fragment.text,"start":value.start,"end":value.end})));
        }
        owners
    }).collect()
}
#[cfg(not(feature = "markdown-preview"))]
pub(crate) fn math_owners(_: &[crate::native_presentation::Replacement]) -> Vec<serde_json::Value> {
    Vec::new()
}

#[component]
fn CalloutView(
    callout: Rc<crate::native_markdown_callouts::Callout>,
    start: usize,
    marks: Vec<CellMarks>,
    onlayout: Callback<()>,
    onpointer: Callback<(String, MouseEvent)>,
) -> Element {
    let mut open = use_signal(|| callout.fold.unwrap_or(true));
    let expanded = callout.fold.is_none() || open();
    use_effect(move || {
        let _ = open();
        onlayout.call(());
    });
    rsx! {
        div {class:"mk-callout", "data-callout-type":callout.kind.clone(), "data-callout-expanded":expanded.to_string(),
            div {class:"mk-callout-title",
                if callout.fold.is_some() {
                    button {class:"mk-callout-toggle", r#type:"button", "aria-expanded":expanded.to_string(), "aria-label":callout.fragments[callout.title.clone()].iter().map(|f|f.text.as_str()).collect::<String>(),
                        onmousedown:move |event|{event.prevent_default();event.stop_propagation();},
                        onkeydown:move |event|{if !moonkale_ext_api::keys::primary(&event.modifiers()) {event.stop_propagation();}},
                        onkeyup:move |event|event.stop_propagation(),
                        onclick:move |event|{event.prevent_default();event.stop_propagation();open.toggle();},
                    }
                }
                for index in callout.title.clone() {
                    MarkdownFragment {fragment:callout.fragments[index].clone(),identity:format!("{start}:{index}"),resolved:fragment_resolved(&callout.fragments[index],&marks),onlayout,onpointer}
                }
            }
            div {class:"mk-callout-body",hidden:!expanded,
                for row in &callout.rows {
                    div {class:"mk-callout-row", "data-quote-depth":row.quote_depth.to_string(),"data-heading-level":row.heading.to_string(),style:format!("padding-left:{}px;",row.quote_depth.saturating_mul(12)),
                        for index in row.fragments.clone() {
                            MarkdownFragment {fragment:callout.fragments[index].clone(),identity:format!("{start}:{index}"),resolved:fragment_resolved(&callout.fragments[index],&marks),onlayout,onpointer}
                        }
                    }
                }
            }
        }
    }
}
