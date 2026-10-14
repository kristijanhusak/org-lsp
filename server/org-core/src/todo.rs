//! Changing the TODO state of an entry, with everything Org does around it.
//!
//! [`change_state`] ports `org-todo`. Besides replacing the keyword it
//!
//! - records the change where the keyword definitions or the settings ask
//!   for it: a `CLOSED:` timestamp, a state note;
//! - realigns the tags of the headline;
//! - updates statistics cookies (`[1/3]`, `[33%]`) in the parent headline;
//! - and, when an entry with a repeater is marked done, moves its
//!   timestamps to the next occurrence and puts the entry back to its
//!   first state (`org-auto-repeat-maybe`).
//!
//! The function works on a copy of the text, step by step as Emacs works on
//! its buffer, and returns the new text. [`edits`] turns the difference
//! into the few line edits an editor has to apply.

use org_syntax::{Logging, SyntaxKind, TodoKeywords, TokenAtOffset};

use crate::date::DateTime;
use crate::indent::{self, Adapt};
use crate::outline::Outline;
use crate::stamp::{self, format_timestamp, Around, Placed, Shift, Stamp, Unit};
use crate::text::{char_width, tag_group, width, word_start};
use crate::timestr::parse_time_string;

/// The state to give the entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// The next keyword, or none after the last one (`org-todo` with
    /// `right`: Shift-right).
    Next,
    /// The previous keyword, or none before the first one.
    Previous,
    /// `org-todo` without fast selection: the next state of the entry's
    /// sequence, none after its last done state, and from none the head
    /// of `head`'s sequence (the one the entry was last in). `repeated`:
    /// the command again, which steps on through a `type` sequence.
    Cycle {
        head: Option<String>,
        repeated: bool,
    },
    /// The first keyword of the next sequence (`nextset`), or of the
    /// previous one (`previousset`); `head` is the sequence of an entry
    /// with no keyword.
    NextSet {
        head: Option<String>,
    },
    PreviousSet {
        head: Option<String>,
    },
    /// This keyword, or none.
    State(Option<String>),
}

/// What to do with a repeating entry that was marked done.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RepeatTo {
    /// Back to the first keyword of its sequence.
    #[default]
    Head,
    /// To this keyword.
    State(String),
    /// To the state it had before.
    Previous,
}

/// `org-ts-regexp`: an active timestamp.
static ACTIVE_STAMP: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"<[0-9]{4}-[0-9]{2}-[0-9]{2}[^>\n]*>").expect("a valid regex"));
/// Runs of spaces, which a heading's pattern takes as ` +`.
static SPACES: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(" +").expect("a valid regex"));
/// `org-drawer-regexp`
static DRAWER_LINE: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"^[ \t]*:(?:\w|[-_])+:[ \t]*$").expect("a valid regex"));

/// Settings, named after Org's variables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// `org-log-done`
    pub log_done: Option<Logging>,
    /// `org-log-repeat`
    pub log_repeat: Option<Logging>,
    /// `org-enforce-todo-dependencies`: an entry is not done while a
    /// child is not, or an earlier sibling under `ORDERED` is not.
    pub enforce_todo_dependencies: bool,
    /// `org-enforce-todo-checkbox-dependencies`: an entry is not done
    /// while it has an unchecked box.
    pub enforce_todo_checkbox_dependencies: bool,
    /// `org-log-reschedule`
    pub log_reschedule: Option<Logging>,
    /// `org-log-redeadline`
    pub log_redeadline: Option<Logging>,
    /// `org-log-refile`
    pub log_refile: Option<Logging>,
    /// `org-log-into-drawer`: the drawer state notes go into.
    pub log_into_drawer: Option<String>,
    /// `org-tags-column`
    pub tags_column: i64,
    /// `org-todo-repeat-to-state`
    pub repeat_to_state: RepeatTo,
    /// `org-adapt-indentation`: how the lines Org writes are indented.
    pub adapt: Adapt,
    /// `org-extend-today-until`: the hour before which `.+` and `++`
    /// repeaters count from yesterday.
    pub extend_today_until: i64,
    /// `org-log-done-with-time`: the `CLOSED:` timestamp has the time.
    pub log_done_with_time: bool,
    /// `org-log-note-headings`
    pub note_headings: NoteHeadings,
    /// `org-log-states-order-reversed`: the latest note first.
    pub states_order_reversed: bool,
    /// `org-log-state-notes-insert-after-drawers`
    pub notes_after_drawers: bool,
}

/// `org-log-note-headings`: the heading of each kind of note, with Org's
/// `%`-escapes (`%t` the time, `%s` the new state, `%S` the old one…).
/// An empty heading leaves the note its text only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteHeadings {
    pub done: String,
    pub state: String,
    pub note: String,
    pub reschedule: String,
    pub delschedule: String,
    pub redeadline: String,
    pub deldeadline: String,
    pub refile: String,
    pub clock_out: String,
}

impl Default for NoteHeadings {
    fn default() -> Self {
        NoteHeadings {
            done: "CLOSING NOTE %t".to_string(),
            state: "State %-12s from %-12S %t".to_string(),
            note: "Note taken on %t".to_string(),
            reschedule: "Rescheduled from %S on %t".to_string(),
            delschedule: "Not scheduled, was %S on %t".to_string(),
            redeadline: "New deadline from %S on %t".to_string(),
            deldeadline: "Removed deadline, was %S on %t".to_string(),
            refile: "Refiled on %t".to_string(),
            clock_out: String::new(),
        }
    }
}

impl NoteHeadings {
    /// The heading for the note of `purpose`.
    fn of(&self, purpose: Purpose) -> &str {
        match purpose {
            Purpose::Done => &self.done,
            Purpose::State => &self.state,
            Purpose::Note => &self.note,
            Purpose::Refile => &self.refile,
            Purpose::Planning(PlanningChange::Reschedule) => &self.reschedule,
            Purpose::Planning(PlanningChange::Unschedule) => &self.delschedule,
            Purpose::Planning(PlanningChange::Redeadline) => &self.redeadline,
            Purpose::Planning(PlanningChange::Undeadline) => &self.deldeadline,
        }
    }
}

/// `user-login-name`, from the environment.
fn login_name() -> String {
    ["LOGNAME", "USER", "USERNAME"].iter().find_map(|name| std::env::var(name).ok()).unwrap_or_default()
}

/// `user-full-name`: `NAME`, or the user's name in `/etc/passwd`.
fn full_name() -> String {
    if let Ok(name) = std::env::var("NAME") {
        return name;
    }
    let login = login_name();
    std::fs::read_to_string("/etc/passwd")
        .ok()
        .and_then(|passwd| {
            passwd.lines().find_map(|line| {
                let fields: Vec<&str> = line.split(':').collect();
                (fields.first() == Some(&login.as_str()))
                    .then(|| fields.get(4).map(|gecos| gecos.split(',').next().unwrap_or("").to_string()))
                    .flatten()
            })
        })
        .unwrap_or_default()
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            log_done: None,
            log_repeat: Some(Logging::Time),
            enforce_todo_dependencies: false,
            enforce_todo_checkbox_dependencies: false,
            log_reschedule: None,
            log_redeadline: None,
            log_refile: None,
            log_into_drawer: None,
            tags_column: -77,
            repeat_to_state: RepeatTo::Head,
            adapt: Adapt::Off,
            extend_today_until: 0,
            log_done_with_time: true,
            note_headings: NoteHeadings::default(),
            states_order_reversed: true,
            notes_after_drawers: false,
        }
    }
}

/// A replacement of whole lines: lines `start..end` of the old text become
/// `text`, which is empty or ends with a line break unless it reaches the
/// end of the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineEdit {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// The lines of a note's text, as `org-store-log-note` takes them from
/// the note buffer: without blank lines at the start (the comment lines
/// the buffer starts with take them along), without comment lines, and
/// without blanks at the end of the last line, split at newlines
/// (`org-split-string`, which ignores one at the end).
fn note_lines(text: &str) -> Vec<&str> {
    // `\`# .*\n[ \t\n]*`, after the buffer's own comment lines.
    let mut text = text.trim_start_matches([' ', '\t', '\n']);
    while let Some(rest) = text.strip_prefix("# ").and_then(|rest| rest.split_once('\n')).map(|(_, rest)| rest) {
        text = rest.trim_start_matches([' ', '\t', '\n']);
    }
    // `\s-+\'`: a newline is no whitespace in the note buffer.
    let text = text.trim_end_matches([' ', '\t']);
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<&str> = text.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    lines
}

/// What a note is for, which its heading says (`org-log-note-headings`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Purpose {
    /// `CLOSING NOTE`, for an entry done.
    Done,
    /// `State "DONE" from "TODO"`.
    State,
    /// `Note taken on`, of `org-add-note`.
    Note,
    /// A planning date changed or removed.
    Planning(PlanningChange),
    /// `Refiled on`, of `org-log-refile`.
    Refile,
}

/// A change of an entry's `SCHEDULED` or `DEADLINE` date, which
/// `org-log-reschedule` and `org-log-redeadline` log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanningChange {
    /// `Rescheduled from …`
    Reschedule,
    /// `Not scheduled, was …`
    Unschedule,
    /// `New deadline from …`
    Redeadline,
    /// `Removed deadline, was …`
    Undeadline,
}

impl PlanningChange {
    /// What the note buffer says the note is for (`org-add-log-note`).
    pub fn purpose(self) -> &'static str {
        match self {
            PlanningChange::Reschedule => "rescheduling",
            PlanningChange::Unschedule => "no longer scheduled",
            PlanningChange::Redeadline => "changing deadline",
            PlanningChange::Undeadline => "removing deadline",
        }
    }

    fn is_deadline(self) -> bool {
        matches!(self, PlanningChange::Redeadline | PlanningChange::Undeadline)
    }
}

/// A note that waits to be written.
struct Note {
    purpose: Purpose,
    state: Option<String>,
    previous: Option<String>,
    /// The user writes text for it (`@`); otherwise it records the time
    /// only (`!`) and is written at once.
    text: bool,
}

pub(crate) struct Editor<'a> {
    pub(crate) text: String,
    keywords: &'a TodoKeywords,
    settings: &'a Settings,
    now: DateTime,
    /// Byte offset of the entry's headline. It moves when text before it
    /// changes.
    pub(crate) heading: usize,
    note: Option<Note>,
    /// What `org-auto-repeat-maybe` says when it moved the timestamps:
    /// `Entry repeats: SCHEDULED: <…> `.
    repeated: Option<String>,
    /// The headline as the change made it, before a repeater set the
    /// state back: what the agenda shows on the entry's line until it is
    /// rebuilt (`org-agenda-headline-snapshot-before-repeat`).
    before_repeat: Option<String>,
    /// The logging in force for this command.
    log: Log,
}

