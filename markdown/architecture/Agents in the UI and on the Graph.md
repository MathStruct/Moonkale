---
title: "Agents in the UI and on the graph — showing, sharing control, and private input"
description: "What an agent may show a person in Moonkale (display tools, never its own HTML or scripts), what working on the graph needs beyond Shared Work's tasks and approvals, share-keyboard and share-pointer buttons for people and agents, terminal input that stays private (passwords), and herdr as an optional agent runtime. From Daniel, 2026-10-29. Display tools, private input and opt-in position sharing are built; keyboard grants and herdr remain planned."
tags: [architecture, agents, collaboration, security, requirements, planned]
---
**From Daniel (2026-10-29):**
- He doesn't know whether there are special requirements for an agent that does something on the graph.
- He asks whether an agent can change Moonkale's UI so that it shows a person what it wants.
- He suggests a "share keyboard/cursor" and a "share mouse pointer" button.
- When someone types a password in a terminal, nobody else may see it.
- He also points to [herdr](https://github.com/herdrdev/herdr) as an extension, or something to build on.

This page extends [[Shared Work]] (SW-01 to SW-08 for participants and tasks, SW-40 to SW-45 for tools) and [[Collaboration]] (presence). Its rules are added to [[Security]].

## Implementation status (2026-10-30)

WP-71 and WP-70 are built on the codex contributor branch. All AGENTS gates and
the requested smoke/structural/Sophia/default graph browser matrix pass.
AU-20–23 cover both terminal views
with Unix PTY mode detection, private tails, paste confirmation and Pause sharing.
AU-24 has no current handover path; terminal exclusion remains mandatory when
AU-12 adds grants after accounts (SW-02), outside WP-72’s position-sharing slice.
AU-01–07 add typed display tools, marked groups, bounded per-conversation display
budgets and the user/session view-opening setting. AU-10, AU-11, AU-13 and AU-14 are built and validated (WP-72, 2026-10-30; see Status). AU-12, AG-01–05 and
AU-30–32 remain planned. The shared multi-agent task/token budget (SW-08) remains
WP-48; today’s display budget is its bounded local precursor.

## Today

- **Agent tools:**
  - `index.search`, `graph.query`, `graph.fetch` and `workspace.list_sources` read;
  - `editor.replace`, `file.create` and `file.save` write, through the policy gate (`services/llm/src/policy.rs`);
  - `editor.open` opens a node; `ui.reveal`, `ui.select`, `ui.show_result` and `ui.canvas` show bounded typed content in the conversation’s own marked group.
- **Agent messages** render as safe Markdown: raw HTML is shown as text, and images are never loaded ([[Shared Work]] §2).
- **Presence** shows other people's initials on tabs and files ([[Collaboration]]). Opt-in carets/selections and 2D graph pointers travel through the server hub or the local session bus; an independently enabled agent reveal target demonstrates agent presence.
- **Not built:** tasks, approvals, attribution and budgets ([[Shared Work]] SW-01 to SW-08, WP-48, which needs server accounts first).

## 1. What an agent may show (display tools)

An agent asks Moonkale to show something, and Moonkale draws it with its own renderers. An agent never supplies HTML, CSS or scripts: those would let a prompt-injected agent forge UI, read the page or send data out ([[Security]]).

- **AU-01 Reveal and select:**
  - open a node with its context (a linked group, [[Linked Panels]]);
  - reveal a range in a document;
  - select or highlight nodes in that group's graph (the engine's bounded `select_nodes`);
  - fit the view to them.
- **AU-02 Show a result:**
  - a query's rows in the table view;
  - a subgraph in a graph view;
  - both as a follower of the agent's conversation.
- **AU-03 The agent's canvas:** a panel the agent fills with typed content:
  - Markdown (the same safe renderer as messages);
  - a table;
  - a graph description (nodes, edges, styles from the theme);
  - later a plot ([[Plotting Requirements]]) and a diff (SW-14).

  Content is data: it is validated, bounded in size, and rendered by Moonkale.
- **AU-04 Marked as the agent's:** everything an agent opens or shows joins the agent's link group. Its tabs carry that group's colour and marker plus the agent's name, so a person always sees what came from an agent ([[Linked Panels]]).
- **AU-05 No surprises (ADR-0027):**
  - an agent never takes the focus;
  - it never covers what someone is typing in;
  - it never closes or moves a person's tabs.

  What an agent wants to show depends on a setting, **Agents may open views** (Daniel, 2026-10-29), with three values, in the user scope and overridable per session:
  - **Ask:** every view is a quiet offer ("Agent X wants to show 3 nodes — Show"). This is the default.
  - **When I'm not typing:** views open when nobody is typing; otherwise they wait as an offer.
  - **Always:** views open at once, never taking the focus.
