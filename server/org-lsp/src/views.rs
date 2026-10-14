//! The agenda as a document the server owns.
//!
//! A view is rendered from the agenda files into plain text. The server
//! remembers, for every line, the entry it came from, so that requests on
//! the view (go to definition, code actions, commands) reach the entry in
//! its file. Highlighting comes from semantic tokens, as for Org files.

use lsp_types::Uri;
use org_core::agenda::{self, Config, Face, ItemKind, LineKind, Matcher, Now, Span, Style};
use org_core::date::Day;
use org_core::outline::Outline;
use org_syntax::{TextRange, TextSize};

use crate::highlight::{modifier, Token, TokenType};
use crate::line_index::LineIndex;

/// The URI scheme of views the client fetches from the server.
pub const SCHEME: &str = "org-agenda";
/// The view document. There is one, as in Emacs: a new agenda replaces it.
pub const VIRTUAL_URI: &str = "org-agenda:/Org%20Agenda";
/// The file name used when the client cannot show virtual documents.
pub const FILE_NAME: &str = "Agenda.org-agenda";
/// The same for a client without org-lsp's editor code: an Org file's
/// name, so that the editor attaches the server to it, which colors it
/// and offers its commands.
pub const PLAIN_FILE_NAME: &str = "Agenda.org";
/// The language of the view document, by which editors tell it from Org files.
pub const LANGUAGE: &str = "org-agenda";

/// What a view shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Query {
    /// The agenda for `span`, including the day `start`, or today.
    Agenda { start: Option<Day>, span: Span },
    /// The global TODO list.
    Todo,
    /// `org-tags-view` from the agenda menu (`m`, `M`): the entries a
    /// match string matches, only open TODO entries with `todo_only`.
    Tags { matching: String, todo_only: bool },
    /// `org-search-view` from the agenda menu (`s`, `S`): the entries with
    /// the words of `string`, only open TODO entries with `todo_only`.
    Search { string: String, todo_only: bool },
    /// `org-agenda-list-stuck-projects` (`#`).
    Stuck,
    /// A command of `org_agenda_custom_commands`, by its keys. One of its
    /// agenda blocks may have been moved to other days (`f`, `b`, `d`
    /// there), as Emacs's `org-agenda-overriding-cmd` moves one block.
    Custom { keys: String, moved: Option<Moved> },
}

/// An agenda block of a custom command, moved to other days.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Moved {
    pub block: usize,
    pub start: Option<Day>,
    pub span: Span,
}

/// One block of a view: a whole view, or a part of a block agenda.
#[derive(Debug, Clone)]
pub struct Block {
    pub kind: BlockKind,
    /// `org-agenda-overriding-header`: another header, none if empty.
    pub header: Option<String>,
    pub config: Config,
}

#[derive(Debug, Clone)]
pub enum BlockKind {
    /// `org-agenda-list`
    Agenda { start: Option<Day>, span: Span },
    /// `org-todo-list`: open TODO entries, or those with the keywords
    /// `NEXT|WAIT`.
    Todo { keywords: Option<String> },
    /// `org-tags-view`; `tags-todo` with `todo_only`.
    Tags { matcher: Matcher, todo_only: bool },
    /// `org-search-view`
    Search { string: String, todo_only: bool },
    /// `org-agenda-list-stuck-projects`
    Stuck,
}

impl BlockKind {
    /// `org-agenda-type`: what Org's messages call the block.
    pub fn agenda_type(&self) -> &'static str {
        match self {
            BlockKind::Agenda { .. } => "agenda",
            BlockKind::Todo { .. } => "todo",
            // The stuck projects are a tags view (`org-tags-view`).
            BlockKind::Tags { .. } | BlockKind::Stuck => "tags",
            BlockKind::Search { .. } => "search",
        }
    }
}

/// The entry behind a line, in a form that survives edits to its file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryRef {
    pub uri: Uri,
    /// Byte offset of the headline when the view was built.
    pub offset: usize,
    /// The headline text, to find the entry again if it has moved.
    pub head: String,
    /// Byte offset of the timestamp the line stands for, when the view
    /// was built: its opening bracket, or the end of a date range.
    pub marker: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ViewLine {
    pub entry: Option<EntryRef>,
    /// What made the entry show up on this line.
    pub kind: Option<ItemKind>,
    /// The day the line stands under, in an agenda.
    pub day: Option<Day>,
    /// The block the line belongs to.
    pub block: usize,
}

/// A rendered view.
#[derive(Debug)]
pub struct View {
    pub query: Query,
    /// The first day shown, in the first agenda block.
    pub start: Day,
    /// The first day and the span of each block that is an agenda.
    pub blocks: Vec<Option<(Day, Span)>>,
    pub text: String,
    pub index: LineIndex,
    pub lines: Vec<ViewLine>,
    pub tokens: Vec<Token>,
    /// The lines of the entry the clock runs on (`org-agenda-clocking`),
    /// highlighted from edge to edge where the client can.
    pub clocking: Vec<u32>,
    /// The line of today's date, if it is shown.
    pub today_line: Option<u32>,
    pub today: Day,
    /// The lines of the blocks the view is made of, to show changes on
    /// them where they are ([`hold`]).
    pub source: Vec<(usize, agenda::Line)>,
    /// What lines show at their end, after a date changed from the agenda.
    pub stamps: Vec<(usize, String)>,
}

