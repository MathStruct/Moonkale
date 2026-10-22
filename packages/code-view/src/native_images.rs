//! Source-backed images: workspace cache owns bytes, scoped widgets own display.
use crate::native_image_parser::{Destination, Image};
use base64::Engine;
use dioxus::prelude::*;
use moonkale_core::{ContentRef, Node, NodeId, Query, SourceId, Version};
use moonkale_ext_api::{
    media::image::{self, MAX_BYTES, MAX_PIXELS},
    t, Workspace,
};
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    rc::Rc,
    time::Duration,
};

#[derive(Clone, Copy)]
pub(crate) struct ImageContext {
    pub ws: Workspace,
    pub node: NodeId,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Key {
    source: SourceId,
    node: NodeId,
    version: Version,
    epoch: u64,
}
#[derive(Clone)]
struct Ready {
    node: Node,
    url: Rc<str>,
}
type ResultSlot = Rc<RefCell<Option<Result<Rc<Ready>, String>>>>;
#[derive(Default)]
struct Cache {
    ready: HashMap<Key, Rc<Ready>>,
    order: VecDeque<Key>,
    bytes: usize,
    pending: HashMap<Key, ResultSlot>,
    active: usize,
    failures: HashMap<Key, String>,
    catalogs: HashMap<SourceId, (u64, Rc<Vec<Node>>)>,
}
const CACHE_BYTES: usize = 64 * 1024 * 1024;
#[derive(Clone, Default)]
struct Images(Rc<RefCell<Cache>>);
impl Cache {
    fn hit(&mut self, key: &Key) -> Option<Rc<Ready>> {
        let value = self.ready.get(key)?.clone();
        self.order.retain(|old| old != key);
        self.order.push_back(key.clone());
        Some(value)
    }
    fn put(&mut self, key: Key, value: Rc<Ready>) {
        self.put_with_budget(key, value, CACHE_BYTES);
    }
    fn put_with_budget(&mut self, key: Key, value: Rc<Ready>, budget: usize) {
        if let Some(old) = self.ready.remove(&key) {
            self.bytes -= old.url.len();
        }
        self.order.retain(|old| old != &key);
        while self.bytes + value.url.len() > budget {
            let Some(old) = self.order.pop_front() else {
                break;
            };
            if let Some(value) = self.ready.remove(&old) {
                self.bytes -= value.url.len();
            }
        }
        if value.url.len() <= budget {
            self.bytes += value.url.len();
            self.order.push_back(key.clone());
            self.ready.insert(key, value);
        }
    }
}
/// Resolve a Rich-view image through the same source jail, limits and cache
/// as native Markdown previews. Never fetch an external URL.
pub async fn source_image_url(ws: Workspace, from: &Node, destination: &str) -> Option<String> {
    if destination.len() > 4096 {
        return None;
    }
    let crate::native_links::Target::File { path, .. } =
        crate::native_links::Target::markdown(destination)?
    else {
        return None;
    };
    let cache = ws.shared_state(Images::default);
    let epoch = *ws.sources.graph_epoch.peek();
    let node = resolve(ws, from, &Destination::File(path), &cache, epoch).await?;
    load(ws, node, epoch, cache)
        .await
        .ok()
        .map(|image| image.url.to_string())
}

async fn resolve(
    ws: Workspace,
    from: &Node,
    destination: &Destination,
    cache: &Images,
    epoch: u64,
) -> Option<Node> {
    let (path, wiki) = match destination {
        Destination::File(path) => (path, false),
        Destination::Wiki(path) => (path, true),
    };
    let relative = crate::native_links::file_path(&from.native_key, path)?;
    if let Some(node) = ws
        .node_at_path(&from.source, &relative)
        .await
        .filter(image::is_image)
    {
        return Some(node);
    }
    if !wiki || path.starts_with('/') {
        return None;
    }
    let root = crate::native_links::file_path("", path)?;
    if root != relative {
        if let Some(node) = ws
            .node_at_path(&from.source, &root)
            .await
            .filter(image::is_image)
        {
            return Some(node);
        }
    }
    let cached = cache
        .0
        .borrow()
        .catalogs
        .get(&from.source)
        .filter(|(old, _)| *old == epoch)
        .map(|(_, nodes)| nodes.clone());
    let nodes = if let Some(nodes) = cached {
        nodes
    } else {
        let source = ws.source(&from.source)?;
        let query = Query::All {
            limit: 20_000,
            kinds: Some(vec![moonkale_core::NodeKind::File]),
        };
        let result = match source.query(query.clone()).await {
            Ok(result) => result,
            Err(_) => ws.index()?.source.query(query).await.ok()?,
        };
        let nodes = Rc::new(result.nodes);
        cache
            .0
            .borrow_mut()
            .catalogs
            .insert(from.source.clone(), (epoch, nodes.clone()));
        nodes
    };
    let explicit = path.contains('/');
    let filename = path.to_lowercase();
    let mut hits = nodes
        .iter()
        .filter(|node| node.source == from.source && image::is_image(node))
        .filter(|node| {
            if explicit {
                node.native_key.eq_ignore_ascii_case(&relative)
                    || node.native_key.eq_ignore_ascii_case(&root)
            } else {
                node.native_key
                    .rsplit('/')
                    .next()
                    .is_some_and(|name| name.to_lowercase() == filename)
            }
        })
        .collect::<Vec<_>>();
    hits.sort_by_key(|node| {
        let priority = if node.native_key.eq_ignore_ascii_case(&relative) {
            0
        } else if node.native_key.eq_ignore_ascii_case(&root) {
            1
        } else {
            2
        };
        (priority, node.native_key.len(), &node.native_key)
    });
    let node = hits.first()?;
    ws.node_at_path(&from.source, &node.native_key)
        .await
        .filter(image::is_image)
}
fn encoded(node: &Node, bytes: &[u8]) -> Result<Rc<str>, String> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err("size".into());
    }
    if image::dimensions(bytes).is_some_and(|(w, h)| w as u64 * h as u64 > MAX_PIXELS) {
        return Err("pixels".into());
    }
    Ok(Rc::from(format!(
        "data:{};base64,{}",
        image::mime_of(&node.native_key),
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )))
}
// Fetch tokens describe content; node versions can be cheap listing hints.
// Re-query the hint after reading instead of comparing these distinct tokens.
async fn fetch_image_bytes(
    source: &dyn moonkale_core::Source,
    node: &Node,
) -> Result<Vec<u8>, String> {
    let (bytes, _) = source
        .fetch_bytes(node.id)
        .await
        .map_err(|error| error.to_string())?;
    if node.version != Version(0) {
        let current = source
            .query(Query::Node(node.id))
            .await
            .map_err(|error| error.to_string())?;
        if !current
            .nodes
            .iter()
            .any(|n| n.id == node.id && n.version == node.version)
        {
            return Err("changed".into());
        }
    }
    Ok(bytes)
}

