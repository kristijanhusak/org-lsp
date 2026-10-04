//! What a parsed document means: its entries and what each one inherits.
//!
//! [`Outline::new`] walks a syntax tree once and records, for every
//! headline, the things Org commands ask about: TODO state, tags with
//! inheritance, category, planning timestamps, and whether the entry sits
//! in an archived or commented subtree. It also collects the active
//! timestamps and date ranges the agenda shows. The agenda and other
//! commands then work on this flat model instead of the tree.

use org_syntax::{ast, ast::Headline, Parse, SyntaxKind, SyntaxKind::*, SyntaxNode, TodoKeywords, TokenAtOffset};

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
    /// The agenda's own search (`KEYWORD: *<…>`) finds something on the
    /// planning line. Without that the agenda never looks at the entry.
    pub searchable: bool,
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
    /// The TODO keyword, as `org-get-todo-state` sees it: spaces after the
    /// stars, the keyword, then a space or only blanks.
    pub todo: Option<String>,
    /// Whether `todo` is a done state.
    pub done: bool,
    /// Whether the syntax tree has a done keyword. The two views differ
    /// when tabs surround the keyword.
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

/// A place where the agenda's scan for plain timestamps can stop: `<`
/// followed by a date.
///
/// The scan stops in one of two ways. On the day the date names it stops
/// right after the date. On other days it stops only at the repeater form,
/// `<… +1w>`, and then after the closing bracket. What it finds there
/// decides whether the timestamp counts, so both outcomes are recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stamp {
    pub offset: usize,
    /// The date after the bracket, as written. It may be out of range
    /// (`2026-02-30`), and then no day matches it.
    pub date: Option<crate::date::Date>,
    /// What shows when the scan stops here on that date.
    pub on_date: Option<Shown>,
    /// Byte offset just past the repeater form, when the text has it.
    pub repeat_end: Option<usize>,
    /// What shows when the scan stops at the repeater form.
    pub on_repeat: Option<Shown>,
}

/// The entry a timestamp belongs to, and the timestamp as the agenda reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub entry: usize,
    pub text: String,
}

