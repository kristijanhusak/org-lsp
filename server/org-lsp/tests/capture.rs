//! Conformance against Emacs: `tests/capture/cases.json` holds Org files, a
//! capture template, the answers to its prompts and the text typed in the
//! capture buffer, with what `org-capture` showed and made of the files
//! (see `scripts/capture_reference.py`). `ORG_CAPTURE_CASES` points the test
//! at another file, which is how `scripts/fuzz-capture.py` feeds it
//! generated cases.

use std::path::PathBuf;

use org_core::capture::{fill, place, Context, Filled, Kind, Properties, Target};
use org_core::date::DateTime;
use org_core::indent::Adapt;
use org_core::read_date::Settings;
use org_syntax::TodoKeywords;
use serde_json::Value;

fn clock(text: &str) -> DateTime {
    let (date, time) = text.split_once(' ').expect("date and time");
    DateTime { day: org_lsp::cli::parse_date(date).unwrap(), minute: org_lsp::cli::parse_time(time).unwrap() }
}

/// What org-lsp makes of a case: the capture buffer with `¦` at the
/// cursor, and the target file after typing.
fn run(case: &Value) -> Result<(String, String, String), String> {
    let template = &case["template"];
    let now = clock(case["clock"].as_str().unwrap());
    let file = case["from"].as_str().map(|name| (name.to_string(), format!("/{name}")));
    let context = Context { now, file, initial: String::new(), annotation: String::new(), user_name: String::new() };
    // Prompts take the answers in order, and nothing once they run out.
    let mut answers: Vec<String> =
        case["answers"].as_array().into_iter().flatten().map(|answer| answer.as_str().unwrap().to_string()).collect();
    let given = answers.len();
    let text = loop {
        match fill(template["template"].as_str().unwrap(), &context, &answers, &Settings::default())
            .map_err(|refusal| refusal.message().to_string())?
        {
            Filled::Text(text) => break text,
            Filled::Ask(_) if answers.len() < given + 20 => answers.push(String::new()),
            Filled::Ask(question) => return Err(format!("still asking {question:?}")),
        }
    };
    let target_spec: Vec<&str> =
        template["target"].as_array().unwrap().iter().map(|part| part.as_str().unwrap()).collect();
    let name = target_spec[1].to_string();
    let target = match target_spec[0] {
        "file" => Target::File,
        "file+headline" => Target::Headline(target_spec[2].to_string()),
        "file+olp" => Target::Path(target_spec[2..].iter().map(|part| part.to_string()).collect()),
        other => return Err(format!("no target {other}")),
    };
    let kind = match template["type"].as_str() {
        None | Some("entry") => Kind::Entry,
        Some("plain") => Kind::Plain,
        Some(other) => return Err(format!("no type {other}")),
    };
    let number = |key: &str| template[key].as_u64().map(|n| n as usize);
    let properties = Properties {
        prepend: template["prepend"].as_bool().unwrap_or(false),
        empty_lines: number("emptyLines"),
        empty_lines_before: number("emptyLinesBefore"),
        empty_lines_after: number("emptyLinesAfter"),
    };
    let original = case["files"][&name].as_str().unwrap();
    let keywords = TodoKeywords::from_document(original).unwrap_or_default();
    let adapt = match case.pointer("/options/adaptIndentation") {
        Some(Value::Bool(true)) => Adapt::All,
        Some(Value::String(name)) if name == "headline-data" => Adapt::HeadlineData,
        _ => Adapt::Off,
    };
    let levels = org_core::refile::Settings { adapt, ..Default::default() };
    let placed = place(original, &keywords, &target, kind, &text, &properties, &levels)
        .map_err(|refusal| refusal.message().to_string())?;
    let at = placed.cursor - placed.start;
    let shown = placed.shown();
    let buffer = format!("{}¦{}", &shown[..at], &shown[at..]);
    let typed = case["typed"].as_str().unwrap_or("");
    let edited = format!("{}{typed}{}", &shown[..at], &shown[at..]);
    Ok((name, buffer, placed.finish(&edited)))
}

#[test]
fn capturing_matches_emacs() {
    let path = match std::env::var_os("ORG_CAPTURE_CASES") {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/capture/cases.json"),
    };
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = String::new();
    let mut failed = 0;
    for (index, case) in cases.iter().enumerate() {
        let expected = &case["expected"];
        let result = run(case);
        let same = match (expected, &result) {
            (Value::String(_), Err(_)) => true,
            (Value::Object(expected), Ok((name, buffer, after))) => {
                expected["buffer"].as_str() == Some(buffer.as_str())
                    && expected["files"][name].as_str() == Some(after.as_str())
            }
            _ => false,
        };
        if !same {
            failed += 1;
            if failed <= 10 {
                failures.push_str(&format!(
                    "\ncase {index}: {}\n--- files\n{:#}\n--- emacs\n{expected:#}\n--- org-lsp\n{result:#?}\n",
                    case["template"], case["files"]
                ));
            }
        }
    }
    assert!(failed == 0, "{failed} of {} captures differ from Emacs:{failures}", cases.len());
}
