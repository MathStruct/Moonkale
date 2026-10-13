//! Scoped math presentation with shared, bounded markup and macro caches.
use crate::{native_images::ImageContext, native_math_parser::Math};
use dioxus::prelude::*;
use moonkale_core::{ContentRef, Query, SourceId};
use moonkale_ext_api::{
    media::math::{KATEX_DIR, KATEX_RENDERER},
    t, Workspace,
};
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    rc::Rc,
    time::Duration,
};
const BUDGET: usize = 8 * 1024 * 1024;
const MAX_CONFIG: usize = 64 * 1024;
#[derive(Clone, PartialEq, Eq)]
struct Config {
    macros: serde_json::Value,
    fingerprint: Rc<str>,
    error: bool,
}
impl Config {
    fn parse(text: Option<&str>) -> Self {
        let parsed = text.map(|text| {
            if text.len() > MAX_CONFIG {
                Err(())
            } else {
                serde_json::from_str::<serde_json::Value>(text).map_err(|_| ())
            }
        });
        let (macros, error) = match parsed {
            None => (serde_json::json!({}), false),
            Some(Ok(value)) => match value.get("macros") {
                None => (serde_json::json!({}), false),
                Some(macros) if macros.is_object() => (macros.clone(), false),
                _ => (serde_json::json!({}), true),
            },
            Some(Err(())) => (serde_json::json!({}), true),
        };
        let fingerprint = Rc::from(serde_json::to_string(&macros).unwrap_or_default());
        Self {
            macros,
            fingerprint,
            error,
        }
    }
}
#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    source: SourceId,
    math: Math,
    macros: Rc<str>,
}
#[derive(Clone, PartialEq, Eq)]
struct Rendered {
    html: Option<Rc<str>>,
    error: bool,
}
type Slot<T> = Rc<RefCell<Option<T>>>;
#[derive(Default)]
struct Cache {
    configs: HashMap<SourceId, (u64, Rc<Config>)>,
    config_pending: HashMap<(SourceId, u64), Slot<Rc<Config>>>,
    ready: HashMap<Key, Rc<Rendered>>,
    order: VecDeque<Key>,
    bytes: usize,
    pending: HashMap<Key, Slot<Rc<Rendered>>>,
}
#[derive(Clone, Default)]
struct Shared(Rc<RefCell<Cache>>);
impl Cache {
    fn hit(&mut self, key: &Key) -> Option<Rc<Rendered>> {
        let value = self.ready.get(key)?.clone();
        self.order.retain(|old| old != key);
        self.order.push_back(key.clone());
        Some(value)
    }
    fn put(&mut self, key: Key, value: Rc<Rendered>) {
        let size = value.html.as_ref().map_or(0, |html| html.len());
        if let Some(old) = self.ready.remove(&key) {
            self.bytes -= old.html.as_ref().map_or(0, |html| html.len());
        }
        self.order.retain(|old| old != &key);
        while self.bytes + size > BUDGET || self.order.len() >= 256 {
            let Some(old) = self.order.pop_front() else {
                break;
            };
            if let Some(value) = self.ready.remove(&old) {
                self.bytes -= value.html.as_ref().map_or(0, |html| html.len());
            }
        }
        if size <= BUDGET {
            self.bytes += size;
            self.order.push_back(key.clone());
            self.ready.insert(key, value);
        }
    }
}
async fn completed<T: Clone>(slot: Slot<T>) -> T {
    loop {
        if let Some(value) = slot.borrow().clone() {
            return value;
        }
        futures_timer::Delay::new(Duration::from_millis(10)).await;
    }
}
async fn config(ws: Workspace, source: SourceId, epoch: u64, cache: Shared) -> Rc<Config> {
    let handle = ws.source(&source);
    let slot = {
        let mut state = cache.0.borrow_mut();
        if let Some((old, value)) = state.configs.get(&source).filter(|(old, _)| *old == epoch) {
            let _ = old;
            return value.clone();
        }
        let key = (source.clone(), epoch);
        if let Some(slot) = state.config_pending.get(&key) {
            slot.clone()
        } else {
            let slot: Slot<Rc<Config>> = Rc::new(RefCell::new(None));
            state.config_pending.insert(key.clone(), slot.clone());
            let result = slot.clone();
            let cache = cache.clone();
            dioxus::core::spawn_forever(async move {
                let text=async {
                    let source=handle?;
                    let node=source.query(Query::Text{dialect:"path".into(),text:".moonkale/katex.json".into()}).await.ok()?.nodes.into_iter().next()?;
                    if matches!(node.content,Some(ContentRef::Text{len,..}|ContentRef::Blob{len,..}) if len>MAX_CONFIG as u64){return Some("invalid oversized config".to_string());}
                    source.fetch_text(node.id).await.ok().map(|(text,_)|text)
                }.await;
                let value = Rc::new(Config::parse(text.as_deref()));
                let mut state = cache.0.borrow_mut();
                state.config_pending.remove(&key);
                if state
                    .configs
                    .get(&source)
                    .is_none_or(|(old, _)| *old <= epoch)
                {
                    state.configs.insert(source, (epoch, value.clone()));
                }
                *result.borrow_mut() = Some(value);
            });
            slot
        }
    };
    completed(slot).await
}
#[derive(serde::Deserialize)]
struct Reply {
    html: Option<String>,
    error: bool,
}
async fn render(key: Key, config: Rc<Config>, cache: Shared) -> Rc<Rendered> {
    let slot = {
        let mut state = cache.0.borrow_mut();
        if let Some(value) = state.hit(&key) {
            return value;
        }
        if let Some(slot) = state.pending.get(&key) {
            slot.clone()
        } else {
            let slot: Slot<Rc<Rendered>> = Rc::new(RefCell::new(None));
            state.pending.insert(key.clone(), slot.clone());
            let result = slot.clone();
            let cache = cache.clone();
            dioxus::core::spawn_forever(async move {
                let mut eval = document::eval(include_str!("native_math_render.js"));
                let request = serde_json::json!({"url":KATEX_RENDERER.to_string(),"tex":key.math.tex,"display":key.math.display,"macros":config.macros});
                let reply = if eval.send(request).is_ok() {
                    eval.recv::<Reply>().await.ok()
                } else {
                    None
                };
                let value = Rc::new(match reply {
                    Some(Reply {
                        html: Some(html),
                        error: false,
                    }) if html.len() <= 1024 * 1024 => Rendered {
                        html: Some(Rc::from(html)),
                        error: false,
                    },
                    _ => Rendered {
                        html: None,
                        error: true,
                    },
                });
                let mut state = cache.0.borrow_mut();
                state.pending.remove(&key);
                state.put(key, value.clone());
                *result.borrow_mut() = Some(value);
            });
            slot
        }
    };
    completed(slot).await
}
#[component]
pub(crate) fn MathView(
    math: ReadSignal<Math>,
    label: ReadSignal<String>,
    identity: ReadSignal<String>,
    onlayout: Callback<()>,
    onpointer: Callback<(String, MouseEvent)>,
) -> Element {
    let context = try_use_context::<ImageContext>();
    let cache = use_hook(move || {
        context.map_or_else(Shared::default, |context| {
            context.ws.shared_state(Shared::default)
        })
    });
    let mut visible = use_signal(|| false);
    let origin = use_memo(move || {
        context
            .and_then(|context| context.ws.document(context.node))
            .map(|doc| doc.read().node.source.clone())
    });
    let config_cache = cache.clone();
    let settings = use_resource(move || {
        let source = origin();
        let visible = visible();
        let epoch = context.map_or(0, |context| *context.ws.sources.graph_epoch.read());
        let cache = config_cache.clone();
        async move {
            if !visible {
                return None;
            }
            let context = context?;
            Some(config(context.ws, source?, epoch, cache).await)
        }
    });
    let configuration = use_memo(move || settings.read().as_ref().and_then(Clone::clone));
    use_effect(move || {
        if configuration().is_some_and(|config| config.error) {
            if let Some(context) = context {
                let mut ws = context.ws;
                ws.set_status(t!(ws, crate::L, "editor-math-config"));
            }
        }
    });
    let wanted = use_memo(move || {
        Some(Key {
            source: origin()?,
            math: math(),
            macros: configuration()?.fingerprint.clone(),
        })
    });
    let rendering = use_resource(move || {
        let key = wanted();
        let config = configuration();
        let cache = cache.clone();
        async move {
            let key = key?;
            Some((key.clone(), render(key, config?, cache).await))
        }
    });
    let output = use_memo(move || {
        let wanted = wanted();
        rendering
            .read()
            .as_ref()
            .and_then(|result| result.as_ref())
            .filter(|(key, _)| Some(key) == wanted.as_ref())
            .cloned()
    });
    use_effect(move || {
        let _ = wanted();
        let _ = output();
        onlayout.call(());
    });
    let rendered = output().map(|(_, value)| value);
    let error = rendered.as_ref().is_some_and(|value| value.error);
    let title = if error {
        context.map_or_else(String::new, |context| {
            t!(context.ws, crate::L, "editor-math-failed")
        })
    } else {
        String::new()
    };
    rsx! {
        moonkale_ext_api::StylesheetUrl {href:format!("{KATEX_DIR}/katex.min.css")}
        span {class:"mk-math-owner",class:if math().display {"mk-math-display"},class:if error {"mk-math-error"},"data-math-owner":identity(),title,
            onmounted:move|event|{spawn(async move {if event.data().get_client_rect().await.is_ok_and(|rect|rect.size.width>0.0&&rect.size.height>0.0){visible.set(true);}});},
            onresize:move|event|{if let Ok(size)=event.data().get_border_box_size(){visible.set(size.width>0.0&&size.height>0.0);}},
            onscroll:move|event|{event.stop_propagation();onlayout.call(());},
            onmousedown:move|event|{if math().display {event.stop_propagation();onpointer.call((identity(),event));}},
            if let Some(html)=rendered.and_then(|value|value.html.clone()) {span {class:"mk-math-rendered",dangerous_inner_html:html.to_string()}}
            else {"{label}"}
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn macro_defaults_invalid_and_bounded_configuration() {
        assert!(!Config::parse(None).error);
        assert!(!Config::parse(Some("{}")).error);
        assert!(Config::parse(Some("broken")).error);
        assert!(Config::parse(Some("{\"macros\":[]} ")).error);
        assert!(Config::parse(Some(&"x".repeat(MAX_CONFIG + 1))).error);
        let config = Config::parse(Some("{\"macros\":{\"\\\\R\":\"\\\\mathbb{R}\"}}"));
        assert!(!config.error);
        assert_eq!(config.macros["\\R"], "\\mathbb{R}");
    }
    #[test]
    fn markup_cache_is_bounded_and_touches_lru() {
        let mut cache = Cache::default();
        let key = |n| Key {
            source: SourceId::new("test"),
            math: Math {
                tex: format!("{n}"),
                display: false,
            },
            macros: Rc::from("{}"),
        };
        let large = Rc::new(Rendered {
            html: Some(Rc::from("x".repeat(BUDGET / 2))),
            error: false,
        });
        cache.put(key(1), large.clone());
        cache.put(key(2), large.clone());
        assert!(cache.hit(&key(1)).is_some());
        cache.put(key(3), large);
        assert!(cache.hit(&key(2)).is_none());
        assert_eq!(cache.bytes, BUDGET);
        for n in 4..300 {
            cache.put(
                key(n),
                Rc::new(Rendered {
                    html: None,
                    error: true,
                }),
            );
        }
        assert!(cache.ready.len() <= 256);
        assert!(cache.bytes <= BUDGET);
    }
}
