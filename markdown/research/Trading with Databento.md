---
title: "Trading with Databento — requirements for an editor"
description: A friend of Daniel's trades with Databento's market-data API. What a trader needs from Moonkale (market data as a source with costs shown up front, order-book and price charts, replay, backtests as runs, a trading journal linked to the data), what Moonkale already has toward it, and what is out of scope. Study of 2026-10-14; nothing built.
tags: [research, trading, market-data, databento, requirements]
---
**Context (2026-10-14):** a friend of Daniel's trades and uses **Databento**, a market-data vendor. It serves historical and live data from exchanges in its binary DBN format, with official clients in Python, C++ and Rust (`databento` on crates.io, Apache-2.0; historical, live and intraday replay). Some of his requirements for an editor should be met. This also feeds the markets use case of [[Lenticulum Frontend Requirements]] (UC-04).

*To confirm with him:* which datasets (for example CME futures or Nasdaq equities), which schemas, historical or live, which language he writes strategies in, and what he uses today (TradingView, Sierra Chart, Jupyter, …). The list below is what such a setup usually needs; his answers reorder it.

## Requirements

`TR-nn`.

### Data

- **TR-01 Databento as a source:** historical requests (dataset, symbols, schema, time range) and live subscriptions, through the official Rust client in-process. The API key is in Moonkale's secret store, and the source is read-only.
- **TR-02 Costs before cost:** historical data is billed per request. Before a download, the source asks Databento's cost estimate (`metadata.get_cost`) and shows it, the way the KG studio shows billed rows (KG-02). An optional per-session budget refuses requests over it.
- **TR-03 Download once:** a downloaded range is cached in the folder as DBN, and as Parquet for DuckDB, so a repeated query costs nothing and works offline. The cache knows which ranges it holds.
- **TR-04 The schemas, typed:**
  - order book by order (MBO), top of book (MBP-1), ten levels of depth (MBP-10), top of book on each trade (TBBO);
  - trades;
  - bars (OHLCV at 1 s, 1 m, 1 h, 1 d);
  - instrument definitions, statistics, trading status, imbalances.

  They show in the table grid with **nanosecond timestamps** (`ts_event`, `ts_recv`) and **fixed-point prices**, never rounded through floats. This needs typed cells (WP-15): `Value` today has no exact decimal and no nanosecond time.
- **TR-05 Symbology:** raw symbols, parent symbols (`ES.FUT`), continuous contracts (`ES.c.0`) and instrument ids resolve both ways, and rolls are shown on the time axis. Exchange calendars and time zones are explicit.

### Seeing the market

- **TR-06 Price charts:** candlesticks or OHLC with volume, several timeframes in sync, a crosshair with values, and indicators as derived series (moving averages, VWAP) computed from the cached data. These are chart types of the plot extension ([[Plotting Requirements]]).
- **TR-07 Order book views:** a depth ladder from MBP-10, time and sales from trades, and an **order-book heatmap** over time from MBO or MBP data (resting liquidity as colour, trades on top). This is millions of cells, a job for the GPU path, as the graph engine draws 100 k nodes.
- **TR-08 Replay:** a historical day replays at a chosen speed (Databento's intraday replay through the live client, or the local cache), driving every chart and the ladder together. Its cursor is the same time cursor runs use (LT-12).

### Strategies

- **TR-09 Backtests are runs:** a strategy (Python, Rust or Julia code, or a flow) runs over a data range and a parameter set. The run record keeps the data range, parameters, code version, fills, P&L and metrics (returns, drawdown, Sharpe), and runs compare side by side. It is the same run-record service as Lenticulum and workflows (WP-38/41).
- **TR-10 Notebooks** for research, with Databento queries as cells ([[Notebook Editor]]).
- **TR-11 Alerts and watchlists:** conditions on live data (price crosses, spread, volume spike) raise a desktop notification; watchlists of instruments sit in the Sources panel.

### The trader's own notes

- **TR-12 A journal linked to the data:** a trade note links to an instrument and a moment (a wiki-link to an instrument node and a timestamp), and clicking it opens the chart and replay at that moment. Notes plus data in one place is what Moonkale has and trading tools lack.
- **TR-13 The market as a graph:** instruments, their relations (correlations, spreads, the same underlying) and order flow between venues drawn as a graph, which leads into the markets factor graphs of UC-04.

## Out of scope, or limited

The [[Scrutiny]] entries:
- **Order entry and execution.** Sending orders to a broker is a different product, with compliance and risk responsibilities. If ever, it would be a separate extension behind an explicit gate, never in Moonkale's core. Moonkale is for research, review and monitoring.
- **Execution-grade latency in the UI.** Charts update smoothly (60 FPS), but Moonkale is not a low-latency trading terminal.
- **Republishing market data.** Exchange data is licensed, and its redistribution is restricted. Reader mode and any publishing must not serve downloaded market data to others. A source can mark its data as non-redistributable, and publishing refuses it.

## Where it would go

**An extension in its own repository** (like `moonkale-julia`): a market-data source for Databento, the cache, symbology, and the order-book views. It depends on the plot extension for the charts, on typed cells (WP-15) and on run records (WP-38). It is vendor-specific, so it stays out of the core ([[Core and Extensions]]).

Work package: WP-42 in [[Work Packages]].
