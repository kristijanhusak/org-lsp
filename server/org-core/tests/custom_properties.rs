//! The lines `org-toggle-custom-properties-visibility` hides, the
//! properties of `org-custom-properties` in the property drawers, as
//! Emacs hides them in the texts of `tests/properties/custom.json`
//! (`scripts/custom_properties_reference.py`), or of the file
//! `ORG_CUSTOM_PROPERTIES_CASES` names (`scripts/fuzz-custom-props.py`).

use org_core::properties::custom_property_lines;
use serde_json::Value;

#[test]
fn hides_the_lines_emacs_hides() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/properties/custom.json");
    let path = std::env::var("ORG_CUSTOM_PROPERTIES_CASES").unwrap_or_else(|_| path.to_string());
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for case in &cases {
        let text = case["text"].as_str().unwrap();
        let names: Vec<String> =
            case["names"].as_array().unwrap().iter().map(|name| name.as_str().unwrap().to_string()).collect();
        let got = custom_property_lines(text, &names);
        let expected: Vec<usize> =
            case["expected"].as_array().unwrap().iter().map(|line| line.as_u64().unwrap() as usize).collect();
        if got != expected {
            failures.push(format!("{text:?} {names:?}: {got:?}, Emacs {expected:?}"));
        }
    }
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
}
