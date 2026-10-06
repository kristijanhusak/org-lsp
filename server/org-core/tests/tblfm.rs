//! Table formulas against Emacs.
//!
//! `tests/tblfm/cases.json` holds Org files with a table and what Emacs's
//! `org-table-recalculate` made of them (`scripts/update-tblfm-fixtures.py`).
//! `ORG_TBLFM_CASES` points the test at other cases.

use std::path::Path;

use org_core::tblfm::{self, Mode};
use serde_json::{json, Value};

/// Emacs's curved quotes in messages, as the server writes them.
fn plain(message: &str) -> String {
    message.replace(['‘', '’'], "'").replace('`', "'")
}

#[test]
fn recalculates_as_emacs_does() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/tblfm/cases.json");
    let path = std::env::var("ORG_TBLFM_CASES").map_or(path, Into::into);
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for case in &cases {
        let marked = case["text"].as_str().unwrap();
        let offset = marked.find('¦').or_else(|| marked.find('|')).unwrap();
        let text = marked.replacen('¦', "", 1);
        let mode = match case["command"].as_str() {
            Some("line") => Mode::Line,
            Some("iterate") => Mode::Iterate,
            _ => Mode::All,
        };
        let actual = match tblfm::recalculate(&text, offset, mode, &|_| None) {
            Ok(outcome) => json!({ "text": outcome.text }),
            Err(error) => json!({ "error": plain(&error) }),
        };
        let mut expected = case["expected"].clone();
        if let Some(error) = expected["error"].as_str() {
            expected = json!({ "error": plain(error) });
        }
        if actual != expected {
            failures.push(format!(
                "\n{}:\nexpected: {}\nactual:   {}",
                case["name"].as_str().unwrap_or("?"),
                serde_json::to_string_pretty(&expected).unwrap(),
                serde_json::to_string_pretty(&actual).unwrap()
            ));
        }
    }
    assert!(failures.is_empty(), "{} of {} cases differ:{}", failures.len(), cases.len(), failures.join("\n"));
}