fn token_style(style: Style) -> Option<(TokenType, u32)> {
    Some(match style {
        Style::Structure => (TokenType::AgendaStructure, 0),
        Style::Date { today, weekend } => {
            let today = if today { modifier::TODAY } else { 0 };
            (TokenType::AgendaDate, today | if weekend { modifier::WEEKEND } else { 0 })
        }
        Style::Keyword { done } => (TokenType::TodoKeyword, if done { modifier::DONE } else { 0 }),
        Style::Priority => (TokenType::Priority, 0),
        Style::Tags => (TokenType::Tag, 0),
        Style::Link => (TokenType::Link, 0),
        Style::Table => (TokenType::TableDelimiter, 0),
        Style::Face(face) => match face {
            Face::Scheduled => (TokenType::AgendaScheduled, 0),
            Face::ScheduledToday => (TokenType::AgendaScheduled, modifier::TODAY),
            Face::ScheduledPreviously => (TokenType::AgendaScheduled, modifier::PAST),
            Face::DeadlineWarning => (TokenType::AgendaDeadline, modifier::IMMINENT),
            Face::DeadlineUpcoming => (TokenType::AgendaDeadline, 0),
            Face::DeadlineDistant => (TokenType::AgendaDeadline, modifier::DISTANT),
            Face::CalendarEvent | Face::CalendarSexp => (TokenType::AgendaEvent, 0),
            Face::CalendarDateRange => (TokenType::AgendaEvent, modifier::RANGE),
            Face::Done => (TokenType::AgendaDone, 0),
            Face::TimeGrid => (TokenType::AgendaTimeGrid, 0),
            Face::CurrentTime => (TokenType::AgendaTimeGrid, modifier::CURRENT),
            Face::Default => return None,
        },
        Style::Habit(face) => {
            use org_core::agenda::habit::Face as Habit;
            let (kind, future) = match face {
                Habit::Clear => (0, false),
                Habit::ClearFuture => (0, true),
                Habit::Ready => (modifier::READY, false),
                Habit::ReadyFuture => (modifier::READY, true),
                Habit::Alert => (modifier::ALERT, false),
                Habit::AlertFuture => (modifier::ALERT, true),
                Habit::Overdue => (modifier::OVERDUE, false),
                Habit::OverdueFuture => (modifier::OVERDUE, true),
            };
            (TokenType::AgendaHabit, kind | if future { modifier::FUTURE } else { 0 })
        }
    })
}

/// Build the view for `query` from the agenda files, given as their URIs
/// and outlines in agenda order, as `blocks` say: one for a plain agenda
/// or TODO list, more for a block agenda, with a separator between them.
/// The files a view is made of, when a search view searches more than
/// the agenda files: the agenda files come first in the list, and the
/// search goes through `search`, indices into it, in its own order.
#[derive(Debug, Clone, Default)]
pub struct SearchFiles {
    /// How many of the files are the agenda's.
    pub agenda: usize,
    pub search: Vec<usize>,
}

/// The files `org-search-view` searches, as indices into `paths` (whose
/// first `agenda` are the agenda files): with `agenda-archives` each
/// agenda file followed by the files it archives to (`org-add-archive-files`,
/// each once), then `extra`; a file that comes twice counts at its last
/// place (`cl-remove-duplicates`). `archives_of` gives the files a file
/// archives to, which `paths` must hold.
pub fn search_order<P: PartialEq + Clone>(
    paths: &[P],
    agenda: usize,
    archives_of: Option<&dyn Fn(usize) -> Vec<usize>>,
    extra: &[usize],
) -> Vec<usize> {
    let mut order: Vec<usize> = Vec::new();
    for index in 0..agenda {
        let mut here = vec![index];
        if let Some(archives_of) = archives_of {
            here.extend(archives_of(index));
        }
        for found in here {
            if !order.iter().any(|known| paths[*known] == paths[found]) {
                order.push(found);
            }
        }
    }
    order.extend(extra.iter().copied());
    // `cl-remove-duplicates` keeps the last of equal files.
    let mut kept: Vec<usize> = Vec::new();
    for (at, index) in order.iter().enumerate() {
        if !order[at + 1..].iter().any(|later| paths[*later] == paths[*index]) {
            kept.push(*index);
        }
    }
    kept
}

#[allow(clippy::too_many_arguments)]
pub fn render(
    query: Query,
    blocks: &[Block],
    files: &[(Uri, &Outline)],
    search: Option<&SearchFiles>,
    now: Now,
    clocked: Option<(&Uri, usize)>,
    filters: Option<&agenda::filter::Filters>,
    marks: &[EntryRef],
) -> View {
    let named: Vec<(&Outline, String, String)> = files
        .iter()
        .map(|(uri, outline)| {
            let path = crate::uri::to_path(uri).map(|path| path.display().to_string()).unwrap_or_default();
            (*outline, path, crate::uri::file_name(uri))
        })
        .collect();
    let (rendered, starts) = block_lines(blocks, &named, search, now, false, filters);
    assemble(query, rendered, starts, files, now, clocked, marks, Vec::new(), 0)
}

