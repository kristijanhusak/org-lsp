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
    /// An option an editor gives, which the shared configuration file
    /// does not.
    pub editor_only: bool,
}

const fn setting(
    name: &'static str,
    kind: &'static str,
    default: &'static str,
    org: &'static str,
    doc: &'static str,
) -> Setting {
    Setting { name, kind, default: Some(default), unset: "", org, doc, integration: false, editor_only: false }
}

pub const SETTINGS: &[Setting] = &[
    Setting {
        default: None,
        unset: "the shared file, `~/.config/org-lsp/config.toml` or `config.json`",
        editor_only: true,
        ..setting(
            "configFile",
            "false, or a path",
            "",
            "",
            "The shared configuration file, whose options this editor's go on top of: `false` for none, or \
             the path of another file (JSON if it ends in `.json`, else TOML). Unset, it is `$ORG_LSP_CONFIG`, \
             else `config.toml` or `config.json` in `$XDG_CONFIG_HOME/org-lsp` (`~/.config/org-lsp`; \
             `%APPDATA%\\org-lsp` on Windows).",
        )
    },
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
    setting(
        "clockIntoDrawer",
        "boolean, string",
        "true",
        "org-clock-into-drawer",
        "Where `CLOCK:` lines go: `true` for the drawer of `logIntoDrawer` (`LOGBOOK` when that is not a name), \
         the name of a drawer, or `false` for the entry itself.",
    ),
    setting(
        "clockOutRemoveZeroTimeClocks",
        "boolean",
        "false",
        "org-clock-out-remove-zero-time-clocks",
        "Clocking out after less than a minute removes the `CLOCK:` line.",
    ),
    setting(
        "clockOutWhenDone",
        "boolean",
        "true",
        "org-clock-out-when-done",
        "Marking the clocked entry done stops the clock.",
    ),
    setting(
        "completeTagsAlwaysOfferAllAgendaTags",
        "boolean",
        "false",
        "org-complete-tags-always-offer-all-agenda-tags",
        "Completing a tag offers the tags of every agenda file, after those of the file. Off, only the \
         file's own.",
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
        "linkDescriptive",
        "boolean",
        "true",
        "org-link-descriptive",
        "Show a link as its description (or its target, when it has none) and hide the rest: the brackets, \
         and the target of a link with a description. The text is marked `hidden` in the highlighting, \
         and editors that can conceal text conceal it.",
    ),
    setting(
        "hideEmphasisMarkers",
        "boolean",
        "false",
        "org-hide-emphasis-markers",
        "Hide the markers around `*bold*`, `/italic/`, `_underline_`, `+strike-through+`, `=verbatim=` \
         and `~code~`, as for links.",
    ),
    setting(
        "autoSave",
        "boolean",
        "true",
        "",
        "Save the files a command changes other than the one it was run in: an entry's file changed from \
         the agenda, the target of a capture or of a refile. The editor saves them once it has made the \
         change. Off, they are left with unsaved changes, as Emacs leaves them.",
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
    setting(
        "agenda.startWithLogMode",
        "boolean",
        "false",
        "org-agenda-start-with-log-mode",
        "Start the agenda in log mode, which shows what was closed and clocked each day (`l` toggles it).",
    ),
    setting(
        "agenda.startWithClockreportMode",
        "boolean",
        "false",
        "org-agenda-start-with-clockreport-mode",
        "Start the agenda with the clock report under it (`R` toggles it).",
    ),
    setting(
        "agenda.logModeItems",
        "string[]",
        r#"["closed", "clock"]"#,
        "org-agenda-log-mode-items",
        "What log mode shows: `closed` entries, `clock` lines and `state` changes.",
    ),
    Setting {
        default: None,
        unset: "today",
        ..setting(
            "agenda.startDay",
            "string",
            "",
            "org-agenda-start-day",
            "The day the agenda starts from, as the date prompt reads it: `-1d`, `+2d`, `2026-10-01`.",
        )
    },
    setting(
        "agenda.customCommands",
        "object[]",
        "[]",
        "org-agenda-custom-commands",
        "Commands of the agenda menu. Each has a `key` and a `description`, and either a `type` (`agenda`, \
         `alltodo`, `todo`, `tags` or `tags-todo`) with a `match` (keywords such as `NEXT|WAIT` for `todo`, \
         a match string such as `work-home` or `PRIORITY=\"A\"` for `tags`), or `blocks`, a list of such \
         views shown one after the other. `header` replaces a view's header (empty: none), and `settings` \
         holds agenda options for the command or a block (`span`, `startDay`…). A command with only a key \
         and a description groups the commands whose keys start with its key.",
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

/// The values each option takes, as JSON Schema: what editors complete and
/// check the options against (VS Code's settings, the schema of the shared
/// file, Neovim's type annotations). An object that is a type of its own
/// names it in `x-lua-class`.
const SCHEMAS: &[(&str, &str)] = &[
    ("configFile", r#"{ "anyOf": [{ "const": false }, { "type": "string" }] }"#),
    ("agendaFiles", r#"{ "type": "array", "items": { "type": "string" } }"#),
    ("todoKeywords", r#"{ "type": "array", "items": { "type": "string" } }"#),
    ("logDone", r#"{ "enum": [false, "time", "note"] }"#),
    ("logRepeat", r#"{ "enum": [false, "time", "note"] }"#),
    ("logIntoDrawer", r#"{ "type": ["boolean", "string"] }"#),
    ("tagsColumn", r#"{ "type": "integer" }"#),
    ("completeTagsAlwaysOfferAllAgendaTags", BOOLEAN),
    ("clockIntoDrawer", r#"{ "type": ["boolean", "string"] }"#),
    ("clockOutRemoveZeroTimeClocks", BOOLEAN),
    ("clockOutWhenDone", BOOLEAN),
    ("todoRepeatToState", r#"{ "anyOf": [{ "type": "string" }, { "const": true }] }"#),
    ("priorityHighest", PRIORITY),
    ("priorityLowest", PRIORITY),
    ("priorityDefault", PRIORITY),
    ("priorityStartCycleWithDefault", BOOLEAN),
    ("timestampRoundingMinutes", r#"{ "type": "integer", "minimum": 0 }"#),
    ("readDatePreferFuture", r#"{ "enum": [true, false, "time"] }"#),
    ("readDateForceCompatibleDates", BOOLEAN),
    (
        "refileTargets",
        r#"{ "type": "array", "items": { "type": "object", "x-lua-class": "org_lsp.RefileTarget", "additionalProperties": false, "properties": {
            "files": { "anyOf": [{ "enum": ["current", "agenda"] }, { "type": "string" }, { "type": "array", "items": { "type": "string" } }],
                "description": "`current` (the entry's own file), `agenda`, a path or a list of paths" },
            "level": { "type": "integer", "minimum": 1, "description": "The headlines of this level" },
            "maxLevel": { "type": "integer", "minimum": 1, "description": "The headlines up to this level" },
            "tag": { "type": "string", "description": "The headlines with this tag" },
            "todo": { "type": "string", "description": "The headlines with this TODO keyword" },
            "regexp": { "type": "string", "description": "The headlines that match this regular expression" },
            "all": { "type": "boolean", "description": "Every headline" }
        } } }"#,
    ),
    ("refileUseOutlinePath", r#"{ "enum": [true, false, "file", "full-file-path", "title"] }"#),
    ("reverseNoteOrder", BOOLEAN),
    (
        "captureTemplates",
        r#"{ "type": "array", "items": { "type": "object", "x-lua-class": "org_lsp.CaptureTemplate", "required": ["key", "description"], "additionalProperties": false, "properties": {
            "key": { "type": "string", "description": "The keys that pick the template" },
            "description": { "type": "string", "description": "What the menu shows" },
            "type": { "enum": ["entry", "plain"], "description": "An Org entry (the default), or plain text" },
            "target": { "type": "array", "items": { "type": "string" },
                "description": "`file` and a path; `file+headline`, a path and a headline; or `file+olp`, a path and the headlines of an outline path" },
            "template": { "anyOf": [{ "type": "string" }, { "type": "array", "items": { "type": "string" } }], "description": "The template, or its lines" },
            "prepend": { "type": "boolean", "description": "First under the target, not last" },
            "emptyLines": { "type": "integer", "minimum": 0, "description": "Blank lines before and after" },
            "emptyLinesBefore": { "type": "integer", "minimum": 0, "description": "Blank lines before" },
            "emptyLinesAfter": { "type": "integer", "minimum": 0, "description": "Blank lines after" },
            "immediateFinish": { "type": "boolean", "description": "Capture without a capture buffer" }
        } } }"#,
    ),
    ("orgDirectory", STRING),
    ("defaultNotesFile", STRING),
    ("linkDescriptive", BOOLEAN),
    ("hideEmphasisMarkers", BOOLEAN),
    ("autoSave", BOOLEAN),
    ("cycleSeparatorLines", r#"{ "type": "integer" }"#),
    (
        "agenda.span",
        r#"{ "anyOf": [{ "enum": ["day", "week", "fortnight", "month", "year"] }, { "type": "integer", "minimum": 1 }] }"#,
    ),
    (
        "agenda.startOnWeekday",
        r#"{ "anyOf": [{ "type": "integer", "minimum": 0, "maximum": 6 }, { "const": false }] }"#,
    ),
    ("agenda.deadlineWarningDays", INTEGER),
    ("agenda.scheduledDelayDays", INTEGER),
    ("agenda.scheduledPastDays", INTEGER),
    ("agenda.deadlinePastDays", INTEGER),
    ("agenda.skipScheduledIfDone", BOOLEAN),
    ("agenda.skipDeadlineIfDone", BOOLEAN),
    ("agenda.skipTimestampIfDone", BOOLEAN),
    ("agenda.showFutureRepeats", r#"{ "enum": [true, false, "next"] }"#),
    ("agenda.showAllDates", BOOLEAN),
    ("agenda.tagsColumn", INTEGER),
    ("agenda.useTimeGrid", BOOLEAN),
    ("agenda.timeGrid.daily", BOOLEAN),
    ("agenda.timeGrid.today", BOOLEAN),
    ("agenda.timeGrid.weekly", BOOLEAN),
    ("agenda.timeGrid.requireTimed", BOOLEAN),
    ("agenda.timeGrid.removeMatch", BOOLEAN),
    ("agenda.timeGrid.times", r#"{ "type": "array", "items": { "type": "integer", "minimum": 0, "maximum": 2400 } }"#),
    ("agenda.timeGrid.trailer", STRING),
    ("agenda.timeGrid.line", STRING),
    ("agenda.timeGrid.now", STRING),
    ("agenda.showCurrentTimeInGrid", BOOLEAN),
    ("agenda.weekendDays", r#"{ "type": "array", "items": { "type": "integer", "minimum": 0, "maximum": 6 } }"#),
    ("agenda.startDay", STRING),
    ("agenda.startWithLogMode", BOOLEAN),
    ("agenda.startWithClockreportMode", BOOLEAN),
    ("agenda.logModeItems", r#"{ "type": "array", "items": { "enum": ["closed", "clock", "state"] } }"#),
    (
        "agenda.customCommands",
        r#"{ "type": "array", "items": { "type": "object", "x-lua-class": "org_lsp.AgendaCommand", "required": ["key", "description"], "additionalProperties": false, "properties": {
            "key": { "type": "string", "description": "The keys that pick the command in the agenda menu" },
            "description": { "type": "string", "description": "What the menu shows" },
            "type": { "enum": ["agenda", "alltodo", "todo", "tags", "tags-todo"], "description": "The view" },
            "match": { "type": "string", "description": "For todo, keywords (NEXT|WAIT); for tags, a match string (work-home, PRIORITY=\"A\")" },
            "header": { "type": "string", "description": "Replaces the view's header; empty for none" },
            "settings": { "type": "object", "description": "Agenda options for this command: span, startDay…" },
            "blocks": { "type": "array", "description": "Views shown one after the other", "items": { "type": "object", "x-lua-class": "org_lsp.AgendaBlock", "required": ["type"], "additionalProperties": false, "properties": {
                "type": { "enum": ["agenda", "alltodo", "todo", "tags", "tags-todo"], "description": "The view" },
                "match": { "type": "string", "description": "For todo, keywords; for tags, a match string" },
                "header": { "type": "string", "description": "Replaces the view's header; empty for none" },
                "settings": { "type": "object", "description": "Agenda options for this block: span, startDay…" }
            } } }
        } } }"#,
    ),
    ("virtualDocuments", BOOLEAN),
];

const BOOLEAN: &str = r#"{ "type": "boolean" }"#;
const INTEGER: &str = r#"{ "type": "integer" }"#;
const STRING: &str = r#"{ "type": "string" }"#;
const PRIORITY: &str = r#"{ "anyOf": [{ "type": "string", "pattern": "^[A-Za-z]$" }, { "type": "integer", "minimum": 0, "maximum": 64 }] }"#;

/// The schema of the option `name`.
pub fn schema(name: &str) -> Option<Value> {
    let (_, schema) = SCHEMAS.iter().find(|(known, _)| *known == name)?;
    serde_json::from_str(schema).ok()
}

/// The table as JSON: a list of `{ "name", "type", "default"?, "unset"?,
/// "org"?, "doc", "schema", "integration"?, "editorOnly"? }`.
pub fn manifest() -> Value {
    Value::Array(
        SETTINGS
            .iter()
            .map(|setting| {
                let mut entry = json!({ "name": setting.name, "type": setting.kind, "doc": setting.doc });
                if let Some(schema) = schema(setting.name) {
                    entry["schema"] = schema;
                }
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
                if setting.editor_only {
                    entry["editorOnly"] = json!(true);
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

    /// Whether `value` is what `schema` allows, for the parts of JSON
    /// Schema the table uses.
    fn allows(schema: &Value, value: &Value) -> bool {
        if let Some(choices) = schema.get("anyOf").and_then(Value::as_array) {
            return choices.iter().any(|choice| allows(choice, value));
        }
        if let Some(constant) = schema.get("const") {
            return constant == value;
        }
        if let Some(choices) = schema.get("enum").and_then(Value::as_array) {
            return choices.contains(value);
        }
        let of_type = |name: &str| match name {
            "boolean" => value.is_boolean(),
            "string" => value.is_string(),
            "integer" => value.is_i64() || value.is_u64(),
            "array" => value.as_array().is_some_and(|items| {
                items.iter().all(|item| schema.get("items").is_none_or(|items| allows(items, item)))
            }),
            "object" => value.as_object().is_some_and(|fields| {
                fields.iter().all(|(key, field)| {
                    schema.pointer(&format!("/properties/{key}")).is_some_and(|property| allows(property, field))
                })
            }),
            _ => false,
        };
        match &schema["type"] {
            Value::String(name) => of_type(name),
            Value::Array(names) => names.iter().filter_map(Value::as_str).any(of_type),
            _ => false,
        }
    }

    #[test]
    fn every_option_has_a_schema_that_allows_its_default() {
        for setting in SETTINGS {
            let schema = schema(setting.name).unwrap_or_else(|| panic!("no schema for `{}`", setting.name));
            if let Some(default) = setting.default {
                let default: Value = serde_json::from_str(default).unwrap();
                assert!(allows(&schema, &default), "the default of `{}` is not what its schema allows", setting.name);
            }
        }
        assert_eq!(SCHEMAS.len(), SETTINGS.len(), "a schema for an option that is not there");
        // And an example of each kind of value the schemas catch.
        assert!(!allows(&schema("logDone").unwrap(), &json!("yes")));
        assert!(!allows(&schema("captureTemplates").unwrap(), &json!([{ "key": "t", "taget": [] }])));
        assert!(allows(&schema("agenda.span").unwrap(), &json!(3)));
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
