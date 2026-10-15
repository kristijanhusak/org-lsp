//! TAB on new, empty headlines and items (`org-cycle-level`,
//! `org-cycle-item-indentation`), as Emacs pressed it in the cases of
//! `tests/cycle/cases.json` (`scripts/cycle_reference.py`). Where neither
//! applies, TAB folds, which leaves the text as it is.

use org_core::list::{cycle_item_indentation, Settings as ListSettings, TabIndState};
use org_core::structure::{cycle_level, Settings};
use org_syntax::TodoKeywords;
use serde_json::{json, Value};

#[test]
fn tab_on_new_entries_as_emacs_presses_it() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/cycle/cases.json");
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let keywords = TodoKeywords::default();
    let settings = Settings { tags_column: -77, indent: Default::default() };
    let list_settings = ListSettings::default();
    let mut failures = Vec::new();
    for case in &cases {
        let mut text = case["text"].as_str().unwrap().to_string();
        let mut at = case["point"].as_u64().unwrap() as usize;
        let mut state: Option<TabIndState> = None;
        for (press, expected) in case["expected"].as_array().unwrap().iter().enumerate() {
            let got = if let Some(outcome) = cycle_level(&text, at, &keywords, &settings) {
                state = None;
                json!({ "text": outcome.text, "point": outcome.cursor })
            } else {
                match cycle_item_indentation(&text, at, state.as_ref(), &keywords, -77, &list_settings) {
                    Some(Ok(cycled)) => {
                        state = cycled.continues.then_some(cycled.state);
                        json!({ "text": cycled.text, "point": cycled.cursor })
                    }
                    Some(Err(org_core::Refusal::Failed(message) | org_core::Refusal::NotHere(message))) => {
                        json!({ "error": message })
                    }
                    None => {
                        state = None;
                        json!({ "text": text, "point": at })
                    }
                }
            };
            if &got != expected {
                failures.push(format!(
                    "{:?} at {}, press {}: {got}, Emacs {expected}",
                    case["text"],
                    case["point"],
                    press + 1
                ));
                break;
            }
            if let (Some(new), Some(point)) = (got["text"].as_str(), got["point"].as_u64()) {
                text = new.to_string();
                at = point as usize;
            }
        }
    }
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
}
