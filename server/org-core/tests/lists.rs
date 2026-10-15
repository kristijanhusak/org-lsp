//! Indenting, moving and cycling the bullets of list items, going to the
//! next or previous one, repairing a list with `C-c C-c` and sorting it
//! with `C-c ^`, against Emacs.
//!
//! `tests/lists/cases.json` is written by `scripts/update-lists-fixtures.py`:
//! a list, where the cursor is, the key, and the list and cursor after the
//! key as Emacs leaves them, or the error it shows, and at times the
//! list options the case was recorded with. `ORG_LISTS_CASES` points the
//! test at another file.

use std::path::Path;

use org_core::date::{Date, DateTime};
use org_core::list;
use org_syntax::TodoKeywords;
use serde_json::Value;

fn byte(text: &str, chars: usize) -> usize {
    text.char_indices().nth(chars).map_or(text.len(), |(at, _)| at)
}

/// The list options of a case: `org-list-two-spaces-after-bullet-regexp`,
/// `org-list-demote-modify-bullet`, `org-list-indent-offset`,
/// `org-list-automatic-rules` and `org-checkbox-hierarchical-statistics`.
fn settings(case: &Value) -> list::Settings {
    let options = &case["settings"];
    let defaults = list::Settings::default();
    list::Settings {
        two_spaces: options["two_spaces"].as_str().and_then(list::BulletRegexp::new),
        demote_bullets: options["demote"]
            .as_object()
            .map(|map| map.iter().map(|(old, new)| (old.clone(), new.as_str().unwrap().to_string())).collect())
            .unwrap_or_default(),
        indent_offset: options["indent_offset"].as_u64().map_or(defaults.indent_offset, |offset| offset as usize),
        checkbox_rule: options["checkbox_rule"].as_bool().unwrap_or(defaults.checkbox_rule),
        indent_rule: options["indent_rule"].as_bool().unwrap_or(defaults.indent_rule),
        checkbox_hierarchical: options["hierarchical"].as_bool().unwrap_or(defaults.checkbox_hierarchical),
    }
}

/// How a case writes ordered items: `org-list-allow-alphabetical` and
/// `org-plain-list-ordered-item-terminator`.
fn syntax(case: &Value) -> org_syntax::ListSyntax {
    let options = &case["settings"];
    org_syntax::ListSyntax {
        alphabetical: options["alphabetical"].as_bool().unwrap_or(false),
        terminator: match options["terminator"].as_str() {
            Some(".") => org_syntax::Terminator::Dot,
            Some(")") => org_syntax::Terminator::Paren,
            _ => org_syntax::Terminator::Both,
        },
    }
}

