//! org-journal (2.2.0): a directory of journal files, one a day, week,
//! month or year (`org-journal-file-type`), named by a date
//! (`org-journal-file-format`); an entry starts with a headline for its
//! date and has headlines for the times of the day under it. The commands
//! that make entries and move between them run here as org-journal runs
//! them on Emacs's buffers ([`buffer`]), following its functions one by
//! one, and are checked against it by recorded cases and differential
//! fuzzing.

use std::collections::BTreeMap;
use std::ops::Range;

use regex::{Regex, RegexBuilder};

use crate::date::{weekday, Date, DateTime};
use crate::read_date::ReadDate;

mod buffer;
mod carryover;
mod search;

pub use search::{Link, Period, View};

use buffer::{search_regex, Buffer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

/// `org-journal-carryover-delete-empty-journal`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteEmpty {
    Never,
    Always,
    Ask,
}

/// org-journal's options, with what it takes from Org.
#[derive(Debug, Clone)]
pub struct Settings {
    /// `org-journal-dir` as Emacs names it in messages: its true name,
    /// with a slash at the end.
    pub directory: String,
    pub file_type: FileType,
    /// `org-journal-start-on-weekday`: 1 is Monday, 7 Sunday.
    pub start_on_weekday: i64,
    pub file_format: String,
    pub date_format: String,
    pub date_prefix: String,
    pub time_format: String,
    pub time_format_post_midnight: String,
    pub time_prefix: String,
    /// `org-journal-hide-entries-p`
    pub hide_entries: bool,
    pub carryover_items: String,
    pub skip_carryover_drawers: Vec<String>,
    pub carryover_delete_empty_journal: DeleteEmpty,
    pub file_header: String,
    pub created_property_timestamp_format: String,
    pub scheduled_string: String,
    /// `org-journal-enable-encryption`: a new entry's headline gets the
    /// tag of `org-crypt` (`org-crypt-tag-matcher`).
    pub enable_encryption: bool,
    pub crypt_tag: String,
    /// `org-extend-today-until`
    pub extend_today_until: i64,
    pub search_result_date_format: String,
    /// `org-journal-search-results-order-by` `:desc`
    pub search_results_descending: bool,
    /// How Org writes properties, and reads keywords.
    pub properties: crate::properties::Context,
    pub tags: crate::tags::Settings,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            directory: "~/Documents/journal/".to_string(),
            file_type: FileType::Daily,
            start_on_weekday: 1,
            file_format: "%Y%m%d".to_string(),
            date_format: "%A, %x".to_string(),
            date_prefix: "* ".to_string(),
            time_format: "%R ".to_string(),
            time_format_post_midnight: String::new(),
            time_prefix: "** ".to_string(),
            hide_entries: true,
            carryover_items: "TODO=\"TODO\"".to_string(),
            skip_carryover_drawers: Vec::new(),
            carryover_delete_empty_journal: DeleteEmpty::Never,
            file_header: String::new(),
            created_property_timestamp_format: "%Y%m%d".to_string(),
            scheduled_string: String::new(),
            enable_encryption: false,
            crypt_tag: "crypt".to_string(),
            extend_today_until: 0,
            search_result_date_format: "%A, %x".to_string(),
            search_results_descending: false,
            properties: crate::properties::Context::default(),
            tags: crate::tags::Settings::default(),
        }
    }
}

impl Settings {
    /// `org-journal--daily-p`
    fn daily(&self) -> bool {
        self.file_type == FileType::Daily
    }

    /// `org-journal--is-date-prefix-org-heading-p`
    fn date_prefix_is_heading(&self) -> bool {
        self.date_prefix.starts_with("* ")
    }

    /// `org-journal--time-entry-level`: the stars of
    /// `org-journal-time-prefix`.
    fn time_entry_level(&self) -> Option<usize> {
        self.time_prefix
            .split('\n')
            .map(|line| line.bytes().take_while(|b| *b == b'*').count())
            .find(|stars| *stars > 0)
    }
}

/// A date as the calendar has it, `(MONTH DAY YEAR)`, perhaps out of
/// range as a file name gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalendarDate {
    pub month: i64,
    pub day: i64,
    pub year: i64,
}

impl CalendarDate {
    fn of(day: crate::date::Day) -> CalendarDate {
        let date = Date::from_day(day);
        CalendarDate { month: date.month, day: date.day, year: date.year }
    }

    /// `calendar-absolute-from-gregorian`, for out-of-range fields too.
    fn absolute(self) -> i64 {
        let mut day_of_year = self.day + 31 * (self.month - 1);
        if self.month > 2 {
            day_of_year -= (23 + 4 * self.month) / 10;
            if crate::date::is_leap_year(self.year) {
                day_of_year += 1;
            }
        }
        let offset = self.year - 1;
        day_of_year + 365 * offset + offset.div_euclid(4) - offset.div_euclid(100) + offset.div_euclid(400)
    }

