use crate::links_panel::LinksPanel;
use crate::typst_preview::TypstPreviewPanel;
use dioxus::prelude::*;
use moonkale_ext_api::prelude::*;

pub const PANEL_ID: &str = "links";
pub const PREVIEW_ID: &str = "typst-preview";

pub struct LinksExtension;

impl Extension for LinksExtension {
    fn manifest(&self) -> Manifest {
        Manifest {
            id: "dev.moonkale.editor-markdown",
            name: "Markdown",
        }
    }

    fn panels(&self, ws: Workspace) -> Vec<PanelContribution> {
        let mut panels = vec![PanelContribution {
            id: PANEL_ID.into(),
            title: "Links".into(),
            home: PanelHome::Side,
            closable: false,
            dirty: false,
            node: None,
        }];
        // A preview tab exists while any .typ document is open and the platform can compile.
        let any_typ = ws
            .documents
            .read()
            .iter()
            .any(|(_, d)| d.read().node.native_key.ends_with(".typ"));
        if any_typ && ws.compile_typst().is_some() {
            panels.push(PanelContribution {
                id: PREVIEW_ID.into(),
                title: "Typst preview".into(),
                home: PanelHome::Main,
                closable: false,
                dirty: false,
                node: None,
            });
        }
        panels
    }

    fn render(&self, panel_id: &str, ws: Workspace) -> Element {
        if panel_id == PREVIEW_ID {
            rsx! { TypstPreviewPanel { ws } }
        } else {
            rsx! { LinksPanel { ws } }
        }
    }
}
