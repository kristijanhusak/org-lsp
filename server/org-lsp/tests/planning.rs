//! Conformance against Emacs: `tests/planning/cases.json` holds documents,
//! a cursor, a command that asks for a date, the answer, and what Emacs
//! made of them (see `scripts/planning_reference.py`). `ORG_PLANNING_CASES`
//! points the test at another file, which is how `scripts/fuzz-planning.py`
//! feeds it generated cases.
//!
//! The cursor is compared after the timestamp commands, which leave it
//! after what they inserted. After scheduling, the editor keeps its
//! cursor where it was.

use std::path::PathBuf;

use org_core::date::DateTime;
use org_core::indent::Adapt;
use org_core::planning::{prompt, remove, run, Command};
use org_core::read_date::Settings;
use org_core::todo::{log_planning_change, planning_logging, Settings as TodoSettings};
use org_syntax::Logging;
use serde_json::Value;

fn cases() -> Vec<Value> {
    let path = match std::env::var_os("ORG_PLANNING_CASES") {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/planning/cases.json"),
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
fn date_commands_match_emacs() {
    let cases = cases();
    let mut failures = String::new();
    let mut failed = 0;
    let mut compared = 0;
    for (index, case) in cases.iter().enumerate() {
        let expected = case["expected"].as_str().expect("run scripts/update-planning-fixtures.py first");
        let marked = case["text"].as_str().unwrap();
        let offset = marked.find('¦').expect("cursor marker");
        let text = marked.replacen('¦', "", 1);
        let answer = case["answer"].as_str().unwrap();
        let removing = case["command"].as_str().unwrap().ends_with("-remove");
        let command = match case["command"].as_str().unwrap() {
            "schedule" | "schedule-remove" => Command::Schedule,
            "deadline" | "deadline-remove" => Command::Deadline,
            "timestamp" => Command::Timestamp { inactive: false },
            "timestamp-inactive" => Command::Timestamp { inactive: true },
            other => panic!("unknown command {other}"),
        };
        let now = clock(case["clock"].as_str().unwrap());
        let repeated = case.get("repeated").and_then(Value::as_bool).unwrap_or(false);
        let keeps_cursor = matches!(command, Command::Schedule | Command::Deadline);
        let adapt = match case.pointer("/options/org_adapt_indentation") {
            Some(Value::Bool(true)) => Adapt::All,
            Some(Value::String(name)) if name == "headline-data" => Adapt::HeadlineData,
            _ => Adapt::Off,
        };
        let ran = if removing {
            remove(command, &text, offset)
        } else {
            run(command, &text, offset, answer, repeated, now, &Settings::default(), adapt)
        };
        // What `org-log-reschedule` and `org-log-redeadline` log.
        let logging = |name: &str| match case.pointer(&format!("/options/{name}")).and_then(Value::as_str) {
            Some("time") => Some(Logging::Time),
            Some("note") => Some(Logging::Note),
            _ => None,
        };
        let todo = TodoSettings {
            log_reschedule: logging("org_log_reschedule"),
            log_redeadline: logging("org_log_redeadline"),
            log_into_drawer: (case.pointer("/options/org_log_into_drawer") == Some(&Value::Bool(true)))
                .then(|| "LOGBOOK".to_string()),
            adapt,
            ..TodoSettings::default()
        };
        let ran = ran.map(|mut outcome| {
            if let Some(changed) = outcome.changed.clone() {
                if let Some(logged) = planning_logging(&todo, &outcome.text, changed.change) {
                    let note = if logged == Logging::Note { case["note"].as_str().unwrap_or("") } else { "" };
                    let keywords = org_syntax::parse(&outcome.text, &Default::default()).todo_keywords().clone();
                    outcome.text = log_planning_change(
                        &outcome.text,
                        &keywords,
                        changed.heading,
                        &todo,
                        now,
                        changed.change,
                        &changed.old,
                        note,
                    )
                    .unwrap();
                }
            }
            outcome
        });
        let actual = match ran {
            Ok(outcome) => match outcome.cursor {
                Some(cursor) => format!("{}¦{}", &outcome.text[..cursor], &outcome.text[cursor..]),
                None => outcome.text,
            },
            Err(_) => "ERROR".to_string(),
        };
        let expected = if expected.starts_with("ERROR:") {
            "ERROR".to_string()
        } else if keeps_cursor {
            expected.replacen('¦', "", 1)
        } else {
            expected.to_string()
        };
        // What the prompt offers, where Emacs got as far as asking.
        let prompt_value = prompt(command, &text, offset).map(|prompt| prompt.value).unwrap_or_default();
        let expected_prompt = case["prompt"].as_str().unwrap_or("");
        compared += 1;
        if actual != expected || (expected != "ERROR" && prompt_value != expected_prompt) {
            failed += 1;
            failures.push_str(&format!(
                "\n--- case {index}: {} {answer:?} (prompt {expected_prompt:?}, server {prompt_value:?})\n{marked}\n  expected:\n{expected}\n  actual:\n{actual}\n",
                case["command"].as_str().unwrap(),
            ));
        }
    }
    assert!(failed == 0, "{failed} of {compared} date commands differ from Emacs:{failures}");
}
