---
title: "Security — the trust model and the security document"
description: Who and what Moonkale trusts on each platform, and the skeleton of the project's security document. It covers secrets, connections, prompt injection, untrusted content and exploits, extensions, the supply chain and data at rest, plus the rules every change follows so security issues don't pile up. What is enforced where today, and the open findings.
tags: [architecture, security]
---
**Purpose (Daniel, 2026-10-14):** much of Moonkale runs internally today, so security isn't the priority yet. But a document should lay out the security of the whole project (how secrets are managed, whether it is safe from exploits and prompt injection, whether its connections are safe), and Moonkale should never reach the point where a large share of its issues are security issues. So this page is two things:
- the **skeleton of that document**, filled in where the facts exist and marked *to write* where they don't;
- the **rules every change follows from now on** (at the end), so the debt doesn't grow while the document waits.

The pieces are described where they were built ([[Remote and Server Modes]], [[LLM and RAG]], [[Extension System]], [[Agent Sessions and Profiles]]); this page puts them side by side. The open findings of the external audit are in [[Audit 2026-09-23]].

> [!warning] State on 2026-10-01
> Moonkale is a prototype. Eleven of the twenty audit issues are security findings; the structural part of #1, #4 and #8 is fixed on the `refactor` branch (Milestone 18 phase 4.5: the server and the user decide, not a client or a folder), the rest are open. Run a server only on loopback, behind SSH, or on a private network (WireGuard, Tailscale) until they are fixed.

## Boundaries

