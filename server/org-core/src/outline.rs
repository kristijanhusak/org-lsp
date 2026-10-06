//! What a parsed document means: its entries and what each one inherits.
//!
//! [`Outline::new`] walks a syntax tree once and records, for every
//! headline, the things Org commands ask about: TODO state, tags with
//! inheritance, category, planning timestamps, and whether the entry sits
//! in an archived or commented subtree. It also collects the active
//! timestamps and date ranges the agenda shows. The agenda and other
//! commands then work on this flat model instead of the tree.

use crate::priority::Priorities;
use org_syntax::{ast, ast::Headline, Parse, SyntaxKind, SyntaxKind::*, SyntaxNode, TodoKeywords, TokenAtOffset};

/// A tag on an entry, and whether it comes from a parent or the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    pub inherited: bool,
}

/// Which tags or properties pass from an entry to its children (and from
/// the file to its entries): Org's `t`, `nil`, a list of names, or a
/// regexp, as `org-use-tag-inheritance` and
/// `org-use-property-inheritance` take them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Inherit {
    #[default]
    All,
    Nothing,
    Names(Vec<String>),
    /// An Emacs regexp, matched without regard to case.
    Matching(String),
}

/// What entries inherit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inheritance {
    /// `org-use-tag-inheritance`
    pub tags: Inherit,
    /// `org-tags-exclude-from-inheritance`
    pub tags_excluded: Vec<String>,
    /// `org-use-property-inheritance`
    pub properties: Inherit,
}

impl Default for Inheritance {
    fn default() -> Self {
        Inheritance { tags: Inherit::All, tags_excluded: Vec::new(), properties: Inherit::Nothing }
    }
}

impl Inheritance {
    /// `org-remove-uninherited-tags`, for one tag: whether entries below
    /// get it. The excluded tags count with `t` and a regexp, not with a
    /// list.
    pub fn inherits_tag(&self, tag: &str) -> bool {
        let excluded = || self.tags_excluded.iter().any(|name| name == tag);
        match &self.tags {
            Inherit::All => !excluded(),
            Inherit::Nothing => false,
            Inherit::Names(names) => names.iter().any(|name| name == tag),
            Inherit::Matching(pattern) => matches(pattern, tag) && !excluded(),
        }
    }

    /// `org-property-inherit-p`
    pub fn inherits_property(&self, name: &str) -> bool {
        match &self.properties {
            Inherit::All => true,
            Inherit::Nothing => false,
            Inherit::Names(names) => names.iter().any(|known| known.eq_ignore_ascii_case(name)),
            Inherit::Matching(pattern) => matches(pattern, name),
        }
    }
}

/// `string-match` with `case-fold-search` on, as in an Org buffer.
fn matches(pattern: &str, text: &str) -> bool {
    crate::agenda::matcher::emacs_regex(pattern).is_some_and(|regex| regex.is_match(text))
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
    /// The entry or one of its parents is commented. The agenda leaves
    /// such entries out.
    pub commented: bool,
    /// The entry or one of its parents has the ARCHIVE tag, or the whole
    /// file has it. The agenda leaves such entries out unless archived
    /// trees are asked for (`org-agenda-archives-mode`).
    pub archived: bool,
    pub scheduled: Option<Planning>,
    pub deadline: Option<Planning>,
    pub closed: Option<Planning>,
    /// The entry's own properties, keys in upper case.
    pub properties: Vec<(String, String)>,
}

