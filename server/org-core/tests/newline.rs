//! RET (`org-return`) against Emacs.
//!
//! `tests/return/cases.json` is written by
//! `scripts/update-return-fixtures.py`: a document, where the cursor is,
//! and the document and cursor after the key, as Emacs leaves them.

use std::path::Path;

use org_core::indent::{self, Adapt};
use org_core::newline::{self, Settings};
use org_syntax::TodoKeywords;
use serde_json::Value;

fn byte(text: &str, chars: usize) -> usize {
    text.char_indices().nth(chars).map_or(text.len(), |(at, _)| at)
}

#[test]
fn returns_as_emacs_does() {
    let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/return/cases.json"));
    let path = std::env::var("ORG_RETURN_CASES").map_or(path.to_path_buf(), Into::into);
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for case in &cases {
        let key = case["key"].as_str().unwrap_or("RET");
        let expected = &case["expected"];
        if expected.get("error").is_some() || key != "RET" {
            continue;
        }
        let text = case["text"].as_str().unwrap();
        let point = byte(text, case["point"].as_u64().unwrap() as usize);
        let adapt = match case["adapt"].as_str() {
            Some("t") => Adapt::All,
            Some("headline-data") => Adapt::HeadlineData,
            _ => Adapt::Off,
        };
        let settings = Settings { indent: indent::Settings { adapt, ..indent::Settings::default() }, tags_column: -77 };
        let Ok(outcome) = newline::org_return(text, point, &TodoKeywords::default(), &settings) else {
            // In a source block's code Emacs indents as the language does;
            // the editor's own RET applies there.
            let line_end = text[point..].find('\n').map_or(text.len(), |at| point + at);
            assert!(text[..line_end].to_lowercase().contains("#+begin_src"), "refused {text:?}");
            continue;
        };
        let want_text = expected["text"].as_str().unwrap();
        let want_cursor = byte(want_text, expected["point"].as_u64().unwrap() as usize);
        if outcome.text != want_text || outcome.cursor != want_cursor {
            let show = |text: &str, at: usize| format!("{:?}", format!("{}§{}", &text[..at], &text[at..]));
            failures.push(format!(
                "{}\n  expected {}\n  got      {}",
                show(text, point),
                show(want_text, want_cursor),
                show(&outcome.text, outcome.cursor.min(outcome.text.len()))
            ));
        }
    }
    assert!(failures.is_empty(), "{} of {} cases differ:\n{}", failures.len(), cases.len(), failures.join("\n"));
}