/// A change made from the agenda to an entry, which the lines of the
/// entry show where they are until the agenda is built again: Emacs
/// changes them, and does not move them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Held {
    /// `org-agenda-change-all-lines`: the entry's lines show its new
    /// headline, the state `keyword` (`done` or not) where no line of the
    /// view built anew stands in for them. With `only`, that line alone
    /// shows it: the state a repeater has since set back, on today's line
    /// (`org-agenda-headline-snapshot-before-repeat`).
    Headline { entry: EntryRef, keyword: Option<String>, done: bool, only: Option<usize> },
    /// `org-agenda-show-new-time`: the entry's lines for the timestamp at
    /// `entry.marker` show `text` at their end.
    Stamp { entry: EntryRef, text: String },
}

impl Held {
    /// The entry it changes.
    pub fn entry(&self) -> &EntryRef {
        match self {
            Held::Headline { entry, .. } | Held::Stamp { entry, .. } => entry,
        }
    }

    /// The entry it changes, which follows edits to its file.
    pub fn entry_mut(&mut self) -> &mut EntryRef {
        match self {
            Held::Headline { entry, .. } | Held::Stamp { entry, .. } => entry,
        }
    }
}

/// `view` with `held` shown on its lines, the rest of it as it was: the
/// agenda as Emacs keeps it after changes made from it. `fresh` is the
/// view built anew, whose lines stand in for those of a changed entry
/// where it has them; `files` are the agenda files as they read now, with
/// as many entries each as when `view` was built.
#[allow(clippy::too_many_arguments)]
pub fn hold(
    view: &View,
    fresh: &View,
    held: &[Held],
    files: &[(Uri, &Outline)],
    now: Now,
    clocked: Option<(&Uri, usize)>,
    marks: &[EntryRef],
    width: usize,
) -> View {
    let same = |a: &EntryRef, b: &EntryRef| a.uri == b.uri && a.offset == b.offset;
    let mut source = view.source.clone();
    // The timestamps where they are now, their files having changed.
    for ((_, line), shown) in source.iter_mut().zip(&view.lines) {
        if let (Some(item), Some(entry)) = (line.item.as_mut(), shown.entry.as_ref()) {
            item.marker = entry.marker;
        }
    }
    let mut stamps = view.stamps.clone();
    for change in held {
        for (number, shown) in view.lines.iter().enumerate() {
            let is_item = source[number].1.kind == LineKind::Item;
            let Some(entry) = shown.entry.as_ref().filter(|_| is_item) else { continue };
            match change {
                Held::Headline { entry: changed, keyword, done, only } => {
                    if !same(entry, changed) || only.is_some_and(|only| only != number) {
                        continue;
                    }
                    let of_entry = |(line, (_, rendered)): &(&ViewLine, &(usize, agenda::Line))| {
                        rendered.kind == LineKind::Item && line.entry.as_ref().is_some_and(|other| same(other, entry))
                    };
                    let mut lines = fresh.lines.iter().zip(&fresh.source).filter(of_entry);
                    let standing = lines.clone().find(|(line, _)| line.kind == shown.kind && line.day == shown.day);
                    source[number].1 = match (standing, lines.next()) {
                        _ if only.is_some() => with_keyword(&source[number].1, keyword.as_deref(), *done, None),
                        (Some((_, (_, rendered))), _) => rendered.clone(),
                        (None, other) => {
                            with_keyword(&source[number].1, keyword.as_deref(), *done, other.map(|(_, (_, line))| line))
                        }
                    };
                    // The line is written anew, without the date it showed.
                    stamps.retain(|(line, _)| *line != number);
                }
                Held::Stamp { entry: changed, text } => {
                    if same(entry, changed) && entry.marker == changed.marker {
                        stamps.retain(|(line, _)| *line != number);
                        stamps.push((number, text.clone()));
                    }
                }
            }
        }
    }
    assemble(view.query.clone(), source, view.blocks.clone(), files, now, clocked, marks, stamps, width)
}

