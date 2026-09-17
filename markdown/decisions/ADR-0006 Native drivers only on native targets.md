---
title: "ADR-0006 — Native drivers only on native targets"
tags: [adr]
status: accepted
date: 2026-09-17
---
**Status:** accepted

## Context
`sqlx`/`duckdb`/`typedb-driver`/`redis` etc. do not compile to `wasm32`; some (Supabase REST) could work from a browser.

## Decision
Split `moonkale-sources` (abstraction, lifting, remote proxy — compiles everywhere) from `moonkale-sources-{sql,graph,kv}` (drivers — native only). The `web` crate must never depend on a driver crate; CI enforces it.

## Consequences
- Web knows what sources *are* (UI, capabilities) without linking drivers.
- Browser-direct sources (Supabase REST) would be a separate, wasm-compatible crate later.
- Feature flags per dialect keep build times sane.
