//! What a parsed document means: its entries and what each one inherits.
//!
//! [`Outline::new`] walks a syntax tree once and records, for every
//! headline, the things Org commands ask about: TODO state, tags with
//! inheritance, category, planning timestamps, and whether the entry sits
//! in an archived or commented subtree. It also collects the active
//! timestamps and date ranges the agenda shows. The agenda and other
//! commands then work on this flat model instead of the tree.

use org_syntax::{ast, ast::Headline, Parse, SyntaxKind, SyntaxKind::*, SyntaxNode, TodoKeywords, TokenAtOffset};

use crate::timestr;

/// A tag on an entry, and whether it comes from a parent or the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    pub inherited: bool,
}

/// A `SCHEDULED:`, `DEADLINE:` or `CLOSED:` timestamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planning {
    /// The timestamp without its brackets; for a range, both halves.
    pub raw: String,
    /// Byte offset of the opening bracket.
    pub offset: usize,
    /// `<…>` rather than `[…]`.
    pub active: bool,
}

/// One headline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Byte offset of the first star.
    pub offset: usize,
    /// Byte offset of the end of the headline line, before its terminator.
    pub line_end: usize,
    pub level: usize,
    pub parent: Option<usize>,
    /// The line after the stars, trimmed: keyword, priority, title and tags.
    pub head: String,
    /// The TODO keyword, as `org-get-todo-state` sees it.
    pub todo: Option<String>,
    /// Whether `todo` is a done state.
    pub done: bool,
    /// Whether the element parser sees a done keyword. It wants a blank
    /// after the keyword, so `* DONE` alone does not count.
    pub element_done: bool,
    /// File tags, then tags of parents, then the entry's own. A tag that
    /// appears more than once is kept at its most local position.
    pub tags: Vec<Tag>,
    pub category: String,
    /// The entry or one of its parents is archived or commented, or the
    /// whole file is archived. The agenda leaves such entries out.
    pub skip: bool,
    pub scheduled: Option<Planning>,
    pub deadline: Option<Planning>,
    pub closed: Option<Planning>,
    /// The entry's own properties, keys in upper case.
    pub properties: Vec<(String, String)>,
}

impl Entry {
    /// Whether the entry counts for the global TODO list: its headline has
    /// one of the not-done keywords, followed by blanks or nothing.
    pub fn is_open_todo(&self) -> bool {
        self.todo.is_some() && !self.done
    }
}

/// An active timestamp outside planning lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stamp {
    pub offset: usize,
    /// The timestamp with its brackets, up to the first closing one.
    pub text: String,
    pub entry: usize,
    /// The date as written. It may be out of range (`2026-02-30`).
    pub date: crate::date::Date,
    /// The text ends in a repeater, which is the only place the agenda
    /// recognises one in a plain timestamp.
    pub repeats: bool,
}

/// A date range, `<…>--<…>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// Byte offset just past the range.
    pub offset: usize,
    /// The two timestamps without their brackets.
    pub first: String,
    pub second: String,
    pub entry: usize,
}

/// The model of one document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outline {
    /// The category entries get when no property sets one.
    pub category: String,
    pub file_tags: Vec<String>,
    pub todo: TodoKeywords,
    pub entries: Vec<Entry>,
    pub stamps: Vec<Stamp>,
    pub blocks: Vec<Block>,
}

const ARCHIVE_TAG: &str = "ARCHIVE";

impl Outline {
    /// Build the model. `name` is the file name without directory and
    /// extension, which is the category of last resort.
    pub fn new(parse: &Parse, text: &str, name: &str) -> Outline {
        let root = parse.syntax();
        let todo = parse.todo_keywords().clone();
        let file_tags = file_tags(&root);
        let top_properties = root
            .children()
            .find(|child| child.kind() == SECTION)
            .and_then(|section| section.children().find(|child| child.kind() == PROPERTY_DRAWER))
            .map(|drawer| properties(&drawer))
            .unwrap_or_default();
        let category = property(&top_properties, "CATEGORY")
            .map(str::to_string)
            .or_else(|| last_keyword_value(&root, "CATEGORY"))
            .unwrap_or_else(|| name.to_string());

        let mut outline =
            Outline { category, file_tags, todo, entries: Vec::new(), stamps: Vec::new(), blocks: Vec::new() };
        let file_archived = outline.file_tags.iter().any(|tag| tag == ARCHIVE_TAG);
        let inherited = Inherited {
            tags: outline.file_tags.iter().map(|name| Tag { name: name.clone(), inherited: true }).collect(),
            category: outline.category.clone(),
            skip: file_archived,
        };
        for headline in ast::headlines(&root) {
            outline.add_entry(&headline, text, None, &inherited);
        }
        outline.collect_stamps(&root, text);
        outline.collect_blocks(&root, text);
        outline
    }

