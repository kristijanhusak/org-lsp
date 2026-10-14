//! Storing and inserting links against Emacs: `tests/link/cases.json`
//! holds files, the link commands run on them and what Emacs made of
//! them (see `scripts/link_reference.py`). `ORG_LINK_CASES` points the
//! test at another file.

use std::collections::BTreeMap;
use std::path::PathBuf;

use org_core::link::{self, File, PathType, Settings, Step, Stored, UseId};
use org_core::{properties, refile};
use serde_json::{json, Value};

const HOME: &str = "/home/u";
const DIRECTORY: &str = "/home/u/org";

fn settings(case: &Value) -> Settings {
    let mut settings = Settings::default();
    let Some(options) = case.get("options").and_then(Value::as_object) else { return settings };
    for (key, value) in options {
        let flag = value.as_bool().unwrap_or(false);
        match key.as_str() {
            "org_id_link_to_org_use_id" => {
                settings.use_id = match value.as_str().unwrap_or("nil") {
                    "t" => UseId::Always,
                    "create-if-interactive" => UseId::CreateIfInteractive,
                    "create-if-interactive-and-no-custom-id" => UseId::CreateIfInteractiveAndNoCustomId,
                    "use-existing" => UseId::UseExisting,
                    _ => UseId::Never,
                }
            }
            "org_link_context_for_files" => settings.context_for_files = flag,
            "org_id_link_use_context" => settings.id_use_context = flag,
            "org_id_link_consider_parent_id" => settings.consider_parent_id = flag,
            "org_link_keep_stored_after_insertion" => settings.keep_stored = flag,
            "org_link_file_path_type" => {
                settings.path_type = match value.as_str().unwrap_or("adaptive") {
                    "absolute" => PathType::Absolute,
                    "noabbrev" => PathType::NoAbbrev,
                    "relative" => PathType::Relative,
                    _ => PathType::Adaptive,
                }
            }
            other => panic!("unknown option {other}"),
        }
    }
    settings
}

fn context(name: &str) -> properties::Context {
    let category = name.rsplit('/').next().unwrap_or(name).trim_end_matches(".org").to_string();
    properties::Context { category, ..properties::Context::default() }
}

/// The `C-u`s of a step.
fn prefix(step: &[Value]) -> usize {
    step.get(1).and_then(Value::as_str).map_or(0, |prefix| prefix.split_whitespace().count())
}

