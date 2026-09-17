//! File watching abstraction: `trait Watcher` with a `notify`-backed impl (native), a polling impl (web FSA), and a no-op impl (mobile). Debounced, coalesced into `SourceEvent`s.
