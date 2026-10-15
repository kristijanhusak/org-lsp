//! Cutting, copying and pasting subtrees against Emacs.
//!
//! `tests/subtree/cases.json` is written by
//! `scripts/update-subtree-fixtures.py`: a document, where the cursor is,
//! the command, and the document, cursor and what was copied after it, as
//! Emacs leaves them.

use std::path::Path;

use org_core::indent::{self, Adapt};
use org_core::structure::{self, Settings};
use serde_json::Value;

fn byte(text: &str, chars: usize) -> usize {
    text.char_indices().nth(chars).map_or(text.len(), |(at, _)| at)
}

#[test]
fn cuts_copies_and_pastes_as_emacs_does() {
    let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/subtree/cases.json"));
    let path = std::env::var("ORG_SUBTREE_CASES").map_or(path.to_path_buf(), Into::into);
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for case in &cases {
        let text = case["text"].as_str().unwrap();
        let point = byte(text, case["point"].as_u64().unwrap() as usize);
        let op = case["op"].as_str().unwrap();
        let adapt = match case["adapt"].as_str() {
            Some("t") => Adapt::All,
            Some("headline-data") => Adapt::HeadlineData,
            _ => Adapt::Off,
        };
        let settings = Settings {
            tags_column: -77,
            indent: indent::Settings { adapt, ..indent::Settings::default() },
            allow_promoting_top_level: false,
        };
        let count = case["count"].as_u64().unwrap_or(1) as usize;
        let outcome = match op {
            "cut" | "copy" => structure::copy_subtree(text, point, count, op == "cut")
                .map(|(outcome, copied, _)| (outcome, Some(copied))),
            // Where Org yanks the text as it is, the editor's own paste.
            "yank" => {
                let kill = case["kill"].as_str().unwrap();
                match structure::yank_subtree(text, point, kill, &settings) {
                    Some(yanked) => yanked.map(|(outcome, _)| (outcome, None)),
                    None => {
                        let text = format!("{}{kill}{}", &text[..point], &text[point..]);
                        Ok((org_core::newline::Outcome { text, cursor: point + kill.len() }, None))
                    }
                }
            }
            _ => {
                let level = case["level"].as_u64().map(|level| level as usize);
                structure::paste_subtree(text, point, case["kill"].as_str().unwrap(), level, &settings)
                    .map(|(outcome, _)| (outcome, None))
            }
        };
        let expected = &case["expected"];
        let show = |text: &str, at: usize| {
            format!("{:?}", format!("{}§{}", &text[..at.min(text.len())], &text[at.min(text.len())..]))
        };
        let name = format!("{op} {} {:?}", case["count"], case["kill"].as_str().unwrap_or(""));
        match (outcome, expected.get("error")) {
            (Err(_), Some(_)) => {}
            (Err(refusal), None) => {
                failures.push(format!("{name} {}\n  refused {}", show(text, point), refusal.message()))
            }
            (Ok((outcome, _)), Some(error)) => failures.push(format!(
                "{name} {}\n  expected error {error}\n  got      {}",
                show(text, point),
                show(&outcome.text, outcome.cursor)
            )),
            (Ok((outcome, copied)), None) => {
                let want = expected["text"].as_str().unwrap();
                let cursor = byte(want, expected["point"].as_u64().unwrap() as usize);
                let want_copied = expected["kill"].as_str().filter(|kill| *kill != "json-null");
                if outcome.text != want || outcome.cursor != cursor || copied.as_deref() != want_copied {
                    failures.push(format!(
                        "{name} {}\n  expected {} copied {want_copied:?}\n  got      {} copied {copied:?}",
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
