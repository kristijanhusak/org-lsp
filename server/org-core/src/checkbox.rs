//! Checkboxes: `org-toggle-checkbox` (`C-c C-c` on a list item), the box
//! of a parent item that follows its children (`org-list-struct-fix-box`),
//! and the statistics cookies that count checkboxes, `[1/3]` and `[33%]`
//! (`org-update-checkbox-count`). Checked against Emacs in `tests/todo`.

use org_syntax::{SyntaxKind::*, SyntaxNode, TodoKeywords};

use crate::text::tag_group;
use crate::todo::{headline_level, tags_padding};
use crate::Refusal;

/// A list item and its checkbox.
#[derive(Debug, Clone)]
struct Item {
    /// The start of its line.
    begin: usize,
    indent: usize,
    /// Where `[ ]` is, and the character inside.
    checkbox: Option<(usize, u8)>,
    parent: Option<usize>,
    /// The outermost list it belongs to (`org-list-struct`).
    list: usize,
    /// Whether it is the first item of its list, where the list starts.
    first: bool,
}

struct Lists {
    items: Vec<Item>,
    /// The range of each outermost list.
    ranges: Vec<(usize, usize)>,
    /// Where org-element stops reading each outermost list: the end of
    /// what holds it.
    limits: Vec<usize>,
}

fn span(node: &SyntaxNode) -> (usize, usize) {
    let range = node.text_range();
    (usize::from(range.start()), usize::from(range.end()))
}

fn line_start(text: &str, at: usize) -> usize {
    text[..at.min(text.len())].rfind('\n').map_or(0, |newline| newline + 1)
}

fn line_end(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |newline| at + newline)
}

fn next_line(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |newline| at + newline + 1)
}

impl Lists {
    fn new(text: &str, root: &SyntaxNode) -> Lists {
        let mut lists = Lists { items: Vec::new(), ranges: Vec::new(), limits: Vec::new() };
        lists.collect(text, root, None, None);
        lists
    }

    fn collect(&mut self, text: &str, node: &SyntaxNode, parent: Option<usize>, list: Option<usize>) {
        for child in node.children() {
            match child.kind() {
                LIST => {
                    let list = list.unwrap_or_else(|| {
                        self.ranges.push(span(&child));
                        self.limits.push(contents_end(text, node));
                        self.ranges.len() - 1
                    });
                    for (number, item) in child.children().filter(|item| item.kind() == LIST_ITEM).enumerate() {
                        let begin = line_start(text, span(&item).0);
                        let indent = text[begin..].bytes().take_while(|b| matches!(b, b' ' | b'\t')).count();
                        let checkbox = item
                            .children_with_tokens()
                            .filter_map(|element| element.into_token())
                            .find(|token| token.kind() == CHECKBOX)
                            .map(|token| {
                                let at = usize::from(token.text_range().start());
                                (at, token.text().as_bytes()[1])
                            });
                        self.items.push(Item { begin, indent, checkbox, parent, list, first: number == 0 });
                        let index = self.items.len() - 1;
                        self.collect(text, &item, Some(index), Some(list));
                    }
                }
                _ => self.collect(text, &child, parent, list),
            }
        }
    }

    fn children(&self, item: usize) -> impl Iterator<Item = usize> + '_ {
        (0..self.items.len()).filter(move |index| self.items[*index].parent == Some(item))
    }
}

/// The end of the contents of `node`: before the line closing a drawer
/// or a block, the end of a section.
fn contents_end(text: &str, node: &SyntaxNode) -> usize {
    let (_, end) = span(node);
    if matches!(node.kind(), DRAWER | DYNAMIC_BLOCK | BLOCK) {
        let trimmed = text[..end].trim_end_matches([' ', '\r', '\t', '\n']).len();
        line_start(text, trimmed)
    } else {
        end
    }
}

fn parse(text: &str, keywords: &TodoKeywords) -> SyntaxNode {
    org_syntax::parse(text, keywords).syntax()
}

/// The end of a headline's planning line, property drawer and the drawers
/// after them (`org-end-of-meta-data` with `full`).
fn end_of_meta_data(text: &str, heading: usize, limit: usize) -> usize {
    let mut at = next_line(text, heading);
    let line = |at: usize| text[at..line_end(text, at)].trim();
    let upper = |at: usize| line(at).to_ascii_uppercase();
    if at < limit && ["SCHEDULED:", "DEADLINE:", "CLOSED:"].iter().any(|word| upper(at).starts_with(word)) {
        at = next_line(text, at);
    }
    loop {
        let current = line(at);
        let is_drawer = current.len() > 2
            && current.starts_with(':')
            && current.ends_with(':')
            && current[1..current.len() - 1].bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
        if at >= limit || !is_drawer {
            break;
        }
        let mut end = next_line(text, at);
        while end < limit && !upper(end).eq(":END:") {
            end = next_line(text, end);
        }
        if end >= limit {
            break;
        }
        at = next_line(text, end);
    }
    at
}

