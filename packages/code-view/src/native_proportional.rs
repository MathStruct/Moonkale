//! Isolated, bounded proportional-layout fixture; production code stays on
//! its virtualized monospace path until this view geometry is integrated.
use crate::{
    editor_core_spike::{move_cursor, place_caret_at},
    native_browser_geometry::{self, Geometry},
};
use dioxus::prelude::*;
use editor_core::{Command, CursorCommand, EditCommand, EditorStateManager, Position};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

/// Fixture-only proportional layout and Rust-owned editing probe.
#[component]
pub fn ProportionalGeometryProbe() -> Element {
    let mut engine = use_signal(|| {
        EditorStateManager::new(
            "Wi iii WWW 😀 e\u{301} 中\tTabs and proportional text wrap by pixels. "
                .repeat(4)
                .as_str(),
            80,
        )
    });
    let text = use_memo(move || engine.read().editor().get_text());
    let id = use_hook(|| format!("mk-proportional-{}", dioxus::core::current_scope_id().0));
    let mut geometry = use_signal(|| None::<(String, Geometry, u64)>);
    let mut font_epoch = use_signal(|| 0u64);
    let mut font_loading = use_signal(|| false);
    let mut dragging = use_signal(|| false);
    let mut limit_reached = use_signal(|| false);
    let mut input_epoch = use_signal(|| 0u64);
    let mut narrow = use_signal(|| false);
    let mut delayed = use_signal(|| false);
    let requests = use_hook(|| Rc::new(Cell::new(0u64)));
    let input_node = use_hook(|| Rc::new(RefCell::new(None::<Rc<MountedData>>)));
    let measure = {
        let id = id.clone();
        let requests = requests.clone();
        Callback::new(move |_: ()| {
            if *font_loading.peek() {
                return;
            }
            let epoch = *font_epoch.peek();
            let source = text.peek().clone();
            let request = requests.get().wrapping_add(1);
            requests.set(request);
            let delay = *delayed.peek();
            let id = id.clone();
            let requests = requests.clone();
            spawn(async move {
                let result = native_browser_geometry::measure(&id, &source).await;
                if delay {
                    futures_timer::Delay::new(std::time::Duration::from_millis(300)).await;
                }
                if geometry.try_peek().is_err()
                    || requests.get() != request
                    || *font_epoch.peek() != epoch
                    || *font_loading.peek()
                {
                    return;
                }
                let Ok(current) = text.try_peek() else {
                    return;
                };
                if *current != source {
                    return;
                }
                drop(current);
                if let Some(result) = result {
                    geometry.set(Some((source, result, epoch)));
                }
            });
        })
    };
    use_effect(move || {
        let _ = text();
        let _ = narrow();
        let _ = delayed();
        let _ = font_epoch();
        let _ = font_loading();
        measure.call(());
    });
    let cursor = engine.read().get_cursor_state().offset;
    let layout = geometry
        .read()
        .as_ref()
        .filter(|(source, _, epoch)| *source == text() && *epoch == font_epoch() && !font_loading())
        .map(|(_, value, _)| value.clone());
    let can_hit = layout
        .as_ref()
        .is_some_and(|value| value.width == if narrow() { 230.0 } else { 600.0 });
    let caret = layout.as_ref().and_then(|value| value.caret(cursor));
    let drag_layout = layout.clone();
    let selection_boxes = engine.with(|editor| {
        let selected = editor.get_cursor_state().selection;
        let Some(selection) = selected else {
            return Vec::new();
        };
        let index = editor.editor().line_index();
        let a = index.position_to_char_offset(selection.start.line, selection.start.column);
        let b = index.position_to_char_offset(selection.end.line, selection.end.column);
        layout
            .as_ref()
            .map(|geometry| {
                geometry
                    .boxes
                    .iter()
                    .filter(|rect| rect.start < a.max(b) && rect.end > a.min(b))
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    });
    let key_input = input_node.clone();
    rsx! {
        div {
            class: "proportional-probe",
            "data-ready": can_hit.to_string(),
            "data-cursor": cursor.to_string(),
            "data-width": layout.as_ref().map(|value| value.width.to_string()).unwrap_or_default(),
            "data-height": layout.as_ref().map(|value| value.height.to_string()).unwrap_or_default(),
            button { id: "proportional-width", onclick: move |_| narrow.toggle(), "Resize proportional probe" }
            button { id: "proportional-delay", onclick: move |_| delayed.toggle(), "Delay geometry" }
            button { id: "proportional-reset", onclick: move |_| { engine.set(EditorStateManager::new("Wi 😀 e\u{301} 中\tTabs", 80)); }, "Reset probe" }
            div {
                style: if narrow() { "position:relative; width:230px;" } else { "position:relative; width:600px;" },
                div {
                    id: id.clone(), class: "proportional-source",
                    style: "font: 24px/32px serif; white-space:pre-wrap; overflow-wrap:anywhere; tab-size:4; direction:ltr; user-select:none;",
                    onmounted: {
                        let id=id.clone();
                        move |_| {
                            let id=id.clone();
                            spawn(async move { native_browser_geometry::watch_fonts(&id,Callback::new(move |notice:native_browser_geometry::FontNotice| {
                                if font_epoch.try_peek().is_err() || font_loading.try_peek().is_err() { return; }
                                if *font_epoch.peek()!=notice.epoch { font_epoch.set(notice.epoch); }
                                if *font_loading.peek()!=notice.loading { font_loading.set(notice.loading); }
                            })).await; });
                            measure.call(());
                        }
                    },
                    onresize: move |event| {
                        let unchanged=event.data().get_border_box_size().ok().is_some_and(|size| geometry.peek().as_ref().is_some_and(|(_,layout,_)| (layout.width-size.width).abs()<0.000001 && (layout.height-size.height).abs()<0.000001));
                        if !unchanged { geometry.set(None);measure.call(()); }
                    },
                    onmousedown: move |event| {
                        event.prevent_default();
                        if !can_hit { return; }
                        dragging.set(true);
                        if let Some(offset) = layout.as_ref().and_then(|value| value.hit(event.element_coordinates().x, event.element_coordinates().y)) {
                            engine.with_mut(|editor| {
                                let (line, column) = editor.editor().line_index().char_offset_to_position(offset);
                                place_caret_at(editor, Position::new(line, column));
                            });
                            if let Some(input) = key_input.borrow().clone() { spawn(async move { let _ = input.set_focus(true).await; }); }
                        }
                    },
                    onmousemove: move |event| {
                        if !dragging() || !can_hit || !event.held_buttons().contains(dioxus::html::input_data::MouseButton::Primary) { return; }
                        if let Some(offset) = drag_layout.as_ref().and_then(|value| value.hit(event.element_coordinates().x, event.element_coordinates().y)) {
                            engine.with_mut(|editor| {
                                let (line, column) = editor.editor().line_index().char_offset_to_position(offset);
                                let _ = editor.execute(Command::Cursor(CursorCommand::ExtendSelection { to: Position::new(line, column) }));
                            });
                        }
                    },
                    onmouseup: move |_| dragging.set(false),
                    onmouseleave: move |_| dragging.set(false),
                    "{text}"
                }
                for rect in selection_boxes {
                    span { class: "proportional-selection", style: "position:absolute; pointer-events:none; left:{rect.x}px; top:{rect.y}px; width:{rect.width}px; height:{rect.height}px; background:rgba(80,140,220,0.25);" }
                }
                if let Some((x, y, height)) = caret {
                    span { class: "proportional-caret", style: "position:absolute; pointer-events:none; left:{x}px; top:{y}px; height:{height}px; border-left:1px solid black;" }
                }
            }
            for epoch in [input_epoch()] {
            textarea {
                key: "{epoch}",
                class: "proportional-input", aria_label: "Proportional geometry test input",
                style: "position:absolute; width:1px; height:1px; opacity:0;",
                value: "",
                onmounted: {
                    let input_node = input_node.clone();
                    move |event: MountedEvent| {
                        *input_node.borrow_mut() = Some(event.data.clone());
                        if input_epoch() > 0 { spawn(async move { let _ = event.data.set_focus(true).await; }); }
                    }
                },
                oninput: move |event| { let value = event.value(); if !value.is_empty() {
                    if text.peek().chars().count() + value.chars().count() > 4096 { limit_reached.set(true); input_epoch.with_mut(|value| *value += 1); return; }
                    limit_reached.set(false);
                    engine.with_mut(|editor| { let _ = editor.execute(Command::Edit(EditCommand::InsertText { text: value })); }); input_epoch.with_mut(|value| *value += 1); } },
                onkeydown: move |event| {
                    let primary = moonkale_ext_api::keys::primary(&event.modifiers());
                    let handled = engine.with_mut(|editor| {
                        if primary && event.key() == Key::Character("z".into()) {
                            let _ = editor.execute(Command::Edit(if event.modifiers().contains(Modifiers::SHIFT) { EditCommand::Redo } else { EditCommand::Undo })); return true;
                        }
                        let command = match event.key() {
                            Key::ArrowLeft => Some(CursorCommand::MoveGraphemeLeft),
                            Key::ArrowRight => Some(CursorCommand::MoveGraphemeRight),
                            Key::Home => Some(CursorCommand::MoveToLineStart),
                            Key::End => Some(CursorCommand::MoveToLineEnd),
                            _ => None,
                        };
                        if let Some(command) = command { move_cursor(editor, command, event.modifiers().contains(Modifiers::SHIFT)); return true; }
                        false
                    });
                    if handled { event.prevent_default(); }
                },
            }
            }
            if limit_reached() { p { role: "status", "This geometry probe is limited to 4096 source characters." } }
            pre { class: "proportional-canonical", "{text}" }
        }
    }
}
