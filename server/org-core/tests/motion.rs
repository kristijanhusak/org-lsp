//! Org's motion commands against Emacs.
//!
//! `tests/motion/cases.json` is written by `scripts/motion_reference.py`:
//! a document, where the cursor is (bytes), the command, its count and how
//! many times it runs, and where Emacs leaves the point and what it says,
//! or its error. `scripts/fuzz-motion.py` runs random documents through it
//! (`ORG_MOTION_CASES`).

use std::path::Path;

use org_core::motion::{self, Motion};
use org_syntax::TodoKeywords;
use serde_json::Value;

#[test]
fn moves_as_emacs_does() {
    let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/motion/cases.json"));
    let path = std::env::var("ORG_MOTION_CASES").map_or(path.to_path_buf(), Into::into);
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for case in &cases {
        let text = case["text"].as_str().unwrap();
        let point = case["point"].as_u64().unwrap() as usize;
        let command = case["command"].as_str().unwrap();
        let count = case.get("count").and_then(Value::as_i64).unwrap_or(1);
        let times = case.get("times").and_then(Value::as_u64).unwrap_or(1);
        let which = match command {
            "org-next-visible-heading" => Motion::NextHeading,
            "org-previous-visible-heading" => Motion::PreviousHeading,
            "org-forward-heading-same-level" => Motion::ForwardSameLevel,
            "org-backward-heading-same-level" => Motion::BackwardSameLevel,
            "outline-up-heading" => Motion::Up,
            "org-next-link" => Motion::NextLink,
            "org-previous-link" => Motion::PreviousLink,
            other => panic!("unknown command {other}"),
        };
        let keywords = TodoKeywords::from_document(text).unwrap_or_default();
        let parse = org_syntax::parse(text, &keywords);
        let mut at = point;
        let mut failed = false;
        let mut outcome = Ok(None);
        for _ in 0..times {
            match motion::run(text, &parse, at, which, count, failed) {
                Ok(moved) => {
                    at = moved.offset;
                    failed = moved.failed;
                    outcome = Ok(moved.message);
                }
                Err(error) => {
                    outcome = Err(error);
                    break;
                }
            }
        }
        let expected = &case["expected"];
        let show =
            |at: usize| format!("{:?}", format!("{}§{}", &text[..at.min(text.len())], &text[at.min(text.len())..]));
        let name = format!("{command} {count} ×{times} {}", show(point));
        match (outcome, expected.get("error").and_then(Value::as_str)) {
            (Err(error), Some(want)) if error == want => {}
            (Err(error), _) => failures.push(format!("{name}\n  expected {expected}\n  error    {error}")),
            (Ok(_), Some(want)) => failures.push(format!("{name}\n  expected error {want}\n  got      {}", show(at))),
            (Ok(message), None) => {
                let want_at = expected["point"].as_u64().unwrap() as usize;
                // `outline-up-heading` sets the mark, which an editor does
                // not say.
                let want_message = expected["message"].as_str().filter(|said| *said != "Mark set");
                if at != want_at || message.as_deref() != want_message {
                    failures.push(format!(
                        "{name}\n  expected {} {want_message:?}\n  got      {} {message:?}",
                        show(want_at),
                        show(at)
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{} of {} cases differ:\n{}", failures.len(), cases.len(), failures.join("\n"));
}
