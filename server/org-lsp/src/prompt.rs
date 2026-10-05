//! Org's date prompt as a document the server owns, for editors that can
//! show one: the calendar Emacs pops up, and below it the minibuffer line
//! with what was typed and what it means.
//!
//! ```text
//!         September 2026            October 2026            November 2026
//!      Su Mo Tu We Th Fr Sa     Su Mo Tu We Th Fr Sa     Su Mo Tu We Th Fr Sa
//!             1  2  3  4  5                  1  2  3      1  2  3  4  5  6  7
//!       6  7  8  9 10 11 12      4  5  6  7  8  9 10      8  9 10 11 12 13 14
//!   …
//!
//! SCHEDULED Date+time [2026-10-05]: 10:00    => <2026-10-09 Fri 10:00>
//! ```
//!
//! The calendar has a selected day, which the editor's cursor is put on.
//! As in Emacs, the answer is what was typed, followed by the selected day
//! once it was moved (`org-read-date`'s `org-ans2`) or picked
//! (`org-calendar-select`'s `org-ans1`).

use lsp_types::{Position, Uri};
use org_core::calendar::Calendar;
use org_core::date::{Date, DateTime, Day};
use org_core::planning::{self, Command};
use org_core::read_date::Settings;
use org_syntax::{TextRange, TextSize};

use crate::highlight::{modifier, Token, TokenType};
use crate::views::ViewLine;

/// The prompt document. It shares the agenda's scheme, so that clients
/// fetch it the same way.
pub const URI: &str = "org-agenda:/Org%20Date";
/// The language of the prompt document.
pub const LANGUAGE: &str = "org-date";

/// Where the command that asked for a date acts.
#[derive(Debug, Clone)]
pub enum Target {
    /// A place in an Org file.
    Place { uri: Uri, position: Position },
    /// The entry of a line of the agenda `view`, as `org-agenda-schedule`
    /// finds it: the command runs in the entry's file, where the line's
    /// marker points. `line` and `character` are where the cursor was, so
    /// that it can follow the entry.
    Entry { view: Uri, line: u32, character: u32, row: ViewLine },
    /// The agenda `view` itself: the answer is the day to show
    /// (`org-agenda-goto-date`).
    Agenda { view: Uri },
}

impl Target {
    /// The document the command was run from.
    pub fn from(&self) -> &Uri {
        match self {
            Target::Place { uri, .. } => uri,
            Target::Entry { view, .. } | Target::Agenda { view } => view,
        }
    }
}

/// A date prompt that waits for its answer.
pub struct DatePrompt {
    /// The command that asked.
    pub id: String,
    pub command: Command,
    /// Where the command acts.
    pub target: Target,
    /// The date the answer is read against (DEFAULT-TIME).
    pub default: Option<DateTime>,
    pub label: String,
    /// What was typed so far: at first what the prompt is filled with.
    pub typed: String,
    pub selected: Day,
    /// The selected day was moved, so that the answer ends with it.
    pub moved: bool,
    pub calendar: Calendar,
    pub text: String,
    pub tokens: Vec<Token>,
    /// The cursor is to go to the selected day once the client shows the
    /// current text.
    pub reveal: bool,
    /// The client has been told to put the cursor on the selected day and
    /// has not said yet that it has: the cursor it reports is an old one.
    pub moving_cursor: bool,
}

