//! Sparse trees against Emacs.
//!
//! `tests/sparse/cases.json` is written by
//! `scripts/update-sparse-fixtures.py`: a document, the kind of sparse
//! tree and what it asks for, and the matches Emacs highlights with its
//! message, on Monday 18 May 2026 at 10:30.

use std::path::Path;

use org_core::agenda::matcher::Clock;
use org_core::agenda::Matcher;
use org_core::date::{Date, DateTime};
use org_core::outline::Outline;
use org_core::sparse::{self, CaseFold, DateType, Sparse};
use org_syntax::TodoKeywords;
use serde_json::Value;

fn chars(text: &str, byte: usize) -> usize {
    text[..byte].chars().count()
}

fn date(text: &str) -> DateTime {
    let number = |range: std::ops::Range<usize>| text.get(range).and_then(|part| part.parse().ok()).unwrap_or(0);
    let time = text.get(11..).map(|time| time.split_once(':').unwrap_or(("0", "0")));
    let (hour, minute) = time.map_or((0, 0), |(hour, minute)| (hour.parse().unwrap_or(0), minute.parse().unwrap_or(0)));
    DateTime::new(Date::new(number(0..4), number(5..7), number(8..10)), hour, minute)
}

#[test]
fn finds_what_emacs_finds() {
    let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/sparse/cases.json"));
    let path = std::env::var("ORG_SPARSE_CASES").map_or(path.to_path_buf(), Into::into);
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let today = Date::new(2026, 5, 18).to_day();
    let mut failures = Vec::new();
    for case in &cases {
        let text = case["text"].as_str().unwrap();
        let kind = case["type"].as_str().unwrap();
        let args: Vec<&str> = case["args"].as_array().unwrap().iter().map(|arg| arg.as_str().unwrap()).collect();
        let parse = org_syntax::parse(text, &TodoKeywords::default());
        let keywords = parse.todo_keywords().clone();
        let case_fold = match case["case"].as_str() {
            Some("nil") => CaseFold::Never,
            Some("smart") => CaseFold::Smart,
            _ => CaseFold::Always,
        };
        let date_type = case["date_type"].as_str().and_then(DateType::from_name).unwrap_or_default();
        let (found, message): (Sparse, Option<String>) = match kind {
            "r" => (sparse::occur(text, &sparse::occur_regex(args[0], case_fold).unwrap(), |_| true), None),
            "t" | "T" => {
                let wanted: Option<Vec<String>> =
                    (kind == "T").then(|| args[0].split('|').map(str::to_string).collect());
                let found = sparse::todo_tree(text, &keywords, wanted.as_deref(), case_fold);
                let message = format!("{} TODO entries found", found.count);
                (found, Some(message))
            }
            "m" | "M" => {
                let outline = Outline::new(&parse, text, "case.org");
                let clock = Clock { today, minute: 630 };
                (sparse::tags_match(text, &outline, &Matcher::new(args[0]), clock, kind == "m"), None)
            }
            "d" => {
                let found = sparse::deadlines(text, &keywords, today, 14, case_fold);
                let message = format!("{} deadlines past-due or due within 14 days", found.count);
                (found, Some(message))
            }
            "b" => {
                let found = sparse::dates(text, &parse, None, Some(date(args[0])), date_type, case_fold);
                let message = format!("{} entries before {}", found.count, args[0]);
                (found, Some(message))
            }
            "a" => {
                let found = sparse::dates(text, &parse, Some(date(args[0])), None, date_type, case_fold);
                let message = format!("{} entries after {}", found.count, args[0]);
                (found, Some(message))
            }
            _ => {
                let found = sparse::dates(text, &parse, Some(date(args[0])), Some(date(args[1])), date_type, case_fold);
                let message = format!("{} entries between {} and {}", found.count, args[0], args[1]);
                (found, Some(message))
            }
        };
        let highlights: Vec<[usize; 2]> =
            found.highlights.iter().map(|(start, end)| [chars(text, *start), chars(text, *end)]).collect();
        let want: Vec<[usize; 2]> = serde_json::from_value(case["expected"]["highlights"].clone()).unwrap();
        let want_message = case["expected"]["message"].as_str().filter(|message| *message != "json-null");
        if highlights != want || message.as_deref() != want_message {
            failures.push(format!(
                "{kind} {args:?} {:?} {:?} {text:?}\n  expected {want:?} {want_message:?}\n  got      {highlights:?} {message:?}",
                case["case"], case["date_type"]
            ));
        }
    }
    assert!(failures.is_empty(), "{} of {} cases differ:\n{}", failures.len(), cases.len(), failures.join("\n"));
}