/// What is logged: `org-log-done`, `org-log-repeat`, `org-log-into-drawer`
/// as the settings and the file's `#+STARTUP:` words have them, and the
/// states that log (`org-todo-log-states`), the keywords' own unless the
/// entry's `LOGGING` property says otherwise.
#[derive(Debug, Clone)]
struct Log {
    done: Option<Logging>,
    repeat: Option<Logging>,
    reschedule: Option<Logging>,
    redeadline: Option<Logging>,
    refile: Option<Logging>,
    into_drawer: Option<String>,
    /// `org-log-states-order-reversed`, as `#+STARTUP:` may set it.
    reversed: bool,
    /// The states that log when entered and when left; `None` for those
    /// the keywords say (`WAIT(w@/!)`).
    states: Option<Vec<LogState>>,
}

/// A state, what entering it logs and what leaving it logs.
type LogState = (String, Option<Logging>, Option<Logging>);

impl Log {
    /// The settings, with the `#+STARTUP:` words of `text` after them.
    fn new(settings: &Settings, text: &str) -> Log {
        let mut log = Log {
            done: settings.log_done,
            repeat: settings.log_repeat,
            reschedule: settings.log_reschedule,
            redeadline: settings.log_redeadline,
            refile: settings.log_refile,
            into_drawer: settings.log_into_drawer.clone(),
            reversed: settings.states_order_reversed,
            states: None,
        };
        for word in crate::text::startup_words(text) {
            match word {
                "logdrawer" => log.into_drawer = Some("LOGBOOK".to_string()),
                "nologdrawer" => log.into_drawer = None,
                "logstatesreversed" => log.reversed = true,
                "nologstatesreversed" => log.reversed = false,
                word => {
                    log.set(word);
                }
            }
        }
        log
    }

    /// The `org-startup-options` word `word` for `org-log-done` or
    /// `org-log-repeat`, if it is one.
    fn set(&mut self, word: &str) -> bool {
        match word {
            "logdone" => self.done = Some(Logging::Time),
            "lognotedone" => self.done = Some(Logging::Note),
            "nologdone" => self.done = None,
            // `state`, which logs as a time does.
            "logrepeat" => self.repeat = Some(Logging::Time),
            "lognoterepeat" => self.repeat = Some(Logging::Note),
            "nologrepeat" => self.repeat = None,
            "logreschedule" => self.reschedule = Some(Logging::Time),
            "lognotereschedule" => self.reschedule = Some(Logging::Note),
            "nologreschedule" => self.reschedule = None,
            "logredeadline" => self.redeadline = Some(Logging::Time),
            "lognoteredeadline" => self.redeadline = Some(Logging::Note),
            "nologredeadline" => self.redeadline = None,
            "logrefile" => self.refile = Some(Logging::Time),
            "lognoterefile" => self.refile = Some(Logging::Note),
            "nologrefile" => self.refile = None,
            _ => return false,
        }
        true
    }

    /// `org-local-logging`: the words of a `LOGGING` property, which start
    /// from no logging at all.
    fn local(&mut self, value: &str, keywords: &TodoKeywords) {
        self.done = None;
        self.repeat = None;
        let mut states = Vec::new();
        for word in value.split_whitespace() {
            // A word of `org-startup-options` is never a state's.
            if !self.set(word) {
                if let Some(state) = log_state_settings(word).filter(|(name, ..)| keywords.contains(name)) {
                    states.insert(0, state);
                }
            }
        }
        self.states = Some(states);
    }
}

/// `org-extract-log-state-settings`: `WAIT(w@/!)` is `WAIT`, a note when
/// entered and the time when left; a word that logs nothing is none.
fn log_state_settings(word: &str) -> Option<LogState> {
    let kind = |c: char| if c == '!' { Logging::Time } else { Logging::Note };
    let (name, spec) = match word.strip_suffix(')').and_then(|rest| rest.rsplit_once('(')) {
        Some((name, spec)) => (name, spec),
        None => (word, ""),
    };
    let mut chars = spec.chars().peekable();
    // A key, then `!` or `@`, then `/` and `!` or `@`.
    if chars.peek().is_some_and(|c| !"!@/".contains(*c)) {
        chars.next();
    }
    let enter = chars.next_if(|c| "!@".contains(*c)).map(kind);
    let leave = if chars.next_if_eq(&'/').is_some() { chars.next_if(|c| "!@".contains(*c)).map(kind) } else { None };
    if chars.next().is_some() {
        // Not the form Org reads: the whole word is the keyword.
        return None;
    }
    (enter.is_some() || leave.is_some()).then(|| (name.to_string(), enter, leave))
}

const TAB_WIDTH: usize = 8;
const KEYWORDS: [&str; 3] = ["SCHEDULED:", "DEADLINE:", "CLOSED:"];

pub(crate) fn is_blank(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

/// The column reached after `text`, which starts a line.
pub(crate) fn column(text: &str) -> usize {
    text.chars()
        .fold(0, |column, c| if c == '\t' { (column / TAB_WIDTH + 1) * TAB_WIDTH } else { column + char_width(c) })
}

fn leading_blanks(line: &str) -> usize {
    line.bytes().take_while(|b| is_blank(*b)).count()
}

/// Whether the line starts a planning line (`org-planning-line-re`).
fn is_planning_line(line: &str) -> bool {
    let line = &line[leading_blanks(line)..];
    KEYWORDS.iter().any(|keyword| line.starts_with(keyword))
}

/// Number of stars if the line is a headline.
pub(crate) fn headline_level(line: &str) -> Option<usize> {
    let stars = line.bytes().take_while(|b| *b == b'*').count();
    (stars > 0 && line.as_bytes().get(stars) == Some(&b' ')).then_some(stars)
}

/// `org--align-tags-here`: where the blanks before the tags of a headline
/// start and end, and the blanks that belong there instead; nothing when
/// the line has no tags or they stand where they should.
pub(crate) fn tags_padding(line: &str, tags_column: i64) -> Option<(usize, usize, String)> {
    // `org-tag-line-re`: the tags follow a blank, or the stars' space.
    let (blanks, tags) = tag_group(line)?;
    let tags_text = line[tags..].trim_end_matches([' ', '\t']);
    let wanted = if tags_column >= 0 {
        tags_column as usize
    } else {
        (tags_column.unsigned_abs() as usize).saturating_sub(width(tags_text))
    };
    // At least one space after the title or the stars.
    let before = column(&line[..blanks]);
    let target = wanted.max(before + 1);
    (target != column(&line[..tags])).then(|| (blanks, tags, " ".repeat(target - before)))
}

/// `org-time-string-to-time`: the first date in `text` and the time
/// after it.
fn time_of(text: &str) -> Result<DateTime, String> {
    let parsed = parse_time_string(text).ok_or("Not a standard Org time string")?;
    let (hour, minute) = parsed.time.unwrap_or((0, 0));
    Ok(DateTime::new(parsed.date, hour, minute))
}

/// The next match of `org-repeat-re` at or after `from`, before `end`:
/// an active timestamp with a repeater. Returns where the timestamp
/// starts and where the repeater ends.
fn next_repeater(text: &str, from: usize, end: usize) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut at = from;
    while let Some(open) = text[at..end].find('<').map(|index| at + index) {
        at = open + 1;
        let is_date = bytes.get(open + 1..open + 11).is_some_and(|date| {
            date.iter()
                .enumerate()
                .all(|(index, b)| if index == 4 || index == 7 { *b == b'-' } else { b.is_ascii_digit() })
        });
        if !is_date || bytes.get(open + 11) != Some(&b' ') {
            continue;
        }
        // The first repeater before a closing bracket or the line end.
        let limit =
            bytes[open + 12..end].iter().position(|b| matches!(b, b'>' | b'\n')).map_or(end, |index| open + 12 + index);
        let mut plus = open + 12;
        while let Some(index) = text[plus..limit].find('+') {
            let sign = plus + index;
            let digits = bytes[sign + 1..limit].iter().take_while(|b| b.is_ascii_digit()).count();
            let unit = sign + 1 + digits;
            if digits > 0 && unit < limit && matches!(bytes[unit], b'h' | b'd' | b'w' | b'm' | b'y') {
                let mut repeater_end = unit + 1;
                // `/N[hdwmy]`: the deadline of a habit.
                if bytes.get(repeater_end) == Some(&b'/') {
                    let digits = bytes[repeater_end + 1..limit].iter().take_while(|b| b.is_ascii_digit()).count();
                    let unit = repeater_end + 1 + digits;
                    if digits > 0 && unit < limit && matches!(bytes[unit], b'h' | b'd' | b'w' | b'm' | b'y') {
                        repeater_end = unit + 1;
                    }
                }
                return Some((open, repeater_end));
            }
            plus = sign + 1;
        }
    }
    None
}

/// The repeater of a match of `org-repeat-re`: its kind (`.`, `+` or
/// none), its count and its unit.
fn repeater_parts(stamp: &str) -> Option<(Option<char>, i64, char)> {
    let bytes = stamp.as_bytes();
    (0..bytes.len()).find_map(|at| {
        let (kind, sign) = match bytes[at] {
            b'.' | b'+' if bytes.get(at + 1) == Some(&b'+') => (Some(char::from(bytes[at])), at + 1),
            b'+' => (None, at),
            _ => return None,
        };
        let digits = bytes[sign + 1..].iter().take_while(|b| b.is_ascii_digit()).count();
        let unit = *bytes.get(sign + 1 + digits)?;
        let count = stamp[sign + 1..sign + 1 + digits].parse().ok()?;
        matches!(unit, b'h' | b'd' | b'w' | b'm' | b'y').then_some((kind, count, char::from(unit)))
    })
}