/// `org-toggle-checkbox` at byte `offset`: on a list item, its checkbox;
/// on a headline, those of all the items below it, as the first one goes.
/// Parent boxes follow their children, and the cookies of the section are
/// counted again (with the `checkbox` rule of `org-list-automatic-rules`).
/// An item whose box changes has its bullet written again, with the
/// spaces `org-list-bullet-string` puts after it.
pub fn toggle(
    text: &str,
    keywords: &TodoKeywords,
    offset: usize,
    tags_column: i64,
    settings: &crate::list::Settings,
) -> Result<String, Refusal> {
    let root = parse(text, keywords);
    let lists = Lists::new(text, &root);
    let line = line_start(text, offset);
    let line_text = &text[line..line_end(text, line)];
    let (up, down) = if headline_level(line_text).is_some() {
        let limit = next_heading(text, line).unwrap_or(text.len());
        let meta = end_of_meta_data(text, line, limit);
        let first = lists.items.iter().map(|item| item.begin).filter(|begin| *begin >= meta && *begin < limit).min();
        (first.ok_or_else(|| Refusal::Failed("No item in subtree".to_string()))?, limit)
    } else if lists.items.iter().any(|item| item.begin == line) {
        (line, line_end(text, line))
    } else {
        return Err(Refusal::NotHere("Not at an item or heading, and no active region".to_string()));
    };
    let first = lists.items.iter().find(|item| item.begin == up).expect("an item");
    let reference = if first.checkbox.map(|(_, state)| state) == Some(b'X') { b' ' } else { b'X' };
    let mut states: Vec<Option<u8>> = lists.items.iter().map(|item| item.checkbox.map(|(_, state)| state)).collect();
    let mut touched = Vec::new();
    for (index, item) in lists.items.iter().enumerate() {
        if item.begin >= up && item.begin <= down {
            if states[index].is_some() {
                states[index] = Some(reference);
            }
            if !touched.contains(&item.list) {
                touched.push(item.list);
            }
        }
    }
    // `org-list-struct-fix-box`: a parent's box from its children's,
    // deepest first.
    for list in touched {
        let mut parents: Vec<usize> = (0..lists.items.len())
            .filter(|index| lists.items[*index].list == list && states[*index].is_some())
            .filter(|index| lists.children(*index).next().is_some())
            .collect();
        parents.sort_by(|a, b| lists.items[*b].indent.cmp(&lists.items[*a].indent));
        for parent in parents {
            let boxes: Vec<u8> = lists.children(parent).filter_map(|child| states[child]).collect();
            let has = |state: u8| boxes.contains(&state);
            states[parent] = if (has(b' ') && has(b'X')) || has(b'-') {
                Some(b'-')
            } else if has(b'X') {
                Some(b'X')
            } else if has(b' ') {
                Some(b' ')
            } else {
                states[parent]
            };
        }
    }
    let mut out = text.to_string();
    for (index, item) in lists.items.iter().enumerate().rev() {
        let (Some((at, old)), Some(state)) = (item.checkbox, states[index]) else { continue };
        if old == state {
            continue;
        }
        out.replace_range(at + 1..at + 2, &char::from(state).to_string());
        let bullet = item.begin + item.indent;
        let bullet_end = bullet + out[bullet..].find([' ', '\t']).unwrap_or(0);
        let spaces = out[bullet_end..].len() - out[bullet_end..].trim_start_matches([' ', '\t']).len();
        let new = crate::list::bullet_string(&out[bullet..bullet_end + spaces], settings);
        out.replace_range(bullet..bullet_end + spaces, &new);
    }
    if !settings.checkbox_rule {
        return Ok(out);
    }
    Ok(update_counts(&out, keywords, offset, tags_column, settings.checkbox_hierarchical))
}

/// The start of the next headline after the line at `at`.
fn next_heading(text: &str, at: usize) -> Option<usize> {
    let mut line = next_line(text, at);
    while line < text.len() {
        if headline_level(&text[line..line_end(text, line)]).is_some() {
            return Some(line);
        }
        line = next_line(text, line);
    }
    None
}

