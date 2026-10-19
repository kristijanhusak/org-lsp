//! `org-treat-S-cursor-todo-selection-as-state-change`,
//! `org-treat-insert-todo-heading-as-state-change` and
//! `org-edit-timestamp-down-means-later`, against what Org 9.8.10 makes
//! of the same text with `org-log-done` `time` and
//! `org-enforce-todo-dependencies` on, on Monday 18 May 2026, 10:30.

use org_core::date::{Date, DateTime};
use org_core::{edit, meta_return, todo};
use org_syntax::{Logging, TodoKeywords};

fn now() -> DateTime {
    DateTime::new(Date::new(2026, 5, 18), 10, 30)
}

fn todo_settings() -> todo::Settings {
    todo::Settings {
        log_done: Some(Logging::Time),
        log_repeat: Some(Logging::Time),
        enforce_todo_dependencies: true,
        ..todo::Settings::default()
    }
}

/// The text after a shifted cursor key on the first line that has the
/// cursor (its third character, or the year of a timestamp).
fn shift(text: &str, line: usize, direction: edit::Direction, settings: &edit::Settings) -> Result<String, String> {
    let keywords = TodoKeywords::from_document(text).unwrap_or_default();
    let parse = org_syntax::parse(text, &keywords);
    let start: usize = text.split_inclusive('\n').take(line).map(str::len).sum();
    let line_text = text[start..].lines().next().unwrap();
    let at = start + line_text.find('<').map_or(2, |open| open + 3);
    let call = edit::Call { count: None, repeated: false, now: now(), note: None, skip_note: false, time_only: false };
    edit::run(&edit::Command::Shift(direction), text, &parse, at, &call, settings)
        .map(|outcome| outcome.text)
        .map_err(|refusal| refusal.message().to_string())
}

#[test]
fn shifted_todo_changes_log_unless_told_not_to() {
    let logged = edit::Settings { todo: todo_settings(), ..edit::Settings::default() };
    let quiet = edit::Settings { shift_todo_is_state_change: false, ..logged.clone() };
    let right = edit::Direction::Right;
    assert_eq!(shift("* TODO a\n", 0, right, &logged).unwrap(), "* DONE a\nCLOSED: [2026-05-18 Mon 10:30]\n");
    assert_eq!(shift("* TODO a\n", 0, right, &quiet).unwrap(), "* DONE a\n");
    let closed = "* DONE a\nCLOSED: [2026-05-01 Fri 10:00]\n";
    assert_eq!(shift(closed, 0, right, &quiet).unwrap(), "* a\nCLOSED: [2026-05-01 Fri 10:00]\n");
    assert_eq!(shift(closed, 0, right, &logged).unwrap(), "* a\n");
    // Blocked by the open sibling before it, unless quiet.
    let ordered = "* P\n:PROPERTIES:\n:ORDERED: t\n:END:\n** TODO a\n** TODO b\n";
    assert_eq!(
        shift(ordered, 5, right, &quiet).unwrap(),
        "* P\n:PROPERTIES:\n:ORDERED: t\n:END:\n** TODO a\n** DONE b\n"
    );
    assert_eq!(
        shift(ordered, 5, right, &logged).unwrap_err(),
        "TODO state change from TODO to DONE blocked (by \"** TODO a\")"
    );
    // No note asked for.
    assert_eq!(
        shift("#+TODO: TODO WAIT(w@) | DONE\n* TODO a\n", 1, right, &quiet).unwrap(),
        "#+TODO: TODO WAIT(w@) | DONE\n* WAIT a\n"
    );
    // A repeater repeats, and logs as `org-log-repeat` says; the note at
    // the end of the text has no line break after it, as in Emacs.
    assert_eq!(
        shift("* TODO a\nSCHEDULED: <2026-05-18 Mon +1d>\n", 0, right, &quiet).unwrap(),
        "* TODO a\nSCHEDULED: <2026-05-19 Tue +1d>\n:PROPERTIES:\n:LAST_REPEAT: [2026-05-18 Mon 10:30]\n:END:\n\
         - State \"DONE\"       from \"TODO\"       [2026-05-18 Mon 10:30]"
    );
}

#[test]
fn timestamps_go_the_other_way_when_down_means_later() {
    let settings = edit::Settings { timestamp_down_means_later: true, ..edit::Settings::default() };
    let text = "* a <2026-05-18 Mon>\n";
    assert_eq!(shift(text, 0, edit::Direction::Up, &settings).unwrap(), "* a <2025-05-18 Sun>\n");
    assert_eq!(shift(text, 0, edit::Direction::Down, &settings).unwrap(), "* a <2027-05-18 Tue>\n");
}

#[test]
fn a_new_todo_heading_is_a_state_change_when_told() {
    let insert = |text: &str, line: usize, state_change: bool, todo: todo::Settings| {
        let keywords = TodoKeywords::from_document(text).unwrap_or_default();
        let start: usize = text.split_inclusive('\n').take(line).map(str::len).sum();
        let at = start + text[start..].lines().next().unwrap().len();
        let settings = meta_return::Settings {
            todo_state_change: state_change.then(|| (todo, now())),
            ..meta_return::Settings::default()
        };
        meta_return::insert_todo_heading(text, at, &keywords, &settings, false).unwrap().text
    };
    // `!` logs entering TODO, the note at the end without a line break.
    let logged = "#+TODO: TODO(t!) | DONE(d!)\n* TODO a\n";
    assert_eq!(insert(logged, 1, false, todo_settings()), "#+TODO: TODO(t!) | DONE(d!)\n* TODO a\n* TODO \n");
    assert_eq!(
        insert(logged, 1, true, todo_settings()),
        "#+TODO: TODO(t!) | DONE(d!)\n* TODO a\n* TODO \n- State \"TODO\"       from              [2026-05-18 Mon 10:30]"
    );
    let triggers = todo::Settings {
        state_tags_triggers: vec![("TODO".to_string(), vec![("t".to_string(), true)])],
        ..todo_settings()
    };
    assert_eq!(
        insert("* TODO a\n", 0, true, triggers.clone()),
        "* TODO a\n* TODO                                                                    :t:\n"
    );
    assert_eq!(insert("* TODO a\n", 0, false, triggers), "* TODO a\n* TODO \n");
}