impl<'a> Editor<'a> {
    pub(crate) fn new(text: &str, keywords: &'a TodoKeywords, settings: &'a Settings, now: DateTime) -> Editor<'a> {
        let log = Log::new(settings, text);
        Editor {
            text: text.to_string(),
            keywords,
            settings,
            now,
            heading: 0,
            note: None,
            repeated: None,
            before_repeat: None,
            log,
        }
    }

    pub(crate) fn replace(&mut self, start: usize, end: usize, new: &str) {
        self.text.replace_range(start..end, new);
        if end <= self.heading {
            self.heading = self.heading + new.len() - (end - start);
        }
    }

    pub(crate) fn line_start(&self, at: usize) -> usize {
        self.text[..at].rfind('\n').map_or(0, |index| index + 1)
    }

    fn indent_settings(&self) -> indent::Settings {
        indent::Settings {
            adapt: self.settings.adapt,
            log_into_drawer: self.settings.log_into_drawer.clone(),
            ..indent::Settings::default()
        }
    }

    /// `org-indent-region` over the lines from `start` up to `end`.
    pub(crate) fn indent_region(&mut self, start: usize, end: usize) {
        for edit in indent::indent_region(&self.text, start, end, &self.indent_settings()).into_iter().rev() {
            self.replace(edit.start, edit.end, &edit.text);
        }
    }

    /// `org-indent-line` on the line at `at`.
    pub(crate) fn indent_line(&mut self, at: usize) {
        if let Some(edit) = indent::indent_line(&self.text, at, &self.indent_settings()) {
            self.replace(edit.start, edit.end, &edit.text);
        }
    }

    /// The indentation of a planning line Org adds below the headline:
    /// past the stars with `org-adapt-indentation`.
    pub(crate) fn planning_indentation(&self) -> String {
        match self.settings.adapt {
            Adapt::Off => String::new(),
            _ => " ".repeat(self.line(self.heading).bytes().take_while(|b| *b == b'*').count() + 1),
        }
    }

    /// The end of the line holding `at`, before its line break.
    pub(crate) fn line_end(&self, at: usize) -> usize {
        self.text[at..].find('\n').map_or(self.text.len(), |index| at + index)
    }

    /// The start of the line after the one holding `at`, or the end of
    /// the text.
    pub(crate) fn next_line(&self, at: usize) -> usize {
        (self.line_end(at) + 1).min(self.text.len())
    }

    pub(crate) fn line(&self, start: usize) -> &str {
        &self.text[start..self.line_end(start)]
    }

    /// The start of the first headline after the line holding `at`, or
    /// the end of the text.
    pub(crate) fn next_heading(&self, at: usize) -> usize {
        let mut line = self.next_line(at);
        while line < self.text.len() {
            if (line == 0 || self.text.as_bytes()[line - 1] == b'\n') && headline_level(self.line(line)).is_some() {
                return line;
            }
            let next = self.next_line(line);
            if next == line {
                break;
            }
            line = next;
        }
        self.text.len()
    }

    /// `org-entry-end-position`
    pub(crate) fn entry_end(&self) -> usize {
        // A headline that ends the text without a line break has no body.
        if self.line_end(self.heading) == self.text.len() {
            return self.text.len();
        }
        self.next_heading(self.heading)
    }

    /// The planning line of the entry, if it has one.
    fn planning_line(&self) -> Option<usize> {
        let after = self.line_end(self.heading);
        (after < self.text.len() && is_planning_line(self.line(after + 1))).then_some(after + 1)
    }

    /// The property drawer of the entry: the start of its first line and
    /// of its `:END:` line.
    pub(crate) fn property_drawer(&self) -> Option<(usize, usize)> {
        let mut line = self.planning_line().unwrap_or(self.heading);
        if self.line_end(line) == self.text.len() {
            return None;
        }
        line = self.next_line(line);
        let end = self.entry_end();
        if line >= end || !self.line(line).trim_matches([' ', '\t']).eq_ignore_ascii_case(":PROPERTIES:") {
            return None;
        }
        let mut at = self.next_line(line);
        while at < end && at > line {
            let text = self.line(at).trim_matches([' ', '\t']);
            if text.eq_ignore_ascii_case(":END:") {
                return Some((line, at));
            }
            // A property line: `:KEY:` or `:KEY: value`.
            let key_end = text.strip_prefix(':').and_then(|rest| rest.find(':'));
            let valid = key_end.is_some_and(|key| {
                key > 0
                    && !text[1..1 + key].contains(char::is_whitespace)
                    && text[key + 2..].chars().next().is_none_or(|c| c == ' ' || c == '\t')
            });
            if !valid {
                return None;
            }
            let next = self.next_line(at);
            if next == at {
                break;
            }
            at = next;
        }
        None
    }

    /// `org-end-of-meta-data`: the start of the line after the headline,
    /// its planning line and its property drawer; the end of the text
    /// when nothing follows.
    pub(crate) fn end_of_meta_data(&self) -> usize {
        let mut last = self.planning_line().unwrap_or(self.heading);
        if let Some((_, drawer_end)) = self.property_drawer() {
            last = drawer_end;
        }
        self.next_line(last)
    }

    // ----- the headline

    /// Make the headline at or above byte `offset` the entry's.
    pub(crate) fn go_to_heading(&mut self, offset: usize) -> Result<(), String> {
        let mut line = self.line_start(offset.min(self.text.len()));
        while headline_level(self.line(line)).is_none() {
            if line == 0 {
                return Err("Before first headline".to_string());
            }
            line = self.line_start(line - 1);
        }
        self.heading = line;
        Ok(())
    }

    /// The keyword of the headline as `org-todo` sees it, and where the
    /// text it replaces ends: the spaces after the stars, the keyword and
    /// the blanks after it.
    fn keyword_match(&self) -> (Option<String>, usize, usize) {
        let line = self.line(self.heading);
        let stars = line.bytes().take_while(|b| *b == b'*').count();
        let spaces = line[stars..].bytes().take_while(|b| *b == b' ').count();
        let rest = &line[stars + spaces..];
        if spaces > 0 {
            for name in self.keywords.names() {
                let Some(after) = rest.strip_prefix(name) else { continue };
                let following = after.bytes().take_while(|b| *b == b' ').count();
                if following > 0 {
                    return (
                        Some(name.to_string()),
                        self.heading + stars,
                        self.heading + stars + spaces + name.len() + following,
                    );
                }
                if after.bytes().all(is_blank) {
                    return (Some(name.to_string()), self.heading + stars, self.heading + line.len());
                }
            }
        }
        (None, self.heading + stars, self.heading + stars + spaces)
    }

    /// `org--align-tags-here` for the headline at `line`.
    fn align_tags(&mut self, line: usize) {
        if let Some((blanks, tags, padding)) = tags_padding(self.line(line), self.settings.tags_column) {
            self.replace(line + blanks, line + tags, &padding);
        }
    }

    /// The nearest headline above `at` with fewer than `level` stars.
    fn parent_of(&self, mut at: usize, level: usize) -> Option<(usize, usize)> {
        while at > 0 {
            at = self.line_start(at - 1);
            if let Some(found) = headline_level(self.line(at)).filter(|found| *found < level) {
                return Some((at, found));
            }
        }
        None
    }

    /// `org-update-parent-todo-statistics`: the cookies in the parent's
    /// headline count its children that have a keyword, and those done.
    ///
    /// With `recursive` in the `COOKIE_DATA` property the count covers the
    /// whole subtree, and the ancestors up to the entry that sets the
    /// property are updated as well.
    pub(crate) fn update_parent_statistics(&mut self) {
        let has_word =
            |data: &str, word: &str| data.to_lowercase().split(|c: char| !c.is_alphanumeric()).any(|w| w == word);
        let Some(level) = headline_level(self.line(self.heading)) else { return };
        let Some(mut ancestor) = self.parent_of(self.heading, level) else { return };

        // The property, from the parent or the nearest ancestor that has it.
        let mut owner = Some(ancestor);
        let mut inherited = String::new();
        let mut limit = 0;
        while let Some((at, at_level)) = owner {
            inherited = self.cookie_data(at);
            if !inherited.is_empty() {
                limit = at;
                break;
            }
            owner = self.parent_of(at, at_level);
        }
        let recursive = has_word(&inherited, "recursive");

        loop {
            let (parent, parent_level) = ancestor;
            if has_word(&self.cookie_data(parent), "checkbox") {
                return;
            }
            // Count the entries below that have a keyword.
            let (mut all, mut done) = (0, 0);
            let mut line = self.next_heading(parent);
            while line < self.text.len() {
                let Some(found) = headline_level(self.line(line)) else { break };
                if found <= parent_level {
                    break;
                }
                if recursive || found == level {
                    if let Some(keyword) = self.complex_keyword(line) {
                        all += 1;
                        if self.keywords.is_done(&keyword) {
                            done += 1;
                        }
                    }
                }
                let next = self.next_heading(line);
                if next == line {
                    break;
                }
                line = next;
            }

            // Rewrite every cookie on the headline.
            let mut from = parent;
            let mut changed = false;
            loop {
                let line_end = self.line_end(parent);
                let Some((start, end, percent)) = next_cookie(&self.text[..line_end], from) else { break };
                let new = if percent { format!("[{}%]", 100 * done / all.max(1)) } else { format!("[{done}/{all}]") };
                from = start + new.len();
                if self.text[start..end] != new {
                    self.replace(start, end, &new);
                    changed = true;
                }
            }
            if changed {
                self.align_tags(parent);
            }

            match self.parent_of(parent, parent_level).filter(|(at, _)| recursive && *at >= limit) {
                Some(next) => ancestor = next,
                None => return,
            }
        }
    }

    /// The `COOKIE_DATA` property of the entry at `heading`.
    fn cookie_data(&self, heading: usize) -> String {
        let mut line = self.next_line(heading);
        if line < self.text.len() && is_planning_line(self.line(line)) {
            line = self.next_line(line);
        }
        if line >= self.text.len() || !self.line(line).trim_matches([' ', '\t']).eq_ignore_ascii_case(":PROPERTIES:") {
            return String::new();
        }
        let mut at = self.next_line(line);
        while at < self.text.len() && at > line {
            let text = self.line(at).trim_matches([' ', '\t']);
            if text.eq_ignore_ascii_case(":END:") || !text.starts_with(':') {
                break;
            }
            if let Some(value) =
                text.get(..13).filter(|key| key.eq_ignore_ascii_case(":COOKIE_DATA:")).map(|_| &text[13..])
            {
                return value.trim().to_string();
            }
            let next = self.next_line(at);
            if next == at {
                break;
            }
            at = next;
        }
        String::new()
    }

    /// The keyword `org-complex-heading-regexp` finds in the headline at
    /// `line`: after the stars and spaces, followed by a space or the end.
    fn complex_keyword(&self, line: usize) -> Option<String> {
        let text = self.line(line);
        let stars = text.bytes().take_while(|b| *b == b'*').count();
        let spaces = text[stars..].bytes().take_while(|b| *b == b' ').count();
        let rest = &text[stars + spaces..];
        self.keywords
            .names()
            .find(|name| {
                rest.strip_prefix(name).is_some_and(|after| {
                    after.starts_with(' ')
                        || after.trim_end_matches([' ', '\t']).is_empty()
                        || tag_group(after).is_some_and(|(blanks, _)| blanks == 0)
                })
            })
            .map(str::to_string)
    }

    // ----- planning

    /// `org-add-planning-info` for `CLOSED:`: remove it, and with `add`
    /// write a new one with the current time.
    fn planning_closed(&mut self, add: bool) {
        match self.planning_line() {
            Some(line) => {
                let content = line + leading_blanks(self.line(line));
                let line_end = self.line_end(line);
                if let Some(found) = find_closed(&self.text[..line_end], content) {
                    // Delete up to the next keyword with its timestamp.
                    let stop = next_planning_keyword(&self.text[..line_end], found.1).unwrap_or(line_end);
                    self.replace(found.0, stop, "");
                }
                let line_end = self.line_end(line);
                if self.text[content..line_end].bytes().all(is_blank) && !add {
                    // Nothing is left: remove the line.
                    self.replace(line - 1, line_end, "");
                    return;
                }
                let trimmed = self.text[content..line_end].trim_end_matches([' ', '\t']).len();
                self.replace(content + trimmed, line_end, "");
                if add {
                    // `org-log-done-with-time`
                    let stamp = format_timestamp(self.now, self.settings.log_done_with_time, true, "");
                    let space = if content == self.line_end(line) { "" } else { " " };
                    self.replace(content, content, &format!("CLOSED: {stamp}{space}"));
                }
            }
            None if add => {
                let at = self.line_end(self.heading);
                let stamp = format_timestamp(self.now, self.settings.log_done_with_time, true, "");
                let indentation = self.planning_indentation();
                self.replace(at, at, &format!("\n{indentation}CLOSED: {stamp}"));
            }
            None => {}
        }
    }

    /// `org-remove-timestamp-with-keyword` for `SCHEDULED:`.
    fn remove_scheduled(&mut self) {
        let end = self.entry_end();
        let mut matches = Vec::new();
        let mut at = self.heading;
        while let Some(index) = self.text[at..end].find("SCHEDULED:") {
            let start = at + index;
            at = start + 1;
            let bytes = self.text.as_bytes();
            let spaces = bytes[start + 10..end].iter().take_while(|b| **b == b' ').count();
            let open = start + 10 + spaces;
            if spaces == 0 || bytes.get(open) != Some(&b'<') || !word_start(&self.text, start) {
                continue;
            }
            let Some(close) = bytes[open + 1..end].iter().position(|b| matches!(b, b'>' | b'\n')) else { continue };
            if close == 0 || bytes[open + 1 + close] != b'>' {
                continue;
            }
            let mut stop = open + 1 + close + 1;
            stop += bytes[stop..].iter().take_while(|b| is_blank(**b)).count();
            matches.push((start, stop));
            at = stop;
        }
        for (start, stop) in matches.into_iter().rev() {
            self.replace(start, stop, "");
            let line = self.line_start(start);
            let before = &self.text[line..start];
            if before.bytes().any(|b| !is_blank(b)) && before.ends_with(' ') {
                self.replace(start - 1, start, "");
            } else if self.line(line).bytes().all(is_blank) {
                let stop = self.next_line(line);
                self.replace(line, stop, "");
            }
        }
    }

    // ----- properties and notes

    /// `org-entry-put` for `LAST_REPEAT`.
    fn put_last_repeat(&mut self) {
        let value = format_timestamp(self.now, true, true, "");
        self.put_property("LAST_REPEAT", &value);
    }

    /// `org-entry-put`: the property `key` of the entry set to `value`, in
    /// the property drawer, which is made when there is none.
    pub(crate) fn put_property(&mut self, key: &str, value: &str) {
        let (drawer, end) = match self.property_drawer() {
            Some(found) => found,
            None => {
                // `org-insert-property-drawer`: below the headline and its
                // planning line, indented as `org-indent-region` has it.
                let after = self.planning_line().unwrap_or(self.heading);
                let at = self.line_end(after);
                let at_end = at == self.text.len();
                let drawer = format!("\n:PROPERTIES:\n:END:{}", if at_end { "\n" } else { "" });
                self.replace(at, at, &drawer);
                let end = self.line_end(self.next_line(at + 1));
                self.indent_region(at + 1, end);
                self.property_drawer().expect("the new drawer")
            }
        };
        // Replace an existing value, or add a line before `:END:`; then
        // `org-indent-line` on it.
        let key = format!(":{key}:");
        let mut at = self.next_line(drawer);
        while at < end {
            let text = self.line(at).trim_start_matches([' ', '\t']);
            let is_it = text.get(..key.len()).is_some_and(|found| found.eq_ignore_ascii_case(&key))
                && text[key.len()..].chars().next().is_none_or(|c| c == ' ' || c == '\t');
            if is_it {
                let line_end = self.line_end(at);
                self.replace(at, line_end, &format!("{key} {value}"));
                self.indent_line(at);
                return;
            }
            at = self.next_line(at);
        }
        self.replace(end, end, &format!("{key} {value}\n"));
        self.indent_line(end);
    }

    /// Whether the entry has a clock line.
    fn has_clock(&self) -> bool {
        let end = self.entry_end();
        self.text[self.heading..end].lines().any(|line| {
            let line = line.trim_start_matches([' ', '\t']);
            line.strip_prefix("CLOCK:").is_some_and(|rest| rest.trim_start_matches([' ', '\t']).starts_with('['))
        })
    }

    /// `org-store-log-note`: write the note at `org-log-beginning`, with
    /// the text the user wrote for it.
    fn store_note(&mut self, note: &Note, text: &str) {
        // `%s` and `%S`: a state in quotes, an active timestamp as an
        // inactive one.
        let quoted = |state: &Option<String>| match state {
            None => String::new(),
            Some(state) if ACTIVE_STAMP.is_match(state) => {
                let inner: String = state.chars().skip(1).take(state.chars().count().saturating_sub(2)).collect();
                format!("\"[{inner}]\"")
            }
            Some(state) => format!("\"{state}\""),
        };
        let now = self.now;
        let template = self.settings.note_headings.of(note.purpose);
        let heading = (!template.trim().is_empty()).then(|| {
            crate::text::replace_escapes(template, |c| {
                Some(match c {
                    'u' => login_name(),
                    'U' => full_name(),
                    't' => format_timestamp(now, true, true, ""),
                    'T' => format_timestamp(now, true, false, ""),
                    'd' => format_timestamp(now, false, true, ""),
                    'D' => format_timestamp(now, false, false, ""),
                    's' => quoted(&note.state),
                    'S' => quoted(&note.previous),
                    _ => return None,
                })
            })
        });
        let mut lines: Vec<String> = note_lines(text).into_iter().map(str::to_string).collect();
        if let Some(mut heading) = heading {
            if !lines.is_empty() {
                heading.push_str(" \\\\");
            }
            lines.insert(0, heading);
        }
        if lines.is_empty() {
            return;
        }

        let mut at = self.log_beginning();
        // The note needs a line of its own.
        if at > 0 && self.text.as_bytes()[at - 1] != b'\n' {
            self.replace(at, at, "\n");
            at += 1;
        } else if self.line(at).bytes().any(|b| !is_blank(b)) {
            self.replace(at, at, "\n");
        }
        // In an existing list, the top level of the list; else
        // `org-indent-line` on the note's line.
        let blanks = leading_blanks(self.line(at));
        let top = crate::list::in_item(&self.text, at)
            .and_then(|item| crate::list::structure(&self.text, item).first().map(|first| first.ind.max(0) as usize));
        let indent = match top.or_else(|| indent::line_column(&self.text, at, &self.indent_settings())) {
            Some(column) => " ".repeat(column),
            None => self.line(at)[..blanks].to_string(),
        };
        let mut item = format!("{indent}- {}", lines[0]);
        // The text below the heading, indented to the item's body.
        for line in &lines[1..] {
            item.push('\n');
            if !line.is_empty() {
                item.push_str(&indent);
                item.push_str("  ");
            }
            item.push_str(line);
        }
        self.replace(at, at + blanks, &item);
    }

    /// `org-log-beginning`: where a note goes, in the log drawer or after
    /// the entry's meta data; at the top of the notes there, or with
    /// `org-log-states-order-reversed` off, at their bottom.
    fn log_beginning(&mut self) -> usize {
        if let Some(drawer) = self.log_into_drawer() {
            return self.log_drawer(&drawer);
        }
        // `org-end-of-meta-data`, past all drawers with
        // `org-log-state-notes-insert-after-drawers`.
        let meta =
            if self.settings.notes_after_drawers { self.end_of_all_meta_data() } else { self.end_of_meta_data() };
        // Past blank lines, to the start of the next line with text.
        let skipped = self.text[meta..].bytes().take_while(|b| matches!(b, b' ' | b'\t' | b'\n')).count();
        let mut at = self.line_start(meta + skipped);
        if !self.log.reversed {
            // `org-skip-over-state-notes`, then back over blank lines.
            at = self.skip_over_state_notes(at);
            let back = self.text[..at].bytes().rev().take_while(|b| matches!(b, b' ' | b'\t' | b'\n')).count();
            at = self.next_line(at - back);
        }
        let at = at.max(meta);
        if at == self.line_start(at) {
            at
        } else {
            self.next_line(at)
        }
    }

    /// `org-end-of-meta-data` with `full` `t`: past the planning line,
    /// the property drawer, and then clock lines, blank lines and drawers.
    fn end_of_all_meta_data(&self) -> usize {
        let mut at = self.end_of_meta_data();
        let end = self.entry_end();
        if headline_level(self.line(at)).is_some() {
            return at;
        }
        while at < self.text.len() {
            let line = self.line(at);
            let trimmed = line.trim_start_matches([' ', '\t']);
            if trimmed.trim_end_matches([' ', '\t']).is_empty() || trimmed.starts_with("CLOCK:") {
                at = self.next_line(at);
                continue;
            }
            if !DRAWER_LINE.is_match(line) {
                break;
            }
            // Its `:END:`, before the next headline.
            let mut close = self.next_line(at);
            while close < end && !self.line(close).trim_matches([' ', '\t']).eq_ignore_ascii_case(":END:") {
                close = self.next_line(close);
            }
            if close >= end {
                break;
            }
            at = self.next_line(close);
        }
        at
    }

    /// `org-skip-over-state-notes` at `at`: past the items of the list
    /// there that are state notes (`org-log-note-headings`' `state`).
    fn skip_over_state_notes(&self, at: usize) -> usize {
        let Some(item) = crate::list::in_item(&self.text, at) else { return at };
        let list = crate::list::structure(&self.text, item);
        // `regexp-quote` of the heading, its escapes as regexps.
        const PATTERNS: [(char, &str); 8] = [
            ('d', r"\[([0-9]{4}-[0-9]{2}-[0-9]{2}(?: .*?)?)\]"),
            ('t', r"\[([0-9]{4}-[0-9]{2}-[0-9]{2}(?: .*?)?)\]"),
            ('D', r"<([0-9]{4}-[0-9]{2}-[0-9]{2}(?: .*?)?)>"),
            ('T', r"<([0-9]{4}-[0-9]{2}-[0-9]{2}(?: .*?)?)>"),
            ('s', r#"(?:"\S+")?"#),
            ('S', r#"(?:"\S+")?"#),
            ('u', ".*?"),
            ('U', ".*?"),
        ];
        let marked = crate::text::replace_escapes(&self.settings.note_headings.state, |c| {
            PATTERNS.iter().position(|(key, _)| *key == c).map(|index| format!("\0{index}\0"))
        });
        let mut pattern = regex::escape(&marked);
        for (index, (_, regexp)) in PATTERNS.iter().enumerate() {
            pattern = pattern.replace(&format!("\0{index}\0"), regexp);
        }
        let pattern = format!("^[ \t]*- +{}", SPACES.replace_all(&pattern, " +"));
        let Ok(state) = regex::Regex::new(&pattern) else { return at };
        let mut at = item;
        while at < self.text.len() && state.is_match(&self.text[at..]) {
            match crate::list::next_item_or_end(&list, at) {
                Some(next) if next > at => at = next,
                _ => break,
            }
        }
        at
    }

    /// `org-log-beginning` with a drawer: the line below the drawer's
    /// first line, or with `org-log-states-order-reversed` off, its
    /// `:END:` line. The drawer is created after the entry's planning line
    /// and property drawer when it does not exist.
    pub(crate) fn log_drawer(&mut self, name: &str) -> usize {
        let meta = self.end_of_meta_data();
        let end = self.entry_end();
        let opening = format!(":{name}:");
        let mut line = meta;
        while line < end {
            if self.line(line).trim_matches([' ', '\t']).eq_ignore_ascii_case(&opening) && self.drawer_closes(line, end)
            {
                if !self.log.reversed {
                    let mut close = self.next_line(line);
                    while !self.line(close).trim_matches([' ', '\t']).eq_ignore_ascii_case(":END:") {
                        close = self.next_line(close);
                    }
                    return close;
                }
                return self.next_line(line);
            }
            let next = self.next_line(line);
            if next == line {
                break;
            }
            line = next;
        }
        // Create it, as `org-indent-region` indents it. At the end of
        // the text, or right before the next headline, it goes on a new
        // line after what is there.
        let start = if meta == self.text.len() && !self.text.is_empty() && !self.text.ends_with('\n') {
            self.replace(meta, meta, &format!("\n{opening}\n:END:"));
            meta + 1
        } else {
            self.replace(meta, meta, &format!("{opening}\n:END:\n"));
            meta
        };
        let end = self.line_end(self.next_line(start));
        self.indent_region(start, end);
        self.next_line(start)
    }

    /// Whether the drawer opening at `line` has its `:END:` before `end`.
    pub(crate) fn drawer_closes(&self, line: usize, end: usize) -> bool {
        let mut at = self.next_line(line);
        while at < end && at > line {
            if self.line(at).trim_matches([' ', '\t']).eq_ignore_ascii_case(":END:") {
                return true;
            }
            let next = self.next_line(at);
            if next == at {
                break;
            }
            at = next;
        }
        false
    }

    // ----- repeaters

    /// The entry from its headline to its end, parsed on its own: enough
    /// to tell which timestamps Org takes for timestamps.
    fn entry_outline(&self) -> (org_syntax::Parse, Outline, usize) {
        let end = self.entry_end();
        let text = &self.text[self.heading..end];
        let parse = org_syntax::parse(text, self.keywords);
        let outline = Outline::new(&parse, text, "");
        (parse, outline, end)
    }

    /// `org-at-timestamp-p` with `agenda`: whether the character at `at`
    /// is in a timestamp, on a planning line or in a property drawer.
    fn in_timestamp(&self, parse: &org_syntax::Parse, at: usize) -> bool {
        let offset = (at - self.heading) as u32;
        let token = match parse.syntax().token_at_offset(offset.into()) {
            TokenAtOffset::None => return false,
            TokenAtOffset::Single(token) => token,
            TokenAtOffset::Between(_, right) => right,
        };
        token.parent_ancestors().any(|node| {
            matches!(node.kind(), SyntaxKind::PLANNING | SyntaxKind::NODE_PROPERTY)
                || (node.kind() == SyntaxKind::TIMESTAMP
                    && !node.parent().is_some_and(|parent| parent.kind() == SyntaxKind::CLOCK))
        })
    }

    /// `org-blocker-hook` with `org-block-todo-from-checkboxes` and
    /// `org-block-todo-from-children-or-siblings-or-parent`: why the entry
    /// may not go from `this` to the done state `state`, if it may not.
    fn blocked(&self, this: &Option<String>, state: &Option<String>) -> Option<String> {
        let settings = self.settings;
        if !settings.enforce_todo_dependencies && !settings.enforce_todo_checkbox_dependencies {
            return None;
        }
        let to = state.as_deref()?;
        if this.as_deref().is_some_and(|this| self.keywords.is_done(this)) || !self.keywords.is_done(to) {
            return None;
        }
        self.blocked_by()
    }

    /// What blocks the entry from being done, if anything does: the
    /// checks of [`Editor::blocked`] past the states.
    fn blocked_by(&self) -> Option<String> {
        let settings = self.settings;
        if settings.enforce_todo_checkbox_dependencies {
            static UNCHECKED: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
                regex::Regex::new(
                    r"(?m)^[ \t]*(?:[-+]|[ \t]\*|(?:[0-9]+|[A-Za-z])[.)])[ \t]+(?:\[@(?:start:)?(?:[0-9]+|[A-Za-z])\][ \t]*)?\[[- ]\]",
                )
                .expect("a valid regex")
            });
            if UNCHECKED.is_match(&self.text[self.heading..self.entry_end()]) {
                return Some("contained checkboxes".to_string());
            }
        }
        if !settings.enforce_todo_dependencies {
            return None;
        }
        let parse = org_syntax::parse(&self.text, self.keywords);
        let outline = Outline::new(&parse, &self.text, "");
        let entries = &outline.entries;
        let index = outline.entry_at(self.heading)?;
        let open = |entry: usize| entries[entry].todo.is_some() && !entries[entry].done;
        let quoted =
            |entry: usize| format!("\"{}\"", org_get_heading(&entries[entry].head, entries[entry].todo.as_deref()));
        // A child not done.
        let level = entries[index].level;
        if let Some(child) =
            (index + 1..entries.len()).take_while(|&child| entries[child].level > level).find(|&child| open(child))
        {
            return Some(quoted(child));
        }
        // An earlier sibling not done, under `ORDERED`.
        let ordered =
            |entry: usize| entries[entry].properties.iter().any(|(key, value)| key == "ORDERED" && value != "nil");
        let parent = entries[index].parent?;
        if ordered(parent) {
            if let Some(earlier) = (parent + 1..index).find(|&earlier| open(earlier)) {
                let line = &self.text[entries[earlier].offset..entries[earlier].line_end];
                return Some(format!("\"{line}\""));
            }
        }
        // An ancestor blocked so.
        let mut current = parent;
        loop {
            if !open(current) {
                return None;
            }
            let above = entries[current].parent?;
            if ordered(above) {
                if let Some(earlier) = (above + 1..current).find(|&earlier| open(earlier)) {
                    return Some(quoted(earlier));
                }
            }
            current = above;
        }
    }

    /// `org-entry-get` with inheritance and `literal-nil` at the entry.
    fn inherited(&self, name: &str) -> Option<String> {
        let parse = org_syntax::parse(&self.text, self.keywords);
        let outline = Outline::new(&parse, &self.text, "");
        outline.inherited_literal(outline.entry_at(self.heading), name)
    }

    /// `org-log-into-drawer`: the drawer notes go into, as the entry's
    /// `LOG_INTO_DRAWER` says (inherited; `nil` for none, `t` for
    /// `LOGBOOK`), or else the settings.
    fn log_into_drawer(&self) -> Option<String> {
        match self.inherited("LOG_INTO_DRAWER").as_deref() {
            Some("nil") => None,
            Some("t") => Some("LOGBOOK".to_string()),
            Some(name) => Some(name.to_string()),
            None => self.log.into_drawer.clone(),
        }
    }

    /// What entering `state` logs (`org-todo-log-states`).
    fn logs_entering(&self, state: &str) -> Option<Logging> {
        match &self.log.states {
            Some(states) => states.iter().find(|(name, ..)| name == state).and_then(|(_, enter, _)| *enter),
            None => self.keywords.logs_entering(state),
        }
    }

    /// What leaving `state` logs.
    fn logs_leaving(&self, state: &str) -> Option<Logging> {
        match &self.log.states {
            Some(states) => states.iter().find(|(name, ..)| name == state).and_then(|(_, _, leave)| *leave),
            None => self.keywords.logs_leaving(state),
        }
    }

    /// Whether any state logs.
    fn logs_any(&self) -> bool {
        match &self.log.states {
            Some(states) => !states.is_empty(),
            None => self.keywords.logs_any(),
        }
    }

    /// The first repeater of the entry, as `org-get-repeat` finds it.
    fn first_repeater(&self) -> Option<String> {
        let (parse, _, end) = self.entry_outline();
        let mut at = self.heading;
        while let Some((start, repeater_end)) = next_repeater(&self.text, at, end) {
            if self.in_timestamp(&parse, repeater_end.min(end - 1)) {
                let stamp = &self.text[start..repeater_end];
                let word = &stamp[stamp.rfind(' ').map_or(0, |index| index + 1)..];
                // Group 1 of `org-repeat-re`: where `[.+]?\+N[hdwmy]` starts.
                let repeater = word.char_indices().map(|(at, _)| &word[at..]).find(|rest| {
                    let rest = rest.strip_prefix(['.', '+']).filter(|after| after.starts_with('+')).unwrap_or(rest);
                    rest.strip_prefix('+').is_some_and(|after| after.starts_with(|c: char| c.is_ascii_digit()))
                });
                return Some(repeater.unwrap_or(word).to_string());
            }
            at = repeater_end;
        }
        None
    }

    /// `org-timestamp-change` with the cursor at `point`: move the
    /// timestamp there by `count` units.
    fn shift_timestamp(&mut self, point: usize, count: i64, unit: char, drop_delay: bool) -> Result<Placed, String> {
        let what = match unit {
            'h' => Unit::Hour,
            'd' => Unit::Day,
            'm' => Unit::Month,
            'y' => Unit::Year,
            // `M`: minutes.
            _ => Unit::Minute,
        };
        let shift = Shift { n: count, what: Some(what), updown: false, counted: false };
        // `org-todo` runs without case folding. The rounding of minutes
        // only applies to changes from the keyboard.
        let around = Around { clock: true, drop_delay, fold: false };
        stamp::change_in_place(&mut self.text, point, &shift, 1, around)
            .map_err(|refusal| refusal.message().to_string())
    }

    /// The date and time of the timestamp around `point`.
    fn timestamp_time(&self, point: usize) -> Result<DateTime, String> {
        let stamp = Stamp::around(&self.text, point).ok_or("Not at a timestamp")?;
        time_of(&self.text[stamp.start..stamp.end])
    }

    /// `org-auto-repeat-maybe`: the entry was marked done; if it repeats,
    /// move its timestamps on and reopen it.
    fn auto_repeat(&mut self, done_word: &Option<String>, previous: &Option<String>) -> Result<(), String> {
        let Some(repeat) = self.first_repeater() else { return Ok(()) };
        let count: i64 = repeat[1..]
            .trim_start_matches('+')
            .bytes()
            .take_while(u8::is_ascii_digit)
            .fold(0, |n, b| n * 10 + i64::from(b - b'0'));
        if count == 0 {
            return Ok(());
        }

        // Back to an open state.
        let (_, outline, _) = self.entry_outline();
        let property = outline.entries.first().and_then(|entry| {
            entry.properties.iter().find(|(key, _)| key == "REPEAT_TO_STATE").map(|(_, value)| value.clone())
        });
        let to_state = property.or(match &self.settings.repeat_to_state {
            RepeatTo::Head => None,
            RepeatTo::State(state) => Some(state.clone()),
            RepeatTo::Previous => previous.clone(),
        });
        let head = previous.as_deref().and_then(|state| self.keywords.head(state)).map(str::to_string);
        let state = to_state.filter(|state| self.keywords.contains(state)).or(head);
        let (this, _, _) = self.keyword_match();
        self.set_state(&Target::State(state.clone()), false)?;
        // `org-todo` back to that state runs with no logging of its own
        // (`org-log-done`, `org-todo-log-states` nil) but reads the entry's
        // `LOGGING` again: the states it names log.
        if self.log.states.is_some() {
            let record = state
                .as_deref()
                .and_then(|state| self.logs_entering(state))
                .or_else(|| this.as_deref().and_then(|this| self.logs_leaving(this)));
            if let (Some(_), Some(record)) = (&state, record) {
                let text = record == Logging::Note;
                self.note = Some(Note { purpose: Purpose::State, state: state.clone(), previous: this, text });
            }
        }

        self.planning_closed(false);
        if self.log.repeat.is_some() || self.has_clock() {
            self.put_last_repeat();
        }
        // Already set up for a note, `org-log-repeat` `note` makes it want
        // text.
        if self.log.repeat == Some(Logging::Note) {
            if let Some(note) = &mut self.note {
                note.text = true;
            }
        }
        if self.log.repeat.is_some() && self.note.is_none() {
            self.note = Some(Note {
                purpose: Purpose::State,
                state: done_word
                    .clone()
                    .or_else(|| self.keywords.names().find(|name| self.keywords.is_done(name)).map(str::to_string)),
                // Org writes `""` here for an entry that had no keyword.
                previous: Some(previous.clone().unwrap_or_default()),
                text: self.log.repeat == Some(Logging::Note),
            });
        }

        // A `SCHEDULED:` without repeater is of no use any more.
        let (_, outline, _) = self.entry_outline();
        let scheduled = outline.entries.first().and_then(|entry| entry.scheduled.as_ref());
        if scheduled.is_some_and(|planning| {
            planning.active && next_repeater(&format!("<{}>", planning.raw), 0, planning.raw.len() + 2).is_none()
        }) {
            self.remove_scheduled();
        }

        // Move every timestamp with a repeater. Like Org, this follows
        // the cursor: it starts behind the repeater, and every change is
        // made to the timestamp the cursor is then in.
        let mut message = "Entry repeats: ".to_string();
        let today = self.now.org_today(self.settings.extend_today_until);
        let mut at = self.heading;
        loop {
            let (parse, _, end) = self.entry_outline();
            let Some((start, repeater_end)) = next_repeater(&self.text, at, end) else { break };
            at = repeater_end;
            if !self.in_timestamp(&parse, repeater_end.min(end - 1)) {
                continue;
            }
            let mut point = repeater_end;
            let stamp = self.text[start..repeater_end].to_string();
            let Some((kind, count, unit)) = repeater_parts(&stamp) else { continue };
            let (count, unit) = if unit == 'w' { (count.saturating_mul(7), 'd') } else { (count, unit) };
            // A `++` repeater of 0 leaves its timestamp where it is:
            // shifting by nothing would never land in the future. Others
            // are changed by nothing, which writes the timestamp again
            // with the right day name.
            if count == 0 && kind == Some('+') {
                continue;
            }
            if unit == 'h' && !has_clock_time(&stamp) {
                return Err(format!("Cannot repeat in {count} hour(s) because no hour has been set"));
            }
            match kind {
                // `.+`: count from today (`org-today`, which late at night
                // is yesterday), or from now when repeating by hours.
                Some('.') => {
                    let time = time_of(&stamp)?;
                    if unit == 'h' {
                        let minutes = (self.now.day - time.day) * 1440 + self.now.minute - time.minute;
                        point = self.shift_timestamp(point, minutes, 'M', false)?.cursor;
                    } else {
                        point = self.shift_timestamp(point, today - time.day, 'd', false)?.cursor;
                    }
                }
                // `++`: keep the rhythm, but land in the future.
                Some('+') => {
                    let mut time = time_of(&stamp)?;
                    let mut shifts = 0;
                    loop {
                        let behind = if unit == 'h' {
                            (self.now.day, self.now.minute) >= (time.day, time.minute)
                        } else {
                            today >= time.day
                        };
                        if shifts > 0 && !behind {
                            break;
                        }
                        // Emacs asks whether to go on after ten shifts;
                        // this many are a timestamp far in the past.
                        if shifts == 100_000 {
                            return Err("Abort".to_string());
                        }
                        shifts += 1;
                        point = self.shift_timestamp(point, count, unit, false)?.cursor;
                        time = self.timestamp_time(point)?;
                    }
                    point = self.shift_timestamp(point, -count, unit, false)?.cursor;
                }
                _ => {}
            }
            let kind = self.repeat_kind(repeater_end);
            let moved = self.shift_timestamp(point, count, unit, true)?.stamp;
            message.push_str(&format!("{kind} {} ", &self.text[moved.0..moved.1]));
            at = moved.1;
        }
        self.repeated = Some(message.trim_end().to_string());
        Ok(())
    }

    /// What `org-auto-repeat-maybe` calls the timestamp at `at`: the
    /// planning keyword before it on the planning line, else `Plain:`.
    fn repeat_kind(&self, at: usize) -> String {
        let line_start = self.text[..at].rfind('\n').map_or(0, |index| index + 1);
        let heading_end = self.text[self.heading..].find('\n').map_or(self.text.len(), |index| self.heading + index);
        let line = &self.text[line_start..at];
        let planning = line_start == heading_end + 1
            && ["SCHEDULED:", "DEADLINE:", "CLOSED:"].iter().any(|keyword| line.trim_start().starts_with(keyword));
        if !planning {
            return "Plain:".to_string();
        }
        let scheduled = line.rfind("SCHEDULED:");
        let deadline = line.rfind("DEADLINE:");
        match (scheduled, deadline) {
            (Some(s), Some(d)) if d > s => "DEADLINE:",
            (Some(_), _) => "SCHEDULED:",
            (None, Some(_)) => "DEADLINE:",
            (None, None) => "Plain:",
        }
        .to_string()
    }

    // ----- the command

    /// `org-todo` on the entry at `self.heading`.
    fn set_state(&mut self, target: &Target, top: bool) -> Result<(), String> {
        // `org-local-logging` for the entry's `LOGGING`, inherited.
        if top {
            if let Some(logging) = self.inherited("LOGGING") {
                self.log.local(&logging, self.keywords);
            }
        }
        let (this, start, end) = self.keyword_match();
        let names: Vec<&str> = self.keywords.names().collect();
        let position = this.as_deref().and_then(|this| names.iter().position(|name| *name == this));
        let state: Option<String> = match target {
            Target::Next => match position {
                Some(index) => names.get(index + 1).map(|name| name.to_string()),
                None => names.first().map(|name| name.to_string()),
            },
            Target::Previous => match position {
                Some(0) => None,
                Some(index) => Some(names[index - 1].to_string()),
                None => names.last().map(|name| name.to_string()),
            },
            Target::Cycle { head, repeated } => {
                self.keywords.cycle(this.as_deref(), head.as_deref(), *repeated).map(str::to_string)
            }
            Target::NextSet { head } => self.keywords.next_set(this.as_deref(), head.as_deref()).map(str::to_string),
            Target::PreviousSet { head } => {
                self.keywords.previous_set(this.as_deref(), head.as_deref()).map(str::to_string)
            }
            Target::State(None) => None,
            Target::State(Some(state)) if state.is_empty() => None,
            Target::State(Some(state)) => {
                if !self.keywords.contains(state) {
                    return Err(format!("State `{state}' not valid in this file"));
                }
                Some(state.clone())
            }
        };
        if top {
            if let Some(reason) = self.blocked(&this, &state) {
                return Err(format!(
                    "TODO state change from {} to {} blocked (by {reason})",
                    this.as_deref().unwrap_or("nil"),
                    state.as_deref().unwrap_or("nil"),
                ));
            }
        }
        let replacement = match &state {
            Some(state) => format!(" {state} "),
            None => " ".to_string(),
        };
        self.replace(start, end, &replacement);

        let is_done = |state: &Option<String>| state.as_deref().is_some_and(|state| self.keywords.is_done(state));
        let is_open = |state: &Option<String>| {
            state.as_deref().is_some_and(|state| self.keywords.contains(state) && !self.keywords.is_done(state))
        };
        let now_done = is_done(&state) && !is_done(&this);
        if top && (self.logs_any() || self.log.done.is_some()) {
            let record = state
                .as_deref()
                .and_then(|state| self.logs_entering(state))
                .or_else(|| this.as_deref().and_then(|this| self.logs_leaving(this)));
            if state.is_none() || (is_open(&state) && !is_open(&this)) {
                // No longer done: a `CLOSED:` timestamp is out of date.
                self.planning_closed(false);
            }
            if now_done && self.log.done.is_some() {
                self.planning_closed(true);
                if record.is_none() && self.log.done == Some(Logging::Note) {
                    let (state, previous) = (state.clone(), this.clone());
                    self.note = Some(Note { purpose: Purpose::Done, state, previous, text: true });
                }
            }
            if state.is_some() && record.is_some() {
                let text = record == Some(Logging::Note);
                self.note = Some(Note { purpose: Purpose::State, state: state.clone(), previous: this.clone(), text });
            }
        }
        self.align_tags(self.heading);
        self.update_parent_statistics();
        if now_done {
            let before = self.line(self.line_start(self.heading)).to_string();
            self.auto_repeat(&state, &this)?;
            if self.repeated.is_some() {
                self.before_repeat = Some(before);
            }
        }
        Ok(())
    }
}

