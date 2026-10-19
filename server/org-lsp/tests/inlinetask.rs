//! Inline tasks in the server: what `TAB` hides on them and on their
//! parents in Emacs (from `tests/inlinetask/cases.json`, see
//! `scripts/update-inlinetask-fixtures.py`) against the folding ranges,
//! and the outline, which leaves them out.

use std::path::Path;

use org_lsp::folding::{folding_ranges, SEPARATOR_LINES};
use org_lsp::line_index::{Encoding, LineIndex};
use org_lsp::symbols::document_symbols;
use org_syntax::TodoKeywords;
use serde_json::Value;

/// Every range Emacs hides with `TAB` (`[first, last]` hidden lines) is
/// the fold that starts on the line before it; where `TAB` on an inline
/// task hides nothing, there is no fold there.
#[test]
fn folds_are_what_tab_hides_in_emacs() {
    org_core::inlinetask::set_min_level(Some(15));
    let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/inlinetask/cases.json"));
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = Vec::new();
    let mut compared = 0;
    for case in cases.iter().filter(|case| case["key"].as_str().unwrap().starts_with("TAB")) {
        let text = case["text"].as_str().unwrap();
        let point = case["point"].as_u64().unwrap() as usize;
        let parse = org_syntax::parse(text, &TodoKeywords::default());
        let index = LineIndex::new(text);
        let folds: Vec<(u32, u32)> = folding_ranges(&parse.syntax(), text, &index, SEPARATOR_LINES)
            .into_iter()
            .map(|range| (range.start_line, range.end_line))
            .collect();
        let line = text[..point].matches('\n').count() as u32;
        let hidden: Vec<(u32, u32)> = case["expected"]["hidden"]
            .as_array()
            .unwrap()
            .iter()
            .map(|pair| (pair[0].as_u64().unwrap() as u32, pair[1].as_u64().unwrap() as u32))
            .collect();
        for (first, last) in &hidden {
            compared += 1;
            if !folds.contains(&(first - 1, *last)) {
                failures.push(format!("{text:?} {}: hides {first}..{last}, folds {folds:?}", case["key"]));
            }
        }
        let on_task = text.split('\n').nth(line as usize).is_some_and(|line| line.starts_with(&"*".repeat(15)));
        if case["key"] == "TAB" && on_task && hidden.is_empty() {
            compared += 1;
            if folds.iter().any(|fold| fold.0 == line) {
                failures.push(format!("{text:?}: no fold on line {line} in Emacs, folds {folds:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(compared > 20, "only {compared} compared");
}

/// The outline leaves inline tasks out, as `org-imenu-get-tree` does.
#[test]
fn the_outline_leaves_inline_tasks_out() {
    org_core::inlinetask::set_min_level(Some(15));
    let stars = "*".repeat(15);
    let text = format!("* A\n{stars} TODO Task\nbody\n{stars} END\n** B\n{stars} Lone\n");
    let parse = org_syntax::parse(&text, &TodoKeywords::default());
    let index = LineIndex::new(&text);
    let symbols = document_symbols(&parse.syntax(), &text, &index, Encoding::Utf16);
    let names: Vec<(String, Vec<String>)> = symbols
        .iter()
        .map(|symbol| {
            let children = symbol.children.iter().flatten().map(|child| child.name.clone()).collect();
            (symbol.name.clone(), children)
        })
        .collect();
    assert_eq!(names, [("A".to_string(), vec!["B".to_string()])]);
}
