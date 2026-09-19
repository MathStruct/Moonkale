//! The `Extension` trait.
//!
//! Static extensions implement this directly and are handed to the shell as
//! `Box<dyn Extension>`. `panels()` is called during the shell's render and
//! may read workspace signals — that is how an editor contributes one panel
//! per open document and how the tab's dirty dot updates.

use crate::{Manifest, PanelContribution, Workspace};
use dioxus::prelude::Element;

pub trait Extension: 'static {
    fn manifest(&self) -> Manifest;

    /// The panels this extension currently contributes.
    fn panels(&self, ws: Workspace) -> Vec<PanelContribution>;

    /// Render one of them. Called inside the workbench; the returned element
    /// is remounted when the panel is docked elsewhere, so keep state in the
    /// workspace, not in the element.
    fn render(&self, panel_id: &str, ws: Workspace) -> Element;

    /// The shell tells the extension a closable panel was closed. Default:
    /// nothing.
    fn on_panel_closed(&self, _panel_id: &str, _ws: Workspace) {}

    /// Commands this extension offers to the palette, menus and keybindings
    /// (Milestone 7). Default: none. Ids are namespaced by convention
    /// (`git.commit`); the shell rejects duplicates by keeping the first.
    fn commands(&self, _ws: Workspace) -> Vec<crate::CommandContribution> {
        Vec::new()
    }

    /// Run one of them. Default: nothing.
    fn run_command(&self, _id: &str, _ws: Workspace) {}

    /// Block libraries for the flow editor (Milestone 6). Default: none.
    fn flow_libraries(&self) -> Vec<crate::flow::FlowLibrary> {
        Vec::new()
    }
}
