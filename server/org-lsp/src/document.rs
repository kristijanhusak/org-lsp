//! An open document: its text, its parse and the index for positions.

use lsp_types::TextDocumentContentChangeEvent;
use org_syntax::{Parse, SyntaxNode, TodoKeywords};

use crate::line_index::{Encoding, LineIndex};

/// A change to a text: the bytes from `start` to `end` became `length`
/// bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Change {
    pub start: usize,
    pub end: usize,
    pub length: usize,
}

impl Change {
    /// The change from `old` to `new`, a whole text an editor sent: from
    /// where they start to differ to where they end the same.
    pub fn between(old: &str, new: &str) -> Change {
        let mut prefix = old.bytes().zip(new.bytes()).take_while(|(a, b)| a == b).count();
        while !old.is_char_boundary(prefix) || !new.is_char_boundary(prefix) {
            prefix -= 1;
        }
        let most = old.len().min(new.len()) - prefix;
        let mut suffix = old.bytes().rev().zip(new.bytes().rev()).take(most).take_while(|(a, b)| a == b).count();
        while !old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix) {
            suffix -= 1;
        }
        Change { start: prefix, end: old.len() - suffix, length: new.len() - prefix - suffix }
    }

    /// The changes from `old` to `new`, a whole text an editor sent: each
    /// run of lines that differ, the last first, so that each is in the
    /// text the ones before left. Places between them keep their entries
    /// (a whole text replaced at once would move them all to its start).
    pub fn all_between(old: &str, new: &str) -> Vec<Change> {
        let mut changes: Vec<Change> = crate::actions::edits_between(old, new)
            .into_iter()
            .map(|edit| Change {
                start: usize::from(edit.range.start()),
                end: usize::from(edit.range.end()),
                length: edit.new_text.len(),
            })
            .collect();
        changes.reverse();
        changes
    }

    /// Where `offset` is after the change: a place in the bytes that
    /// changed goes to where they started.
    pub fn moved(&self, offset: usize) -> usize {
        if self.end <= offset {
            offset - (self.end - self.start) + self.length
        } else if self.start < offset {
            self.start
        } else {
            offset
        }
    }
}

#[derive(Debug)]
pub struct Document {
    pub version: i32,
    pub text: String,
    pub index: LineIndex,
    pub parse: Parse,
}

impl Document {
    pub fn new(text: String, version: i32, todo: &TodoKeywords) -> Document {
        let index = LineIndex::new(&text);
        let parse = org_syntax::parse(&text, todo);
        Document { version, text, index, parse }
    }

    pub fn syntax(&self) -> SyntaxNode {
        self.parse.syntax()
    }

    /// Apply the changes of one `didChange` notification, in order, then reparse.
    ///
    /// The text, its index, version and parse change together: a parser
    /// that fails on the new text leaves an empty parse, not the old one,
    /// so the text and version still agree with the editor's. A change of
    /// a range is parsed again where it is (`Parse::edit`: the lines of the
    /// headline it is in), the whole text when one replaces it.
    ///
    /// What changed, in order, each in the text the ones before left: the
    /// bytes from `start` to `end` became `length` bytes.
    pub fn apply(
        &mut self,
        changes: Vec<TextDocumentContentChangeEvent>,
        version: i32,
        encoding: Encoding,
        todo: &TodoKeywords,
    ) -> Vec<Change> {
        let mut text = self.text.clone();
        let mut index = self.index.clone();
        let mut changed = Vec::new();
        // The parse of `text` as far as the changes went, while each one
        // could be parsed where it is.
        let mut parse = Some(self.parse.clone());
        for change in changes {
            match change.range {
                Some(range) => {
                    let range = index.text_range(&text, range, encoding);
                    let (start, end) = (usize::from(range.start()), usize::from(range.end()));
                    // What changed of it: a client may send whole lines
                    // (vim-lsp), and the places in them stay.
                    let narrowed = Change::between(&text[start..end], &change.text);
                    let (start, end) = (start + narrowed.start, start + narrowed.end);
                    changed.push(Change { start, end, length: narrowed.length });
                    let before = text.clone();
                    text.replace_range(start..end, &change.text[narrowed.start..narrowed.start + narrowed.length]);
                    parse = parse.and_then(|parse| {
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            parse.edit(&before, start, end, &text, todo)
                        }))
                        .ok()
                    });
                }
                None => {
                    // The runs of lines that changed, each parsed where it
                    // is, the last first: the text before each is as it
                    // will be. Many at once are parsed as a whole.
                    let mut runs = crate::actions::edits_between(&text, &change.text);
                    runs.reverse();
                    if runs.len() > 8 {
                        parse = None;
                    }
                    for run in &runs {
                        let (start, end) = (usize::from(run.range.start()), usize::from(run.range.end()));
                        changed.push(Change { start, end, length: run.new_text.len() });
                        let before = text.clone();
                        text.replace_range(start..end, &run.new_text);
                        parse = parse.and_then(|parse| {
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                parse.edit(&before, start, end, &text, todo)
                            }))
                            .ok()
                        });
                    }
                    if text != change.text {
                        // The runs did not give the editor's text: it is
                        // parsed as a whole.
                        text = change.text;
                        parse = None;
                    }
                }
            }
            // Later changes in the same notification refer to the new text.
            index = LineIndex::new(&text);
        }
        let parse = parse.unwrap_or_else(|| {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| org_syntax::parse(&text, todo)))
                .unwrap_or_else(|_| org_syntax::parse("", todo))
        });
        *self = Document { version, text, index, parse };
        changed
    }
}

