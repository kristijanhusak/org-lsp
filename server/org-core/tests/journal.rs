//! org-journal's commands against Emacs.
//!
//! `tests/journal/cases.json` is written by
//! `scripts/update-journal-fixtures.py`: the files of a journal directory,
//! the clock, options, a command and what it is given, and what Emacs
//! leaves: the files, the file and place of the cursor, the lines its
//! headlines' folds hide, the messages; or its error. The directory is
//! `/journal/` in messages.

use std::collections::BTreeMap;
use std::path::Path;

use org_core::date::{Date, DateTime};
use org_core::journal::{self, Command, DeleteEmpty, FileType, Request, Settings, Stop};
use org_core::read_date::{read_date, Settings as ReadDateSettings};
use org_syntax::TodoKeywords;
use serde_json::{json, Value};

fn byte(text: &str, chars: usize) -> usize {
    text.char_indices().nth(chars).map_or(text.len(), |(at, _)| at)
}

fn chars(text: &str, bytes: usize) -> usize {
    text[..bytes.min(text.len())].chars().count()
}

fn clock(text: &str) -> DateTime {
    let number = |range: std::ops::Range<usize>| text[range].parse::<i64>().unwrap();
    DateTime::new(Date::new(number(0..4), number(5..7), number(8..10)), number(11..13), number(14..16))
}

fn settings(options: &Value) -> Settings {
    let mut settings = Settings { directory: "/journal/".to_string(), ..Settings::default() };
    let string = |name: &str| options[name].as_str().map(str::to_string);
    if let Some(kind) = string("org_journal_file_type") {
        settings.file_type = match kind.as_str() {
            "weekly" => FileType::Weekly,
            "monthly" => FileType::Monthly,
            "yearly" => FileType::Yearly,
            _ => FileType::Daily,
        };
    }
    if let Some(day) = options["org_journal_start_on_weekday"].as_i64() {
        settings.start_on_weekday = day;
    }
    let texts: [(&str, &mut String); 10] = [
        ("org_journal_file_format", &mut settings.file_format),
        ("org_journal_date_format", &mut settings.date_format),
        ("org_journal_date_prefix", &mut settings.date_prefix),
        ("org_journal_time_format", &mut settings.time_format),
        ("org_journal_time_format_post_midnight", &mut settings.time_format_post_midnight),
        ("org_journal_time_prefix", &mut settings.time_prefix),
        ("org_journal_carryover_items", &mut settings.carryover_items),
        ("org_journal_file_header", &mut settings.file_header),
        ("org_journal_created_property_timestamp_format", &mut settings.created_property_timestamp_format),
        ("org_journal_scheduled_string", &mut settings.scheduled_string),
    ];
    for (name, field) in texts {
        if let Some(value) = string(name) {
            *field = value;
        }
    }
    if let Some(hide) = options["org_journal_hide_entries_p"].as_bool() {
        settings.hide_entries = hide;
    }
    if let Some(drawers) = options["org_journal_skip_carryover_drawers"].as_array() {
        settings.skip_carryover_drawers = drawers.iter().filter_map(Value::as_str).map(str::to_string).collect();
    }
    if let Some(delete) = string("org_journal_carryover_delete_empty_journal") {
        settings.carryover_delete_empty_journal = match delete.as_str() {
            "always" => DeleteEmpty::Always,
            "ask" => DeleteEmpty::Ask,
            _ => DeleteEmpty::Never,
        };
    }
    if let Some(on) = options["org_journal_enable_encryption"].as_bool() {
        settings.enable_encryption = on;
    }
    if let Some(hour) = options["org_extend_today_until"].as_i64() {
        settings.extend_today_until = hour;
    }
    if let Some(column) = options["org_tags_column"].as_i64() {
        settings.tags.tags_column = column;
    }
    if let Some(words) = options["org_todo_keywords"].as_array() {
        settings.properties.keywords = TodoKeywords::from_sequence(words.iter().filter_map(Value::as_str));
    }
    settings
}

