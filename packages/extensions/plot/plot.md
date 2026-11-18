# moonkale-ext-plot

Optional, on by default: cheap quick-look SVG without a native driver or JS library.
`plot.result` (Ctrl+Alt+D, rebindable) is contributed to result overflow menus and
the palette. It creates one plot follower per table panel instance; no rail entry.

Rust owns mark, columns, scales and camera. Settings survive docking in a bounded
workspace runtime map, keyed by outer session and panel (512 entries); the camera
is not saved across application restart, the chart itself is, as a spec file (below).
The link membership persists with other views, but query data is never serialized.
After restart the table must load before its plot can draw.

Data is the latest loaded result, shared by Rc, including table page/filter/edit
changes. SQL pages stay at the grid's 4000-row limit. Preparation scans at most
50,000 loaded rows; line/scatter uniformly sample at most 5000 marks, with endpoints
and original row indices retained. Histograms use at most 40 bins, bars at most 100
categories (sum). Partial, sampling and omitted-value notices explain the bounds.
Histograms and bars over a SQL pushdown source (SQLite, DuckDB, Turso) are
aggregated in the database instead (`Source::aggregate_rows`, PLT-11): prepared
statements, schema-checked identifiers, bound values, the grid's filters applied,
and inner queries that expose only aliases (a source column cannot capture the
GROUP BY). Bars keep the first 100 categories in the grid's row order (the whole
row key) and omit a category whose sum overflows f64. A finite range wider than
f64 can subtract is binned in the source, but the axis cannot span it, so the plot
reports those rows as omitted, as the client path does. Other sources make no
source request; the loaded-page fallback above applies.

Plot specifications (PLT-04) are `*.plot.json` files, by default under
`.moonkale/plots/`. `PlotFile::parse` is bounded (size, string lengths, 32
filters), refuses unknown fields and versions other than 1; the fuzz target is
`fuzz/fuzz_targets/plot_spec.rs`. Save plot (⋯ and palette) writes one through
the folder source; Export SVG writes `render_svg`'s escaped SVG next to it. A spec
is content: its `source.path` is opened only when the folder's jailed path lookup
finds it inside the folder (`spec_source_inside`; no `..`, no link leading out).

Time uses UTC seconds from typed DateTime values; units are not present in the
current Value contract. Axes use column names, finite linear/log/time transforms,
width-limited ticks and an optional equal aspect. Nonpositive log values are
omitted. Histogram binning on a log X scale occurs in log space. Hover and keyboard
inspection show bounded row text. Left/right/Home/End inspect, Enter/Space select,
+/− zoom, Shift+arrows pan; the overflow provides mouse equivalents. Wheel zoom
and pointer drag also work. Selecting an aggregate reveals its first constituent
row, explicitly described in the tooltip; brushing all members is later PLT-12.

Snapshot row tokens include owner, position and a hash of typed values. They avoid
misidentifying a different row after replacement/eviction. Source-known Ref node
ids travel alongside the token through the existing group selection. The table
reveals the selected row and graphs select the authoritative ids. Row/point
selection does not navigate or steal focus. A changed row invalidates its old
snapshot token rather than selecting an unrelated row.

`model.rs` tests defaults, transforms/ticks, sampling, aggregation and spec
round-trips/refusals; `tests/spec_source.rs` the spec source confinement;
`sources/sql/tests/table_query.rs` the source aggregate on all three engines
against a brute-force reference.
`plot.mjs` covers SQLite/DuckDB defaults, controls, both input modes, linked rows,
query decimation, the million-row source-aggregated histogram (whole table, with
a grid filter), Export SVG, Save plot and reopening the saved spec after reload.

Uniform sampling always retains an explicitly selected current row (replacing
one middle sample within the same cap). Aggregated marks highlight when any
constituent row is selected. Plot tabs can dock locally; moving held plot data
between windows is deferred with saved/replayable chart specifications.
