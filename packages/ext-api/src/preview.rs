//! Small, static overview schematics. These helpers never fetch or execute content.
use crate::Workspace;
use dioxus::prelude::*;
/// Reuse recovery’s credential and hidden-path exclusions without source reads.
pub use moonkale_state::recovery::allowed_path;

/// Requested CSS size; clamped independently of an extension's content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreviewSize {
    /// Width in CSS pixels (at most 512).
    pub width: u16,
    /// Height in CSS pixels (at most 256).
    pub height: u16,
}
impl PreviewSize {
    /// Clamp untrusted/oversized requests to a thumbnail.
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width: width.clamp(1, 512),
            height: height.clamp(1, 256),
        }
    }
}
/// Bound both scan work and output, preserving UTF-8 and line boundaries.
pub fn bounded_text(text: &str) -> String {
    text.chars()
        .take(2048)
        .collect::<String>()
        .lines()
        .take(8)
        .map(|s| s.chars().take(100).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}
/// Preview only already-held text from an admissible path. No source I/O,
/// subscriptions, parser, math renderer, command or embedded HTML is used.
pub fn document(ws: Workspace, panel: &str) -> Option<Element> {
    let node = ws
        .linked_content(panel)
        .or_else(|| panel.rsplit(':').next()?.parse().ok())?;
    let doc = ws.document(node)?;
    let doc = doc.peek();
    if !moonkale_state::recovery::allowed_path(&doc.node.native_key) {
        return None;
    }
    let text = bounded_text(&doc.text);
    Some(rsx! { pre { "{text}" } })
}
/// A bounded schematic from text already held by an extension.
pub fn text(text: &str) -> Option<Element> {
    let text = bounded_text(text);
    Some(rsx! { pre { "{text}" } })
}
/// Ask an existing renderer for its schematic; never create a second engine.
pub fn graph(canvas_id: String, flow: bool) -> Option<Element> {
    Some(rsx! { GraphSnapshot { canvas_id, flow } })
}
#[component]
fn GraphSnapshot(canvas_id: String, flow: bool) -> Element {
    let id = use_hook(|| format!("mk-overview-{}", canvas_id));
    let mount_id = id.clone();
    rsx! { canvas { id, width: 256, height: 120, onmounted: move |_| {
        let ids = serde_json::to_string(&(canvas_id.clone(), mount_id.clone(), flow)).unwrap_or_default();
        spawn(async move {
            let mut eval = document::eval(&format!(r#"
                const [id,target,flow]={ids};
                const canvas=document.getElementById(target);
                const deadline=performance.now()+750;
                while(canvas?.isConnected && performance.now()<deadline) {{
                    const view=(flow?window.moonkale?.flowViews:window.moonkale?.graphViews)?.[id];
                    if(view?.snapshot && view.node_count()>0) {{
                        canvas.getContext('2d').drawImage(view.snapshot(256,120),0,0);
                        canvas.dataset.snapshotNodes=String(view.node_count());
                        canvas.dataset.snapshotEdges=String(view.edge_count());
                        break;
                    }}
                    await new Promise(resolve=>setTimeout(resolve,25));
                }}
                if(canvas?.isConnected && !canvas.dataset.snapshotNodes) {{
                    const ctx=canvas.getContext('2d');
                    ctx.fillStyle=getComputedStyle(canvas).color;
                    ctx.font='24px sans-serif';ctx.fillText('◇',116,65);
                }}
                dioxus.send(true);
            "#));
            let _ = eval.recv::<bool>().await;
        });
    } } }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_limits_scan_lines_and_unicode_output() {
        let text = "中".repeat(100_000);
        assert_eq!(bounded_text(&text).chars().count(), 100);
        assert_eq!(bounded_text(&"line\n".repeat(100)).lines().count(), 8);
        assert_eq!(
            PreviewSize::new(u16::MAX, 0),
            PreviewSize {
                width: 512,
                height: 1
            }
        );
    }
}