    /// The entry whose section contains byte `offset`.
    pub fn entry_at(&self, offset: usize) -> Option<usize> {
        self.entries.partition_point(|entry| entry.offset <= offset).checked_sub(1)
    }

    fn add_entry(&mut self, headline: &Headline, text: &str, parent: Option<usize>, inherited: &Inherited) {
        let line_range = headline.line_range();
        let (offset, line_end) = (usize::from(line_range.start()), usize::from(line_range.end()));
        let line = &text[offset..line_end];
        let level = headline.level();
        let head = crate::text::trim(&line[level..]).to_string();

        // `org-todo-line-regexp`: blanks, a keyword, then blanks or the end.
        let first_word = head.split([' ', '\t']).next().unwrap_or("");
        let after_word = &head[first_word.len()..];
        let has_keyword = line.as_bytes().get(level) == Some(&b' ')
            && self.todo.contains(first_word)
            && (after_word.is_empty() || after_word.starts_with(' '));
        let todo = has_keyword.then(|| first_word.to_string());
        let done = todo.as_deref().is_some_and(|word| self.todo.is_done(word));
        // The element parser needs a space after the keyword.
        let element_keyword = self.todo.contains(first_word) && after_word.starts_with(' ');
        let element_done = element_keyword && self.todo.is_done(first_word);

        let own_tags = headline.tags();
        let commented = is_commented(&head, element_keyword.then_some(first_word));
        let archived = own_tags.iter().any(|tag| tag == ARCHIVE_TAG);
        let skip = inherited.skip || commented || archived;

        let section = headline.section();
        let planning = section.as_ref().and_then(|section| section.children().find(|child| child.kind() == PLANNING));
        let drawer =
            section.as_ref().and_then(|section| section.children().find(|child| child.kind() == PROPERTY_DRAWER));
        let properties = drawer.map(|drawer| properties(&drawer)).unwrap_or_default();
        let category = property(&properties, "CATEGORY").map_or_else(|| inherited.category.clone(), str::to_string);

        // Inherited tags first, the entry's own last; of duplicates the
        // most local one stays.
        let mut tags: Vec<Tag> = inherited.tags.clone();
        tags.extend(own_tags.iter().map(|name| Tag { name: name.clone(), inherited: false }));
        let mut seen = Vec::new();
        let mut deduped: Vec<Tag> = Vec::new();
        for tag in tags.into_iter().rev() {
            if !seen.contains(&tag.name) {
                seen.push(tag.name.clone());
                deduped.push(tag);
            }
        }
        deduped.reverse();

        let index = self.entries.len();
        self.entries.push(Entry {
            offset,
            line_end,
            level,
            parent,
            head,
            todo,
            done,
            element_done,
            tags: deduped,
            category: category.clone(),
            skip,
            scheduled: planning.as_ref().and_then(|planning| planning_time(planning, "SCHEDULED:")),
            deadline: planning.as_ref().and_then(|planning| planning_time(planning, "DEADLINE:")),
            closed: planning.as_ref().and_then(|planning| planning_time(planning, "CLOSED:")),
            properties,
        });

        let mut child_tags = inherited.tags.clone();
        child_tags.extend(own_tags.into_iter().map(|name| Tag { name, inherited: true }));
        let for_children = Inherited { tags: child_tags, category, skip };
        for child in headline.children() {
            self.add_entry(&child, text, Some(index), &for_children);
        }
    }