/// The `COOKIE_DATA` property of the entry the text at `at` is in.
fn cookie_data(text: &str, at: usize) -> String {
    let mut line = line_start(text, at);
    while headline_level(&text[line..line_end(text, line)]).is_none() {
        if line == 0 {
            return String::new();
        }
        line = line_start(text, line - 1);
    }
    let limit = next_heading(text, line).unwrap_or(text.len());
    let mut at = next_line(text, line);
    while at < limit {
        let current = text[at..line_end(text, at)].trim();
        if current.len() > 13 && current.get(..13).is_some_and(|key| key.eq_ignore_ascii_case(":COOKIE_DATA:")) {
            return current[13..].trim().to_string();
        }
        if current.eq_ignore_ascii_case(":END:") {
            break;
        }
        at = next_line(text, at);
    }
    String::new()
}

fn has_word(text: &str, word: &str) -> bool {
    text.split(|c: char| !c.is_alphanumeric()).any(|part| part == word)
}

/// Whether byte `offset` is on a statistics cookie, or right after one, as
/// `org-element-context` has it.
pub fn on_statistics_cookie(root: &SyntaxNode, offset: usize) -> bool {
    root.descendants().filter(|node| node.kind() == STATISTICS_COOKIE).any(|node| {
        let (start, end) = span(&node);
        (start..=end).contains(&offset)
    })
}

/// `C-c C-c` on the line of an item (`org-ctrl-c-ctrl-c`): its checkbox
/// toggled if it has one, the list's indentation, bullets, numbers and
/// checkboxes put right, and the cookies counted again. `ordered` is the
/// entry's `ORDERED` property. The text, the cursor after it, and what to
/// tell the user.
pub fn ctrl_c_ctrl_c(
    text: &str,
    keywords: &TodoKeywords,
    offset: usize,
    tags_column: i64,
    ordered: bool,
    settings: &crate::list::Settings,
) -> Result<(String, usize, Option<String>), Refusal> {
    let root = parse(text, keywords);
    // On a statistics cookie, the cookies are counted again
    // (`org-update-statistics-cookies`), and that is all.
    let on_cookie = on_statistics_cookie(&root, offset);
    let lists = Lists::new(text, &root);
    let line = line_start(text, offset);
    if on_cookie && lists.items.iter().any(|item| item.begin == line) {
        let (edited, cursor) =
            update_counts_keeping(text, keywords, offset, tags_column, offset, settings.checkbox_hierarchical);
        return Ok((edited, cursor, None));
    }
    let item = lists
        .items
        .iter()
        .find(|item| item.begin == line)
        .ok_or_else(|| Refusal::NotHere("Not at an item".to_string()))?;
    let (begin, _) = lists.ranges[item.list];
    let limit = lists.limits[item.list];
    // At the start of a list, Org is on the list rather than its first item.
    let (edited, cursor, message) = if item.first && offset == line {
        crate::list::ctrl_c_ctrl_c_list(text, offset, begin, limit, settings)?
    } else {
        crate::list::ctrl_c_ctrl_c(text, offset, begin, limit, ordered, settings)?
    };
    if !settings.checkbox_rule {
        return Ok((edited, cursor, message));
    }
    let (edited, cursor) =
        update_counts_keeping(&edited, keywords, cursor, tags_column, cursor, settings.checkbox_hierarchical);
    Ok((edited, cursor, message))
}

/// `org-update-checkbox-count` for the section around byte `offset`: each
/// statistics cookie there counts the checkboxes of what it is in (an
/// item's children, or the lists of a drawer, a block or the section).
///
/// A cookie on a headline with tags realigns them to `tags_column`, as
/// `org-fix-tags-on-the-fly` does. Unless `hierarchical`
/// (`org-checkbox-hierarchical-statistics`), a cookie counts every box
/// below it, as with `recursive` in `COOKIE_DATA`.
pub fn update_counts(
    text: &str,
    keywords: &TodoKeywords,
    offset: usize,
    tags_column: i64,
    hierarchical: bool,
) -> String {
    update_counts_keeping(text, keywords, offset, tags_column, offset, hierarchical).0
}