impl Entry {
    /// Whether the agenda leaves the entry out (`org-agenda-skip`), with
    /// archived trees included or not.
    pub fn skipped(&self, archives: bool) -> bool {
        self.commented || (self.archived && !archives)
    }

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

/// A line the agenda's log mode shows (`org-agenda-get-progress`): a
/// `CLOSED:`, a `CLOCK:` or a state change, with an inactive timestamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Log {
    /// Where the keyword starts.
    pub offset: usize,
    pub entry: usize,
    pub kind: LogKind,
    /// The date of the timestamp.
    pub date: crate::date::Date,
    /// What the agenda reads the time from: the keyword and the
    /// timestamp, with a clock's end time put in it (`[… 07:00-07:20]`).
    pub time: String,
    /// The note under it (`org-agenda-log-mode-add-notes`).
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogKind {
    Closed,
    /// With the time clocked, or `-` for a clock that runs.
    Clock(String),
    /// The state changed to.
    State(String),
}

/// A `CLOCK:` line, and the entry it belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockLine {
    pub offset: usize,
    pub entry: Option<usize>,
    pub clock: crate::clock::Clock,
}

/// The model of one document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outline {
    /// The category entries get when no property sets one.
    pub category: String,
    pub file_tags: Vec<String>,
    pub todo: TodoKeywords,
    /// The priorities the file sets with `#+PRIORITIES:`.
    pub priorities: Option<Priorities>,
    pub entries: Vec<Entry>,
    pub stamps: Vec<Stamp>,
    pub blocks: Vec<Block>,
    pub logs: Vec<Log>,
    pub clocks: Vec<ClockLine>,
    /// The properties of the drawer before the first headline, keys in
    /// upper case: the file's own, which every entry may inherit.
    pub file_properties: Vec<(String, String)>,
    /// `#+PROPERTY:` lines (`org-keyword-properties`), with `NAME+`
    /// lines appended to the value before them.
    pub keyword_properties: Vec<(String, String)>,
    pub inheritance: Inheritance,
}

const ARCHIVE_TAG: &str = "ARCHIVE";

impl Outline {
    /// Build the model. `name` is the file name without directory and
    /// extension, which is the category of last resort.
    pub fn new(parse: &Parse, text: &str, name: &str) -> Outline {
        Outline::with_inheritance(parse, text, name, &Inheritance::default())
    }