/// `line` with its TODO keyword `keyword` in place of the one it has, its
/// tags where they ended: what `org-agenda-change-all-lines` makes of a
/// line no view built anew stands in for. With `other`, a line of the same
/// entry in that view, the whole headline is that line's (its priority and
/// title too), after this line's own prefix.
fn with_keyword(line: &agenda::Line, keyword: Option<&str>, done: bool, other: Option<&agenda::Line>) -> agenda::Line {
    let Some(item) = &line.item else { return line.clone() };
    let text = &line.text;
    let after_dots = |text: &str, heading: usize| {
        let heading = heading.min(text.len());
        heading + text[heading..].bytes().take_while(|b| *b == b'.').count()
    };
    let start = after_dots(text, item.heading);
    // A habit's graph stands on the headline: only the keyword changes.
    let other = other
        .filter(|_| item.graph.is_none())
        .and_then(|other| Some((other, other.item.as_ref().filter(|other| other.graph.is_none())?)));
    let (mut changed, delta, mut item) = if let Some((other, other_item)) = other {
        let tail = &other.text[after_dots(&other.text, other_item.heading)..];
        let changed = format!("{}{}", &text[..start], tail.trim_end_matches([' ', '\t']));
        let mut item = item.clone();
        item.priority = other_item.priority;
        (changed, 0, item)
    } else {
        // The old keyword and the blank after it, or nothing.
        let old_end = if item.keyword_len > 0 {
            let end = (start + item.keyword_len).min(text.len());
            end + usize::from(text[end..].starts_with(' '))
        } else {
            start
        };
        let new = keyword.map(|keyword| format!("{keyword} ")).unwrap_or_default();
        let changed = format!("{}{new}{}", &text[..start], &text[old_end..]);
        (changed, new.len() as isize - (old_end - start) as isize, item.clone())
    };
    // The tags end where they did, unless a habit's graph stands on them.
    if item.graph.is_none() {
        if let Some((_, tags_start)) = agenda::tag_group(text) {
            let end = text.trim_end_matches([' ', '\t']).len();
            let column = agenda::display_column(&text[..tags_start]) + agenda::display_column(&text[tags_start..end]);
            changed = agenda::align_tags(&changed, -(column as i64));
        }
    }
    if let Some((graph, _)) = item.graph.as_mut() {
        *graph = graph.saturating_add_signed(delta);
    }
    item.text = changed.clone();
    item.keyword_len = keyword.map_or(0, str::len);
    item.keyword_done = done;
    if done {
        item.face = agenda::Face::Done;
    } else if let Some((_, other_item)) = other.filter(|_| item.face == agenda::Face::Done) {
        item.face = other_item.face;
    }
    agenda::Line { text: changed, kind: line.kind, item: Some(item) }
}

/// `org-agenda-show-new-time`: `stamp` over the end of `shown`, from the
/// column before the one where it then ends at `width`, the line made
/// longer with blanks to reach it.
fn put_stamp(shown: &mut agenda::Displayed, stamp: &str, width: usize) {
    let keep = width.saturating_sub(stamp.chars().count()).max(1) - 1;
    let count = shown.text.chars().count();
    let cut = if count <= keep {
        shown.text.push_str(&" ".repeat(keep - count));
        shown.text.len()
    } else {
        shown.text.char_indices().nth(keep).map_or(shown.text.len(), |(at, _)| at)
    };
    shown.text.truncate(cut);
    shown.segments.retain(|segment| segment.start < cut);
    for segment in &mut shown.segments {
        segment.end = segment.end.min(cut);
    }
    shown.links.retain(|link| link.end <= cut);
    shown.text.push_str(stamp);
}