/// A date range, `<…>--<…>`, that the agenda shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// Byte offset just past the range.
    pub offset: usize,
    /// The two timestamps without their brackets. The first reaches back
    /// to the first timestamp on the line that leads up to the dashes.
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
        let from_file = last_keyword_value(&root, "CATEGORY").unwrap_or_else(|| name.to_string());
        let category = category_values(&top_properties, &from_file);

        let mut outline = Outline {
            category: shown_category(&category),
            file_tags,
            todo,
            entries: Vec::new(),
            stamps: Vec::new(),
            blocks: Vec::new(),
        };
        let file_archived = outline.file_tags.iter().any(|tag| tag == ARCHIVE_TAG);
        let inherited = Inherited {
            tags: outline.file_tags.iter().map(|name| Tag { name: name.clone(), inherited: true }).collect(),
            category,
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

        // `org-todo-line-regexp`: spaces, a keyword, then a space or only
        // blanks up to the end of the line.
        let spaces = line[level..].bytes().take_while(|b| *b == b' ').count();
        let after_spaces = &line[level + spaces..];
        let todo = self
            .todo
            .names()
            .find(|name| {
                after_spaces
                    .strip_prefix(name)
                    .is_some_and(|rest| rest.starts_with(' ') || rest.trim_matches([' ', '\t']).is_empty())
            })
            .map(str::to_string);
        let done = todo.as_deref().is_some_and(|word| self.todo.is_done(word));
        let element_done = headline.todo_keyword().is_some_and(|token| self.todo.is_done(token.text()));

        let own_tags = headline.tags();
        let archived = own_tags.iter().any(|tag| tag == ARCHIVE_TAG);
        let skip = inherited.skip || headline.is_commented() || archived;

        let section = headline.section();
        let planning = section.as_ref().and_then(|section| section.children().find(|child| child.kind() == PLANNING));
        let drawer =
            section.as_ref().and_then(|section| section.children().find(|child| child.kind() == PROPERTY_DRAWER));
        let properties = drawer.map(|drawer| properties(&drawer)).unwrap_or_default();
        let category = category_values(&properties, &inherited.category);

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
            category: shown_category(&category),
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

    /// Every place the agenda's scan for plain timestamps can stop, with
    /// what `org-agenda-get-timestamps` makes of it there.
    fn collect_stamps(&mut self, root: &SyntaxNode, text: &str) {
        let bytes = text.as_bytes();
        let mut from = 0;
        while let Some(offset) = text[from..].find('<').map(|index| from + index) {
            from = offset + 1;
            let date = is_date_at(bytes, offset + 1).then(|| parse_date(&text[offset + 1..offset + 11]));
            let repeat_end = repeater_form_end(text, offset);
            if date.is_none() && repeat_end.is_none() {
                continue;
            }
            // On its date the timestamp is read up to its first closing
            // bracket; the scan itself stops right after the date.
            let on_date = date.and_then(|_| {
                let end = timestamp_end(text, offset)?;
                let entry = self.stamp_entry(root, text, offset + 11, offset + 11, end)?;
                Some(Shown { entry, text: text[offset..end].to_string() })
            });
            let on_repeat = repeat_end.and_then(|end| {
                let entry = self.stamp_entry(root, text, end, end - 1, end)?;
                Some(Shown { entry, text: text[offset..end].to_string() })
            });
            self.stamps.push(Stamp { offset, date, on_date, repeat_end, on_repeat });
        }
    }

    /// The entry a timestamp counts for when the scan stops at `point`, or
    /// `None` when Org skips it there: in a date range, on a planning
    /// line, before the first headline, where the text is not a timestamp
    /// to Org, or in an archived or commented subtree.
    ///
    /// `inside` is a byte of the timestamp, used to look at its context;
    /// `end` is where the timestamp text ends.
    fn stamp_entry(&self, root: &SyntaxNode, text: &str, point: usize, inside: usize, end: usize) -> Option<usize> {
        let entry = self.entry_at(point)?;
        if self.entries[entry].skip || at_date_range(text, point) {
            return None;
        }
        let token = token_at(root, inside)?;
        let mut in_property = false;
        let mut timestamp = false;
        for node in token.parent_ancestors() {
            match node.kind() {
                PLANNING => return None,
                TIMESTAMP => timestamp = !node.parent().is_some_and(|parent| parent.kind() == CLOCK),
                NODE_PROPERTY => in_property = true,
                _ => {}
            }
        }
        // In a headline, Org looks for objects in the title as its
        // commands delimit it, which is not always the parser's title.
        let headline = &self.entries[entry];
        if inside < headline.line_end {
            let line = &text[headline.offset..headline.line_end];
            let (start, title_end) = complex_heading_title(line, headline.level, &self.todo)?;
            let start = start + if line[start..].starts_with("COMMENT") { "COMMENT".len() } else { 0 };
            let (start, title_end) = (headline.offset + start, headline.offset + title_end);
            return (start <= inside && inside < title_end && (timestamp || end <= title_end)).then_some(entry);
        }
        // In a property drawer anything shaped like a timestamp counts.
        let line_end = text[inside..].find('\n').map_or(text.len(), |index| inside + index);
        (timestamp || (in_property && end <= line_end)).then_some(entry)
    }

    /// Date ranges, found the way `org-agenda-get-blocks` finds them: by
    /// scanning the text, wherever they are, except in comments, source
    /// blocks and skipped subtrees.
    fn collect_blocks(&mut self, root: &SyntaxNode, text: &str) {
        let mut at = 0;
        while let Some(range) = next_date_range(text, at) {
            at = range.end;
            let line_start = text[..range.end].rfind('\n').map_or(0, |index| index + 1);
            let line = text[line_start..].trim_start_matches([' ', '\t']);
            if line.strip_prefix('#').is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\n'])) {
                continue;
            }
            let Some(entry) = self.entry_at(range.end) else { continue };
            if self.entries[entry].skip || in_source_block(root, range.end - 1) {
                continue;
            }
            self.blocks.push(Block {
                offset: range.end,
                first: text[range.first.0..range.first.1].to_string(),
                second: text[range.second.0..range.second.1].to_string(),
                entry,
            });
        }
    }
}

/// What a headline passes down to its children.
struct Inherited {
    tags: Vec<Tag>,
    category: String,
    skip: bool,
}

fn token_text(node: &SyntaxNode, kind: SyntaxKind) -> Option<String> {
    node.children_with_tokens()
        .filter_map(|child| child.into_token())
        .find(|token| token.kind() == kind)
        .map(|token| token.text().to_string())
}