impl DatePrompt {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: &str,
        command: Command,
        target: Target,
        default: Option<DateTime>,
        typed: String,
        now: DateTime,
        settings: &Settings,
    ) -> DatePrompt {
        let selected = default.unwrap_or(now).day;
        let mut prompt = DatePrompt {
            id: id.to_string(),
            command,
            target,
            default,
            label: planning::label(command, default, now),
            typed,
            selected,
            moved: false,
            calendar: Calendar::around(selected),
            text: String::new(),
            tokens: Vec::new(),
            reveal: true,
            moving_cursor: false,
        };
        prompt.render(now, settings);
        prompt
    }

    /// The answer: what was typed, and the selected day once it moved.
    pub fn answer(&self) -> String {
        if self.moved {
            self.answer_with(self.selected)
        } else {
            self.typed.clone()
        }
    }

    /// The answer with `day` picked in the calendar.
    pub fn answer_with(&self, day: Day) -> String {
        format!("{} {}", self.typed, Date::from_day(day))
    }

    /// Select `day`, showing the months around it if it is not shown.
    pub fn select(&mut self, day: Day, now: DateTime, settings: &Settings) {
        self.selected = day;
        self.moved = true;
        if !self.calendar.shows(day) {
            self.calendar = Calendar::around(day);
        }
        self.render(now, settings);
        self.reveal = true;
    }

    /// Show the months `by` months on; the selected day stays if it is
    /// still shown, or else goes to the first of the middle month
    /// (`calendar-scroll-left`).
    pub fn scroll(&mut self, by: i64, now: DateTime, settings: &Settings) {
        self.calendar = self.calendar.scrolled(by);
        if !self.calendar.shows(self.selected) {
            self.selected = Date::new(self.calendar.year, self.calendar.month, 1).to_day();
        }
        self.moved = true;
        self.render(now, settings);
        self.reveal = true;
    }

    /// What was typed changed; the selected day did not, and the cursor
    /// stays where it is.
    pub fn set_typed(&mut self, typed: String, now: DateTime, settings: &Settings) {
        self.typed = typed;
        self.render(now, settings);
    }

    /// The day at `position` of the document, if the cursor is on one.
    pub fn day_at(&self, position: Position) -> Option<Day> {
        self.calendar.day_at(position.line as usize, position.character as usize)
    }

    /// Where the cursor goes for the selected day: on its last digit, as
    /// in Emacs's calendar. The calendar is plain ASCII, so a column is
    /// the same in every position encoding.
    pub fn cursor(&self) -> Position {
        let cell = self.calendar.cell(self.selected).expect("the selected day is shown");
        Position::new(cell.line as u32, cell.column as u32 + 1)
    }

    fn render(&mut self, now: DateTime, settings: &Settings) {
        let mut text = String::new();
        let mut tokens = Vec::new();
        let mut token = |start: usize, len: usize, ty: TokenType, modifiers: u32| {
            let range = TextRange::at(TextSize::from(start as u32), TextSize::from(len as u32));
            tokens.push(Token { range, ty, modifiers });
        };
        for (number, line) in self.calendar.lines.iter().enumerate() {
            let start = text.len();
            match number {
                0 => {
                    for (at, header) in month_headers(line) {
                        token(start + at, header, TokenType::CalendarHeader, 0);
                    }
                }
                1 => {
                    for (index, at) in
                        line.match_indices(|c: char| c.is_ascii_alphabetic()).step_by(2).map(|(at, _)| at).enumerate()
                    {
                        let weekend = matches!(index % 7, 0 | 6);
                        token(start + at, 2, TokenType::CalendarWeekday, if weekend { modifier::WEEKEND } else { 0 });
                    }
                }
                _ => {}
            }
            text.push_str(line);
            text.push('\n');
        }
        if let Some(cell) = self.calendar.cell(self.selected) {
            let start: usize = self.calendar.lines[..cell.line].iter().map(|line| line.len() + 1).sum();
            token(start + cell.column, 2, TokenType::CalendarSelected, 0);
        }
        // The minibuffer, after a blank line.
        text.push('\n');
        let start = text.len();
        token(start, self.label.len(), TokenType::CalendarPrompt, 0);
        let mut line = format!("{}{}", self.label, self.typed);
        // `org-read-date-display` pads the answer with four blanks.
        while !line.ends_with("    ") {
            line.push(' ');
        }
        let display = planning::display(self.command, &self.answer(), self.default, now, settings);
        if let Some(at) = display.find(['<', '[']) {
            let end = display[at..].find(['>', ']']).map_or(display.len(), |end| at + end + 1);
            token(start + line.len() + at, end - at, TokenType::Timestamp, 0);
        }
        line.push_str(&display);
        text.push_str(line.trim_end());
        text.push('\n');
        self.text = text;
        self.tokens = tokens;
    }
}

/// Where the month headers of the first calendar line are, and how long.
fn month_headers(line: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(start) = line[at..].find(|c: char| c != ' ').map(|offset| at + offset) {
        // A header is a month name, a blank and a year.
        let name_end = line[start..].find(' ').map_or(line.len(), |offset| start + offset);
        let end = line[name_end + 1..].find(' ').map_or(line.len(), |offset| name_end + 1 + offset);
        out.push((start, end - start));
        at = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime {
        DateTime::new(Date::new(2026, 10, 5), 10, 30)
    }

    fn prompt() -> DatePrompt {
        let file: Uri = "file:///org/notes.org".parse().unwrap();
        let default = Some(DateTime::new(Date::new(2026, 10, 7), 10, 0));
        DatePrompt::new(
            "org.schedule",
            Command::Schedule,
            Target::Place { uri: file, position: Position::new(0, 0) },
            default,
            "10:00".to_string(),
            now(),
            &Settings::default(),
        )
    }

    #[test]
    fn shows_the_calendar_and_the_minibuffer() {
        let prompt = prompt();
        let lines: Vec<&str> = prompt.text.lines().collect();
        assert_eq!(lines[0], "        September 2026            October 2026            November 2026");
        assert_eq!(lines[lines.len() - 2], "");
        assert_eq!(lines[lines.len() - 1], "SCHEDULED Date+time [2026-10-07]: 10:00    => <2026-10-07 Wed 10:00>");
        // The cursor goes on the selected day's last digit.
        assert_eq!(prompt.cursor(), Position::new(3, 40));
        assert_eq!(&lines[3][39..41], " 7");
        assert!(prompt.tokens.iter().any(|token| token.ty == TokenType::CalendarSelected));
    }

    #[test]
    fn the_answer_ends_with_the_day_once_it_moved() {
        let mut prompt = prompt();
        assert_eq!(prompt.answer(), "10:00");
        let friday = Date::new(2026, 10, 9).to_day();
        prompt.select(friday, now(), &Settings::default());
        assert_eq!(prompt.answer(), "10:00 2026-10-09");
        assert!(prompt.text.ends_with("10:00    => <2026-10-09 Fri 10:00>\n"));
        // Out of view, the months follow.
        prompt.select(Date::new(2027, 3, 1).to_day(), now(), &Settings::default());
        assert_eq!((prompt.calendar.year, prompt.calendar.month), (2027, 3));
        prompt.scroll(-1, now(), &Settings::default());
        assert_eq!(Date::from_day(prompt.selected), Date::new(2027, 3, 1));
        prompt.scroll(-3, now(), &Settings::default());
        assert_eq!(Date::from_day(prompt.selected), Date::new(2026, 11, 1));
        assert_eq!((prompt.calendar.year, prompt.calendar.month), (2026, 11));
        assert_eq!(prompt.day_at(Position::new(2, 31)), Some(Date::new(2026, 11, 1).to_day()));
    }

    #[test]
    fn shows_a_date_taken_to_be_in_the_future() {
        let mut prompt = prompt();
        prompt.set_typed("3".to_string(), now(), &Settings::default());
        assert!(prompt.text.ends_with("=> <2026-11-03 Tue> (=>F)\n"));
    }
}
