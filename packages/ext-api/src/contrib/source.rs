//! `SourceContribution` — a new kind of data source.
//!
//! Registers a `SourceFactory { family, dialect, connect(ConnectParams) }`
//! plus a *connection form* description so the shell can render "Connect to
//! TypeDB…" without the extension shipping UI. Native-only unless the
//! extension implements the source over HTTP (e.g. Supabase REST) in which
//! case it also works on web.
