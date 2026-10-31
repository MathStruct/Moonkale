//! Optional quick-look plot extension. No source I/O: only bounded loaded results.
mod model;
mod panel;
use dioxus::prelude::*;
use moonkale_ext_api::links::{LinkView, Subject};
use moonkale_ext_api::prelude::*;
use moonkale_ext_api::t;

static L: moonkale_ext_api::i18n::Locales = &[
    ("en", include_str!("../locales/en.ftl")),
    ("de", include_str!("../locales/de.ftl")),
    ("zh-CN", include_str!("../locales/zh-CN.ftl")),
];

pub struct PlotExtension;
impl Extension for PlotExtension {
    fn manifest(&self) -> Manifest {
        Manifest::optional(
            "dev.moonkale.plot",
            "Plot",
            "Quick-look plots of loaded table results, linked to rows and graph nodes.",
        )
    }
    fn locales(&self) -> moonkale_ext_api::i18n::Locales {
        L
    }
    fn panels(&self, ws: Workspace) -> Vec<PanelContribution> {
        ws.links
            .views
            .read()
            .keys()
            .filter(|id| id.starts_with("plot:"))
            .map(|id| {
                PanelContribution::new(id.clone(), t!(ws, L, "plot-title"), PanelHome::Main)
                    .closable(true)
            })
            .collect()
    }
    fn commands(&self, ws: Workspace) -> Vec<CommandContribution> {
        vec![CommandContribution::new("plot.result", t!(ws, L, "plot-action")).key("Ctrl+Alt+D")]
    }
    fn result_commands(&self) -> Vec<&'static str> {
        vec!["plot.result"]
    }
    fn run_command(&self, id: &str, mut ws: Workspace) {
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
        ws.close_linked_panel(panel_id);
        ws.links.views.with_mut(|v| {
            v.remove(panel_id);
        });
    }
}
