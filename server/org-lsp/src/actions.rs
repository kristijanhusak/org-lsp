//! Code actions: Org commands that edit the document.
//!
//! Each action has a stable kind (`org.todo.next`, …) so that editors can
//! bind a key to it: they request code actions with `only` set to that kind
//! and apply the single result. The kinds are listed in `docs/protocol.md`.

use lsp_types::{CodeAction, CodeActionKind, TextEdit};
use org_syntax::{ast, ast::Headline, SyntaxKind, SyntaxNode, TextRange, TextSize, TodoKeywords};

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
    pub edit: Edit,
}

/// The actions available with the cursor at `offset`.
pub fn actions(
    root: &SyntaxNode,
    todo: &TodoKeywords,
    offset: TextSize,
    only: Option<&[CodeActionKind]>,
) -> Vec<Action> {
    let Some(headline) = ast::headline_at(root, offset) else { return Vec::new() };
    let mut out = Vec::new();
    if todo.is_empty() {
        return out;
    }
    let current = headline.todo_keyword();
    let current = current.as_ref().map(|token| token.text());
    for (kind, direction, target) in
        [(TODO_NEXT, "forward", todo.next(current)), (TODO_PREVIOUS, "backward", todo.previous(current))]
    {
        if requested(kind, only) {
            out.push(Action {
                kind,
                title: format!("Cycle TODO state {direction} (→ {})", target.unwrap_or("none")),
                edit: set_todo_keyword(&headline, target),
            });
        }
    }
    out
}

/// The edit that gives `headline` the keyword `target`, or removes its keyword.
fn set_todo_keyword(headline: &Headline, target: Option<&str>) -> Edit {
    match (headline.todo_keyword(), target) {
        (Some(current), Some(target)) => Edit { range: current.text_range(), new_text: target.to_string() },
        (Some(current), None) => {
            // Remove the keyword together with the blanks that follow it.
            let end = match current.next_token() {
                Some(next) if next.kind() == SyntaxKind::WHITESPACE => next.text_range().end(),
                _ => current.text_range().end(),
            };
            Edit { range: TextRange::new(current.text_range().start(), end), new_text: String::new() }
        }
        (None, target) => {
            // Insert after the stars and their blank, before whatever follows.
            let stars = headline.stars();
            let at = match stars.next_token() {
                Some(next) if next.kind() == SyntaxKind::WHITESPACE => next.text_range().end(),
                _ => stars.text_range().end(),
            };
            let line_end = headline.line_range().end();
            let new_text = match target {
                Some(target) if at == line_end => target.to_string(),
                Some(target) => format!("{target} "),
                None => String::new(),
            };
            Edit { range: TextRange::empty(at), new_text }
        }
    }
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

impl Edit {
    pub fn to_lsp(&self, range: lsp_types::Range) -> TextEdit {
        TextEdit { range, new_text: self.new_text.clone() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Apply the action of `kind` with the cursor at `¦` and return the new text.
    fn apply(marked: &str, kind: &str) -> Option<String> {
        let offset = marked.find('¦').expect("cursor marker");
        let text = marked.replacen('¦', "", 1);
        let parse = org_syntax::parse(&text, &TodoKeywords::default());
        let only = [CodeActionKind::from(kind.to_string())];
        let found = actions(&parse.syntax(), parse.todo_keywords(), TextSize::new(offset as u32), Some(&only));
        assert!(found.len() <= 1);
        let action = found.into_iter().next()?;
        let mut text = text;
        text.replace_range(
            usize::from(action.edit.range.start())..usize::from(action.edit.range.end()),
            &action.edit.new_text,
        );
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
    fn keeps_priority_tags_and_spacing() {
        assert_eq!(next("** [#A] Title¦   :tag:\n"), "** TODO [#A] Title   :tag:\n");
        assert_eq!(next("** TODO   [#A] Title :tag:¦\n"), "** DONE   [#A] Title :tag:\n");
        assert_eq!(next("** DONE   [#A] Title :tag:¦\n"), "** [#A] Title :tag:\n");
        assert_eq!(next("* ¦:tag:\n"), "* TODO :tag:\n");
    }

    #[test]
    fn empty_and_keyword_only_headlines() {
        assert_eq!(next("* ¦"), "* TODO");
        assert_eq!(next("* ¦\n"), "* TODO\n");
        assert_eq!(next("* TODO¦\n"), "* DONE\n");
        assert_eq!(next("* DONE¦\n"), "* \n");
        // The result is still a headline with the expected keyword.
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
    fn only_filters_by_kind_prefix() {
        let kinds = |only: &[&str]| -> Vec<&'static str> {
            let only: Vec<CodeActionKind> = only.iter().map(|kind| CodeActionKind::from(kind.to_string())).collect();
            let parse = org_syntax::parse("* a\n", &TodoKeywords::default());
            actions(&parse.syntax(), parse.todo_keywords(), TextSize::new(0), Some(&only))
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