/// The title of a headline line as `org-complex-heading-regexp` delimits
/// it (its fourth group), as a byte range of `line`.
///
/// Org's commands use this expression rather than the parser. It wants
/// spaces, not tabs, before the keyword and the priority, takes any single
/// character as a priority, and only sees a cookie right after the keyword.
fn complex_heading_title(line: &str, level: usize, todo: &TodoKeywords) -> Option<(usize, usize)> {
    // What may end the line after any of the parts: blanks, with tags or without.
    let is_tail = |rest: &str| {
        let trimmed = rest.trim_end_matches([' ', '\t']);
        trimmed.is_empty() || crate::text::tag_group(rest).is_some_and(|(blanks, _)| blanks == 0)
    };
    let follows = |rest: &str| rest.starts_with(' ') || is_tail(rest);
    let spaces = |at: usize| line[at..].bytes().take_while(|b| *b == b' ').count();

    let mut pos = level;
    let word = pos + spaces(pos);
    if let Some(keyword) = todo.names().find(|name| line[word..].strip_prefix(name).is_some_and(follows)) {
        pos = word + keyword.len();
    }
    let cookie = pos + spaces(pos);
    if cookie > pos {
        if let Some(rest) = line[cookie..].strip_prefix("[#") {
            let mut chars = rest.chars();
            if chars.next().is_some() && chars.next() == Some(']') && follows(chars.as_str()) {
                pos = line.len() - chars.as_str().len();
            }
        }
    }
    if is_tail(&line[pos..]) {
        return None;
    }
    let start = pos + spaces(pos);
    let end = match crate::text::tag_group(&line[start..]) {
        Some((blanks, _)) => start + blanks,
        None => line.trim_end_matches([' ', '\t']).len(),
    };
    (end > start).then_some((start, end))
}

/// The properties of a drawer, keys without colons and in upper case.
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

/// The `CATEGORY` value a drawer leads to: its own value, or the inherited
/// one, with any `CATEGORY+` values appended.
fn category_values(properties: &[(String, String)], inherited: &str) -> String {
    let own = properties.iter().find(|(name, _)| name == "CATEGORY").map(|(_, value)| value.as_str());
    let mut values = vec![own.unwrap_or(inherited)];
    values.extend(properties.iter().filter(|(name, _)| name == "CATEGORY+").map(|(_, value)| value.as_str()));
    values.join(" ")
}

/// The category as the agenda shows it. The value `nil` unsets the
/// property, and Org then has no category at all.
fn shown_category(value: &str) -> String {
    if value == "nil" { "???" } else { value }.to_string()
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
                searchable: false,
            }
        });
    }
    let line = planning.text().to_string();
    found.map(|planning| Planning { searchable: keyword_search_matches(&line, keyword), ..planning })
}

/// `\<KEYWORD *<[^>]+>` somewhere in `line`: what `org-deadline-time-regexp`
/// and `org-scheduled-time-regexp` look for.
fn keyword_search_matches(line: &str, keyword: &str) -> bool {
    line.match_indices(keyword).any(|(at, _)| {
        let rest = line[at + keyword.len()..].trim_start_matches(' ');
        crate::text::word_start(line, at)
            && rest.strip_prefix('<').is_some_and(|inner| inner.find('>').is_some_and(|close| close > 0))
    })
}

fn is_date_at(bytes: &[u8], at: usize) -> bool {
    bytes.get(at..at + 10).is_some_and(|date| {
        date.iter().enumerate().all(|(index, b)| match index {
            4 | 7 => *b == b'-',
            _ => b.is_ascii_digit(),
        })
    })
}

/// The date in ten bytes that `is_date_at` accepted.
fn parse_date(text: &str) -> crate::date::Date {
    let number = |range: std::ops::Range<usize>| text[range].parse().unwrap_or(0);
    crate::date::Date::new(number(0..4), number(5..7), number(8..10))
}

/// Where the timestamp that opens at `start` ends (`org-ts-regexp-both`):
/// a bracket, a date, then the closing bracket or a space and anything up
/// to the first closing bracket of either kind on the line.
pub(crate) fn timestamp_end(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if !matches!(bytes.get(start), Some(b'<' | b'[')) || !is_date_at(bytes, start + 1) {
        return None;
    }
    match bytes.get(start + 11)? {
        b'>' | b']' => Some(start + 12),
        b' ' => {
            let close = bytes[start + 12..].iter().position(|b| matches!(b, b'>' | b']' | b'\n'))?;
            (bytes[start + 12 + close] != b'\n').then_some(start + 12 + close + 1)
        }
        _ => None,
    }
}

