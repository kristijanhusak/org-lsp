//! Code actions: Org commands that edit the document.
//!
//! Each action has a stable kind (`org.todo.next`, …), which is also the
//! name of the command that does the same through
//! `workspace/executeCommand`. The kinds are listed in `docs/protocol.md`.

use lsp_types::{CodeAction, CodeActionKind};
use org_core::date::DateTime;
use org_core::edit::{self, Call, Command, Direction, Settings};
use org_core::priority::Action as Priority;
use org_core::todo::{self, Target};
use org_syntax::{Parse, TextRange, TextSize};

use crate::commands::{
    self, DATE_EARLIER, DATE_LATER, PRIORITY_DOWN, PRIORITY_UP, SHIFT_DOWN, SHIFT_LEFT, SHIFT_RIGHT, SHIFT_UP,
    TIMESTAMP_DOWN, TIMESTAMP_UP, TODO_NEXT, TODO_PREVIOUS,
};

/// Every kind the server can return, for the capabilities announcement.
/// The shifted cursor keys are not among them: what they do is offered
/// under its own name.
pub const KINDS: &[&str] =
    &[TODO_NEXT, TODO_PREVIOUS, PRIORITY_UP, PRIORITY_DOWN, TIMESTAMP_UP, TIMESTAMP_DOWN, DATE_LATER, DATE_EARLIER];

/// The command of this name as it runs in an Org file.
pub fn command(id: &str) -> Option<Command> {
    Some(match id {
        TODO_NEXT => Command::Todo(Target::Next),
        TODO_PREVIOUS => Command::Todo(Target::Previous),
        PRIORITY_UP => Command::Priority(Priority::Up),
        PRIORITY_DOWN => Command::Priority(Priority::Down),
        TIMESTAMP_UP => Command::Timestamp { forward: true },
        TIMESTAMP_DOWN => Command::Timestamp { forward: false },
        DATE_LATER => Command::Date { forward: true },
        DATE_EARLIER => Command::Date { forward: false },
        SHIFT_UP => Command::Shift(Direction::Up),
        SHIFT_DOWN => Command::Shift(Direction::Down),
        SHIFT_LEFT => Command::Shift(Direction::Left),
        SHIFT_RIGHT => Command::Shift(Direction::Right),
        _ => return None,
    })
}

/// Whether an action of `kind` was asked for. `only` filters hierarchically:
/// `org` and `org.todo` both select `org.todo.next`.
pub fn requested(kind: &str, only: Option<&[CodeActionKind]>) -> bool {
    let Some(only) = only else { return true };
    only.iter().any(|wanted| {
        let wanted = wanted.as_str();
        kind == wanted || (kind.starts_with(wanted) && kind.as_bytes().get(wanted.len()) == Some(&b'.'))
    })
}

/// A change to the text, in byte offsets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub range: TextRange,
    pub new_text: String,
}

/// An action before it is converted to LSP positions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub kind: &'static str,
    pub title: String,
    /// In document order, without overlap.
    pub edits: Vec<Edit>,
}

/// The edits that turn `old` into `new`: one for each run of changed
/// lines, narrowed to the characters that differ, so that the cursor and
/// marks around a change stay where they are.
pub fn edits_between(old: &str, new: &str) -> Vec<Edit> {
    let mut starts: Vec<usize> = vec![0];
    starts.extend(old.match_indices('\n').map(|(index, _)| index + 1));
    let offset_of = |line: usize| starts.get(line).copied().unwrap_or(old.len()).min(old.len());
    todo::edits(old, new)
        .into_iter()
        .map(|edit| {
            let (start, end) = (offset_of(edit.start), offset_of(edit.end));
            let (before, after) = (&old[start..end], edit.text.as_str());
            let prefix: usize =
                before.chars().zip(after.chars()).take_while(|(a, b)| a == b).map(|(a, _)| a.len_utf8()).sum();
            let suffix: usize = before[prefix..]
                .chars()
                .rev()
                .zip(after[prefix..].chars().rev())
                .take_while(|(a, b)| a == b)
                .map(|(a, _)| a.len_utf8())
                .sum();
            Edit {
                range: TextRange::new(TextSize::new((start + prefix) as u32), TextSize::new((end - suffix) as u32)),
                new_text: after[prefix..after.len() - suffix].to_string(),
            }
        })
        .collect()
}