- **AU-06 Budgeted:** display calls count toward the agent's budget (SW-08). Calls are rate-limited (for example at most one new view per few seconds), so a looping agent can't flood the workbench.
- **AU-07 Classed as read-only:** display tools don't change sources, so the policy gate treats them as read-only. They can still be denied per session, like `editor.open` today.

## 2. Agents working on the graph

[[Shared Work]] already requires:
- scoped tasks (SW-04);
- an approval inbox (SW-05);
- attribution (SW-06);
- presence for agents (SW-03);
- budgets (SW-08);
- tools as actors (SW-40 to SW-45).

The graph adds:

- **AG-01 Graph changes are proposals, shown as a graph diff:** added, removed and changed nodes and edges drawn on the graph (green, red, amber, plus a marker, never colour alone), not only as a text diff. This covers flow blocks and wires, Sophia definitions and renames, and table rows. It is the same proposal model as SW-23 and WP-49. For store-truth folders it uses Sophia's `propose_files` (SPEC-010).
- **AG-02 A working region:** while an agent works, the nodes it reads and the nodes it proposes to change are marked on the graph as its region. The marking is faint for reading and stronger for proposals. A person sees where the agent is, as they would see a person's cursor (SW-03).
- **AG-03 Regions don't collide silently:**
  - when two agents, or an agent and a person, propose changes to overlapping nodes, both are told before either is applied;
  - applying uses expected versions, so a stale proposal is refused rather than merged blindly (SPEC-010 file tokens; WP-49).
- **AG-04 Scope is a subgraph:**
  - a task's scope can be a query result or a selection, not only a list of sources;
  - reads outside the scope are refused, or asked for;
  - writes outside it are always refused.
- **AG-05 Derived stays derived (SW-41):** an agent's inferences (a type it guessed, a link it suggests) are annotations with provenance until a person accepts them. They never silently become authored nodes.

## 3. Sharing the keyboard, the cursor and the pointer

Two buttons, both off by default, per session and per participant (a person or an agent):

- **AU-10 Share my cursor (built, WP-72):**
  - others see your caret and selection in the documents you have open (presence, [[Collaboration]]);
  - they can **follow** you through sources they already have open; their own navigation or Escape ends follow.

  You can stop sharing at any time. Agents can share their cursor too (SW-03), which is how "watch the agent work" looks.
- **AU-11 Share my pointer (built for 2D graphs, WP-72):** others see your mouse pointer over the same view, as a labelled arrow, on graphs and canvases where position means something. This is for explaining ("this node here").
- **AU-12 Hand over the keyboard (later, needs accounts SW-02):** a separate, explicit grant: "let X type in this document". It is time-limited and revocable. Every keystroke is attributed to whoever typed it (SW-06).
  - **Agents never get live keystrokes** (decided, Daniel, 2026-10-29). Their edits are proposals (SW-05). Their text can stream into their own canvas or scratch document (AU-03), which is showing, not typing. A demonstration replays a proposal step by step, with its cursor moving through the edit, and is then accepted or rejected as a whole.
- **AU-13 Visible state (built, WP-72):**
  - a small indicator shows when you are sharing your cursor or pointer, or have handed over the keyboard;
  - one command stops all of it ("Stop sharing", with a shortcut);
  - nothing is shared without that indicator.
- **AU-14 Phones and screen readers (built, WP-72):**
  - the pointer is never required, and sharing works by cursor alone;
  - another participant's cursor is announced on request, not continuously.

## 4. Private input: passwords in a terminal

A terminal is shared output, but its input is private, and a password prompt is private output too:

- **AU-20 Keystrokes into a terminal are never shared:** not with followers, agents, presence, recordings, the entity log, the overview's previews or crash reports. Others see only what the terminal prints.
- **AU-21 Password prompts are detected from the terminal's mode:** the PTY host reads the line discipline on the master side (`tcgetattr`). Canonical mode with echo off is what `sudo`, `ssh`, `passwd` and `getpass` set. While it is in that mode:
  - the terminal shows a 🔒 marker;
  - the overview card shows "password prompt", not the screen;
  - followers see the prompt line without anything typed;
  - pasting into it asks first.
