//! Conformance against Emacs: `tests/capture/cases.json` holds Org files, a
//! capture template, the answers to its prompts and the text typed in the
//! capture buffer, with what `org-capture` showed and made of the files
//! (see `scripts/capture_reference.py`). `ORG_CAPTURE_CASES` points the test
//! at another file, which is how `scripts/fuzz-capture.py` feeds it
//! generated cases.

use std::path::PathBuf;

use org_core::capture::{fill_with, place, Context, Filled, Group, Kind, Outside, Properties, Target};
use org_core::date::DateTime;
use org_core::indent::Adapt;
use org_core::read_date::Settings;
use org_syntax::TodoKeywords;
use serde_json::Value;

fn clock(text: &str) -> DateTime {
    let (date, time) = text.split_once(' ').expect("date and time");
    DateTime { day: org_lsp::cli::parse_date(date).unwrap(), minute: org_lsp::cli::parse_time(time).unwrap() }
}

/// What org-lsp makes of a case: the capture buffer with `¦` at the
/// cursor, and the target file after typing.
fn run(case: &Value) -> Result<(String, String, String), String> {
    let template = &case["template"];
    let now = clock(case["clock"].as_str().unwrap());
    let file = case["from"].as_str().map(|name| (name.to_string(), format!("/{name}")));
    // Prompts take the answers in order, and nothing once they run out.
    let mut answers: Vec<String> =
        case["answers"].as_array().into_iter().flatten().map(|answer| answer.as_str().unwrap().to_string()).collect();
    // `:time-prompt`: the date of the tree comes first, and stands for
    // the time of the capture.
    let extend_today_until = case.pointer("/options/org_extend_today_until").and_then(Value::as_i64).unwrap_or(0);
    let settings = Settings { extend_today_until, ..Settings::default() };
    let mut default_time = now;
    let mut tree_date = org_core::capture::tree_date(now, &settings);
    if template["time_prompt"].as_bool() == Some(true) {
        let answer = if answers.is_empty() { String::new() } else { answers.remove(0) };
        default_time = org_core::capture::prompted_time(&answer, now, &settings)?;
        tree_date = default_time.date();
    }
    // `%k`: the headline clocked into first.
    let clock_heading =
        case["clock_in"].as_array().map_or(String::new(), |clock| clock[1].as_str().unwrap().to_string());
    let context = Context {
        now: default_time,
        file,
        initial: String::new(),
        annotation: String::new(),
        link_props: Vec::new(),
        user_name: String::new(),
        clock_heading,
        clock_link: String::new(),
        tags_column: -77,
        clipboard: case["clipboard"].as_str().unwrap_or_default().to_string(),
    };
    // `%[file]`: the case's files, by name; `%(sexp)`: evaluated by Emacs.
    let insert_file = |name: &str| case["files"][name].as_str().unwrap_or_default().to_string();
    let mut eval = |sexp: &str| -> Result<Option<String>, String> {
        let form = format!("(princ (let ((value (org-eval (read {sexp:?})))) (if value (concat \"S\" value) \"N\")))");
        let out = std::process::Command::new("emacs")
            .args(["--batch", "-Q", "--eval", "(require 'org)", "--eval", &form])
            .output()
            .map_err(|error| error.to_string())?;
        let out = String::from_utf8_lossy(&out.stdout).into_owned();
        Ok(out.strip_prefix('S').map(str::to_string))
    };
    let given = answers.len();
    let text = loop {
        let outside = Outside { insert_file: &insert_file, eval: &mut eval };
        match fill_with(template["template"].as_str().unwrap(), &context, &answers, &settings, Some(outside))
            .map_err(|refusal| refusal.message().to_string())?
        {
            Filled::Text(text) => break text,
            Filled::Ask(_) if answers.len() < given + 20 => answers.push(String::new()),
            Filled::Ask(question) => return Err(format!("still asking {question:?}")),
        }
    };
    let target_spec: Vec<&str> =
        template["target"].as_array().unwrap().iter().map(|part| part.as_str().unwrap()).collect();
    // The clock's file, clocked in first.
    let mut files: serde_json::Map<String, Value> = case["files"].as_object().unwrap().clone();
    let mut clocked = None;
    if let Some(clock_in) = case["clock_in"].as_array() {
        let (name, title) = (clock_in[0].as_str().unwrap(), clock_in[1].as_str().unwrap());
        let text = files[name].as_str().unwrap().to_string();
        let heading = text
            .lines()
            .scan(0, |at, line| {
                let start = *at;
                *at += line.len() + 1;
                Some((start, line))
            })
            .find(|(_, line)| line.starts_with('*') && line.contains(title))
            .map(|(start, _)| start)
            .unwrap();
        let keywords = TodoKeywords::from_document(&text).unwrap_or_default();
        let (text, _) = org_core::clock::clock_in(&text, &keywords, heading, &Default::default(), now)
            .map_err(|error| error.to_string())?;
        files.insert(name.to_string(), Value::String(text));
        clocked = Some((name.to_string(), heading));
    }
    let (name, target) = match target_spec[0] {
        "clock" => {
            let (name, heading) = clocked.ok_or("No running clock that could be used as capture target")?;
            (name, Target::Heading(heading))
        }
        kind => {
            let name = target_spec[1].to_string();
            let rest: Vec<String> = target_spec[2..].iter().map(|part| part.to_string()).collect();
            let target = match kind {
                "file" => Target::File,
                "file+headline" => Target::Headline(rest[0].clone()),
                "file+olp" => Target::Path(rest),
                "file+regexp" => Target::Regexp(rest[0].clone()),
                "file+olp+datetree" => {
                    let tree: Vec<String> = match &template["tree_type"] {
                        Value::String(name) => vec![name.clone()],
                        Value::Array(names) => names.iter().map(|name| name.as_str().unwrap().to_string()).collect(),
                        _ => Vec::new(),
                    };
                    let groups = Group::parse(&tree).map_err(|refusal| refusal.message().to_string())?;
                    Target::DateTree { path: rest, date: tree_date, groups }
                }
                other => return Err(format!("no target {other}")),
            };
            (name, target)
        }
    };
    let kind = match template["type"].as_str() {
        None | Some("entry") => Kind::Entry,
        Some("plain") => Kind::Plain,
        Some("item") => Kind::Item,
        Some("checkitem") => Kind::CheckItem,
        Some("table-line") => Kind::TableLine,
        Some(other) => return Err(format!("no type {other}")),
    };
    let number = |key: &str| template[key].as_u64().map(|n| n as usize);
    let properties = Properties {
        prepend: template["prepend"].as_bool().unwrap_or(false),
        empty_lines: number("empty_lines"),
        empty_lines_before: number("empty_lines_before"),
        empty_lines_after: number("empty_lines_after"),
        table_line_pos: template["table_line_pos"].as_str().map(str::to_string),
    };
    let original = files[&name].as_str().unwrap();
    let keywords = TodoKeywords::from_document(original).unwrap_or_default();
    let adapt = match case.pointer("/options/org_adapt_indentation") {
        Some(Value::Bool(true)) => Adapt::All,
        Some(Value::String(name)) if name == "headline-data" => Adapt::HeadlineData,
        _ => Adapt::Off,
    };
    let levels = org_core::refile::Settings { adapt, ..Default::default() };
    let placed = place(original, &keywords, &target, kind, &text, &properties, &levels, &Default::default())
        .map_err(|refusal| refusal.message().to_string())?;
    let at = placed.cursor - placed.start;
    let shown = placed.shown();
    let buffer = format!("{}¦{}", &shown[..at], &shown[at..]);
    let typed = case["typed"].as_str().unwrap_or("");
    let edited = format!("{}{typed}{}", &shown[..at], &shown[at..]);
    let settings = org_core::todo::Settings::default();
    Ok((name, buffer, placed.finalize(&edited, &keywords, &settings, now)))
}

