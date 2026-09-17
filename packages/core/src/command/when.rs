//! `WhenClause` — a tiny boolean expression language over UI context keys.
//!
//! Modelled on VS Code's `when` clauses because extension authors already
//! know them: `activePanel == 'editor-code' && lang == 'rust' && !readOnly`.
//!
//! Planned: a hand-written parser to an AST, evaluated against a
//! `ContextSnapshot` (string/bool/number keys published by the shell and by
//! extensions). Extensions may publish their own keys with a namespaced
//! prefix (`myext.someState`).
