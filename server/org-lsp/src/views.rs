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
    /// A command of `agenda.customCommands`, by its keys. One of its
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
    /// The line of today's date, if it is shown.
    pub today_line: Option<u32>,
    pub today: Day,
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
            Face::CalendarEvent => (TokenType::AgendaEvent, 0),
            Face::CalendarDateRange => (TokenType::AgendaEvent, modifier::RANGE),
            Face::Done => (TokenType::AgendaDone, 0),
            Face::TimeGrid => (TokenType::AgendaTimeGrid, 0),
            Face::CurrentTime => (TokenType::AgendaTimeGrid, modifier::CURRENT),
            Face::Default => return None,
        },
    })
}

/// Build the view for `query` from the agenda files, given as their URIs
/// and outlines in agenda order, as `blocks` say: one for a plain agenda
/// or TODO list, more for a block agenda, with a separator between them.
pub fn render(
    query: Query,
    blocks: &[Block],
    files: &[(Uri, &Outline)],
    now: Now,
    clocked: Option<(&Uri, usize)>,
) -> View {
    let outlines: Vec<&Outline> = files.iter().map(|(_, outline)| *outline).collect();
    let named: Vec<(&Outline, String, String)> = files
        .iter()
        .map(|(uri, outline)| {
            let path = crate::uri::to_path(uri).map(|path| path.display().to_string()).unwrap_or_default();
            (*outline, path, crate::uri::file_name(uri))
        })
        .collect();
    let (rendered, starts) = block_lines(blocks, &named, now);
    let mut text = String::new();
    let mut lines = Vec::with_capacity(rendered.len());
    let mut tokens = Vec::new();
    let mut today_line = None;
    let mut day = None;
    for (number, (block, line)) in rendered.iter().enumerate() {
        match line.kind {
            LineKind::Date { day: date, today, .. } => {
                day = Some(date);
                if today && today_line.is_none() {
                    today_line = Some(number as u32);
                }
            }
            LineKind::Header | LineKind::Separator | LineKind::Table => day = None,
            LineKind::Item => {}
        }
        let shown = agenda::display(line);
        let base = text.len();
        // The entry the clock runs on shows with `org-agenda-clocking`:
        // every part of its line, the gaps included.
        let clocking = line.item.as_ref().and_then(|item| item.source).is_some_and(|source| {
            clocked.is_some_and(|(uri, offset)| {
                files[source.file].0 == *uri && agenda::entry(&outlines, source).offset == offset
            })
        });
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
        let entry = line.item.as_ref().and_then(|item| Some((item, item.source?))).map(|(item, source)| {
            let entry = agenda::entry(&outlines, source);
            EntryRef {
                uri: files[source.file].0.clone(),
                offset: entry.offset,
                head: entry.head.clone(),
                marker: item.marker,
            }
        });
        let kind = line.item.as_ref().map(|item| item.kind);
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
        today_line,
        today: now.today,
    }
}

/// The text of the view `blocks` make, as Org prints it (`org-batch-agenda`):
/// links as written.
pub fn org_text(blocks: &[Block], files: &[(&Outline, String, String)], now: Now) -> String {
    block_lines(blocks, files, now).0.into_iter().map(|(_, line)| line.text + "\n").collect()
}

type BlockLines = (Vec<(usize, agenda::Line)>, Vec<Option<(Day, Span)>>);

/// The lines of every block, with the block each belongs to, and the
/// days and span of the blocks that are agendas.
fn block_lines(blocks: &[Block], files: &[(&Outline, String, String)], now: Now) -> BlockLines {
    let outlines: Vec<&Outline> = files.iter().map(|(outline, _, _)| *outline).collect();
    let outlines = outlines.as_slice();
    let mut rendered: Vec<(usize, agenda::Line)> = Vec::new();
    let mut starts = Vec::new();
    for (number, block) in blocks.iter().enumerate() {
        let config = &block.config;
        let mut view = match &block.kind {
            BlockKind::Agenda { start, span } => agenda::view(outlines, now, *start, *span, config),
            BlockKind::Todo { keywords } => agenda::todo_view(outlines, keywords.as_deref(), config),
            BlockKind::Tags { matcher, todo_only } => agenda::tags_view(outlines, matcher, *todo_only, now, config),
        };
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
            rendered.extend(agenda::block_separator(config).into_iter().map(|line| (number, line)));
        }
        let (start, end) = (view.start, view.end);
        rendered.extend(view.lines.into_iter().map(|line| (number, line)));
        // `org-agenda-clockreport-mode`: the clock table for the days shown.
        if config.clockreport && agenda {
            let named: Vec<(&Outline, &str, &str)> =
                files.iter().map(|(outline, path, name)| (*outline, path.as_str(), name.as_str())).collect();
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

    /// The line that shows the entry at `offset` of `uri` the way line
    /// `from` did before the view was rebuilt: for the same reason and
    /// under the same day if there is such a line, or else the nearest
    /// one.
    pub fn line_of(&self, uri: &Uri, offset: usize, from: (u32, &ViewLine)) -> Option<u32> {
        let (number, old) = from;
        let shows =
            |line: &ViewLine| line.entry.as_ref().is_some_and(|entry| entry.uri == *uri && entry.offset == offset);
        let candidates = || self.lines.iter().enumerate().filter(|(_, line)| shows(line));
        let nearest = |lines: &mut dyn Iterator<Item = (usize, &ViewLine)>| {
            lines.min_by_key(|(index, _)| index.abs_diff(number as usize)).map(|(index, _)| index as u32)
        };
        nearest(&mut candidates().filter(|(_, line)| line.kind == old.kind && line.day == old.day))
            .or_else(|| nearest(&mut candidates().filter(|(_, line)| line.kind == old.kind)))
            .or_else(|| nearest(&mut candidates()))
    }

    /// The line the cursor should go to when the view is first shown:
    /// today, or the first day.
    pub fn home_line(&self) -> u32 {
        self.today_line.unwrap_or(if self.lines.len() > 1 { 1 } else { 0 })
    }

    /// The line the cursor goes to in block `block` once it has moved to
    /// other days: the first line of `day` if the block shows it, else
    /// today's, else the block's first day.
    pub fn block_line(&self, block: usize, day: Option<Day>) -> u32 {
        let lines = || self.lines.iter().enumerate().filter(|(_, line)| line.block == block);
        let first_of = |wanted: Day| lines().find(|(_, line)| line.day == Some(wanted)).map(|(number, _)| number);
        day.and_then(first_of)
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
        let view =
            render(Query::Agenda { start: None, span: Span::Week }, &[block], &[(uri.clone(), &outline)], now, None);
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
