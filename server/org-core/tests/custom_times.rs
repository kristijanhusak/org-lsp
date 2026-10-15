//! Timestamps shown in custom formats (`org-display-custom-times`), as
//! Emacs shows the texts of `tests/custom-times/cases.json`
//! (`scripts/custom_times_reference.py`).

use org_core::custom_time::{displays, DEFAULT_FORMATS};
use serde_json::{json, Value};

#[test]
fn timestamps_as_emacs_shows_them_in_custom_formats() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/custom-times/cases.json");
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for case in &cases {
        let text = case["text"].as_str().unwrap();
        let formats = match case.get("formats").and_then(Value::as_array) {
            Some(pair) => (pair[0].as_str().unwrap(), pair[1].as_str().unwrap()),
            None => DEFAULT_FORMATS,
        };
        let got: Vec<Value> =
            displays(text, formats).into_iter().map(|(start, end, shown)| json!([start, end, shown])).collect();
        if Value::Array(got.clone()) != case["expected"] {
            failures.push(format!("{text:?} {formats:?}: {got:?}, Emacs {}", case["expected"]));
        }
    }
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
}
