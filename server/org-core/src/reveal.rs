//! `org-reveal` (`C-c C-r`, `org-fold-reveal`): the lines Org shows
//! around the cursor, as `org-fold-show-set-visibility` shows them. The
//! editor makes each of them visible and leaves its other folds as they
//! are; drawers and blocks stay folded, as Org folds them apart from the
//! outline.

use std::sync::LazyLock;

use regex::Regex;

use crate::structure::{level_at, line, line_start, next_heading, next_line, subtree_end};

/// How much `org-reveal` shows: without a prefix argument (`lineage`),
/// with `C-u` (`canonical`), with `C-u C-u` (the parent's whole tree).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detail {
    Lineage,
    Canonical,
    Tree,
}

/// The starts of the lines to show around `at`, in order.
pub fn reveal(text: &str, at: usize, detail: Detail) -> Vec<usize> {
    let here = line_start(text, at.min(text.len()));
    let heading = heading_at_or_above(text, here);
    let mut lines = Vec::new();
    if detail == Detail::Tree {
        // `org-up-heading-safe`, then `org-fold-show-subtree`.
        if let Some(parent) = heading.and_then(|heading| parent(text, heading)) {
            outline_lines(text, parent, subtree_end(text, parent), &mut lines);
        }
        return finish(lines);
    }
    if heading == Some(here) {
        lines.push(here);
    } else {
        // `org-fold-show-entry`, and the point exposed wherever it is.
        let start = heading.unwrap_or(0);
        outline_lines(text, start, next_heading_from(text, start, heading.is_some()), &mut lines);
        lines.push(here);
        if let Some(heading) = heading {
            match detail {
                Detail::Canonical => lines.extend(children(text, heading)),
                _ => {
                    let next = next_heading(text, here);
                    if next < text.len() {
                        lines.push(next);
                    }
                }
            }
        }
    }
    let Some(heading) = heading else { return finish(lines) };
    if detail == Detail::Lineage {
        lines.extend(siblings(text, heading));
    }
    let mut above = parent(text, heading);
    while let Some(ancestor) = above {
        lines.push(ancestor);
        if detail == Detail::Canonical {
            outline_lines(text, ancestor, next_heading(text, ancestor), &mut lines);
            lines.extend(children(text, ancestor));
        }
        above = parent(text, ancestor);
    }
    finish(lines)
}

fn finish(mut lines: Vec<usize>) -> Vec<usize> {
    lines.sort_unstable();
    lines.dedup();
    lines
}

/// The headline the line at `at` is or is under (`org-back-to-heading`).
fn heading_at_or_above(text: &str, at: usize) -> Option<usize> {
    let mut start = at;
    loop {
        if level_at(text, start).is_some() {
            return Some(start);
        }
        if start == 0 {
            return None;
        }
        start = line_start(text, start - 1);
    }
}

/// Where the entry that starts at `start` ends: the next headline, from
/// the headline itself or from the start of the text.
fn next_heading_from(text: &str, start: usize, is_heading: bool) -> usize {
    if is_heading || level_at(text, start).is_some() {
        next_heading(text, start)
    } else {
        let mut at = start;
        while at < text.len() && level_at(text, at).is_none() {
            at = next_line(text, at);
        }
        at
    }
}

/// `org-up-heading-safe`: the headline above of a lower level.
fn parent(text: &str, heading: usize) -> Option<usize> {
    let level = level_at(text, heading)?;
    let mut at = heading;
    while at > 0 {
        at = line_start(text, at - 1);
        if level_at(text, at).is_some_and(|found| found < level) {
            return Some(at);
        }
    }
    None
}

/// `org-fold-show-children`: the subheadings of `heading` not under
/// another of them.
fn children(text: &str, heading: usize) -> Vec<usize> {
    let end = subtree_end(text, heading);
    let mut out = Vec::new();
    let mut shallowest = usize::MAX;
    let mut at = next_heading(text, heading);
    while at < end {
        let level = level_at(text, at).unwrap_or(usize::MAX);
        if level <= shallowest {
            shallowest = level;
            out.push(at);
        }
        at = next_heading(text, at);
    }
    out
}