- **AU-22 Raw-mode programs are a known limit:** editors and ssh sessions put the local terminal in raw mode, which also turns echo off, so a remote password typed inside an ssh session can't be told apart from vim keys by mode alone. AU-20 already keeps all terminal keystrokes private, so nothing typed leaks. What can't be guaranteed is hiding *output* a remote program echoes. For that, a "Pause sharing" toggle on the terminal (and the global "Stop sharing") is the user's control.
- **AU-23 Agents never read terminal input:**
  - an agent that reads a terminal gets its output, through the same bounded, ANSI-stripped tail the overview uses (WP-60);
  - while AU-21's private mode is on, the agent gets nothing.
  - An agent that wants a password must ask the person through the secrets UI (secret references by name, never values; [[Security]]), not through a terminal.
- **AU-24 Hand-over excludes terminals:** AU-12's keyboard grant never covers terminals unless the owner explicitly grants that one terminal. It always ends when AU-21's private mode starts.

## 5. herdr as an agent runtime (optional)

[herdr](https://github.com/herdrdev/herdr) (Rust, Apache-2.0) runs agents' terminals in a background server:
- they survive closing the client or losing SSH;
- it marks each pane as working, blocked or idle;
- agents drive it through a CLI and a socket API.

It knows terminals, not the graph, so it would be a runtime under §2, not a replacement for it.

- **AU-30 A herdr adapter extension (optional, off by default):**
  - Moonkale's terminal panel attaches to herdr panes over its socket;
  - herdr's working, blocked and idle states show on tabs, in the overview and as a quiet "needs an answer" notice;
  - "Run agent here" starts an agent in herdr with a folder as its working directory.

  Credentials and SSH stay with herdr and the system. The adapter only gets the socket herdr exposes to the same user.
- **AU-31 The same rules apply:** AU-20 to AU-24 (private input) hold for herdr panes too. A herdr pane shown in Moonkale is a terminal like any other.
- **AU-32 Not a dependency:** Moonkale's own terminals and agents work without herdr. If herdr is absent, the extension says so and does nothing else.

## Order

1. **AU-01 to AU-07, the display tools:** they need linked panels, the graph's selection call and the overview, all built. They need no accounts. Work package WP-70.
2. **AU-20 to AU-24, private terminal input:** small, a security fix in its own right, and it comes before any sharing. Work package WP-71.
3. **AU-10 to AU-14, sharing buttons:**
   - cursor and pointer sharing between windows and on one server first;
   - the keyboard grant after accounts (SW-02).

   Work package WP-72.
4. **AG-01 to AG-05:** with proposals (WP-49) and agent tasks (WP-48).
5. **AU-30 to AU-32, the herdr adapter:** when someone uses herdr with Moonkale. Work package WP-73.

## Decided (Daniel, 2026-10-29)

- **AU-05:** whether agents open views is the setting above, defaulting to Ask.
- **AU-12:** no live keystrokes for agents. Streaming into their own canvas and step-by-step proposal replay cover drafting and demonstrations.


## Implementation notes

Display payloads are at most 64 KiB. A canvas is `{kind: "markdown", text}`, a
`{kind: "table", columns, rows}` of strings, or `{kind: "graph", nodes, edges}`.
Markdown rejects actual HTML events while accepting code blocks, quotes and
math. Validation caps nesting at 64 and parser events at 8,192.
Graph node styles are `default`, `accent`, `success` and `warning`, mapped to
existing theme-controlled graph kinds; edges refer to ids already in that
payload. `ui.select` uses those ids (or the displayed UUIDs), selects through
`select_nodes` and fits that subset. `ui.reveal` uses zero-based UTF-16 document
line/columns and an optional end, in its own linked content instance. Each
conversation and agent profile reuse their canvas/result views; pinned documents
keep distinct instances. Attribution is captured at turn start, independent of
later profile changes. Profiles share the conversation budget. Ranges do not focus the editor.

The display setting is `agent.open_views`: `ask`, `when_idle`, `always`. A folder
cannot override it. Settings offers User and Session scopes. Recent input means
a key/input/composition event within 1.5 seconds; the adapter retains only a
deadline. Offers are explicit Show/Dismiss controls in the Agent panel. Display
calls share the agent tool loop/audit and its turn safety limits; an additional
100-call budget and one admitted call per two seconds prevent view floods.
Legacy editor.open and views after approved writes follow the same consent;
a focused agent table queues updates as offers to preserve typed controls.
Server-owned conversations publish bounded ephemeral requests to connected
clients; saved transcripts never replay them as new display actions.

Private prompts use the local Unix master line discipline, sampled every 100 ms
(including quiet prompts with no new output). Raw mode is intentionally outside
the detector; non-Unix PTYs have no termios detector. Neither limitation adds a
terminal-input sharing path. “Pause sharing” clears and gates the bounded relay
tail until resumed; owner output replay stays available. No keyboard hand-over
is implemented, so no grant can currently include terminal input.