/// What the port leaves, as the reference records it.
fn run(case: &Value) -> Value {
    let settings = settings(&case["options"]);
    let now = clock(case["clock"].as_str().unwrap());
    let files: BTreeMap<String, String> = case["files"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, text)| (name.clone(), text.as_str().unwrap().to_string()))
        .collect();
    let prefix = case["prefix"].as_u64().unwrap_or(0) > 0;
    let date = || {
        read_date(case["date"].as_str().unwrap_or(""), None, now, &ReadDateSettings::default())
            .expect("a date the prompt reads")
    };
    if case["command"] == "agenda-files" {
        let names: Vec<String> = files.keys().cloned().collect();
        return match journal::agenda_files(&names, now, &settings) {
            Ok(agenda) => json!({
                "buffer": null, "point": null, "hidden": [], "agenda": agenda, "files": files, "messages": [],
            }),
            Err(message) => json!(format!("ERROR: {message}")),
        };
    }
    let command = match case["command"].as_str().unwrap() {
        "new-entry" => Command::NewEntry { prefix },
        "new-date-entry" => Command::NewDateEntry { prefix, date: date() },
        "new-scheduled-entry" => Command::NewScheduledEntry { prefix, date: date() },
        "next" => Command::Next,
        "previous" => Command::Previous,
        "open-current" => Command::OpenCurrent,
        other => panic!("unknown command {other}"),
    };
    let from_file = case["from"].as_str();
    let from = from_file.map(|file| (file, byte(&files[file], case["point"].as_u64().unwrap_or(0) as usize)));
    let mut answers: Vec<bool> = Vec::new();
    let given: Vec<bool> = case["answers"]
        .as_array()
        .map(|answers| answers.iter().map(|answer| matches!(answer.as_str(), Some("y" | "yes"))).collect())
        .unwrap_or_default();
    loop {
        let request = Request { files: &files, now, from, answers: &answers };
        match journal::run(&command, &request, &settings) {
            Ok(outcome) => {
                let mut after = files.clone();
                // Emacs saves a changed Org buffer with a final newline
                // (`require-final-newline`).
                for (file, text) in &outcome.changed {
                    match text {
                        Some(text) if !text.is_empty() && !text.ends_with('\n') => {
                            after.insert(file.clone(), format!("{text}\n"))
                        }
                        Some(text) => after.insert(file.clone(), text.clone()),
                        None => after.remove(file),
                    };
                }
                let (buffer, point, hidden) = match &outcome.visit {
                    Some(visit) => {
                        let text = after.get(&visit.file).cloned().unwrap_or_default();
                        (json!(visit.file), json!(chars(&text, visit.offset)), json!(visit.hidden_lines))
                    }
                    None => (Value::Null, Value::Null, json!([])),
                };
                return json!({
                    "buffer": buffer,
                    "point": point,
                    "hidden": hidden,
                    "files": after,
                    "messages": outcome.messages,
                });
            }
            Err(Stop::Ask(question)) => match given.get(answers.len()) {
                Some(answer) => answers.push(*answer),
                None => return json!(format!("ERROR: No answer to: {question}")),
            },
            Err(Stop::Error(message)) => return json!(format!("ERROR: {message}")),
        }
    }
}

#[test]
fn runs_org_journal_as_emacs_does() {
    let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/journal/cases.json"));
    let path = std::env::var("ORG_JOURNAL_CASES").map_or(path.to_path_buf(), Into::into);
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let got = run(case);
        let expected = &case["expected"];
        if &got != expected {
            failures.push(format!(
                "case {index}: {} {}\n  options  {}\n  files    {}\n  expected {}\n  got      {}",
                case["command"],
                case["clock"],
                case["options"],
                serde_json::to_string(&case["files"]).unwrap(),
                expected,
                got
            ));
        }
    }
    assert!(failures.is_empty(), "{} of {} cases differ:\n{}", failures.len(), cases.len(), failures.join("\n"));
}
