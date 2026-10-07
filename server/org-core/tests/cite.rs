//! Citations against Emacs: `tests/cite/cases.json` is written by
//! `scripts/update-cite-fixtures.py`: an Org document and its
//! bibliography files, with what Org (`oc-basic`) makes of them: the
//! entries of each file, the keys, each entry as its help echo shows it,
//! its line in the completion table, where following it goes, and the
//! keys close to unknown ones. `ORG_CITE_CASES` points the test at
//! another file.

use std::path::Path;

use org_core::cite::{self, Entry, Value};
use org_syntax::TodoKeywords;
use serde_json::{json, Value as Json};

/// What org-lsp makes of a case, in the form of the reference.
fn run(case: &Json) -> Json {
    let text = case["text"].as_str().unwrap();
    let files = case["files"].as_object().cloned().unwrap_or_default();
    let names = cite::bibliography_files(&cite::bibliography_keywords(text, &TodoKeywords::default()), &[]);
    // Org lists the files it read last first.
    let mut parsed: Vec<(String, Vec<Entry>)> = Vec::new();
    for name in &names {
        let Some(content) = files.get(name).and_then(Json::as_str) else { continue };
        if let Ok(entries) = cite::parse_file(name, content) {
            parsed.insert(0, (name.clone(), entries));
        }
    }
    let entries: Vec<Json> = parsed
        .iter()
        .map(|(name, entries)| {
            let entries: Vec<Json> = entries
                .iter()
                .map(|entry| {
                    let fields: Vec<Json> = entry
                        .fields
                        .iter()
                        .map(|(field, value)| match value {
                            Value::Text(text) => json!([field, text]),
                            Value::Other(other) => {
                                json!([field, serde_json::from_str::<Json>(other).unwrap_or(json!(other))])
                            }
                        })
                        .collect();
                    json!([entry.key, fields])
                })
                .collect();
            json!([name, entries])
        })
        .collect();
    let all: Vec<(&String, &Entry)> =
        parsed.iter().flat_map(|(name, entries)| entries.iter().map(move |entry| (name, entry))).collect();
    let keys: Vec<String> = all.iter().map(|(_, entry)| entry.key.clone()).collect();
    let find = |key: &str| all.iter().find(|(_, entry)| entry.key == key).copied();
    let plain: Vec<Json> = keys
        .iter()
        .map(|key| {
            let (_, entry) = find(key).unwrap();
            json!([key, entry.plain().unwrap_or_else(|error| format!("ERROR: {error}"))])
        })
        .collect();
    // The completion table maps each line to a key: a line two entries
    // share keeps the last.
    let mut completion: Vec<(String, String)> = Vec::new();
    for key in &keys {
        let (_, entry) = find(key).unwrap();
        if let Ok(line) = entry.completion() {
            completion.retain(|(_, known)| *known != line);
            completion.push((key.clone(), line));
        }
    }
    let goto: Vec<Json> = keys
        .iter()
        .map(|key| {
            let (name, _) = find(key).unwrap();
            let content = files[name.as_str()].as_str().unwrap().replace("\r\n", "\n");
            let at = cite::entry_position(name, &content, key).unwrap_or(0);
            json!([key, name, content[..at].chars().count()])
        })
        .collect();
    let close: Vec<Json> = case["unknown"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|key| {
            let key = key.as_str().unwrap();
            json!([key, cite::close_keys(key, &keys)])
        })
        .collect();
    json!({
        "entries": entries,
        "keys": keys,
        "plain": plain,
        "completion": completion.into_iter().map(|(key, line)| json!([key, line])).collect::<Vec<_>>(),
        "goto": goto,
        "close": close,
    })
}

#[test]
fn reads_bibliographies_as_org_does() {
    let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/cite/cases.json"));
    let path = std::env::var("ORG_CITE_CASES").map_or(path.to_path_buf(), Into::into);
    let cases: Vec<Json> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = Vec::new();
    let mut compared = 0;
    for case in &cases {
        let expected = &case["expected"];
        if expected.get("error").is_some() {
            continue;
        }
        compared += 1;
        let got = run(case);
        for part in ["entries", "keys", "plain", "completion", "goto", "close"] {
            let mut theirs = expected[part].clone();
            if part == "completion" {
                // Order of a hash table's lines is of no matter.
                let mut lines: Vec<Json> = theirs.as_array().cloned().unwrap_or_default();
                lines.sort_by_key(|line| line.to_string());
                theirs = Json::Array(lines);
            }
            let mut ours = got[part].clone();
            if part == "completion" {
                let mut lines: Vec<Json> = ours.as_array().cloned().unwrap_or_default();
                lines.sort_by_key(|line| line.to_string());
                ours = Json::Array(lines);
            }
            if theirs != ours {
                failures.push(format!(
                    "{:?} {}\n  {part}\n  expected {theirs}\n  got      {ours}",
                    case["text"].as_str().unwrap(),
                    case["files"],
                ));
                break;
            }
        }
    }
    assert!(compared > 0);
    assert!(
        failures.is_empty(),
        "{} of {compared} cases differ:\n{}",
        failures.len(),
        failures.iter().take(8).cloned().collect::<Vec<_>>().join("\n")
    );
}
