//! M-RET, M-S-RET, C-RET and C-S-RET against Emacs.
//!
//! `tests/meta-return/cases.json` is written by
//! `scripts/update-meta-return-fixtures.py`: a document, where the cursor
//! is, the key, and the document and cursor after the key, as Emacs leaves
//! them.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use org_core::meta_return::{self, Settings};
use org_syntax::TodoKeywords;
use serde_json::Value;

/// The start of a list item.
static ITEM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[ \t]*([-+*]|[0-9]+[.)])([ \t]|$)").unwrap());

/// An item with nothing after its bullet, counter, checkbox and tag: from
/// the end of its line Emacs puts the new item before it, where the keys
/// of modal editors put it below.
static EMPTY_ITEM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^[ \t]*([-+*]|[0-9]+[.)])([ \t]+|$)(\[@[0-9a-zA-Z]+\][ \t]*)?(\[[ X-]\]([ \t]+|$))?(.*[ \t]::)?[ \t]*$",
    )
    .unwrap()
});

fn byte(text: &str, chars: usize) -> usize {
    text.char_indices().nth(chars).map_or(text.len(), |(at, _)| at)
}

#[test]
fn inserts_as_emacs_does() {
    let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/meta-return/cases.json"));
    let path = std::env::var("ORG_META_RETURN_CASES").map_or(path.to_path_buf(), Into::into);
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let keywords = TodoKeywords::default();
    let mut failures = Vec::new();
    for case in &cases {
        let text = case["text"].as_str().unwrap();
        let point = byte(text, case["point"].as_u64().unwrap() as usize);
        // `below:M-RET`: from the keys of modal editors, below the line.
        let key = case["key"].as_str().unwrap();
        let below = key.starts_with("below:");
        let name = key;
        let key = key.trim_start_matches("below:");
        let settings = Settings { tags_column: -77, below, statistics: Default::default() };
        let line_start = text[..point].rfind('\n').map_or(0, |at| at + 1);
        let line_end = text[point..].find('\n').map_or(text.len(), |at| point + at);
        if below && EMPTY_ITEM.is_match(&text[line_start..line_end]) {
            continue;
        }
        let expected = &case["expected"];
        let outcome = match key {
            "M-RET" => meta_return::meta_return(text, point, &keywords, &settings),
            "M-S-<return>" => meta_return::insert_todo_heading(text, point, &keywords, &settings, false),
            "C-<return>" => Ok(meta_return::insert_heading_after(text, point, &keywords, &settings)),
            "C-S-<return>" => meta_return::insert_todo_heading(text, point, &keywords, &settings, true),
            _ => panic!("unknown key {key}"),
        };
        let show = |text: &str, at: usize| format!("{:?}", format!("{}§{}", &text[..at], &text[at..]));
        let outcome = match (outcome, expected.get("error")) {
            (Err(_), Some(_)) => continue,
            // Emacs loops forever on a few broken lists; anything else is
            // better.
            (Ok(_), Some(error)) if error == "Emacs did not finish" => continue,
            (Ok(outcome), Some(error)) => {
                failures.push(format!(
                    "{name} {}\n  expected error {error}\n  got      {}",
                    show(text, point),
                    show(&outcome.text, outcome.cursor.min(outcome.text.len()))
                ));
                continue;
            }
            (Err(refusal), None) => {
                failures.push(format!("{name} {}\n  refused {}", show(text, point), refusal.message()));
                continue;
            }
            (Ok(outcome), None) => outcome,
        };
        let want_text = expected["text"].as_str().unwrap();
        let want_cursor = byte(want_text, expected["point"].as_u64().unwrap() as usize);
        // When Emacs's own list code leaves the new item without a bullet,
        // `org-insert-item` puts the cursor where the stale match data of
        // the checkbox count says; the port leaves it at the line's start.
        let line_start = outcome.text[..outcome.cursor.min(outcome.text.len())].rfind('\n').map_or(0, |at| at + 1);
        let no_bullet = !ITEM.is_match(&outcome.text[line_start..]);
        if outcome.text == want_text && no_bullet && key != "C-<return>" && key != "C-S-<return>" {
            continue;
        }
        if outcome.text != want_text || outcome.cursor != want_cursor {
            failures.push(format!(
                "{name} {}\n  expected {}\n  got      {}",
                show(text, point),
                show(want_text, want_cursor),
                show(&outcome.text, outcome.cursor.min(outcome.text.len()))
            ));
        }
    }
    assert!(failures.is_empty(), "{} of {} cases differ:\n{}", failures.len(), cases.len(), failures.join("\n"));
}