#[cfg(test)]
mod tests {
    use lsp_types::{Position, Range};

    use super::*;

    fn change(range: Option<(u32, u32, u32, u32)>, text: &str) -> TextDocumentContentChangeEvent {
        TextDocumentContentChangeEvent {
            range: range.map(|(l1, c1, l2, c2)| Range::new(Position::new(l1, c1), Position::new(l2, c2))),
            range_length: None,
            text: text.to_string(),
        }
    }

    #[test]
    fn narrows_a_whole_text_to_what_changed() {
        assert_eq!(
            Change::between("* a\nlong body\n* b\n", "* a\nbody\n* b\n"),
            Change { start: 4, end: 9, length: 0 }
        );
        let change = Change::between("* é\n* b\n", "* è\n* b\n");
        assert_eq!(change, Change { start: 2, end: 4, length: 2 });
        // The second headline stays where it is, the first too.
        assert_eq!(change.moved(5), 5);
        assert_eq!(change.moved(0), 0);
        assert_eq!(Change::between("same", "same"), Change { start: 4, end: 4, length: 0 });
    }

    #[test]
    fn narrows_a_change_of_whole_lines_to_what_changed() {
        let todo = TodoKeywords::default();
        let mut document = Document::new("* a\nSCHEDULED: <2026-05-16 Sat>\n".to_string(), 1, &todo);
        let changes = document.apply(
            vec![change(Some((1, 0, 2, 0)), "SCHEDULED: <2026-05-18 Mon>\n")],
            2,
            Encoding::Utf16,
            &todo,
        );
        // The timestamp's bracket stays where it is.
        assert_eq!(changes, vec![Change { start: 25, end: 30, length: 5 }]);
        assert_eq!(changes[0].moved(15), 15);
    }

    #[test]
    fn splits_a_whole_text_into_the_lines_that_changed() {
        let old = "* A\n* B\n* C\n";
        let new = "* A\nS\n* B\nS\n* C\nS\n";
        let changes = Change::all_between(old, new);
        let moved = |offset: usize| changes.iter().fold(offset, |offset, change| change.moved(offset));
        // Each headline, where it is now.
        assert_eq!([moved(0), moved(4), moved(8)], [0, 6, 12]);
    }

    /// Changes of ranges and whole texts alike leave the parse the whole
    /// text gives, whether they were parsed where they are or not.
    #[test]
    fn a_change_parses_as_the_whole_text_does() {
        let todo = TodoKeywords::default();
        let start = "#+TITLE: T\n* TODO A\nSCHEDULED: <2026-05-18 Mon>\nbody *bold*\n** B\n- item\n* C\n#+begin_src sh\necho\n#+end_src\n";
        let mut document = Document::new(start.to_string(), 1, &todo);
        let same = |document: &Document| {
            assert_eq!(
                document.syntax().green().into_owned(),
                org_syntax::parse(&document.text, &todo).syntax().green().into_owned(),
                "{:?}",
                document.text
            );
        };
        type Range = Option<(u32, u32, u32, u32)>;
        let steps: Vec<(Range, &str)> = vec![
            (Some((1, 7, 1, 8)), "a"),
            (Some((3, 0, 3, 0)), "more "),
            (Some((5, 2, 5, 6)), "[ ] item"),
            (Some((4, 4, 4, 4)), "\n* New"),
            (Some((7, 0, 7, 0)), "#+TODO: WAIT | OK\n"),
            (None, "#+TITLE: T\n* WAIT A\nbody\n** B\n- item\n* C\n#+begin_src sh\necho\n#+end_src\n"),
            (None, "#+TITLE: T\n* WAIT Ab\nbody\n** B\n- item\n* C\n#+begin_src sh\necho \"x\"\n#+end_src\n"),
            (None, "* only\n"),
        ];
        for (version, (range, text)) in steps.into_iter().enumerate() {
            document.apply(vec![change(range, text)], version as i32 + 2, Encoding::Utf16, &todo);
            same(&document);
        }
    }

    #[test]
    fn incremental_changes_apply_in_order() {
        let todo = TodoKeywords::default();
        let mut document = Document::new("* one\ntwo\n".to_string(), 1, &todo);
        document.apply(
            vec![
                change(Some((0, 2, 0, 5)), "TODO 😀"),
                change(Some((0, 9, 1, 0)), " "),
                change(Some((9, 9, 9, 9)), "!"),
            ],
            2,
            Encoding::Utf16,
            &todo,
        );
        assert_eq!(document.text, "* TODO 😀 two\n!");
        assert_eq!(document.version, 2);
        assert_eq!(document.syntax().text().to_string(), document.text);
    }

    #[test]
    fn full_change_replaces_the_text() {
        let todo = TodoKeywords::default();
        let mut document = Document::new("old".to_string(), 1, &todo);
        document.apply(vec![change(None, "* new\n")], 5, Encoding::Utf8, &todo);
        assert_eq!(document.text, "* new\n");
        assert_eq!(document.index.line_count(), 2);
    }
}
