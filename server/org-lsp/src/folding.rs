//! Folding ranges: subtrees, drawers, blocks and multi-line list items.

use std::collections::HashMap;

use lsp_types::FoldingRange;
use org_syntax::{ast::Headline, SyntaxKind::*, SyntaxNode};

use crate::line_index::LineIndex;

/// `org-cycle-separator-lines`, Org's default.
pub const SEPARATOR_LINES: i64 = 2;

/// What folds, as Org's options of cycling say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Folding {
    /// `org-cycle-separator-lines`
    pub separator_lines: i64,
    /// `org-cycle-max-level`: deeper headlines are text of their parent's
    /// subtree, with no fold of their own.
    pub max_level: Option<usize>,
    /// `org-cycle-include-plain-lists` not nil: list items fold.
    pub plain_lists: bool,
}

impl Default for Folding {
    fn default() -> Self {
        Folding { separator_lines: SEPARATOR_LINES, max_level: None, plain_lists: true }
    }
}

/// How TAB works (`org-cycle`): what folds, and what TAB does before it
/// folds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cycle {
    pub folding: Folding,
    /// `org-cycle-include-plain-lists` `integrate`: showing a headline's
    /// children folds its lists to their top items.
    pub integrate_lists: bool,
    /// `org-cycle-global-at-bob`
    pub global_at_bob: bool,
    /// `org-cycle-level-after-item/entry-creation`
    pub level_after_creation: bool,
}

impl From<i64> for Folding {
    /// Org's defaults, with `separator_lines`.
    fn from(separator_lines: i64) -> Self {
        Folding { separator_lines, ..Folding::default() }
    }
}

/// Every foldable range in the document, as `(first line, last line)`.
///
/// A subtree folds up to the next headline, with the blank lines before
/// it, as in Emacs. `separator_lines` is `org-cycle-separator-lines`: when
/// at least that many blank lines follow the subtree, the last one stays
/// visible (all of them when it is negative); 0 hides them all. Blank
/// lines at the end of the file always stay visible. Other elements end
/// at their last line with text.
pub fn folding_ranges(
    root: &SyntaxNode,
    text: &str,
    index: &LineIndex,
    folding: impl Into<Folding>,
) -> Vec<FoldingRange> {
    folds(root, text, index, folding.into()).into_iter().map(|(_, range)| range).collect()
}

/// The folding ranges of the headlines' subtrees.
pub fn headline_folds(
    root: &SyntaxNode,
    text: &str,
    index: &LineIndex,
    folding: impl Into<Folding>,
) -> Vec<FoldingRange> {
    folds(root, text, index, folding.into())
        .into_iter()
        .filter(|(node, _)| node.kind() == HEADLINE)
        .map(|(_, range)| range)
        .collect()
}

/// The folding ranges, each with the element it folds.
fn folds(root: &SyntaxNode, text: &str, index: &LineIndex, folding: Folding) -> Vec<(SyntaxNode, FoldingRange)> {
    let separator_lines = folding.separator_lines;
    let mut ranges = Vec::new();
    let inline_tasks = org_core::inlinetask::min_stars_in(text);
    let outline_stars = org_core::inlinetask::outline_stars_in(text);
    let fold = |node: &SyntaxNode, start_line: usize, end_line: usize| {
        (
            node.clone(),
            FoldingRange {
                start_line: start_line as u32,
                start_character: None,
                end_line: end_line as u32,
                end_character: None,
                kind: None,
                collapsed_text: None,
            },
        )
    };
    for node in root.descendants() {
        let stars = || node.first_token().map_or(0, |stars| stars.text().trim_end().len());
        // An inline task has no subtree: it folds its text up to its `END`
        // line (`org-inlinetask-toggle-visibility`).
        if let Some(min) = inline_tasks.filter(|min| node.kind() == HEADLINE && stars() >= *min) {
            let start = usize::from(node.text_range().start());
            if let Some((first, end)) = org_core::inlinetask::fold_range(text, start, min) {
                let (start_line, end_line) = (index.line_of(first), index.line_of(end));
                if end_line > start_line {
                    ranges.push(fold(&node, start_line, end_line));
                }
            }
            continue;
        }
        let foldable = match node.kind() {
            HEADLINE => {
                folding.max_level.is_none_or(|max| stars() <= max)
                    && outline_stars.is_none_or(|deepest| stars() <= deepest)
            }
            LIST_ITEM => folding.plain_lists,
            kind => matches!(kind, DRAWER | PROPERTY_DRAWER | BLOCK | DYNAMIC_BLOCK | LATEX_ENVIRONMENT),
        };
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
            ranges.push(fold(&node, start_line, end_line));
        }
    }
    ranges
}

