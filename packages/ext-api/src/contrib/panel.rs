//! `PanelContribution` — a dockable panel in the workbench.
//!
//! Fields: `id`, `title`, `home` (which tile it first appears in — `side`,
//! `main`, `bottom`, `right`), `icon`, `when` (visibility), `closable`,
//! `singleton` (one instance vs. one per node). Maps directly onto
//! `dioxus_workbench::Panel` in the shell.
