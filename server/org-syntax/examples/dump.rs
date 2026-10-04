//! Print the syntax tree of an Org file: `cargo run --example dump -- file.org`.

use org_syntax::{parse, TodoKeywords};

fn main() {
    let path = std::env::args().nth(1).expect("usage: dump FILE");
    let text = std::fs::read_to_string(&path).expect("readable file");
    let parsed = parse(&text, &TodoKeywords::default());
    print!("{:#?}", parsed.syntax());
}
