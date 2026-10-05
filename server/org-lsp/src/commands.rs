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

pub struct Command {
    pub id: &'static str,
    pub title: &'static str,
    /// Whether the command changes the text of an Org file. Editors with a
    /// "repeat the last change" key can make these repeatable.
    pub edits: bool,
    /// Default keys as Emacs has them, in its notation (`C-c C-t`, `RET`).
    pub emacs: &'static [(Scope, &'static str)],
    /// Default keys for modal editors, in Vim's notation (`<CR>`). These
    /// are nvim-orgmode's defaults.
    pub vim: &'static [(Scope, &'static str)],
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

use Scope::{Agenda, Global, Org};

const fn command(id: &'static str, title: &'static str) -> Command {
    Command { id, title, edits: false, emacs: &[], vim: &[] }
}

pub const COMMANDS: &[Command] = &[
    Command {
        emacs: &[(Global, "C-c a")],
        vim: &[(Global, "<Leader>oa")],
        ..command(AGENDA_DISPATCH, "Agenda commands")
    },
    command(AGENDA_LIST, "Agenda for the current week or day"),
    command(AGENDA_TODOS, "List of all TODO entries"),
    Command { emacs: &[(Agenda, "f")], vim: &[(Agenda, "f")], ..command(AGENDA_LATER, "Agenda: go forward in time") },
    Command {
        emacs: &[(Agenda, "b")],
        vim: &[(Agenda, "b")],
        ..command(AGENDA_EARLIER, "Agenda: go backward in time")
    },
    Command { emacs: &[(Agenda, ".")], vim: &[(Agenda, ".")], ..command(AGENDA_TODAY, "Agenda: go to today") },
    Command {
        emacs: &[(Agenda, "d"), (Agenda, "v d")],
        vim: &[(Agenda, "vd")],
        ..command(AGENDA_DAY, "Agenda: day view")
    },
    Command {
        emacs: &[(Agenda, "w"), (Agenda, "v w")],
        vim: &[(Agenda, "vw")],
        ..command(AGENDA_WEEK, "Agenda: week view")
    },
    Command {
        emacs: &[(Agenda, "v t")],
        vim: &[(Agenda, "vt")],
        ..command(AGENDA_FORTNIGHT, "Agenda: fortnight view")
    },
    Command { emacs: &[(Agenda, "v m")], vim: &[(Agenda, "vm")], ..command(AGENDA_MONTH, "Agenda: month view") },
    Command { emacs: &[(Agenda, "v y")], vim: &[(Agenda, "vy")], ..command(AGENDA_YEAR, "Agenda: year view") },
    Command {
        emacs: &[(Agenda, "r"), (Agenda, "g")],
        vim: &[(Agenda, "r")],
        ..command(AGENDA_REDO, "Agenda: rebuild")
    },
    Command {
        emacs: &[(Agenda, "RET"), (Agenda, "TAB")],
        vim: &[(Agenda, "<CR>"), (Agenda, "<Tab>")],
        ..command(AGENDA_GOTO, "Agenda: go to the entry")
    },
    Command {
        edits: true,
        emacs: &[(Org, "C-c C-t"), (Org, "S-<right>"), (Agenda, "t")],
        vim: &[(Org, "cit"), (Agenda, "t")],
        ..command(TODO_NEXT, "Cycle TODO state forward")
    },
    Command {
        edits: true,
        emacs: &[(Org, "S-<left>")],
        vim: &[(Org, "ciT")],
        ..command(TODO_PREVIOUS, "Cycle TODO state backward")
    },
];

pub fn ids() -> Vec<String> {
    COMMANDS.iter().map(|command| command.id.to_string()).collect()
}

/// The table as JSON: a list of
/// `{ "command", "title", "edits", "keys": [{ "key", "in", "preset" }] }`.
pub fn manifest() -> Value {
    let commands: Vec<Value> = COMMANDS
        .iter()
        .map(|command| {
            let keys: Vec<Value> = [("emacs", command.emacs), ("vim", command.vim)]
                .into_iter()
                .flat_map(|(preset, keys)| {
                    keys.iter().map(move |(scope, key)| json!({ "key": key, "in": scope.name(), "preset": preset }))
                })
                .collect();
            json!({ "command": command.id, "title": command.title, "edits": command.edits, "keys": keys })
        })
        .collect();
    Value::Array(commands)
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
                .flat_map(|command| preset(command).iter().map(|(scope, key)| (scope.name(), *key)))
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
        let keys: Vec<(Scope, &str)> = COMMANDS.iter().flat_map(|command| command.emacs.iter().copied()).collect();
        for (scope, key) in &keys {
            let clash =
                keys.iter().any(|(other_scope, other)| other_scope == scope && other.starts_with(&format!("{key} ")));
            assert!(!clash, "`{key}` is a key and a prefix in {scope:?}");
        }
    }
}
