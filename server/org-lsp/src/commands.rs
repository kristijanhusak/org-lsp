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
}

impl Scope {
    fn name(self) -> &'static str {
        match self {
            Scope::Global => "global",
            Scope::Org => "org",
            Scope::Agenda => "agenda",
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
pub const TODO_NEXT: &str = "org.todo.next";
pub const TODO_PREVIOUS: &str = "org.todo.previous";
pub const PRIORITY_UP: &str = "org.priority.up";
pub const PRIORITY_DOWN: &str = "org.priority.down";
pub const TIMESTAMP_UP: &str = "org.timestamp.up";
pub const TIMESTAMP_DOWN: &str = "org.timestamp.down";
pub const DATE_LATER: &str = "org.date.later";
pub const DATE_EARLIER: &str = "org.date.earlier";
pub const SHIFT_UP: &str = "org.shift.up";
pub const SHIFT_DOWN: &str = "org.shift.down";
pub const SHIFT_LEFT: &str = "org.shift.left";
pub const SHIFT_RIGHT: &str = "org.shift.right";

use Scope::{Agenda, Global, Org};

const fn command(id: &'static str, title: &'static str) -> Command {
    Command { id, title, edits: false, emacs: &[], vim: &[] }
}

const fn edit(id: &'static str, title: &'static str) -> Command {
    Command { id, title, edits: true, emacs: &[], vim: &[] }
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
    Command {
        emacs: &[key(Org, "C-c C-t"), key(Agenda, "t")],
        vim: &[key(Org, "cit"), key(Agenda, "t")],
        ..edit(TODO_NEXT, "Cycle TODO state forward")
    },
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
];

pub fn ids() -> Vec<String> {
    COMMANDS.iter().map(|command| command.id.to_string()).collect()
}

/// The table as JSON: a list of
/// `{ "command", "title", "edits", "keys": [{ "key", "in", "preset" }] }`.
/// A key that keeps its own meaning where the command does not apply has
/// `"native": true` as well.
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
            json!({ "command": command.id, "title": command.title, "edits": command.edits, "keys": keys })
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
        assert_eq!(ids.len(), COMMANDS.len());
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
