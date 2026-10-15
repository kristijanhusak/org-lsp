//! `org-startup-align-all-tables` against Emacs: `tests/startup-align/cases.json`
//! holds documents and what they are once `org-mode` has started in them
//! with the option (`scripts/startup_align_reference.py`).

use org_core::format::{align_tables, Alignment};
use org_syntax::TodoKeywords;
use serde_json::Value;

/// Whether the document's tables are aligned: the option, unless
/// `#+STARTUP:` says `align` or `noalign`, the last one counting.
fn aligns(text: &str, option: bool) -> bool {
    let mut align = option;
    for line in text.lines().filter(|line| line.to_ascii_lowercase().starts_with("#+startup:")) {
        for word in line[10..].split_whitespace() {
            match word {
                "align" => align = true,
                "noalign" => align = false,
                _ => {}
            }
        }
    }
    align
}

#[test]
fn aligns_tables_at_startup_as_emacs_does() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/startup-align/cases.json");
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for case in &cases {
        let text = case["text"].as_str().unwrap();
        let expected = case["expected"].as_str().unwrap();
        let keywords = TodoKeywords::default();
        let got = if aligns(text, case["align"].as_bool().unwrap_or(false)) {
            align_tables(text, &keywords, &Alignment::default())
        } else {
            text.to_string()
        };
        if got != expected {
            failures.push(format!("{text:?}\n  emacs   {expected:?}\n  org-lsp {got:?}"));
        }
    }
    assert!(failures.is_empty(), "{} of {} cases differ:\n{}", failures.len(), cases.len(), failures.join("\n"));
}
