//! Disposable E2E fixture writer, separate from the source host. It deliberately
//! exposes only a fixed table/key update, never an application SQL execution API.
fn main() {
    let path = std::env::args().nth(1).expect("fixture DuckDB path");
    let value = std::env::args().nth(2).expect("new fixture value");
    let c = duckdb::Connection::open(path).expect("open disposable fixture");
    c.execute("UPDATE edit_rows SET name = ? WHERE id = 1", [value])
        .expect("external fixture write");
}
