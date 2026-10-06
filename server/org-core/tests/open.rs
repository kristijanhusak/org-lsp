//! Conformance against Emacs: `tests/open/cases.json` holds files with a
//! cursor and what `org-open-at-point` did there (see
//! `scripts/open_reference.py`).

use std::path::{Path, PathBuf};

use org_core::open::{at_point, find_id, search_in_file, Open};
use org_syntax::{Parse, TodoKeywords};
use serde_json::{json, Value};

fn parse(text: &str) -> Parse {
    org_syntax::parse(text, &TodoKeywords::from_document(text).unwrap_or_default())
}

/// Line (from 1) and column (in characters) of `offset`, as Emacs counts.
fn place(text: &str, offset: usize) -> (usize, usize) {
    let before = &text[..offset];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().unwrap_or("").chars().count();
    (line, column)
}

fn goto(name: &str, text: &str, offset: usize) -> Value {
    let (line, column) = place(text, offset);
    let file = Path::new(name).file_name().unwrap().to_string_lossy().to_string();
    json!({ "goto": { "file": file, "line": line, "column": column } })
}

#[test]
fn opening_matches_emacs() {
    let path = match std::env::var_os("ORG_OPEN_CASES") {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/open/cases.json"),
    };
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = String::new();
    for (index, case) in cases.iter().enumerate() {
        let files: Vec<(String, String)> = case["files"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(name, text)| (name.clone(), text.as_str().unwrap().to_string()))
            .collect();
        let (name, marked) = files.iter().find(|(_, text)| text.contains('¦')).unwrap();
        let offset = marked.find('¦').unwrap();
        let text = marked.replacen('¦', "", 1);
        let parsed = parse(&text);
        let answer = case["answer"].as_str().and_then(|answer| answer.chars().next());
        let text_of = |wanted: &str| {
            if wanted == name {
                return Some(text.clone());
            }
            files.iter().find(|(other, _)| other == wanted).map(|(_, text)| text.clone())
        };
        let mut offered = None;
        let mut result = at_point(&text, &parsed, offset, None);
        if let Ok(Open::Choose(choices)) = &result {
            offered =
                Some(choices.iter().map(|choice| format!("[{}]  {}\n", choice.key, choice.label)).collect::<String>());
            result = at_point(&text, &parsed, offset, answer);
        }
        let mut actual = match result {
            Ok(Open::Here(at)) => goto(name, &text, at),
            Ok(Open::File { path, search, system }) => {
                let base = Path::new(name).parent().unwrap_or(Path::new(""));
                let wanted = base.join(path.trim_start_matches("./")).to_string_lossy().to_string();
                if system {
                    json!({ "system": Path::new(&wanted).file_name().unwrap().to_string_lossy() })
                } else {
                    let target = text_of(&wanted).unwrap_or_default();
                    let org = wanted.ends_with(".org");
                    let target_parse = parse(&target);
                    match search {
                        None => goto(&wanted, &target, 0),
                        Some(option) => match search_in_file(&target, org.then_some(&target_parse), &option) {
                            Ok(at) => goto(&wanted, &target, at),
                            Err(error) => json!({ "error": error }),
                        },
                    }
                }
            }
            Ok(Open::Id(id)) => files
                .iter()
                .filter(|(other, _)| other.ends_with(".org"))
                .find_map(|(other, _)| {
                    let target = text_of(other)?;
                    find_id(&parse(&target), &id).map(|at| goto(other, &target, at))
                })
                .unwrap_or_else(|| json!({ "error": format!("Cannot find entry with ID \"{id}\"") })),
            Ok(Open::External(url)) => json!({ "external": url }),
            Ok(Open::Agenda { date, span }) => {
                json!({ "agenda": format!("{:04}-{:02}-{:02}", date.year, date.month, date.day), "span": span })
            }
            Ok(Open::Tags(tag)) => json!({ "tags": tag }),
            Ok(Open::Nothing(_)) => goto(name, &text, offset),
            Ok(Open::Choose(_)) => json!({ "error": "asked twice" }),
            Err(error) => json!({ "error": error }),
        };
        if let Some(offered) = offered {
            actual["offered"] = json!(offered);
        }
        if actual != case["expected"] {
            failures.push_str(&format!(
                "case {index} ({}): expected {}, got {}\n",
                case["note"].as_str().unwrap_or(""),
                case["expected"],
                actual
            ));
        }
    }
    assert!(failures.is_empty(), "{failures}");
}
