//! Folding ranges: subtrees, drawers, blocks and multi-line list items.

use lsp_types::FoldingRange;
use org_syntax::{SyntaxKind::*, SyntaxNode};

use crate::line_index::LineIndex;

/// Every foldable range in the document, as `(first line, last line)`.
///
/// A range ends at the last line that has text, so the blank lines between
/// two subtrees stay visible when both are folded.
pub fn folding_ranges(root: &SyntaxNode, text: &str, index: &LineIndex) -> Vec<FoldingRange> {
    let mut ranges = Vec::new();
    for node in root.descendants() {
        let foldable = matches!(
            node.kind(),
            HEADLINE | DRAWER | PROPERTY_DRAWER | BLOCK | DYNAMIC_BLOCK | LATEX_ENVIRONMENT | LIST_ITEM
        );
        if !foldable {
            continue;
        }
        // Affiliated keywords sit above the element; the fold starts below them.
        let Some(first) = node.children_with_tokens().find(|child| !matches!(child.kind(), KEYWORD | WHITESPACE))
        else {
            continue;
        };
        let start = usize::from(first.text_range().start());
        let end = usize::from(node.text_range().end());
        let content_end = start + text[start..end].trim_end().len();
        let (start_line, end_line) = (index.line_of(start), index.line_of(content_end.saturating_sub(1).max(start)));
        if end_line > start_line {
            ranges.push(FoldingRange {
                start_line: start_line as u32,
                start_character: None,
                end_line: end_line as u32,
                end_character: None,
                kind: None,
                collapsed_text: None,
            });
        }
    }
    ranges
}

#[cfg(test)]
mod tests {
    use org_syntax::TodoKeywords;

    use super::*;

    fn ranges(text: &str) -> Vec<(u32, u32)> {
        let parse = org_syntax::parse(text, &TodoKeywords::default());
        let index = LineIndex::new(text);
        folding_ranges(&parse.syntax(), text, &index)
            .into_iter()
            .map(|range| (range.start_line, range.end_line))
            .collect()
    }

    #[test]
    fn subtrees_stop_before_trailing_blank_lines() {
        let text = "* a\nbody\n\n** b\nmore\n\n\n* c\n* d\ntext\n";
        assert_eq!(ranges(text), [(0, 4), (3, 4), (8, 9)]);
    }

    #[test]
    fn drawers_blocks_and_items() {
        let text = "\
* h
:PROPERTIES:
:ID: 1
:END:
#+NAME: x
#+begin_src sh
echo
#+end_src
- one
  two
- single
:LOG:
x
:END:
";
        assert_eq!(ranges(text), [(0, 13), (1, 3), (5, 7), (8, 9), (11, 13)]);
    }

    #[test]
    fn single_line_elements_do_not_fold() {
        assert_eq!(ranges("* a\n* b\n- item\n"), [(1, 2)]);
        assert_eq!(ranges(""), []);
    }
}
