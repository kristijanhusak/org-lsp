//! The commands the server offers, with their titles and default keys.
//!
//! This table is the single source for every editor integration. The
//! server announces it in the `initialize` result (see `manifest`), and
//! `org-lsp manifest` prints it, so an editor needs no list of its own:
//! it runs a command with `workspace/executeCommand` and passes where the
//! cursor is.

use serde_json::{json, Value};

/// Where a key applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Everywhere in the editor.
    Global,
    /// In Org files.
    Org,
    /// In the agenda.
    Agenda,
    /// In the date prompt.
    Calendar,
    /// In a capture buffer.
    Capture,
}

impl Scope {
    fn name(self) -> &'static str {
        match self {
            Scope::Global => "global",
            Scope::Org => "org",
            Scope::Agenda => "agenda",
            Scope::Calendar => "calendar",
            Scope::Capture => "capture",
        }
    }
}

/// A default key for a command.
pub struct Key {
    pub scope: Scope,
    pub key: &'static str,
    /// The key has a meaning of its own in the editor, which should apply
    /// wherever the command does not: the server then refuses the command
    /// with `notApplicable` in the error data. `<C-a>` still increments a
    /// number that is not part of a timestamp.
    pub native: bool,
}

const fn key(scope: Scope, key: &'static str) -> Key {
    Key { scope, key, native: false }
}

const fn native(scope: Scope, key: &'static str) -> Key {
    Key { scope, key, native: true }
}

pub struct Command {
    pub id: &'static str,
    pub title: &'static str,
    /// Whether the command changes the text of an Org file. Editors with a
    /// "repeat the last change" key can make these repeatable.
    pub edits: bool,
    /// Default keys as Emacs has them, in its notation (`C-c C-t`, `RET`).
    pub emacs: &'static [Key],
    /// Default keys for modal editors, in Vim's notation (`<CR>`). These
    /// are nvim-orgmode's defaults.
    pub vim: &'static [Key],
    /// What the command asks the user for before it can run, as the
    /// prompt the editor shows.
    pub input: Option<Input>,
}

/// Input a command asks for.
pub struct Input {
    /// What kind of answer: `date` is read as Org's date prompt reads it.
    pub kind: &'static str,
    pub prompt: &'static str,
}