    /// `org-journal--calendar-date->time`: its midnight, `encode-time`
    /// carrying fields over.
    pub fn time(self) -> DateTime {
        DateTime { day: Date::new(self.year, self.month, self.day).to_day(), minute: 0 }
    }

    /// `org-journal--calendar-date-compare`
    fn before(self, other: CalendarDate) -> bool {
        self.absolute() < other.absolute()
    }
}

/// `format-time-string`
pub fn format_time(format: &str, time: DateTime) -> String {
    crate::capture::format_time(format, time)
}

/// `regexp-quote`
fn regexp_quote(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if "[*.\\?+^$".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// What a group of a file name pattern holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Year,
    Month,
    Day,
    Name,
}

/// A format as org-journal matches names with it
/// (`org-journal--format->regex`): the Emacs regexp, and the same as a
/// regex of this crate, with what each of its groups holds.
struct Pattern {
    emacs: String,
    regex: String,
    parts: Vec<Part>,
}

impl Pattern {
    fn new(format: &str) -> Pattern {
        let mut emacs = regexp_quote(&format.replace("%F", "%Y-%m-%d"));
        for (directive, rx) in [
            (&["%a", "%A", "%b", "%B"][..], r"\(?4:[[:alpha:]]\{3,\}\)"),
            (&["%d"][..], r"\(?3:[0-9]\{2\}\)"),
            (&["%m"][..], r"\(?2:[0-9]\{2\}\)"),
            (&["%Y", "%G"][..], r"\(?1:[0-9]\{4\}\)"),
            (&["%V"][..], r"[0-9]\{2\}"),
        ] {
            for directive in directive {
                emacs = emacs.replace(directive, rx);
            }
        }
        // Extra parts of a timestamp are let through.
        if emacs.ends_with([']', '>']) {
            emacs.insert_str(emacs.len() - 1, "[A-z0-9: ]*");
        }
        let (regex, parts) = translate(&emacs);
        Pattern { emacs, regex, parts }
    }
}

/// The Emacs regexps [`Pattern`] makes, as regexes of this crate.
fn translate(emacs: &str) -> (String, Vec<Part>) {
    let mut out = String::new();
    let mut parts = Vec::new();
    let mut chars = emacs.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('(') => {
                    chars.next(); // ?
                    let part = match chars.next() {
                        Some('1') => Part::Year,
                        Some('2') => Part::Month,
                        Some('3') => Part::Day,
                        _ => Part::Name,
                    };
                    chars.next(); // :
                    parts.push(part);
                    out.push('(');
                }
                Some(')') => out.push(')'),
                Some('{') => out.push('{'),
                Some('}') => out.push('}'),
                Some(other) => out.push_str(&regex::escape(&other.to_string())),
                None => out.push_str(r"\\"),
            },
            '[' => {
                let mut class = String::new();
                for c in chars.by_ref() {
                    if c == ']' && !class.is_empty() && !class.ends_with("[:alpha:") {
                        break;
                    }
                    class.push(c);
                }
                if class == "[:alpha:]" {
                    out.push_str(r"\p{Alphabetic}");
                } else {
                    out.push('[');
                    out.push_str(&class.replace('[', r"\[").replace(r"\[:alpha:]", "[:alpha:]"));
                    out.push(']');
                }
            }
            _ => out.push_str(&regex::escape(&c.to_string())),
        }
    }
    (out, parts)
}

/// `string-to-number`: the leading digits, or 0.
fn string_to_number(text: &str) -> i64 {
    let text = text.trim_start();
    let (sign, rest) = match text.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, text.strip_prefix('+').unwrap_or(text)),
    };
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    sign * digits.parse::<i64>().unwrap_or(0)
}

/// Which files are journal files, and their dates.
struct Files<'s> {
    settings: &'s Settings,
    pattern: Pattern,
    /// The name of a journal file relative to the directory
    /// (`org-journal--dir-and-file-format->pattern`).
    names: Regex,
}

