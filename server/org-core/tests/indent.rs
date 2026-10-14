//! `org-indent-line` and `org-indent-region` against Emacs.
//!
//! `tests/indent/cases.json` is written by
//! `scripts/update-indent-fixtures.py`: for each document and setting of
//! `org-adapt-indentation`, every line as `org-indent-line` leaves it, the
//! indentation of a line opened after each line, and the document after
//! `org-indent-region`.

use std::path::Path;

use org_core::indent::{self, Adapt, Settings};
use serde_json::Value;

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn settings(case: &Value) -> Settings {
    let adapt = match case["adapt"].as_str() {
        Some("t") => Adapt::All,
        Some("headline-data") => Adapt::HeadlineData,
        _ => Adapt::Off,
    };
    Settings { adapt, ..Settings::default() }
}

/// The lines of source blocks' code, which Emacs indents with the
/// language's mode.
fn code_lines(text: &str) -> Vec<bool> {
    let mut inside = false;
    text.split('\n')
        .map(|line| {
            let lower = line.trim().to_ascii_lowercase();
            if !inside && lower.starts_with("#+begin_src") {
                inside = true;
                false
            } else if lower == "#+end_src" {
                inside = false;
                false
            } else {
                inside
            }
        })
        .collect()
}

fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0).chain(text.match_indices('\n').map(|(at, _)| at + 1)).collect()
}

#[test]
fn indents_as_org_does() {
    // `scripts/fuzz-indent.py` points this at the cases it generated.
    let path = std::env::var("ORG_INDENT_CASES").map_or_else(|_| root().join("tests/indent/cases.json"), Into::into);
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let text = match case["file"].as_str() {
            Some(file) => std::fs::read_to_string(root().join(file)).unwrap(),
            None => case["text"].as_str().unwrap().to_string(),
        };
        let name = format!("case {index}, {} (adapt {})", case["file"].as_str().unwrap_or("text"), case["adapt"]);
        let settings = settings(case);
        let code = code_lines(&text);
        let starts = line_starts(&text);
        let expected = &case["expected"];
        // Org stopped with an error on this document.
        if expected.get("error").is_some() {
            continue;
        }
        for (number, wanted) in expected["lines"].as_array().unwrap().iter().enumerate() {
            if code[number] {
                continue;
            }
            let start = starts[number];
            let got = match indent::indent_line(&text, start, &settings) {
                Some(edit) => indent::apply(&text, &[edit]),
                None => text.clone(),
            };
            let got_line = got[start..].split('\n').next().unwrap();
            if got_line != wanted.as_str().unwrap() {
                failures.push(format!(
                    "{name} line {number}: {:?}\n  expected {wanted}\n  actual   {got_line:?}",
                    text.split('\n').nth(number).unwrap()
                ));
            }
        }
        for (number, wanted) in expected["newline"].as_array().unwrap().iter().enumerate() {
            if code[number] {
                continue;
            }
            let end = starts.get(number + 1).map_or(text.len(), |next| next - 1);
            let opened = format!("{}\n{}", &text[..end], &text[end..]);
            let column = indent::line_column(&opened, end + 1, &settings).unwrap_or(0);
            if column as u64 != wanted.as_u64().unwrap() {
                failures.push(format!(
                    "{name} newline after line {number}: {:?}\n  expected {wanted}\n  actual   {column}",
                    text.split('\n').nth(number).unwrap()
                ));
            }
        }
        // `org-indent-mode`: the prefixes of every line.
        if let Some(virtual_lines) = expected.get("virtual").and_then(Value::as_array) {
            let shown = indent::virtual_indentation(&text, true, false, settings.adapt, None);
            for (number, wanted) in virtual_lines.iter().enumerate() {
                let found = shown.iter().find(|prefix| prefix.line == number);
                let got = found.map_or((0, 0), |prefix| (prefix.prefix, prefix.wrap));
                let wanted = (wanted[0].as_u64().unwrap() as usize, wanted[1].as_u64().unwrap() as usize);
                if got != wanted {
                    failures.push(format!(
                        "{name} virtual line {number}: {:?}\n  expected {wanted:?}\n  actual   {got:?}",
                        text.split('\n').nth(number).unwrap()
                    ));
                }
            }
        }
        let edits = indent::indent_region(&text, 0, text.len(), &settings);
        let got = indent::apply(&text, &edits);
        let wanted = expected["region"].as_str().unwrap();
        if got != wanted {
            let diff: Vec<String> = got
                .split('\n')
                .zip(wanted.split('\n'))
                .enumerate()
                .filter(|(number, (got, wanted))| got != wanted && !code.get(*number).copied().unwrap_or(false))
                .map(|(number, (got, wanted))| {
                    format!("  line {number}: expected {wanted:?}\n           actual   {got:?}")
                })
                .collect();
            if !diff.is_empty() {
                failures.push(format!("{name} region:\n{}", diff.join("\n")));
            }
        }
    }
    assert!(failures.is_empty(), "{} differences:\n{}", failures.len(), failures[..failures.len().min(40)].join("\n"));
}

/// A list or item whose last text ends with a character of more than one
/// byte: its end is found by line, not by stepping back a byte.
#[test]
fn ends_after_text_of_several_bytes() {
    let text = "* a\n- one 🤦\n- two 🤦\n\n| x | 🤦 |\n\n* b\n";
    for adapt in [Adapt::Off, Adapt::All] {
        let settings = Settings { adapt, ..Settings::default() };
        let edits = indent::indent_region(text, 0, text.len(), &settings);
        indent::apply(text, &edits);
        indent::fixup_entry(text, 1, &settings);
    }
}
