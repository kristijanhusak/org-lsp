//! Conformance against Emacs: `tests/todo/cases.json` holds documents, a
//! cursor, a TODO command and what `org-todo` made of them (see
//! `scripts/todo_reference.py`). `ORG_TODO_CASES` points the test at another
//! file, which is how `scripts/fuzz-todo.py` feeds it generated cases.

use std::path::PathBuf;

use org_core::date::DateTime;
use org_core::todo::{change_state_with_note, fast_selection, Target};
use org_lsp::config::Options;
use org_syntax::TodoKeywords;
use serde_json::Value;

fn cases() -> Vec<Value> {
    let path = match std::env::var_os("ORG_TODO_CASES") {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/todo/cases.json"),
    };
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display())))
        .unwrap()
}

fn clock(text: &str) -> DateTime {
    let (date, time) = text.split_once(' ').expect("date and time");
    let day = org_lsp::cli::parse_date(date).unwrap();
    DateTime { day, minute: org_lsp::cli::parse_time(time).unwrap() }
}

#[test]
fn todo_changes_match_emacs() {
    let cases = cases();
    let mut failures = String::new();
    let mut failed = 0;
    let mut compared = 0;
    for (index, case) in cases.iter().enumerate() {
        let expected = case["expected"].as_str().expect("run scripts/update-todo-fixtures.py first");
        // Where Org itself gives up there is nothing to compare.
        if expected.starts_with("ERROR:") {
            continue;
        }
        let marked = case["text"].as_str().unwrap();
        let offset = marked.find('¦').expect("cursor marker");
        let text = marked.replacen('¦', "", 1);
        let options: Options =
            serde_json::from_value(case.get("options").cloned().unwrap_or_else(|| serde_json::json!({}))).unwrap();
        let default = options.todo_keywords();
        let keywords = TodoKeywords::from_document(&text).unwrap_or(default);
        // `org-toggle-checkbox`, on an item or a headline.
        if case["command"] == "checkbox" {
            let settings = options.todo_settings();
            let actual = org_core::checkbox::toggle(&text, &keywords, offset, settings.tags_column, &settings.lists)
                .unwrap_or_else(|refusal| format!("ERROR: {}", refusal.message()));
            compared += 1;
            if actual != expected {
                failed += 1;
                failures.push_str(&format!(
                    "\ncase {index} (checkbox):\n{marked}\nexpected:\n{expected}\nactual:\n{actual}\n"
                ));
            }
            continue;
        }
        // `org-add-note`, with the case's note.
        if case["command"] == "note" {
            let note = case.get("note").and_then(Value::as_str).unwrap_or("");
            let now = clock(case["clock"].as_str().unwrap());
            let actual = org_core::todo::add_note(&text, &keywords, offset, &options.todo_settings(), now, note)
                .unwrap_or_else(|error| format!("ERROR: {error}"));
            compared += 1;
            if actual != expected {
                failed += 1;
                failures
                    .push_str(&format!("\ncase {index} (note):\n{marked}\nexpected:\n{expected}\nactual:\n{actual}\n"));
            }
            continue;
        }
        let target = match &case["command"] {
            Value::String(name) if name == "next" => Some(Target::Next),
            Value::String(name) if name == "previous" => Some(Target::Previous),
            // `org-todo` with no fast selection keys, and switching sets.
            Value::String(name) if name == "cycle" => Some(Target::Cycle { head: None, repeated: false }),
            Value::String(name) if name == "nextset" => Some(Target::NextSet { head: None }),
            Value::String(name) if name == "previousset" => Some(Target::PreviousSet { head: None }),
            // Fast selection: `QUIT` when the key selects nothing.
            other if other.get("key").is_some() => {
                match fast_selection(&text, &keywords, offset, other["key"].as_str().unwrap()) {
                    Ok(target) => target,
                    Err(error) => {
                        compared += 1;
                        failed += usize::from(!expected.starts_with("ERROR:"));
                        failures.push_str(&format!("\ncase {index}: refused with {error}\n"));
                        continue;
                    }
                }
            }
            other => Some(Target::State(other["state"].as_str().map(str::to_string))),
        };
        let now = clock(case["clock"].as_str().unwrap());
        // The note is written with the text the case gives, or none.
        let note = case.get("note").and_then(Value::as_str).unwrap_or("");
        let actual = match target {
            None => "QUIT".to_string(),
            Some(target) => {
                match change_state_with_note(
                    &text,
                    &keywords,
                    offset,
                    &target,
                    &options.todo_settings(),
                    now,
                    Some(note),
                ) {
                    Ok(changed) => changed.text,
                    // A change Org blocked leaves the text, saying why.
                    Err(error) if case.get("blocked").and_then(Value::as_str) == Some(error.as_str()) => text.clone(),
                    Err(error) => format!("ERROR: {error}"),
                }
            }
        };
        compared += 1;
        if actual != expected {
            failed += 1;
            if failed <= 12 {
                failures.push_str(&format!(
                    "\ncase {index}: {} {} note {:?}\n--- text\n{marked}\n--- emacs\n{expected}\n--- org-lsp\n{actual}\n",
                    case["command"], case["options"], case["note"]
                ));
            }
        }
    }
    assert!(compared > 0, "no cases");
    assert!(failed == 0, "{failed} of {compared} TODO changes differ from Emacs:{failures}");
}
