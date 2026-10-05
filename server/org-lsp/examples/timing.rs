//! Time the parser and the highlighter on a file:
//! `cargo run --release -p org-lsp --example timing -- file.org`.

use std::time::Instant;

use org_lsp::highlight;
use org_lsp::line_index::{Encoding, LineIndex};
use org_syntax::TodoKeywords;

fn main() {
    let path = std::env::args().nth(1).expect("usage: timing FILE");
    let text = std::fs::read_to_string(&path).expect("readable file");

    let start = Instant::now();
    let parse = org_syntax::parse(&text, &TodoKeywords::default());
    let parsed = start.elapsed();

    let start = Instant::now();
    let index = LineIndex::new(&text);
    let tokens = highlight::tokens(&parse.syntax(), parse.todo_keywords(), highlight::Conceal::default());
    let encoded = highlight::encode(&tokens, &text, &index, Encoding::Utf16, None);
    let highlighted = start.elapsed();

    println!("{} bytes, {} lines", text.len(), index.line_count());
    println!("parse:     {parsed:?}");
    println!("highlight: {highlighted:?} ({} tokens)", encoded.len());
}