/// How a file shows when it is opened (`org-cycle-set-startup-visibility`):
/// the options, which its `#+STARTUP:` lines override.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Startup {
    /// `org-startup-folded`: `overview` (or `fold`), `content`,
    /// `show2levels` to `show5levels`, `showeverything` (or `nofold`,
    /// `showall`).
    pub folded: String,
    /// `org-cycle-hide-block-startup`
    pub hide_blocks: bool,
    /// `org-cycle-hide-drawer-startup`
    pub hide_drawers: bool,
    /// `org-cycle-open-archived-trees`
    pub open_archived: bool,
}

impl Default for Startup {
    fn default() -> Self {
        Startup { folded: "showeverything".to_string(), hide_blocks: false, hide_drawers: true, open_archived: false }
    }
}

/// The folds closed when the file is opened, innermost first, as
/// `org-cycle-set-startup-visibility` leaves the file: by
/// `org-startup-folded` (`#+STARTUP: overview`, `content`, `show2levels`…),
/// then each subtree's `VISIBILITY` property, archived subtrees, blocks
/// with `hideblocks`, and drawers. Folds hide a whole subtree, so where Org
/// shows a headline's children but not its text (`content`), the text
/// stays visible.
pub fn startup_folds(
    root: &SyntaxNode,
    text: &str,
    index: &LineIndex,
    folding: impl Into<Folding>,
    options: &Startup,
) -> Vec<FoldingRange> {
    let mut startup = options.clone();
    for word in org_core::text::startup_words(text) {
        match word.to_ascii_lowercase().as_str() {
            folded @ ("overview" | "fold" | "content" | "show2levels" | "show3levels" | "show4levels"
            | "show5levels" | "showall" | "nofold" | "showeverything") => startup.folded = folded.to_string(),
            "hideblocks" => startup.hide_blocks = true,
            "nohideblocks" | "showblocks" => startup.hide_blocks = false,
            _ => {}
        }
    }
    // `showeverything` leaves drawers, blocks and archived subtrees open
    // too; with `showall` (or `nofold`) every headline shows, and the rest
    // folds as Org folds it (`org-cycle-set-startup-visibility`).
    if startup.folded == "showeverything" {
        return Vec::new();
    }
    let levels = match startup.folded.as_str() {
        "overview" | "fold" | "t" => Some(1),
        "content" => Some(usize::MAX),
        "show2levels" => Some(2),
        "show3levels" => Some(3),
        "show4levels" => Some(4),
        "show5levels" => Some(5),
        _ => None,
    };
    let folds = folds(root, text, index, folding.into());
    let mut closed = vec![false; folds.len()];
    let places: HashMap<&SyntaxNode, usize> =
        folds.iter().enumerate().map(|(index, (node, _))| (node, index)).collect();
    let at = |node: &SyntaxNode| places.get(node).copied();
    let headlines: Vec<Headline> = root.descendants().filter_map(Headline::cast).collect();
    let leaf = |headline: &Headline| headline.children().next().is_none();
    // `org-cycle-content`: the headlines up to `levels` show, those below
    // a headline at that level hide with it.
    if let Some(levels) = levels {
        for headline in &headlines {
            if let Some(index) = at(headline.syntax()) {
                closed[index] = headline.level() >= levels || leaf(headline);
            }
        }
    }
    for (index, (node, _)) in folds.iter().enumerate() {
        if startup.hide_blocks && matches!(node.kind(), BLOCK | DYNAMIC_BLOCK) {
            closed[index] = true;
        }
    }
    // `org-cycle-set-visibility-according-to-property`, in the order of
    // the file.
    for headline in &headlines {
        let Some(state) = visibility(headline) else { continue };
        let below: Vec<Headline> = headline.syntax().descendants().skip(1).filter_map(Headline::cast).collect();
        let mut set = |headline: &Headline, value: bool| {
            if let Some(index) = at(headline.syntax()) {
                closed[index] = value;
            }
        };
        match state.as_str() {
            "folded" => {
                set(headline, true);
                below.iter().for_each(|child| set(child, true));
            }
            "children" => {
                set(headline, false);
                below.iter().for_each(|child| set(child, true));
            }
            "content" => {
                set(headline, false);
                for child in &below {
                    set(child, leaf(child));
                }
            }
            "all" | "showall" => {
                set(headline, false);
                below.iter().for_each(|child| set(child, false));
            }
            _ => {}
        }
    }
    for (index, (node, _)) in folds.iter().enumerate() {
        let archived = Headline::cast(node.clone())
            .is_some_and(|headline| headline.tags().iter().any(|tag| tag == org_core::archive::ARCHIVE_TAG));
        if archived && !startup.open_archived {
            closed[index] = true;
        }
        if startup.hide_drawers && matches!(node.kind(), DRAWER | PROPERTY_DRAWER) {
            closed[index] = true;
        }
    }
    let mut out: Vec<FoldingRange> =
        folds.into_iter().zip(closed).filter(|(_, closed)| *closed).map(|((_, range), _)| range).collect();
    // Inner folds first: closing one inside a closed fold would close the
    // fold around it.
    out.sort_by_key(|range| std::cmp::Reverse(range.start_line));
    out
}

