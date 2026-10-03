//! The `Extension` trait.
//!
//! Static extensions implement this directly and are handed to the shell as
//! `Box<dyn Extension>`. `panels()` is called during the shell's render and
//! may read workspace signals — that is how an editor contributes one panel
//! per open document and how the tab's dirty dot updates.

use crate::{Manifest, PanelContribution, Workspace};
use dioxus::prelude::Element;

/// An extension: what it is ([`Manifest`]), the panels it contributes and how to render them, and optionally commands, settings, document claims, strings and file marks. Implemented by every built-in editor and panel; listed by a distribution.
pub trait Extension: 'static {
    /// Id, name, tier and permissions.
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

    /// Whether this extension is an editor for `node`, and how strongly
    /// (Milestone 18 phase 2): `None` = it does not open it. Of the enabled
    /// extensions that claim a node, the highest number wins its document
    /// tab; on a tie the user's choice decides
    /// ([`Workspace::preferred_editor`]). Built-ins use 10 for "any text"
    /// (the code editors) and 50 for one format (markdown, flow, images,
    /// tables). Default: `None`.
    fn claims(&self, _node: &moonkale_core::Node) -> Option<u8> {
        None
    }

    /// Block libraries for the flow editor (Milestone 6). Default: none.
    fn flow_libraries(&self) -> Vec<crate::flow::FlowLibrary> {
        Vec::new()
    }

    /// The extension's own settings, shown under its row in the Extensions
    /// panel (Milestone 13). `target` says which file a change goes to; use
    /// [`Workspace::update_settings_in`]. Default: none.
    fn settings(&self, _ws: Workspace, _target: crate::SettingsTarget) -> Option<Element> {
        None
    }

    /// The extension's strings per language (Milestone 18 phase 4.4, for
    /// spec 030): Fluent sources keyed by language tag, English required.
    /// Look up with [`crate::i18n::lookup`]. Default: none (the extension's
    /// text is still Rust literals).
    fn locales(&self) -> crate::i18n::Locales {
        &[]
    }

    /// Themes this extension brings (spec 030): JSON in the theme-file format
    /// (`{"name", "base": "dark" | "light", "tokens": {…}}`, see the shell's
    /// `theme` module). Default: none.
    fn themes(&self) -> Vec<String> {
        Vec::new()
    }
}

/// Which settings file a change goes to (the Extensions panel's switch).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsTarget {
    /// The user's settings (this machine, every folder).
    User,
    /// The open folder's `.moonkale/settings.json` (data, not authority: see `SettingsFile::without_authority`).
    Workspace,
}