pub const AGENDA_DISPATCH: &str = "org.agenda.dispatch";
pub const AGENDA_LIST: &str = "org.agenda.list";
pub const AGENDA_TODOS: &str = "org.agenda.todos";
pub const AGENDA_LATER: &str = "org.agenda.later";
pub const AGENDA_EARLIER: &str = "org.agenda.earlier";
pub const AGENDA_TODAY: &str = "org.agenda.today";
pub const AGENDA_DAY: &str = "org.agenda.day";
pub const AGENDA_WEEK: &str = "org.agenda.week";
pub const AGENDA_FORTNIGHT: &str = "org.agenda.fortnight";
pub const AGENDA_MONTH: &str = "org.agenda.month";
pub const AGENDA_YEAR: &str = "org.agenda.year";
pub const AGENDA_REDO: &str = "org.agenda.redo";
pub const AGENDA_GOTO: &str = "org.agenda.goto";
pub const AGENDA_SAVE: &str = "org.agenda.save";
pub const AGENDA_GOTO_DATE: &str = "org.agenda.goto.date";
pub const TODO: &str = "org.todo";
pub const TODO_NEXT: &str = "org.todo.next";
pub const TODO_PREVIOUS: &str = "org.todo.previous";
pub const PRIORITY_UP: &str = "org.priority.up";
pub const PRIORITY_DOWN: &str = "org.priority.down";
pub const PRIORITY: &str = "org.priority";
pub const REFILE: &str = "org.refile";
pub const CAPTURE: &str = "org.capture";
pub const CAPTURE_FINISH: &str = "org.capture.finish";
pub const CAPTURE_ABORT: &str = "org.capture.abort";
pub const CAPTURE_REFILE: &str = "org.capture.refile";
pub const TIMESTAMP_UP: &str = "org.timestamp.up";
pub const TIMESTAMP_DOWN: &str = "org.timestamp.down";
pub const DATE_LATER: &str = "org.date.later";
pub const DATE_EARLIER: &str = "org.date.earlier";
pub const SHIFT_UP: &str = "org.shift.up";
pub const SHIFT_DOWN: &str = "org.shift.down";
pub const SHIFT_LEFT: &str = "org.shift.left";
pub const SHIFT_RIGHT: &str = "org.shift.right";
pub const SCHEDULE: &str = "org.schedule";
pub const DEADLINE: &str = "org.deadline";
pub const TIMESTAMP_INSERT: &str = "org.timestamp.insert";
pub const TIMESTAMP_INSERT_INACTIVE: &str = "org.timestamp.insert.inactive";
pub const TIMESTAMP_CHANGE: &str = "org.timestamp.change";
/// What an answer to a date prompt means, for an editor that shows it
/// while the answer is typed. Not a command of its own for users.
pub const DATE_PREVIEW: &str = "org.date.preview";
pub const CALENDAR_DAY_LATER: &str = "org.calendar.day.later";
pub const CALENDAR_DAY_EARLIER: &str = "org.calendar.day.earlier";
pub const CALENDAR_WEEK_LATER: &str = "org.calendar.week.later";
pub const CALENDAR_WEEK_EARLIER: &str = "org.calendar.week.earlier";
pub const CALENDAR_MONTH_LATER: &str = "org.calendar.month.later";
pub const CALENDAR_MONTH_EARLIER: &str = "org.calendar.month.earlier";
pub const CALENDAR_YEAR_LATER: &str = "org.calendar.year.later";
pub const CALENDAR_YEAR_EARLIER: &str = "org.calendar.year.earlier";
pub const CALENDAR_SCROLL_LATER: &str = "org.calendar.scroll.later";
pub const CALENDAR_SCROLL_EARLIER: &str = "org.calendar.scroll.earlier";
pub const CALENDAR_TODAY: &str = "org.calendar.today";
pub const CALENDAR_SELECT: &str = "org.calendar.select";
pub const CALENDAR_TYPE: &str = "org.calendar.type";
pub const CALENDAR_CANCEL: &str = "org.calendar.cancel";

use Scope::{Agenda, Calendar, Capture, Global, Org};

const fn command(id: &'static str, title: &'static str) -> Command {
    Command { id, title, edits: false, emacs: &[], vim: &[], input: None }
}

const fn edit(id: &'static str, title: &'static str) -> Command {
    Command { id, title, edits: true, emacs: &[], vim: &[], input: None }
}

const fn ask_date(id: &'static str, title: &'static str, prompt: &'static str) -> Command {
    Command { input: Some(Input { kind: "date", prompt }), ..edit(id, title) }
}