/// The `VISIBILITY` property of a headline, from its property drawer.
fn visibility(headline: &Headline) -> Option<String> {
    let drawer = headline.section()?.children().find(|child| child.kind() == PROPERTY_DRAWER)?;
    drawer.text().to_string().lines().find_map(|line| {
        let rest = line.trim_start().strip_prefix(':')?;
        let (name, value) = rest.split_once(':')?;
        name.eq_ignore_ascii_case("VISIBILITY").then(|| value.trim().to_string())
    })
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

    fn startup(text: &str, folded: &str) -> Vec<(u32, u32)> {
        let parse = org_syntax::parse(text, &TodoKeywords::default());
        let index = LineIndex::new(text);
        let options = Startup { folded: folded.to_string(), ..Startup::default() };
        startup_folds(&parse.syntax(), text, &index, SEPARATOR_LINES, &options)
            .into_iter()
            .map(|range| (range.start_line, range.end_line))
            .collect()
    }

    #[test]
    fn startup_visibility() {
        let text = "\
* a
:PROPERTIES:
:ID: 1
:END:
text
** b
text
*** c
text
* d
text
";
        // Showing everything closes nothing, drawers neither.
        assert_eq!(startup(text, "showeverything"), []);
        // Every subtree folded, inner ones first; the drawer too.
        assert_eq!(startup(text, "overview"), [(9, 10), (7, 8), (5, 8), (1, 3), (0, 8)]);
        // The headlines show: only those without children fold.
        assert_eq!(startup(text, "content"), [(9, 10), (7, 8), (1, 3)]);
        // Down to the second level: `b` folds with `c` in it.
        assert_eq!(startup(text, "show2levels"), [(9, 10), (7, 8), (5, 8), (1, 3)]);
        // `#+STARTUP:` decides.
        assert_eq!(startup(&format!("#+STARTUP: overview\n{text}"), "showeverything").len(), 5);
        // `showall` shows every headline, the drawer still folded.
        assert_eq!(startup(&format!("#+STARTUP: showall\n{text}"), "overview"), [(2, 4)]);
    }

    /// `showall` and `nofold` show everything, but then fold what Org
    /// folds whatever the startup: subtrees by their `VISIBILITY`,
    /// archived subtrees, drawers, and blocks with `hideblocks`; as Emacs
    /// hides lines 3-7, 9, 12-13 and 15-16 (from 1) of this text.
    #[test]
    fn showall_still_folds_what_org_folds() {
        let text = "\
#+STARTUP: showall hideblocks
* A
:PROPERTIES:
:VISIBILITY: folded
:END:
a text
** A1
* B :ARCHIVE:
b text
* C
:LOGBOOK:
x
:END:
#+begin_src sh
echo
#+end_src
";
        let folds = startup(text, "showeverything");
        assert_eq!(folds, [(13, 15), (10, 12), (7, 8), (2, 4), (1, 6)]);
        let nofold = text.replacen("showall", "nofold", 1);
        assert_eq!(startup(&nofold, "showeverything"), folds);
        assert_eq!(startup(&text.replacen("showall", "showeverything", 1), "overview"), []);
    }

    #[test]
    fn startup_follows_visibility_archives_and_blocks() {
        let text = "\
* a
:PROPERTIES:
:VISIBILITY: children
:END:
** b
text
** c :ARCHIVE:
text
* d
:PROPERTIES:
:VISIBILITY: all
:END:
** e
#+begin_src sh
ls
#+end_src
";
        // `a` open, its children folded; `d` and `e` open, the drawers
        // folded; the archived `c` folded.
        assert_eq!(startup(text, "overview"), [(9, 11), (6, 7), (4, 5), (1, 3)]);
        // `hideblocks` folds the block too.
        assert_eq!(startup(&format!("#+STARTUP: hideblocks\n{text}"), "overview")[0], (14, 16));
        // With `content` the same: the properties decide for their subtrees.
        assert_eq!(startup(text, "content"), [(9, 11), (6, 7), (4, 5), (1, 3)]);
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
