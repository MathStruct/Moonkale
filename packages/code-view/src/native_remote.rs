//! Other participants' metadata projected against the local editor only.
use dioxus::prelude::*;
use moonkale_ext_api::presence::Member;

#[derive(Clone, PartialEq)]
pub(crate) struct Mark {
    pub name: String,
    pub anchor: usize,
    pub head: usize,
    pub color: usize,
}
pub(crate) fn marks(model: &crate::native_model::NativeModel, members: &[Member]) -> Vec<Mark> {
    members
        .iter()
        .filter_map(|member| {
            let cursor = member.sharing.as_ref()?.cursor.as_ref()?;
            Some(Mark {
                name: member.name.clone(),
                anchor: model.shared_offset(cursor.anchor)?,
                head: model.shared_offset(cursor.head)?,
                color: member.color(),
            })
        })
        .collect()
}
#[component]
pub(crate) fn Caret(name: String, color: usize, x: f64, y: f64, height: f64) -> Element {
    rsx! { span { class: "mk-remote-caret", "data-color": color.to_string(), "data-participant": name.clone(), title: name.clone(),
        style: "left:{x}px;top:{y}px;height:{height}px;",
        span { class: "mk-remote-name", "{name}" }
    } }
}
