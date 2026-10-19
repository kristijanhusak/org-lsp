//! The commands of `ol-bibtex.el` against Emacs.
//!
//! `tests/bibtex/cases.json` is written by
//! `scripts/update-bibtex-fixtures.py`: a document, where the cursor is,
//! the command, the answers to its questions and its options, and what
//! Emacs asked, said and left (or its error). Each question is checked by
//! running the command with the answers before it. `scripts/fuzz-bibtex.py`
//! replays random cases through it (`ORG_BIBTEX_CASES`).

use std::path::Path;

use org_core::bibtex::{self, Env, Settings, Stop};
use org_core::edit;
use org_core::indent::{self, Adapt};
use org_core::outline::Inheritance;
use org_syntax::TodoKeywords;
use serde_json::Value;

fn byte(text: &str, chars: usize) -> usize {
    text.char_indices().nth(chars).map_or(text.len(), |(at, _)| at)
}

/// A Lisp value of the cases: a string, `t`, `nil` or a list of strings.
fn strings(lisp: &str) -> Vec<String> {
    lisp.split('"').skip(1).step_by(2).map(String::from).collect()
}

fn string(lisp: &str) -> Option<String> {
    (lisp != "nil").then(|| strings(lisp).into_iter().next().unwrap_or_default())
}

fn show(text: &str, at: usize) -> String {
    format!("{:?}", format!("{}§{}", &text[..at.min(text.len())], &text[at.min(text.len())..]))
}