/// The actions available with the cursor at byte `offset`: every command
/// that would change something there.
pub fn actions(
    text: &str,
    parse: &Parse,
    offset: usize,
    only: Option<&[CodeActionKind]>,
    settings: &Settings,
    now: DateTime,
) -> Vec<Action> {
    let call = Call { count: None, repeated: false, now };
    let keywords = parse.todo_keywords();
    let headline = org_syntax::ast::headline_at(&parse.syntax(), TextSize::new(offset as u32));
    let current = headline.as_ref().and_then(|headline| headline.todo_keyword());
    let current = current.as_ref().map(|token| token.text());
    KINDS
        .iter()
        .filter(|kind| requested(kind, only))
        .filter_map(|kind| {
            let title = match *kind {
                TODO_NEXT | TODO_PREVIOUS if keywords.is_empty() => return None,
                TODO_NEXT => format!("Cycle TODO state forward (→ {})", keywords.next(current).unwrap_or("none")),
                TODO_PREVIOUS => {
                    format!("Cycle TODO state backward (→ {})", keywords.previous(current).unwrap_or("none"))
                }
                other => commands::title(other).to_string(),
            };
            let outcome = edit::run(&command(kind)?, text, parse, offset, &call, settings).ok()?;
            let edits = edits_between(text, &outcome.text);
            (!edits.is_empty()).then_some(Action { kind, title, edits })
        })
        .collect()
}

