//! # moonkale-project-fs
//!
//! "Open a folder" as a `Source`. Directories are `Directory` nodes, files
//! are `File` nodes with `ContentRef::Text`/`Blob`, containment is the edge.
//! Markdown wiki-links and code references are *not* extracted here — that
//! is `moonkale-index`'s job — this crate only knows about bytes and paths.
//!
//! The same trait, three very different backends:
//!
//! | platform | backend                                   | watch?        |
//! |----------|-------------------------------------------|---------------|
//! | desktop  | `std::fs` + `notify`                       | yes           |
//! | web      | OPFS (private) or File System Access API   | polling / no  |
//! |          | (Chromium only) or server-side folder      |               |
//! | mobile   | document picker → scoped directory         | no            |
//!
//! Large files: the source reports `len` and the editor decides whether to
//! open it (the old README's "discourage very large files" lives on as a
//! soft limit with a "open anyway" affordance).

pub mod platform;
pub mod source;
pub mod tree;
pub mod watch;