fn has_clock_time(stamp: &str) -> bool {
    let bytes = stamp.as_bytes();
    (1..bytes.len().saturating_sub(2)).any(|at| {
        bytes[at] == b':'
            && bytes[at - 1].is_ascii_digit()
            && bytes[at + 1].is_ascii_digit()
            && bytes[at + 2].is_ascii_digit()
    })
}

/// The next statistics cookie in `text` at or after `from`: its range and
/// whether it is a percentage.
fn next_cookie(text: &str, from: usize) -> Option<(usize, usize, bool)> {
    let bytes = text.as_bytes();
    let mut at = from;
    while let Some(open) = text[at..].find('[').map(|index| at + index) {
        at = open + 1;
        let digits = bytes[open + 1..].iter().take_while(|b| b.is_ascii_digit()).count();
        let after = open + 1 + digits;
        match bytes.get(after) {
            Some(b'%') if bytes.get(after + 1) == Some(&b']') => return Some((open, after + 2, true)),
            Some(b'/') => {
                let total = bytes[after + 1..].iter().take_while(|b| b.is_ascii_digit()).count();
                if bytes.get(after + 1 + total) == Some(&b']') {
                    return Some((open, after + 2 + total, false));
                }
            }
            _ => {}
        }
    }
    None
}

/// `\<CLOSED: *\[…\]` in `text` at or after `from`.
fn find_closed(text: &str, from: usize) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut at = from;
    while let Some(start) = text[at..].find("CLOSED:").map(|index| at + index) {
        at = start + 1;
        let spaces = bytes[start + 7..].iter().take_while(|b| **b == b' ').count();
        let open = start + 7 + spaces;
        if bytes.get(open) != Some(&b'[') || !word_start(text, start) {
            continue;
        }
        if let Some(close) = text[open + 1..].find(']').filter(|close| *close > 0) {
            return Some((start, open + 1 + close + 1));
        }
    }
    None
}

