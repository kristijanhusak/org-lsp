//! Sorting entries against Emacs.
//!
//! `tests/sort/cases.json` is written by `scripts/update-sort-fixtures.py`:
//! a document, where the cursor is, the sorting type, and the document and
//! cursor after `org-sort-entries`, as Emacs leaves them (in the C locale,
//! on Monday 18 May 2026 at 10:30), or its error.

use std::path::Path;

use org_core::date::{Date, DateTime};
use org_core::priority::Priorities;
use org_core::sort::{self, Settings};
use org_syntax::TodoKeywords;
use serde_json::Value;

fn byte(text: &str, chars: usize) -> usize {
    text.char_indices().nth(chars).map_or(text.len(), |(at, _)| at)
}

#[test]
fn sorts_as_emacs_does() {
    let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/sort/cases.json"));
    let path = std::env::var("ORG_SORT_CASES").map_or(path.to_path_buf(), Into::into);
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for case in &cases {
        let text = case["text"].as_str().unwrap();
        let point = byte(text, case["point"].as_u64().unwrap() as usize);
        let kind = case["type"].as_str().unwrap().chars().next().unwrap();
        let keywords = TodoKeywords::from_document(text).unwrap_or_default();
        let settings = Settings {
            keywords: &keywords,
            priorities: Priorities::default(),
            now: DateTime::new(Date::new(2026, 5, 18), 10, 30),
        };
        let outcome = sort::sort_entries(text, point, kind, case["property"].as_str(), &settings);
        let expected = &case["expected"];
        let show = |text: &str, at: usize| {
            format!("{:?}", format!("{}§{}", &text[..at.min(text.len())], &text[at.min(text.len())..]))
        };
        let name = format!("{kind} {:?}", case["property"].as_str().unwrap_or(""));
        match (outcome, expected.get("error").and_then(Value::as_str)) {
            (Err(refusal), Some(error)) if refusal.message() == error => {}
            (Err(refusal), _) => failures.push(format!(
                "{name} {}\n  expected {}\n  refused  {}",
                show(text, point),
                expected,
                refusal.message()
            )),
            (Ok(outcome), Some(error)) => failures.push(format!(
                "{name} {}\n  expected error {error}\n  got      {}",
                show(text, point),
                show(&outcome.text, outcome.cursor)
            )),
            (Ok(outcome), None) => {
                let want = expected["text"].as_str().unwrap();
                let cursor = byte(want, expected["point"].as_u64().unwrap() as usize);
                if outcome.text != want || outcome.cursor != cursor {
                    failures.push(format!(
                        "{name} {}\n  expected {}\n  got      {}",
                        show(text, point),
                        show(want, cursor),
                        show(&outcome.text, outcome.cursor)
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{} of {} cases differ:\n{}", failures.len(), cases.len(), failures.join("\n"));
}
