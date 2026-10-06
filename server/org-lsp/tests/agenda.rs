//! The agenda against Emacs.
//!
//! Every directory under `tests/agenda` holds Org files, a `case.json`
//! naming a pretended clock and the agenda commands to run, and the text
//! Emacs printed for each (`expected-N.txt`, written by
//! `scripts/update-agenda-fixtures.py`). The server must print the same.

use std::path::{Path, PathBuf};

use serde_json::Value;

fn cases_directory() -> PathBuf {
    // `ORG_AGENDA_CASES` points the test at generated cases; see
    // `scripts/fuzz-agenda.py`.
    match std::env::var_os("ORG_AGENDA_CASES") {
        Some(directory) => PathBuf::from(directory),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/agenda"),
    }
}

fn org_files(directory: &Path) -> Vec<String> {
    let mut files: Vec<String> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "org"))
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    files.sort();
    files
}

/// The `org-lsp agenda` arguments for one run of a case.
fn arguments(directory: &Path, case: &Value, run: &Value) -> Vec<String> {
    let text = |value: &Value| value.as_str().map_or_else(|| value.to_string(), str::to_string);
    let view = text(&run["view"]);
    let mut arguments = vec![
        "--today".to_string(),
        text(&case["today"]),
        "--now".to_string(),
        text(&case["now"]),
        "--view".to_string(),
        view.clone(),
    ];
    if view == "agenda" {
        arguments.extend(["--span".to_string(), run.get("span").map_or("week".to_string(), text)]);
        if let Some(start) = run.get("start") {
            arguments.extend(["--start".to_string(), text(start)]);
        }
    }
    for (name, flag) in [("match", "--match"), ("key", "--key")] {
        if let Some(value) = run.get(name) {
            arguments.extend([flag.to_string(), text(value)]);
        }
    }
    for flag in ["log", "clockreport"] {
        if run.get(flag).and_then(Value::as_bool) == Some(true) {
            arguments.push(format!("--{flag}"));
        }
    }
    if let Some(archives) = run.get("archives") {
        arguments.extend(["--archives".to_string(), text(archives)]);
    }
    if let Some(options) = case.get("options") {
        arguments.extend(["--options".to_string(), options.to_string()]);
    }
    arguments.extend(org_files(directory));
    arguments
}

/// The first lines that differ, side by side.
fn difference(expected: &str, actual: &str) -> String {
    let (expected, actual): (Vec<&str>, Vec<&str>) = (expected.lines().collect(), actual.lines().collect());
    let mut out = String::new();
    let mut shown = 0;
    for index in 0..expected.len().max(actual.len()) {
        let (theirs, ours) = (expected.get(index), actual.get(index));
        if theirs != ours && shown < 6 {
            shown += 1;
            out.push_str(&format!("  line {}\n    emacs:   {:?}\n    org-lsp: {:?}\n", index + 1, theirs, ours));
        }
    }
    out
}

#[test]
fn agenda_matches_emacs() {
    let root = cases_directory();
    let mut directories: Vec<PathBuf> = std::fs::read_dir(&root)
        .unwrap_or_else(|error| panic!("{}: {error}", root.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.join("case.json").exists())
        .map(|path| path.canonicalize().unwrap())
        .collect();
    directories.sort();
    assert!(!directories.is_empty(), "no cases in {}", root.display());

    let mut failures = String::new();
    let (mut compared, mut failed) = (0, 0);
    for directory in &directories {
        let case: Value = serde_json::from_str(&std::fs::read_to_string(directory.join("case.json")).unwrap()).unwrap();
        for (index, run) in case["runs"].as_array().unwrap().iter().enumerate() {
            let expected = std::fs::read_to_string(directory.join(format!("expected-{index}.txt")))
                .unwrap_or_else(|error| panic!("{} run {index}: {error}", directory.display()));
            // Emacs itself gave up on this input; there is nothing to match.
            if expected.starts_with("ERROR:") {
                continue;
            }
            let arguments = arguments(directory, &case, run);
            let actual = org_lsp::cli::agenda(&arguments).unwrap_or_else(|error| format!("ERROR: {error}\n"));
            // The clock report links to the files: by where the case is.
            let actual = actual.replace(&format!("{}/", directory.display()), "{dir}/");
            compared += 1;
            if actual != expected {
                failed += 1;
                if failed <= 12 {
                    let name = directory.file_name().unwrap().to_string_lossy();
                    failures.push_str(&format!("\n{name} run {index} {run}\n{}", difference(&expected, &actual)));
                }
            }
        }
    }
    assert!(failed == 0, "{failed} of {compared} agenda runs differ from Emacs:{failures}");
}