impl<'s> Files<'s> {
    fn new(settings: &'s Settings) -> Files<'s> {
        let pattern = Pattern::new(&settings.file_format);
        let names = RegexBuilder::new(&format!(r"\A{}(?:.(?:gpg|age))?\z", pattern.regex))
            .case_insensitive(true)
            .build()
            .unwrap_or_else(|_| Regex::new(r"\A\z[^\s\S]").expect("valid"));
        Files { settings, pattern, names }
    }

    /// `org-journal-is-journal`
    fn is_journal(&self, file: &str) -> bool {
        self.names.is_match(file)
    }

    /// `org-journal--file-name->calendar-date`
    fn date(&self, file: &str) -> Result<CalendarDate, String> {
        let emacs = format!("{}{}", self.settings.directory, self.pattern.emacs);
        let has_month = emacs.contains("2:");
        let has_day = emacs.contains("3:");
        let captures = self.names.captures(file);
        // `\N` is what the last group holding that part matched.
        let group = |part: Part| -> i64 {
            let Some(captures) = &captures else { return 0 };
            let mut value = 0;
            for (index, kind) in self.pattern.parts.iter().enumerate() {
                if *kind == part {
                    if let Some(found) = captures.get(index + 1) {
                        value = string_to_number(found.as_str());
                    }
                }
            }
            value
        };
        let path = format!("{}{file}", self.settings.directory);
        let year = match &captures {
            Some(_) => group(Part::Year),
            None => string_to_number(&path),
        };
        if year == 0 {
            return Err(format!("Failed to extract year from file: {path}"));
        }
        let kind = self.settings.file_type;
        let month = if !has_month && kind != FileType::Yearly {
            return Err(format!("Failed to extract month from file: {path}"));
        } else if has_month {
            group(Part::Month)
        } else {
            1
        };
        let day = if !has_day && matches!(kind, FileType::Daily | FileType::Weekly) {
            return Err(format!("Failed to extract day from file: {path}"));
        } else if has_day {
            group(Part::Day)
        } else {
            1
        };
        Ok(CalendarDate { month, day, year })
    }

    /// `org-journal--convert-time-to-file-type-time`
    fn file_type_time(&self, time: DateTime) -> DateTime {
        let date = time.date();
        match self.settings.file_type {
            FileType::Daily => time,
            FileType::Weekly => {
                let monday = time.day - (weekday(time.day) as i64 + 6) % 7;
                let target = monday + self.settings.start_on_weekday - 1;
                DateTime { day: if target > time.day { target - 7 } else { target }, minute: 0 }
            }
            FileType::Monthly => DateTime { day: Date::new(date.year, date.month, 1).to_day(), minute: 0 },
            FileType::Yearly => DateTime { day: Date::new(date.year, 1, 1).to_day(), minute: 0 },
        }
    }

    /// `org-journal--get-entry-path`: relative to the directory.
    fn entry_path(&self, time: DateTime) -> String {
        format_time(&self.settings.file_format, self.file_type_time(time))
    }
}

/// `org-journal-is-journal`: whether the file named `relative` to the
/// directory is a journal file.
pub fn is_journal_file(relative: &str, settings: &Settings) -> bool {
    Files::new(settings).is_journal(relative)
}

/// `org-journal--search-build-file-list`: the journal files among `names`
/// with days after `start` and before `end`, the last first.
fn files_between(names: &[String], start: DateTime, end: DateTime, settings: &Settings) -> Result<Vec<String>, String> {
    let files = Files::new(settings);
    let mut found = Vec::new();
    for name in names {
        let time = files.date(name)?.time();
        let date = time.date();
        // The last day the file can hold.
        let boundary = match settings.file_type {
            FileType::Daily => time,
            FileType::Weekly => DateTime { day: time.day + 6, minute: 0 },
            FileType::Monthly => {
                DateTime::new(Date::new(date.year, date.month, crate::date::days_in_month(date.year, date.month)), 0, 0)
            }
            FileType::Yearly => DateTime::new(Date::new(date.year, 12, 31), 0, 0),
        };
        if start < boundary && time < end {
            found.push(name.clone());
        }
    }
    found.reverse();
    Ok(found)
}

/// `org-journal--update-org-agenda-files`
/// (`org-journal-enable-agenda-integration`): the journal files among
/// `names` (relative to the directory, in `org-journal--list-files`'s
/// order) of today and the days to come, until 2030.
pub fn agenda_files(names: &[String], now: DateTime, settings: &Settings) -> Result<Vec<String>, String> {
    let files = Files::new(settings);
    let encrypted = RegexBuilder::new(r".(?:gpg|age)$").case_insensitive(true).build().expect("valid");
    let mut names: Vec<String> =
        names.iter().filter(|name| files.is_journal(name) && !encrypted.is_match(name)).cloned().collect();
    crate::roam::sync_order(&mut names);
    // From yesterday, or the day before while it is still yesterday.
    let mut start = now.day - 1;
    if now.minute / 60 < settings.extend_today_until {
        start -= 1;
    }
    let start = DateTime { day: start, minute: 0 };
    let end = DateTime::new(Date::new(2030, 12, 31), 0, 0);
    files_between(&names, start, end, settings)
}

/// `org-journal--entry-date->calendar-date` for the entry at `at`.
fn entry_date(settings: &Settings, buffer: &Buffer, at: usize) -> Result<CalendarDate, String> {
    let pattern = Pattern::new(&settings.created_property_timestamp_format);
    let value = crate::properties::entry_get(&buffer.text, at, "CREATED", &settings.properties);
    let matched = value.as_deref().and_then(|value| {
        let regex = RegexBuilder::new(&pattern.regex).case_insensitive(true).build().ok()?;
        let captures = regex.captures(value)?;
        let group = |part: Part| -> Option<i64> {
            let mut found = None;
            for (index, kind) in pattern.parts.iter().enumerate() {
                if *kind == part {
                    if let Some(text) = captures.get(index + 1) {
                        found = Some(string_to_number(text.as_str()));
                    }
                }
            }
            found
        };
        Some((group(Part::Month), group(Part::Day), group(Part::Year)))
    });
    match matched {
        Some((Some(month), Some(day), Some(year))) => Ok(CalendarDate { month, day, year }),
        Some(_) => Err("Wrong type argument: stringp, nil".to_string()),
        None => {
            let line = buffer.text[..at].matches('\n').count() + 1;
            Err(format!(
                "Created property timestamp format \"{}\" doesn't match CREATED property value ({}) from entry at line: Line {line}",
                settings.created_property_timestamp_format,
                value.as_deref().unwrap_or("nil")
            ))
        }
    }
}

/// `org-property-drawer-re`
fn property_drawer_regex() -> Regex {
    search_regex(r"^[ \t]*:PROPERTIES:[ \t]*\n(?:[ \t]*:\S+:(?:[ \t].*)?[ \t]*\n)*?[ \t]*:END:[ \t]*$")
}

/// `org-journal--file->calendar-dates`: the dates of the entries of a
/// file that is not daily, from their `CREATED` properties, the last
/// first. A buffer visiting the file is read as it is, narrowed or not
/// (`org-journal--with-journal`).
fn file_dates(settings: &Settings, buffer: &Buffer) -> Result<Vec<CalendarDate>, String> {
    let mut buffer = buffer.clone();
    buffer.point = buffer.begv;
    // `org-journal--skip-meta-data`
    if let Some(found) = buffer.looking_at(&property_drawer_regex()) {
        buffer.goto(found.end());
        buffer.forward_line(1);
    }
    let created = search_regex(r"^ *:CREATED: +.*$");
    let mut dates = Vec::new();
    while buffer.re_search_forward(&created, None, false).is_some() {
        let mut probe = buffer.clone();
        probe.org_back_to_heading(true)?;
        if probe.outline_level() == 1 {
            dates.push(entry_date(settings, &buffer, buffer.point)?);
        }
    }
    dates.reverse();
    Ok(dates)
}

/// A command of org-journal.
#[derive(Debug, Clone)]
pub enum Command {
    /// `org-journal-new-entry`; with `prefix`, today's entry is shown
    /// without a new time entry.
    NewEntry { prefix: bool },
    /// `org-journal-new-date-entry`, for the date read at its prompt.
    NewDateEntry { prefix: bool, date: ReadDate },
    /// `org-journal-new-scheduled-entry`; with `prefix` no TODO.
    NewScheduledEntry { prefix: bool, date: ReadDate },
    /// `org-journal-next-entry`
    Next,
    /// `org-journal-previous-entry`
    Previous,
    /// `org-journal-open-current-journal-file`
    OpenCurrent,
    /// `org-journal-search` and the commands that search a period of
    /// their own (`org-journal-search-forever` …); an empty string
    /// finds every entry with a time.
    Search { string: String, period: Period },
    /// `org-journal-schedule-view`
    ScheduleView,
    /// `org-journal-reschedule-scheduled-entry`, at the point, to the
    /// date read at its prompt.
    Reschedule { date: ReadDate },
}

/// What a command runs on.
#[derive(Debug, Clone)]
pub struct Request<'a> {
    /// The files of the directory, by their names relative to it.
    pub files: &'a BTreeMap<String, String>,
    pub now: DateTime,
    /// The journal file the command runs in, and where in it.
    pub from: Option<(&'a str, usize)>,
    /// The answers to its questions so far.
    pub answers: &'a [bool],
}

/// Why a command stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stop {
    /// It refuses, with Emacs's message.
    Error(String),
    /// It asks this question (`y-or-n-p`); run it again with the answer.
    Ask(String),
}