    /// Build the model with what entries inherit.
    pub fn with_inheritance(parse: &Parse, text: &str, name: &str, inheritance: &Inheritance) -> Outline {
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
            // Only the first such line counts.
            priorities: keyword_values(&root, "PRIORITIES").first().and_then(|value| Priorities::from_keyword(value)),
            entries: Vec::new(),
            stamps: Vec::new(),
            blocks: Vec::new(),
            logs: Vec::new(),
            clocks: Vec::new(),
            keyword_properties: keyword_properties(&root),
            file_properties: top_properties,
            inheritance: inheritance.clone(),
        };
        let file_archived = outline.file_tags.iter().any(|tag| tag == ARCHIVE_TAG);
        let inherited = Inherited {
            tags: outline.file_tags.iter().map(|name| Tag { name: name.clone(), inherited: true }).collect(),
            category,
            commented: false,
            archived: file_archived,
        };
        for headline in ast::headlines(&root) {
            outline.add_entry(&headline, text, None, &inherited);
        }
        outline.collect_stamps(&root, text);
        outline.collect_blocks(&root, text);
        outline.collect_logs(text);
        outline
    }

    /// `org-entry-get` with `selective` inheritance, for a property that
    /// is not one of Org's special ones: the entry's value, with its
    /// `NAME+` values appended; when the property is inherited and the
    /// entry has no `NAME`, those of its parents, the file's drawer and
    /// `#+PROPERTY:` before them. `nil` means unset.
    pub fn property(&self, entry: usize, name: &str) -> Option<String> {
        let local = |properties: &[(String, String)]| {
            let base = properties.iter().rev().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, v)| v.clone());
            let extra: Vec<String> = properties
                .iter()
                .filter(|(key, _)| key.strip_suffix('+').is_some_and(|key| key.eq_ignore_ascii_case(name)))
                .map(|(_, value)| value.clone())
                .collect();
            (base, extra)
        };
        let not_nil = |value: String| (value != "nil").then_some(value);
        if !self.inheritance.inherits_property(name) {
            let (base, extra) = local(&self.entries[entry].properties);
            // `org--property-local-values` drops a base value of `nil`.
            let parts: Vec<String> = base.and_then(not_nil).into_iter().chain(extra).collect();
            return (!parts.is_empty()).then(|| parts.join(" ")).and_then(not_nil);
        }
        // `org-entry-get-with-inheritance`: up the lineage, the file last.
        let mut values: Vec<String> = Vec::new();
        let mut found = false;
        let mut at = Some(entry);
        loop {
            let properties = match at {
                Some(index) => &self.entries[index].properties,
                None => &self.file_properties,
            };
            let (base, extra) = local(properties);
            let mut here: Vec<String> = base.iter().cloned().collect();
            here.extend(extra);
            here.append(&mut values);
            values = here;
            if base.is_some() {
                found = true;
                break;
            }
            match at {
                Some(index) => at = self.entries[index].parent,
                None => break,
            }
        }
        if !found {
            if let Some((_, global)) = self.keyword_properties.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)) {
                values.insert(0, global.clone());
            }
        }
        (!values.is_empty()).then(|| values.join(" ")).and_then(not_nil)
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
        let commented = inherited.commented || headline.is_commented();
        let archived = inherited.archived || archived;

        let section = headline.section();
        let planning = section.as_ref().and_then(|section| section.children().find(|child| child.kind() == PLANNING));
        // A property drawer counts right below the headline or its planning
        // line, not after a blank line or anything else.
        let drawer = section.as_ref().and_then(|section| {
            let mut children = section.children().skip_while(|child| child.kind() == PLANNING);
            let first = children.next().filter(|child| child.kind() == PROPERTY_DRAWER)?;
            let after = planning.as_ref().map_or(section.text_range().start(), |planning| planning.text_range().end());
            (first.text_range().start() == after).then_some(first)
        });
        let properties = drawer.map(|drawer| properties(&drawer)).unwrap_or_default();
        let category = category_values(&properties, &inherited.category);

        // Inherited tags first, the entry's own last; of duplicates the
        // most local one stays. File tags and those of parents pass as
        // `org-use-tag-inheritance` says.
        let mut tags: Vec<Tag> =
            inherited.tags.iter().filter(|tag| self.inheritance.inherits_tag(&tag.name)).cloned().collect();
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
            commented,
            archived,
            scheduled: planning.as_ref().and_then(|planning| planning_time(planning, "SCHEDULED:")),
            deadline: planning.as_ref().and_then(|planning| planning_time(planning, "DEADLINE:")),
            closed: planning.as_ref().and_then(|planning| planning_time(planning, "CLOSED:")),
            properties,
        });

        let mut child_tags = inherited.tags.clone();
        child_tags.extend(own_tags.into_iter().map(|name| Tag { name, inherited: true }));
        let for_children = Inherited { tags: child_tags, category, commented, archived };
        for child in headline.children() {
            self.add_entry(&child, text, Some(index), &for_children);
        }
    }

    /// Every place the agenda's scan for plain timestamps can stop, with
    /// what `org-agenda-get-timestamps` makes of it there.
    /// The lines of `org-agenda-get-progress`'s search: `CLOSED:`,
    /// `CLOCK:` and `- State "KEYWORD"`, each followed by an inactive
    /// timestamp; and every `CLOCK:` line.
    fn collect_logs(&mut self, text: &str) {
        let mut at = 0;
        for line in text.split_inclusive('\n') {
            let start = at;
            at += line.len();
            let line = line.trim_end_matches(['\n', '\r']);
            if let Some(clock) = crate::clock::parse_line(line) {
                self.clocks.push(ClockLine { offset: start, entry: self.entry_at(start), clock });
            }
            let next = text[at..].split('\n').next().unwrap_or("");
            for found in log_matches(line, &self.todo) {
                let Some(entry) = self.entry_at(start + found.at) else { continue };
                // `org-agenda-log-mode-add-notes`
                let note = match &found.kind {
                    LogKind::Clock(_) => next
                        .trim_start_matches([' ', '\t'])
                        .strip_prefix('-')
                        .filter(|rest| rest.starts_with([' ', '\t']))
                        .map(|rest| rest.trim_matches([' ', '\t']))
                        .filter(|rest| !rest.is_empty() && !rest.starts_with('-')),
                    LogKind::State(_) if line.ends_with('\\') => {
                        Some(next.trim_matches([' ', '\t'])).filter(|rest| !rest.is_empty() && !rest.starts_with('-'))
                    }
                    _ => None,
                };
                self.logs.push(Log {
                    offset: start + found.at,
                    entry,
                    kind: found.kind,
                    date: found.date,
                    time: found.time,
                    note: note.map(str::to_string),
                });
            }
        }
    }

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
    /// to Org, or in a commented subtree. (Archived subtrees are left out
    /// by the agenda, unless it is asked for them.)
    ///
    /// `inside` is a byte of the timestamp, used to look at its context;
    /// `end` is where the timestamp text ends.
    fn stamp_entry(&self, root: &SyntaxNode, text: &str, point: usize, inside: usize, end: usize) -> Option<usize> {
        let entry = self.entry_at(point)?;
        if self.entries[entry].commented || at_date_range(text, point) {
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
    /// blocks and commented subtrees.
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
            if self.entries[entry].commented || in_source_block(root, range.end - 1) {
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
    commented: bool,
    archived: bool,
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
pub(crate) fn complex_heading_title(line: &str, level: usize, todo: &TodoKeywords) -> Option<(usize, usize)> {
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
pub(crate) fn keyword_values(root: &SyntaxNode, key: &str) -> Vec<String> {
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
/// `org-keyword-properties`: the `#+PROPERTY: NAME VALUE` lines in
/// order, a `NAME+` line appending to the value of `NAME`.
fn keyword_properties(root: &SyntaxNode) -> Vec<(String, String)> {
    let mut properties: Vec<(String, String)> = Vec::new();
    for value in keyword_values(root, "PROPERTY") {
        let Some((key, rest)) = value.split_once([' ', '\t']) else { continue };
        let rest = rest.trim_start_matches([' ', '\t']);
        let (key, appending) = match key.strip_suffix('+') {
            Some(key) => (key, true),
            None => (key, false),
        };
        let previous = properties.iter().position(|(known, _)| known == key).map(|at| properties.remove(at).1);
        let value = match previous {
            Some(previous) if appending => format!("{previous} {rest}"),
            _ => rest.to_string(),
        };
        properties.insert(0, (key.to_string(), value));
    }
    properties
}

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

/// A match of `org-agenda-get-progress`'s search in one line.
struct LogMatch {
    at: usize,
    kind: LogKind,
    date: crate::date::Date,
    time: String,
}

/// `\\<CLOSED: *[DATE`, `\\<CLOCK: *[DATE` and `- +State "KEYWORD".*? *[DATE`
/// in `line`, read as `org-agenda-get-progress` reads them.
fn log_matches(line: &str, todo: &TodoKeywords) -> Vec<LogMatch> {
    let mut out = Vec::new();
    let word_start =
        |at: usize| at == 0 || !line[..at].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_');
    // The inactive timestamp right after `from`, past blanks.
    let stamp_after = |from: usize| -> Option<(usize, usize)> {
        let open = from + line[from..].bytes().take_while(|b| *b == b' ').count();
        stamp_at(line, open).map(|end| (open, end))
    };
    for keyword in ["CLOSED:", "CLOCK:"] {
        for (at, _) in line.match_indices(keyword) {
            if !word_start(at) {
                continue;
            }
            let Some((open, end)) = stamp_after(at + keyword.len()) else { continue };
            let Some(date) = stamp_date(&line[open..]) else { continue };
            let mut time = line[at..end].to_string();
            let kind = if keyword == "CLOSED:" {
                LogKind::Closed
            } else {
                // A clock's end time goes into the timestamp, and the
                // time clocked after it is what it shows.
                match clock_end(&line[end..]) {
                    Some((until, clocked)) => {
                        time.pop();
                        time.push('-');
                        time.push_str(&until);
                        time.push(']');
                        LogKind::Clock(clocked)
                    }
                    None => LogKind::Clock("-".to_string()),
                }
            };
            out.push(LogMatch { at, kind, date, time });
        }
    }
    // `- +State "KEYWORD"`
    for (at, _) in line.match_indices("- ") {
        let rest = &line[at + 1..];
        let rest_trimmed = rest.trim_start_matches(' ');
        let Some(quoted) = rest_trimmed.strip_prefix("State \"") else { continue };
        let Some(close) = quoted.find('"') else { continue };
        let state = &quoted[..close];
        if !todo.names().any(|name| name == state) {
            continue;
        }
        let after = line.len() - quoted.len() + close + 1;
        // The first inactive timestamp after it.
        let Some(open) = line[after..].find('[').map(|offset| after + offset) else { continue };
        let Some(end) = stamp_at(line, open) else { continue };
        let Some(date) = stamp_date(&line[open..]) else { continue };
        out.push(LogMatch { at, kind: LogKind::State(state.to_string()), date, time: line[at..end].to_string() });
    }
    out.sort_by_key(|found| found.at);
    out
}

/// The end of the inactive timestamp (`org-ts-regexp-inactive`) that
/// opens at `open`, past its closing bracket.
fn stamp_at(line: &str, open: usize) -> Option<usize> {
    let rest = line.get(open..)?.strip_prefix('[')?;
    stamp_date(&line[open..])?;
    let close = rest.find(']')?;
    if rest[..close].contains(['[', '\n', '>']) {
        return None;
    }
    Some(open + 1 + close + 1)
}

/// The date of `[YYYY-MM-DD`.
fn stamp_date(text: &str) -> Option<crate::date::Date> {
    let bytes = text.as_bytes();
    if bytes.first() != Some(&b'[') || !is_date_at(bytes, 1) {
        return None;
    }
    Some(parse_date(&text[1..11]))
}

/// After a clock's start: its end time, and the time clocked
/// (`\\([0-9]\\{1,2\\}:[0-9]\\{2\\}\\)\\].*?\\([0-9]\\{1,2\\}:[0-9]\\{2\\}\\)`).
fn clock_end(rest: &str) -> Option<(String, String)> {
    let time_at = |text: &str, at: usize| -> Option<usize> {
        let bytes = text.as_bytes();
        let hours = bytes[at..].iter().take_while(|b| b.is_ascii_digit()).count();
        if !(1..=2).contains(&hours) || bytes.get(at + hours) != Some(&b':') {
            return None;
        }
        let minutes = bytes.get(at + hours + 1..at + hours + 3)?;
        minutes.iter().all(u8::is_ascii_digit).then_some(at + hours + 3)
    };
    let mut first = None;
    for at in 0..rest.len() {
        if !rest.is_char_boundary(at) {
            continue;
        }
        if let Some(end) = time_at(rest, at).filter(|end| rest.as_bytes().get(*end) == Some(&b']')) {
            first = Some((at, end));
            break;
        }
    }
    let (start, end) = first?;
    let until = rest[start..end].to_string();
    for at in end + 1..rest.len() {
        if !rest.is_char_boundary(at) {
            continue;
        }
        if let Some(second_end) = time_at(rest, at) {
            return Some((until, rest[at..second_end].to_string()));
        }
    }
    None
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
        let skips: Vec<bool> = outline(text).entries.iter().map(|entry| entry.skipped(false)).collect();
        assert_eq!(skips, [true, true, true, true, true, false, true]);
        assert!(outline("#+FILETAGS: :ARCHIVE:\n* a\n").entries[0].skipped(false));
        // With archived trees asked for, only the commented ones.
        let skips: Vec<bool> = outline(text).entries.iter().map(|entry| entry.skipped(true)).collect();
        assert_eq!(skips, [false, false, true, true, true, false, true]);
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
