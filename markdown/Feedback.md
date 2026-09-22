---
title: "Feedback — what to do when something is wrong"
description: Where to send a bug, a feature request, a suggestion, a design flaw or a plea for help; what makes a report useful; and what happens to it afterwards.
tags: [start, feedback, meta]
---
Moonkale is version 0.1 — usable daily by its author, rough at the edges, and tested on Linux, the web and one Android phone. **Windows and macOS builds are unsigned and untested; reports are the only testing they get.** If something is wrong, saying so is the most useful thing you can do.

## Where to put it
**Issues:** <https://github.com/MathStruct/Moonkale/issues> → *New issue* and pick the kind. Each form applies its own label, so nothing has to be sorted by hand:

| pick this | label | for |
|---|---|---|
| **Bug** | `bug` | it crashed, lost data, did the wrong thing, or did nothing |
| **Feature request** | `feature request` | a capability that does not exist yet |
| **Suggestion** | `suggestion` | a smaller change to what exists: wording, a default, a shortcut, a layout |
| **Design flaw** | `design flaw` | the *idea* is wrong — a model, an interaction, a security assumption |
| **Help** | `help` | you cannot get it to install, build, connect, or talk to a model |

Not sure which? Take the closest one, or open a blank issue — the label can be changed later. Questions and half-formed ideas belong in **Discussions**; they become issues when they sharpen.

Further labels appear during triage: `needs info` (waiting for you), `confirmed` (reproduced here and in the [[Problem Log]]), and `platform:windows` / `platform:macos` / `platform:android` / `platform:web`.

## What makes a report useful
1. **What you did, what you expected, what happened.** Three sentences beat a paragraph of theory.
2. **Where**: platform, version (*Help → About Moonkale* puts it in the status bar; the server prints it with `moonkale-server --version`), and how you installed it — the release `.deb`, pacman, Nix, a build from source at some commit.
3. **The text of the error, not a picture of it.** The status bar's message, the app's terminal output, or the log file ([[Debugging and Logging]] says where it is on each platform). Paste it; it is searchable, a screenshot is not.
4. **One thing per issue.** Five problems in one thread lose four of them.
5. **What you cannot know, say as what you saw.** "The graph stayed empty for a minute on a 20 000-file folder" is a good report; "the graph is broken" is not.

Nothing else is expected: no reproduction repository, no bisect, no patch. If you *have* a fix, a pull request is welcome — but a plain report is never the lesser contribution.

## What happens to it
Every reported problem that is reproduced ends up as a row in the **[[Problem Log]]** (`P-nnn`) with its cause and its fix, and the issue is linked from there; that log is public and is the honest history of the project, failures included. A design flaw that changes a decision becomes an ADR (the `decisions/` folder, e.g. [[ADR-0001 Dioxus instead of Lumino and Tauri]]). A feature request lands in the [[Roadmap]] or, when it is small and concrete, as a numbered specification (`specifications/`) that gets implemented and marked *done*. Anything that stays open stays visible — "wontfix" is used, and said out loud, rather than letting a thread rot.

You will not get a service-desk answer time. This is one person's project with an agent doing the typing; expect a reply in days, sometimes an hour, and expect to be asked one precise question rather than a form letter.

## Privacy before you paste
Logs can carry paths, file names and folder contents; a transcript can carry whatever you asked an agent. Redact before pasting. **Never paste an API key, a token or a password** — not in an issue, not in a log excerpt. If a report needs a secret to make sense, say so and leave it out. Moonkale itself never puts keys in settings files ([[LLM and RAG]], [[Remote and Server Modes]]), but your shell history and your terminal panel might.

## If it is urgent and private
A security problem — something that lets one person read another's files, escape the server's root, or capture credentials — goes to <daniel@mathstruct.org> instead of a public issue, and gets a fix before it gets a description.

Related: [[Getting Started]] · [[Install]] · [[Problem Log]] · [[Development]].
