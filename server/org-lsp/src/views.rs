//! The agenda as a document the server owns.
//!
//! A view is rendered from the agenda files into plain text. The server
//! remembers, for every line, the entry it came from, so that requests on
//! the view (go to definition, code actions, commands) reach the entry in
//! its file. Highlighting comes from semantic tokens, as for Org files.

use lsp_types::Uri;
use org_core::agenda::{self, Config, Face, LineKind, Now, Span, Style};
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Query {
    /// The agenda for `span`, including the day `start`, or today.
    Agenda { start: Option<Day>, span: Span },
    /// The global TODO list.
    Todo,
}

/// The entry behind a line, in a form that survives edits to its file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryRef {
    pub uri: Uri,
    /// Byte offset of the headline when the view was built.
    pub offset: usize,
    /// The headline text, to find the entry again if it has moved.
    pub head: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ViewLine {
    pub entry: Option<EntryRef>,
    /// The day the line stands under, in an agenda.
    pub day: Option<Day>,
}

/// A rendered view.
#[derive(Debug)]
pub struct View {
    pub query: Query,
    /// The first day shown, in an agenda.
    pub start: Day,
    pub text: String,
    pub index: LineIndex,
    pub lines: Vec<ViewLine>,
    pub tokens: Vec<Token>,
    /// The line of today's date, if it is shown.
    pub today_line: Option<u32>,
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
/// and outlines in agenda order.
pub fn render(query: Query, files: &[(Uri, &Outline)], now: Now, config: &Config) -> View {
    let outlines: Vec<&Outline> = files.iter().map(|(_, outline)| *outline).collect();
    let rendered = match query {
        Query::Agenda { start, span } => agenda::view(&outlines, now, start, span, config),
        Query::Todo => agenda::todo_view(&outlines, config),
    };

    let mut text = String::new();
    let mut lines = Vec::with_capacity(rendered.lines.len());
    let mut tokens = Vec::new();
    let mut today_line = None;
    let mut day = None;
    for (number, line) in rendered.lines.iter().enumerate() {
        if let LineKind::Date { day: date, today, .. } = line.kind {
            day = Some(date);
            if today {
                today_line = Some(number as u32);
            }
        }
        let shown = agenda::display(line);
        let base = text.len();
        for segment in &shown.segments {
            if let Some((ty, modifiers)) = token_style(segment.style) {
                let range = TextRange::new(
                    TextSize::new((base + segment.start) as u32),
                    TextSize::new((base + segment.end) as u32),
                );
                tokens.push(Token { range, ty, modifiers });
            }
        }
        text.push_str(&shown.text);
        text.push('\n');
        let entry = line.item.as_ref().and_then(|item| item.source).map(|source| {
            let entry = agenda::entry(&outlines, source);
            EntryRef { uri: files[source.file].0.clone(), offset: entry.offset, head: entry.head.clone() }
        });
        lines.push(ViewLine { entry, day: if line.kind == LineKind::Header { None } else { day } });
    }
    View { query, start: rendered.start, index: LineIndex::new(&text), text, lines, tokens, today_line }
}

impl View {
    pub fn line(&self, line: u32) -> Option<&ViewLine> {
        self.lines.get(line as usize)
    }

    /// The line the cursor should go to when the view is first shown:
    /// today, or the first day.
    pub fn home_line(&self) -> u32 {
        self.today_line.unwrap_or(if self.lines.len() > 1 { 1 } else { 0 })
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
        let view = render(
            Query::Agenda { start: None, span: Span::Week },
            &[(uri.clone(), &outline)],
            now,
            &Config::default(),
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
        let reference = EntryRef { uri: "file:///x.org".parse().unwrap(), offset: 4, head: "TODO b".to_string() };
        assert_eq!(resolve(&reference, &before), Some(1));
        // Lines were added above it.
        assert_eq!(resolve(&reference, &outline("* new\n* a\n* TODO b\n* c\n")), Some(2));
        // Two candidates: the nearer one.
        assert_eq!(resolve(&reference, &outline("* TODO b\n* x\n* y\n* z\n* TODO b\n")), Some(0));
        // Its headline changed: it is gone.
        assert_eq!(resolve(&reference, &outline("* a\n* DONE b\n")), None);
    }
}