/// The view of `rendered`, the lines of the agenda's blocks: its text,
/// highlighting and entries, with `stamps` over the ends of their lines
/// for a window `width` columns wide.
#[allow(clippy::too_many_arguments)]
fn assemble(
    query: Query,
    rendered: Vec<(usize, agenda::Line)>,
    starts: Vec<Option<(Day, Span)>>,
    files: &[(Uri, &Outline)],
    now: Now,
    clocked: Option<(&Uri, usize)>,
    marks: &[EntryRef],
    stamps: Vec<(usize, String)>,
    width: usize,
) -> View {
    let outlines: Vec<&Outline> = files.iter().map(|(_, outline)| *outline).collect();
    let mut text = String::new();
    let mut lines: Vec<ViewLine> = Vec::with_capacity(rendered.len());
    let mut tokens = Vec::new();
    let mut today_line = None;
    // The agenda's today, which late at night is yesterday
    // (`org-extend-today-until`): the day the agenda marks as today.
    let mut today = now.today;
    let mut clocking_lines = Vec::new();
    let mut day = None;
    for (number, (block, line)) in rendered.iter().enumerate() {
        match line.kind {
            LineKind::Date { day: date, today: is_today, .. } => {
                day = Some(date);
                if is_today && today_line.is_none() {
                    today_line = Some(number as u32);
                    today = date;
                }
            }
            LineKind::Header | LineKind::Separator | LineKind::Table => day = None,
            // A group of org-super-agenda stays in its day.
            LineKind::Item | LineKind::EntryText | LineKind::Group { .. } | LineKind::Blank => {}
        }
        let entry = line.item.as_ref().and_then(|item| Some((item, item.source?))).map(|(item, source)| {
            let entry = agenda::entry(&outlines, source);
            EntryRef {
                uri: files[source.file].0.clone(),
                offset: entry.offset,
                head: entry.head.clone(),
                marker: item.marker,
            }
        });
        let mut shown = agenda::display(line);
        // Marked for a bulk action (`org-agenda-bulk-mark-char`).
        if entry.as_ref().is_some_and(|entry| marks.contains(entry)) {
            agenda::mark(&mut shown, ">");
        }
        if let Some((_, stamp)) = stamps.iter().find(|(line, _)| *line == number) {
            put_stamp(&mut shown, stamp, width);
        }
        let base = text.len();
        // The entry the clock runs on shows with `org-agenda-clocking`:
        // every part of its line, the gaps included.
        let clocking = line.item.as_ref().and_then(|item| item.source).is_some_and(|source| {
            clocked.is_some_and(|(uri, offset)| {
                files[source.file].0 == *uri && agenda::entry(&outlines, source).offset == offset
            })
        });
        if clocking {
            clocking_lines.push(number as u32);
        }
        let mut at = 0;
        for segment in &shown.segments {
            let range = |start: usize, end: usize| {
                TextRange::new(TextSize::new((base + start) as u32), TextSize::new((base + end) as u32))
            };
            if clocking && segment.start > at {
                tokens.push(Token { range: range(at, segment.start), ty: TokenType::AgendaClocking, modifiers: 0 });
            }
            at = segment.end;
            if let Some((ty, modifiers)) = token_style(segment.style) {
                let modifiers = if clocking { modifiers | modifier::CLOCKING } else { modifiers };
                tokens.push(Token { range: range(segment.start, segment.end), ty, modifiers });
            } else if clocking {
                tokens.push(Token {
                    range: range(segment.start, segment.end),
                    ty: TokenType::AgendaClocking,
                    modifiers: 0,
                });
            }
        }
        if clocking && shown.text.len() > at {
            let range =
                TextRange::new(TextSize::new((base + at) as u32), TextSize::new((base + shown.text.len()) as u32));
            tokens.push(Token { range, ty: TokenType::AgendaClocking, modifiers: 0 });
        }
        text.push_str(&shown.text);
        text.push('\n');
        let kind = line.item.as_ref().map(|item| item.kind);
        // Entry text belongs to the line of its entry, as Emacs shows it
        // after that line's end.
        let (entry, kind) = match (line.kind, lines.last()) {
            (LineKind::EntryText, Some(above)) => (above.entry.clone(), above.kind),
            _ => (entry, kind),
        };
        lines.push(ViewLine { entry, kind, day, block: *block });
    }
    let start = starts.iter().flatten().next().map_or(0, |(start, _)| *start);
    View {
        query,
        start,
        blocks: starts,
        index: LineIndex::new(&text),
        text,
        lines,
        tokens,
        clocking: clocking_lines,
        today_line,
        today,
        source: rendered,
        stamps,
    }
}

/// The text of the view `blocks` make, as Org prints it (`org-batch-agenda`):
/// links as written.
pub fn org_text(
    blocks: &[Block],
    files: &[(&Outline, String, String)],
    search: Option<&SearchFiles>,
    now: Now,
    filters: Option<&agenda::filter::Filters>,
) -> String {
    block_lines(blocks, files, search, now, true, filters).0.into_iter().map(|(_, line)| line.text + "\n").collect()
}

type BlockLines = (Vec<(usize, agenda::Line)>, Vec<Option<(Day, Span)>>);

/// The tags and the categories of the lines of the view `blocks` make,
/// without its filters, for `/` to tell them apart
/// (`org-agenda-get-represented-tags`, `-categories`).
pub fn represented(
    blocks: &[Block],
    files: &[(&Outline, String, String)],
    search: Option<&SearchFiles>,
    now: Now,
) -> (Vec<String>, Vec<String>) {
    let all: Vec<&Outline> = files.iter().map(|(outline, _, _)| *outline).collect();
    let lines: Vec<agenda::Line> =
        block_lines(blocks, files, search, now, false, None).0.into_iter().map(|(_, line)| line).collect();
    let view = agenda::View { lines, start: 0, end: 0 };
    let config = blocks.first().map(|block| block.config.clone()).unwrap_or_default();
    let agenda_count = search.map_or(all.len(), |search| search.agenda);
    let groups = if config.group_tags { agenda::tag_groups(&all[..agenda_count], &config) } else { Vec::new() };
    (agenda::filter::represented_tags(&view, &all, &groups), agenda::filter::represented_categories(&view, &all))
}

