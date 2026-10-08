//! Conformance against Emacs: `tests/refile/cases.json` holds Org files,
//! the entry to refile, the target picked and what `org-refile` offered
//! and made of the files (see `scripts/refile_reference.py`).

use std::path::PathBuf;

use org_core::refile::{self, all_targets, file_title, find, names, new_child, new_node, refile, File, NewNodes};
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
        // `«`: the mark of an active region.
        let mut mark = None;
        let texts: Vec<String> = named
            .iter()
            .enumerate()
            .map(|(index, (_, text))| {
                let text = text.as_str().unwrap();
                if let Some(at) = text.replacen('«', "", 1).find('¦') {
                    cursor = Some((index, at));
                }
                if let Some(at) = text.replacen('¦', "", 1).find('«') {
                    mark = Some(at);
                }
                text.replacen('¦', "", 1).replacen('«', "", 1)
            })
            .collect();
        let (current, offset) = cursor.expect("a cursor");
        let region = mark.map(|mark| refile::region(&texts[current], mark.min(offset), mark.max(offset)));
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
        // `org-refile-allow-creating-parent-nodes`: the files once a new
        // target is made, and where the entry then is. An answer that
        // names a parent and a child refiles nothing when no child is made.
        let mut work = texts.clone();
        let mut offset = offset;
        let new_nodes = options.refile_new_nodes();
        let confirmed = case["confirm"].as_bool() != Some(false);
        let picked: Result<Option<(usize, Option<usize>)>, org_core::Refusal> = match find(&targets, answer) {
            Some((file, target)) => Ok(Some((*file, target.offset))),
            None => match new_node(&targets, answer) {
                Some(((file, parent), child))
                    if new_nodes == NewNodes::Yes || (new_nodes == NewNodes::Confirm && confirmed) =>
                {
                    let (made, heading) = new_child(&work[*file], parent.offset, &child);
                    if *file == current && heading - 1 <= offset {
                        offset += made.len() - work[*file].len();
                    }
                    work[*file] = made;
                    Ok(Some((*file, Some(heading))))
                }
                _ if answer.rsplit_once('/').is_some_and(|(_, child)| !child.is_empty()) => Ok(None),
                _ => Err("Invalid target location".into()),
            },
        };
        let result = picked.and_then(|picked| {
            let Some((file, at)) = picked else { return Ok(None) };
            let region = region.clone().transpose()?;
            let other = (file != current).then(|| work[file].as_str());
            let keep = case["copy"].as_bool() == Some(true);
            refile(&work[current], offset, region, other, at, &options.refile_settings(), keep)
                .map(|done| Some((file, done)))
        });
        // `org-log-refile`, at the time Emacs ran at.
        let result = result.map(|refiled| {
            let (file, mut done) = refiled?;
            let todo = options.todo_settings();
            if let Some(at) = done.pasted {
                let text = done.target.as_mut().unwrap_or(&mut done.source);
                // A region: the time, in the last headline set up.
                let at = match region.clone().and_then(Result::ok) {
                    Some((start, end)) => match refile::region_log_heading(text, at, end - start) {
                        Some(at) => at,
                        None => return Some((file, done)),
                    },
                    None => at,
                };
                if let Some(logged) = org_core::todo::refile_logging(&todo, text) {
                    let note = if logged == org_syntax::Logging::Note && region.is_none() {
                        case["note"].as_str().unwrap_or("")
                    } else {
                        ""
                    };
                    let keywords = org_syntax::parse(text, &Default::default()).todo_keywords().clone();
                    let now = org_core::date::DateTime::new(org_core::date::Date::new(2026, 5, 18), 10, 30);
                    *text = org_core::todo::log_refile(text, &keywords, at, &todo, now, note).unwrap();
                }
            }
            Some((file, done))
        });
        let expected = &case["expected"];
        let same = match (expected, &result) {
            (Value::String(_), Err(_)) => true,
            (Value::Object(expected), Ok(refiled)) => {
                let mut after: Vec<(String, String)> =
                    named.iter().map(|(name, _)| name.to_string()).zip(work.iter().cloned()).collect();
                if let Some((file, done)) = refiled {
                    after[current].1 = done.source.clone();
                    if let Some(target) = &done.target {
                        after[*file].1 = target.clone();
                    }
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
