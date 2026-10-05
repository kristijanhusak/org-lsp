//! Conformance against Emacs: `tests/read-date/cases.json` holds answers to
//! Org's date prompt and what `org-read-date` made of them (see
//! `scripts/read_date_reference.py`). `ORG_READ_DATE_CASES` points the test
//! at another file, which is how `scripts/fuzz-read-date.py` feeds it
//! generated cases.

use std::path::PathBuf;

use org_core::date::DateTime;
use org_core::read_date::{read_date, PreferFuture, Settings};
use serde_json::Value;

fn cases() -> Vec<Value> {
    let path = match std::env::var_os("ORG_READ_DATE_CASES") {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/read-date/cases.json"),
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
fn answers_read_as_emacs_reads_them() {
    let cases = cases();
    let mut failures = String::new();
    let mut failed = 0;
    let mut compared = 0;
    for (index, case) in cases.iter().enumerate() {
        let expected = case["expected"].as_str().expect("run scripts/update-read-date-fixtures.py first");
        let answer = case["answer"].as_str().unwrap();
        let now = clock(case["clock"].as_str().unwrap());
        let default = case.get("default").and_then(Value::as_str).map(clock);
        let prefer_future = match case.get("preferFuture") {
            Some(Value::Bool(false)) => PreferFuture::No,
            Some(Value::String(time)) if time == "time" => PreferFuture::Time,
            _ => PreferFuture::Yes,
        };
        let settings = Settings { prefer_future, ..Settings::default() };
        let actual = match read_date(answer, default, now, &settings) {
            Ok(read) => match &read.end {
                Some(end) => format!("{} | {end}", read.to_org()),
                None => read.to_org(),
            },
            Err(_) => "ERROR".to_string(),
        };
        // Where Org gives up, it is enough that the server does too.
        let expected = if expected.starts_with("ERROR:") { "ERROR" } else { expected };
        compared += 1;
        if actual != expected {
            failed += 1;
            failures.push_str(&format!(
                "\n--- case {index}: {answer:?} at {}{}\n  expected: {expected}\n  actual:   {actual}\n",
                case["clock"].as_str().unwrap(),
                default.map(|_| format!(", default {}", case["default"].as_str().unwrap())).unwrap_or_default(),
            ));
        }
    }
    assert!(failed == 0, "{failed} of {compared} dates differ from Emacs:{failures}");
}