/// The filters `steps` set, one after the other, as the agenda's filter
/// commands set them: `{"type": "tag", "filter": ["+work"]}` (also
/// `category`, `regexp`, `effort`; `"expand": false` for tags not
/// expanded), `{"string": "+work<0:10", "negate": false}` (`/`),
/// `{"top": "Projects", "negative": false}` (`^`), `{"remove": true}`
/// (`|`).
pub fn filter_steps(
    steps: &serde_json::Value,
    represented: &(Vec<String>, Vec<String>),
) -> Result<agenda::filter::Filters, String> {
    let mut filters = agenda::filter::Filters::default();
    let strings = |value: &serde_json::Value| -> Vec<String> {
        value.as_array().into_iter().flatten().filter_map(|item| item.as_str().map(str::to_string)).collect()
    };
    for step in steps.as_array().ok_or("--filters must be a list")? {
        if step.get("remove").and_then(serde_json::Value::as_bool) == Some(true) {
            filters = agenda::filter::Filters::default();
        } else if let Some(string) = step.get("string").and_then(serde_json::Value::as_str) {
            let negate = step.get("negate").and_then(serde_json::Value::as_bool).unwrap_or(false);
            filters = agenda::filter::parse(string, negate, &filters, &represented.0, &represented.1).0;
        } else if let Some(top) = step.get("top").and_then(serde_json::Value::as_str) {
            let negative = step.get("negative").and_then(serde_json::Value::as_bool).unwrap_or(false);
            filters.top_headline = Some((top.to_string(), negative));
        } else {
            let list = strings(&step["filter"]);
            match step.get("type").and_then(serde_json::Value::as_str) {
                Some("tag") => {
                    filters.tag = list;
                    filters.expand_tags = step.get("expand").and_then(serde_json::Value::as_bool).unwrap_or(true);
                }
                Some("category") => filters.category = list,
                Some("regexp") => filters.regexp = list,
                Some("effort") => filters.effort = list,
                other => return Err(format!("unknown filter step {other:?}")),
            }
        }
    }
    Ok(filters)
}

/// The lines of every block, with the block each belongs to, and the
/// days and span of the blocks that are agendas. Emacs leaves a blank line
/// above the separator between blocks; `blank` keeps it, as the agenda
/// printed in a terminal does. The editor's agenda goes without.
fn block_lines(
    blocks: &[Block],
    files: &[(&Outline, String, String)],
    search: Option<&SearchFiles>,
    now: Now,
    blank: bool,
    filters: Option<&agenda::filter::Filters>,
) -> BlockLines {
    let all: Vec<&Outline> = files.iter().map(|(outline, _, _)| *outline).collect();
    let agenda_count = search.map_or(all.len(), |search| search.agenda);
    let outlines = &all[..agenda_count];
    let order: Vec<usize> = search.map_or_else(|| (0..all.len()).collect(), |search| search.search.clone());
    let searched: Vec<&Outline> = order.iter().map(|index| all[*index]).collect();
    let mut rendered: Vec<(usize, agenda::Line)> = Vec::new();
    let mut starts = Vec::new();
    let paths: Vec<String> = files.iter().map(|(_, path, _)| path.clone()).collect();
    for (number, block) in blocks.iter().enumerate() {
        // The files' paths, for what looks at them (org-super-agenda).
        let config = &agenda::Config { file_paths: paths.clone(), ..block.config.clone() };
        let mut view = match &block.kind {
            BlockKind::Agenda { start, span } => agenda::view(outlines, now, *start, *span, config),
            BlockKind::Todo { keywords } => agenda::todo_view(outlines, keywords.as_deref(), now, config),
            BlockKind::Tags { matcher, todo_only } => agenda::tags_view(outlines, matcher, *todo_only, now, config),
            // A setting Org cannot use leaves the view with what it says.
            BlockKind::Stuck => agenda::stuck_view(outlines, now, config).unwrap_or_else(|error| agenda::View {
                lines: vec![agenda::Line { text: error, kind: agenda::LineKind::Header, item: None }],
                start: 0,
                end: 0,
            }),
            // A search string Org cannot read leaves the view empty; the
            // command that asked for it says why.
            BlockKind::Search { string, todo_only } => {
                let config = &agenda::Config {
                    file_paths: order.iter().map(|index| paths[*index].clone()).collect(),
                    ..config.clone()
                };
                agenda::search_view(&searched, string, *todo_only, now, config)
                    .map(|mut view| {
                        // Back to the files of the whole view.
                        for line in &mut view.lines {
                            if let Some(source) = line.item.as_mut().and_then(|item| item.source.as_mut()) {
                                source.file = order[source.file];
                            }
                        }
                        view
                    })
                    .unwrap_or_else(|_| agenda::View {
                        lines: vec![agenda::Line {
                            text: format!("Search words: {string}"),
                            kind: agenda::LineKind::Header,
                            item: None,
                        }],
                        start: 0,
                        end: 0,
                    })
            }
        };
        // The lines the filters keep out (`org-agenda-filter-apply`).
        if let Some(filters) = filters.filter(|filters| !filters.is_empty()) {
            let groups = if config.group_tags { agenda::tag_groups(outlines, config) } else { Vec::new() };
            agenda::filter::apply(&mut view, &all, filters, &groups, config.sorting.noeffort_is_high);
            if config.super_agenda.as_ref().is_some_and(|settings| settings.hide_empty_groups) {
                agenda::super_agenda::hide_empty_groups(&mut view.lines);
            }
        }
        // `org-agenda-entry-text-mode`: the text of each entry under its
        // line.
        if let Some(count) = config.entry_text {
            let mut with_text = Vec::with_capacity(view.lines.len());
            for line in view.lines.drain(..) {
                let source = line.item.as_ref().and_then(|item| item.source);
                with_text.push(line);
                let Some(source) = source else { continue };
                for text in
                    agenda::entry_text::entry_text(all[source.file], source.entry, count, &config.entry_text_settings)
                {
                    with_text.push(agenda::Line { text, kind: LineKind::EntryText, item: None });
                }
            }
            view.lines = with_text;
        }
        let agenda = matches!(block.kind, BlockKind::Agenda { .. });
        // A compact agenda block has no header, not even its own.
        if let Some(header) = block.header.as_ref().filter(|_| !(agenda && config.compact_blocks)) {
            agenda::override_header(&mut view, header);
        }
        starts.push(match block.kind {
            BlockKind::Agenda { span, .. } => Some((view.start, span)),
            _ => None,
        });
        let entries = view.lines.iter().any(|line| line.item.as_ref().is_some_and(|item| item.source.is_some()));
        if blocks.len() > 1 && config.hide_empty_blocks && !entries {
            continue;
        }
        // Between this block and what is above it.
        if !rendered.is_empty() {
            let separator = agenda::block_separator(config);
            let skip = usize::from(!blank && separator.first().is_some_and(|line| line.text.is_empty()));
            rendered.extend(separator.into_iter().skip(skip).map(|line| (number, line)));
        }
        let (start, end) = (view.start, view.end);
        rendered.extend(view.lines.into_iter().map(|line| (number, line)));
        // `org-agenda-clockreport-mode`: the clock table for the days shown.
        if config.clockreport && agenda {
            let named: Vec<(&Outline, &str, &str)> = files[..agenda_count]
                .iter()
                .map(|(outline, path, name)| (*outline, path.as_str(), name.as_str()))
                .collect();
            for text in agenda::report::clock_table(&named, start, end) {
                rendered.push((number, agenda::Line { text, kind: LineKind::Table, item: None }));
            }
        }
    }
    (rendered, starts)
}

