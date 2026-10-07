//! Org's property commands against Emacs: `tests/properties/cases.json`
//! is written by `scripts/update-properties-fixtures.py`: a document,
//! where the cursor is, the command and the answers to its questions,
//! with the document Emacs leaves, the questions it asked and its
//! message. `ORG_PROPERTIES_CASES` points the test at another file.

use std::path::Path;

use org_core::date::{Date, DateTime};
use org_core::priority::{self, Action};
use org_core::properties::{self, Change, Context, Question};
use org_core::todo::{self, Target};
use serde_json::{json, Value};

/// What came of a case: the text (or the error), the questions asked
/// and the message.
#[derive(Debug, Default)]
struct Outcome {
    result: Option<Result<String, String>>,
    point: Option<usize>,
    prompts: Vec<Value>,
    message: Option<String>,
}

/// Asks `question` with the next answer, as the case gives them; `None`
/// when the case runs out of answers or answers what Emacs would not
/// take.
fn ask(question: Question, answers: &mut std::vec::IntoIter<String>, outcome: &mut Outcome) -> Option<String> {
    outcome.prompts.push(json!({
        "prompt": question.prompt,
        "collection": question.collection,
        "require_match": question.require_match,
        "default": question.default,
    }));
    let typed = answers.next()?;
    let answer = question.answer(&typed);
    // Emacs takes only one of the collection then (and the empty
    // answer); the fake prompt of the reference takes anything.
    if question.require_match
        && !typed.is_empty()
        && !question.collection.iter().any(|value| value.eq_ignore_ascii_case(&typed))
    {
        return None;
    }
    Some(answer)
}

fn set_property(
    text: &str,
    point: usize,
    answers: &mut std::vec::IntoIter<String>,
    context: &Context,
    outcome: &mut Outcome,
) -> Option<Result<String, String>> {
    let name = ask(properties::name_question(text, point, None, context), answers, outcome)?;
    if !properties::valid_name(&name) {
        return Some(Err(format!("Invalid property name: \"{name}\"")));
    }
    let question = match properties::value_question(text, point, &name, context) {
        Ok(question) => question,
        Err(error) => return Some(Err(error)),
    };
    let value = ask(question, answers, outcome)?;
    Some(match properties::set(text, point, &name, &value, context) {
        Ok(Change::None) => Ok(text.to_string()),
        Ok(Change::Text(text)) => Ok(text),
        Ok(Change::Todo(state)) => {
            let now = DateTime::new(Date::new(2026, 5, 18), 10, 0);
            outcome.message = Some(match &state {
                Some(state) => format!("TODO state changed to {state}"),
                None => "TODO state changed to ".to_string(),
            });
            todo::change_state(text, &context.keywords, point, &Target::State(state), &todo::Settings::default(), now)
        }
        Ok(Change::Priority(value)) => {
            let action = match value {
                Some(value) => Action::Set(priority::value(&value).unwrap_or(0)),
                None => Action::Remove,
            };
            priority::change(text, &context.keywords, point, action, false, &priority::Settings::default())
                .map_err(|refusal| refusal.message().to_string())
        }
        Err(error) => Err(error),
    })
}

fn delete_property(
    text: &str,
    point: usize,
    answers: &mut std::vec::IntoIter<String>,
    outcome: &mut Outcome,
) -> Option<Result<String, String>> {
    let name = match properties::delete_question(text, point) {
        Ok(question) => ask(question, answers, outcome)?,
        Err(name) => name,
    };
    let (text, _, _) = properties::delete(text, point, &name);
    outcome.message = Some(format!("Property \"{name}\" deleted"));
    Some(Ok(text))
}