/// [`update_counts`], with where `cursor` goes: a cookie is replaced by
/// inserting the new one before it and deleting the old, so a cursor in
/// the old cookie ends up after the new one.
pub fn update_counts_keeping(
    text: &str,
    keywords: &TodoKeywords,
    offset: usize,
    tags_column: i64,
    mut cursor: usize,
    hierarchical: bool,
) -> (String, usize) {
    let root = parse(text, keywords);
    let lists = Lists::new(text, &root);
    let line = line_start(text, offset);
    let mut start = line;
    while headline_level(&text[start..line_end(text, start)]).is_none() && start > 0 {
        start = line_start(text, start - 1);
    }
    if headline_level(&text[start..line_end(text, start)]).is_none() {
        start = 0;
    }
    let end = next_heading(text, start).unwrap_or(text.len());
    let cookies: Vec<SyntaxNode> = root
        .descendants()
        .filter(|node| node.kind() == STATISTICS_COOKIE)
        .filter(|node| (start..end).contains(&span(node).0))
        .collect();
    let mut edits = Vec::new();
    for cookie in cookies {
        let (begin, finish) = span(&cookie);
        let data = cookie_data(text, begin);
        if has_word(&data, "todo") {
            continue;
        }
        let recursive = !hierarchical || has_word(&data, "recursive");
        let container = cookie.ancestors().find(|node| match node.kind() {
            DRAWER | DYNAMIC_BLOCK | LIST_ITEM => true,
            BLOCK => node
                .children_with_tokens()
                .filter_map(|element| element.into_token())
                .find(|token| token.kind() == BLOCK_BEGIN)
                .is_some_and(|token| {
                    let name = token.text().get(8..).unwrap_or("").to_ascii_uppercase();
                    !matches!(name.as_str(), "SRC" | "EXAMPLE" | "EXPORT" | "COMMENT")
                }),
            _ => false,
        });
        // The lists with a checkbox in the area the cookie counts.
        let (from, to) = match &container {
            Some(node) => span(node),
            None => (start, end),
        };
        let item = container
            .as_ref()
            .filter(|node| node.kind() == LIST_ITEM)
            .and_then(|node| lists.items.iter().position(|item| item.begin == line_start(text, span(node).0)));
        // Each outermost list with a checkbox in the area, counted with
        // the structure org-element gives it.
        let mut structs: Vec<usize> = Vec::new();
        for (index, candidate) in lists.items.iter().enumerate() {
            let inside = candidate.begin >= from && candidate.begin < to && Some(index) != item;
            if inside && candidate.checkbox.is_some() && !structs.contains(&candidate.list) {
                structs.push(candidate.list);
            }
        }
        let (mut checked, mut total) = (0, 0);
        for list in structs {
            let (on, all) = crate::list::count_boxes(
                text,
                lists.ranges[list].0,
                lists.limits[list],
                item.map(|item| lists.items[item].begin),
                recursive,
            );
            checked += on;
            total += all;
        }
        let new = if text[begin..finish].contains('%') {
            format!("[{}%]", 100 * checked / total.max(1))
        } else {
            format!("[{checked}/{total}]")
        };
        edits.push((begin, finish, new));
    }
    let mut out = text.to_string();
    for (begin, finish, new) in edits.into_iter().rev() {
        out.replace_range(begin..finish, &new);
        if cursor > finish {
            cursor = cursor + new.len() - (finish - begin);
        } else if cursor > begin {
            cursor = begin + new.len();
        }
        let start = line_start(&out, begin);
        let end = line_end(&out, begin);
        let line = &out[start..end];
        let before_tags = tag_group(line).is_some_and(|(_, tags)| start + tags > begin + new.len());
        if headline_level(line).is_some() && before_tags {
            if let Some((blanks, tags, padding)) = tags_padding(line, tags_column) {
                let (from, to) = (start + blanks, start + tags);
                out.replace_range(from..to, &padding);
                if cursor >= to {
                    cursor = cursor + padding.len() - (to - from);
                } else if cursor > from {
                    cursor = from + (cursor - from).min(padding.len());
                }
            }
        }
    }
    (out, cursor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_headline_goes_by_its_first_item_after_its_drawers() {
        // As Emacs's `org-toggle-checkbox` does: an item in the LOGBOOK is
        // neither the box the others follow nor toggled.
        let keywords = TodoKeywords::default();
        let toggled = |text: &str| toggle(text, &keywords, 1, -77, &Default::default()).unwrap();
        assert_eq!(
            toggled("* H\n:LOGBOOK:\n- [ ] note\n:END:\n- [ ] a\n- [X] b\n"),
            "* H\n:LOGBOOK:\n- [ ] note\n:END:\n- [X] a\n- [X] b\n"
        );
        assert_eq!(
            toggled("* H\n:LOGBOOK:\n- [X] note\n:END:\n- [ ] a\n- [ ] b\n"),
            "* H\n:LOGBOOK:\n- [X] note\n:END:\n- [X] a\n- [X] b\n"
        );
    }
}
