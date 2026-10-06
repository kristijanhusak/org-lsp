//! Tangling against Emacs.
//!
//! `tests/tangle/cases.json` holds Org files and what Emacs's
//! `org-babel-tangle` wrote for them (`scripts/update-tangle-fixtures.py`).
//! `ORG_TANGLE_CASES` points the test at other cases.

use std::path::Path;

use org_core::tangle::{self, Settings};
use org_syntax::TodoKeywords;
use serde_json::{json, Value};

#[test]
fn tangles_as_emacs_does() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/tangle/cases.json");
    let path = std::env::var("ORG_TANGLE_CASES").map_or(path, Into::into);
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let directory = Path::new("/case");
    let mut failures = Vec::new();
    for case in &cases {
        let files = case["files"].as_object().unwrap();
        let mut names: Vec<&String> = files.keys().filter(|name| name.ends_with(".org")).collect();
        names.sort();
        let text = files[names[0].as_str()].as_str().unwrap();
        let settings =
            Settings { preserve_indentation: case["settings"]["srcPreserveIndentation"].as_bool().unwrap_or(false) };
        let parse = org_syntax::parse(text, &TodoKeywords::default());
        let actual = match tangle::tangle(text, &parse, &directory.join(names[0]), &settings) {
            Ok(outcome) => {
                let relative = |path: &Path| path.strip_prefix(directory).unwrap().display().to_string();
                json!({
                    "files": outcome.files.iter().map(|file| (relative(&file.path), json!(file.text))).collect::<serde_json::Map<_, _>>(),
                    "modes": outcome.files.iter().filter_map(|file| file.mode.filter(|mode| *mode != 0o644).map(|mode| (relative(&file.path), json!(format!("{mode:o}"))))).collect::<serde_json::Map<_, _>>(),
                    "message": outcome.message(names[0]),
                })
            }
            Err(error) => json!({ "error": error }),
        };
        if actual != case["expected"] {
            failures.push(format!(
                "\n{}:\nexpected: {}\nactual:   {}",
                case["name"].as_str().unwrap_or("?"),
                serde_json::to_string_pretty(&case["expected"]).unwrap(),
                serde_json::to_string_pretty(&actual).unwrap()
            ));
        }
    }
    assert!(failures.is_empty(), "{} of {} cases differ:{}", failures.len(), cases.len(), failures.join("\n"));
}
