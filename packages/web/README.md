# web

The `web` crate is two programs from one `main.rs`:
- the **browser client** (`dx serve` / `dx build --platform web`, feature `web`), and
- **`moonkale-server`** (`dx build --platform server`, feature `server`): the same server functions as the desktop's built-in server, plus TLS, the token gate and `--version` / `--port` / `--bind` / `--root` / `--token-stdin`.

Browser tests: [tests/e2e/README.md](tests/e2e/README.md). Where it fits: the vault's [Project Structure](../../markdown/architecture/Project%20Structure.md) and [Two Binaries](../../markdown/packaging/Two%20Binaries.md).

The same-origin BroadcastChannel bus carries outer-session-scoped window
catalogues and linked-panel changes. Incoming browser messages use the existing
4 MiB versioned session JSON parser before dispatch. Remote overview cards carry
metadata only; document/terminal/conversation previews stay in their owner.