    /// Active timestamps the agenda's plain-timestamp scan accepts: real
    /// timestamp objects outside planning and clock lines, and anything
    /// shaped like one in a property value.
    fn collect_stamps(&mut self, root: &SyntaxNode, text: &str) {
        let mut found = Vec::new();
        for node in root.descendants() {
            match node.kind() {
                TIMESTAMP => {
                    let in_planning = node.parent().is_some_and(|parent| matches!(parent.kind(), PLANNING | CLOCK));
                    let range = node.text_range();
                    let (start, end) = (usize::from(range.start()), usize::from(range.end()));
                    if !in_planning && text.as_bytes()[start] == b'<' {
                        found.push((start, end));
                    }
                }
                NODE_PROPERTY => {
                    let value = node
                        .children_with_tokens()
                        .filter_map(|child| child.into_token())
                        .find(|token| token.kind() == PROPERTY_VALUE);
                    if let Some(value) = value {
                        let base = usize::from(value.text_range().start());
                        found.extend(active_timestamps(value.text()).into_iter().map(|(a, b)| (base + a, base + b)));
                    }
                }
                _ => {}
            }
        }
        found.sort_unstable();
        for (start, end) in found {
            let Some(entry) = self.entry_at(start) else { continue };
            // A range is shown as a block, never as a timestamp.
            if in_date_range(text, start) {
                continue;
            }
            // Up to the first closing bracket, as `org-ts-regexp-both` reads it.
            let close = text[start..end].find(['>', ']']).map_or(end, |index| start + index + 1);
            let stamp_text = &text[start..close];
            let Some(parsed) = timestr::parse_time_string(stamp_text) else { continue };
            self.stamps.push(Stamp {
                offset: start,
                text: stamp_text.to_string(),
                entry,
                date: parsed.date,
                repeats: ends_in_repeater(stamp_text),
            });
        }
    }

    /// Date ranges, found the way the agenda finds them: by scanning the
    /// text, wherever they are, except in comments and source blocks.
    fn collect_blocks(&mut self, root: &SyntaxNode, text: &str) {
        let mut at = 0;
        while let Some((start, first, second, end)) = next_date_range(text, at) {
            at = end;
            let line_start = text[..start].rfind('\n').map_or(0, |index| index + 1);
            if text[line_start..].trim_start_matches([' ', '\t']).starts_with('#') {
                continue;
            }
            let Some(entry) = self.entry_at(end) else { continue };
            if self.entries[entry].skip || in_source_block(root, start) {
                continue;
            }
            self.blocks.push(Block { offset: end, first: first.to_string(), second: second.to_string(), entry });
        }
    }
}

/// What a headline passes down to its children.
struct Inherited {
    tags: Vec<Tag>,
    category: String,
    skip: bool,
}

/// Whether the title starts with `COMMENT`, after keyword and priority.
fn is_commented(head: &str, keyword: Option<&str>) -> bool {
    let mut rest = head;
    if let Some(keyword) = keyword {
        rest = rest[keyword.len()..].trim_start_matches([' ', '\t']);
    }
    // `[#A]`: any single character between the brackets.
    if let Some(after) = rest.strip_prefix("[#") {
        let mut chars = after.chars();
        if chars.next().is_some() && chars.next() == Some(']') {
            rest = chars.as_str().trim_start_matches([' ', '\t']);
        }
    }
    rest.strip_prefix("COMMENT").is_some_and(|after| after.is_empty() || after.starts_with([' ', '\t']))
}

fn token_text(node: &SyntaxNode, kind: SyntaxKind) -> Option<String> {
    node.children_with_tokens()
        .filter_map(|child| child.into_token())
        .find(|token| token.kind() == kind)
        .map(|token| token.text().to_string())
}

/// The properties of a drawer, keys without colons and in upper case.
/// A value of `nil` means the property is not set.
fn properties(drawer: &SyntaxNode) -> Vec<(String, String)> {
    drawer
        .children()
        .filter(|child| child.kind() == NODE_PROPERTY)
        .filter_map(|node| {
            let key = token_text(&node, PROPERTY_KEY)?;
            let value = token_text(&node, PROPERTY_VALUE).unwrap_or_default();
            Some((key.trim_matches(':').to_uppercase(), value))
        })
        .collect()
}

fn property<'a>(properties: &'a [(String, String)], key: &str) -> Option<&'a str> {
    properties.iter().find(|(name, value)| name == key && value != "nil").map(|(_, value)| value.as_str())
}

