//! Reproducible million-row browser fixture. Only accepts a new database path.
fn main() {
    let path = std::env::args().nth(1).expect("new fixture database path");
    assert!(
        !std::path::Path::new(&path).exists(),
        "fixture already exists"
    );
    let c = duckdb::Connection::open(&path).expect("fixture database");
    c.execute_batch("SET threads=2; CREATE TABLE million(id BIGINT PRIMARY KEY, bucket BIGINT, label VARCHAR); INSERT INTO million SELECT i, i % 10, 'row ' || i FROM range(1000000) n(i); CHECKPOINT").expect("million-row fixture");
}