/// The start of the next planning keyword that has a timestamp
/// (`org-keyword-time-not-clock-regexp`), at or after `from`.
fn next_planning_keyword(text: &str, from: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    (from..text.len()).filter(|at| text.is_char_boundary(*at)).find(|at| {
        KEYWORDS.iter().any(|keyword| {
            let Some(rest) = text[*at..].strip_prefix(keyword) else { return false };
            let spaces = rest.bytes().take_while(|b| *b == b' ').count();
            let open = at + keyword.len() + spaces;
            matches!(bytes.get(open), Some(b'[' | b'<'))
                && text[open + 1..].find([']', '>']).is_some_and(|close| close > 0)
                && word_start(text, *at)
        })
    })
}

/// Change the TODO state of the entry that contains byte `offset`.
///
/// Returns the new text of the document. `now` is the local time, which
/// goes into the timestamps that are written.
pub fn change_state(
    text: &str,
    keywords: &TodoKeywords,
    offset: usize,
    target: &Target,
    settings: &Settings,
    now: DateTime,
) -> Result<String, String> {
    // A note, if one is wanted, is written without text.
    change_state_with_note(text, keywords, offset, target, settings, now, Some("")).map(|changed| changed.text)
}

/// The state that `answer` selects at `org-todo`'s fast selection
/// (`org-fast-todo-selection`) for the entry at byte `offset`: its first
/// character is the key. A space clears the keyword; a key that selects
/// nothing quits, which is `Ok(None)`. Without keys, `org-todo` cycles
/// instead.
pub fn fast_selection(
    text: &str,
    keywords: &TodoKeywords,
    offset: usize,
    answer: &str,
) -> Result<Option<Target>, String> {
    if keywords.fast_keys().is_empty() {
        return Ok(Some(Target::Cycle { head: None, repeated: false }));
    }
    let settings = Settings::default();
    let mut editor = Editor::new(text, keywords, &settings, DateTime::new(crate::date::Date::new(1970, 1, 1), 0, 0));
    editor.go_to_heading(offset)?;
    let (current, ..) = editor.keyword_match();
    Ok(match answer.chars().next() {
        Some(' ') => Some(Target::State(None)),
        Some(key) => keywords.fast_key(key, current.as_deref()).map(|name| Target::State(Some(name.to_string()))),
        None => None,
    })
}

