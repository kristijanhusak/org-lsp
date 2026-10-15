//! `org-beginning-of-line`, `org-end-of-line` and `org-kill-line`, as
//! Emacs ran them from every position of the texts of
//! `tests/line/cases.json` (`scripts/line_reference.py`).

use org_core::line::{beginning_of_line, end_of_line, kill_line, KillSettings, Special};
use org_syntax::TodoKeywords;
use serde_json::Value;

fn special(value: &Value) -> Special {
    match value {
        Value::Bool(true) => Special::On,
        Value::String(name) if name == "reversed" => Special::Reversed,
        _ => Special::Off,
    }
}

/// `org-special-ctrl-a/e` for `C-a` (`first`) or `C-e`.
fn special_of(value: &Value, first: bool) -> Special {
    match value {
        Value::Array(pair) => special(&pair[if first { 0 } else { 1 }]),
        value => special(value),
    }
}

#[test]
fn line_commands_as_emacs_runs_them() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/line/cases.json");
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let todo = TodoKeywords::default();
    let mut failures = Vec::new();
    for case in &cases {
        let text = case["text"].as_str().unwrap();
        let command = case["command"].as_str().unwrap();
        let times = case["times"].as_u64().unwrap_or(1);
        let count = case.get("count").and_then(Value::as_i64).unwrap_or(1);
        let starts = text.char_indices().map(|(at, _)| at).chain([text.len()]);
        for (start, expected) in starts.zip(case["expected"].as_array().unwrap()) {
            let got = match command {
                "org-kill-line" => {
                    let settings =
                        KillSettings { special: case["special"] == true, auto_align_tags: true, tags_column: -77 };
                    let mut now = text.to_string();
                    let mut at = start;
                    let mut killed = String::new();
                    let mut error = None;
                    for _ in 0..times {
                        match kill_line(&now, at, settings) {
                            Ok(done) => {
                                now = done.text;
                                at = done.cursor;
                                killed.push_str(&done.kill);
                            }
                            Err(message) => {
                                error = Some(message);
                                break;
                            }
                        }
                    }
                    match error {
                        Some(message) => serde_json::json!({ "error": message }),
                        None => serde_json::json!({ "point": at, "text": now, "kill": killed }),
                    }
                }
                _ => {
                    let first = command == "org-beginning-of-line";
                    let special = special_of(&case["special"], first);
                    let mut at = start;
                    for time in 0..times {
                        at = if first {
                            beginning_of_line(text, at, count, special, time > 0, &todo)
                        } else {
                            end_of_line(text, at, count, special, time > 0, &todo)
                        };
                    }
                    serde_json::json!({ "point": at })
                }
            };
            if &got != expected {
                failures.push(format!(
                    "{command} {} x{times} (count {count}) at {start} of {text:?}: {got}, Emacs {expected}",
                    case["special"]
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
}