pub const COMMANDS: &[Command] = &[
    Command {
        emacs: &[key(Global, "C-c a")],
        vim: &[key(Global, "<Leader>oa")],
        ..command(AGENDA_DISPATCH, "Agenda commands")
    },
    command(AGENDA_LIST, "Agenda for the current week or day"),
    command(AGENDA_TODOS, "List of all TODO entries"),
    Command {
        emacs: &[key(Agenda, "f")],
        vim: &[key(Agenda, "f")],
        ..command(AGENDA_LATER, "Agenda: go forward in time")
    },
    Command {
        emacs: &[key(Agenda, "b")],
        vim: &[key(Agenda, "b")],
        ..command(AGENDA_EARLIER, "Agenda: go backward in time")
    },
    Command { emacs: &[key(Agenda, ".")], vim: &[key(Agenda, ".")], ..command(AGENDA_TODAY, "Agenda: go to today") },
    Command {
        emacs: &[key(Agenda, "j")],
        vim: &[key(Agenda, "J")],
        input: Some(Input { kind: "date", prompt: "Date" }),
        ..command(AGENDA_GOTO_DATE, "Agenda: go to a date")
    },
    Command {
        emacs: &[key(Agenda, "d"), key(Agenda, "v d")],
        vim: &[key(Agenda, "vd")],
        ..command(AGENDA_DAY, "Agenda: day view")
    },
    Command {
        emacs: &[key(Agenda, "w"), key(Agenda, "v w")],
        vim: &[key(Agenda, "vw")],
        ..command(AGENDA_WEEK, "Agenda: week view")
    },
    Command {
        emacs: &[key(Agenda, "v t")],
        vim: &[key(Agenda, "vt")],
        ..command(AGENDA_FORTNIGHT, "Agenda: fortnight view")
    },
    Command { emacs: &[key(Agenda, "v m")], vim: &[key(Agenda, "vm")], ..command(AGENDA_MONTH, "Agenda: month view") },
    Command { emacs: &[key(Agenda, "v y")], vim: &[key(Agenda, "vy")], ..command(AGENDA_YEAR, "Agenda: year view") },
    Command {
        emacs: &[key(Agenda, "r"), key(Agenda, "g")],
        vim: &[key(Agenda, "r")],
        ..command(AGENDA_REDO, "Agenda: rebuild")
    },
    Command {
        emacs: &[key(Agenda, "RET"), key(Agenda, "TAB")],
        vim: &[key(Agenda, "<CR>"), key(Agenda, "<Tab>")],
        ..command(AGENDA_GOTO, "Agenda: go to the entry")
    },
    Command { emacs: &[key(Agenda, "s"), key(Agenda, "C-x C-s")], ..command(AGENDA_SAVE, "Save all Org files") },
    // `org-todo`: the state picked by key when the keywords have keys,
    // and else the next one.
    Command {
        emacs: &[key(Org, "C-c C-t"), key(Agenda, "t")],
        vim: &[key(Org, "cit"), key(Agenda, "t")],
        ..edit(TODO, "Change the TODO state")
    },
    Command { ..edit(TODO_NEXT, "Cycle TODO state forward") },
    Command { vim: &[key(Org, "ciT")], ..edit(TODO_PREVIOUS, "Cycle TODO state backward") },
    Command {
        emacs: &[key(Agenda, "+"), key(Agenda, "S-<up>")],
        vim: &[key(Org, "ciR"), key(Agenda, "+")],
        ..edit(PRIORITY_UP, "Raise the priority")
    },
    Command {
        emacs: &[key(Agenda, "-"), key(Agenda, "S-<down>")],
        vim: &[key(Org, "cir"), key(Agenda, "-")],
        ..edit(PRIORITY_DOWN, "Lower the priority")
    },
    Command {
        emacs: &[key(Org, "C-c ,"), key(Agenda, ",")],
        vim: &[key(Org, "<Leader>o,"), key(Agenda, "<Leader>o,")],
        input: Some(Input { kind: "priority", prompt: "Priority" }),
        ..edit(PRIORITY, "Set the priority")
    },
    Command {
        emacs: &[key(Org, "C-c C-w"), key(Agenda, "C-c C-w")],
        vim: &[key(Org, "<Leader>or"), key(Agenda, "<Leader>or")],
        input: Some(Input { kind: "choice", prompt: "Refile to" }),
        ..edit(REFILE, "Refile the entry")
    },
    Command {
        emacs: &[key(Global, "C-c c")],
        vim: &[key(Global, "<Leader>oc")],
        input: Some(Input { kind: "key", prompt: "Select a capture template" }),
        ..command(CAPTURE, "Capture")
    },
    Command {
        emacs: &[key(Capture, "C-c C-c")],
        vim: &[key(Capture, "<C-c>")],
        ..command(CAPTURE_FINISH, "Capture: finish")
    },
    Command {
        emacs: &[key(Capture, "C-c C-k")],
        vim: &[key(Capture, "<Leader>ok")],
        ..command(CAPTURE_ABORT, "Capture: abort")
    },
    Command {
        emacs: &[key(Capture, "C-c C-w")],
        vim: &[key(Capture, "<Leader>or")],
        input: Some(Input { kind: "choice", prompt: "Refile to" }),
        ..command(CAPTURE_REFILE, "Capture: refile the entry")
    },
    Command { vim: &[native(Org, "<C-a>")], ..edit(TIMESTAMP_UP, "Increase the part of the timestamp at the cursor") },
    Command {
        vim: &[native(Org, "<C-x>")],
        ..edit(TIMESTAMP_DOWN, "Decrease the part of the timestamp at the cursor")
    },
    Command {
        emacs: &[key(Agenda, "S-<right>")],
        vim: &[native(Org, "<S-Up>"), key(Agenda, "<S-Right>")],
        ..edit(DATE_LATER, "Move the date one day later")
    },
    Command {
        emacs: &[key(Agenda, "S-<left>")],
        vim: &[native(Org, "<S-Down>"), key(Agenda, "<S-Left>")],
        ..edit(DATE_EARLIER, "Move the date one day earlier")
    },
    // Org's shifted cursor keys, which act on what the cursor is on: a
    // timestamp, or else the headline.
    Command { emacs: &[native(Org, "S-<up>")], ..edit(SHIFT_UP, "Increase the timestamp part or raise the priority") },
    Command {
        emacs: &[native(Org, "S-<down>")],
        ..edit(SHIFT_DOWN, "Decrease the timestamp part or lower the priority")
    },
    Command {
        emacs: &[native(Org, "S-<left>")],
        ..edit(SHIFT_LEFT, "Move the date a day earlier or cycle the TODO state backward")
    },
    Command {
        emacs: &[native(Org, "S-<right>")],
        ..edit(SHIFT_RIGHT, "Move the date a day later or cycle the TODO state forward")
    },
    Command {
        emacs: &[key(Org, "C-c C-s"), key(Agenda, "C-c C-s")],
        vim: &[key(Org, "<Leader>ois"), key(Agenda, "<Leader>ois")],
        ..ask_date(SCHEDULE, "Schedule the entry", "Schedule")
    },
    Command {
        emacs: &[key(Org, "C-c C-d"), key(Agenda, "C-c C-d")],
        vim: &[key(Org, "<Leader>oid"), key(Agenda, "<Leader>oid")],
        ..ask_date(DEADLINE, "Set the deadline of the entry", "Deadline")
    },
    Command {
        emacs: &[key(Org, "C-c .")],
        vim: &[key(Org, "<Leader>oi.")],
        ..ask_date(TIMESTAMP_INSERT, "Insert or change a timestamp", "Timestamp")
    },
    Command {
        emacs: &[key(Org, "C-c !")],
        vim: &[key(Org, "<Leader>oi!")],
        ..ask_date(TIMESTAMP_INSERT_INACTIVE, "Insert or change an inactive timestamp", "Inactive timestamp")
    },
    // nvim-orgmode's `cid`; in Emacs, `C-c .` on a timestamp changes it.
    Command {
        vim: &[key(Org, "cid")],
        ..ask_date(TIMESTAMP_CHANGE, "Change the timestamp at the cursor", "Timestamp")
    },
    // The date prompt: the keys of `org-read-date-minibuffer-local-map`
    // and of the calendar Emacs shows with it.
    Command {
        emacs: &[key(Calendar, "S-<right>")],
        vim: &[key(Calendar, "l"), key(Calendar, "<S-Right>")],
        ..command(CALENDAR_DAY_LATER, "Date prompt: one day later")
    },
    Command {
        emacs: &[key(Calendar, "S-<left>")],
        vim: &[key(Calendar, "h"), key(Calendar, "<S-Left>")],
        ..command(CALENDAR_DAY_EARLIER, "Date prompt: one day earlier")
    },
    Command {
        emacs: &[key(Calendar, "S-<down>")],
        vim: &[key(Calendar, "j"), key(Calendar, "<S-Down>")],
        ..command(CALENDAR_WEEK_LATER, "Date prompt: one week later")
    },
    Command {
        emacs: &[key(Calendar, "S-<up>")],
        vim: &[key(Calendar, "k"), key(Calendar, "<S-Up>")],
        ..command(CALENDAR_WEEK_EARLIER, "Date prompt: one week earlier")
    },
    Command {
        emacs: &[key(Calendar, "M-S-<right>")],
        vim: &[key(Calendar, "<M-S-Right>")],
        ..command(CALENDAR_MONTH_LATER, "Date prompt: one month later")
    },
    Command {
        emacs: &[key(Calendar, "M-S-<left>")],
        vim: &[key(Calendar, "<M-S-Left>")],
        ..command(CALENDAR_MONTH_EARLIER, "Date prompt: one month earlier")
    },
    Command {
        emacs: &[key(Calendar, "M-S-<down>")],
        vim: &[key(Calendar, "<M-S-Down>")],
        ..command(CALENDAR_YEAR_LATER, "Date prompt: one year later")
    },
    Command {
        emacs: &[key(Calendar, "M-S-<up>")],
        vim: &[key(Calendar, "<M-S-Up>")],
        ..command(CALENDAR_YEAR_EARLIER, "Date prompt: one year earlier")
    },
    Command {
        emacs: &[key(Calendar, ">")],
        vim: &[key(Calendar, ">")],
        ..command(CALENDAR_SCROLL_LATER, "Date prompt: show the next months")
    },
    Command {
        emacs: &[key(Calendar, "<")],
        vim: &[key(Calendar, "<")],
        ..command(CALENDAR_SCROLL_EARLIER, "Date prompt: show the previous months")
    },
    Command {
        emacs: &[key(Calendar, ".")],
        vim: &[key(Calendar, ".")],
        ..command(CALENDAR_TODAY, "Date prompt: go to today")
    },
    Command {
        emacs: &[key(Calendar, "RET")],
        vim: &[key(Calendar, "<CR>")],
        ..command(CALENDAR_SELECT, "Date prompt: take the day at the cursor")
    },
    Command {
        emacs: &[key(Calendar, "i")],
        vim: &[key(Calendar, "i")],
        input: Some(Input { kind: "date", prompt: "Date+time" }),
        ..command(CALENDAR_TYPE, "Date prompt: type the date")
    },
    Command {
        emacs: &[key(Calendar, "q"), key(Calendar, "C-g")],
        vim: &[key(Calendar, "q"), key(Calendar, "<Esc>")],
        ..command(CALENDAR_CANCEL, "Date prompt: cancel")
    },
];