/// Stand-alone keyword lines with the given key, in document order.
fn keyword_values(root: &SyntaxNode, key: &str) -> Vec<String> {
    let wanted = format!("#+{key}:");
    root.descendants()
        .filter(|node| node.kind() == KEYWORD && !ast::is_affiliated_keyword(node))
        .filter(|node| token_text(node, KEYWORD_KEY).is_some_and(|found| found.eq_ignore_ascii_case(&wanted)))
        .map(|node| token_text(&node, KEYWORD_VALUE).unwrap_or_default())
        .collect()
}

fn last_keyword_value(root: &SyntaxNode, key: &str) -> Option<String> {
    keyword_values(root, key).pop()
}

/// `#+FILETAGS:` values, split on blanks and colons.
fn file_tags(root: &SyntaxNode) -> Vec<String> {
    keyword_values(root, "FILETAGS")
        .iter()
        .flat_map(|value| value.split_whitespace())
        .flat_map(|word| word.split(':'))
        .filter(|tag| !tag.is_empty())
        .map(str::to_string)
        .collect()
}

/// The timestamp directly after `keyword` on a planning line. When the
/// keyword is written twice, the last one counts.
fn planning_time(planning: &SyntaxNode, keyword: &str) -> Option<Planning> {
    let mut found = None;
    let mut children = planning.children_with_tokens().peekable();
    while let Some(child) = children.next() {
        if child.kind() != PLANNING_KEYWORD || child.as_token().is_none_or(|token| token.text() != keyword) {
            continue;
        }
        while children.peek().is_some_and(|next| next.kind() == WHITESPACE) {
            children.next();
        }
        found = children.peek().and_then(|next| next.as_node()).filter(|node| node.kind() == TIMESTAMP).map(|node| {
            let text = node.text().to_string();
            Planning {
                raw: text[1..text.len() - 1].to_string(),
                offset: node.text_range().start().into(),
                active: text.starts_with('<'),
            }
        });
    }
    found
}

/// `\+[0-9]+[hdwmy]>` at the end: the only repeater the agenda's scan for
/// plain timestamps recognises.
fn ends_in_repeater(stamp: &str) -> bool {
    let Some(body) = stamp.strip_suffix('>') else { return false };
    let Some(body) = body.strip_suffix(['h', 'd', 'w', 'm', 'y']) else { return false };
    let digits = body.bytes().rev().take_while(u8::is_ascii_digit).count();
    digits > 0 && body[..body.len() - digits].ends_with('+')
}

/// Length of `<DATE>` or `<DATE REST>` at the start of `text`
/// (`org-ts-regexp`): the date, then the closing bracket or a space and
/// anything up to the first closing bracket on the line.
pub(crate) fn active_timestamp_len(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.first() != Some(&b'<') {
        return None;
    }
    let date = bytes.get(1..11)?;
    let is_date = date.iter().enumerate().all(|(index, b)| match index {
        4 | 7 => *b == b'-',
        _ => b.is_ascii_digit(),
    });
    if !is_date {
        return None;
    }
    match bytes.get(11)? {
        b'>' => Some(12),
        b' ' => {
            let close = bytes[12..].iter().position(|b| matches!(b, b'>' | b'\n'))?;
            (bytes[12 + close] == b'>').then_some(12 + close + 1)
        }
        _ => None,
    }
}

/// Every active timestamp in `text`, as byte ranges.
fn active_timestamps(text: &str) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(open) = text[at..].find('<').map(|index| at + index) {
        match active_timestamp_len(&text[open..]) {
            Some(len) => {
                found.push((open, open + len));
                at = open + len;
            }
            None => at = open + 1,
        }
    }
    found
}

/// The next `<…>--<…>` at or after `from` (`org-tr-regexp`), with one to
/// three dashes: its start, the two timestamps without brackets, its end.
fn next_date_range(text: &str, from: usize) -> Option<(usize, &str, &str, usize)> {
    let mut at = from;
    while let Some(open) = text[at..].find('<').map(|index| at + index) {
        at = open + 1;
        let Some(first_len) = active_timestamp_len(&text[open..]) else { continue };
        let rest = &text[open + first_len..];
        let dashes = rest.bytes().take_while(|b| *b == b'-').count();
        if !(1..=3).contains(&dashes) {
            continue;
        }
        let second_start = open + first_len + dashes;
        let Some(second_len) = active_timestamp_len(&text[second_start..]) else { continue };
        let first = &text[open + 1..open + first_len - 1];
        let second = &text[second_start + 1..second_start + second_len - 1];
        return Some((open, first, second, second_start + second_len));
    }
    None
}

