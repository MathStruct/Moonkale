//! Permissions and capability tokens.
//!
//! The manifest declares what an extension *wants*; the user (or an org
//! policy) decides what it *gets*; the `Host` enforces it per call. A denied
//! call returns `ExtError::Denied(Capability)`, never a panic.
//!
//! `Capability` variants mirror the manifest's `[permissions]` table:
//! `SourcesRead, SourcesWrite, Network(UrlPattern), Process, Fs(PathPattern),
//! Llm, Clipboard, Secrets`.
//!
//! Static (compiled-in) extensions go through the same checks. Yes, even the
//! built-in editors — it keeps us honest that the API is sufficient.