/// `org-add-note` (`C-c C-z`): a note with the text `note` (as written in
/// the note buffer) for the entry at byte `offset`, at
/// `org-log-beginning`, headed `Note taken on` and the time.
pub fn add_note(
    text: &str,
    keywords: &TodoKeywords,
    offset: usize,
    settings: &Settings,
    now: DateTime,
    note: &str,
) -> Result<String, String> {
    let mut editor = Editor::new(text, keywords, settings, now);
    editor.go_to_heading(offset)?;
    editor.store_note(&Note { purpose: Purpose::Note, state: None, previous: None, text: true }, note);
    Ok(editor.text)
}

/// What `change` logs in `text`: `org-log-reschedule` or
/// `org-log-redeadline`, with the file's `#+STARTUP:` words
/// (`logreschedule`, `lognoteredeadline`…) after them.
pub fn planning_logging(settings: &Settings, text: &str, change: PlanningChange) -> Option<Logging> {
    let log = Log::new(settings, text);
    if change.is_deadline() {
        log.redeadline
    } else {
        log.reschedule
    }
}

/// `org-entry-blocked-p` for the entry at byte `offset`: whether it is a
/// task its dependencies keep from being done (`org-enforce-todo-dependencies`
/// and its checkbox sibling), and if so whether by checkboxes.
pub fn entry_blocked(text: &str, keywords: &TodoKeywords, offset: usize, settings: &Settings) -> Option<bool> {
    if !settings.enforce_todo_dependencies && !settings.enforce_todo_checkbox_dependencies {
        return None;
    }
    let mut editor = Editor::new(text, keywords, settings, DateTime { day: 0, minute: 0 });
    editor.go_to_heading(offset).ok()?;
    let (this, _, _) = editor.keyword_match();
    if this.as_deref().is_none_or(|this| keywords.is_done(this)) {
        return None;
    }
    let parse = org_syntax::parse(text, keywords);
    let outline = Outline::new(&parse, text, "");
    let entry = outline.entry_at(editor.heading)?;
    if outline.entries[entry].properties.iter().any(|(key, _)| key == "NOBLOCKING") {
        return None;
    }
    editor.blocked_by().map(|reason| reason == "contained checkboxes")
}

