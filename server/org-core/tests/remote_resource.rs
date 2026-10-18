//! `org--should-fetch-remote-resource-p`: whether a remote resource is
//! downloaded, what is asked first and what the answer marks as safe, as
//! Emacs answered for the cases of `tests/link/remote-resources.json`
//! (`scripts/remote_resource_reference.py`).

use org_core::remote_resource::{answer_fetches, confirm, safe_pattern, should_fetch, Fetch, Policy};
use serde_json::{json, Value};

fn policy(value: &Value) -> Policy {
    match value {
        Value::Bool(true) => Policy::Always,
        Value::Bool(false) | Value::Null => Policy::Never,
        Value::String(name) if name == "safe" => Policy::Safe,
        _ => Policy::Prompt,
    }
}

#[test]
fn downloads_and_asks_as_emacs_does() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/link/remote-resources.json");
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for case in &cases {
        let uri = case["uri"].as_str().unwrap();
        let file = case["file"].as_str();
        let safe: Vec<String> =
            case["safe"].as_array().unwrap().iter().map(|pattern| pattern.as_str().unwrap().to_string()).collect();
        let key = case["key"].as_str().unwrap().chars().next().unwrap();
        let found = match should_fetch(policy(&case["policy"]), uri, file, &safe) {
            Fetch::Yes => json!({ "fetch": true, "prompt": null, "text": null, "saved": null }),
            Fetch::No => json!({ "fetch": false, "prompt": null, "text": null, "saved": null }),
            Fetch::Ask => {
                let question = confirm(uri, file);
                json!({
                    "fetch": answer_fetches(key),
                    "prompt": question.prompt,
                    "text": question.text,
                    "saved": safe_pattern(key, uri, file),
                })
            }
        };
        if found != case["expected"] {
            failures.push(format!("{case}\n  found: {found}"));
        }
    }
    assert!(failures.is_empty(), "{} of {} cases differ:\n{}", failures.len(), cases.len(), failures.join("\n"));
}

/// The regexps an answer adds match what they were made for. (That of a
/// domain does not match its URLs with a port, in Emacs either.)
#[test]
fn saved_patterns_match_what_they_name() {
    let uri = "https://www.example.com/a/b+c.png";
    let file = Some("/notes/a.org");
    for key in ['!', 'd', 'f'] {
        let pattern = safe_pattern(key, uri, file).unwrap();
        assert_eq!(should_fetch(Policy::Safe, uri, file, &[pattern]), Fetch::Yes, "{key}");
    }
    let domain = safe_pattern('d', uri, None).unwrap();
    assert_eq!(
        should_fetch(Policy::Safe, "https://www.example.com/other.png", None, std::slice::from_ref(&domain)),
        Fetch::Yes
    );
    assert_eq!(should_fetch(Policy::Safe, "https://www.example.community/x.png", None, &[domain]), Fetch::No);
}
