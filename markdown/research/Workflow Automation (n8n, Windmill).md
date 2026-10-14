---
title: "Workflow automation — requirements from n8n and Windmill, under scrutiny"
description: What n8n and Windmill offer (triggers, nodes, credentials, executions, scripts in many languages, approvals), which of it Moonkale's flow editor should have, and what is rejected. Their licences allow requirements only. Study of 2026-10-14; nothing built.
tags: [research, workflows, flow, requirements, scrutiny]
---
**Context (2026-10-14, Daniel):** n8n and Windmill are a good source of requirements, but they should be put under scrutiny. Moonkale's [[Flow Editor]] today is a *modelling* editor: typed ports, generated Julia code (Lux, ModelingToolkit, RxInfer). Workflow tools are *execution* editors: a graph of steps that runs on triggers. Some of their requirements fit Moonkale; some would pull it toward being a SaaS automation platform, which it is not.

**Licences: requirements only, no code** ([[Scrutiny]] rules):
- **n8n** is source-available under its *Sustainable Use License* (fair-code, not OSI open source).
- **Windmill**'s repository is AGPLv3, with its OpenFlow spec Apache-2.0. Its prebuilt Community Edition mixes in proprietary code.

Neither is compatible with copying code into MIT Moonkale, and this study took nothing from their code.

## What they offer

| Area | n8n | Windmill |
|---|---|---|
| Graph | visual nodes and connections, branching, merging, loops | flows as DAGs of steps (the OpenFlow format) |
| Steps | 400+ integration nodes, a Code node (JS, Python) | scripts in many languages (Python, TypeScript, Go, Bash, SQL, Rust …) with typed parameters from signatures |
| Triggers | webhooks, schedules, app events, manual | schedules, webhooks, HTTP routes, email, Kafka-style queues |
| Data | JSON items passed between nodes, expressions to map fields | typed inputs and outputs (JSON Schema from script signatures) |
| Credentials | encrypted per workspace, shared by role | resources and variables, secrets per workspace |
| Runs | execution history with every node's input and output, retries, error workflows | job queue, logs, retries, timeouts, concurrency limits, caching |
| People | — | approval steps (suspend until a human approves), auto-generated forms and apps |
| Versions | workflow versions | scripts and flows versioned, deployable from git |

## What fits Moonkale

`WF-nn`.

- **WF-01 Typed step data:** Windmill's typed inputs and outputs from function signatures match Moonkale's typed ports. A step's ports come from its signature (a Julia or Rust function, a SQL query's columns). n8n's untyped JSON items don't.
- **WF-02 Runs as records:** every run keeps each step's inputs, outputs, logs, timing and errors, inspectable on the graph (select a step, see what went through it). It is the same need as Lenticulum's run records (LT-13), so one mechanism serves both.
- **WF-03 Re-run from a step,** with the earlier steps' outputs cached; retries and timeouts per step.
- **WF-04 Scripts as steps in the core languages,** run by the toolchains Moonkale already starts (Julia, Python, Rust, SQL on a source), locally or on the server.
- **WF-05 Triggers that make sense for an editor:** manual, on file change (the folder watcher exists), on a schedule (desktop or server), and on a source event (`SourceEvent`). Webhooks only in server mode, behind the gate.
- **WF-06 Approval and human steps:** a step that pauses for a person, the way the policy gate already pauses an agent's write.
- **WF-07 Versions from git:** a flow is a document in the folder; its version is the commit, or a content hash.
- **WF-08 Credentials through Moonkale's secret store,** scoped to a source or a project, never inside the flow document.

## Rejected or limited

The [[Scrutiny]] entries:
- **The integration catalogue as a goal** (400+ SaaS connectors). It is the product of an automation company, and maintaining it is a business of its own. Moonkale gets its integrations from sources and extensions, and an HTTP or OpenAPI step covers the rest.
- **Untyped JSON items with field-mapping expressions** (n8n's model). They lose types at every wire, and Moonkale's flows are typed (ADR-0020). An expression language for mapping can come later, typed.
- **A hosted multi-tenant runner** (workers, queues, quotas, billing). Moonkale's runner is the desktop or the user's own server. Scale-out execution is a backend's job (an extension may hand runs to Windmill or a cluster through their APIs).
- **Auto-generated end-user apps and forms** (Windmill). That is out of scope for an editor. Reader mode ([[Publishing Sources]]) and dashboards ([[Plotting Requirements]]) are Moonkale's ways to show results to others.

## Where it would go

- **An optional extension**, not core: execution on the flow editor's model, with a runner service on desktop and server. The flow model gains a *step* role beside variable and factor (ADR-0022's roles), so modelling and workflow graphs share one editor, one engine and one run-record mechanism.
- **First:** WF-02 run records and WF-03 re-run, because Lenticulum needs the same. Then scripts as steps (WF-04) and triggers (WF-05).

Work package: WP-41 in [[Work Packages]].