impl View {
    pub fn line(&self, line: u32) -> Option<&ViewLine> {
        self.lines.get(line as usize)
    }

    /// The view without the lines `hidden` (by number), as Org hides them
    /// (`org-agenda-filter-hide-line`): each with the line break before it.
    pub fn hide_lines(&mut self, hidden: &[usize]) {
        if hidden.is_empty() {
            return;
        }
        let old: Vec<&str> = self.text.split('\n').collect();
        let mut starts = Vec::with_capacity(old.len());
        let mut at = 0;
        for line in &old {
            starts.push(at);
            at += line.len() + 1;
        }
        // Each line's new number and start, if it stays.
        let mut moved: Vec<Option<(u32, usize)>> = Vec::with_capacity(old.len());
        let mut kept: Vec<&str> = Vec::new();
        let mut start = 0;
        for (number, line) in old.iter().enumerate() {
            if hidden.contains(&number) {
                moved.push(None);
                continue;
            }
            moved.push(Some((kept.len() as u32, start)));
            kept.push(line);
            start += line.len() + 1;
        }
        let text = kept.join("\n");
        let line_of = |offset: usize| starts.partition_point(|start| *start <= offset).saturating_sub(1);
        self.tokens.retain_mut(|token| {
            let (from, to) = (usize::from(token.range.start()), usize::from(token.range.end()));
            let line = line_of(from);
            let Some((_, new_start)) = moved[line] else { return false };
            let shift = |offset: usize| (offset - starts[line] + new_start) as u32;
            token.range = org_syntax::TextRange::new(shift(from).into(), shift(to).into());
            true
        });
        let renumber = |line: u32| moved.get(line as usize).copied().flatten().map(|(number, _)| number);
        self.clocking = self.clocking.iter().filter_map(|line| renumber(*line)).collect();
        self.today_line = self.today_line.and_then(renumber);
        self.stamps = std::mem::take(&mut self.stamps)
            .into_iter()
            .filter_map(|(line, stamp)| Some((renumber(line as u32)? as usize, stamp)))
            .collect();
        let mut number = 0;
        self.lines.retain(|_| {
            number += 1;
            !hidden.contains(&(number - 1))
        });
        let mut number = 0;
        self.source.retain(|_| {
            number += 1;
            !hidden.contains(&(number - 1))
        });
        self.index = LineIndex::new(&text);
        self.text = text;
    }

    /// The line the cursor should go to when the view is first shown:
    /// today, or the first day.
    pub fn home_line(&self) -> u32 {
        self.today_line.unwrap_or(if self.lines.len() > 1 { 1 } else { 0 })
    }

    /// The days block `block` shows, in order.
    fn days_of(&self, block: usize) -> Vec<Day> {
        let mut days: Vec<Day> = Vec::new();
        for day in self.lines.iter().filter(|line| line.block == block).filter_map(|line| line.day) {
            if days.last() != Some(&day) {
                days.push(day);
            }
        }
        days
    }

    /// `org-day-cnt`: which of the days its block shows line `line` stands
    /// under, counted from 0; none on a line under no day.
    pub fn day_count(&self, line: u32) -> Option<usize> {
        let line = self.line(line)?;
        let day = line.day?;
        self.days_of(line.block).iter().position(|shown| *shown == day)
    }