| boundary | what is trusted | enforced by | where in the code |
|---|---|---|---|
| **Server ↔ browser/desktop client** | whoever holds `MOONKALE_TOKEN` gets every file under `MOONKALE_ROOT` and, unless `MOONKALE_TERMINAL=0`, a shell as the server's user | cookie (HttpOnly, SameSite=Strict) or `Authorization: Bearer`; `/login` rate-limited; non-loopback bind refused without a token (`guard_bind`); TLS built in (`MOONKALE_TLS_CERT`/`_KEY`), plain HTTP off loopback refused unless `MOONKALE_INSECURE_HTTP=1`; Origin check on websocket upgrades; COOP/COEP | `server/src/auth.rs`, `web/src/main.rs` |
| **External agents (MCP)** | a separate bearer, `MOONKALE_MCP_TOKEN` | `server/src/mcp.rs` | — |
| **Paths** | everything below `MOONKALE_ROOT` (server) or the opened folder (desktop) | canonicalisation in `moonkale_server::open_any`, relative keys in `project-fs` | symlinks and Windows paths escape it (#5) |
| **Secrets** | names in settings, values never | resolved from `MOONKALE_SECRET_<NAME>`, the provider's usual variable, or `<config>/moonkale/secrets.json` (mode 600); web clients never see a value | `llm/src/secrets.rs`; an OS keychain (`keyring`) is planned, not built |
| **The agent** | a model that may propose any tool call | the policy gate: reads allowed, writes ask, destructive always ask; SQL statements classified; every call in the audit log | `llm/src/policy.rs`, `core/src/source/risk.rs` (`Source::classify`, overridable per source since Milestone 18); Cypher is a keyword scan, and LadybugDB runs any statement on its read-only database (#9) |
| **Extensions (wasm)** | a module with the permissions the user granted | JSON ABI v1: every host call checked against the grant; grants come from the user scope only (a folder cannot grant, Milestone 18 phase 4.5); an id belongs to the file that loaded it first, so a folder's module cannot squat a user-installed module's id and grants | `ext-host`; fuel per call (50 M for the manifest, 5 G per command run) and 256 MB of linear memory per instance, each call in a fresh store (audit #4) |
| **Extensions (static)** | compiled in; trusted like the rest of the binary | the tier (`core`/`optional`/`opt_in`) only decides what is on; the distribution's Cargo features decide what is in the binary | `moonkale_distribution::default_extensions()` |
| **A folder's own settings** | `.moonkale/settings.json` of a folder you open — **data, not authority** (Milestone 18 phase 4.5) | `SettingsFile::without_authority` before the merge: no language model or agent, no auto-approved writes, no embeddings switched on, no grants, no shell, no SSH hosts; it may switch things off and deny tools (added to the user's denials). Settings shows what was ignored | `ext-api/src/settings.rs`; test `a_folder_has_no_authority` |
| **SSH remote folders** | the system `ssh` and the user's keys; Moonkale never sees a password | the server binary uploaded per version, a per-session token typed over stdin with echo off, loopback-only port | `remote/src/session.rs`; temp names and the control socket (#18); the remote server is not checksummed yet |

## Assets and adversaries (to write in full)

- **What is worth protecting:**
  - the user's files and databases (confidentiality and integrity);
  - secrets (API keys, database passwords, SSH keys, which Moonkale never holds, and tokens);
  - the machine itself, since a terminal, LSP or agent can run code;
  - the user's session state and history;
  - licensed data (market data, whose republishing [[Trading with Databento]] rules out).
- **Who might attack:**
  - **a file or repository the user opens:** malicious content, settings or extensions;
  - **content an agent reads:** prompt injection;
  - **a network peer:** the server's clients, an SSH host, a presence hub, an MCP client;
  - **a dependency or a release artefact:** the supply chain;
  - **a co-tenant** on a shared server.

  Not in scope: an attacker with the user's own OS account.

## Secrets

**Today:**
- Only **names** are in settings, sessions and documents, never values.
- Values come from the environment (`MOONKALE_SECRET_<NAME>`, a provider's usual variable) or `<config>/moonkale/secrets.json` (mode 600).
- Web clients never see a value; the server resolves names for them, and only toward allowed endpoints (the LLM relay below).

**To build:**
- the OS keychain (`keyring`): Secret Service, Keychain, Credential Manager; Android Keystore;
- secrets scoped to a source or a session, not only global;
- redaction of secret values in logs, transcripts and history;
- a rotation story.

**To write:** a diagram of where each kind of secret lives on each platform.

## Connections

| Connection | Today | Gaps |
|---|---|---|
| Client ↔ server (web, desktop as client, phone) | a token as an HttpOnly cookie or bearer; TLS built in; plain HTTP off loopback refused; `/login` rate-limited | accounts and roles; a CSP; generalised rate limits; #20 (the batch of low findings) |
| Desktop ↔ remote folder (SSH) | the system `ssh` and the user's keys; a per-session token over stdin; a loopback-only port | #18: control socket in shared `/tmp`, port hijack, fixed upload temp name, unchecksummed server binary |
| Moonkale ↔ Git remotes | system Git, ssh-agent/credential helper/askpass; server-authorised `/api/git`, configured named remotes only | HTTPS or SSH off loopback; local files and loopback HTTP for fixtures/development; embedded URL credentials refused; no credential input, persistence or argument/output logging |
| Moonkale ↔ databases | drivers in-process; read-only opens; statement classification | per-source credentials through secrets; TLS for remote databases (Postgres, D1, Databento) checked, not optional |
| Moonkale ↔ LLM providers | the server decides endpoints (`MOONKALE_LLM_ENDPOINTS`); the client sends only names | provider TLS pinning not planned; request logging must redact |
| External agents ↔ Moonkale (MCP) | a separate bearer (`MOONKALE_MCP_TOKEN`) | scopes per token (read-only, one source) |
| Presence hub | — | #20 (presence impersonation) |
| Language servers, adapters, toolchains | spawned from discovery, never from a client value | the planned language-server and debug-adapter contribution points (KG-05, DBG) must keep that rule |

## Prompt injection

**The threat:** an agent reads files, web pages, database rows, issue text and tool output. Any of it can contain instructions ("ignore your rules, run this"). Models follow such text sometimes, so **content can never be trusted to stay data.**

**Today:**
- **The policy gate:** reads are allowed, writes ask, destructive calls always ask, and SQL statements are classified.
- **A folder can't grant permissions:** it cannot switch on an agent, auto-approve writes or name a shell.
- Every tool call is in the audit log.

**To build:**
- **Provenance in the context:** tool output and file content enter the model's context marked as *data from <source>*, never as instructions, and the UI shows which part of a proposal came from where.
- **Egress control:** an agent cannot send content to a URL it found in content without the user seeing the destination.
- **No chaining past the gate:** a read result can't trigger a write that skips approval, even through an MCP tool.
- **Separate trust for external MCP servers' tools,** by default lower than built-in ones.
- **A test suite of injection fixtures** (files, rows and pages carrying instructions) that the agent must not follow.

## Untrusted content and exploits

Moonkale renders and parses what it opens: Markdown (HTML inside it, KaTeX, Typst, Mermaid, TikZ), images and SVG, notebooks, flow documents, database files, RDF, DBN. **Today:**
- path jail (#5 open for symlinks and Windows paths);
- link-scheme allow-list (#10);
- bounded buffers (#14);
- folder settings without authority.
- **Built — fuzzing (WP-47, 2026-10-17):** `fuzz/` compiles the production math range parser and `Flow::parse` directly. Separate nightly CI jobs run each for 60 seconds with 64 KiB inputs, a 10-second per-input timeout and 2 GiB RSS limit; failures are non-blocking and crash inputs are uploaded for regression tests. The planned WP-36 delimiter normaliser does not exist yet; it and the wasm ABI JSON still need targets;

- **Parser crash containment (P-212, 2026-10-17):** Markdown presentation rejects NUL, vertical tab and form feed before pulldown-cmark construction, retaining original source on desktop and abort-profile wasm. Unwind builds also catch complete parse failures and discard partial results. Diagnostics are fixed, content-free text; rejected controls log once per process. Stable regressions exercise math, block/list/table/callout/image preview, wiki projection/hover/heading lookup, native front matter, documentation and agent messages; four corpus seeds retain the first CI crash and minimized variants. No parser upgrade is available; upstream #1129 remains open. This prevents the known crash family on abort builds, not arbitrary future parser panics. See [[P-212 Markdown offset iterator can panic]].

**To build:**
- **a Content-Security-Policy** for the web client and the webviews;
- **HTML sanitising** in the Markdown renderers, one allow-list;
- **SVG sanitising** (no scripts, no external references);
- **resource limits** for every renderer (Typst compile time, image size, Mermaid in a worker);
- **#15** (project-fs races: data loss).

## Extensions and the supply chain

**Extensions:**
- **Static ones** are compiled in and trusted like the binary. The distribution's features decide what's included.
- **Wasm modules** get only the permissions the user granted, and grants come from the user scope only. They run with **fuel** (50 M units for the manifest, 5 G per command run) and **256 MB of linear memory** per instance, each call in a fresh store, so a trap or an exhausted budget is an error for that call only. (This page said otherwise until 2026-10-14; `ext-host/src/runtime.rs` has had the limits since audit #4.) **To build:** a signed manifest for "Install from URL".

**Dependencies:**
- **Enforced in CI (WP-47, 2026-10-17):** `tools/check-security.py rust` runs pinned cargo-deny 0.20.2 (all workspace features/platforms, licences, RustSec, sources; duplicate versions warn) and cargo-audit 0.22.2 against `Cargo.lock`. New vulnerabilities, informational advisories and yanked crates fail the job. Both read **one dated exception list in `deny.toml`**: 25 existing vulnerabilities, 12 informational advisories and yanked `yoke-derive` 0.8.3 are deferred for Daniel's upgrade decision, not fixed. No dependency versions were changed. The HelixDB `helix-metrics` AGPL-3.0 exception records existing licence obligations; it does not approve adding AGPL code. Missing HelixDB crate licence metadata is clarified from its pinned root Apache-2.0 LICENSE; local Moonkale crate metadata inherits the existing MIT workspace licence.
- **JS:** `tools/check-security.py js` discovers every tracked npm/pnpm/yarn lockfile (and unignored local additions), audits npm production dependencies without installs/scripts, and fails on new high/critical advisories or registry errors. An unsupported lockfile format fails pending a pinned auditor. Six npm lockfiles are covered: CodeMirror, KaTeX, Milkdown, wasm-host, xterm and Quartz. `tools/npm-audit-exceptions.json` records 53 existing package/advisory pairs in Milkdown and Quartz with URLs, exact locked versions, dates and reasons; lower severities are reported too. The e2e environment has no committed lockfile: its existing runtime-created npm lock is audited immediately after installation. That install remains unpinned and needs a separately reviewed lockfile; no versions were upgraded in this work.
- **Vendored bytes:** `tools/check-vendored.py` is an offline blocking check of all 63 files under `packages/ext-api/assets/katex/` and any `vendor/` directory. `tools/vendored-assets.json` records SHA-256 per file, upstream versions/URLs and local patch provenance; additions, removals and changes fail until explicitly reviewed. KaTeX 0.18.7's 20 woff2 fonts match upstream, and its CSS matches after removing unused woff/truetype fallback URLs. The upstream tarball SHA-256 is recorded. The Rust forks intentionally include the patches documented beside them, so their reviewed local hashes are not represented as pristine upstream hashes. CI does not download vendored assets.
- **Local checks:** `python3 tools/check-security.py rust` (install the pinned tools first), `python3 tools/check-security.py js`, `python3 tools/check-vendored.py`, `python3 tools/test-security.py`. A writable RustSec cache can be supplied with `--db-root <directory>`; registry/advisory fetch failures fail closed. Only the checksum check and gate regression tests are fully offline. Updating a dependency requires revisiting its exceptions; never regenerate a baseline to hide a new finding.

**Releases:**
- **To build:** signed artefacts and published checksums, and a checksum for the server binary the SSH path uploads (#18).

## Data at rest

- **What is stored:** the state store (`state.sqlite`: settings, sessions (ADR-0025), layouts, the entity log, agent transcripts), caches (`.moonkale/cache/`, downloaded market data), and the index.
- **To decide:**
  - encryption at rest for transcripts and history (they may hold pasted secrets: redaction first);
  - file permissions on every platform;
  - what export and session bundles may contain (never secrets, never licensed data marked non-redistributable).

## The LLM relay (server)
Since Milestone 18 phase 4.5 the server decides what a client's provider settings may make it do (`moonkale_server::llm::server_side`, audit #1): the client picks the kind, the model and the secret's *name*; a secret goes only to the provider's built-in endpoint or one the operator listed in `MOONKALE_LLM_ENDPOINTS` (comma-separated URL prefixes), and `claude-code` runs `MOONKALE_CLAUDE_BIN` or `claude` on the server's `PATH`, never a path from the client. A refusal reaches the client as an error, not a silent fallback.

## Rules that follow from the audit
1. **The server decides.** Anything that turns into a process, a network request with a secret, or a permission is resolved from the server's own settings, never from a value the client sends (#1, #4, #8). This is the main design input to phase 4 of [[Milestone 18 - Library Refactor]].
2. **The workspace scope is data, not authority.** A folder may choose a theme, a layout or an editor; it may not name a command to run, grant a permission or point a secret at an endpoint (#8).
3. **Read-only means read-only for the engine too.** A driver opened "read-only" must also be unable to read outside its file (#2: DuckDB's `read_csv`, `glob`).
4. **Every buffer has a bound** (#14), every path goes through one jail function (#5), every link scheme through one allow-list (#10).

## Status by area (to keep current)

| Area | State | Open |
|---|---|---|
| Server boundary | ◐ token, TLS, loopback guard | accounts, CSP, #20 |
| Secrets | ◐ names only, env and file | keychain, redaction |
| SSH remote | ◐ | #18 |
| Agent and prompt injection | ◐ gate, classification, audit log | provenance, egress, injection tests |
| Untrusted content | ◐ jail, link allow-list, bounds; P-218 Rich images share native-preview source scoping, image limits and bounded cache, with display-only URLs and blocked remote/escaping destinations; default-off content normalizer has byte/work/parser limits; the Rust lossless planner also retains its depth limit, exact-input guards and move-token checks; single-document proposals recheck Files/write capability, source/session identity, backing version and exact live revision/text before one bounded in-memory batch (WP-66/67); content caches are bounded/checksummed; opt-in structural planning is pure, authenticated on web, capped at 4 MiB raw/512 KiB decoded input, two non-queued workers, and existing parser limits; source bytes are never logged or rendered as HTML; graph Rename reuses the existing LSP connection, rejects cross-source/resource operations and incomplete plans; two-pass target preparation, 64-file/4096-edit/4-MiB-payload/8-MiB-text caps, all-buffer staging and held/stamped editor input guard grouped history; explicit per-file saves report partial success; validates URI/range/version bounds and live identities before preview | #5, sanitising, CSP, fuzzing, #15 |
| Wasm extensions | ◐ permissions, fuel, memory limits | signing |
| Supply chain and releases | ◐ WP-47: blocking Rust/npm audits with dated baseline exceptions, offline vendored hashes; non-blocking parser fuzz smoke jobs | dependency upgrades, signing, release/SSH server checksums, wasm ABI fuzzing |
| Data at rest | ○ | redaction, encryption decision |

Legend: ◐ partly, ○ not started. Open GitHub issues today: #15, #18, #20.

## Rules for every change (from 2026-10-14)

They keep the debt from growing until the full document and a review exist. Each is one line in a review:
1. **The server decides** what becomes a process, a network request with a secret, or a permission (rule 1 above), and so do new contribution points (language servers, debug adapters, runners).
2. **No secret values** in settings, sessions, documents, logs, transcripts, test fixtures or error messages; names only.
3. **Content is data:** anything read from a file, a row, a page or tool output never becomes an instruction, a command line, a URL with a secret, or an approval.
4. **Every parser of untrusted input has a bound** (size, time, depth), and every new one gets a fuzz target before it ships enabled by default. WP-47 adds the first two nightly CI smoke targets with resource limits; this does not establish production bounds for every existing parser. Retain each crash as a regression in its normal crate tests before fixing it ([[How to Write Tests]]).
5. **Every new connection** has TLS off loopback, authenticates, and says in this page's table what it trusts.
6. **Every new dependency** passes the blocking Rust/JS security jobs above and has a reason in its commit. New CI tool installs have pinned versions; new actions have reviewed commit SHAs. Exceptions need an advisory ID (or exact yanked crate/version), date and reason; upgrades and licence decisions stay with Daniel.
7. **A security-relevant change updates this page** (the table above), and a found weakness is a GitHub issue labelled `security`, never only a comment.

## Not built
Accounts and roles for a shared server (design in [[Remote and Server Modes]]); a Content-Security-Policy; signed releases and a checksum for the uploaded remote server; an OS keychain.

Browser CI startup diagnostics contain only the owned process group’s executable names, process state, disk availability and progress timestamps. No environment or process argument dump is taken. Servers launch in a dedicated session; cleanup checks the recorded PID birth identity and signals only that process group, including orphaned descendants (P-209). Build logs/screenshots are retained for seven days.

WP-69 preview performance fixes retain at most three folding parser/tree/source entries per thread (one latest document per supported grammar). Incremental edits align byte endpoints to UTF-8 boundaries; switching documents is diffed against the exact retained source. The cache is derived editor state and adds no evaluation, file access, network authority, dependency or persistence.

Markdown highlighting may reuse captures for ASCII alphanumeric appends at EOF only when the unchanged prefix matches exactly, the last nonempty line is bounded to 4096 bytes of plain ASCII prose, and no retained capture touches that tail. Other edits parse normally, including syntax changes after retained appends. Canonical source always updates; parser/structure providers still receive it. Fresh-parser comparisons cover syntax, Unicode and earlier edits after a retained tail.

WP-23 (2026-10-17, review corrected) separates cheap folder query metadata hints from content save preconditions. Listings (`Children`, `ls`, `Node`, `path`, `Neighbours`) never open file contents; Unix hints include device/inode/ctime as well as mtime/length. Only content reads and known written text produce BLAKE3-based `Version(u64)` tokens, which are opaque and not cryptographic identities. Workspace state/history/settings writes fetch first and refuse failed reads; document/proposal preconditions already come from fetched text. Image freshness compares query hints with query hints. Fetch/save reads remain capped for growing files, special-file content is refused, and neighbourhoods cap nodes, entry collection and depth. Native rename exclusion on Linux/Android/macOS/Windows refuses unsupported operations; other platforms retain check-then-rename. Saves hold an OS file lock through handle revalidation and pathname identity checks. Unix writers ignoring advisory locks can still race after the final check (#15); parent/symlink swaps remain #5. No security issue is declared closed. Existing transitive BLAKE3 and same-file become direct dependencies for content hashes and portable identity checks. Index failures publish only a fixed content-free file diagnostic and clear stale derived/search data; no contents or OS error text are logged.


WP-18 Unison (2026-10-17), default-off: only the operator's `MOONKALE_UCM`/PATH selects an executable. A direct argument-vector spawn owns a loopback-only UCM 1.5 HTTP server with a fresh random token; proxying and redirects are disabled, no remote endpoints/Share commands or shell are exposed, and no token/process output is logged. Hash-only definition display avoids UCM's named attached-documentation evaluation path. The source submits only fixed read API routes. UCM itself may acquire its codebase lock and perform migrations/maintenance; Moonkale's read-only capabilities do not make UCM a read-only SQLite engine. Snapshot limits are 4,096 nodes / 65,536 edges / 64 namespace levels / 16 MiB aggregate successful API bytes / 16,384 requests / 60 seconds overall / five seconds per request / ten seconds startup, with serde_json recursion limits. An RAII guard kills/reaps UCM on completion/error; cancellation's blocking task remains bounded. Branch hash changes reject publication. No source text becomes HTML, a command or a URL authority. Existing reqwest (blocking, no default/TLS features), serde_json, Tokio and UUID crates provide loopback reads, bounded parsing, worker isolation and random tokens. Tests create bundled-only transcript fixtures and stop their own UCMs. See `packages/sources/unison/sources-unison.md` for limits and the per-branch snapshot/port-reservation limitations.

Pre-existing symlinks in `.unison`, `v2` and UCM's SQLite/lock files are refused. This is a pre-spawn check, not OS process confinement: concurrent pathname replacement remains the existing #5 boundary.

WP-26 portable CI (2026-10-17) uses SHA-pinned actions, read-only repository
permissions, OS/architecture-specific Rust caches and a 45-minute job timeout.
Windows/macOS tests start advisory; the existing blocking security gates remain
in force. No secrets, publishing or deployment permissions are supplied. Native
symlink fixture coverage remains Unix-only; Windows path jail checks run without
requiring elevated symlink privileges. See [[How to Write Tests]] for coverage.

P-224 graph transport keeps the existing serde JSON parser/wire format: the
panel sends its graph as a serde-escaped string through `Eval::send`, and the
view passes the string directly to the renderer. No graph text becomes script,
HTML or a URL. Numeric load profiling records timings only, never node contents.
The two renderer regression suites serve only the two committed engine files
from an ephemeral loopback test server with exact route matching and cleanup;
no production connection, dependency or parser is added.

P-218 Rich images reuse native-preview source resolution, path normalization,
image classification, byte/pixel limits, freshness checks, a two-reader queue
and bounded workspace cache. Display-only data URLs never replace document
paths. Node views request at most 128 images per mount, with 4096-byte paths;
remote destinations and source escapes receive no URL. Stale backend responses
are discarded. No new dependency, network fetch or parser is introduced.

WP-04/05 canvas components (2026-10-22) add numeric camera reports and a host-only
Dioxus component layer. Node content remains escaped Dioxus text/attributes;
`embed` does not introduce HTML, scripts or extension authority. Pointer capture
uses a fixed eval script with a serialized element id and pointer number. Bounds
reject non-finite camera/geometry data, and invisible/small cards unmount. The
shared crate reuses existing Dioxus/serde dependencies; no external connection,
parser, persistence or extension contribution point is introduced.

P-227 flow code generation now uses the fetched content version and text length together for replacements, and refuses failed reads. Query metadata versions never authorize generated-file overwrites; optimistic conflict checks remain enforced by the source.

WP-07 e-graph paste (2026-10-23) reuses serde JSON with its default recursion
limit and caps input at 4 MiB, 4096 nodes, 32768 edges and 4096 groups before
projection. Unique node/edge/group identities, member ownership, representatives
and child endpoints are validated. Rewrite walks use visited ids and adjacency
lists, including cycles. JSON text goes through `Eval::send` as data; labels
remain canvas/Dioxus text. There is no evaluation, connection, new dependency or
persistence. The `fuzz` workspace's `egraph` target exercises parsing, ancestry,
collapse and expansion, seeded by the synthetic snapshot. It is available for
local fuzzing; the existing two-target nightly jobs are unchanged.

WP-06 hierarchy projection (2026-10-23) reuses the structured serde query
protocol and existing source reads. It fetches no content bodies and adds no
connection, dependency or authority. The snapshot fallback refuses incomplete
results, duplicate ids, cycles and multiple parents of equal precedence; it caps input at two
million nodes/eight million edges, requested level at 64 and output at 100,000
nodes. Traversal is iterative, including malformed cyclic input. These are
projection bounds after source enumeration, not new limits on source-internal
allocation. Source-native cached cuts and end-to-end performance remain open.

WP-06 region navigation sends source query results as escaped JSON strings
through the existing Eval channel. The static controller never interpolates
labels into script or HTML. Source membership and complete-source counts stay
source-owned; only numeric placement, pinning, groups and boundary proxies are
computed in the host. The combined visible cut/region refuses more than 100k
nodes. Generation checks discard replies after source replacement. The lazy
million-node test source lives only in a separate fixture crate. No new
production parser, connection, permission or dependency is introduced.

P-229: Sophia coarse queries use namespace/name/definition-ID/indexed-reference
metadata instead of Interface::all's definition/Calls traversal, after the latter
exhausted a worker stack on the large local copy. Metadata limits remain two
million nodes/eight million edges, and log-position checks reject changes during
listing. No term decoder is added. This avoids that traversal in coarse queries;
existing All/neighbour/body traversal is not declared fixed or bounded by it.

WP-11/15 (2026-10-23): symbol paths are data parsed by Path2D, capped at 64 KiB
and 256 closed-region fills per draw. Grid drawing caps rows/columns and skips
distant zoom. Core date parsing caps text at 128 bytes and validates offset and
timestamp ranges. Numeric vectors cap at 4096 finite samples during construction
and streaming deserialization; DuckDB inspects borrowed list lengths before
allocating. Sparkline coordinates are normalized against extreme magnitudes.
Ref targets come from source-known NodeIds, never guessed UUID text or URLs;
opening reuses workspace source/editor behavior. New direct chrono dependency
reuses the locked version with std/serde only, no clock or connection.
The typed_cells and symbol_paths fuzz targets exercise production serde/date
and closed-region parsing with 64 KiB inputs; no new network, process or permission authority is added.


WP-21 naming prerequisite (2026-10-23) adds pure metadata extraction over the
existing bounded content-id syntax trees, with the same file/definition/byte
limits and request budget. Package discovery and file reads stay with callers.
Names with empty segments, whitespace, controls or backquotes bind nothing and
are reported; formatting never removes offending bytes. Store paths remain data,
with independent Sophia path-validation regression checks. Conflicting imports
are unresolved independent of read order; named collisions are explicit errors.
No normalizer/id change, dependency, evaluation, persistence or connection is added.


SR-145 content identities (2026-10-24): nesting has no depth cap. Explicit heap
stacks cover normalization, patterns, type parameters and Julia scope
continuations. Requests instead cap source bytes (16 MiB/file), definitions
(4,096/file), work (32 million default visits) and accumulated canonical/graph
bytes (256 MiB, charged before field allocation); parser progress still has a
two-second deadline. Julia modules retain their aggregate source/work budgets.
No dependency, code execution or new authority is added. See
`packages/services/content-id/content-id.md` for the full budget contract and
256 KiB-stack depth-100,000 regression coverage.


## WP-21 folder-cache boundary (2026-10-24)

Behind `sophia`, the index persists only the code it admits through the
ignore-aware folder Source: Rust, Python and Julia, no hidden/ignored files,
configuration or `.env`, at most 512 KiB each. A truncated 5,000-file walk or
64 MiB aggregate snapshot fails rather than reconciling incomplete input.
Cargo/Python naming metadata is read with a 64 KiB bound, not persisted.
Parser/normal-form budgets still apply. `.moonkale/` is excluded from the walk
and receives an ignore file. Cache directory/database/SQLite sidecar symlinks and code aliases are refused.

The store is enforced derived and the adapter advertises file truth and no
writes; application writes are refused before any op executes. Current-file
reads validate code paths, folder containment, UTF-8 and the same size cap,
compare source digests, and expose stale markers when falling back from stored
spans. It keeps no live derived-file connection between calls. Sophia owns
atomic per-file fact replacement, provenance and rollback; Moonkale neither
executes stored code nor directly edits Sophia's tables. Failed retries are
bounded (16 owners), and the remote refresh endpoint caps batches at 10,000 ids.

ADR-0029 history replay (2026-10-24) is an opt-in example/test behind `sophia`.
It reads Git object ids/blobs through argument-based Git subprocesses, without
shell interpolation, hooks, checkout or writes to the input repository. Scratch
must exist outside the canonical input path; snapshots admit only bounded
regular code/naming/ignore blobs, without symlinks or executable permissions. Sources still
pass through the normal ignore-aware selection and producer budgets. Git tree
paths cannot traverse outside scratch; cache/build paths are skipped. Temporary
snapshots/databases are cleaned on normal exit. Reports contain identifiers,
paths, counts, timings and errors, without source bodies. No production source,
write authority, dependency or network capability is added.


P-232's opt-in `query_profile` example reads disposable store copies and prints
only counts/timings. The existing graph profiling flag now splits Sophia's all
read from generic-call expansion. No stored values, text bodies, new dependency,
network access or Source write authority are added.

ADR-0030 (2026-10-26): the default folder cache retains the existing code/path
bounds. Repair never follows symlinks or replaces a readable authored store; a
broken database receives one repair attempt per open. Cache comparison uses a
temporary store under the checked `.moonkale/` directory and the real folder's
bounded naming metadata, cleans up on return and reports differing keys without
source bodies. Graph protocol envelopes are limited to 8 KiB and admit only
structural reads; user preferences travel per request. Local code-file reads
charge repeated neighbourhood traversal against an aggregate 10,000-item budget
and deduplicate/cap their combined edges. Cache checks write only
the derived cache, with files remaining the truth.


ADR-0030 folder truth (2026-10-27): shared detection bounds `truth.json` to 4 KiB,
requires a regular file and refuses symlinked marker/cache directories or invalid
metadata. Missing metadata means file truth. Store-truth writer handles retain
that authority and recheck on every writer entry point; cache sync/check/repair
stop before SQLite open or database/sidecar deletion, including unreadable
stores. The temporary read-only code-graph mode leaves plain file editing
available without feeding those edits into the authoritative store. No migration
or marker-creation command is supplied yet.

P-233 selection mapping adds read-only host protocols bounded to 8 KiB input.
Only an index’s attached cache can supply locations; authored and unrelated
stores cannot become file-edit targets. Manifest traversal caps at 5,000 files
and 10,000 spans, refusing overflow and ambiguous bodies. Locations select an
index node, never grant writes; existing input provenance, document revision,
proposal and rename guards still decide whether an edit can apply.


WP-22 (2026-10-28): cross-file resolution reads only admitted folder syntax facts.
It does not run code, load dependencies or follow new paths. Explicit imports,
unique targets and conservative shadow checks gate resolution; unsupported forms
remain unresolved. Sophia annotations are file-owned and reference target
manifest path/span/source hashes, so old annotations cannot acquire a new target
occupying an old span. Cache edge projection bounds annotations to 64 KiB each,
10,000 examined edges and the query's node cap. Content normalizer inputs,
store-truth protections and the existing snapshot/path bounds are unchanged.


Audit #18 follow-up (2026-10-28): the authenticated SSH master receives its local
forward through bounded multiplex commands before the token is sent. Failed
forwards retry fresh ports up to three attempts with a log for each retry and a
ten-second command deadline. Unwinding handshake panics publish a generic Failed
phase and terminate the master; panic payloads are not copied into UI messages.

WP-57 (2026-10-28): guarded extension failures retain only the extension id in
session state; the native panic hook suppresses guarded panic payloads before
logging or retaining local render details; the web panic hook replaces Dioxus's dev panic reporting and
passes no payload or error text to the overlay or server. Copy details writes a
fixed diagnostic locally. Safe mode neither grants permissions nor changes
persisted enablement/layout. Native startup health uses the existing local
state store; parsing its counter is capped at 128 bytes. Browser storage holds
only a pending flag and saturating count. Wasm invocation retries are bounded,
never automatic; browser Workers time out after 30 seconds and serialize per
module. Existing parser/fuel/memory/permission boundaries remain enforced.
No new network authority or package version is added; `js-sys` is reused by the
shell for its early wasm panic hook. Foreign compiled-in extensions must adopt
the guard helpers, and abort/OOM/stack overflow are outside unwind containment.


WP-64 (2026-10-28): key tips read shortcut attributes produced by the Rust
command registry and create text-only badge nodes. No content becomes HTML,
script, a URL or a command. The adapter adds no parser, dependency, persistence,
network connection or authority. Empty-tile hints use escaped localized text.

P-234 (2026-10-28): crash helpers heap-pin task inputs before constructing their
async wrappers, preventing inline future amplification on the wasm stack.
Owner checks, per-poll native unwind containment and payload-free web panic
handling remain in place. No parser, source write or network authority is added.

WP-64 second half (2026-10-28): shortcut search accepts one printable character
from the DOM adapter; command clicks are checked against the enabled registry
and its effective binding before nudge accounting. Records contain only a
command id key, local date, saturating count and shown flag in the existing
platform state store (SQLite on desktop). No document text, secret, source
write or network authority is added. Hints use `textContent`; cheat-sheet and
search labels use escaped Dioxus text. Focus adapters retain DOM references
only and leave editor/composition/terminal input with its existing owner.

Focus restoration accepts only real outer workbench panels and matching tab
buttons outside panel content. A nested content element imitating workbench
classes or `aria-labelledby` cannot cause an automatic click on another
control; the shortcut-discovery browser suite includes that regression.


WP-61 (2026-10-28): input adapters send only command/button ids and bounded
platform layout labels (256 entries, 32-byte codes, 64-byte labels). The input
monitor runs only in its dedicated Settings field; document and secret typing
are not retained. Browser side-button defaults are prevented even when unbound.
Rust checks command ids against the enabled registry. Keymap files are bundled
JSON data, with scoped explicit overrides; no document content becomes script,
HTML or a command. Session navigation retains at most 256 node ids or finite
seven-number camera states, is window-local and is not persisted. Camera restores
validate magnification/distance; host-created ids and numbers cross the adapter
as serde-escaped JSON. Background opens use the existing Source reads, identity
and guarded document paths, without new writes or connection authority. The
new input paths add no dependency or server permission.


WP-63 (2026-10-28): density and chrome choices carry UI preferences only;
accepting chrome never enables an extension or grants permissions. The shell
reads acceptance exclusively from user settings. The disclosure adapter sends
only a boolean close event after a button/outside click, uses generated ids,
and never reads content as HTML or commands. Existing Source, extension crash
and permission boundaries stay in effect; no dependency or connection is added.


WP-56 (2026-10-28): recovery is client-local, versioned and bounded by encoded
bytes. Only open folder-file documents are sampled. Hidden path components,
credential/key filenames and oversized buffers are excluded before copying
text. The read-only `recovery-path` source query additionally checks current
ignore rules and rejects every symlink component on a blocking worker; restore
uses the same admission. Unsupported sources opt out; no snapshot body is logged. Native SQLite I/O uses worker threads.
Source/path/base-version keys and exact base-text checks prevent stale recovery
from replacing newer source content; recovery never invokes a Source write.
Comparison text uses escaped Dioxus text nodes. Snapshot/clear operations are
serialized per workspace. Snapshots are unencrypted; automatic classification
of secrets pasted into eligible files is outside the path exclusion rule.
The Sophia byte cap rechecks file truth, symlink boundaries and the derived
flag before whole-cache eviction; authored stores remain protected. No new
package version, connection, execution or server authority is introduced.

WP-46 (validated 2026-10-28): session exchange is versioned JSON data capped
at 4 MiB with serde's nesting bound; no code is evaluated. Imported settings lose
authority fields before use, and import opens no sources. Exports contain
metadata and secret references, excluding buffer/conversation bodies. Session
soft deletion only writes state; it never calls Source writes. Recovery retains
WP-56 path/ignore/symlink checks and is namespaced per session. Local conversation
keys preserve the host's authorized folder/local prefix. Window messages are
confined by outer-session id; the session id is organization, not authentication.


WP-59 (validated 2026-10-29): link groups contain panel/node
identities and view metadata, never document text or secret values. Tab input
adapters check outer workbench tab elements and the current contributed panel
registry; payloads cannot directly execute a command or open a new source.
Labels use escaped text/DOM attributes. Graph selection inputs are capped at
1 MiB, 4,096 ids and 256 bytes per id, and admit only existing instances without
emitting an event back into the host. Session exchange retains its 4 MiB parser
bound. CodeMirror imports only its bundled asset URL; transient mount ownership
tokens prevent a retiring panel from destroying its replacement. No dependency,
Source write or network authority is added.


WP-60 overview (2026-10-29): previews perform no source reads, process starts,
network requests or document execution. Held text is escaped, bounded to a
2,048-character scan/eight 100-character lines, and excludes hidden/credential
paths using recovery's classifier. Image thumbnails copy only an already-decoded, path-admitted panel image into
a 256×120 canvas, without reading its URL or fetching content. The adapter
accepts a typed canonical UUID, not document text. Table samples cap rows/columns; terminal
relay tails cap bytes without consuming output. Graph snapshots cap dimensions,
node/edge scans and finite coordinates, with no GPU readback. The window-local
64-entry preview cache is revision/session scoped and never persisted. Lazy
visibility uses host-generated element ids as serialized data. The synchronous
4 ms extension budget is cooperative, not process confinement; existing crash
and extension authority boundaries remain in effect. Close uses existing panel
identities and requires explicit discard for dirty documents.


WP-60 follow-up (2026-10-29): the terminal relay incrementally discards ANSI
control sequences before retaining a 2,048-byte plain overview tail. Escape
parser state is constant-size across chunks, including unterminated strings;
preview reads never scan full replay history or consume live output. No new
execution, dependency, connection or Source authority is added.


## Git sync and conflict writes (WP-51, 2026-10-29)

The existing authenticated, same-origin `/api/git` endpoint now also accepts
typed sync and conflict operations. Git runs under the server's own identity;
clients cannot name a remote URL, supply credentials or alter argv. The server
selects a configured named remote and rejects fetch/push URLs with embedded
HTTP credentials, SSH passwords or query/fragment tokens, including expanded
URL rewrites. Off loopback, transport must be HTTPS or SSH; local file
remotes and loopback HTTP remain available. Credentials stay with ssh-agent, the configured credential helper
or the system's askpass UI; Moonkale has no credential input or persistence.
Argument/output tracing is absent. Progress is transient, capped at 1 MiB and
exposed only through the same authenticated jailed endpoint. Network commands
have a five-minute limit. Normal Git diagnostics are shown as inert text.

The server checks the Git repository root against `MOONKALE_ROOT` in addition
to the requested directory. Reviewed sync plans are rechecked before execution;
lease pushes name the exact observed upstream commit. Requests in a repository
are serialized in this process. Conflict writes validate relative literal
paths, refuse symlinks and `.git`, bound text/stages, recheck base/ours/theirs
and the current file, then atomically replace and stage. Conflict refreshes
stop at 100 files or 16 MiB of aggregate text. Git processes outside
Moonkale do not take its mutex: the final external-writer filesystem race
remains the same class of limitation documented for folder writes. The diff3
parser has a standalone production-source fuzz target (`git_conflicts`).
No account/role system is introduced: authorisation remains the host's token
session, or its existing loopback-only development mode.


## Private terminals and agent displays (WP-71 / WP-70, 2026-10-29)

Terminal input goes only to its owner’s PTY: xterm’s data/paste bridge or the
native key/paste handler → backend write → local PTY, or its authenticated
terminal websocket → server PTY. It never enters presence, link groups, the
entity log, recordings, previews or agent context. Terminal wire Debug omits
bodies. Owner replay and the visible screen retain output only. Trace extraction
is refused during a private prompt or Pause sharing. No live keyboard-grant mechanism exists;
a future grant must explicitly exclude terminals and end at private mode.

Unix hosts query the master’s termios (`tcgetattr` through portable-pty), treating
ICANON without ECHO as a private prompt. The server sends the mode separately
from output, polled at 100 ms; native and xterm views show a localized accessible
lock. Previews show only “Password prompt”, and paste waits for an explicit
choice. Per-terminal Pause sharing is in the overflow and palette. The bounded
ANSI-stripped relay tail excludes private/paused output and clears at both
boundaries. Agent terminal.run reads that same bounded tail; there is no tool
for reading an existing owner terminal or its input. No prompt text is parsed.

AU-22 remains a deliberate limit: raw-mode local applications and an ssh
session’s remote password cannot be distinguished by the local line discipline.
Input still has no sharing path; a remote program can echo sensitive output.
Pause sharing protects preview/agent output in that case. Non-Unix PTYs lack
this termios detector. Mode observation is sampled, not a classifier of arbitrary
output or secrets already printed by a process. Owner screens/replay are private
local state, not a follower/agent channel. Global position Stop sharing is implemented by WP-72; per-terminal Pause sharing remains a separate output control.

Display tools are read-only under the existing policy gate and support session
denials. Each conversation has a 100-attempt display budget and a two-second
minimum between admitted calls; invalid/rate-limited attempts also count.
Canvas inputs are typed Markdown/table/graph data: no HTML, CSS, script or
arbitrary style fields. Unknown fields are refused. Serialization stops at
64 KiB; tables cap at 200×32 cells, graphs at 200 unique nodes/1,000 known-endpoint
edges. Only theme styles are accepted. Canvas validation rejects parsed HTML events while accepting inert code blocks,
quotes and math; rendering uses the safe message renderer without remote images. Tables/graph labels are escaped data.
No display call invokes a source write or evaluates content. Markdown render
trees cap nesting at 64 and parser events at 8,192, falling back to inert
source beyond either bound.

Ask is the default; When I’m not typing queues an offer during recent input;
Always shows the marked view in an existing tile away from keyboard focus;
on a single-tile screen it stays in the background. Display calls preserve the active
document and keyboard focus, and never move/close a person’s tabs. Each view
has an attributed link group; selections are confined to its graph. At most
eight offers and 64 budget records are retained per workspace. New document
instances stop once an agent group has 16 members; reusable canvas/result panels
are scoped per conversation and agent profile. A focused agent table queues an
offer rather than resetting its controls. Legacy editor.open and implicit views
after approved writes use the same consent and focus rules. Server sessions retain at most eight ephemeral
display events, with the same validation/budget; each connected client applies
its own consent and current policy. Attribution is captured at turn start, so
changing profiles during a turn cannot relabel its displays; profiles have separate
groups but share the conversation budget. Events are absent from saved transcript
replay. Typing detection retains only an activity deadline, never keys/text.
Display bodies remain transient panel state; existing transcript/audit tools
still retain the model-supplied tool arguments as conversation data.

The direct nix, serde_json, futures-timer and pulldown-cmark dependencies reuse
locked packages;
no package version, new source authority or new network endpoint is added.

## Git everyday operations (WP-52, 2026-10-29)

The same authenticated jailed Git endpoint accepts typed branch, stash, blame,
undo and partial-index requests. It checks the repository top-level against
the jail, then runs relative to that top-level under its repository mutex.
Clients supply no argv, patches, remote addresses or credentials. Local branch
names are checked with `check-ref-format` and verified under `refs/heads`;
upstreams must be existing local/remote refs. Commits must be full hexadecimal
identities reachable from this repository's refs or reflog. Stashes are matched
by object identity before a current selector is used. Option-like refs and
escaping/metadata paths are rejected. Diff operations disable external diff
and textconv helpers. System Git hooks and signing keep the existing WP-51
trust boundary; repositories are executable developer inputs, not a sandbox.

Everyday mutations first return a review plan. Confirmation recomputes and
compares repository identity, HEAD, refs, full status and tracked diffs. Reset
reviews list commits leaving the branch and affected files, explain soft/mixed
preservation, and explicitly name hard reset's lost staged/unstaged edits and
possible obstructing untracked files. Delete refuses unmerged branches. Dirty
switching requires an explicit stash choice, including untracked files; failure
keeps the stash. Stash drop names its saved identity and loss of the reference.
No GC, expiry or pruning command is exposed.

Partial staging rereads the bounded (2 MiB) server-generated text diff and
requires an exact match before constructing selected hunks/lines. It never
applies a client-supplied patch, and changes only the index. Text review,
comparison and blame results are bounded to 2 MiB; UI lists cap refs at 1,000
and recovery entries at 200. Blame annotations are inert labels and hover text,
keyed by document identity and cleared on buffer edits. Historical blame never
applies historical positions to current editor text. Other Git processes do
not take Moonkale's mutex; the existing external-writer race remains.

P-237: global literal pathspec mode breaks Git 2.56's internal untracked-stash
cleanup. Only typed stash commands omit that global option; stash requests
accept no pathspecs. Credentials remain with the system as in WP-51. No new
dependency versions, network authority or persistent data store are added.

The enabled-by-default new Git parsers share production source with the
standalone `git_everyday` fuzz target. Each rejects text beyond 2 MiB;
selection also caps 100,000 row identities and uses a hash set for linear
membership work. Blame caps 100,000 output lines; recovery lists cap 200.
Annotations require the same clean document and content version before
publication and rendering, so asynchronous results cannot relabel changed text.

The server also canonicalizes and jails Git's metadata and common directories,
so a `.git` pointer in a jailed worktree cannot redirect writes outside the
allowed root. Linked worktrees whose common directory is outside that root are
refused; an operator must choose an allowed root containing both directories.
The browser regression uses a disposable outside repository and an inside
pointer to check refusal without writing any outside refs.

## Sophia rendered edit bases (P-238, SR-150)

Writable Sophia source versions identify the node, raw upstream token and
exact fetched rendering. The adapter retains up to 128 in-memory bases, at
most 2 MiB per base and 32 MiB per source. It never applies a splice to a
newer print merely because the upstream token stayed equal. A current raw
Sophia token must still match the retained base's token before writing;
concurrent definition/group changes conflict. Evicted unchanged prints can be
reconstructed by equality; changed evicted prints refuse and require reload.
The equality hash is not an authorisation or cryptographic identity. Bases
are neither persisted nor logged. Derived file versions and immutable content
remain unchanged. No dependency or new connection is added.

Git history retention overrides repository auto-maintenance for these commands
(`gc.auto=0`, `maintenance.auto=false`), with explicit no-prune fetch/pull flags
that override remote-specific pruning config. Regression tests preserve local
remote-tracking refs and tags after their remote counterparts disappear. Branch
switches refuse ignored obstructions (`--no-overwrite-ignore`). Hard reset
reviews also list target paths, naming possible ignored obstructions before
the explicit loss confirmation. System hooks and external Git processes retain
the already documented host trust boundary.

Linked worktrees share the common-directory mutex for ref/stash observations
and mutations. Their private metadata directory is not a separate lock scope;
this prevents in-process worktree actions from changing a reviewed stash's
selector while another operation executes. External writers remain outside
that mutex, as documented above.


## Position sharing (WP-72, 2026-10-29)

Cursor, graph pointer and demonstration agent cursor are explicit independent
window/session choices, off by default, never persisted. The always-mounted
title-bar disclosure owns the indicator and publication clock. Stop sharing
revokes all three immediately, clears pending samples and follow, and publishes
the revocation. Closing the folder and switching sessions also revoke them.
Terminal input/private prompts have no path into this protocol; terminal
Pause sharing continues to govern the separate preview/agent-output channel.

Payloads contain source/key identities, UTF-16 caret/selection offsets, finite
world coordinates and labels only. No buffer, terminal or prompt body is sent.
Credential/hidden paths use recovery's exclusion rule. Receivers render only
their own document in the matching source. Follow can read a newly visited
document through an already-open authorized Source, never open a new source or
receive the provider’s unsaved buffer. Follow reads admit at most one per second
and 32 per explicit follow, with 64 path components and the existing Source
content bounds. Session/participant/target guards are rechecked after reads so
Escape or local navigation prevents a pending result from opening a tab. Labels are escaped text; coordinates are bounded
numbers, never styles supplied by a participant. Pointers have no hit target.

The existing authenticated same-origin websocket hub and scoped local bus carry
the same metadata. Outer sessions are room organization, not authentication or
accounts. The hub binds updates to the socket's joining window, caps member
labels/keys, validates payloads and limits a room to 128 members. It stamps
positions with receipt time. Clients coalesce at 20 Hz, heartbeat at five seconds
and drop positions after fifteen seconds; last write wins. No position is stored
in the entity log, settings, transcript or recovery. A remote member's claimed
name remains a display label until SW-02 accounts exist. No keyboard/control
authority is introduced. Shared offsets refer to the receiver's current text;
version convergence/CRDT and 3D pointer depth are outside this slice.

The agent demonstration publishes only an approved reveal target after the
existing display/policy checks, with its own explicit sharing switch. It never
publishes prompt text, live keystrokes or proposal bodies. This adds no package
version, source write or endpoint; CodeMirror's rebuilt bundled adapter uses
existing decoration APIs.

Cursor announcements update only a core-owned, always-mounted live region on an explicit request. The adapter receives serialized metadata and uses textContent, clearing before a frame to support repeated announcements. Incoming movement never writes that region.

Transport backpressure keeps a single latest outgoing snapshot, so revocation replaces queued motion. The web link uses a bounded wake queue; native hub links use watch channels for outgoing state and incoming member lists. Application queues do not retain a movement history.
