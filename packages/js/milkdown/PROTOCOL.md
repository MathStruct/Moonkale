# @moonkale/milkdown protocol

Messages Rust → JS (`apply`) and events JS → Rust, to be specified alongside
the first implementation. Every message here must be reproducible by a
Rust-native backend; that is the replacement contract.

- `mount(id, opts)`
- `apply(id, patch)`
- `destroy(id)`
- event kinds: TBD
