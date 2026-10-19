//! Clock tables against Emacs: `tests/clocktable/cases.json` holds files,
//! where the cursor is, the clock table commands run on them and what
//! Emacs made of them (see `scripts/clocktable_reference.py`).
//! `ORG_CLOCKTABLE_CASES` points the test at another file.

use std::path::PathBuf;

use org_core::clocktable::{self, Context, Files, Settings};
use org_core::date::{Date, DateTime};
use serde_json::Value;

/// The files of a case, in `/org`.
struct Folder {
    files: Vec<(String, String)>,
}

impl Files for Folder {
    fn agenda_files(&self) -> Vec<String> {
        let mut out = vec!["/org/main.org".to_string()];
        out.extend(
            self.files.iter().filter(|(name, _)| !name.ends_with("_archive")).map(|(name, _)| format!("/org/{name}")),
        );
        out
    }

    fn archive_files(&self, path: &str, _: &str) -> Vec<String> {
        let archive = format!("{path}_archive");
        self.files.iter().any(|(name, _)| format!("/org/{name}") == archive).then_some(archive).into_iter().collect()
    }

    fn read(&self, path: &str) -> Option<String> {
        self.files.iter().find(|(name, _)| format!("/org/{name}") == path).map(|(_, text)| text.clone())
    }
}

fn clock(text: &str) -> DateTime {
    let (date, time) = text.split_once(' ').unwrap();
    let parts: Vec<i64> = date.split('-').map(|part| part.parse().unwrap()).collect();
    let (hour, minute) = time.split_once(':').unwrap();
    DateTime::new(Date::new(parts[0], parts[1], parts[2]), hour.parse().unwrap(), minute.parse().unwrap())
}

fn settings(case: &Value) -> Settings {
    let mut settings = Settings { now: clock(case["clock"].as_str().unwrap()), ..Settings::default() };
    let options = case.get("options").cloned().unwrap_or(Value::Null);
    if let Some(defaults) = options.get("org_clock_clocktable_default_properties").and_then(Value::as_str) {
        settings.default_properties = defaults.to_string();
    }
    if let Some(hour) = options.get("org_extend_today_until").and_then(Value::as_i64) {
        settings.extend_today_until = hour;
    }
    if let Some(defaults) = options.get("org_clocktable_defaults").and_then(Value::as_str) {
        settings.defaults = defaults.to_string();
    }
    if let Some(languages) = options.get("org_clock_clocktable_language_setup").and_then(Value::as_array) {
        settings.languages = languages
            .iter()
            .map(|entry| {
                let words: Vec<String> =
                    entry.as_array().unwrap().iter().map(|word| word.as_str().unwrap().to_string()).collect();
                (words[0].clone(), words[1..].to_vec())
            })
            .collect();
    }
    // `org-clock-report-include-clocking-task`, with the clock that runs
    // started then.
    if options.get("org_clock_report_include_clocking_task") == Some(&Value::Bool(true)) {
        if let Some(clocking) = case.get("clocking").and_then(Value::as_array) {
            let start = clock(clocking[1].as_str().unwrap());
            settings.clocking = Some(clocktable::Clocking { path: "/org/main.org".to_string(), start });
        }
    }
    settings
}

/// Runs the steps of a case: the text with `¦` at the cursor, or the
/// error.
fn run(case: &Value) -> Result<String, String> {
    let marked = case["text"].as_str().unwrap();
    let mut cursor = marked.find('¦').unwrap_or(0);
    let mut text = marked.replacen('¦', "", 1);
    let files = Folder {
        files: case
            .get("files")
            .and_then(Value::as_array)
            .map(|files| {
                files
                    .iter()
                    .map(|pair| (pair[0].as_str().unwrap().to_string(), pair[1].as_str().unwrap().to_string()))
                    .collect()
            })
            .unwrap_or_default(),
    };
    let settings = settings(case);
    let context = Context { path: Some("/org/main.org"), settings: &settings, files: &files };
    for step in case["steps"].as_array().unwrap() {
        let step: Vec<&str> = step.as_array().unwrap().iter().map(|part| part.as_str().unwrap()).collect();
        let done = match step[0] {
            "goto" => {
                cursor = text.find(step[1]).unwrap();
                continue;
            }
            "update" => clocktable::update(&text, cursor, step.len() > 1, &context)?,
            "report" => clocktable::report(&text, cursor, step.len() > 1, &context)?,
            "ctrl-c" => {
                assert!(clocktable::at_block_line(&text, cursor), "C-c C-c away from a block");
                clocktable::update_here(&text, cursor, &context)?
            }
            "shift" => {
                assert!(clocktable::at_clocktable_begin(&text, cursor), "a shift away from a clock table");
                let count = step.get(2).map(|count| count.parse().unwrap());
                clocktable::shift(&text, cursor, matches!(step[1], "right" | "up"), count, &context)?
            }
            other => panic!("unknown step {other}"),
        };
        text = done.text;
        cursor = done.cursor;
    }
    text.insert(cursor, '¦');
    Ok(text)
}

#[test]
fn clock_tables_match_emacs() {
    let path = match std::env::var_os("ORG_CLOCKTABLE_CASES") {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/clocktable/cases.json"),
    };
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let shown = std::env::var("ORG_SHOW_FAILURES").ok().and_then(|count| count.parse().ok()).unwrap_or(8);
    let mut failures = String::new();
    let mut failed = 0;
    for (index, case) in cases.iter().enumerate() {
        let expected = case["expected"].as_str().expect("run scripts/clocktable_reference.py first");
        let actual = run(case);
        let same = match &actual {
            Ok(actual) => actual == expected,
            Err(_) => expected.starts_with("ERROR:"),
        };
        if !same {
            failed += 1;
            if failed <= shown {
                failures.push_str(&format!(
                    "\ncase {index}: {} {}\n--- emacs\n{expected}\n--- org-lsp\n{}\n",
                    case["steps"],
                    case["text"].as_str().unwrap().lines().find(|line| line.contains("#+BEGIN")).unwrap_or(""),
                    actual.unwrap_or_else(|error| format!("ERROR: {error}")),
                ));
            }
        }
    }
    assert!(failed == 0, "{failed} of {} cases differ from Emacs:{failures}", cases.len());
}
