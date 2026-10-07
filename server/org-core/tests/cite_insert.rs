//! Inserting and editing citations against Emacs: `tests/cite/insert.json`
//! is written by `scripts/update-cite-fixtures.py`: an Org document with
//! the cursor (`¦`), the prefix argument and the answers to the prompts,
//! with the document `org-cite-insert` leaves (or its error) and the
//! prompts it asks. `ORG_CITE_INSERT_CASES` points the test at another
//! file.

use std::path::Path;

use org_core::cite::{self, Insert};
use org_syntax::TodoKeywords;
use serde_json::{json, Value as Json};

/// What org-lsp makes of a case, in the form of the reference.
fn run(case: &Json) -> Json {
    let marked = case["text"].as_str().unwrap();
    let offset = marked.find('¦').unwrap();
    let text = marked.replacen('¦', "", 1);
    let arg = case["arg"].as_bool().unwrap_or(false);
    let mut keys: Vec<String> =
        case["keys"].as_array().into_iter().flatten().map(|key| key.as_str().unwrap().to_string()).collect();
    keys.reverse();
    let style = case["style"].as_str().map(str::to_string);
    let parse = org_syntax::parse(&text, &TodoKeywords::default());
    let mut prompts: Vec<String> = Vec::new();
    let outcome = (|| -> Result<String, String> {
        match cite::insert(&text, &parse, offset, arg)? {
            Insert::Delete(changes) => Ok(cite::apply(&text, &changes)),
            Insert::Style { start, end } => {
                prompts.push("Style (\"\" for default): ".to_string());
                let style = style.clone().ok_or("No answer")?;
                let style = if style.trim().is_empty() { String::new() } else { format!("/{style}") };
                Ok(cite::apply(&text, &[cite::Change { start, end, text: style }]))
            }
            Insert::Key { place, reference } => {
                prompts.push("Key: ".to_string());
                let key = keys.pop().ok_or("No answer")?;
                if key.is_empty() && reference {
                    return Err("Aborted".to_string());
                }
                Ok(cite::apply(&text, &[place.change(&key)]))
            }
            Insert::New { at, style: ask_style } => {
                let mut chosen: Vec<String> = Vec::new();
                loop {
                    prompts.push(cite::keys_prompt(&chosen));
                    let key = keys.pop().ok_or("No answer")?;
                    if key.trim().is_empty() {
                        break;
                    }
                    chosen.push(key);
                }
                if chosen.is_empty() {
                    return Err("Aborted".to_string());
                }
                let style = if ask_style {
                    prompts.push("Style (\"\" for default): ".to_string());
                    style.clone().ok_or("No answer")?
                } else {
                    String::new()
                };
                let citation = cite::new_citation(&chosen, &style);
                Ok(cite::apply(&text, &[cite::Change { start: at, end: at, text: citation }]))
            }
        }
    })();
    match outcome {
        Ok(text) => json!({ "text": text, "prompts": prompts }),
        Err(error) => json!({ "error": error, "prompts": prompts }),
    }
}

#[test]
fn citations_are_inserted_as_in_emacs() {
    let path = std::env::var("ORG_CITE_INSERT_CASES").unwrap_or_else(|_| {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/cite/insert.json").display().to_string()
    });
    let cases: Vec<Json> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let expected = &case["expected"];
        if expected.get("skip").is_some() {
            continue;
        }
        let actual = run(case);
        if &actual != expected {
            failures.push(format!(
                "case {index}: {}\n  arg {} keys {} style {}\n  expected {expected}\n  actual   {actual}",
                serde_json::to_string(&case["text"]).unwrap(),
                case["arg"],
                case["keys"],
                case["style"],
            ));
        }
    }
    assert!(failures.is_empty(), "{} of {} cases differ:\n{}", failures.len(), cases.len(), failures.join("\n"));
}