impl Action {
    pub fn to_lsp(&self, edit: lsp_types::WorkspaceEdit) -> CodeAction {
        CodeAction {
            title: self.title.clone(),
            kind: Some(CodeActionKind::new(self.kind)),
            diagnostics: None,
            edit: Some(edit),
            command: None,
            is_preferred: None,
            disabled: None,
            data: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use org_core::date::Date;
    use org_syntax::{ast, TodoKeywords};

    use super::*;

    /// Apply the action of `kind` with the cursor at `¦` and return the new text.
    fn apply(marked: &str, kind: &str) -> Option<String> {
        let offset = marked.find('¦').expect("cursor marker");
        let text = marked.replacen('¦', "", 1);
        let parse = org_syntax::parse(&text, &TodoKeywords::default());
        let only = [CodeActionKind::from(kind.to_string())];
        let now = DateTime::new(Date::new(2026, 5, 18), 10, 30);
        let found = actions(&text, &parse, offset, Some(&only), &Settings::default(), now);
        assert!(found.len() <= 1);
        let action = found.into_iter().next()?;
        let mut text = text;
        for edit in action.edits.iter().rev() {
            text.replace_range(usize::from(edit.range.start())..usize::from(edit.range.end()), &edit.new_text);
        }
        Some(text)
    }

    fn next(marked: &str) -> String {
        apply(marked, TODO_NEXT).expect("an action")
    }

    fn previous(marked: &str) -> String {
        apply(marked, TODO_PREVIOUS).expect("an action")
    }

    #[test]
    fn cycles_forward() {
        assert_eq!(next("* Ti¦tle\n"), "* TODO Title\n");
        assert_eq!(next("* TODO Ti¦tle\n"), "* DONE Title\n");
        assert_eq!(next("* DONE Ti¦tle\n"), "* Title\n");
    }

    #[test]
    fn cycles_backward() {
        assert_eq!(previous("* Ti¦tle\n"), "* DONE Title\n");
        assert_eq!(previous("* DONE Ti¦tle\n"), "* TODO Title\n");
        assert_eq!(previous("* TODO Ti¦tle\n"), "* Title\n");
    }

    #[test]
    fn keeps_the_priority_and_realigns_tags() {
        // Tags end at column 77, as `org-tags-column` asks.
        assert_eq!(next("** [#A] Title¦   :tag:\n"), format!("** TODO [#A] Title{}:tag:\n", " ".repeat(54)));
        assert_eq!(next("** TODO   [#A] Title :tag:¦\n"), format!("** DONE [#A] Title{}:tag:\n", " ".repeat(54)));
    }

    #[test]
    fn empty_and_keyword_only_headlines() {
        // Org leaves a blank after the keyword.
        assert_eq!(next("* ¦"), "* TODO ");
        assert_eq!(next("* TODO¦\n"), "* DONE \n");
        assert_eq!(next("* DONE¦\n"), "* \n");
        let text = next("* ¦\n");
        let parse = org_syntax::parse(&text, &TodoKeywords::default());
        let headline = ast::headlines(&parse.syntax()).next().unwrap();
        assert_eq!(headline.todo_keyword().unwrap().text(), "TODO");
    }

    #[test]
    fn acts_on_the_entry_under_the_cursor() {
        let text = "* a\nbody\n** b\ninner ¦text\n* c\n";
        assert_eq!(next(text), "* a\nbody\n** TODO b\ninner text\n* c\n");
        assert_eq!(next("* a\nbo¦dy\n** b\n"), "* TODO a\nbody\n** b\n");
        assert_eq!(next("* a\n* b¦"), "* a\n* TODO b");
    }

    #[test]
    fn nothing_before_the_first_headline() {
        assert_eq!(apply("intro¦ text\n* a\n", TODO_NEXT), None);
        assert_eq!(apply("¦", TODO_NEXT), None);
    }

    #[test]
    fn uses_keywords_declared_in_the_document() {
        let text = "#+TODO: WAIT NEXT | GONE\n* ¦x\n";
        let step1 = next(text);
        assert_eq!(step1, "#+TODO: WAIT NEXT | GONE\n* WAIT x\n");
        assert_eq!(next(&step1.replace("x\n", "¦x\n")), "#+TODO: WAIT NEXT | GONE\n* NEXT x\n");
        assert_eq!(previous(text), "#+TODO: WAIT NEXT | GONE\n* GONE x\n");
    }

    #[test]
    fn a_repeating_entry_moves_on_and_its_parent_counts() {
        // As in Org, a note written at the very end gets no line break.
        let text = "* Project [0/1]\n** TODO Water plants¦\nSCHEDULED: <2026-05-16 Sat +1w>\n";
        assert_eq!(
            next(text),
            "\
* Project [0/1]
** TODO Water plants
SCHEDULED: <2026-05-23 Sat +1w>
:PROPERTIES:
:LAST_REPEAT: [2026-05-18 Mon 10:30]
:END:
- State \"DONE\"       from \"TODO\"       [2026-05-18 Mon 10:30]"
        );
        let done = next("* Project [0/2]\n** TODO a¦\n** TODO b\n");
        assert_eq!(done, "* Project [1/2]\n** DONE a\n** TODO b\n");
    }

    #[test]
    fn priorities_and_timestamps() {
        assert_eq!(apply("* TODO a\nbo¦dy\n", PRIORITY_UP).unwrap(), "* TODO [#B] a\nbody\n");
        assert_eq!(apply("* [#B] a¦\n", PRIORITY_DOWN).unwrap(), "* [#C] a\n");
        assert_eq!(apply("* a\n<2026-05-1¦8 Mon>\n", TIMESTAMP_UP).unwrap(), "* a\n<2026-05-19 Tue>\n");
        assert_eq!(apply("* a\n<2026-0¦5-18 Mon>\n", TIMESTAMP_DOWN).unwrap(), "* a\n<2026-04-18 Sat>\n");
        assert_eq!(apply("* a\n<2026-0¦5-18 Mon>\n", DATE_LATER).unwrap(), "* a\n<2026-05-19 Tue>\n");
        assert_eq!(apply("intro <2026-05-18 Mon>¦\n", DATE_EARLIER).unwrap(), "intro <2026-05-17 Sun>\n");
        // Away from a timestamp there is nothing to offer.
        assert_eq!(apply("* a\nbo¦dy <2026-05-18 Mon>\n", TIMESTAMP_UP), None);
        assert_eq!(apply("intro¦\n* a\n", PRIORITY_UP), None);
    }

    #[test]
    fn offers_what_applies_at_the_cursor() {
        let kinds = |marked: &str, only: &[&str]| -> Vec<&'static str> {
            let offset = marked.find('¦').expect("cursor marker");
            let text = marked.replacen('¦', "", 1);
            let only: Vec<CodeActionKind> = only.iter().map(|kind| CodeActionKind::from(kind.to_string())).collect();
            let parse = org_syntax::parse(&text, &TodoKeywords::default());
            let now = DateTime::new(Date::new(2026, 5, 18), 10, 30);
            let only = (!only.is_empty()).then_some(only.as_slice());
            actions(&text, &parse, offset, only, &Settings::default(), now)
                .into_iter()
                .map(|action| action.kind)
                .collect()
        };
        let on_entry = [TODO_NEXT, TODO_PREVIOUS, PRIORITY_UP, PRIORITY_DOWN];
        assert_eq!(kinds("* a¦\n", &[]), on_entry);
        assert_eq!(kinds("* a¦\n", &["org"]), on_entry);
        assert_eq!(kinds("* a¦\n", &["org.todo"]), [TODO_NEXT, TODO_PREVIOUS]);
        assert_eq!(kinds("* a¦\n", &["org.todo.next"]), [TODO_NEXT]);
        assert_eq!(kinds("* a¦\n", &["org.to"]), [] as [&str; 0]);
        assert_eq!(kinds("* a¦\n", &["quickfix", "refactor"]), [] as [&str; 0]);
        assert_eq!(
            kinds("* a\n<2026-05-1¦8 Mon>\n", &[]),
            [
                TODO_NEXT,
                TODO_PREVIOUS,
                PRIORITY_UP,
                PRIORITY_DOWN,
                TIMESTAMP_UP,
                TIMESTAMP_DOWN,
                DATE_LATER,
                DATE_EARLIER
            ]
        );
        assert_eq!(kinds("<2026-05-1¦8 Mon>\n", &["org.date"]), [DATE_LATER, DATE_EARLIER]);
        assert_eq!(kinds("intro¦\n", &[]), [] as [&str; 0]);
    }
}
