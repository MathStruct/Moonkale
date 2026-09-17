//! Per-platform backends (design note).
//!
//! | platform | backend                                   | watch?        |
//! |----------|-------------------------------------------|---------------|
//! | desktop  | `std::fs` + `notify` — **this is `source.rs` today** | later |
//! | web      | OPFS (private) or File System Access API   | polling / no  |
//! |          | (Chromium only) or a server-side folder    |               |
//! | mobile   | document picker → scoped directory         | no            |
//!
//! Milestone 1 ships only the native backend, un-abstracted. When the web
//! backend arrives, `source.rs` splits into `trait Backend { read, write,
//! list, stat }` + `platform::{native, web, mobile}`. Introducing the trait
//! before there is a second implementation would be speculation.