/// `<[0-9]+-[0-9]+-[0-9]+[^>\n]+?\+[0-9]+[hdwmy]>` at `start`: the only
/// form in which the agenda's scan recognises a repeating timestamp.
/// Returns where it ends.
fn repeater_form_end(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut at = start + 1;
    for part in 0..3 {
        let digits = bytes[at..].iter().take_while(|b| b.is_ascii_digit()).count();
        if digits == 0 || (part < 2 && bytes.get(at + digits) != Some(&b'-')) {
            return None;
        }
        at += digits + usize::from(part < 2);
    }
    let close = at + bytes[at..].iter().position(|b| matches!(b, b'>' | b'\n'))?;
    let body = text[at..close].strip_suffix(['h', 'd', 'w', 'm', 'y'])?;
    let digits = body.bytes().rev().take_while(u8::is_ascii_digit).count();
    let before_plus = body.len().checked_sub(digits + 1)?;
    // At least one character stands between the date and the repeater.
    (bytes[close] == b'>' && digits > 0 && body.as_bytes()[before_plus] == b'+' && before_plus > 0).then_some(close + 1)
}

/// Length of `<DATE>` or `<DATE REST>` at the start of `text`
/// (`org-ts-regexp`), read as shortly as possible: up to the first `>`.
pub(crate) fn active_timestamp_len(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.first() != Some(&b'<') || !is_date_at(bytes, 1) {
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

/// A date range as a regular expression search finds it.
struct DateRange {
    /// The two timestamps without their brackets.
    first: (usize, usize),
    second: (usize, usize),
    end: usize,
}

/// A date range that starts exactly at `start`: two timestamps joined by
/// one to three dashes (`org-tr-regexp`, or `org-tr-regexp-both` when
/// `inactive` also allows square brackets).
///
/// Org lets the first timestamp run on, past closing brackets, until one
/// is followed by dashes and a second timestamp. So in `<A> <B>--<C>` the
/// range starts at `<A>`.
fn date_range_at(text: &str, start: usize, inactive: bool) -> Option<DateRange> {
    let bytes = text.as_bytes();
    let opens = |b: Option<&u8>| matches!(b, Some(b'<')) || (inactive && matches!(b, Some(b'[')));
    let closes = |b: u8| b == b'>' || (inactive && b == b']');
    if !opens(bytes.get(start)) || !is_date_at(bytes, start + 1) {
        return None;
    }
    let line_end = text[start..].find('\n').map_or(text.len(), |index| start + index);
    // Where the first timestamp may close.
    let closers: Vec<usize> = match bytes.get(start + 11) {
        Some(b) if closes(*b) => vec![start + 11],
        Some(b' ') => (start + 12..line_end).filter(|at| closes(bytes[*at])).collect(),
        _ => return None,
    };
    for close in closers {
        let dashes = bytes[close + 1..].iter().take_while(|b| **b == b'-').count();
        let second = close + 1 + dashes;
        if !(1..=3).contains(&dashes) || !opens(bytes.get(second)) || !is_date_at(bytes, second + 1) {
            continue;
        }
        let second_close = match bytes.get(second + 11) {
            Some(b) if closes(*b) => Some(second + 11),
            Some(b' ') => (second + 12..line_end).find(|at| closes(bytes[*at])),
            _ => None,
        };
        if let Some(second_close) = second_close {
            return Some(DateRange {
                first: (start + 1, close),
                second: (second + 1, second_close),
                end: second_close + 1,
            });
        }
    }
    None
}

/// The next active date range that starts at or after `from`.
fn next_date_range(text: &str, from: usize) -> Option<DateRange> {
    let mut at = from;
    while let Some(open) = text[at..].find('<').map(|index| at + index) {
        if let Some(range) = date_range_at(text, open, false) {
            return Some(range);
        }
        at = open + 1;
    }
    None
}

/// `org-at-date-range-p` with inactive ranges allowed: whether `point`
/// lies in a date range that starts at one of the two brackets before it.
fn at_date_range(text: &str, point: usize) -> bool {
    let bytes = text.as_bytes();
    let mut at = point;
    for _ in 0..2 {
        while at > 0 && !matches!(bytes[at - 1], b'[' | b'<' | b'\r' | b'\n') {
            at -= 1;
        }
        while at > 0 && matches!(bytes[at - 1], b'[' | b'<') {
            at -= 1;
        }
        if date_range_at(text, at, true).is_some_and(|range| range.end >= point) {
            return true;
        }
    }
    false
}

/// The token that holds byte `offset`.
fn token_at(root: &SyntaxNode, offset: usize) -> Option<org_syntax::SyntaxToken> {
    match root.token_at_offset((offset as u32).into()) {
        TokenAtOffset::None => None,
        TokenAtOffset::Single(token) => Some(token),
        TokenAtOffset::Between(_, right) => Some(right),
    }
}

/// Whether byte `offset` lies in the body of a source block.
fn in_source_block(root: &SyntaxNode, offset: usize) -> bool {
    let Some(token) = token_at(root, offset) else { return false };
    token.kind() == TEXT
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
        assert!(entries[3].done && entries[3].element_done);
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
        let nil = outline("#+CATEGORY: kw\n* a\n:PROPERTIES:\n:CATEGORY: nil\n:END:\n** b\n* c\n:PROPERTIES:\n:CATEGORY+: more\n:END:\n");
        let categories: Vec<&str> = nil.entries.iter().map(|entry| entry.category.as_str()).collect();
        assert_eq!(categories, ["???", "???", "kw more"]);
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
        assert!(a.scheduled.as_ref().is_some_and(|p| p.searchable));
        // Nothing on a planning line is a plain timestamp; `c`'s line is body text.
        let shown: Vec<usize> =
            outline.stamps.iter().filter_map(|stamp| stamp.on_date.as_ref()).map(|shown| shown.entry).collect();
        assert_eq!(shown, [2]);
    }

    #[test]
    fn keyword_states_follow_org_todo_line_regexp() {
        let outline = outline("* TODO\n* DONE\t\n* \tDONE x\n* DONE\tx\n*  DONE  x\n");
        let states: Vec<(Option<&str>, bool)> =
            outline.entries.iter().map(|entry| (entry.todo.as_deref(), entry.element_done)).collect();
        assert_eq!(
            states,
            [(Some("TODO"), false), (Some("DONE"), false), (None, true), (None, false), (Some("DONE"), true)]
        );
    }

    #[test]
    fn planning_the_agenda_cannot_find() {
        let outline = outline("* a\nSCHEDULED:\t<2026-10-05 Mon>\n* b\nSCHEDULED:  <2026-10-05 Mon>\n");
        let searchable: Vec<bool> =
            outline.entries.iter().map(|entry| entry.scheduled.as_ref().is_some_and(|p| p.searchable)).collect();
        assert_eq!(searchable, [false, true]);
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
        let stamps: Vec<(&str, usize, bool)> = outline
            .stamps
            .iter()
            .filter_map(|stamp| Some((stamp.on_date.as_ref()?, stamp.on_repeat.is_some())))
            .map(|(shown, repeats)| (shown.text.as_str(), shown.entry, repeats))
            .collect();
        assert_eq!(
            stamps,
            [
                ("<2026-10-06 Tue 14:00-15:30>", 0, false),
                ("<2026-10-10 Sat>", 0, false),
                ("<2026-10-07 Wed +1w>", 0, true),
                ("<2026-10-09 Fri .+2d -1d>", 0, false),
            ]
        );
        // The scan also stops at the others, and finds nothing to show.
        assert_eq!(outline.stamps.len(), 14);
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

    #[test]
    fn a_range_starts_at_the_first_timestamp_on_the_way() {
        let text = "* a\n<2026-05-17 Sun +1y> <2025-10-13 Mon>--<2026-05-19 Tue 8:00-9:00> x <2026-05-20 Wed>\n";
        let outline = outline(text);
        assert_eq!(outline.blocks.len(), 1);
        assert_eq!(outline.blocks[0].first, "2026-05-17 Sun +1y> <2025-10-13 Mon");
        assert_eq!(outline.blocks[0].second, "2026-05-19 Tue 8:00-9:00");
        // Everything inside the range is skipped; what follows it is not.
        let shown: Vec<&str> =
            outline.stamps.iter().filter_map(|stamp| stamp.on_date.as_ref()).map(|shown| shown.text.as_str()).collect();
        assert_eq!(shown, ["<2026-05-20 Wed>"]);
    }
}
