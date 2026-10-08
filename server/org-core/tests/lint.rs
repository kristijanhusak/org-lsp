//! `org-lint` against Emacs: `tests/lint/cases.json` holds Org texts and
//! the reports Emacs made of them (see `scripts/lint_reference.py`).
//! `ORG_LINT_CASES` points the test at another file.

use std::path::PathBuf;

use org_core::lint::{self, Context};
use serde_json::{json, Value};

fn run(case: &Value) -> Vec<Value> {
    let text = case["text"].as_str().unwrap();
    let files: Vec<String> = case
        .get("files")
        .and_then(Value::as_array)
        .map(|files| files.iter().map(|pair| format!("/org/{}", pair[0].as_str().unwrap())).collect())
        .unwrap_or_default();
    let exists = |name: &str| {
        let path = if name.starts_with('/') { name.to_string() } else { format!("/org/{name}") };
        let path = path.replace("/./", "/");
        path == "/org/main.org" || path == "/org" || path == "/org/" || files.contains(&path)
    };
    let file_texts: Vec<(String, String)> = case
        .get("files")
        .and_then(Value::as_array)
        .map(|files| {
            files
                .iter()
                .map(|pair| (format!("/org/{}", pair[0].as_str().unwrap()), pair[1].as_str().unwrap().to_string()))
                .collect()
        })
        .unwrap_or_default();
    let read = |name: &str| {
        let path = if name.starts_with('/') { name.to_string() } else { format!("/org/{name}") };
        if path == "/org/main.org" {
            return Some(text.to_string());
        }
        file_texts.iter().find(|(file, _)| *file == path).map(|(_, text)| text.clone())
    };
    let name = || Some("main.org".to_string());
    let context = Context {
        file_exists: &exists,
        priorities: org_core::priority::Priorities::default(),
        read_file: &read,
        file_name: &name,
        id_known: &|_| None,
    };
    let parse = org_syntax::parse(text, &Default::default());
    lint::lint(text, &parse, &context)
        .into_iter()
        .map(|report| json!([report.offset, report.trust, report.message, report.checker]))
        .collect()
}

/// `misplaced-heading` reports `match-beginning` after
/// `org-element-at-point` has run, which leaves the match data of its own
/// searches, unless the element was cached: a place org-lsp does not
/// imitate. It reports where the heading starts; only its message counts.
fn comparable(report: &Value) -> Value {
    let mut report = report.clone();
    if report[3] == "misplaced-heading" {
        report[0] = json!(null);
    }
    report
}

#[test]
fn lint_matches_emacs() {
    let path = match std::env::var_os("ORG_LINT_CASES") {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/lint/cases.json"),
    };
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let names = lint::checker_names();
    let shown = std::env::var("ORG_SHOW_FAILURES").ok().and_then(|count| count.parse().ok()).unwrap_or(8);
    let mut failures = String::new();
    let mut failed = 0;
    for (index, case) in cases.iter().enumerate() {
        // Where Emacs's linter fails (a report with no place), nothing to
        // compare.
        if case["expected"].as_str().is_some_and(|expected| expected.starts_with("ERROR:")) {
            continue;
        }
        let expected: Vec<Value> = case["expected"]
            .as_array()
            .expect("run scripts/lint_reference.py first")
            .iter()
            .filter(|report| names.contains(&report[3].as_str().unwrap()))
            .map(comparable)
            .collect();
        let actual: Vec<Value> = run(case).iter().map(comparable).collect();
        // The masked reports go apart: their places sort them.
        let split = |reports: &[Value]| -> (Vec<Value>, usize) {
            let masked = reports.iter().filter(|report| report[0].is_null()).count();
            (reports.iter().filter(|report| !report[0].is_null()).cloned().collect(), masked)
        };
        if split(&actual) != split(&expected) {
            failed += 1;
            if failed <= shown {
                failures.push_str(&format!(
                    "\ncase {index}: {:?}\n--- emacs\n{}\n--- org-lsp\n{}\n",
                    case["text"].as_str().unwrap(),
                    expected.iter().map(Value::to_string).collect::<Vec<_>>().join("\n"),
                    actual.iter().map(Value::to_string).collect::<Vec<_>>().join("\n"),
                ));
            }
        }
    }
    assert!(failed == 0, "{failed} of {} cases differ from Emacs:{failures}", cases.len());
}
