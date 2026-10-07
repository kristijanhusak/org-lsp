//! Setting tags against Emacs: `tests/tags/cases.json` is written by
//! `scripts/update-tags-fixtures.py`: a document, where the cursor is,
//! what is typed after `C-c C-q` (keys of fast tag selection, or the
//! answer at the completion prompt) and the options, with the document
//! Emacs leaves. `ORG_TAGS_CASES` points the test at another file.

use std::path::Path;

use org_core::tags::{self, Fast, Selection, Settings, Sort, State, Step};
use serde_json::Value;

fn settings(options: &Value) -> Settings {
    let alist = |name: &str| tags::tag_string_to_alist(options[name].as_str().unwrap_or(""));
    Settings {
        alist: alist("org_tag_alist"),
        persistent: alist("org_tag_persistent_alist"),
        fast: match &options["org_use_fast_tag_selection"] {
            Value::Bool(true) => Fast::Always,
            Value::Bool(false) => Fast::Never,
            _ => Fast::Auto,
        },
        single_key: options["org_fast_tag_selection_single_key"].as_bool().unwrap_or(false),
        sort: match options["org_tags_sort_function"].as_str() {
            Some("string<") => Some(Sort::Ascending),
            Some("string>") => Some(Sort::Descending),
            _ => None,
        },
        auto_align: options["org_auto_align_tags"].as_bool().unwrap_or(true),
        tags_column: -77,
    }
}

/// What org-lsp makes of a case: the new text, an error, or nothing to
/// compare (an answer typed at the completion prompt only counts as one).
fn run(case: &Value) -> Option<Result<String, String>> {
    let text = case["text"].as_str().unwrap();
    let point = case["point"].as_u64().unwrap() as usize;
    let point = text.char_indices().nth(point).map_or(text.len(), |(at, _)| at);
    let settings = settings(&case["options"]);
    let heading = match tags::heading_at(text, point) {
        Ok(heading) => heading,
        Err(refusal) => return Some(Err(refusal.message().to_string())),
    };
    let line = text[heading..].lines().next().unwrap_or("");
    let table = tags::table(text, &settings, &[]);
    let chosen = if tags::uses_fast_selection(&table, &settings) {
        let selection = Selection::new(&table);
        let mut state = State { current: tags::local_tags(line), groups: true, exit_after_next: settings.single_key };
        let mut keys = case["typed"].as_str().unwrap().chars();
        loop {
            let Some(key) = keys.next() else { return Some(Err("End of keyboard macro".to_string())) };
            let step = match tags::press(&mut state, key, &selection) {
                Step::Type => {
                    let typed: String = keys.by_ref().take_while(|key| *key != '\r').collect();
                    tags::type_tag(&mut state, &typed, &selection)
                }
                step => step,
            };
            match step {
                Step::Again | Step::Type => {}
                Step::Accept => break tags::parse_tags(&state.current.join(":")),
                Step::Quit => return Some(Err("Quit".to_string())),
            }
        }
    } else {
        tags::parse_tags(case["answer"].as_str()?)
    };
    Some(Ok(tags::set_tags(text, heading, &chosen, &settings)))
}

#[test]
fn sets_tags_as_emacs_does() {
    let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/tags/cases.json"));
    let path = std::env::var("ORG_TAGS_CASES").map_or(path.to_path_buf(), Into::into);
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = Vec::new();
    let mut compared = 0;
    for case in &cases {
        // Emacs stuck at its prompt in batch mode: nothing to compare.
        if case["expected"]["error"] == "Emacs did not finish" {
            continue;
        }
        let Some(outcome) = run(case) else { continue };
        compared += 1;
        let expected = &case["expected"];
        let same = match (&outcome, expected.get("error").and_then(Value::as_str)) {
            (Err(_), Some(_)) => true,
            (Ok(text), None) => expected["text"].as_str() == Some(text.as_str()),
            _ => false,
        };
        if !same {
            failures.push(format!(
                "{:?} typed {:?} options {}\n  expected {expected}\n  got      {outcome:?}",
                case["text"].as_str().unwrap(),
                case["typed"].as_str().unwrap(),
                case["options"]
            ));
        }
    }
    assert!(compared > 0);
    assert!(
        failures.is_empty(),
        "{} of {compared} cases differ:\n{}",
        failures.len(),
        failures.iter().take(12).cloned().collect::<Vec<_>>().join("\n")
    );
}
