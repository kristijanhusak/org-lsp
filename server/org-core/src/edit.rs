//! The commands that change an Org document at the cursor.
//!
//! Each is a port of an Org command and works like [`crate::todo`]: it
//! takes the text and a position and returns the new text. The server
//! turns the difference into edits for the editor.

use org_syntax::Parse;

use crate::date::DateTime;
use crate::outline::keyword_values;
use crate::priority::{self, Action, Priorities};
use crate::properties;
use crate::stamp::{self, Shift, Stamp, Unit};
use crate::tblfm;
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
    /// timestamp an agenda line stands for by days, or by hours or minutes
    /// (`org-agenda-date-later-hours`, `-minutes`; a minute step is
    /// `org-time-stamp-rounding-minutes`). The position is where the
    /// agenda marks that timestamp, not a cursor.
    AgendaDate { forward: bool, unit: Unit },
    /// `org-toggle-checkbox`, and the cookies that count checkboxes.
    Checkbox,
    /// `org-add-note`: the note of the call. Without one, the outcome
    /// asks for its text.
    Note,
    /// The note `org-log-reschedule` or `org-log-redeadline` asked for
    /// (`note`), written with the note of the call, or nothing when it
    /// was cancelled.
    PlanningNote { change: todo::PlanningChange, old: String },
    /// The note `org-log-refile` asked for (`note`), for the entry just
    /// refiled, or nothing when it was cancelled.
    RefileNote,
    /// The note `org-log-note-clock-out` asked for, after the clock line
    /// `line` (its text), or nothing when it was cancelled.
    ClockOutNote { line: String },
    /// `org-table-recalculate` and `org-table-iterate`: the table's
    /// formulas, or those of the `#+TBLFM:` line at the cursor.
    Table(tblfm::Mode),
    /// `org-set-tags`: the entry's tags, as picked.
    Tags(Vec<String>),
    /// `org-set-property`: the property `name` set to `value`, or for
    /// `TODO` and `PRIORITY` the keyword or the priority.
    Property { name: String, value: String },
    /// `org-delete-property`
    DeleteProperty(String),
    /// `org-delete-property-globally`
    DeletePropertyGlobally(String),
    /// `org-set-effort` with this value, or `org-inc-effort` without.
    Effort(Option<String>),
    /// `org-toggle-ordered-property`
    Ordered,
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
    /// The tag table and how tags are set.
    pub tags: crate::tags::Settings,
    /// `org-global-properties`
    pub global_properties: Vec<(String, String)>,
    /// `org-list-use-circular-motion`
    pub list_circular_motion: bool,
    /// `org-agenda-move-date-from-past-immediately-to-today`
    pub move_date_from_past_to_today: bool,
    /// How M-RET and its kin insert (`org-M-RET-may-split-line`,
    /// `org-blank-before-new-entry`, `org-insert-heading-respect-content`).
    pub insert: crate::meta_return::Settings,
    /// `org-special-ctrl-a/e`, for C-a and for C-e.
    pub special_ctrl_a_e: (crate::line::Special, crate::line::Special),
    /// `org-special-ctrl-k`
    pub special_ctrl_k: bool,
    /// `org-treat-S-cursor-todo-selection-as-state-change`: `S-<left>` and
    /// `S-<right>` on a headline log and are blocked as any TODO change.
    pub shift_todo_is_state_change: bool,
    /// `org-edit-timestamp-down-means-later`: `S-<down>` makes a
    /// timestamp later, `S-<up>` earlier.
    pub timestamp_down_means_later: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            todo: todo::Settings::default(),
            priorities: Priorities::default(),
            priority_start_cycle_with_default: true,
            timestamp_rounding_minutes: 5,
            tags: crate::tags::Settings::default(),
            global_properties: Vec::new(),
            list_circular_motion: false,
            move_date_from_past_to_today: true,
            insert: crate::meta_return::Settings::default(),
            special_ctrl_a_e: Default::default(),
            special_ctrl_k: false,
            shift_todo_is_state_change: true,
            timestamp_down_means_later: false,
        }
    }
}

/// How a command was called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    /// The count the user gave, Emacs's prefix argument.
    pub count: Option<i64>,
    /// The same command ran just before on the same entry.
    pub repeated: bool,
    /// The local time, for the timestamps a command writes.
    pub now: DateTime,
    /// The text of the note a TODO state change wants. Without it, such a
    /// change is not made: the outcome says that it wants a note.
    pub note: Option<String>,
    /// The note was cancelled: the change is made, and no note is logged.
    pub skip_note: bool,
    /// No note is asked for: a time is logged in its place.
    pub time_only: bool,
}

