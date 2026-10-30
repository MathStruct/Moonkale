//! Browser fixture for typed SQL cells.
fn main() {
    let path = std::env::args().nth(1).expect("database path");
    let c = duckdb::Connection::open(path).expect("fixture database");
    c.execute_batch("CREATE TABLE edit_rows(id INTEGER PRIMARY KEY, name VARCHAR, score DOUBLE); INSERT INTO edit_rows VALUES(1,'original',1.5),(2,'second',2.5)").expect("editable fixture");
    c.execute_batch("CREATE TABLE dates AS SELECT TIMESTAMP '2026-10-23 08:30:00' AS instant, [1.0,4.0,2.0,5.0]::DOUBLE[] AS samples, [3,1,4]::FLOAT[3] AS fixed").expect("typed fixture");
}