    /// The line the cursor goes to in block `block` once it has moved to
    /// other days: the first line of `day` if the block shows it, else of
    /// its `count`th day (`org-agenda-find-same-or-today-or-agenda`: the
    /// same place in the new days), else today's, else the block's first
    /// day.
    pub fn block_line(&self, block: usize, day: Option<Day>, count: Option<usize>) -> u32 {
        let lines = || self.lines.iter().enumerate().filter(|(_, line)| line.block == block);
        let first_of = |wanted: Day| lines().find(|(_, line)| line.day == Some(wanted)).map(|(number, _)| number);
        day.and_then(first_of)
            .or_else(|| count.and_then(|count| self.days_of(block).get(count).copied()).and_then(first_of))
            .or_else(|| first_of(self.today))
            .or_else(|| lines().find(|(_, line)| line.day.is_some()).map(|(number, _)| number))
            .map_or_else(|| self.home_line(), |number| number as u32)
    }
}

/// Where the entry of `reference` is now in `outline`: at the same place
/// if the headline there still reads the same, else at the nearest headline
/// that does.
pub fn resolve(reference: &EntryRef, outline: &Outline) -> Option<usize> {
    let same = |index: &usize| outline.entries[*index].head == reference.head;
    let at = outline.entries.partition_point(|entry| entry.offset < reference.offset);
    if at < outline.entries.len() && outline.entries[at].offset == reference.offset && same(&at) {
        return Some(at);
    }
    (0..outline.entries.len())
        .filter(same)
        .min_by_key(|index| outline.entries[*index].offset.abs_diff(reference.offset))
}

#[cfg(test)]
mod tests {
    use org_core::date::Date;
    use org_syntax::TodoKeywords;

    use super::*;

    fn outline(text: &str) -> Outline {
        Outline::new(&org_syntax::parse(text, &TodoKeywords::default()), text, "notes")
    }

    #[test]
    fn renders_lines_tokens_and_entries() {
        let text = "* TODO [#A] Call [[https://example.com][them]] :work:\nSCHEDULED: <2026-05-18 Mon>\n* Later\n<2026-05-20 Wed>\n";
        let outline = outline(text);
        let uri: Uri = "file:///tmp/notes.org".parse().unwrap();
        let now = Now { today: Date::new(2026, 5, 18).to_day(), minute: 600 };
        let block = Block {
            kind: BlockKind::Agenda { start: None, span: Span::Week },
            header: None,
            config: Config::default(),
        };
        let view = render(
            Query::Agenda { start: None, span: Span::Week },
            &[block],
            &[(uri.clone(), &outline)],
            None,
            now,
            None,
            None,
            &[],
        );
        let lines: Vec<&str> = view.text.lines().collect();
        assert_eq!(lines[0], "Week-agenda (W21):");
        assert_eq!(lines[1], "Monday     18 May 2026 W21");
        // The link is folded and the tags still end at column 80.
        assert_eq!(lines[2], "  notes:      Scheduled:  TODO [#A] Call them                             :work:");
        assert_eq!(view.today_line, Some(1));
        assert_eq!(view.home_line(), 1);
        assert_eq!(view.lines[0], ViewLine::default());
        assert_eq!(view.lines[1].day, Some(now.today));
        let entry = view.lines[2].entry.as_ref().unwrap();
        assert_eq!((entry.uri.as_str(), entry.offset), (uri.as_str(), 0));
        // The line stands for the `SCHEDULED:` timestamp.
        assert_eq!((view.lines[2].kind, entry.marker), (Some(ItemKind::Scheduled), text.find('<')));
        assert_eq!(view.lines[2].day, Some(now.today));

        let described: Vec<(&str, TokenType, u32)> = view
            .tokens
            .iter()
            .filter(|token| view.index.line_of(token.range.start().into()) == 2)
            .map(|token| (&view.text[token.range], token.ty, token.modifiers))
            .collect();
        assert_eq!(
            described,
            [
                ("  notes:      Scheduled:  ", TokenType::AgendaScheduled, modifier::TODAY),
                ("TODO", TokenType::TodoKeyword, 0),
                (" ", TokenType::AgendaScheduled, modifier::TODAY),
                ("[#A]", TokenType::Priority, 0),
                (" Call ", TokenType::AgendaScheduled, modifier::TODAY),
                ("them", TokenType::Link, 0),
                ("                             ", TokenType::AgendaScheduled, modifier::TODAY),
                (":work:", TokenType::Tag, 0),
            ]
        );
    }

    #[test]
    fn finds_an_entry_again_after_edits() {
        let before = outline("* a\n* TODO b\n* c\n");
        let reference =
            EntryRef { uri: "file:///x.org".parse().unwrap(), offset: 4, head: "TODO b".to_string(), marker: None };
        assert_eq!(resolve(&reference, &before), Some(1));
        // Lines were added above it.
        assert_eq!(resolve(&reference, &outline("* new\n* a\n* TODO b\n* c\n")), Some(2));
        // Two candidates: the nearer one.
        assert_eq!(resolve(&reference, &outline("* TODO b\n* x\n* y\n* z\n* TODO b\n")), Some(0));
        // Its headline changed: it is gone.
        assert_eq!(resolve(&reference, &outline("* a\n* DONE b\n")), None);
    }
}
