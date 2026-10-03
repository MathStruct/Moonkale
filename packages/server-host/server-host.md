---
title: moonkale-server-host — crate notes
tags: [crate-notes, milestone-18]
---
Notes for `moonkale-server-host` (Milestone 18 phase 4.3). Plan: [[Milestone 18 - Library Refactor]]; the rules it enforces: [[Security]].

What every **server half** of an extension shares, without depending on `api` (which assembles the server): today the jail — `allowed_root()` (`MOONKALE_ROOT`, else the working directory) and `jail_dir(path)` (a directory inside it, canonical, or `PermissionDenied`). No dependencies. Auth, TLS and the route list stay in the server binary (`api::auth`, `web`'s `build_router`); a server half never sees a request that did not pass them.