/// What org-lsp makes of a case; `None` when there is nothing to compare.
fn run(case: &Value) -> Option<Outcome> {
    let text = case["text"].as_str().unwrap();
    let point = case["point"].as_u64().unwrap() as usize;
    let point = text.char_indices().nth(point).map_or(text.len(), |(at, _)| at);
    let mut answers: std::vec::IntoIter<String> = case["answers"]
        .as_array()
        .map(|answers| answers.iter().map(|answer| answer.as_str().unwrap().to_string()).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter();
    // The reference runs in a buffer without a file, whose category Org
    // writes as `???`.
    let mut context = Context { category: "???".to_string(), ..Context::default() };
    if let Some(globals) = case["options"]["org_global_properties"].as_object() {
        context.globals =
            globals.iter().map(|(name, value)| (name.clone(), value.as_str().unwrap().to_string())).collect();
    }
    let mut outcome = Outcome::default();
    let mut command = case["command"].as_str().unwrap().to_string();
    if command == "action" {
        command = match case["key"].as_str() {
            Some("s") => "set".to_string(),
            Some("d") => "delete".to_string(),
            Some("D") => "delete-globally".to_string(),
            Some(key) => {
                outcome.result = Some(Err(format!("No such property action {key}")));
                return Some(outcome);
            }
            None => return None,
        };
    }
    outcome.result = Some(match command.as_str() {
        "set" => set_property(text, point, &mut answers, &context, &mut outcome)?,
        "delete" => delete_property(text, point, &mut answers, &mut outcome)?,
        "delete-globally" => {
            let name = ask(properties::delete_globally_question(text, &context), &mut answers, &mut outcome)?;
            let (text, count) = properties::delete_globally(text, &name);
            outcome.message = Some(format!("Property \"{name}\" removed from {count} entries"));
            Ok(text)
        }
        "next" | "previous" => match properties::next_allowed(text, point, command == "previous", &context) {
            Ok((text, cursor)) => {
                outcome.point = Some(text[..cursor].chars().count());
                Ok(text)
            }
            Err(error) => Err(error),
        },
        "effort" => {
            let question = match properties::effort_question(text, point, &context) {
                Ok(question) => question,
                Err(error) => {
                    outcome.result = Some(Err(error));
                    return Some(outcome);
                }
            };
            let value = ask(question, &mut answers, &mut outcome)?;
            properties::set_effort(text, point, Some(&value), &context).map(|(text, message)| {
                outcome.message = Some(message);
                text
            })
        }
        "inc-effort" => properties::set_effort(text, point, None, &context).map(|(text, message)| {
            outcome.message = Some(message);
            text
        }),
        "ordered" => properties::toggle_ordered(text, point, &context).map(|(text, message)| {
            outcome.message = Some(message);
            text
        }),
        other => panic!("no command {other}"),
    });
    Some(outcome)
}

#[test]
fn property_commands_do_what_emacs_does() {
    let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/properties/cases.json"));
    let path = std::env::var("ORG_PROPERTIES_CASES").map_or(path.to_path_buf(), Into::into);
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = Vec::new();
    let mut compared = 0;
    for case in &cases {
        let expected = &case["expected"];
        let error = expected["error"].as_str();
        // Emacs out of answers: the case asked one question too many.
        if error.is_some_and(|error| error == "Asked once too often" || error == "Emacs did not finish") {
            continue;
        }
        // Emacs sets the effort before the first headline, then fails to
        // update the clock in its mode line.
        if error == Some("Invalid date: ") {
            continue;
        }
        // Computing needs column view.
        if case["key"] == "c" {
            continue;
        }
        let Some(outcome) = run(case) else { continue };
        compared += 1;
        let result = outcome.result.clone().unwrap();
        let mut same = match (&result, error) {
            // `Before first headline at position 1 in buffer …`
            (Err(ours), Some(theirs)) => ours == theirs || theirs.starts_with(&format!("{ours} at position")),
            (Ok(text), None) => expected["text"].as_str() == Some(text.as_str()),
            _ => false,
        };
        let prompts = Value::Array(outcome.prompts.clone());
        let theirs: Vec<Value> = expected["prompts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|prompt| {
                json!({
                    "prompt": prompt["prompt"], "collection": prompt["collection"],
                    "require_match": prompt["require_match"], "default": prompt["default"],
                })
            })
            .collect();
        same &= prompts == Value::Array(theirs.clone());
        if result.is_ok() && outcome.message.is_some() {
            same &= expected["message"].as_str() == outcome.message.as_deref();
        }
        if let Some(point) = outcome.point {
            same &= expected["point"].as_u64() == Some(point as u64);
        }
        if !same {
            failures.push(format!(
                "{:?} at {} {} {} {:?}\n  expected {}\n  got      {:?} point {:?} message {:?}\n  prompts  {}",
                case["text"].as_str().unwrap(),
                case["point"],
                case["command"],
                case["key"],
                case["answers"],
                expected,
                result,
                outcome.point,
                outcome.message,
                prompts,
            ));
        }
    }
    assert!(compared > 0);
    assert!(
        failures.is_empty(),
        "{} of {compared} cases differ:\n{}",
        failures.len(),
        failures.iter().take(10).cloned().collect::<Vec<_>>().join("\n")
    );
}
