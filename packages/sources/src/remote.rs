//! `RemoteSource` — a `Source` that talks to the `api` crate over server
//! functions / a websocket stream.
//!
//! The web build has exactly one factory: this. The desktop build also has
//! it, for "connect to a Moonkale server" (a team's shared databases). On
//! the server side, `api` holds real `moonkale-sources-*` sources and exposes
//! `query/fetch/apply/subscribe` per `SourceId` after auth.
