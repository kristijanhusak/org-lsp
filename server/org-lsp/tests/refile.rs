//! Conformance against Emacs: `tests/refile/cases.json` holds Org files,
//! the entry to refile, the target picked and what `org-refile` offered
//! and made of the files (see `scripts/refile_reference.py`).

use std::path::PathBuf;

use org_core::refile::{all_targets, file_title, find, names, refile, File};
use org_lsp::config::{Options, RefileFiles};
use org_syntax::TodoKeywords;
use serde_json::Value;

#[test]
fn refiling_matches_emacs() {
    let path = match std::env::var_os("ORG_REFILE_CASES") {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/refile/cases.json"),
    };
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = String::new();
    for (index, case) in cases.iter().enumerate() {
        let options: Options =
            serde_json::from_value(case.get("options").cloned().unwrap_or_else(|| serde_json::json!({}))).unwrap();
        // The files in name order, the agenda files of the case.
        let mut named: Vec<(&String, &Value)> = case["files"].as_object().unwrap().iter().collect();
        named.sort_by_key(|(name, _)| *name);
        let mut cursor = None;
        let texts: Vec<String> = named
            .iter()
            .enumerate()
            .map(|(index, (_, text))| {
                let text = text.as_str().unwrap();
                if let Some(at) = text.find('¦') {
                    cursor = Some((index, at));
                }
                text.replacen('¦', "", 1)
            })
            .collect();
        let (current, offset) = cursor.expect("a cursor");
        let parses: Vec<_> = texts
            .iter()
            .map(|text| org_syntax::parse(text, &TodoKeywords::from_document(text).unwrap_or_default()))
            .collect();
        let titles: Vec<Option<String>> = parses.iter().map(file_title).collect();
        let files: Vec<File> = named
            .iter()
            .enumerate()
            .map(|(index, (name, _))| File {
                text: &texts[index],
                keywords: parses[index].todo_keywords(),
                name: name.as_str(),
                path: name.as_str(),
                title: titles[index].as_deref(),
            })
            .collect();
        let selections: Vec<_> = options
            .refile_targets()
            .into_iter()
            .map(|target| {
                let indexes = match target.files {
                    RefileFiles::Current => vec![current],
                    RefileFiles::Agenda => (0..files.len()).collect(),
                    RefileFiles::Paths(paths) => {
                        paths.iter().filter_map(|path| named.iter().position(|(name, _)| *name == path)).collect()
                    }
                };
                (target.select, indexes)
            })
            .collect();
        let targets = all_targets(&selections, &files, Some(current), options.refile_outline_path());
        // The name picked, or the one at `answerIndex` of those offered.
        let offered = names(&targets);
        let picked = case["answerIndex"].as_u64().map(|at| offered.get(at as usize % offered.len().max(1)).cloned());
        let answer = match picked {
            Some(name) => name.unwrap_or_default(),
            None => case["answer"].as_str().unwrap().to_string(),
        };
        let answer = answer.as_str();
        let result = find(&targets, answer).ok_or("Invalid target location".into()).and_then(|(file, target)| {
            let other = (*file != current).then(|| texts[*file].as_str());
            let keep = case["copy"].as_bool() == Some(true);
            refile(&texts[current], offset, other, target.offset, &options.refile_settings(), keep)
                .map(|done| (*file, done))
        });
        // `org-log-refile`, at the time Emacs ran at.
        let result = result.map(|(file, mut done)| {
            let todo = options.todo_settings();
            if let Some(at) = done.pasted {
                let text = done.target.as_mut().unwrap_or(&mut done.source);
                if let Some(logged) = org_core::todo::refile_logging(&todo, text) {
                    let note =
                        if logged == org_syntax::Logging::Note { case["note"].as_str().unwrap_or("") } else { "" };
                    let keywords = org_syntax::parse(text, &Default::default()).todo_keywords().clone();
                    let now = org_core::date::DateTime::new(org_core::date::Date::new(2026, 5, 18), 10, 30);
                    *text = org_core::todo::log_refile(text, &keywords, at, &todo, now, note).unwrap();
                }
            }
            (file, done)
        });
        let expected = &case["expected"];
        let same = match (expected, &result) {
            (Value::String(_), Err(_)) => true,
            (Value::Object(expected), Ok((file, done))) => {
                let mut after: Vec<(String, String)> =
                    named.iter().map(|(name, _)| name.to_string()).zip(texts.iter().cloned()).collect();
                after[current].1 = done.source.clone();
                if let Some(target) = &done.target {
                    after[*file].1 = target.clone();
                }
                let mut offered: Vec<String> = Vec::new();
                for name in expected["offered"].as_array().unwrap() {
                    let name = name.as_str().unwrap().to_string();
                    if !offered.contains(&name) {
                        offered.push(name);
                    }
                }
                let files_match =
                    after.iter().all(|(name, text)| expected["files"][name].as_str() == Some(text.as_str()));
                files_match && names(&targets) == offered
            }
            (Value::Object(_), Err(_)) if names(&targets).is_empty() => false,
            _ => false,
        };
        if !same {
            failures.push_str(&format!(
                "\ncase {index}: refile to {answer:?} {}\n--- emacs\n{expected:#}\n--- org-lsp offers {:?}\n{:#?}\n",
                case["options"],
                names(&targets),
                result
            ));
        }
    }
    assert!(failures.is_empty(), "refiling differs from Emacs:{failures}");
}
