//! Code actions: Org commands that edit the document.
//!
//! Each action has a stable kind (`org.todo.next`, …), which is also the
//! name of the command that does the same through
//! `workspace/executeCommand`. The kinds are listed in `docs/protocol.md`.

use lsp_types::{CodeAction, CodeActionKind};
use org_core::date::DateTime;
use org_core::todo::{self, Settings, Target};
use org_syntax::{ast, SyntaxNode, TextRange, TextSize, TodoKeywords};

pub const TODO_NEXT: &str = "org.todo.next";
pub const TODO_PREVIOUS: &str = "org.todo.previous";

/// Every kind the server can return, for the capabilities announcement.
pub const KINDS: &[&str] = &[TODO_NEXT, TODO_PREVIOUS];

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

/// The edits that change the TODO state of the entry containing `offset`,
/// with everything `org-todo` does around it: logging, repeaters, tags,
/// statistics cookies.
pub fn todo_edits(
    text: &str,
    keywords: &TodoKeywords,
    offset: usize,
    target: &Target,
    settings: &Settings,
    now: DateTime,
) -> Result<Vec<Edit>, String> {
    let changed = todo::change_state(text, keywords, offset, target, settings, now)?;
    // Line edits to byte ranges of the old text.
    let mut starts: Vec<usize> = vec![0];
    starts.extend(text.match_indices('\n').map(|(index, _)| index + 1));
    let offset_of = |line: usize| starts.get(line).copied().unwrap_or(text.len()).min(text.len());
    let edits = todo::edits(text, &changed)
        .into_iter()
        .map(|edit| {
            // Narrow the edit to what really changes, so that the cursor
            // and marks around it stay where they are.
            let (start, end) = (offset_of(edit.start), offset_of(edit.end));
            let (old, new) = (&text[start..end], edit.text.as_str());
            let prefix: usize =
                old.chars().zip(new.chars()).take_while(|(a, b)| a == b).map(|(a, _)| a.len_utf8()).sum();
            let suffix: usize = old[prefix..]
                .chars()
                .rev()
                .zip(new[prefix..].chars().rev())
                .take_while(|(a, b)| a == b)
                .map(|(a, _)| a.len_utf8())
                .sum();
            Edit {
                range: TextRange::new(TextSize::new((start + prefix) as u32), TextSize::new((end - suffix) as u32)),
                new_text: new[prefix..new.len() - suffix].to_string(),
            }
        })
        .collect();
    Ok(edits)
}

/// The actions available with the cursor at `offset`.
pub fn actions(
    root: &SyntaxNode,
    text: &str,
    todo: &TodoKeywords,
    offset: TextSize,
    only: Option<&[CodeActionKind]>,
    settings: &Settings,
    now: DateTime,
) -> Vec<Action> {
    let Some(headline) = ast::headline_at(root, offset) else { return Vec::new() };
    let mut out = Vec::new();
    if todo.is_empty() {
        return out;
    }
    let current = headline.todo_keyword();
    let current = current.as_ref().map(|token| token.text());
    for (kind, direction, target, to) in [
        (TODO_NEXT, "forward", Target::Next, todo.next(current)),
        (TODO_PREVIOUS, "backward", Target::Previous, todo.previous(current)),
    ] {
        if !requested(kind, only) {
            continue;
        }
        if let Ok(edits) = todo_edits(text, todo, offset.into(), &target, settings, now) {
            out.push(Action {
                kind,
                title: format!("Cycle TODO state {direction} (→ {})", to.unwrap_or("none")),
                edits,
            });
        }
    }
    out
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

    use super::*;

    /// Apply the action of `kind` with the cursor at `¦` and return the new text.
    fn apply(marked: &str, kind: &str) -> Option<String> {
        let offset = marked.find('¦').expect("cursor marker");
        let text = marked.replacen('¦', "", 1);
        let parse = org_syntax::parse(&text, &TodoKeywords::default());
        let only = [CodeActionKind::from(kind.to_string())];
        let now = DateTime::new(Date::new(2026, 5, 18), 10, 30);
        let found = actions(
            &parse.syntax(),
            &text,
            parse.todo_keywords(),
            TextSize::new(offset as u32),
            Some(&only),
            &Settings::default(),
            now,
        );
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
    fn only_filters_by_kind_prefix() {
        let kinds = |only: &[&str]| -> Vec<&'static str> {
            let only: Vec<CodeActionKind> = only.iter().map(|kind| CodeActionKind::from(kind.to_string())).collect();
            let parse = org_syntax::parse("* a\n", &TodoKeywords::default());
            let now = DateTime::new(Date::new(2026, 5, 18), 10, 30);
            actions(
                &parse.syntax(),
                "* a\n",
                parse.todo_keywords(),
                TextSize::new(0),
                Some(&only),
                &Settings::default(),
                now,
            )
            .into_iter()
            .map(|action| action.kind)
            .collect()
        };
        assert_eq!(kinds(&["org"]), [TODO_NEXT, TODO_PREVIOUS]);
        assert_eq!(kinds(&["org.todo"]), [TODO_NEXT, TODO_PREVIOUS]);
        assert_eq!(kinds(&["org.todo.next"]), [TODO_NEXT]);
        assert_eq!(kinds(&["org.to"]), [] as [&str; 0]);
        assert_eq!(kinds(&["quickfix", "refactor"]), [] as [&str; 0]);
    }
}
