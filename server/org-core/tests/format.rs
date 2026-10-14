//! Document formatting against Emacs.
//!
//! `tests/format/cases.json` is written by
//! `scripts/update-format-fixtures.py`: each document after
//! `org-table-align` on every table and `(org-align-tags t)`.

use std::path::Path;

use org_core::format::{self, Settings};
use org_syntax::TodoKeywords;
use serde_json::Value;

#[test]
fn formats_as_emacs_does() {
    let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/format/cases.json"));
    let path = std::env::var("ORG_FORMAT_CASES").map_or(path.to_path_buf(), Into::into);
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for case in &cases {
        let Some(expected) = case["expected"].as_str() else { continue };
        let text = case["text"].as_str().unwrap();
        let alignment = format::Alignment {
            number: case["numberRegexp"].as_str().and_then(org_core::text::EmacsRegexp::new),
            fraction: case["numberFraction"].as_f64().unwrap_or(0.5),
        };
        let settings = Settings { tags_column: case["tagsColumn"].as_i64().unwrap_or(-77), alignment };
        let changes = format::format(text, &TodoKeywords::default(), &settings);
        let got = format::apply(text, &changes);
        if got != expected {
            failures.push(format!("{text:?}\n  expected {expected:?}\n  got      {got:?}"));
        }
    }
    assert!(failures.is_empty(), "{} of {} cases differ:\n{}", failures.len(), cases.len(), failures.join("\n"));
}