/// What refiling to `text` logs: `org-log-refile`, with the file's
/// `#+STARTUP:` words (`logrefile`, `lognoterefile`, `nologrefile`).
pub fn refile_logging(settings: &Settings, text: &str) -> Option<Logging> {
    Log::new(settings, text).refile
}

/// `org-store-log-note` for the entry at byte `offset` just refiled:
/// `Refiled on [now]`, with the text `note` (empty for a time only).
pub fn log_refile(
    text: &str,
    keywords: &TodoKeywords,
    offset: usize,
    settings: &Settings,
    now: DateTime,
    note: &str,
) -> Result<String, String> {
    let mut editor = Editor::new(text, keywords, settings, now);
    editor.go_to_heading(offset)?;
    editor.store_note(&Note { purpose: Purpose::Refile, state: None, previous: None, text: true }, note);
    Ok(editor.text)
}

/// `org-store-log-note` for `change` of the entry at byte `offset`: the
/// note headed `Rescheduled from "[old]" on [now]` and the rest, with the
/// text `note` (empty for a time only), at `org-log-beginning`.
#[allow(clippy::too_many_arguments)]
pub fn log_planning_change(
    text: &str,
    keywords: &TodoKeywords,
    offset: usize,
    settings: &Settings,
    now: DateTime,
    change: PlanningChange,
    old: &str,
    note: &str,
) -> Result<String, String> {
    let mut editor = Editor::new(text, keywords, settings, now);
    editor.go_to_heading(offset)?;
    // The new date, for `%s` (`org-last-inserted-timestamp`).
    let state = match change {
        PlanningChange::Reschedule | PlanningChange::Redeadline => editor.planning_line().and_then(|line| {
            let keyword = if change == PlanningChange::Redeadline { "DEADLINE:" } else { "SCHEDULED:" };
            let text = editor.line(line);
            let after = text[text.find(keyword)? + keyword.len()..].trim_start_matches(' ');
            after.starts_with('<').then(|| after[..after.find('>').map_or(after.len(), |end| end + 1)].to_string())
        }),
        _ => None,
    };
    let wanted = Note { purpose: Purpose::Planning(change), state, previous: Some(old.to_string()), text: true };
    editor.store_note(&wanted, note);
    Ok(editor.text)
}

/// A state change that may want a note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changed {
    pub text: String,
    /// What Org says: that the entry repeats, and where to.
    pub message: Option<String>,
    /// The note the change wants, when no text was given for it. The text
    /// is then without the note.
    pub wants_note: Option<WantedNote>,
    /// The headline line before a repeater set the state back.
    pub before_repeat: Option<String>,
}

/// A note a state change wants (`org-add-note`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WantedNote {
    /// What it is for, as the note buffer says: `state change from
    /// "TODO" to "DONE"`, or `closed todo item`.
    pub purpose: String,
    /// The state the entry was changed to, which the note is about.
    pub state: Option<String>,
}

/// What becomes of the note a state change wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteText<'a> {
    /// None given yet: the change says that it wants one.
    Ask,
    /// This text, as the user wrote it.
    Write(&'a str),
    /// The note was cancelled (`C-c C-k` in Emacs's note buffer): the
    /// state changes, and nothing is logged.
    Skip,
    /// No note is asked for, a time is logged instead (`org-inhibit-logging`
    /// `note`, as the agenda's bulk action has it): the line of a state
    /// change without text; for a closed item, its `CLOSED` stamp alone.
    TimeOnly,
}

/// [`change_state`], with the text of the note the change records, as
/// the user writes it after the change (`org-add-note`). Without `note`,
/// a change that wants one says so instead of writing it.
pub fn change_state_with_note(
    text: &str,
    keywords: &TodoKeywords,
    offset: usize,
    target: &Target,
    settings: &Settings,
    now: DateTime,
    note: Option<&str>,
) -> Result<Changed, String> {
    let note = note.map_or(NoteText::Ask, NoteText::Write);
    change_state_noted(text, keywords, offset, target, settings, now, note)
}