#[test]
fn capturing_matches_emacs() {
    let path = match std::env::var_os("ORG_CAPTURE_CASES") {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/capture/cases.json"),
    };
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = String::new();
    let mut failed = 0;
    // `%(sexp)` needs Emacs, which CI's Rust job has not.
    let emacs = std::process::Command::new("emacs").arg("--version").output().is_ok_and(|out| out.status.success());
    assert!(emacs || std::env::var_os("ORG_LSP_REQUIRE_EMACS").is_none(), "Emacs is required but not found");
    for (index, case) in cases.iter().enumerate() {
        if !emacs && case["template"]["template"].as_str().is_some_and(|template| template.contains("%(")) {
            continue;
        }
        let expected = &case["expected"];
        let result = run(case);
        let same = match (expected, &result) {
            (Value::String(_), Err(_)) => true,
            (Value::Object(expected), Ok((name, buffer, after))) => {
                expected["buffer"].as_str() == Some(buffer.as_str())
                    && expected["files"][name].as_str() == Some(after.as_str())
            }
            _ => false,
        };
        if !same {
            failed += 1;
            if failed <= 10 {
                failures.push_str(&format!(
                    "\ncase {index}: {}\n--- files\n{:#}\n--- emacs\n{expected:#}\n--- org-lsp\n{result:#?}\n",
                    case["template"], case["files"]
                ));
            }
        }
    }
    assert!(failed == 0, "{failed} of {} captures differ from Emacs:{failures}", cases.len());
}
