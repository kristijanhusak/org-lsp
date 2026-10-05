//! Conformance against Emacs: `tests/todo/cases.json` holds documents, a
//! cursor, a TODO command and what `org-todo` made of them (see
//! `scripts/todo_reference.py`). `ORG_TODO_CASES` points the test at another
//! file, which is how `scripts/fuzz-todo.py` feeds it generated cases.

use std::path::PathBuf;

use org_core::date::DateTime;
use org_core::todo::{change_state, Target};
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
        let default = options.todo_keywords.clone().map(TodoKeywords::from_sequence).unwrap_or_default();
        let keywords = TodoKeywords::from_document(&text).unwrap_or(default);
        let target = match &case["command"] {
            Value::String(name) if name == "next" => Target::Next,
            Value::String(name) if name == "previous" => Target::Previous,
            other => Target::State(other["state"].as_str().map(str::to_string)),
        };
        let now = clock(case["clock"].as_str().unwrap());
        let actual = change_state(&text, &keywords, offset, &target, &options.todo_settings(), now)
            .unwrap_or_else(|error| format!("ERROR: {error}"));
        compared += 1;
        if actual != expected {
            failed += 1;
            if failed <= 12 {
                failures.push_str(&format!(
                    "\ncase {index}: {} {}\n--- text\n{marked}\n--- emacs\n{expected}\n--- org-lsp\n{actual}\n",
                    case["command"], case["options"]
                ));
            }
        }
    }
    assert!(compared > 0, "no cases");
    assert!(failed == 0, "{failed} of {compared} TODO changes differ from Emacs:{failures}");
}
