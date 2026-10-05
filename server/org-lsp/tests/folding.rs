//! Conformance against Emacs: `tests/folding/cases.json` holds documents
//! and the lines Emacs hides when each subtree is folded (see
//! `scripts/fold_reference.py`). `ORG_FOLD_CASES` points the test at
//! another file, which is how `scripts/fuzz-fold.py` feeds it generated
//! cases.

use std::path::PathBuf;

use org_lsp::folding::{folding_ranges, SEPARATOR_LINES};
use org_lsp::line_index::LineIndex;
use org_syntax::{SyntaxKind, TodoKeywords};
use serde_json::Value;

fn cases() -> Vec<Value> {
    let path = match std::env::var_os("ORG_FOLD_CASES") {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/folding/cases.json"),
    };
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display())))
        .unwrap()
}

#[test]
fn subtree_folds_match_emacs() {
    let cases = cases();
    let mut failures = String::new();
    let mut failed = 0;
    for (index, case) in cases.iter().enumerate() {
        let text = case["text"].as_str().unwrap();
        let separator_lines = case.get("separatorLines").and_then(Value::as_i64).unwrap_or(SEPARATOR_LINES);
        let expected: Vec<(u32, u32)> = case["expected"]
            .as_array()
            .expect("run scripts/update-fold-fixtures.py first")
            .iter()
            .map(|pair| (pair[0].as_u64().unwrap() as u32, pair[1].as_u64().unwrap() as u32))
            .collect();
        let parse = org_syntax::parse(text, &TodoKeywords::default());
        let line_index = LineIndex::new(text);
        // Only subtrees: Emacs was asked about headlines.
        let headlines: Vec<u32> = parse
            .syntax()
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::HEADLINE)
            .map(|node| line_index.line_of(usize::from(node.text_range().start())) as u32)
            .collect();
        let actual: Vec<(u32, u32)> = folding_ranges(&parse.syntax(), text, &line_index, separator_lines)
            .into_iter()
            .filter(|range| headlines.contains(&range.start_line))
            .map(|range| (range.start_line, range.end_line))
            .collect();
        if actual != expected {
            failed += 1;
            failures.push_str(&format!(
                "\n--- case {index} (separatorLines {separator_lines})\n{text:?}\n  expected: {expected:?}\n  actual:   {actual:?}\n"
            ));
        }
    }
    assert!(failed == 0, "{failed} of {} folds differ from Emacs:{failures}", cases.len());
}