/// Every command the server runs: the table's, and those editors call
/// on their own.
pub fn ids() -> Vec<String> {
    COMMANDS.iter().map(|command| command.id).chain([DATE_PREVIEW]).map(str::to_string).collect()
}

/// The input the command `id` asks for, if any.
pub fn input(id: &str) -> Option<&'static Input> {
    COMMANDS.iter().find(|command| command.id == id)?.input.as_ref()
}

/// The table as JSON: a list of
/// `{ "command", "title", "edits", "keys": [{ "key", "in", "preset" }] }`.
/// A key that keeps its own meaning where the command does not apply has
/// `"native": true` as well. A command that asks for something has
/// `"input": { "kind", "prompt" }`.
pub fn manifest() -> Value {
    let commands: Vec<Value> = COMMANDS
        .iter()
        .map(|command| {
            let keys: Vec<Value> = [("emacs", command.emacs), ("vim", command.vim)]
                .into_iter()
                .flat_map(|(preset, keys)| {
                    keys.iter().map(move |key| {
                        let mut entry = json!({ "key": key.key, "in": key.scope.name(), "preset": preset });
                        if key.native {
                            entry["native"] = json!(true);
                        }
                        entry
                    })
                })
                .collect();
            let mut entry =
                json!({ "command": command.id, "title": command.title, "edits": command.edits, "keys": keys });
            if let Some(input) = &command.input {
                entry["input"] = json!({ "kind": input.kind, "prompt": input.prompt });
            }
            entry
        })
        .collect();
    Value::Array(commands)
}

pub fn title(id: &str) -> &'static str {
    COMMANDS.iter().find(|command| command.id == id).map_or("", |command| command.title)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_keys_do_not_clash() {
        let mut ids = ids();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), COMMANDS.len() + 1);
        for preset in [|command: &Command| command.emacs, |command: &Command| command.vim] {
            let mut keys: Vec<(&str, &str)> = COMMANDS
                .iter()
                .flat_map(|command| preset(command).iter().map(|key| (key.scope.name(), key.key)))
                .collect();
            let total = keys.len();
            keys.sort();
            keys.dedup();
            assert_eq!(keys.len(), total, "a key is bound twice in one scope");
        }
    }

    #[test]
    fn a_key_is_not_also_the_start_of_a_longer_one() {
        // `v` then `d` in the agenda only works if `v` alone does nothing.
        let keys: Vec<(Scope, &str)> =
            COMMANDS.iter().flat_map(|command| command.emacs.iter().map(|key| (key.scope, key.key))).collect();
        for (scope, key) in &keys {
            let clash =
                keys.iter().any(|(other_scope, other)| other_scope == scope && other.starts_with(&format!("{key} ")));
            assert!(!clash, "`{key}` is a key and a prefix in {scope:?}");
        }
    }
}