async fn load(ws: Workspace, node: Node, epoch: u64, images: Images) -> Result<Rc<Ready>, String> {
    if matches!(node.content,Some(ContentRef::Blob{len,..} | ContentRef::Text{len,..}) if len>MAX_BYTES)
    {
        return Err("size".into());
    }
    let key = Key {
        source: node.source.clone(),
        node: node.id,
        version: node.version,
        epoch: if node.version == Version(0) { epoch } else { 0 },
    };
    let failure_key = Key {
        epoch,
        ..key.clone()
    };
    let slot = {
        let mut cache = images.0.borrow_mut();
        if let Some(ready) = cache.hit(&key) {
            return Ok(ready);
        }
        if let Some(error) = cache.failures.get(&failure_key) {
            return Err(error.clone());
        }
        if let Some(slot) = cache.pending.get(&key) {
            slot.clone()
        } else {
            let slot: ResultSlot = Rc::new(RefCell::new(None));
            cache.pending.insert(key.clone(), slot.clone());
            let images = images.clone();
            let key = key.clone();
            let completed = slot.clone();
            dioxus::core::spawn_forever(async move {
                loop {
                    let acquired = {
                        let mut cache = images.0.borrow_mut();
                        if cache.active < 2 {
                            cache.active += 1;
                            true
                        } else {
                            false
                        }
                    };
                    if acquired {
                        break;
                    }
                    futures_timer::Delay::new(Duration::from_millis(10)).await;
                }
                let result: Result<Rc<Ready>, String> = async {
                    let source = ws
                        .source(&node.source)
                        .ok_or_else(|| "missing".to_string())?;
                    let bytes = fetch_image_bytes(source.as_ref(), &node).await?;
                    Ok(Rc::new(Ready {
                        url: encoded(&node, &bytes)?,
                        node,
                    }))
                }
                .await;
                let mut cache = images.0.borrow_mut();
                cache.active -= 1;
                cache.pending.remove(&key);
                match &result {
                    Ok(value) => cache.put(key.clone(), value.clone()),
                    Err(error) => {
                        if cache.failures.len() >= 128 {
                            cache.failures.clear();
                        }
                        cache.failures.insert(failure_key, error.clone());
                    }
                }
                *completed.borrow_mut() = Some(result);
            });
            slot
        }
    };
    loop {
        if let Some(result) = slot.borrow().clone() {
            return result;
        }
        futures_timer::Delay::new(Duration::from_millis(10)).await;
    }
}
#[component]
pub(crate) fn ImageBlock(image: Image, identity: String, onlayout: Callback<()>) -> Element {
    let context = try_use_context::<ImageContext>();
    let mut visible = use_signal(|| false);
    let mut failed = use_signal(|| false);
    let cache = use_hook(move || {
        context
            .map(|context| context.ws.shared_state(Images::default))
            .unwrap_or_default()
    });
    let destination = use_memo(use_reactive!(|image| image.destination));
    let origin = use_memo(move || {
        context
            .and_then(|context| context.ws.document(context.node))
            .map(|doc| {
                let node = &doc.read().node;
                (node.id, node.source.clone(), node.native_key.clone())
            })
    });
    let loaded = use_resource(move || {
        let destination = destination();
        let origin = origin();
        let visible = visible();
        let cache = cache.clone();
        let epoch = context.map_or(0, |context| *context.ws.sources.graph_epoch.read());
        async move {
            if !visible {
                return None;
            }
            let context = context?;
            let (id, _, _) = origin?;
            let from = context.ws.document(id)?.peek().node.clone();
            let node = resolve(context.ws, &from, &destination, &cache, epoch).await;
            Some(match node {
                Some(node) => load(context.ws, node, epoch, cache).await,
                None => Err("missing".into()),
            })
        }
    });
    let status = use_memo(move || {
        loaded
            .read()
            .as_ref()
            .and_then(|value| value.as_ref())
            .map(|result| {
                result
                    .as_ref()
                    .map(|value| (value.node.id, value.node.version, value.url.clone()))
                    .map_err(Clone::clone)
            })
    });
    use_effect(move || {
        let _ = status();
        failed.set(false);
        onlayout.call(());
    });
    let ws = context.map(|context| context.ws);
    let error = if failed() {
        Some("decode".to_string())
    } else {
        status().and_then(Result::err)
    };
    let message = match (ws, error.as_deref()) {
        (Some(ws), Some("size")) => t!(ws, crate::L, "editor-image-too-big"),
        (Some(ws), Some("pixels")) => t!(ws, crate::L, "editor-image-pixels"),
        (Some(ws), Some("missing")) => t!(ws, crate::L, "editor-image-missing"),
        (Some(ws), Some(_)) => t!(ws, crate::L, "editor-image-failed"),
        (Some(ws), None) => t!(ws, crate::L, "editor-image-loading"),
        _ => image.label(),
    };
    let ready = if failed() {
        None
    } else {
        status().and_then(Result::ok)
    };
    rsx! { div {
        class:"mk-image-block", "data-image-owner":identity.clone(), title:image.title.clone(),
        onmounted:move |event| {spawn(async move {if event.data().get_client_rect().await.is_ok_and(|rect|rect.size.width>0.0&&rect.size.height>0.0) {visible.set(true);}});},
        onresize:move |event| {if let Ok(size)=event.data().get_border_box_size() {visible.set(size.width>0.0&&size.height>0.0);}},
        if let Some((_,_,url))=ready {
            img {class:"mk-preview-image", "data-preview-image":identity.clone(), src:url.to_string(),alt:image.label(),
                onload:move |_|onlayout.call(()),
                onerror:move |_|{failed.set(true);onlayout.call(());}
            }
        } else {div {class:"mk-image-placeholder", "data-preview-image":identity.clone(),role:"img","aria-label":image.label(),"{message}"}}
    }}
}

