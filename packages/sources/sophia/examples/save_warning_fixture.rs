//! Seed an authored store for the browser save-warning acceptance test.
use sophia_interface::Interface;
use sophia_store::sqlite::Sqlite;
fn main() {
    let path = std::env::args().nth(1).expect("fixture path");
    let i = Interface::open(Sqlite::open(path).unwrap(), "moonkale").unwrap();
    i.load_sample().unwrap();
    i.create_text(
        &sophia_interface::Key::ns("sample"),
        "helper",
        "function helper(x::Int64)::Int64\n    x\nend\n",
    )
    .unwrap();
    i.create_text(
        &sophia_interface::Key::ns("sample"),
        "user",
        "function user(x::Int64)::Int64\n    helper(x)\nend\n",
    )
    .unwrap();
}
