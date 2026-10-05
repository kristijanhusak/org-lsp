//! Conformance against Emacs: `tests/shift/cases.json` holds documents, a
//! cursor, a priority or timestamp command and what Emacs made of them
//! (see `scripts/shift_reference.py`). `ORG_SHIFT_CASES` points the test
//! at another file, which is how `scripts/fuzz-shift.py` feeds it
//! generated cases.

use std::path::PathBuf;

use org_core::date::DateTime;
use org_core::edit::{document_priorities, run, Call, Command, Direction};
use org_core::priority::{self, Action};
use org_lsp::config::Options;
use org_syntax::TodoKeywords;
use serde_json::Value;

fn cases() -> Vec<Value> {
    let path = match std::env::var_os("ORG_SHIFT_CASES") {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/shift/cases.json"),
    };
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display())))
        .unwrap()
}

fn clock(text: &str) -> DateTime {
    let (date, time) = text.split_once(' ').expect("date and time");
    let day = org_lsp::cli::parse_date(date).unwrap();
    DateTime { day, minute: org_lsp::cli::parse_time(time).unwrap() }
}

fn command(name: &str) -> Command {
    match name {
        "priority-up" => Command::Priority(Action::Up),
        "priority-down" => Command::Priority(Action::Down),
        "timestamp-up" => Command::Timestamp { forward: true },
        "timestamp-down" => Command::Timestamp { forward: false },
        "date-later" => Command::Date { forward: true },
        "date-earlier" => Command::Date { forward: false },
        "shift-up" => Command::Shift(Direction::Up),
        "shift-down" => Command::Shift(Direction::Down),
        "shift-left" => Command::Shift(Direction::Left),
        "shift-right" => Command::Shift(Direction::Right),
        other => panic!("unknown command {other}"),
    }
}

#[test]
fn priority_and_timestamp_changes_match_emacs() {
    let cases = cases();
    let mut failures = String::new();
    let mut failed = 0;
    let mut compared = 0;
    let shown = std::env::var("ORG_SHOW_FAILURES").ok().and_then(|count| count.parse().ok()).unwrap_or(12);
    for (index, case) in cases.iter().enumerate() {
        let expected = case["expected"].as_str().expect("run scripts/update-shift-fixtures.py first");
        let marked = case["text"].as_str().unwrap();
        let offset = marked.find('¦').expect("cursor marker");
        let text = marked.replacen('¦', "", 1);
        let options: Options =
            serde_json::from_value(case.get("options").cloned().unwrap_or_else(|| serde_json::json!({}))).unwrap();
        let default = options.todo_keywords.clone().map(TodoKeywords::from_sequence).unwrap_or_default();
        let parse = org_syntax::parse(&text, &default);
        let call = Call {
            count: case.get("count").and_then(Value::as_i64),
            repeated: case.get("repeated").and_then(Value::as_bool).unwrap_or(false),
            now: clock(case["clock"].as_str().unwrap()),
        };
        let name = case["command"].as_str().unwrap();
        let settings = options.edit_settings();
        // `org-priority` reads its answer against the file's priorities.
        let actual = if name == "priority" {
            let priorities = document_priorities(&parse).unwrap_or(settings.priorities);
            priority::read_answer(case["answer"].as_str().unwrap(), &priorities)
                .and_then(|action| run(&Command::Priority(action), &text, &parse, offset, &call, &settings))
        } else {
            run(&command(name), &text, &parse, offset, &call, &settings)
        };
        compared += 1;
        // Emacs marks where it leaves the cursor. Where the server says
        // where the cursor goes, that must be the place; where Emacs stops
        // with an error, the server must refuse as well.
        let actual = actual.map(|outcome| {
            let mut text = outcome.text;
            if let Some(cursor) = outcome.cursor {
                text.insert(cursor, '¦');
            }
            text
        });
        let same = match &actual {
            Ok(actual) if actual.contains('¦') => actual == expected,
            Ok(actual) => *actual == expected.replacen('¦', "", 1) && !expected.starts_with("ERROR:"),
            Err(_) => expected.starts_with("ERROR:"),
        };
        if !same {
            failed += 1;
            if failed <= shown {
                failures.push_str(&format!(
                    "\ncase {index}: {name} count {} repeated {} answer {} {}\n--- text\n{marked}\n--- emacs\n{expected}\n--- org-lsp\n{}\n",
                    case["count"],
                    call.repeated,
                    case["answer"],
                    case["options"],
                    actual.unwrap_or_else(|refusal| format!("ERROR: {refusal:?}")),
                ));
            }
        }
    }
    assert!(compared > 0, "no cases");
    assert!(failed == 0, "{failed} of {compared} changes differ from Emacs:{failures}");
}