/// `org-at-date-range-p` for the timestamp starting at `start`: whether it
/// is the first or the second half of a date range.
fn in_date_range(text: &str, start: usize) -> bool {
    if next_date_range(text, start).is_some_and(|(found, ..)| found == start) {
        return true;
    }
    // The second half: look for a range that begins earlier on this line
    // and whose second timestamp is this one.
    let line_start = text[..start].rfind('\n').map_or(0, |index| index + 1);
    let mut at = line_start;
    while let Some((found, first, _, end)) = next_date_range(text, at) {
        if found >= start {
            break;
        }
        let second_start =
            found + first.len() + 2 + text[found + first.len() + 2..].bytes().take_while(|b| *b == b'-').count();
        if second_start == start {
            return true;
        }
        at = end.max(found + 1);
    }
    false
}

/// Whether `offset` lies in the body of a source block.
fn in_source_block(root: &SyntaxNode, offset: usize) -> bool {
    let token = match root.token_at_offset((offset as u32).into()) {
        TokenAtOffset::None => return false,
        TokenAtOffset::Single(token) => token,
        TokenAtOffset::Between(_, right) => right,
    };
    let in_body = token.kind() == TEXT;
    in_body
        && token.parent_ancestors().any(|node| {
            node.kind() == BLOCK
                && token_text(&node, BLOCK_BEGIN).is_some_and(|begin| begin.eq_ignore_ascii_case("#+begin_src"))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outline(text: &str) -> Outline {
        let parse = org_syntax::parse(text, &TodoKeywords::default());
        Outline::new(&parse, text, "notes")
    }

    fn tag_names(entry: &Entry) -> Vec<String> {
        entry.tags.iter().map(|tag| format!("{}{}", if tag.inherited { "^" } else { "" }, tag.name)).collect()
    }

    #[test]
    fn entries_and_inheritance() {
        let text = "\
#+FILETAGS: :file:shared:
#+CATEGORY: first
#+CATEGORY: work
* TODO [#A] Parent :p:shared:
** DONE Child :c:
:PROPERTIES:
:CATEGORY: sub
:END:
*** Grandchild
* DONE
* TODO
* Plain
";
        let outline = outline(text);
        assert_eq!(outline.category, "work");
        assert_eq!(outline.file_tags, ["file", "shared"]);
        let entries = &outline.entries;
        assert_eq!(entries.len(), 6);
        assert_eq!(entries[0].head, "TODO [#A] Parent :p:shared:");
        assert_eq!((entries[0].todo.as_deref(), entries[0].done), (Some("TODO"), false));
        assert_eq!(tag_names(&entries[0]), ["^file", "p", "shared"]);
        assert_eq!(tag_names(&entries[1]), ["^file", "^p", "^shared", "c"]);
        assert_eq!(tag_names(&entries[2]), ["^file", "^p", "^shared", "^c"]);
        assert_eq!((entries[1].parent, entries[2].parent, entries[3].parent), (Some(0), Some(1), None));
        assert_eq!(entries[0].category, "work");
        assert_eq!(entries[1].category, "sub");
        assert_eq!(entries[2].category, "sub");
        assert!(entries[1].done && entries[1].element_done);
        // A keyword with nothing after it: a state for Org, not for the element parser.
        assert!(entries[3].done && !entries[3].element_done);
        assert!(entries[4].is_open_todo());
        assert_eq!(entries[5].todo, None);
        assert_eq!(outline.entry_at(0), None);
        assert_eq!(outline.entry_at(entries[1].offset + 3), Some(1));
    }

    #[test]
    fn category_falls_back_to_the_file_name() {
        assert_eq!(outline("* a\n").entries[0].category, "notes");
        let with_drawer = outline(":PROPERTIES:\n:CATEGORY: top\n:END:\n#+CATEGORY: kw\n* a\n");
        assert_eq!(with_drawer.entries[0].category, "top");
        let nil = outline("#+CATEGORY: kw\n* a\n:PROPERTIES:\n:CATEGORY: nil\n:END:\n");
        assert_eq!(nil.entries[0].category, "kw");
    }

    #[test]
    fn archived_and_commented_subtrees_are_skipped() {
        let text = "* a :ARCHIVE:\n** b\n* COMMENT c\n** d\n* TODO COMMENT e\n* COMMENTARY f\n* [#A] COMMENT\n";
        let skips: Vec<bool> = outline(text).entries.iter().map(|entry| entry.skip).collect();
        assert_eq!(skips, [true, true, true, true, true, false, true]);
        assert!(outline("#+FILETAGS: :ARCHIVE:\n* a\n").entries[0].skip);
    }

    #[test]
    fn planning() {
        let text = "\
* a
SCHEDULED: <2026-10-05 Mon 09:00 +1w> DEADLINE: <2026-10-09 Fri -2d> CLOSED: [2026-10-01 Thu]
* b
DEADLINE: [2026-10-09 Fri] SCHEDULED: nothing <2026-10-05 Mon>
* c
text
SCHEDULED: <2026-10-05 Mon>
* d
SCHEDULED: <2026-10-05 Mon> SCHEDULED: <2026-10-06 Tue>
";
        let outline = outline(text);
        let a = &outline.entries[0];
        assert_eq!(a.scheduled.as_ref().map(|p| (p.raw.as_str(), p.active)), Some(("2026-10-05 Mon 09:00 +1w", true)));
        assert_eq!(a.deadline.as_ref().map(|p| p.raw.as_str()), Some("2026-10-09 Fri -2d"));
        assert_eq!(a.closed.as_ref().map(|p| p.active), Some(false));
        let b = &outline.entries[1];
        assert_eq!(b.deadline.as_ref().map(|p| p.active), Some(false));
        assert_eq!(b.scheduled, None);
        assert_eq!(outline.entries[2].scheduled, None);
        assert_eq!(outline.entries[3].scheduled.as_ref().map(|p| p.raw.as_str()), Some("2026-10-06 Tue"));
        // Nothing on a planning line is a plain timestamp; `c`'s line is body text.
        assert_eq!(outline.stamps.iter().map(|s| s.entry).collect::<Vec<_>>(), [2]);
    }

    #[test]
    fn stamps_and_blocks() {
        let text = "\
<2026-10-01 Thu> before the first headline
* Meeting <2026-10-06 Tue 14:00-15:30> :t:
:PROPERTIES:
:WHEN: <2026-10-10 Sat> and [2026-10-10 Sat]
:END:
Body <2026-10-07 Wed +1w> and [2026-10-08 Thu] and <2026-10-09 Fri .+2d -1d>.
# <2026-10-11 Sun>
#+begin_src org
<2026-10-12 Mon>--<2026-10-13 Tue>
#+end_src
* Range
<2026-10-14 Wed>--<2026-10-16 Fri> and <2026-10-17 Sat>-<2026-10-18 Sun>
#+begin_example
<2026-10-19 Mon>---<2026-10-20 Tue>
#+end_example
";
        let outline = outline(text);
        let stamps: Vec<(&str, usize, bool)> =
            outline.stamps.iter().map(|stamp| (stamp.text.as_str(), stamp.entry, stamp.repeats)).collect();
        assert_eq!(
            stamps,
            [
                ("<2026-10-06 Tue 14:00-15:30>", 0, false),
                ("<2026-10-10 Sat>", 0, false),
                ("<2026-10-07 Wed +1w>", 0, true),
                ("<2026-10-09 Fri .+2d -1d>", 0, false),
            ]
        );
        let blocks: Vec<(&str, &str, usize)> =
            outline.blocks.iter().map(|block| (block.first.as_str(), block.second.as_str(), block.entry)).collect();
        assert_eq!(
            blocks,
            [
                ("2026-10-14 Wed", "2026-10-16 Fri", 1),
                ("2026-10-17 Sat", "2026-10-18 Sun", 1),
                ("2026-10-19 Mon", "2026-10-20 Tue", 1),
            ]
        );
    }
}