/// The priorities a document declares with `#+PRIORITIES:`. Only the
/// first such line counts.
pub fn document_priorities(parse: &Parse) -> Option<Priorities> {
    Priorities::from_keyword(keyword_values(&parse.syntax(), "PRIORITIES").first()?)
}

/// Whether `offset` is on a headline (`org-at-heading-p`).
/// Whether Org's shifted cursor keys act on a list item at `offset`: on
/// the line of one, with no timestamp or headline before it.
pub fn shifts_item(text: &str, offset: usize) -> bool {
    Stamp::around(text, offset).is_none() && !on_headline(text, offset) && crate::list::at_item(text, offset)
}

fn on_headline(text: &str, offset: usize) -> bool {
    let start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
    headline_level(text[start..].split('\n').next().unwrap_or("")).is_some()
}

/// What the property commands need of `parse` and `settings`.
pub fn property_context(parse: &Parse, settings: &Settings, category: &str) -> properties::Context {
    properties::Context {
        keywords: parse.todo_keywords().clone(),
        priorities: document_priorities(parse).unwrap_or(settings.priorities),
        globals: settings.global_properties.clone(),
        settings: settings.todo.clone(),
        category: category.to_string(),
    }
}

/// What a command made of the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub text: String,
    /// Where Emacs leaves the cursor, for the commands that move it
    /// within the text they change.
    pub cursor: Option<usize>,
    /// The note a TODO state change wants, when the call gave no text for
    /// it. The text is then without the note.
    pub note: Option<todo::WantedNote>,
    /// What to tell the user.
    pub message: Option<String>,
    /// The headline as a change to a done state made it, before the
    /// entry's repeater set it back (`org-agenda-headline-snapshot-before-repeat`).
    pub before_repeat: Option<String>,
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
    let unplaced = |text: String| Outcome { text, cursor: None, note: None, message: None, before_repeat: None };
    let placed = |changed: stamp::Changed| Outcome {
        text: changed.text,
        cursor: Some(changed.cursor),
        note: None,
        message: None,
        before_repeat: None,
    };
    match command {
        Command::Todo(target) => {
            let keywords = parse.todo_keywords();
            let note = match (&call.note, call.skip_note) {
                _ if call.time_only => todo::NoteText::TimeOnly,
                (_, true) => todo::NoteText::Skip,
                (Some(note), false) => todo::NoteText::Write(note),
                (None, false) => todo::NoteText::Ask,
            };
            todo::change_state_noted(text, keywords, offset, target, &settings.todo, call.now, note)
                .map(|changed| Outcome {
                    text: changed.text,
                    cursor: None,
                    note: changed.wants_note,
                    message: changed.message,
                    before_repeat: changed.before_repeat,
                })
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
                auto_align: settings.tags.auto_align,
            };
            priority::change(text, parse.todo_keywords(), offset, *action, call.repeated, &priority).map(unplaced)
        }
        Command::Note => match (&call.note, call.skip_note) {
            (_, true) => Ok(unplaced(text.to_string())),
            (Some(note), false) => todo::add_note(text, parse.todo_keywords(), offset, &settings.todo, call.now, note)
                .map(unplaced)
                .map_err(|message| {
                    if message == "Before first headline" {
                        Refusal::NotHere(message)
                    } else {
                        Refusal::Failed(message)
                    }
                }),
            (None, false) => {
                // On a headline only, as Emacs refuses before the first.
                todo::add_note(text, parse.todo_keywords(), offset, &settings.todo, call.now, "")
                    .map_err(Refusal::NotHere)?;
                let wanted = todo::WantedNote { purpose: "this entry".to_string(), state: None };
                Ok(Outcome {
                    text: text.to_string(),
                    cursor: None,
                    note: Some(wanted),
                    message: None,
                    before_repeat: None,
                })
            }
        },
        Command::PlanningNote { change, old } => match (&call.note, call.skip_note) {
            (Some(note), false) => todo::log_planning_change(
                text,
                parse.todo_keywords(),
                offset,
                &settings.todo,
                call.now,
                *change,
                old,
                note,
            )
            .map(unplaced)
            .map_err(Refusal::Failed),
            _ => Ok(unplaced(text.to_string())),
        },
        Command::ClockOutNote { line } => match (&call.note, call.skip_note) {
            (Some(note), false) => {
                // The clock line, wherever it is now.
                let Some(start) = text.find(line.as_str()) else { return Ok(unplaced(text.to_string())) };
                let end = start + line.len();
                todo::log_clock_out(text, parse.todo_keywords(), end, &settings.todo, call.now, note)
                    .map(unplaced)
                    .map_err(Refusal::Failed)
            }
            _ => Ok(unplaced(text.to_string())),
        },
        Command::RefileNote => match (&call.note, call.skip_note) {
            (Some(note), false) => {
                todo::log_refile(text, parse.todo_keywords(), offset, &settings.todo, call.now, note)
                    .map(unplaced)
                    .map_err(Refusal::Failed)
            }
            _ => Ok(unplaced(text.to_string())),
        },
        // `C-c C-c` on a `#+TBLFM:` line applies its formulas.
        Command::Checkbox if tblfm::on_formula_line(text, offset) => {
            run(&Command::Table(tblfm::Mode::All), text, parse, offset, call, settings)
        }
        // `C-c C-c` on a timestamp rewrites it (`org-timestamp-change` by
        // nothing), its weekday put right.
        Command::Checkbox if Stamp::around(text, offset).is_some() => {
            let shift = Shift { n: 0, what: Some(Unit::Day), updown: false, counted: false };
            stamp::change(text, offset, &shift, settings.timestamp_rounding_minutes).map(placed)
        }
        // `C-c C-c` on the line of an item, as `org-ctrl-c-ctrl-c` there:
        // the box toggled and the list repaired.
        Command::Checkbox if crate::list::at_item(text, offset) && !on_headline(text, offset) => {
            let outline = crate::outline::Outline::new(parse, text, "");
            let ordered = outline.entry_property(outline.entry_at(offset), "ORDERED", false);
            let ordered = ordered.is_some_and(|value| value != "nil");
            crate::checkbox::ctrl_c_ctrl_c(
                text,
                parse.todo_keywords(),
                offset,
                settings.todo.tags_column,
                ordered,
                &settings.todo.lists,
            )
            .map(|(text, cursor, message)| Outcome {
                text,
                cursor: Some(cursor),
                note: None,
                message,
                before_repeat: None,
            })
        }
        Command::Checkbox => crate::checkbox::toggle(
            text,
            parse.todo_keywords(),
            offset,
            settings.todo.tags_column,
            &settings.todo.lists,
        )
        .map(unplaced),
        Command::Table(mode) => {
            let outline = crate::outline::Outline::new(parse, text, "");
            let entry = outline.entry_at(offset);
            let property = |name: &str| outline.entry_property(entry, name, true);
            // A Windows file's lines, without their `\r`s and with them
            // again after.
            let crlf = crate::text::crlf(text);
            let recalculated = if crlf {
                let (lf, offset) = crate::text::to_lf(text, offset);
                tblfm::recalculate(&lf, offset, *mode, &property, &settings.todo.table.formulas)
                    .map(|done| tblfm::Recalculated { text: crate::text::from_lf(&done.text, 0).0, ..done })
            } else {
                tblfm::recalculate(text, offset, *mode, &property, &settings.todo.table.formulas)
            };
            match recalculated {
                Ok(recalculated) => Ok(Outcome {
                    text: recalculated.text,
                    cursor: None,
                    note: None,
                    message: recalculated.message,
                    before_repeat: None,
                }),
                Err(message) if message == "Not at a table" => Err(Refusal::NotHere(message)),
                Err(message) => Err(Refusal::Failed(message)),
            }
        }
        Command::Property { name, value } => {
            let context = property_context(parse, settings, "");
            let failed = |message: String| {
                if message == "Before first headline" {
                    Refusal::NotHere(message)
                } else {
                    Refusal::Failed(message)
                }
            };
            match properties::set(text, offset, name, value, &context).map_err(failed)? {
                properties::Change::None => Ok(unplaced(text.to_string())),
                properties::Change::Text(text) => Ok(unplaced(text)),
                properties::Change::Todo(state) => {
                    run(&Command::Todo(Target::State(state)), text, parse, offset, call, settings)
                }
                properties::Change::Priority(value) => {
                    let action = match value {
                        Some(value) => Action::Set(priority::value(&value).unwrap_or(0)),
                        None => Action::Remove,
                    };
                    run(&Command::Priority(action), text, parse, offset, call, settings)
                }
            }
        }
        Command::DeleteProperty(name) => {
            let (text, _, _) = properties::delete(text, offset, name);
            Ok(Outcome {
                text,
                cursor: None,
                note: None,
                message: Some(format!("Property \"{name}\" deleted")),
                before_repeat: None,
            })
        }
        Command::DeletePropertyGlobally(name) => {
            let (text, count) = properties::delete_globally(text, name);
            let message = format!("Property \"{name}\" removed from {count} entries");
            Ok(Outcome { text, cursor: None, note: None, message: Some(message), before_repeat: None })
        }
        Command::Effort(value) => {
            let context = property_context(parse, settings, "");
            let (text, message) =
                properties::set_effort(text, offset, value.as_deref(), &context).map_err(Refusal::Failed)?;
            Ok(Outcome { text, cursor: None, note: None, message: Some(message), before_repeat: None })
        }
        Command::Ordered => {
            let context = property_context(parse, settings, "");
            let (text, message) = properties::toggle_ordered(text, offset, &context).map_err(Refusal::NotHere)?;
            Ok(Outcome { text, cursor: None, note: None, message: Some(message), before_repeat: None })
        }
        Command::Tags(tags) => {
            let heading = crate::tags::heading_at(text, offset)?;
            let settings = crate::tags::Settings { tags_column: settings.todo.tags_column, ..settings.tags.clone() };
            Ok(unplaced(crate::tags::set_tags(text, heading, tags, &settings)))
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
        Command::AgendaDate { forward, unit } => {
            let rounding = settings.timestamp_rounding_minutes;
            let stamp =
                Stamp::around(text, offset).ok_or_else(|| Refusal::Failed("Cannot find time stamp".to_string()))?;
            let range = stamp::date_range_at(text, offset);
            let mut days = steps(*forward);
            if *unit == Unit::Minute {
                days *= rounding.max(1);
            }
            if settings.move_date_from_past_to_today && days == 1 && range.is_none() && *unit == Unit::Day {
                // `org-agenda-move-date-from-past-immediately-to-today`: one
                // step forward takes a date that has passed to today.
                let date = parse_time_string(&text[stamp.start..stamp.end]).map(|parsed| parsed.date.to_day());
                if let Some(behind) = date.map(|date| call.now.day - date).filter(|behind| *behind > 0) {
                    days = behind;
                }
            }
            let shift = Shift { n: days, what: Some(*unit), updown: false, counted: false };
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
                    Command::Timestamp { forward: (*direction == Direction::Up) != settings.timestamp_down_means_later }
                }
                Direction::Up if on_headline => Command::Priority(Action::Up),
                Direction::Down if on_headline => Command::Priority(Action::Down),
                Direction::Left | Direction::Right if at_timestamp => {
                    Command::Date { forward: *direction == Direction::Right }
                }
                Direction::Left | Direction::Right if on_headline && !settings.shift_todo_is_state_change => {
                    let target = if *direction == Direction::Right { Target::Next } else { Target::Previous };
                    return todo::change_state_quietly(
                        text,
                        parse.todo_keywords(),
                        offset,
                        &target,
                        &settings.todo,
                        call.now,
                    )
                    .map(|changed| Outcome {
                        text: changed.text,
                        cursor: None,
                        note: changed.wants_note,
                        message: changed.message,
                        before_repeat: changed.before_repeat,
                    })
                    .map_err(Refusal::Failed);
                }
                Direction::Right if on_headline => Command::Todo(Target::Next),
                Direction::Left if on_headline => Command::Todo(Target::Previous),
                // On a list item: its list's bullets cycled
                // (`org-cycle-list-bullet`), or the cursor to the item
                // before or after (`org-previous-item`, `org-next-item`).
                Direction::Left | Direction::Right if crate::list::at_item(text, offset) => {
                    let edited =
                        crate::list::cycle_bullet(text, offset, *direction == Direction::Left, &settings.todo.lists)?;
                    return Ok(Outcome {
                        text: edited.text,
                        cursor: Some(edited.cursor),
                        note: None,
                        message: None,
                        before_repeat: None,
                    });
                }
                Direction::Up | Direction::Down if crate::list::at_item(text, offset) => {
                    let cursor = crate::list::next_item(
                        text,
                        offset,
                        *direction == Direction::Up,
                        settings.list_circular_motion,
                    )?;
                    return Ok(Outcome {
                        text: text.to_string(),
                        cursor: Some(cursor),
                        note: None,
                        message: None,
                        before_repeat: None,
                    });
                }
                // On a property line: its next or previous allowed value
                // (`org-property-next-allowed-value`).
                Direction::Left | Direction::Right if properties::property_at(text, offset).is_some() => {
                    let context = property_context(parse, settings, "");
                    let (text, cursor) =
                        properties::next_allowed(text, offset, *direction == Direction::Left, &context)
                            .map_err(Refusal::Failed)?;
                    return Ok(Outcome { text, cursor: Some(cursor), note: None, message: None, before_repeat: None });
                }
                _ => return Err(Refusal::NotHere("No timestamp, headline, item or property here".to_string())),
            };
            run(&next, text, parse, offset, call, settings)
        }
    }
}