/// Runs the steps of a case: what Emacs records, or the error.
fn run(case: &Value) -> Result<Value, String> {
    let marked = case["text"].as_str().unwrap();
    let mut files: BTreeMap<String, String> = case
        .get("files")
        .and_then(Value::as_array)
        .map(|files| {
            files
                .iter()
                .map(|pair| (pair[0].as_str().unwrap().to_string(), pair[1].as_str().unwrap().to_string()))
                .collect()
        })
        .unwrap_or_default();
    files.insert("main.org".to_string(), marked.replacen('¦', "", 1));
    let mut visited = vec!["main.org".to_string()];
    let mut current = "main.org".to_string();
    let mut cursor = marked.find('¦').unwrap_or(0);
    let settings = settings(case);
    let mut stored: Stored = Vec::new();
    let mut returned: Vec<String> = Vec::new();
    let ids = std::cell::Cell::new(0);
    let new_id = || {
        ids.set(ids.get() + 1);
        format!("id-{}", ids.get())
    };
    let mut new_id = new_id;
    // `org-id-track-globally`, with the files scanned for IDs.
    let id_files: Option<Vec<String>> = case
        .get("id_files")
        .and_then(Value::as_array)
        .map(|names| names.iter().map(|name| name.as_str().unwrap().to_string()).collect());
    let mut region: Option<(usize, usize)> = None;
    for step in case["steps"].as_array().unwrap() {
        let step = step.as_array().unwrap();
        // The region of a `select` lasts one step.
        let selected = region.take();
        let path = format!("{DIRECTORY}/{current}");
        let file = File { path: &path, home: Some(HOME) };
        let context = context(&current);
        let text = files[&current].clone();
        match step[0].as_str().unwrap() {
            "visit" => {
                current = step[1].as_str().unwrap().to_string();
                files.entry(current.clone()).or_default();
                if !visited.contains(&current) {
                    visited.push(current.clone());
                }
                cursor = 0;
            }
            "goto" => cursor = text.find(step[1].as_str().unwrap()).unwrap(),
            // The region, for the next step; the cursor at its end.
            "select" => {
                let start = text.find(step[1].as_str().unwrap()).unwrap();
                cursor = start + step[1].as_str().unwrap().len();
                region = Some((start, cursor));
                continue;
            }
            "store" => {
                let made = link::store(
                    &text,
                    cursor,
                    selected,
                    &file,
                    &settings,
                    true,
                    prefix(step) > 0,
                    &context,
                    &mut new_id,
                )?;
                if let Some((edited, at)) = &made.edited {
                    files.insert(current.clone(), edited.clone());
                    cursor = *at;
                }
                link::remember(&mut stored, &made);
            }
            "annotation" => {
                match link::store(&text, cursor, selected, &file, &settings, false, false, &context, &mut new_id) {
                    Ok(made) => {
                        if let Some((edited, at)) = &made.edited {
                            files.insert(current.clone(), edited.clone());
                            cursor = *at;
                        }
                        returned.push(link::annotation(&made));
                    }
                    Err(_) => returned.push(String::new()),
                }
            }
            "insert" => {
                let given: Vec<Option<String>> =
                    step[2..].iter().map(|answer| answer.as_str().map(str::to_string)).collect();
                let mut answers: Vec<String> = Vec::new();
                let keywords = org_syntax::TodoKeywords::default();
                // `org-id-files`: the files scanned that have IDs.
                let others: Vec<String> = id_files
                    .iter()
                    .flatten()
                    .filter(|name| **name != current && link::has_ids(&files[*name]))
                    .cloned()
                    .collect();
                let names: Vec<String> = std::iter::once(current.clone()).chain(others.iter().cloned()).collect();
                let paths: Vec<String> = names.iter().map(|name| format!("{DIRECTORY}/{name}")).collect();
                let refile_files: Vec<refile::File> = names
                    .iter()
                    .zip(&paths)
                    .map(|(name, path)| refile::File {
                        text: if *name == current { &text } else { &files[name] },
                        keywords: &keywords,
                        name: name.rsplit('/').next().unwrap(),
                        path,
                        title: None,
                    })
                    .collect();
                // The default `org-id-completion-targets`.
                let selections =
                    vec![(refile::Select::All, vec![0]), (refile::Select::All, (1..refile_files.len()).collect())];
                let targets = link::id_targets(&selections, &refile_files, refile::OutlinePath::Yes);
                let entries = refile::names(&targets);
                let abbreviations = org_core::open::abbreviations(&text, &[]);
                let before = ids.get();
                let mut made_there: Option<(String, String)>;
                let tracked = id_files.is_some();
                let mut offered = false;
                loop {
                    ids.set(before);
                    made_there = None;
                    let mut id_entry = |answer: &str| {
                        let (index, target) = refile::find(&targets, answer)?;
                        let offset = target.offset?;
                        if *index == 0 {
                            return Some(link::IdEntry::Here(offset));
                        }
                        let name = &names[*index];
                        let (edited, id) =
                            link::id_get_create(&files[name], offset, &self::context(name), &mut || new_id()).ok()?;
                        if let Some((edited, _)) = edited {
                            made_there = Some((name.clone(), edited));
                        }
                        let heading = link::entry_heading(&files[name], offset, &keywords);
                        Some(link::IdEntry::There { id, heading })
                    };
                    // `org-id-find`: the current file, then the files
                    // tracked.
                    let mut id_heading = |id: &str| {
                        let searched = std::iter::once(&current).chain(names[1..].iter().filter(|_| tracked));
                        searched.into_iter().find_map(|name| {
                            let text = if *name == current { &text } else { &files[name] };
                            let offset = org_core::open::find_id(&org_syntax::parse(text, &keywords), id)?;
                            Some(link::entry_heading(text, offset, &keywords))
                        })
                    };
                    let mut fresh_id = || new_id();
                    let mut outside = link::Outside {
                        abbreviations: &abbreviations,
                        id_entries: &entries,
                        id_entry: &mut id_entry,
                        id_heading: &mut id_heading,
                        context: &context,
                        new_id: &mut fresh_id,
                    };
                    match link::insert(
                        &text,
                        cursor,
                        selected,
                        &file,
                        &stored,
                        prefix(step),
                        &answers,
                        &settings,
                        &mut outside,
                    )? {
                        Step::Ask(question) => {
                            offered |= question.prompt == "Entry";
                            let Some(answer) = given.get(answers.len()) else {
                                panic!("no answer to {:?}", question.prompt);
                            };
                            answers.push(answer.clone().unwrap_or(question.initial));
                        }
                        Step::Done(inserted) => {
                            files.insert(current.clone(), inserted.text);
                            cursor = inserted.cursor;
                            stored = inserted.stored;
                            break;
                        }
                    }
                }
                if let Some((name, edited)) = made_there.take() {
                    files.insert(name, edited);
                }
                // Org visited the files it offered entries of.
                for name in others.iter().filter(|_| offered) {
                    if !visited.contains(name) {
                        visited.push(name.clone());
                    }
                }
            }
            "insert-last" | "insert-all" => {
                let last = step[0] == "insert-last";
                let (count, keep, pre) = if last { (Some(1), false, "") } else { (None, prefix(step) > 0, "- ") };
                if let Some(inserted) =
                    link::insert_all(&text, cursor, &file, &stored, count, keep, pre, "\n", &settings)?
                {
                    files.insert(current.clone(), inserted.text);
                    cursor = inserted.cursor;
                    stored = inserted.stored;
                }
            }
            "id" => {
                let (edited, _) = link::id_get_create(&text, cursor, &context, &mut new_id)?;
                if let Some((edited, at)) = edited {
                    files.insert(current.clone(), edited);
                    cursor = at;
                }
            }
            other => panic!("unknown step {other}"),
        }
    }
    let mut shown = serde_json::Map::new();
    for name in &visited {
        let mut text = files[name].clone();
        if *name == current {
            text.insert(cursor, '¦');
        }
        shown.insert(name.clone(), Value::String(text));
    }
    let stored: Vec<Value> = stored.iter().map(|(link, description)| json!([link, description])).collect();
    Ok(json!({ "files": shown, "stored": stored, "returned": returned }))
}

#[test]
fn links_match_emacs() {
    let path = match std::env::var_os("ORG_LINK_CASES") {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/link/cases.json"),
    };
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let shown = std::env::var("ORG_SHOW_FAILURES").ok().and_then(|count| count.parse().ok()).unwrap_or(8);
    let mut failures = String::new();
    let mut failed = 0;
    for (index, case) in cases.iter().enumerate() {
        let expected = case["expected"].as_str().expect("run scripts/link_reference.py first");
        let actual = run(case);
        let same = match &actual {
            Ok(actual) => serde_json::from_str::<Value>(expected).is_ok_and(|expected| expected == *actual),
            Err(_) => expected.starts_with("ERROR:"),
        };
        if !same {
            failed += 1;
            if failed <= shown {
                failures.push_str(&format!(
                    "\ncase {index}: {} {}\n--- emacs\n{expected}\n--- org-lsp\n{}\n",
                    case["steps"],
                    case["text"],
                    actual.map_or_else(|error| format!("ERROR: {error}"), |value| value.to_string()),
                ));
            }
        }
    }
    assert!(failed == 0, "{failed} of {} cases differ from Emacs:{failures}", cases.len());
}
