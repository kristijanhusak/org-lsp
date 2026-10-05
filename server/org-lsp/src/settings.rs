//! The server's options, with their types, defaults and meanings.
//!
//! This table is the single source for the documentation of the options:
//! `org-lsp settings` prints it, and the configuration pages of each
//! editor and the options in `docs/protocol.md` are written from it. A test checks
//! that the defaults written here are the ones `config` applies.

use serde_json::{json, Value};

pub struct Setting {
    /// The option's name; `agenda.span` is `span` inside `agenda`.
    pub name: &'static str,
    /// The values it takes, in JSON's terms.
    pub kind: &'static str,
    /// The default as JSON, or `None` when the default depends on
    /// something else; `unset` then says what applies.
    pub default: Option<&'static str>,
    pub unset: &'static str,
    /// The Org variable it stands for, if any.
    pub org: &'static str,
    pub doc: &'static str,
    /// An option for editor integrations rather than for users.
    pub integration: bool,
}

const fn setting(
    name: &'static str,
    kind: &'static str,
    default: &'static str,
    org: &'static str,
    doc: &'static str,
) -> Setting {
    Setting { name, kind, default: Some(default), unset: "", org, doc, integration: false }
}

pub const SETTINGS: &[Setting] = &[
    setting(
        "agendaFiles",
        "string[]",
        "[]",
        "org-agenda-files",
        "Files, directories and glob patterns (`*`, `?`, `**`) the agenda is built from. `~/` is the home \
         directory, and a directory stands for the Org files directly in it. When empty, the agenda is built \
         from the Org files under the workspace folders (hidden directories, `node_modules` and `target` left \
         out), or else from the Org files that are open.",
    ),
    setting(
        "todoKeywords",
        "string[]",
        r#"["TODO", "|", "DONE"]"#,
        "org-todo-keywords",
        "The TODO sequence, written like Org's `#+TODO:` line. `|` separates the states that count as done; \
         without it the last keyword is the only done state. `TODO(t)` gives a keyword a key for fast \
         selection, and `WAIT(w@/!)` makes it log a note (`@`) or the time (`!`) on entering and leaving it. \
         A file's own `#+TODO:` lines take precedence.",
    ),
    setting(
        "logDone",
        r#"false, "time", "note""#,
        "false",
        "org-log-done",
        "Write `CLOSED:` with the time when an entry is done, and with `note` ask for a note too.",
    ),
    setting(
        "logRepeat",
        r#"false, "time", "note""#,
        r#""time""#,
        "org-log-repeat",
        "Record when a repeating entry is done and moves to its next date.",
    ),
    setting(
        "logIntoDrawer",
        "boolean, string",
        "false",
        "org-log-into-drawer",
        "Put state changes and notes into a drawer: `true` for `LOGBOOK`, or the name of a drawer.",
    ),
    setting(
        "tagsColumn",
        "number",
        "-77",
        "org-tags-column",
        "Where tags are aligned after a change to a headline. Negative: they end at that column.",
    ),
    Setting {
        default: None,
        unset: "the first keyword of the sequence",
        ..setting(
            "todoRepeatToState",
            "string, true",
            "",
            "org-todo-repeat-to-state",
            "The state a repeating entry returns to when it is done: a keyword, or `true` for the state it \
             had.",
        )
    },
    setting("priorityHighest", "letter, or 0 to 64", r#""A""#, "org-priority-highest", "The highest priority."),
    setting("priorityLowest", "letter, or 0 to 64", r#""C""#, "org-priority-lowest", "The lowest priority."),
    setting(
        "priorityDefault",
        "letter, or 0 to 64",
        r#""B""#,
        "org-priority-default",
        "The priority of an entry without a cookie. A file's `#+PRIORITIES:` line takes precedence over the \
         three priority options.",
    ),
    setting(
        "priorityStartCycleWithDefault",
        "boolean",
        "true",
        "org-priority-start-cycle-with-default",
        "Raising or lowering the priority of an entry that has none starts at the default.",
    ),
    setting(
        "timestampRoundingMinutes",
        "number",
        "5",
        "org-time-stamp-rounding-minutes",
        "The step in which the minutes of a timestamp change (the second number of Org's option).",
    ),
    setting(
        "readDatePreferFuture",
        r#"boolean, "time""#,
        "true",
        "org-read-date-prefer-future",
        "A date typed without a year or month is the next one to come. With `time`, a time that has passed \
         today is tomorrow's.",
    ),
    setting(
        "readDateForceCompatibleDates",
        "boolean",
        "true",
        "org-read-date-force-compatible-dates",
        "A typed year is kept within 1970 to 2037.",
    ),
    setting(
        "refileTargets",
        "object[]",
        r#"[{ "files": "current", "level": 1 }]"#,
        "org-refile-targets",
        "Where entries can be refiled to. Each entry has `files`: `current` (the entry's own file), `agenda`, \
         a path or a list of paths; and one of `level`, `maxLevel`, `tag`, `todo`, `regexp` (the Rust `regex` \
         crate's syntax) or `all` (true), which says which headlines there are targets.",
    ),
    setting(
        "refileUseOutlinePath",
        r#"boolean, "file", "full-file-path", "title""#,
        "false",
        "org-refile-use-outline-path",
        "Name targets by their outline path (`Projects/Alpha/`). The three names start the path with the \
         file, and offer the file itself, for its top level.",
    ),
    setting(
        "reverseNoteOrder",
        "boolean",
        "false",
        "org-reverse-note-order",
        "A refiled entry goes first under its target, not last.",
    ),
    setting(
        "captureTemplates",
        "object[]",
        r#"[{ "key": "t", "description": "Task", "target": ["file+headline", "", "Tasks"], "template": "* TODO %?\n  %u\n  %a" }]"#,
        "org-capture-templates",
        "Capture templates. Each has a `key`, a `description`, a `type` (`entry`, the default, or `plain`), \
         a `target` and a `template` (text, or a list of lines), and may have `prepend`, `emptyLines`, \
         `emptyLinesBefore`, `emptyLinesAfter` and `immediateFinish`. The target is a list: `file` and a \
         path; `file+headline`, a path and a headline; or `file+olp`, a path and the headlines of an outline \
         path. An empty path is `defaultNotesFile`, and without a target the entry goes at the end of \
         `defaultNotesFile`. A template with only a key and a description groups the templates whose keys \
         start with its key.",
    ),
    setting(
        "orgDirectory",
        "string",
        r#""~/org""#,
        "org-directory",
        "Where the relative paths of capture targets are.",
    ),
    setting(
        "defaultNotesFile",
        "string",
        r#""~/.notes""#,
        "org-default-notes-file",
        "The target of a capture template whose file is empty.",
    ),
    setting(
        "cycleSeparatorLines",
        "number",
        "2",
        "org-cycle-separator-lines",
        "With at least this many blank lines after a subtree, the last one stays outside its fold. Negative: \
         all of them stay outside. `0`: none does.",
    ),
    setting(
        "agenda.span",
        r#""day", "week", "fortnight", "month", "year", or a number of days"#,
        r#""week""#,
        "org-agenda-span",
        "How much time the agenda shows.",
    ),
    setting(
        "agenda.startOnWeekday",
        "0 to 6 (0 is Sunday), or false",
        "1",
        "org-agenda-start-on-weekday",
        "The day a week or fortnight starts on; `false` starts it today.",
    ),
    setting(
        "agenda.deadlineWarningDays",
        "number",
        "14",
        "org-deadline-warning-days",
        "How many days before a deadline it shows up.",
    ),
    setting(
        "agenda.scheduledDelayDays",
        "number",
        "0",
        "org-scheduled-delay-days",
        "How many days after its date a scheduled entry shows up.",
    ),
    setting(
        "agenda.scheduledPastDays",
        "number",
        "10000",
        "org-scheduled-past-days",
        "How many days a scheduled entry that is not done keeps showing up.",
    ),
    setting(
        "agenda.deadlinePastDays",
        "number",
        "10000",
        "org-deadline-past-days",
        "How many days a missed deadline keeps showing up.",
    ),
    setting(
        "agenda.skipScheduledIfDone",
        "boolean",
        "false",
        "org-agenda-skip-scheduled-if-done",
        "Leave out scheduled entries that are done.",
    ),
    setting(
        "agenda.skipDeadlineIfDone",
        "boolean",
        "false",
        "org-agenda-skip-deadline-if-done",
        "Leave out deadlines of entries that are done.",
    ),
    setting(
        "agenda.skipTimestampIfDone",
        "boolean",
        "false",
        "org-agenda-skip-timestamp-if-done",
        "Leave out timestamps of entries that are done.",
    ),
    setting(
        "agenda.showFutureRepeats",
        r#"boolean, "next""#,
        "true",
        "org-agenda-show-future-repeats",
        "Show the future dates of repeating entries: all of them, none, or only the next.",
    ),
    setting("agenda.showAllDates", "boolean", "true", "org-agenda-show-all-dates", "Show days without entries."),
    setting(
        "agenda.tagsColumn",
        "number",
        "-80",
        "org-agenda-tags-column",
        "Where tags are aligned in the agenda. Org's own default follows the window width, which a server \
         does not know.",
    ),
    setting("agenda.useTimeGrid", "boolean", "true", "org-agenda-use-time-grid", "Draw the time grid."),
    setting("agenda.timeGrid.daily", "boolean", "true", "org-agenda-time-grid", "Draw the grid in a one-day agenda."),
    setting("agenda.timeGrid.today", "boolean", "true", "org-agenda-time-grid", "Draw the grid for today."),
    setting("agenda.timeGrid.weekly", "boolean", "false", "org-agenda-time-grid", "Draw the grid for every day."),
    setting(
        "agenda.timeGrid.requireTimed",
        "boolean",
        "true",
        "org-agenda-time-grid",
        "Draw the grid only on days with an entry that has a time.",
    ),
    setting(
        "agenda.timeGrid.removeMatch",
        "boolean",
        "false",
        "org-agenda-time-grid",
        "Leave out grid lines at a time an entry already has.",
    ),
    setting(
        "agenda.timeGrid.times",
        "number[]",
        "[800, 1000, 1200, 1400, 1600, 1800, 2000]",
        "org-agenda-time-grid",
        "The times of the grid lines, as `800` for 8:00.",
    ),
    setting(
        "agenda.timeGrid.trailer",
        "string",
        r#""......""#,
        "org-agenda-time-grid",
        "What follows a time that has no end time.",
    ),
    setting(
        "agenda.timeGrid.line",
        "string",
        r#""----------------""#,
        "org-agenda-time-grid",
        "The text of a grid line.",
    ),
    setting(
        "agenda.timeGrid.now",
        "string",
        r#""now - - - - - - - - - - - - - - - - - - - - - - - - -""#,
        "org-agenda-current-time-string",
        "The text of the line for the current time.",
    ),
    setting(
        "agenda.showCurrentTimeInGrid",
        "boolean",
        "true",
        "org-agenda-show-current-time-in-grid",
        "Show the current time in the grid.",
    ),
    setting(
        "agenda.weekendDays",
        "number[]",
        "[6, 0]",
        "org-agenda-weekend-days",
        "The days drawn as the weekend (0 is Sunday).",
    ),
    Setting {
        default: None,
        unset: "from the client's capabilities",
        integration: true,
        ..setting(
            "virtualDocuments",
            "boolean",
            "",
            "",
            "Whether the client fetches the agenda with `workspace/textDocumentContent`. A client that \
             announces that capability need not set this.",
        )
    },
];

/// The table as JSON: a list of `{ "name", "type", "default"?, "unset"?,
/// "org"?, "doc", "integration"? }`.
pub fn manifest() -> Value {
    Value::Array(
        SETTINGS
            .iter()
            .map(|setting| {
                let mut entry = json!({ "name": setting.name, "type": setting.kind, "doc": setting.doc });
                match setting.default {
                    Some(default) => entry["default"] = serde_json::from_str(default).expect("a default is JSON"),
                    None => entry["unset"] = json!(setting.unset),
                }
                if !setting.org.is_empty() {
                    entry["org"] = json!(setting.org);
                }
                if setting.integration {
                    entry["integration"] = json!(true);
                }
                entry
            })
            .collect(),
    )
}

/// The defaults as the options object a client would pass.
pub fn defaults() -> Value {
    let mut options = json!({});
    for setting in SETTINGS {
        let Some(default) = setting.default else { continue };
        let mut target = &mut options;
        let mut parts = setting.name.split('.').peekable();
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                target[part] = serde_json::from_str(default).expect("a default is JSON");
            } else {
                if target.get(part).is_none() {
                    target[part] = json!({});
                }
                target = &mut target[part];
            }
        }
    }
    options
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Options;

    /// Everything the server makes of its options.
    fn resolved(options: &Options) -> String {
        format!(
            "{:?}",
            (
                &options.agenda_files,
                options.todo_keywords(),
                options.agenda_config(),
                options.edit_settings(),
                options.read_date_settings(),
                options.refile_targets(),
                options.refile_outline_path(),
                options.refile_settings(),
                options.capture_templates(),
                options.org_directory(),
                options.default_notes_file(),
                options.separator_lines(),
            )
        )
    }

    #[test]
    fn the_defaults_written_here_are_the_ones_that_apply() {
        let none: Options = serde_json::from_str("{}").unwrap();
        let written: Options = serde_json::from_value(defaults()).unwrap();
        assert_eq!(resolved(&written), resolved(&none));
    }

    /// The dotted names of every option `Options` reads.
    fn names(value: &Value, prefix: &str, out: &mut Vec<String>) {
        for (key, value) in value.as_object().unwrap() {
            let name = format!("{prefix}{key}");
            match value {
                Value::Object(_) => names(value, &format!("{name}."), out),
                _ => out.push(name),
            }
        }
    }

    #[test]
    fn every_option_is_listed_and_every_listed_one_exists() {
        let mut read = Vec::new();
        names(&serde_json::to_value(Options::default()).unwrap(), "", &mut read);
        // A fixed clock, for tests only.
        read.retain(|name| name != "clock");
        read.sort();
        let mut listed: Vec<String> = SETTINGS.iter().map(|setting| setting.name.to_string()).collect();
        listed.sort();
        assert_eq!(listed, read);
    }

    #[test]
    fn names_are_unique_and_documented() {
        let mut names: Vec<&str> = SETTINGS.iter().map(|setting| setting.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), SETTINGS.len());
        for setting in SETTINGS {
            assert!(!setting.doc.is_empty() && (setting.default.is_some() || !setting.unset.is_empty()));
        }
    }
}
