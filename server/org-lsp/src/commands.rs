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
    /// In Org files.
    Org,
    /// In the agenda.
    Agenda,
}

impl Scope {
    fn name(self) -> &'static str {
        match self {
            Scope::Org => "org",
            Scope::Agenda => "agenda",
        }
    }
}

pub struct Command {
    pub id: &'static str,
    pub title: &'static str,
    /// Default keys, written the way Emacs writes them (`C-c C-t`, `RET`).
    pub keys: &'static [(Scope, &'static str)],
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

use Scope::{Agenda, Org};

pub const COMMANDS: &[Command] = &[
    Command { id: AGENDA_DISPATCH, title: "Agenda commands", keys: &[(Org, "C-c a"), (Agenda, "C-c a")] },
    Command { id: AGENDA_LIST, title: "Agenda for the current week or day", keys: &[] },
    Command { id: AGENDA_TODOS, title: "List of all TODO entries", keys: &[] },
    Command { id: AGENDA_LATER, title: "Agenda: go forward in time", keys: &[(Agenda, "f")] },
    Command { id: AGENDA_EARLIER, title: "Agenda: go backward in time", keys: &[(Agenda, "b")] },
    Command { id: AGENDA_TODAY, title: "Agenda: go to today", keys: &[(Agenda, ".")] },
    Command { id: AGENDA_DAY, title: "Agenda: day view", keys: &[(Agenda, "d"), (Agenda, "v d")] },
    Command { id: AGENDA_WEEK, title: "Agenda: week view", keys: &[(Agenda, "w"), (Agenda, "v w")] },
    Command { id: AGENDA_FORTNIGHT, title: "Agenda: fortnight view", keys: &[(Agenda, "v t")] },
    Command { id: AGENDA_MONTH, title: "Agenda: month view", keys: &[(Agenda, "v m")] },
    Command { id: AGENDA_YEAR, title: "Agenda: year view", keys: &[(Agenda, "v y")] },
    Command { id: AGENDA_REDO, title: "Agenda: rebuild", keys: &[(Agenda, "r"), (Agenda, "g")] },
    Command { id: AGENDA_GOTO, title: "Agenda: go to the entry", keys: &[(Agenda, "RET"), (Agenda, "TAB")] },
    Command {
        id: TODO_NEXT,
        title: "Cycle TODO state forward",
        keys: &[(Org, "C-c C-t"), (Org, "S-<right>"), (Agenda, "t")],
    },
    Command { id: TODO_PREVIOUS, title: "Cycle TODO state backward", keys: &[(Org, "S-<left>")] },
];

pub fn ids() -> Vec<String> {
    COMMANDS.iter().map(|command| command.id.to_string()).collect()
}

/// The table as JSON: `{ "commands": [{ "command", "title", "keys": [{ "key", "in" }] }] }`.
pub fn manifest() -> Value {
    let commands: Vec<Value> = COMMANDS
        .iter()
        .map(|command| {
            let keys: Vec<Value> =
                command.keys.iter().map(|(scope, key)| json!({ "key": key, "in": scope.name() })).collect();
            json!({ "command": command.id, "title": command.title, "keys": keys })
        })
        .collect();
    json!({ "commands": commands })
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
        let mut keys: Vec<(&str, &str)> =
            COMMANDS.iter().flat_map(|command| command.keys.iter().map(|(scope, key)| (scope.name(), *key))).collect();
        let total = keys.len();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), total, "a key is bound twice in one scope");
    }
}