pub(crate) fn use_follow(context: ImageContext) -> Callback<Image> {
    Callback::new(move |image: Image| {
        let ws = context.ws;
        let Some(doc) = ws.document(context.node) else {
            return;
        };
        if *ws.docs.active.peek() != Some(context.node) {
            return;
        }
        let from = doc.peek().node.clone();
        let text = doc.peek().text.clone();
        let session = ws.editor_session(context.node).expect("open image origin");
        let (revision, selection) = {
            let state = session.peek();
            (state.snapshot().revision, state.snapshot().selection)
        };
        let cache = ws.shared_state(Images::default);
        spawn(async move {
            let current = || {
                *ws.docs.active.peek() == Some(context.node)
                    && doc
                        .try_peek()
                        .is_ok_and(|doc| doc.node == from && doc.text == text)
                    && session.try_peek().is_ok_and(|state| {
                        state.snapshot().revision == revision
                            && state.snapshot().selection == selection
                    })
            };
            let epoch = *ws.sources.graph_epoch.peek();
            let target = resolve(ws, &from, &image.destination, &cache, epoch).await;
            if !current() {
                return;
            }
            if let Some(node) = target {
                let _ = ws.reveal_guarded(node, 0, 0, current).await;
            } else {
                let mut ws = ws;
                ws.set_status(t!(ws, crate::L, "editor-image-missing"));
            }
        });
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn node() -> Node {
        let source = SourceId::new("folder:/x");
        Node {
            id: NodeId::derive(&source, "pic.png"),
            source,
            kind: moonkale_core::NodeKind::File,
            label: "pic.png".into(),
            native_key: "pic.png".into(),
            content: None,
            props: Default::default(),
            version: Version(1),
        }
    }
    struct ImageSource {
        hint: Version,
        content: Version,
    }
    #[moonkale_core::async_trait]
    impl moonkale_core::Source for ImageSource {
        fn id(&self) -> SourceId {
            node().source
        }
        fn descriptor(&self) -> moonkale_core::SourceDescriptor {
            moonkale_core::SourceDescriptor {
                id: node().source,
                root: node().id,
                truth: Default::default(),
                display_name: "image".into(),
                family: moonkale_core::SourceFamily::Folder,
                capabilities: Default::default(),
            }
        }
        async fn query(
            &self,
            _: Query,
        ) -> Result<moonkale_core::QueryResult, moonkale_core::SourceError> {
            let mut node = node();
            node.version = self.hint;
            Ok(moonkale_core::QueryResult::single(node))
        }
        async fn fetch_text(
            &self,
            _: NodeId,
        ) -> Result<(String, Version), moonkale_core::SourceError> {
            Err(moonkale_core::SourceError::NotFound)
        }
        async fn fetch_bytes(
            &self,
            _: NodeId,
        ) -> Result<(Vec<u8>, Version), moonkale_core::SourceError> {
            Ok((b"image".to_vec(), self.content))
        }
        async fn apply(
            &self,
            _: moonkale_core::Transaction,
        ) -> Result<moonkale_core::Applied, moonkale_core::SourceError> {
            Err(moonkale_core::SourceError::NotFound)
        }
    }
    #[test]
    fn image_freshness_compares_listing_hints_instead_of_content_tokens() {
        use futures_util::FutureExt;
        let current = ImageSource {
            hint: Version(1),
            content: Version(99),
        };
        assert_eq!(
            fetch_image_bytes(&current, &node())
                .now_or_never()
                .unwrap()
                .unwrap(),
            b"image"
        );
        let changed = ImageSource {
            hint: Version(2),
            content: Version(1),
        };
        assert_eq!(
            fetch_image_bytes(&changed, &node())
                .now_or_never()
                .unwrap()
                .unwrap_err(),
            "changed"
        );
    }

    #[test]
    fn image_encoding_guards_headers_and_preserves_svg_as_data() {
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        bytes.extend(50_000u32.to_be_bytes());
        bytes.extend(40_000u32.to_be_bytes());
        assert!(encoded(&node(), &bytes).is_err());
        let mut svg = node();
        svg.native_key = "pic.svg".into();
        assert!(encoded(&svg, b"<svg/>")
            .unwrap()
            .starts_with("data:image/svg+xml;base64,"));
    }
    #[test]
    fn cache_touches_lru_and_replaces_versions_without_duplicate_budget() {
        let mut cache = Cache::default();
        let node = node();
        let key = Key {
            source: node.source.clone(),
            node: node.id,
            version: node.version,
            epoch: 0,
        };
        let value = Rc::new(Ready {
            node,
            url: Rc::from("data:small"),
        });
        cache.put(key.clone(), value.clone());
        cache.put(key.clone(), value);
        assert_eq!(cache.bytes, 10);
        assert!(cache.hit(&key).is_some());
        assert_eq!(cache.order.len(), 1);
    }
    #[test]
    fn cache_evicts_the_least_recent_image_and_refuses_entries_over_budget() {
        let mut cache = Cache::default();
        let make = |version| {
            let mut node = node();
            node.version = Version(version);
            let key = Key {
                source: node.source.clone(),
                node: node.id,
                version: node.version,
                epoch: 0,
            };
            (
                key,
                Rc::new(Ready {
                    node,
                    url: Rc::from("data:small"),
                }),
            )
        };
        let (a, av) = make(1);
        let (b, bv) = make(2);
        let (c, cv) = make(3);
        cache.put_with_budget(a.clone(), av, 20);
        cache.put_with_budget(b.clone(), bv, 20);
        assert!(cache.hit(&a).is_some());
        cache.put_with_budget(c.clone(), cv, 20);
        assert!(cache.hit(&b).is_none());
        assert!(cache.hit(&a).is_some());
        assert!(cache.hit(&c).is_some());
        assert_eq!(cache.bytes, 20);
        let (d, dv) = make(4);
        cache.put_with_budget(d.clone(), dv, 5);
        assert!(cache.hit(&d).is_none());
        assert_eq!(cache.bytes, 0);
    }
}
