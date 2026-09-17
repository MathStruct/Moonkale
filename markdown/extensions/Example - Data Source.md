---
title: "Example — Data Source"
tags: [extensions, example]
---
A **static** extension (native driver ⇒ desktop/server) adding a hypothetical "CSV directory" source. Shows the `source` contribution point ([[Contribution Points]]) and the lifting rules ([[Data Sources]]).

```toml
[[contributes.source]]
family = "sql"
dialect = "csvdir"
form = [{ name = "path", kind = "directory", label = "Folder of CSV files" }]
platforms = ["desktop", "server"]
```

```rust
struct CsvDirFactory;
impl SourceFactory for CsvDirFactory {
    fn connect(&self, params: ConnectParams) -> Result<Box<dyn Source>, SourceError> {
        // open DuckDB in-memory, `CREATE VIEW <file> AS SELECT * FROM read_csv_auto(...)` per file
    }
}

impl Source for CsvDirSource {
    fn descriptor(&self) -> SourceDescriptor {
        // family Sql, dialect csvdir, capabilities READ | TEXT_QUERY(Sql) | FULLTEXT
        // schema graph: one Table node per file, Column nodes with Contains edges
    }
    async fn query(&self, q: Query) -> Result<QueryResult, SourceError> {
        // structured → SQL via moonkale_sources_sql::structured; rows → Row nodes
        // ids: derive(source_id, table, row_number) — flagged unstable (no PK)
    }
    async fn fetch(&self, id: NodeId) -> Result<Content, SourceError> { /* one row as props */ }
    async fn apply(&self, _tx: Transaction) -> Result<Applied, SourceError> {
        Ok(Applied::all_unsupported("csvdir is read-only"))
    }
    fn subscribe(&self) -> BoxStream<'static, SourceEvent> { /* watch dir via project-fs::watch */ }
}

moonkale_ext_api::export!(CsvDirExt);   // registers the factory in activate()
```

What the user gets for free: the explorer shows tables, the [[Table Editor]] pages rows, the [[Graph View]] shows the schema graph, `[[sales.csv]]` resolves from a wiki page, and the agent can run `SELECT` against it (policy: read-only source ⇒ writes impossible).

Web users see the same source *if* the server has the extension installed — the shell routes through `RemoteSource` ([[ADR-0005 Server functions as the remote backend]]).