#[test]
fn runs_as_emacs_does() {
    let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/bibtex/cases.json"));
    let path = std::env::var("ORG_BIBTEX_CASES").map_or(path.to_path_buf(), Into::into);
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let text = case["text"].as_str().unwrap();
        let point = byte(text, case["point"].as_u64().unwrap() as usize);
        let command = case["command"].as_str().unwrap();
        let optional = case["optional"].as_bool().unwrap_or(false);
        let answers: Vec<String> = case["answers"]
            .as_array()
            .map(|answers| answers.iter().map(|answer| answer.as_str().unwrap().to_string()).collect())
            .unwrap_or_default();
        let mut settings = Settings::default();
        let mut edit_settings = edit::Settings::default();
        let mut indent_settings = indent::Settings::default();
        for (name, value) in case["options"].as_object().into_iter().flatten() {
            let value = value.as_str().unwrap();
            let on = value != "nil";
            match name.as_str() {
                "org-bibtex-autogen-keys" => settings.autogen_keys = on,
                "org-bibtex-prefix" => settings.prefix = string(value),
                "org-bibtex-treat-headline-as-title" => settings.treat_headline_as_title = on,
                "org-bibtex-export-arbitrary-fields" => settings.export_arbitrary_fields = on,
                "org-bibtex-key-property" => settings.key_property = string(value).unwrap(),
                "org-bibtex-tags" => settings.tags = strings(value),
                "org-bibtex-tags-are-keywords" => settings.tags_are_keywords = on,
                "org-bibtex-no-export-tags" => settings.no_export_tags = strings(value),
                "org-bibtex-inherit-tags" => settings.inherit_tags = on,
                "org-bibtex-type-property-name" => settings.type_property_name = string(value).unwrap(),
                "org-adapt-indentation" => {
                    let adapt = if on { Adapt::All } else { Adapt::Off };
                    edit_settings.todo.adapt = adapt;
                    indent_settings.adapt = adapt;
                }
                other => panic!("no option {other}"),
            }
        }
        let keywords = TodoKeywords::default();
        let inheritance = Inheritance::default();
        // A buffer without a file has the category `???`.
        let env = Env {
            settings: &settings,
            edit: &edit_settings,
            keywords: &keywords,
            inheritance: &inheritance,
            indent: &indent_settings,
            category: "???",
            buffer: " *temp*",
        };
        let expected = &case["expected"];
        let name = format!("#{index} {command} {}", show(text, point));
        // The entry Emacs generated a key from, and the key.
        let autokey_entry = expected["autokey"].as_str().map(String::from);
        let generated = |entry: &str| -> Result<String, String> {
            if let Some(wanted) = &autokey_entry {
                if wanted != entry {
                    return Err(format!("autokey of {entry:?}, Emacs had {wanted:?}"));
                }
            }
            let line = expected["text"].as_str().unwrap_or("").lines().find(|line| line.starts_with(":CUSTOM_ID:"));
            match (expected["error"].as_str(), line) {
                (Some(error), _) => Err(error.to_string()),
                (None, Some(line)) => Ok(line.trim_start_matches(":CUSTOM_ID:").trim().to_string()),
                (None, None) => Err("no key".to_string()),
            }
        };
        let run = |answers: &[String]| -> Result<(String, usize, Option<String>), Stop> {
            match command {
                "create" | "create-here" => {
                    bibtex::create(text, point, optional, command == "create-here", answers, &generated, &env)
                        .map(|outcome| (outcome.text, outcome.cursor, None))
                }
                "check" => bibtex::check(text, point, optional, answers, &generated, &env)
                    .map(|outcome| (outcome.text, outcome.cursor, None)),
                "check-all" => bibtex::check_all(text, point, answers, &generated, &env)
                    .map(|outcome| (outcome.text, outcome.cursor, None)),
                "yank" => {
                    let clipboard = case["clipboard"].as_str().unwrap();
                    bibtex::yank(text, point, clipboard, optional, answers, &env)
                        .map(|outcome| (outcome.text, outcome.cursor, None))
                }
                "import" => bibtex::import(text, point, case["bib"].as_str().unwrap(), &env)
                    .map(|(outcome, count)| (outcome.text, outcome.cursor, Some(format!("Parsed {count} entries")))),
                "copy" => match bibtex::headline(text, point, &env) {
                    Some(entry) => Ok((text.to_string(), point, Some(entry))),
                    None => Err(Stop::Failed("Wrong type argument: stringp, nil".to_string())),
                },
                "export" => {
                    let (file, count) = bibtex::export(text, &env);
                    Ok((text.to_string(), point, Some(format!("{count}\n{file}"))))
                }
                other => panic!("no command {other}"),
            }
        };
        // Each question, after the answers before it.
        let prompts = expected["prompts"].as_array().cloned().unwrap_or_default();
        for (asked, prompt) in prompts.iter().enumerate() {
            let wanted_prompt = prompt["prompt"].as_str().unwrap();
            let wanted_initial = prompt["initial"].as_str();
            let wanted_completions: Vec<&str> =
                prompt["completions"].as_array().into_iter().flatten().map(|c| c.as_str().unwrap()).collect();
            match run(&answers[..asked.min(answers.len())]) {
                Err(Stop::Ask(question))
                    if question.prompt == wanted_prompt
                        && question.initial.as_deref() == wanted_initial
                        && question.completions == wanted_completions => {}
                other
                    if asked >= answers.len()
                        && expected["error"].as_str().is_some_and(|e| e.starts_with("org-lsp: no answer")) =>
                {
                    if !matches!(other, Err(Stop::Ask(ref question)) if question.prompt == wanted_prompt) {
                        failures.push(format!("{name}\n  question {asked}: expected {prompt}\n  got {other:?}"));
                    }
                }
                other => failures.push(format!("{name}\n  question {asked}: expected {prompt}\n  got {other:?}")),
            }
        }
        if expected["error"].as_str().is_some_and(|error| error.starts_with("org-lsp: no answer")) {
            continue;
        }
        match (run(&answers), expected["error"].as_str()) {
            (Err(Stop::Failed(message)), Some(error)) if message == error => {}
            (outcome, Some(error)) => failures.push(format!("{name}\n  expected error {error}\n  got {outcome:?}")),
            (Err(stop), None) => failures.push(format!("{name}\n  expected {expected}\n  stopped {stop:?}")),
            (Ok((new, cursor, extra)), None) => {
                let want = expected["text"].as_str().unwrap();
                let want_cursor = byte(want, expected["point"].as_u64().unwrap() as usize);
                let want_extra = match command {
                    "copy" => expected["result"].as_str().map(String::from),
                    "export" => Some(format!(
                        "{}\n{}",
                        expected["message"].as_str().unwrap().split(' ').nth(2).unwrap(),
                        expected["file"].as_str().unwrap()
                    )),
                    "import" => expected["message"].as_str().map(String::from),
                    _ => None,
                };
                if new != want || cursor != want_cursor || extra != want_extra {
                    failures.push(format!(
                        "{name}\n  expected {} {want_extra:?}\n  got      {} {extra:?}",
                        show(want, want_cursor),
                        show(&new, cursor)
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{} of {} cases differ:\n{}", failures.len(), cases.len(), failures.join("\n"));
}
