//! Optional plot extension: linked quick-look plots, source-pushed SQL aggregation (PLT-11),
//! declarative `*.plot.json` specifications (PLT-04), and standalone SVG export.
mod model;
mod panel;

/// The folder-relative path a `.plot.json` `source.path` may open, or `None`.
/// The path is normalized here (as the folder's `path` query does) and that
/// exact string is checked by the folder source's jailed lookup, which refuses
/// `..`, absolute paths and links leading outside. The caller opens only the
/// returned string, so the checked path and the opened path cannot differ.
/// (Not `Workspace::node_at_path`: on a refusal that falls back to walking the
/// tree, which lists outside links.)
pub async fn spec_source_inside(folder: &dyn moonkale_core::Source, rel: &str) -> Option<String> {
    let norm = rel
        .trim()
        .trim_start_matches("./")
        .trim_matches('/')
        .to_string();
    if norm.is_empty() {
        return None;
    }
    let found = folder
        .query(moonkale_core::Query::Text {
            dialect: "path".into(),
            text: norm.clone(),
        })
        .await
        .is_ok_and(|r| !r.nodes.is_empty());
    found.then_some(norm)
}
use dioxus::prelude::*;
use moonkale_ext_api::links::{LinkView, Subject};
use moonkale_ext_api::prelude::*;
use moonkale_ext_api::t;

static L: moonkale_ext_api::i18n::Locales = &[
    ("en", include_str!("../locales/en.ftl")),
    ("de", include_str!("../locales/de.ftl")),
    ("zh-CN", include_str!("../locales/zh-CN.ftl")),
];

pub const EDITOR_PREFIX: &str = "editor:";

/// `*.plot.json` documents belong to the plot extension.
pub fn is_plot_file(node: &Node) -> bool {
    node.native_key
        .to_ascii_lowercase()
        .ends_with(model::PLOT_SPEC_EXTENSION)
}

#[derive(Clone, Copy)]
pub(crate) struct PlotCommandSignal(pub Signal<(u64, &'static str)>);

pub struct PlotExtension;
impl Extension for PlotExtension {
    fn manifest(&self) -> Manifest {
        Manifest::optional(
            "dev.moonkale.plot",
            "Plot",
            "Quick-look plots of loaded table results, linked to rows and graph nodes.",
        )
        .with_permissions(&["write-files"])
    }
    fn locales(&self) -> moonkale_ext_api::i18n::Locales {
        L
    }
    fn claims(&self, node: &Node) -> Option<u8> {
        is_plot_file(node).then_some(50)
    }
    fn panels(&self, ws: Workspace) -> Vec<PanelContribution> {
        let mut out: Vec<PanelContribution> = ws
            .docs
            .open
            .read()
            .iter()
            .filter(|(_, d)| is_plot_file(&d.read().node))
            .map(|(id, doc)| {
                let d = doc.read();
                PanelContribution::new(
                    format!("{EDITOR_PREFIX}{id}"),
                    d.node.label.clone(),
                    PanelHome::Main,
                )
                .closable(true)
                .dirty(d.dirty())
                .node(*id)
            })
            .collect();
        out.extend(
            ws.links
                .views
                .read()
                .keys()
                .filter(|id| id.starts_with("plot:"))
                .map(|id| {
                    PanelContribution::new(id.clone(), t!(ws, L, "plot-title"), PanelHome::Main)
                        .closable(true)
                }),
        );
        out
    }
    fn commands(&self, ws: Workspace) -> Vec<CommandContribution> {
        vec![
            CommandContribution::new("plot.result", t!(ws, L, "plot-action")).key("Ctrl+Alt+D"),
            CommandContribution::new("plot.save", t!(ws, L, "plot-save")),
            CommandContribution::new("plot.export_svg", t!(ws, L, "plot-export-svg")),
        ]
    }
    fn result_commands(&self) -> Vec<&'static str> {
        vec!["plot.result"]
    }
    fn run_command(&self, id: &str, mut ws: Workspace) {
        if id == "plot.save" || id == "plot.export_svg" {
            let cmd = if id == "plot.save" {
                "plot.save"
            } else {
                "plot.export_svg"
            };
            let mut sig = ws
                .shared_state(|| PlotCommandSignal(Signal::new_in_scope((0, ""), ScopeId::ROOT)))
                .0;
            let next = sig.peek().0 + 1;
            sig.set((next, cmd));
            return;
        }
        if id != "plot.result" {
            return;
        }
        let target = ws
            .links
            .focused
            .peek()
            .clone()
            .filter(|p| {
                ws.link_subject(p).is_some_and(|s| {
                    ws.links
                        .results
                        .peek()
                        .get(&s.node)
                        .is_some_and(|r| r.table.is_some())
                })
            })
            .or_else(|| (*ws.docs.active.peek()).map(|n| ws.content_panel(n)));
        let Some(target) = target else {
            ws.set_status(t!(ws, L, "plot-empty"));
            return;
        };
        let Some(subject) = ws.link_subject(&target) else {
            return;
        };
        if !ws
            .links
            .results
            .peek()
            .get(&subject.node)
            .is_some_and(|r| r.table.is_some())
        {
            ws.set_status(t!(ws, L, "plot-empty"));
            return;
        }
        // One follower per originating table instance; repeat actions reopen it.
        let panel = format!(
            "plot:{}",
            moonkale_core::NodeId::from_content(target.as_bytes())
        );
        ws.links.views.with_mut(|views| {
            views.insert(
                panel.clone(),
                LinkView {
                    camera: None,
                    detached: Subject::new(subject.node),
                },
            );
        });
        ws.link_panel(&panel, &target, false);
        ws.show_panel(&panel);
        ws.focus_element(&format!("plot-canvas-{panel}"));
    }
    fn render(&self, panel_id: &str, ws: Workspace) -> Element {
        rsx! {panel::PlotPanel {ws,panel:panel_id.to_string()}}
    }
    fn on_panel_closed(&self, panel_id: &str, mut ws: Workspace) {
        if let Some(node) = panel_id
            .strip_prefix(EDITOR_PREFIX)
            .and_then(|s| s.parse::<NodeId>().ok())
        {
            ws.close_node(node);
            return;
        }
        ws.close_linked_panel(panel_id);
        ws.links.views.with_mut(|v| {
            v.remove(panel_id);
        });
    }
}
