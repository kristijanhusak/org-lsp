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
/// The name of the file the prompt is written to, beside the agenda's, for
/// clients that cannot fetch it.
pub const FILE_NAME: &str = "Date.org-date";

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
    Agenda { view: Uri, line: u32 },
}

impl Target {
    /// The document the command was run from.
    pub fn from(&self) -> &Uri {
        match self {
            Target::Place { uri, .. } => uri,
            Target::Entry { view, .. } | Target::Agenda { view, .. } => view,
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
    /// A time picked with the keys (hours and minutes): it takes the place
    /// of a time in what was typed.
    pub time: Option<(i64, i64)>,
    /// The part of the time the keys change, while they change the time
    /// instead of the day.
    pub time_field: Option<TimeField>,
}

/// The part of a picked time the keys change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeField {
    Hour,
    Minute,
}

impl DatePrompt {
    /// The prompt, its calendar showing `months` months whose weeks start
    /// on `week_start`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: &str,
        command: Command,
        target: Target,
        default: Option<DateTime>,
        typed: String,
        now: DateTime,
        settings: &Settings,
        (months, week_start): (usize, usize),
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
            calendar: Calendar::around(selected, months, week_start),
            text: String::new(),
            tokens: Vec::new(),
            reveal: true,
            moving_cursor: false,
            time: None,
            time_field: None,
        };
        prompt.render(now, settings);
        prompt
    }

    /// The answer: what was typed, and the selected day once it moved,
    /// with the picked time instead of a typed one.
    pub fn answer(&self) -> String {
        if self.moved {
            self.answer_with(self.selected)
        } else {
            self.with_time(self.typed.clone())
        }
    }

    /// The answer with `day` picked in the calendar.
    pub fn answer_with(&self, day: Day) -> String {
        self.with_time(format!("{} {}", self.typed, Date::from_day(day)))
    }

    /// What was typed, with the picked time: where typing the answer
    /// starts from.
    pub fn typed_with_time(&self) -> String {
        self.with_time(self.typed.clone())
    }

    fn with_time(&self, answer: String) -> String {
        match self.time {
            Some((hour, minute)) => {
                let rest = org_core::read_date::without_time(&answer);
                format!("{rest} {hour:02}:{minute:02}").trim_start().to_string()
            }
            None => answer,
        }
    }

    /// Start or stop picking the time with the keys. It starts from the
    /// time of the answer, or else from now, rounded to `step` minutes.
    pub fn toggle_time(&mut self, step: i64, now: DateTime, settings: &Settings) {
        if self.time_field.take().is_none() {
            let given = org_core::read_date::read_date(&self.answer(), self.default, now, settings)
                .ok()
                .and_then(|read| read.time);
            self.time = Some(given.unwrap_or_else(|| {
                let rounded = (now.minute + step / 2) / step * step % 1440;
                (rounded / 60, rounded % 60)
            }));
            self.time_field = Some(TimeField::Hour);
        }
        self.render(now, settings);
    }

    /// Pick which part of the time the keys change.
    pub fn time_field_to(&mut self, field: TimeField, now: DateTime, settings: &Settings) {
        self.time_field = Some(field);
        self.render(now, settings);
    }

    /// Change the part of the time being picked: an hour, or `step`
    /// minutes, up or down, round the clock. Minutes off the step go to
    /// the next multiple of it first, as `org-timestamp-change` moves
    /// them: 12:44 goes up to 12:45, down to 12:40.
    pub fn change_time(&mut self, up: bool, step: i64, now: DateTime, settings: &Settings) {
        let (Some((hour, mut minute)), Some(field)) = (self.time, self.time_field) else { return };
        let by = match field {
            TimeField::Hour => 60,
            TimeField::Minute => {
                let rem = minute % step;
                if rem != 0 {
                    minute += if up { -rem } else { step - rem };
                }
                step
            }
        };
        let total = (hour * 60 + minute + if up { by } else { -by }).rem_euclid(1440);
        self.time = Some((total / 60, total % 60));
        self.render(now, settings);
    }

    /// Select `day`, showing the months around it if it is not shown.
    pub fn select(&mut self, day: Day, now: DateTime, settings: &Settings) {
        self.selected = day;
        self.moved = true;
        if !self.calendar.shows(day) {
            self.calendar = Calendar::around(day, self.calendar.months, self.calendar.week_start);
        }
        self.render(now, settings);
        self.reveal = true;
    }

    /// Show the months `by` months on; the selected day stays if it is
    /// still shown, or else goes to today if that is shown, or else to the
    /// first of the middle month (`calendar-scroll-left`).
    pub fn scroll(&mut self, by: i64, now: DateTime, settings: &Settings) {
        self.calendar = self.calendar.scrolled(by);
        if !self.calendar.shows(self.selected) {
            self.selected = if self.calendar.shows(now.day) {
                now.day
            } else {
                Date::new(self.calendar.year, self.calendar.month, 1).to_day()
            };
        }
        self.moved = true;
        self.render(now, settings);
        self.reveal = true;
    }

    /// What was typed changed; the selected day did not, and the cursor
    /// stays where it is.
    pub fn set_typed(&mut self, typed: String, now: DateTime, settings: &Settings) {
        self.typed = typed;
        // What is typed now carries the time, if it has one.
        self.time = None;
        self.time_field = None;
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
                        let weekend = matches!((index + self.calendar.week_start) % 7, 0 | 6);
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
        // The picked time, with the part the keys change marked.
        if let Some((hour, minute)) = self.time {
            let start = text.len();
            let line = format!("Time: {hour:02}:{minute:02}");
            token(start, 5, TokenType::CalendarHeader, 0);
            match self.time_field {
                Some(TimeField::Hour) => token(start + 6, 2, TokenType::CalendarSelected, 0),
                Some(TimeField::Minute) => token(start + 9, 2, TokenType::CalendarSelected, 0),
                None => token(start + 6, 5, TokenType::Timestamp, 0),
            }
            text.push_str(&line);
            text.push('\n');
        }
        // The minibuffer, after a blank line.
        text.push('\n');
        let start = text.len();
        token(start, self.label.len(), TokenType::CalendarPrompt, 0);
        // A picked time shows in place of a typed one, as `i` would start.
        let mut line = format!("{}{}", self.label, self.typed_with_time());
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
            (3, 0),
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
        // Out of view, the day goes to today when it is shown.
        prompt.scroll(-3, now(), &Settings::default());
        assert_eq!(Date::from_day(prompt.selected), Date::new(2026, 10, 5));
        assert_eq!((prompt.calendar.year, prompt.calendar.month), (2026, 11));
        assert_eq!(prompt.day_at(Position::new(2, 31)), Some(Date::new(2026, 11, 1).to_day()));
        // Else to the first of the middle month.
        prompt.scroll(6, now(), &Settings::default());
        assert_eq!(Date::from_day(prompt.selected), Date::new(2027, 5, 1));
    }

    #[test]
    fn picks_a_time_with_the_keys() {
        let settings = Settings::default();
        let mut picking = prompt();
        // From the time typed, the hours first.
        picking.toggle_time(5, now(), &settings);
        assert_eq!((picking.time, picking.time_field), (Some((10, 0)), Some(TimeField::Hour)));
        picking.change_time(true, 5, now(), &settings);
        picking.time_field_to(TimeField::Minute, now(), &settings);
        picking.change_time(false, 5, now(), &settings);
        assert_eq!(picking.time, Some((10, 55)));
        let lines: Vec<&str> = picking.text.lines().collect();
        assert_eq!(lines[lines.len() - 3], "Time: 10:55");
        // The minibuffer shows it in place of the typed time.
        assert!(
            lines[lines.len() - 1].ends_with("]: 10:55    => <2026-10-07 Wed 10:55>"),
            "{}",
            lines[lines.len() - 1]
        );
        // It takes the place of the typed time, with the day once it moved.
        assert_eq!(picking.answer(), "10:55");
        picking.select(Date::new(2026, 10, 9).to_day(), now(), &settings);
        assert_eq!(picking.answer(), "2026-10-09 10:55");
        assert!(picking.text.ends_with("=> <2026-10-09 Fri 10:55>\n"), "{}", picking.text);
        // Back to days, the time kept; round the clock.
        picking.toggle_time(5, now(), &settings);
        assert_eq!((picking.time, picking.time_field), (Some((10, 55)), None));
        picking.toggle_time(5, now(), &settings);
        for _ in 0..14 {
            picking.change_time(true, 5, now(), &settings);
        }
        assert_eq!(picking.time, Some((0, 55)));
        // Typed again, the typed time counts.
        picking.set_typed("15:00".to_string(), now(), &settings);
        assert_eq!((picking.time, picking.answer()), (None, "15:00 2026-10-09".to_string()));

        // With no time typed: now, rounded.
        let mut untimed = prompt();
        untimed.set_typed(String::new(), now(), &settings);
        let now = DateTime::new(Date::new(2026, 10, 5), 10, 33);
        untimed.toggle_time(5, now, &settings);
        assert_eq!(untimed.time, Some((10, 35)));
    }

    #[test]
    fn rounds_minutes_off_the_step_first() {
        let settings = Settings::default();
        let minutes = |typed: &str, up: bool, times: usize| {
            let mut picking = prompt();
            picking.set_typed(typed.to_string(), now(), &settings);
            picking.toggle_time(5, now(), &settings);
            picking.time_field_to(TimeField::Minute, now(), &settings);
            (0..times)
                .map(|_| {
                    picking.change_time(up, 5, now(), &settings);
                    picking.time.unwrap()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(minutes("12:44", true, 3), [(12, 45), (12, 50), (12, 55)]);
        assert_eq!(minutes("12:44", false, 2), [(12, 40), (12, 35)]);
        assert_eq!(minutes("12:58", true, 2), [(13, 0), (13, 5)]);
        assert_eq!(minutes("0:02", false, 1), [(0, 0)]);
        // The hours keep the minutes.
        let mut picking = prompt();
        picking.set_typed("12:44".to_string(), now(), &settings);
        picking.toggle_time(5, now(), &settings);
        picking.change_time(true, 5, now(), &settings);
        assert_eq!(picking.time, Some((13, 44)));
    }

    #[test]
    fn starts_weeks_on_the_day_asked_for() {
        let file: Uri = "file:///org/notes.org".parse().unwrap();
        let target = Target::Place { uri: file, position: Position::new(0, 0) };
        let prompt = DatePrompt::new(
            "org.schedule",
            Command::Schedule,
            target,
            None,
            String::new(),
            now(),
            &Settings::default(),
            (1, 1),
        );
        let lines: Vec<&str> = prompt.text.lines().collect();
        assert_eq!(lines[1], "     Mo Tu We Th Fr Sa Su");
        // Saturday and Sunday are the weekend, in the last two columns.
        let weekend: Vec<usize> = prompt
            .tokens
            .iter()
            .filter(|token| token.ty == TokenType::CalendarWeekday && token.modifiers & modifier::WEEKEND != 0)
            .map(|token| usize::from(token.range.start()) - lines[0].len() - 1)
            .collect();
        assert_eq!(weekend, [20, 23]);
    }

    #[test]
    fn shows_a_date_taken_to_be_in_the_future() {
        let mut prompt = prompt();
        prompt.set_typed("3".to_string(), now(), &Settings::default());
        assert!(prompt.text.ends_with("=> <2026-11-03 Tue> (=>F)\n"));
    }
}
