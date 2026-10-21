//! Commands on text with characters of several bytes: none may panic.
//!
//! The documents of the conformance cases, with letters replaced by
//! characters of two, three and four bytes, and every command run with
//! the cursor at every character (every seventh in a debug build). What
//! the commands make of them is checked elsewhere; here only that they
//! finish.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

use org_core::date::{Date, DateTime};
use org_core::{archive, edit, indent, meta_return, newline, todo};
use org_syntax::TodoKeywords;
use serde_json::Value;

fn documents() -> Vec<String> {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests"));
    let mut out = Vec::new();
    for name in
        ["folding", "format", "indent", "meta-return", "planning", "return", "shift", "todo", "archive", "refile"]
    {
        let cases: Vec<Value> =
            serde_json::from_str(&std::fs::read_to_string(root.join(name).join("cases.json")).unwrap()).unwrap();
        for case in cases {
            for key in ["text", "file"] {
                if let Some(text) = case[key].as_str() {
                    out.push(text.to_string());
                }
            }
            if let Some(files) = case["files"].as_object() {
                out.extend(files.values().filter_map(Value::as_str).map(str::to_string));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// `text` with some letters replaced by wider characters.
fn widened(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'e' => 'é',
            'o' => '🤦',
            't' => '€',
            'a' => 'ä',
            other => other,
        })
        .collect()
}

#[test]
fn commands_finish_on_text_of_several_bytes() {
    let keywords = TodoKeywords::default();
    let now = DateTime::new(Date::new(2026, 5, 18), 10, 30);
    let path = Path::new("/nonexistent/notes/a.org");
    let context = archive::Context { path, home: None, now };
    let archive_settings = archive::Settings::default();
    let edit_settings = edit::Settings::default();
    let call = edit::Call { count: None, repeated: false, now, note: None, skip_note: true, time_only: false };
    let commands = [
        edit::Command::Todo(todo::Target::Next),
        edit::Command::Priority(org_core::priority::Action::Up),
        edit::Command::Timestamp { forward: true },
        edit::Command::Date { forward: false },
        edit::Command::Shift(edit::Direction::Left),
        edit::Command::Shift(edit::Direction::Up),
        edit::Command::Checkbox,
    ];
    // Where each panic happened, for the thread it happened in: one
    // example of each place.
    thread_local! {
        static PLACE: RefCell<String> = const { RefCell::new(String::new()) };
    }
    std::panic::set_hook(Box::new(|info| {
        let place =
            info.location().map_or(String::new(), |location| format!("{}:{}", location.file(), location.line()));
        PLACE.with(|cell| *cell.borrow_mut() = format!("{place}: {}", info.payload_as_str().unwrap_or("")));
    }));
    // Each core takes the next document when it is done with one: one
    // at a time, this took two minutes of a debug build.
    let documents = documents();
    let threads = std::thread::available_parallelism().map_or(1, usize::from);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let check = || -> BTreeMap<String, String> {
        let mut failures: BTreeMap<String, String> = BTreeMap::new();
        while let Some(document) = documents.get(next.fetch_add(1, std::sync::atomic::Ordering::Relaxed)) {
            let text = widened(document);
            let parse = org_syntax::parse(&text, &keywords);
            let mut run = |what: &str, at: usize, f: &mut dyn FnMut()| {
                if catch_unwind(AssertUnwindSafe(f)).is_err() {
                    let place = PLACE.with(|cell| cell.borrow().clone());
                    let key = place.split(": ").next().unwrap_or("").to_string();
                    failures.entry(key).or_insert_with(|| format!("{place}\n    {what} at {at} of {text:?}"));
                }
            };
            for adapt in [indent::Adapt::Off, indent::Adapt::All] {
                let settings = indent::Settings { adapt, ..indent::Settings::default() };
                run("indent_region", 0, &mut || {
                    indent::indent_region(&text, 0, text.len(), &settings);
                });
                run("fixup_entry", 0, &mut || {
                    indent::fixup_entry(&text, 1, &settings);
                });
            }
            // Every character in an optimized build (`cargo test --release`);
            // every seventh otherwise, which the debug build of CI can afford.
            let step = if cfg!(debug_assertions) { 7 } else { 1 };
            let positions: Vec<usize> =
                text.char_indices().map(|(at, _)| at).chain([text.len()]).step_by(step).collect();
            for &at in &positions {
                let return_settings = newline::Settings {
                    indent: indent::Settings::default(),
                    tags_column: -77,
                    auto_align: true,
                    table: Default::default(),
                };
                run("org_return", at, &mut || drop(newline::org_return(&text, at, &keywords, &return_settings)));
                let meta = meta_return::Settings::default();
                run("meta_return", at, &mut || drop(meta_return::meta_return(&text, at, &keywords, &meta)));
                run("insert_todo_heading", at, &mut || {
                    drop(meta_return::insert_todo_heading(&text, at, &keywords, &meta, false))
                });
                run("insert_heading_after", at, &mut || {
                    meta_return::insert_heading_after(&text, at, &keywords, &meta);
                });
                run("indent_line", at, &mut || {
                    indent::indent_line(&text, at, &indent::Settings::default());
                });
                for command in &commands {
                    run(&format!("{command:?}"), at, &mut || {
                        drop(edit::run(command, &text, &parse, at, &call, &edit_settings))
                    });
                }
                run("archive_subtree", at, &mut || {
                    drop(archive::archive_subtree(&text, at, &keywords, &context, &archive_settings, &mut |_| None))
                });
                run("to_sibling", at, &mut || drop(archive::to_sibling(&text, at, &keywords, now, &archive_settings)));
                run("toggle_tag", at, &mut || drop(archive::toggle_tag(&text, at, &archive_settings)));
            }
        }
        failures
    };
    let mut failures: BTreeMap<String, String> = BTreeMap::new();
    std::thread::scope(|scope| {
        let running: Vec<_> = (0..threads).map(|_| scope.spawn(check)).collect();
        for thread in running {
            for (key, failure) in thread.join().expect("a checking thread") {
                failures.entry(key).or_insert(failure);
            }
        }
    });
    let _ = std::panic::take_hook();
    let found: Vec<&String> = failures.values().collect();
    assert!(
        found.is_empty(),
        "panics in {} places:\n{}",
        found.len(),
        found.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n")
    );
}
