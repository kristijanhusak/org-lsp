//! Folding ranges: subtrees, drawers, blocks and multi-line list items.

use lsp_types::FoldingRange;
use org_syntax::{SyntaxKind::*, SyntaxNode};

use crate::line_index::LineIndex;

/// `org-cycle-separator-lines`, Org's default.
pub const SEPARATOR_LINES: i64 = 2;

/// Every foldable range in the document, as `(first line, last line)`.
///
/// A subtree folds up to the next headline, with the blank lines before
/// it, as in Emacs. `separator_lines` is `org-cycle-separator-lines`: when
/// at least that many blank lines follow the subtree, the last one stays
/// visible (all of them when it is negative); 0 hides them all. Blank
/// lines at the end of the file always stay visible. Other elements end
/// at their last line with text.
pub fn folding_ranges(root: &SyntaxNode, text: &str, index: &LineIndex, separator_lines: i64) -> Vec<FoldingRange> {
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
        let last_text_line = index.line_of(content_end.saturating_sub(1).max(start));
        let end_line = if node.kind() == HEADLINE && end < text.len() {
            // `org-cycle-show-empty-lines`: the subtree ends where the next
            // headline starts, after the blank lines.
            let last_line = index.line_of(end - 1);
            let blank = last_line - last_text_line;
            match separator_lines {
                0 => last_line,
                n if blank < n.unsigned_abs() as usize => last_line,
                n if n > 0 => last_line - 1,
                _ => last_text_line,
            }
        } else {
            last_text_line
        };
        let start_line = index.line_of(start);
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

    fn ranges_with(text: &str, separator_lines: i64) -> Vec<(u32, u32)> {
        let parse = org_syntax::parse(text, &TodoKeywords::default());
        let index = LineIndex::new(text);
        folding_ranges(&parse.syntax(), text, &index, separator_lines)
            .into_iter()
            .map(|range| (range.start_line, range.end_line))
            .collect()
    }

    fn ranges(text: &str) -> Vec<(u32, u32)> {
        ranges_with(text, SEPARATOR_LINES)
    }

    #[test]
    fn subtrees_fold_the_blank_lines_before_the_next_headline() {
        // One blank line is folded away; of two or more, the last stays.
        let text = "* a\nbody\n\n** b\nmore\n\n\n* c\n* d\ntext\n";
        assert_eq!(ranges(text), [(0, 5), (3, 5), (8, 9)]);
        // Blank lines with spaces count as blank.
        assert_eq!(ranges("* a\nbody\n  \n\t\n\n* b\n"), [(0, 3)]);
        // An entry with nothing but a blank line folds it.
        assert_eq!(ranges("* a\n\n* b\n"), [(0, 1)]);
        // At the end of the file, blank lines always stay visible.
        assert_eq!(ranges("* a\nbody\n\n\n"), [(0, 1)]);
        assert_eq!(ranges("* a\n** b\nbody\n\n"), [(0, 2), (1, 2)]);
    }

    #[test]
    fn follows_org_cycle_separator_lines() {
        let one = "* a\nbody\n\n* b\n";
        let three = "* a\nbody\n\n\n\n* b\n";
        // 0: never a blank line between folded subtrees.
        assert_eq!(ranges_with(three, 0), [(0, 4)]);
        // 1: one blank line is enough to keep one.
        assert_eq!(ranges_with(one, 1), [(0, 1)]);
        assert_eq!(ranges_with(three, 1), [(0, 3)]);
        // 3: three are needed.
        assert_eq!(ranges_with(three, 3), [(0, 3)]);
        assert_eq!(ranges_with("* a\nbody\n\n\n* b\n", 3), [(0, 3)]);
        // Negative: with enough of them, all stay visible.
        assert_eq!(ranges_with(three, -2), [(0, 1)]);
        assert_eq!(ranges_with(one, -2), [(0, 2)]);
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
