//! Conformance against Emacs: `tests/setupfile/cases.json` holds files, a
//! document among them with `#+SETUPFILE:` lines, and what Org read from
//! them (see `scripts/setupfile_reference.py`): the values of the keywords
//! of in-buffer settings, and the TODO keywords the document then has.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use org_syntax::TodoKeywords;
use serde_json::Value;

#[test]
fn setup_files_are_read_as_emacs_reads_them() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/setupfile/cases.json");
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let base = std::env::temp_dir().join(format!("org-lsp-setupfile-cases-{}", std::process::id()));
    let mut failures = String::new();
    for (index, case) in cases.iter().enumerate() {
        let expected = case.get("expected").expect("run scripts/setupfile_reference.py first");
        let root = base.join(format!("case-{index}"));
        for (name, text) in case["files"].as_object().unwrap() {
            let file = root.join(name);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, text.as_str().unwrap()).unwrap();
        }
        let root = root.canonicalize().unwrap();
        let document = root.join(case["document"].as_str().unwrap());
        let text = std::fs::read_to_string(&document).unwrap();
        // A file name as a path from the case's directory, as recorded.
        let relative = |value: &str| {
            let file = Path::new(value);
            let file = if file.is_absolute() { file.to_path_buf() } else { document.parent().unwrap().join(file) };
            file.strip_prefix(&root).map_or(value.to_string(), |file| file.display().to_string())
        };
        // The keys Org was asked for, each with its values that are not
        // empty, as `org-collect-keywords` gives them.
        let mut actual: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (key, value) in org_lsp::setup::keywords(Some(&document), &text) {
            if KEYS.contains(&key.as_str()) && !value.is_empty() {
                let value = if key == "BIBLIOGRAPHY" { relative(&value) } else { value };
                actual.entry(key).or_default().push(value);
            }
        }
        let wanted: BTreeMap<String, Vec<String>> = serde_json::from_value(expected["keywords"].clone()).unwrap();
        if actual != wanted {
            failures.push_str(&format!("\ncase {index} keywords:\n expected {wanted:?}\n actual   {actual:?}\n"));
        }
        let settings = org_lsp::setup::settings(Some(&document), &text);
        let parse = org_syntax::parse(&text, &TodoKeywords::default().with_setup(settings));
        let todo: Vec<&str> = parse.todo_keywords().names().collect();
        let wanted: Vec<&str> =
            expected["todo"].as_array().unwrap().iter().map(|word| word.as_str().unwrap()).collect();
        if todo != wanted {
            failures.push_str(&format!("\ncase {index} TODO keywords:\n expected {wanted:?}\n actual   {todo:?}\n"));
        }
    }
    let _ = std::fs::remove_dir_all(&base);
    assert!(failures.is_empty(), "{failures}");
}

/// The keywords `scripts/setupfile_reference.py` asks Org for.
const KEYS: &[&str] =
    &["TODO", "SEQ_TODO", "TYP_TODO", "TAGS", "FILETAGS", "STARTUP", "LINK", "PROPERTY", "CONSTANTS", "BIBLIOGRAPHY"];