impl From<String> for Stop {
    fn from(message: String) -> Self {
        Stop::Error(message)
    }
}

/// Where a command leaves the cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Visit {
    pub file: String,
    pub offset: usize,
    /// What the headlines' folds hide in it.
    pub hidden: Vec<Range<usize>>,
    /// The lines whose start is hidden, counted from 1.
    pub hidden_lines: Vec<usize>,
}

/// What a command did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// The files it changed: their new text, or `None` for those it
    /// deleted.
    pub changed: BTreeMap<String, Option<String>>,
    pub visit: Option<Visit>,
    pub messages: Vec<String>,
    /// The buffer of its own a command shows: the search results, the
    /// schedule.
    pub view: Option<View>,
    /// The file the window shows when it is not the current buffer, and
    /// where: the new day of a rescheduled entry.
    pub shown: Option<(String, usize)>,
}

/// Emacs as org-journal leaves it: its buffers, the current one, the
/// files deleted, the messages shown.
struct Emacs<'a> {
    settings: &'a Settings,
    files: Files<'a>,
    request: &'a Request<'a>,
    buffers: Vec<Buffer>,
    current: Option<usize>,
    deleted: Vec<String>,
    messages: Vec<String>,
    answered: usize,
    /// `org-journal-carryover-items` bound to nil.
    carryover_off: bool,
    /// `org-journal--dates`: the dates of each file as last read, which
    /// are read again only when a journal buffer is saved.
    dates: Option<BTreeMap<String, Vec<CalendarDate>>>,
    view: Option<View>,
    shown: Option<(String, usize)>,
}

