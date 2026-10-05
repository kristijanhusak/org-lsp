//! The commands that change an Org document at the cursor.
//!
//! Each is a port of an Org command and works like [`crate::todo`]: it
//! takes the text and a position and returns the new text. The server
//! turns the difference into edits for the editor.

use org_syntax::Parse;

use crate::date::DateTime;
use crate::outline::keyword_values;
use crate::priority::{self, Action, Priorities};
use crate::stamp::{self, Shift, Stamp, Unit};
use crate::timestr::parse_time_string;
use crate::todo::{self, headline_level, Target};
use crate::Refusal;

/// A shifted cursor key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `org-todo`
    Todo(Target),
    /// `org-priority`
    Priority(Action),
    /// `org-timestamp-up` and `org-timestamp-down`: change the part of
    /// the timestamp the cursor is on.
    Timestamp { forward: bool },
    /// `org-timestamp-up-day` and `org-timestamp-down-day`: move the
    /// timestamp at the cursor by days.
    Date { forward: bool },
    /// `org-shiftup` and its siblings: what Org does on a shifted cursor
    /// key depends on where the cursor is.
    Shift(Direction),
    /// `org-agenda-date-later` and `org-agenda-date-earlier`: move the
    /// timestamp an agenda line stands for by days. The position is where
    /// the agenda marks that timestamp, not a cursor.
    AgendaDate { forward: bool },
}

/// Settings, named after Org's variables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub todo: todo::Settings,
    /// `org-priority-highest`, `org-priority-lowest`, `org-priority-default`
    pub priorities: Priorities,
    /// `org-priority-start-cycle-with-default`
    pub priority_start_cycle_with_default: bool,
    /// The second number of `org-time-stamp-rounding-minutes`: the step
    /// in which minutes change.
    pub timestamp_rounding_minutes: i64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            todo: todo::Settings::default(),
            priorities: Priorities::default(),
            priority_start_cycle_with_default: true,
            timestamp_rounding_minutes: 5,
        }
    }
}

/// How a command was called.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Call {
    /// The count the user gave, Emacs's prefix argument.
    pub count: Option<i64>,
    /// The same command ran just before on the same entry.
    pub repeated: bool,
    /// The local time, for the timestamps a command writes.
    pub now: DateTime,
}

/// The priorities a document declares with `#+PRIORITIES:`. Only the
/// first such line counts.
pub fn document_priorities(parse: &Parse) -> Option<Priorities> {
    Priorities::from_keyword(keyword_values(&parse.syntax(), "PRIORITIES").first()?)
}

/// Whether `offset` is on a headline (`org-at-heading-p`).
fn on_headline(text: &str, offset: usize) -> bool {
    let start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
    headline_level(text[start..].split('\n').next().unwrap_or("")).is_some()
}

/// What a command made of the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub text: String,
    /// Where Emacs leaves the cursor, for the commands that move it
    /// within the text they change.
    pub cursor: Option<usize>,
}

/// Run `command` with the cursor at byte `offset` of `text`, whose parse
/// is `parse`.
pub fn run(
    command: &Command,
    text: &str,
    parse: &Parse,
    offset: usize,
    call: &Call,
    settings: &Settings,
) -> Result<Outcome, Refusal> {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    let steps = |forward: bool| call.count.unwrap_or(1) * if forward { 1 } else { -1 };
    let unplaced = |text: String| Outcome { text, cursor: None };
    let placed = |changed: stamp::Changed| Outcome { text: changed.text, cursor: Some(changed.cursor) };
    match command {
        Command::Todo(target) => {
            todo::change_state(text, parse.todo_keywords(), offset, target, &settings.todo, call.now)
                .map(unplaced)
                .map_err(|message| {
                    if message == "Before first headline" {
                        Refusal::NotHere(message)
                    } else {
                        Refusal::Failed(message)
                    }
                })
        }
        Command::Priority(action) => {
            let priority = priority::Settings {
                priorities: document_priorities(parse).unwrap_or(settings.priorities),
                start_cycle_with_default: settings.priority_start_cycle_with_default,
                tags_column: settings.todo.tags_column,
            };
            priority::change(text, parse.todo_keywords(), offset, *action, call.repeated, &priority).map(unplaced)
        }
        Command::Timestamp { forward } => {
            let shift = Shift { n: steps(*forward), what: None, updown: true, counted: call.count.is_some() };
            stamp::change(text, offset, &shift, settings.timestamp_rounding_minutes).map(placed)
        }
        Command::Date { forward } => {
            let shift =
                Shift { n: steps(*forward), what: Some(Unit::Day), updown: true, counted: call.count.is_some() };
            stamp::change(text, offset, &shift, settings.timestamp_rounding_minutes).map(placed)
        }
        Command::AgendaDate { forward } => {
            let rounding = settings.timestamp_rounding_minutes;
            let stamp =
                Stamp::around(text, offset).ok_or_else(|| Refusal::Failed("Cannot find time stamp".to_string()))?;
            let range = stamp::date_range_at(text, offset);
            let mut days = steps(*forward);
            if days == 1 && range.is_none() {
                // `org-agenda-move-date-from-past-immediately-to-today`: one
                // step forward takes a date that has passed to today.
                let date = parse_time_string(&text[stamp.start..stamp.end]).map(|parsed| parsed.date.to_day());
                if let Some(behind) = date.map(|date| call.now.day - date).filter(|behind| *behind > 0) {
                    days = behind;
                }
            }
            let shift = Shift { n: days, what: Some(Unit::Day), updown: false, counted: false };
            match range {
                None => stamp::change(text, offset, &shift, rounding).map(|changed| unplaced(changed.text)),
                // A date range moves as a whole. The later half goes first,
                // so that the position of the earlier one still holds.
                Some((first, second)) => {
                    let moved = stamp::change(text, second, &shift, rounding)?.text;
                    stamp::change(&moved, first, &shift, rounding).map(|changed| unplaced(changed.text))
                }
            }
        }
        Command::Shift(direction) => {
            let at_timestamp = Stamp::around(text, offset).is_some();
            let on_headline = on_headline(text, offset);
            let next = match direction {
                Direction::Up | Direction::Down if at_timestamp => {
                    Command::Timestamp { forward: *direction == Direction::Up }
                }
                Direction::Up if on_headline => Command::Priority(Action::Up),
                Direction::Down if on_headline => Command::Priority(Action::Down),
                Direction::Left | Direction::Right if at_timestamp => {
                    Command::Date { forward: *direction == Direction::Right }
                }
                Direction::Right if on_headline => Command::Todo(Target::Next),
                Direction::Left if on_headline => Command::Todo(Target::Previous),
                _ => return Err(Refusal::NotHere("No timestamp or headline here".to_string())),
            };
            run(&next, text, parse, offset, call, settings)
        }
    }
}
