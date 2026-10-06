//! Conformance against Emacs: `tests/archive/cases.json` holds Org files,
//! an archiving command and what Emacs made of the files (see
//! `scripts/archive_reference.py`). The files are in `/home/notes`.

use std::path::{Path, PathBuf};

use org_core::archive::{self, AllOutcome, Archived, Context};
use org_core::date::{Date, DateTime};
use org_lsp::config::Options;
use org_syntax::TodoKeywords;
use serde_json::{json, Value};

const FOLDER: &str = "/home/notes";

fn clock(case: &Value) -> DateTime {
    let text = case["clock"].as_str().unwrap_or("2026-05-18 10:30");
    let number = |range: std::ops::Range<usize>| text[range].parse::<i64>().unwrap();
    DateTime::new(Date::new(number(0..4), number(5..7), number(8..10)), number(11..13), number(14..16))
}

#[test]
fn archiving_matches_emacs() {
    let path = match std::env::var_os("ORG_ARCHIVE_CASES") {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/archive/cases.json"),
    };
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = String::new();
    for (index, case) in cases.iter().enumerate() {
        let options: Options =
            serde_json::from_value(case.get("options").cloned().unwrap_or_else(|| json!({}))).unwrap();
        let settings = options.archive_settings();
        let mut files: Vec<(String, String)> = Vec::new();
        let mut cursor = None;
        for (name, text) in case["files"].as_object().unwrap() {
            let text = text.as_str().unwrap();
            if let Some(at) = text.find('¦') {
                cursor = Some((name.clone(), at));
            }
            files.push((name.clone(), text.replacen('¦', "", 1)));
        }
        let (name, offset) = cursor.expect("a cursor");
        let source = files.iter().find(|(found, _)| *found == name).unwrap().1.clone();
        let keywords = TodoKeywords::from_document(&source).unwrap_or_else(|| options.todo_keywords());
        let path = Path::new(FOLDER).join(&name);
        let context = Context { path: &path, home: Some(Path::new("/home")), now: clock(case) };
        let on_disk = files.clone();
        let mut read = |wanted: &Path| {
            let name = wanted.strip_prefix(FOLDER).ok()?.to_str()?.to_string();
            on_disk.iter().find(|(found, _)| *found == name).map(|(_, text)| text.clone())
        };
        let command = case["command"].as_str().unwrap();
        let mut questions = Vec::new();
        let result: Result<Archived, String> = match command {
            "subtree" => archive::archive_subtree(&source, offset, &keywords, &context, &settings, &mut read)
                .map_err(|refusal| refusal.message().to_string()),
            "sibling" => archive::to_sibling(&source, offset, &keywords, context.now, &settings)
                .map(|(text, cursor)| Archived { source: text, files: Vec::new(), message: String::new(), cursor })
                .map_err(|refusal| refusal.message().to_string()),
            "tag" => archive::toggle_tag(&source, offset, &settings)
                .map(|(text, message)| Archived { source: text, files: Vec::new(), message, cursor: offset })
                .map_err(|refusal| refusal.message().to_string()),
            "done" | "tagDone" => {
                // Asked one question at a time, as an editor would.
                let given: Vec<bool> = case["answers"]
                    .as_array()
                    .map(|all| all.iter().map(|a| a.as_bool().unwrap()).collect())
                    .unwrap_or_default();
                let mut answers = Vec::new();
                loop {
                    let outcome = archive::archive_all(
                        &source,
                        offset,
                        &keywords,
                        &context,
                        &settings,
                        command == "tagDone",
                        &answers,
                        &mut read,
                    );
                    match outcome {
                        Ok(AllOutcome::Ask(question)) if questions.len() < 50 => {
                            questions.push(question);
                            answers.push(given.get(answers.len()).copied().unwrap_or(true));
                        }
                        Ok(AllOutcome::Ask(_)) => break Err("too many questions".to_string()),
                        Ok(AllOutcome::Done(done)) => break Ok(done),
                        Err(refusal) => break Err(refusal.message().to_string()),
                    }
                }
            }
            _ => panic!("no command {command}"),
        };
        let expected = &case["expected"];
        let same = match (expected, &result) {
            (Value::String(_), Err(_)) => true,
            (Value::Object(expected), Ok(done)) => {
                let mut after = files.clone();
                after.iter_mut().find(|(found, _)| *found == name).unwrap().1 = done.source.clone();
                for file in &done.files {
                    let file_name = file.path.strip_prefix(FOLDER).unwrap().to_str().unwrap().to_string();
                    match after.iter_mut().find(|(found, _)| *found == file_name) {
                        Some(found) => found.1 = file.text.clone(),
                        None => after.push((file_name, file.text.clone())),
                    }
                }
                let files_match = after.len() == expected["files"].as_object().unwrap().len()
                    && after.iter().all(|(name, text)| expected["files"][name].as_str() == Some(text.as_str()));
                let message_match = command == "sibling" || expected["message"].as_str() == Some(done.message.as_str());
                let asked: Vec<&str> =
                    expected["questions"].as_array().unwrap().iter().map(|q| q.as_str().unwrap()).collect();
                files_match && message_match && asked == questions
            }
            _ => false,
        };
        if !same {
            failures.push_str(&format!(
                "\ncase {index}: {command} {}\n--- emacs\n{expected:#}\n--- org-lsp asked {questions:?}\n{result:#?}\n",
                case.get("options").unwrap_or(&Value::Null),
            ));
        }
    }
    assert!(failures.is_empty(), "archiving differs from Emacs:{failures}");
}
