//! Promoting, demoting and moving subtrees against Emacs.
//!
//! `tests/structure/cases.json` is written by
//! `scripts/update-structure-fixtures.py`: a document, where the cursor
//! is, the key, and the document and cursor after the key, as Emacs leaves
//! them. Off a headline the keys do other things there (a word, an item,
//! a table), which these cases leave to others.

use std::path::Path;

use org_core::indent::{self, Adapt};
use org_core::structure::{self, Settings};
use org_core::Refusal;
use org_syntax::TodoKeywords;
use serde_json::Value;

fn byte(text: &str, chars: usize) -> usize {
    text.char_indices().nth(chars).map_or(text.len(), |(at, _)| at)
}

#[test]
fn edits_structure_as_emacs_does() {
    let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/structure/cases.json"));
    let path = std::env::var("ORG_STRUCTURE_CASES").map_or(path.to_path_buf(), Into::into);
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let keywords = TodoKeywords::default();
    let mut failures = Vec::new();
    let mut compared = 0;
    for case in &cases {
        let text = case["text"].as_str().unwrap();
        let point = byte(text, case["point"].as_u64().unwrap() as usize);
        let key = case["key"].as_str().unwrap();
        let adapt = match case["adapt"].as_str() {
            Some("t") => Adapt::All,
            Some("headline-data") => Adapt::HeadlineData,
            _ => Adapt::Off,
        };
        let parse = org_syntax::parse(text, &keywords);
        // `org-allow-promoting-top-level-subtree`, bound for the case.
        let allow_promoting_top_level =
            case["emacs"].as_str().is_some_and(|lisp| lisp.contains("(org-allow-promoting-top-level-subtree t)"));
        let settings = Settings {
            tags_column: -77,
            indent: indent::Settings { adapt, ..indent::Settings::default() },
            allow_promoting_top_level,
        };
        let (count, key) = match key.strip_prefix("C-u ") {
            Some(rest) => {
                let (number, key) = rest.split_once(' ').unwrap();
                (number.parse::<isize>().unwrap(), key)
            }
            None => (1, key),
        };
        let outcome = match key {
            "M-<left>" => structure::change_levels(text, point, -1, false, &keywords, &settings),
            // On a drawer or a block, it is indented; else the headline.
            "M-<right>" => match structure::indent_element(text, point, &parse.syntax(), &settings.indent) {
                Ok(Some((outcome, _))) => Ok(outcome),
                Ok(None) => structure::change_levels(text, point, 1, false, &keywords, &settings),
                Err(refusal) => Err(refusal),
            },
            "M-S-<left>" => structure::change_levels(text, point, -1, true, &keywords, &settings),
            "M-S-<right>" => structure::change_levels(text, point, 1, true, &keywords, &settings),
            "M-<up>" => structure::move_subtree(text, point, -count),
            "M-<down>" => structure::move_subtree(text, point, count),
            _ => panic!("unknown key {key}"),
        };
        let expected = &case["expected"];
        let show = |text: &str, at: usize| {
            format!("{:?}", format!("{}§{}", &text[..at.min(text.len())], &text[at.min(text.len())..]))
        };
        let name = format!("{} (adapt {})", case["key"].as_str().unwrap(), case["adapt"].as_str().unwrap_or("nil"));
        match (outcome, expected.get("error")) {
            (Err(Refusal::NotHere(_)), _) => continue,
            (Err(_), Some(_)) => continue,
            (Err(refusal), None) => {
                failures.push(format!("{name} {}\n  refused {}", show(text, point), refusal.message()))
            }
            (Ok(outcome), Some(error)) => failures.push(format!(
                "{name} {}\n  expected error {error}\n  got      {}",
                show(text, point),
                show(&outcome.text, outcome.cursor)
            )),
            (Ok(outcome), None) => {
                compared += 1;
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
    // Most of the committed cases do something (random ones often stop
    // with an error).
    if std::env::var("ORG_STRUCTURE_CASES").is_err() {
        assert!(compared * 2 > cases.len(), "only {compared} of {} cases compared", cases.len());
    }
}
