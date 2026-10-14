---
title: "Plotting — requirements from Plots.jl and Makie up to Tableau"
description: Moonkale has no way to plot data. What plotting users need at three levels (a quick plot like Plots.jl, a publication or interactive figure like Makie, a dashboard like Tableau), what fits Moonkale, and a proposal for a Rust plot extension linked to tables, queries, the graph and live runs. Study of 2026-10-14; nothing built.
tags: [research, plotting, visualization, requirements]
---
**Context (2026-10-14):** Moonkale cannot plot. Daniel uses Plots.jl or Makie; others use full Tableau. [[Publishing Sources]] already sketches a Plot panel (`plotters` to SVG or canvas in wasm, no JS charting library), and Lenticulum needs plots beside the graph ([[Lenticulum Frontend Requirements]] LT-17). Requirements come from the public documentation of the tools named, under the [[Scrutiny]] rules.

## Three levels of user

| Level | Tools | What they do |
|---|---|---|
| **Quick look** | Plots.jl, matplotlib, `df.plot()` | one call from data to a line, scatter, histogram or heatmap; sensible defaults; done in seconds |
| **Figures** | Makie, ggplot2, Vega-Lite, plotters | layouts of many panels, legends, colour bars, LaTeX labels; publication export (PDF, SVG); interactive zoom and pan; 3D; animation; big data on the GPU (Makie's GLMakie) |
| **Exploration and dashboards** | Tableau, Superset, Grafana | drag fields onto shelves; filters, linked brushing across views; aggregation over big tables pushed to the database; dashboards that refresh; sharing |

## Requirements

`PLT-nn`.

### The quick look (must have first)

- **PLT-01 From any result to a plot in one action:** a table result (SQL, DuckDB, Parquet, CSV, a SPARQL or Cypher result, a query over the index) has a **Plot** action that picks a sensible chart from the column types: time against number gives a line, number against number a scatter, one number a histogram, a category and a number bars, a matrix a heatmap.
- **PLT-02 The basic marks:** line, scatter, bar, histogram, area, heatmap, box or violin, error bars and bands (uncertainty is everywhere in Lenticulum).
- **PLT-03 Axes that behave:** linear, log and time scales, units in labels, ticks that don't overlap, and an equal-aspect option.

### Figures

- **PLT-04 A declarative spec,** stored as a file or node: data source, marks, encodings (x, y, colour, size, shape, facet), scales, labels. It follows the grammar of graphics (Vega-Lite, ggplot2, AlgebraOfGraphics.jl). An agent can write one, and git can diff one. The spec, not a screenshot, is the document.
- **PLT-05 Layouts:** facets (small multiples), grids of panels, shared axes, legends and colour bars.
- **PLT-06 Interaction:** zoom, pan, hover tooltips with the row's values, select points. A selected point **links back** to its row in the table and its node in the graph.
- **PLT-07 Export:** SVG and PDF for papers, PNG for chat. Math in labels through the same renderers as Markdown ([[Math Delimiters and Renderers]]).
- **PLT-08 Big data:** millions of points through the GPU (the graph engine's wgpu path, which already draws 100 k nodes), with level of detail (binning, density) beyond that.
- **PLT-09 Live:** a plot whose source is live (a Lenticulum run, a streaming source) updates in place: loss curves, marginals over time. The scrub cursor of a run (LT-12) moves on every linked plot.
- **PLT-10 3D and animation** (Makie-level): surfaces, point clouds, and animations over time steps. Later, and only where wgpu makes them cheap.

### Exploration and dashboards

- **PLT-11 Aggregation in the source:** group, bin and aggregate are pushed to the database when it can (DuckDB, SQL), like the table grid's planned pushdown (WP-13). The plot never pulls ten million rows to draw ten bars.
- **PLT-12 Linked views:** brushing in one plot filters the others, the table and the graph.
- **PLT-13 Dashboards:** a layout of plots, tables and graph views over shared filters, saved as a document, refreshed on demand or on a schedule. In reader mode ([[Publishing Sources]]) a dashboard is something a visitor can use.

### Not taken (for [[Scrutiny]])

- **Tableau's proprietary formats and its server model;** Moonkale's dashboards are documents over sources.
- **A JS charting library in the core** (Plotly, ECharts), per [[JS Interop Boundary]]. Rendering is Rust: `plotters` for SVG and PDF, wgpu for big and live data.
- **Pixel-only output as the stored artefact;** the spec is stored, and the pixels are derived.

## Proposal: a Plot extension

- **An optional extension, `plot`**, in this repository: the spec type (PLT-04), a plot panel, and a "Plot" action contributed to the table panel and to query results.
- **Two renderers behind one spec:** `plotters` for publication output (SVG, PDF, PNG) and quick plots, and the graph engine's wgpu path for big and live data.
- **Data comes through sources:** a plot names a query on a source, so plots of a DuckDB file, a Lenticulum run and the code index are the same mechanism.
- **First step:** PLT-01 to PLT-03 and PLT-06 on table results, which covers the quick look Daniel uses Plots.jl for. Then live plots for Lenticulum (PLT-09), then the declarative spec and export, then dashboards.

Work package: WP-40 in [[Work Packages]].