#[test]
fn edits_lists_as_emacs_does() {
    let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/lists/cases.json"));
    let path = std::env::var("ORG_LISTS_CASES").map_or(path.to_path_buf(), Into::into);
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for case in &cases {
        let text = case["text"].as_str().unwrap();
        let keywords = TodoKeywords::from_document(text).unwrap_or_default();
        let point = byte(text, case["point"].as_u64().unwrap() as usize);
        let key = case["key"].as_str().unwrap();
        let circular = case["circular"].as_bool() == Some(true);
        let settings = settings(case);
        let settings = &settings;
        // On a timestamp the shifted keys change it, not the list.
        if key.starts_with("S-<") && !org_core::edit::shifts_item(text, point) {
            continue;
        }
        let outcome = org_syntax::with_list_syntax(syntax(case), || match key {
            "M-<left>" => list::indent_item(text, point, -1, false, &keywords, -77, settings),
            "M-<right>" => list::indent_item(text, point, 1, false, &keywords, -77, settings),
            "M-S-<left>" => list::indent_item(text, point, -1, true, &keywords, -77, settings),
            "M-S-<right>" => list::indent_item(text, point, 1, true, &keywords, -77, settings),
            "M-<up>" => list::move_item(text, point, false, circular, settings),
            "M-<down>" => list::move_item(text, point, true, circular, settings),
            "S-<left>" => list::cycle_bullet(text, point, true, settings),
            "S-<right>" => list::cycle_bullet(text, point, false, settings),
            "S-<up>" | "S-<down>" => list::next_item(text, point, key == "S-<up>", circular)
                .map(|cursor| list::Edited { text: text.to_string(), cursor }),
            sort if sort.starts_with("C-c ^ ") => {
                // Items without a date sort as now, after the dates here.
                let now = DateTime { day: Date::new(2026, 10, 7).to_day(), minute: 0 };
                org_core::sort::sort_list(text, point, sort.chars().last().unwrap(), now, settings)
                    .map(|outcome| list::Edited { text: outcome.text, cursor: outcome.cursor })
            }
            // On a timestamp, `C-c C-c` is about the timestamp.
            "C-c C-c" if org_core::stamp::Stamp::around(text, point).is_some() => {
                let shift = org_core::stamp::Shift {
                    n: 0,
                    what: Some(org_core::stamp::Unit::Day),
                    updown: false,
                    counted: false,
                };
                org_core::stamp::change(text, point, &shift, 5)
                    .map(|changed| list::Edited { text: changed.text, cursor: changed.cursor })
            }
            "C-c C-c" => {
                org_core::checkbox::ctrl_c_ctrl_c(text, &keywords, point, -77, text.contains(":ORDERED: t"), settings)
                    .map(|(text, cursor, _)| list::Edited { text, cursor })
            }
            "M-RET" => {
                let insert = org_core::meta_return::Settings { lists: settings.clone(), ..Default::default() };
                org_core::meta_return::meta_return(text, point, &keywords, &insert)
                    .map(|outcome| list::Edited { text: outcome.text, cursor: outcome.cursor })
            }
            _ => panic!("unknown key {key}"),
        });
        let expected = &case["expected"];
        let show = |text: &str, at: usize| {
            format!("{:?}", format!("{}§{}", &text[..at.min(text.len())], &text[at.min(text.len())..]))
        };
        match (outcome, expected.get("error").and_then(Value::as_str)) {
            (Err(refusal), Some(error)) if refusal.message() == error => {}
            // Which comparison fails first is Emacs's sorting's business.
            (Err(refusal), Some(error))
                if refusal.message().starts_with("Wrong type argument") && error.starts_with("Wrong type argument") => {
            }
            (Err(refusal), Some(error)) => failures.push(format!(
                "{key} {}\n  expected error {error}\n  got error      {}",
                show(text, point),
                refusal.message()
            )),
            (Err(refusal), None) => failures.push(format!(
                "{key} {}\n  expected {}\n  refused  {}",
                show(text, point),
                show(
                    expected["text"].as_str().unwrap(),
                    byte(expected["text"].as_str().unwrap(), expected["point"].as_u64().unwrap() as usize)
                ),
                refusal.message()
            )),
            // Emacs failing on its own, on an ill-formed list: nothing to
            // follow there (sorting aside, whose failures are Org's).
            (Ok(_), Some(error)) if error.starts_with("Wrong type argument") && !key.starts_with("C-c ^") => {}
            (Ok(outcome), Some(error)) => failures.push(format!(
                "{key} {}\n  expected error {error}\n  got      {}",
                show(text, point),
                show(&outcome.text, outcome.cursor)
            )),
            (Ok(outcome), None) => {
                let want = expected["text"].as_str().unwrap();
                let cursor = byte(want, expected["point"].as_u64().unwrap() as usize);
                if outcome.text != want || outcome.cursor != cursor {
                    failures.push(format!(
                        "{key} {}\n  expected {}\n  got      {}",
                        show(text, point),
                        show(want, cursor),
                        show(&outcome.text, outcome.cursor)
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        cases.len(),
        failures.iter().take(15).cloned().collect::<Vec<_>>().join("\n")
    );
}