impl<'a> Emacs<'a> {
    fn new(settings: &'a Settings, request: &'a Request<'a>) -> Emacs<'a> {
        let mut emacs = Emacs {
            settings,
            files: Files::new(settings),
            request,
            buffers: Vec::new(),
            current: None,
            deleted: Vec::new(),
            messages: Vec::new(),
            answered: 0,
            carryover_off: false,
            dates: None,
            view: None,
            shown: None,
        };
        if let Some((file, offset)) = request.from {
            let index = emacs.visit(file);
            emacs.buffers[index].goto(offset);
        }
        emacs
    }

    fn now(&self) -> DateTime {
        self.request.now
    }

    /// The current buffer.
    fn buffer(&mut self) -> &mut Buffer {
        let index = self.current.expect("a current buffer");
        &mut self.buffers[index]
    }

    fn buffer_file(&self) -> Option<&str> {
        self.current.map(|index| self.buffers[index].file.as_str())
    }

    /// Whether the file is on disk.
    fn exists(&self, file: &str) -> bool {
        self.request.files.contains_key(file) && !self.deleted.iter().any(|gone| gone == file)
    }

    /// A file as `org-journal--with-journal` reads it: its buffer, or a
    /// new one with what is on disk.
    fn read(&self, file: &str) -> Buffer {
        match self.buffers.iter().find(|buffer| buffer.file == file && !buffer.killed) {
            Some(buffer) => buffer.clone(),
            None if self.exists(file) => Buffer::new(file, &self.request.files[file]),
            None => Buffer::new(file, ""),
        }
    }

    /// `find-file`: the file's buffer becomes the current one.
    fn visit(&mut self, file: &str) -> usize {
        let index = match self.buffers.iter().position(|buffer| buffer.file == file && !buffer.killed) {
            Some(index) => index,
            None => {
                let text = if self.exists(file) { self.request.files[file].clone() } else { String::new() };
                self.buffers.push(Buffer::new(file, &text));
                self.buffers.len() - 1
            }
        };
        self.current = Some(index);
        index
    }

    /// `y-or-n-p`
    fn ask(&mut self, question: &str) -> Result<bool, Stop> {
        match self.request.answers.get(self.answered) {
            Some(answer) => {
                self.answered += 1;
                Ok(*answer)
            }
            None => Err(Stop::Ask(question.to_string())),
        }
    }

    /// `org-journal--list-files`
    fn list_files(&self) -> Vec<String> {
        let encrypted = RegexBuilder::new(r".(?:gpg|age)$").case_insensitive(true).build().expect("valid");
        let mut names: Vec<String> = self
            .request
            .files
            .keys()
            .filter(|file| self.exists(file) && self.files.is_journal(file) && !encrypted.is_match(file))
            .cloned()
            .collect();
        crate::roam::sync_order(&mut names);
        names
    }

    /// The dates of the entries of a file (`org-journal--dates-puthash`).
    fn dates_of(&self, file: &str) -> Result<Vec<CalendarDate>, String> {
        if self.settings.daily() {
            Ok(vec![self.files.date(file)?])
        } else {
            file_dates(self.settings, &self.read(file))
        }
    }

    /// `org-journal--list-dates`: the dates of all entries, sorted. Files
    /// already read are not read again.
    fn list_dates(&mut self) -> Result<Vec<CalendarDate>, String> {
        let files = self.list_files();
        let mut cache = self.dates.take().unwrap_or_default();
        cache.retain(|file, _| files.contains(file));
        for file in &files {
            if !cache.contains_key(file) {
                match self.dates_of(file) {
                    Ok(dates) => cache.insert(file.clone(), dates),
                    Err(error) => {
                        self.dates = Some(cache);
                        return Err(error);
                    }
                };
            }
        }
        let mut dates: Vec<CalendarDate> = Vec::new();
        for date in cache.values().flatten() {
            if !dates.contains(date) {
                dates.push(*date);
            }
        }
        self.dates = Some(cache);
        dates.sort_by_key(|date| date.absolute());
        Ok(dates)
    }

    /// `save-buffer`: a changed journal buffer's dates are read again
    /// (`org-journal-after-save-hook`).
    fn save(&mut self, index: usize) -> Result<(), String> {
        if !self.buffers[index].modified {
            return Ok(());
        }
        // `require-final-newline`, which Org's buffers have from Text
        // mode: the newline goes at the very end, the point and markers
        // stay before it, the end of a narrowing follows.
        let buffer = &mut self.buffers[index];
        if !buffer.text.is_empty() && !buffer.text.ends_with('\n') {
            if buffer.zv == buffer.text.len() {
                buffer.zv += 1;
            }
            buffer.text.push('\n');
            buffer.hidden.push(false);
        }
        buffer.modified = false;
        let file = buffer.file.clone();
        if self.files.is_journal(&file) {
            let dates = self.dates_of(&file)?;
            self.dates.get_or_insert_with(BTreeMap::new).insert(file, dates);
        }
        Ok(())
    }

    /// The date of the entry of the current buffer: from the file's name,
    /// or the `CREATED` of its entry at the point.
    fn current_date(&mut self) -> Result<CalendarDate, String> {
        if self.settings.daily() {
            let file = self.buffer_file().unwrap_or_default().to_string();
            self.files.date(&file)
        } else {
            while self.buffer().org_up_heading_safe().is_some() {}
            let buffer = self.buffer().clone();
            entry_date(self.settings, &buffer, buffer.point)
        }
    }

    /// `org-journal--search-forward-created`
    fn search_created(&mut self, date: CalendarDate, noerror: bool) -> Result<bool, String> {
        let stamp = format_time(&self.settings.created_property_timestamp_format, date.time());
        let regex = search_regex(&format!(r"[ \t]*:CREATED:[ \t]+{}[ \t]*$", regex::escape(&stamp)));
        if self.buffer().re_search_forward(&regex, None, false).is_some() {
            return Ok(true);
        }
        if noerror {
            return Ok(false);
        }
        let quoted = regexp_quote(&stamp).replace('\\', "\\\\").replace('"', "\\\"");
        Err(format!("Search failed: \"[ \t]*:CREATED:[ \t]+{quoted}[ \t]*$\""))
    }

    /// `org-journal--finalize-view`
    fn finalize_view(&mut self) -> Result<(), String> {
        let settings = self.settings;
        let buffer = self.buffer();
        if settings.date_prefix_is_heading() {
            buffer.org_up_heading_safe();
            buffer.org_back_to_heading(false)?;
            if settings.hide_entries {
                buffer.hide_other()?;
            }
            buffer.show_subtree()?;
        } else {
            buffer.show_all();
        }
        Ok(())
    }

    /// `org-journal--goto-entry`
    fn goto_entry(&mut self, date: CalendarDate) -> Result<(), String> {
        let buffer = self.buffer();
        buffer.widen();
        buffer.point = 0;
        if self.settings.daily() {
            self.buffer().org_next_visible_heading();
        } else {
            self.search_created(date, false)?;
        }
        self.finalize_view()
    }

    /// `org-journal--open-entry`: the entry after the current one, or
    /// before it with `previous`; false when there is none.
    fn open_entry(&mut self, previous: bool) -> Result<bool, String> {
        let date = self.current_date()?;
        let mut dates = self.list_dates()?;
        // `org-journal-sort-dates`
        if !dates.contains(&date) {
            let at = dates.iter().take_while(|other| other.before(date)).count();
            dates.insert(at, date);
        }
        if previous {
            dates.reverse();
        }
        let next = dates.into_iter().find(|other| {
            let passed = if previous { date.before(*other) } else { other.before(date) };
            !(passed || *other == date)
        });
        let Some(next) = next else { return Ok(false) };
        let file = self.files.entry_path(next.time());
        self.visit(&file);
        self.goto_entry(next)?;
        Ok(true)
    }

    /// `org-journal--insert-header`
    fn insert_header(&mut self, time: DateTime) {
        let header = &self.settings.file_header;
        if header.is_empty() || !self.buffer().text.is_empty() {
            return;
        }
        let mut header = format_time(header, time);
        if header.split('\n').filter(|line| !line.is_empty()).count() > 1 && !header.ends_with('\n') {
            header.push('\n');
        }
        self.buffer().insert(&header);
        // `org-ctrl-c-ctrl-c` on a `#+` line restarts Org.
        let mut probe = self.buffer().clone();
        if probe.re_search_backward(&search_regex(r"^#\+"), false).is_some() {
            let line = &probe.text[probe.point..probe.line_end(probe.point)];
            if search_regex(r"^#\+\S+:").is_match(line) {
                self.messages.push("org-journal-mode restarted".to_string());
                self.messages.push("Local setup has been refreshed".to_string());
            }
        }
    }

    /// `org-journal--insert-entry-header`
    fn insert_entry_header(&mut self, time: DateTime) -> Result<(), String> {
        if self.settings.date_format.is_empty() {
            return Err("org-journal-date-format is empty, this won't work".to_string());
        }
        let header = format!("{}{}", self.settings.date_prefix, format_time(&self.settings.date_format, time));
        let file = self.buffer().file.clone();
        self.buffer().point = self.buffer().begv;
        let exists = if self.settings.daily() {
            if self.buffer().search_forward(&header, None).is_some() {
                true
            } else {
                let buffer = self.buffer();
                buffer.point = buffer.zv;
                false
            }
        } else {
            let date = CalendarDate::of(time.day);
            let mut dates = file_dates(self.settings, &self.read(&file))?;
            dates.sort_by_key(|date| std::cmp::Reverse(date.absolute()));
            // org-journal's `cl-loop` tries its `unless` clause on every
            // round: once no date is left, the entry goes first.
            let mut dates = dates.into_iter();
            loop {
                let Some(entry) = dates.next() else {
                    let buffer = self.buffer();
                    if buffer.re_search_forward(&search_regex(r"^(\*+ )"), None, false).is_none() {
                        buffer.point = buffer.zv;
                    }
                    if buffer.at_heading(false) {
                        buffer.beginning_of_line();
                        buffer.insert("\n");
                        buffer.forward_line(-1);
                    } else {
                        buffer.forward_line(-1);
                        buffer.end_of_line();
                    }
                    break false;
                };
                if entry.before(date) || entry == date {
                    self.search_created(entry, false)?;
                    if entry.before(date) {
                        self.buffer().org_end_of_subtree(false, false);
                    }
                    break entry == date;
                }
            }
        };
        if exists {
            return Ok(());
        }
        let buffer = self.buffer();
        if buffer.after_non_blank() {
            buffer.insert("\n");
        }
        buffer.insert(&header);
        if !self.settings.daily() {
            let settings = self.settings;
            let stamp = format_time(&settings.created_property_timestamp_format, time);
            let buffer = self.buffer();
            let text = crate::properties::put(&buffer.text, buffer.point, "CREATED", &stamp, &settings.properties)?;
            self.buffer().replace_text(text);
        }
        if self.settings.enable_encryption {
            // `org-get-tags`: the headline's tags with those it inherits,
            // or the file's before the first headline; `org-set-tags` puts
            // `crypt` alone on the headline.
            let buffer = self.buffer().clone();
            let outline = crate::outline::Outline::new(
                &org_syntax::parse(&buffer.text, &self.settings.properties.keywords),
                &buffer.text,
                "",
            );
            let heading = buffer.containing_heading(buffer.point);
            let tags: Vec<String> = match heading {
                Some(heading) => outline
                    .entries
                    .iter()
                    .find(|entry| entry.offset == heading)
                    .map(|entry| entry.tags.iter().map(|tag| tag.name.clone()).collect())
                    .unwrap_or_default(),
                None => outline.file_tags.clone(),
            };
            if !tags.contains(&self.settings.crypt_tag) {
                let heading = buffer.clone().org_back_to_heading(true)?;
                let text = crate::tags::set_tags(
                    &buffer.text,
                    heading,
                    std::slice::from_ref(&self.settings.crypt_tag),
                    &self.settings.tags,
                );
                self.buffer().replace_text(text);
            }
        }
        Ok(())
    }

    /// `org-journal--insert-entry`
    fn insert_entry(&mut self, time: DateTime, extended: bool, no_timestamp: bool) {
        let settings = self.settings;
        let now = self.now();
        let discrepancy = now.day - time.day;
        let timestamp = if discrepancy == 0 {
            format_time(&settings.time_format, now)
        } else if discrepancy == 1 && extended {
            if settings.time_format_post_midnight.is_empty() {
                format_time(&settings.time_format, now)
            } else {
                format_time(&settings.time_format_post_midnight, now)
            }
        } else {
            String::new()
        };
        let buffer = self.buffer();
        if !buffer.bolp() {
            buffer.insert("\n");
        }
        let timestamp = if no_timestamp { "" } else { &timestamp };
        buffer.insert(&format!("{}{timestamp}", settings.time_prefix));
    }

    /// `org-journal-new-entry`
    fn new_entry(&mut self, prefix: bool, time: Option<DateTime>, no_timestamp: bool) -> Result<(), Stop> {
        let now = self.now();
        let extended = time.is_none() && now.minute / 60 < self.settings.extend_today_until;
        let time = match time {
            Some(time) => time,
            None if extended => DateTime { day: now.day - 1, minute: now.minute },
            None => now,
        };
        let path = self.files.entry_path(time);
        if self.buffer_file() != Some(path.as_str()) {
            self.visit(&path);
        }
        self.insert_header(time);
        self.insert_entry_header(time)?;
        if !self.carryover_off && !self.settings.carryover_items.trim().is_empty() && path == self.files.entry_path(now)
        {
            self.carryover()?;
        }
        if self.settings.date_prefix_is_heading() {
            self.buffer().outline_end_of_subtree()?;
        } else {
            let buffer = self.buffer();
            buffer.point = buffer.zv;
        }
        if !prefix {
            self.insert_entry(time, extended, no_timestamp);
        }
        match self.settings.time_entry_level().filter(|_| self.settings.hide_entries) {
            Some(level) => self.buffer().hide_sublevels(level),
            None => {
                let saved = self.buffer().point;
                self.finalize_view()?;
                self.buffer().point = saved;
            }
        }
        if !prefix {
            self.buffer().show_entry()?;
        }
        Ok(())
    }

    /// `org-journal-new-scheduled-entry`
    fn new_scheduled_entry(&mut self, prefix: bool, time: DateTime, given: Option<&ReadDate>) -> Result<(), Stop> {
        if time < self.now() {
            return Err(Stop::Error("Scheduled time needs to be in the future".to_string()));
        }
        let saved = std::mem::replace(&mut self.carryover_off, true);
        let result = self.new_entry(false, Some(time), true);
        self.carryover_off = saved;
        result?;
        let settings = self.settings;
        let buffer = self.buffer();
        if !prefix {
            buffer.insert("TODO ");
        }
        let with_time = given.and_then(|read| read.time).is_some();
        if with_time {
            buffer.insert(&format_time(&settings.time_format, time));
        }
        let at = buffer.point;
        let separator = if settings.scheduled_string.is_empty() { "" } else { " " };
        buffer.insert(&format!("\n{}{separator}", settings.scheduled_string));
        let end = given.and_then(|read| read.end.as_deref()).and_then(end_time);
        let extra = end.map(|(hour, minute)| format!("-{hour:02}:{minute:02}")).unwrap_or_default();
        buffer.insert(&crate::stamp::format_timestamp(time, with_time, false, &extra));
        buffer.point = at;
        Ok(())
    }
}

/// The end of a time range as `org-insert-timestamp` reads it.
fn end_time(text: &str) -> Option<(i64, i64)> {
    let captures = Regex::new(r"([0-9]+):([0-9]+)").expect("valid").captures(text)?;
    Some((captures[1].parse().ok()?, captures[2].parse().ok()?))
}

/// The time `org-time-string-to-time` makes of what `org-read-date`
/// returns.
fn read_time(read: &ReadDate) -> DateTime {
    let (hour, minute) = read.time.unwrap_or((0, 0));
    DateTime::new(read.date, hour, minute)
}

/// Run `command` on the journal.
pub fn run(command: &Command, request: &Request, settings: &Settings) -> Result<Outcome, Stop> {
    let mut emacs = Emacs::new(settings, request);
    match command {
        Command::NewEntry { prefix } => emacs.new_entry(*prefix, None, false)?,
        Command::NewDateEntry { prefix, date } => {
            // `org-read-date` returns the time only where
            // `org-time-was-given` is bound, which it is not here.
            let time = DateTime::new(date.date, 0, 0);
            if time < emacs.now() {
                emacs.new_entry(*prefix, Some(time), false)?;
            } else {
                emacs.new_scheduled_entry(*prefix, time, None)?;
            }
        }
        Command::NewScheduledEntry { prefix, date } => {
            emacs.new_scheduled_entry(*prefix, read_time(date), Some(date))?;
        }
        Command::Next | Command::Previous => {
            let previous = matches!(command, Command::Previous);
            let journal = emacs.buffer_file().is_some_and(|file| emacs.files.is_journal(file));
            if !journal {
                let which = if previous { "previous" } else { "next" };
                return Err(Stop::Error(format!("org-journal-{which}-entry called outside calendar/org-journal mode")));
            }
            if !emacs.open_entry(previous)? {
                let which = if previous { "before" } else { "after" };
                emacs.messages.push(format!("No journal entry {which} this one"));
            }
        }
        Command::OpenCurrent => {
            let now = emacs.now();
            let time = DateTime::new(now.date(), 0, now.minute - 60 * settings.extend_today_until);
            let file = emacs.files.entry_path(time);
            if emacs.exists(&file) {
                emacs.visit(&file);
                if !settings.daily() {
                    if let Some(last) = file_dates(settings, &emacs.read(&file))?.first().copied() {
                        emacs.goto_entry(last)?;
                    }
                }
            } else {
                emacs.messages.push(format!("Journal file {}{file} not found", settings.directory));
            }
        }
        Command::Search { string, period } => {
            let view = emacs.search(string, *period)?;
            emacs.view = Some(view);
            emacs.current = None;
        }
        Command::ScheduleView => {
            let view = emacs.schedule_view()?;
            emacs.view = Some(view);
            emacs.current = None;
        }
        Command::Reschedule { date } => emacs.reschedule(date)?,
    }
    Ok(emacs.outcome())
}

impl Emacs<'_> {
    fn outcome(self) -> Outcome {
        let mut changed = BTreeMap::new();
        for buffer in self.buffers.iter().filter(|buffer| !buffer.killed) {
            let before = self.request.files.get(&buffer.file);
            if self.deleted.contains(&buffer.file) {
                continue;
            }
            if before != Some(&buffer.text) && !(before.is_none() && buffer.text.is_empty()) {
                changed.insert(buffer.file.clone(), Some(buffer.text.clone()));
            }
        }
        for file in &self.deleted {
            changed.insert(file.clone(), None);
        }
        let visit = self.current.map(|index| {
            let buffer = &self.buffers[index];
            let mut hidden = Vec::new();
            let mut start = None;
            for (at, folded) in buffer.hidden.iter().enumerate() {
                match (folded, start) {
                    (true, None) => start = Some(at),
                    (false, Some(from)) => {
                        hidden.push(from..at);
                        start = None;
                    }
                    _ => {}
                }
            }
            if let Some(from) = start {
                hidden.push(from..buffer.text.len());
            }
            Visit { file: buffer.file.clone(), offset: buffer.point, hidden, hidden_lines: buffer.hidden_lines() }
        });
        Outcome { changed, visit, messages: self.messages, view: self.view, shown: self.shown }
    }
}