/// `org-fold-show-siblings`: the headlines of the same level under the
/// same parent.
fn siblings(text: &str, heading: usize) -> Vec<usize> {
    let level = level_at(text, heading).unwrap_or(1);
    let (start, end) = match parent(text, heading) {
        Some(parent) => (next_heading(text, parent), subtree_end(text, parent)),
        None => (heading_at_or_above(text, 0).unwrap_or_else(|| next_heading(text, 0)), text.len()),
    };
    let mut out = Vec::new();
    let mut at = start;
    while at < end {
        if level_at(text, at) == Some(level) {
            out.push(at);
        }
        at = next_heading(text, at);
    }
    out
}

/// The lines of `start..end` the outline hides, those inside drawers and
/// blocks left out: Org folds them apart.
fn outline_lines(text: &str, start: usize, end: usize, out: &mut Vec<usize>) {
    static DRAWER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[ \t]*:[\w-]+:[ \t]*$").unwrap());
    static DRAWER_END: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^[ \t]*:END:[ \t]*$").unwrap());
    static BLOCK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^[ \t]*#\+begin_(\S+)").unwrap());
    let mut inside: Option<Regex> = None;
    let mut at = start;
    while at < end && at < text.len() {
        let body = line(text, at);
        match &inside {
            Some(closing) => {
                if closing.is_match(body) {
                    inside = None;
                }
            }
            None => {
                out.push(at);
                if let Some(found) = BLOCK.captures(body) {
                    let name = regex::escape(&found[1]);
                    inside = Some(Regex::new(&format!(r"(?i)^[ \t]*#\+end_{name}\b")).unwrap());
                } else if DRAWER.is_match(body) && !DRAWER_END.is_match(body) {
                    inside = Some(DRAWER_END.clone());
                }
            }
        }
        at = next_line(text, at);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The lines shown, by number from 1.
    fn shown(text: &str, line: usize, detail: Detail) -> Vec<usize> {
        let at = text.split_inclusive('\n').take(line - 1).map(str::len).sum();
        reveal(text, at, detail).into_iter().map(|start| text[..start].matches('\n').count() + 1).collect()
    }

    const TEXT: &str = "\
* A
text of A
** B
:PROPERTIES:
:ID: b
:END:
text of B
*** C
** D
*** E
* F
";

    #[test]
    fn shows_the_lineage() {
        // On a headline: it, its siblings and its ancestors.
        assert_eq!(shown(TEXT, 8, Detail::Lineage), vec![1, 3, 8]);
        // In an entry: the entry (its drawer still folded), the next
        // headline, the siblings and the ancestors.
        assert_eq!(shown(TEXT, 7, Detail::Lineage), vec![1, 3, 4, 7, 8, 9]);
        // Inside a drawer: the point exposed.
        assert_eq!(shown(TEXT, 5, Detail::Lineage), vec![1, 3, 4, 5, 7, 8, 9]);
    }

    #[test]
    fn shows_the_canonical_context() {
        // In an entry: the entry and its children; each ancestor with its
        // entry and its children.
        assert_eq!(shown(TEXT, 7, Detail::Canonical), vec![1, 2, 3, 4, 7, 8, 9]);
    }

    #[test]
    fn shows_the_parent_tree() {
        assert_eq!(shown(TEXT, 10, Detail::Tree), vec![9, 10]);
        assert_eq!(shown(TEXT, 7, Detail::Tree), vec![1, 2, 3, 4, 7, 8, 9, 10]);
        // A top headline has no parent.
        assert_eq!(shown(TEXT, 11, Detail::Tree), Vec::<usize>::new());
    }

    #[test]
    fn shows_text_before_the_first_headline() {
        assert_eq!(shown("intro\n#+begin_src\nx\n#+end_src\n* A\n", 1, Detail::Lineage), vec![1, 2]);
    }
}
