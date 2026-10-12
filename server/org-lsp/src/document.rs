//! An open document: its text, its parse and the index for positions.

use lsp_types::TextDocumentContentChangeEvent;
use org_syntax::{Parse, SyntaxNode, TodoKeywords};

use crate::line_index::{Encoding, LineIndex};

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
    /// so the text and version still agree with the editor's.
    pub fn apply(
        &mut self,
        changes: Vec<TextDocumentContentChangeEvent>,
        version: i32,
        encoding: Encoding,
        todo: &TodoKeywords,
    ) {
        let mut text = self.text.clone();
        let mut index = self.index.clone();
        for change in changes {
            match change.range {
                Some(range) => {
                    let range = index.text_range(&text, range, encoding);
                    text.replace_range(usize::from(range.start())..usize::from(range.end()), &change.text);
                }
                None => text = change.text,
            }
            // Later changes in the same notification refer to the new text.
            index = LineIndex::new(&text);
        }
        let parse = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| org_syntax::parse(&text, todo)))
            .unwrap_or_else(|_| org_syntax::parse("", todo));
        *self = Document { version, text, index, parse };
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