/// [`change_state_with_note`], where the note may also be cancelled.
pub fn change_state_noted(
    text: &str,
    keywords: &TodoKeywords,
    offset: usize,
    target: &Target,
    settings: &Settings,
    now: DateTime,
    note: NoteText,
) -> Result<Changed, String> {
    let mut editor = Editor::new(text, keywords, settings, now);
    editor.go_to_heading(offset)?;
    editor.set_state(target, true)?;
    let wants_note = match (editor.note.take(), note) {
        // `org-store-log-note` with `org-note-abort`: nothing is stored.
        (Some(wanted), NoteText::Skip) if wanted.text => None,
        (Some(wanted), NoteText::TimeOnly) => {
            if wanted.purpose != Purpose::Done {
                editor.store_note(&wanted, "");
            }
            None
        }
        // A time only (`!`): no text is asked for.
        (Some(wanted), _) if !wanted.text => {
            editor.store_note(&wanted, "");
            None
        }
        (Some(wanted), NoteText::Write(note)) => {
            editor.store_note(&wanted, note);
            None
        }
        (Some(wanted), NoteText::Ask | NoteText::Skip) => {
            let purpose = match wanted.purpose {
                Purpose::State => {
                    let name = |state: &Option<String>| state.clone().unwrap_or_default();
                    format!("state change from \"{}\" to \"{}\"", name(&wanted.previous), name(&wanted.state))
                }
                Purpose::Done => "closed todo item".to_string(),
                Purpose::Note => "this entry".to_string(),
                Purpose::Planning(change) => change.purpose().to_string(),
                Purpose::Refile => "refiling".to_string(),
            };
            Some(WantedNote { purpose, state: wanted.state })
        }
        (None, _) => None,
    };
    Ok(Changed { text: editor.text, wants_note, message: editor.repeated, before_repeat: editor.before_repeat })
}

/// The line edits that turn `old` into `new`.
///
/// Lines that are equal at the start and at the end are left alone, and
/// what lies between is compared line by line, so a change to a headline
/// and one to its parent become two small edits.
pub fn edits(old: &str, new: &str) -> Vec<LineEdit> {
    let old_lines: Vec<&str> = old.split_inclusive('\n').collect();
    let new_lines: Vec<&str> = new.split_inclusive('\n').collect();
    let prefix = old_lines.iter().zip(&new_lines).take_while(|(a, b)| a == b).count();
    let suffix =
        old_lines[prefix..].iter().rev().zip(new_lines[prefix..].iter().rev()).take_while(|(a, b)| a == b).count();
    let (a, b) = (&old_lines[prefix..old_lines.len() - suffix], &new_lines[prefix..new_lines.len() - suffix]);
    if a.is_empty() && b.is_empty() {
        return Vec::new();
    }
    // Too much to compare line by line: one edit.
    if a.len() * b.len() > 4_000_000 {
        return line_for_line(LineEdit { start: prefix, end: prefix + a.len(), text: b.concat() });
    }
    // Longest common subsequence of the lines in between, `table[i][j]`
    // at `i * width + j`.
    let width = b.len() + 1;
    let mut table = vec![0u32; (a.len() + 1) * width];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            table[i * width + j] = if a[i] == b[j] {
                table[(i + 1) * width + j + 1] + 1
            } else {
                table[(i + 1) * width + j].max(table[i * width + j + 1])
            };
        }
    }
    let mut out: Vec<LineEdit> = Vec::new();
    let (mut i, mut j) = (0, 0);
    let mut open: Option<LineEdit> = None;
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            out.extend(open.take());
            i += 1;
            j += 1;
            continue;
        }
        let edit = open.get_or_insert(LineEdit { start: prefix + i, end: prefix + i, text: String::new() });
        if j < b.len() && (i == a.len() || table[i * width + j + 1] >= table[(i + 1) * width + j]) {
            edit.text.push_str(b[j]);
            j += 1;
        } else {
            edit.end += 1;
            i += 1;
        }
    }
    out.extend(open);
    out.into_iter().flat_map(line_for_line).collect()
}

/// An edit of several lines as an edit for each line it replaces with
/// another, then one for the lines left over: what an editor keeps on a
/// line (a mark, a fold) stays on it.
fn line_for_line(edit: LineEdit) -> Vec<LineEdit> {
    let lines: Vec<&str> = edit.text.split_inclusive('\n').collect();
    let paired = (edit.end - edit.start).min(lines.len());
    if paired < 2 {
        return vec![edit];
    }
    let mut out: Vec<LineEdit> = (0..paired)
        .map(|at| LineEdit { start: edit.start + at, end: edit.start + at + 1, text: lines[at].to_string() })
        .collect();
    let start = edit.start + paired;
    if start < edit.end || paired < lines.len() {
        out.push(LineEdit { start, end: edit.end, text: lines[paired..].concat() });
    }
    out
}

/// `org-get-heading` of a headline whose text after the stars is `head`
/// and keyword `todo`: the keyword, priority, title and tags, a space
/// apart.
fn org_get_heading(head: &str, todo: Option<&str>) -> String {
    let mut rest = head.trim();
    let mut parts: Vec<&str> = Vec::new();
    if let Some(todo) = todo {
        if let Some(after) = rest.strip_prefix(todo) {
            parts.push(todo);
            rest = after.trim_start();
        }
    }
    if rest.len() >= 4 && rest.starts_with("[#") && rest.as_bytes()[3] == b']' {
        parts.push(&rest[..4]);
        rest = rest[4..].trim_start();
    }
    let tags = rest
        .rfind([' ', '\t'])
        .map(|at| &rest[at + 1..])
        .filter(|tags| tags.len() > 1 && tags.starts_with(':') && tags.ends_with(':'));
    let title = match tags {
        Some(tags) => rest[..rest.len() - tags.len()].trim_end(),
        None => rest,
    };
    if !title.is_empty() {
        parts.push(title);
    }
    if let Some(tags) = tags {
        parts.push(tags);
    }
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::date::Date;

    fn change(marked: &str, target: Target, settings: &Settings) -> Result<String, String> {
        let offset = marked.find('¦').expect("cursor marker");
        let text = marked.replacen('¦', "", 1);
        let keywords = TodoKeywords::from_document(&text).unwrap_or_default();
        let now = DateTime::new(Date::new(2026, 5, 18), 10, 30);
        change_state(&text, &keywords, offset, &target, settings, now)
    }

    fn next(marked: &str) -> String {
        change(marked, Target::Next, &Settings::default()).unwrap()
    }

    #[test]
    fn replaces_the_keyword_as_org_todo_does() {
        assert_eq!(next("* TODO a¦\n"), "* DONE a\n");
        assert_eq!(next("* a¦\n"), "* TODO a\n");
        assert_eq!(next("* DONE a¦\n"), "* a\n");
        assert_eq!(next("* TODO¦\n"), "* DONE \n");
        assert_eq!(next("*  TODO   a  :tag:¦\n"), format!("* DONE a{}:tag:\n", " ".repeat(64)));
        assert_eq!(
            change("text¦\n* a\n", Target::Next, &Settings::default()),
            Err("Before first headline".to_string())
        );
        let named = Target::State(Some("NOPE".to_string()));
        assert_eq!(
            change("* a¦\n", named, &Settings::default()),
            Err("State `NOPE' not valid in this file".to_string())
        );
    }

    #[test]
    fn splits_a_change_into_line_edits() {
        let old = "* P [0/2]\ntext\n** TODO a\n** TODO b\n";
        let new = "* P [1/2]\ntext\n** DONE a\n** TODO b\n";
        assert_eq!(
            edits(old, new),
            [
                LineEdit { start: 0, end: 1, text: "* P [1/2]\n".to_string() },
                LineEdit { start: 2, end: 3, text: "** DONE a\n".to_string() },
            ]
        );
        assert_eq!(edits("a\nb\n", "a\nb\n"), []);
        assert_eq!(edits("a\n", "a\nb\nc"), [LineEdit { start: 1, end: 1, text: "b\nc".to_string() }]);
        assert_eq!(edits("a\nb\nc", "a\nc"), [LineEdit { start: 1, end: 2, text: String::new() }]);
        assert_eq!(edits("* TODO a", "* DONE a"), [LineEdit { start: 0, end: 1, text: "* DONE a".to_string() }]);
    }

    #[test]
    fn keeps_what_a_timestamp_carries() {
        assert_eq!(next_repeater("a <2026-05-10 Sun> <2026-05-11 Mon 10:00 .+2d/3d -1d>", 0, 53), Some((19, 48)));
        assert_eq!(repeater_parts("<2026-05-11 Mon 10:00 .+2d/3d"), Some((Some('.'), 2, 'd')));
        assert_eq!(repeater_parts("<2026-05-11 Mon ++1w"), Some((Some('+'), 1, 'w')));
        assert_eq!(repeater_parts("<2026-05-11 Mon +3m"), Some((None, 3, 'm')));
    }

    #[test]
    fn says_where_a_repeating_entry_goes() {
        let keywords = TodoKeywords::default();
        let settings = Settings { log_done: None, log_repeat: None, ..Settings::default() };
        let now = DateTime::new(crate::date::Date::new(2026, 10, 7), 10, 0);
        let done = Target::State(Some("DONE".to_string()));
        let message =
            |text: &str| change_state_noted(text, &keywords, 0, &done, &settings, now, NoteText::Skip).unwrap().message;
        assert_eq!(
            message("* TODO Water\nSCHEDULED: <2026-10-05 Mon +1w>\n").as_deref(),
            Some("Entry repeats: SCHEDULED: <2026-10-12 Mon +1w>")
        );
        assert_eq!(
            message("* TODO Both\nDEADLINE: <2026-10-09 Fri +1m> SCHEDULED: <2026-10-05 Mon .+2d>\n").as_deref(),
            Some("Entry repeats: DEADLINE: <2026-11-09 Mon +1m> SCHEDULED: <2026-10-09 Fri .+2d>")
        );
        assert_eq!(
            message("* TODO Plain\n<2026-10-05 Mon 10:00 ++1w>\nmore <2026-10-06 Tue +1d>\n").as_deref(),
            Some("Entry repeats: Plain: <2026-10-12 Mon 10:00 ++1w> Plain: <2026-10-07 Wed +1d>")
        );
        assert_eq!(message("* TODO Once\nSCHEDULED: <2026-10-05 Mon>\n"), None);
    }

    #[test]
    fn leaves_a_zero_repeater_where_it_is() {
        let keywords = TodoKeywords::default();
        let settings = Settings { log_done: None, log_repeat: None, ..Settings::default() };
        let now = DateTime::new(crate::date::Date::new(2026, 10, 7), 10, 0);
        let done = Target::State(Some("DONE".to_string()));
        let text = "* TODO a\nSCHEDULED: <2026-01-01 Thu +1w>\nbody <2020-01-01 Wed ++0d>\n";
        let changed = change_state_noted(text, &keywords, 0, &done, &settings, now, NoteText::Skip).unwrap();
        assert!(changed.text.contains("body <2020-01-01 Wed ++0d>"), "{}", changed.text);
        assert!(changed.text.contains("SCHEDULED: <2026-01-08 Thu +1w>"), "{}", changed.text);
    }
}
