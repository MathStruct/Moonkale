//! `Registry` — every known extension and everything it contributes.
//!
//! Built at startup from (1) the `inventory` of static extensions and (2)
//! manifests found on disk / served by the server. Exposes typed lookups the
//! shell uses to build UI: `panels()`, `editors_for(NodeKind)`, `commands()`,
//! `language_for_path(..)`, `source_factories()`.
//!
//! Conflicts (two extensions claiming the same command id) are reported, not
//! silently resolved; the later one loses and gets a diagnostic in the
//! "Extensions" panel.
