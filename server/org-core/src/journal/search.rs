//! `org-journal-search`, `org-journal-schedule-view` and
//! `org-journal-reschedule-scheduled-entry`: the journal searched for a
//! string between two dates, the entries to come in a buffer of their own,
//! and an entry to come moved to another day.

use regex::Regex;

use super::buffer::{search_regex, Buffer};
use super::{entry_date, files_between, format_time, CalendarDate, Emacs, Stop};
use crate::date::{Date, DateTime};
use crate::read_date::ReadDate;

/// The days `org-journal--read-period` gives a search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    /// The start and the end read at the prompts.
    Between(Date, Date),
    /// `forever`: from 1971 to 2030.
    Forever,
    /// `future`: from today to 2030.
    Future,
}

/// A buffer of its own that a command shows: the search results, or the
/// schedule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    pub text: String,
    pub links: Vec<Link>,
}

/// A button of the search results: where it is in the view's text, and
/// the place it leads to (`org-journal-link`): the file of the entry's
/// date, and the match's offset in the file it was found in (`source`),
/// which is that file but where a file is named for another day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub at: usize,
    pub file: String,
    pub source: String,
    pub offset: usize,
}

/// `org-journal--search-do-search`: a match, the date of its entry, and
/// its line.
struct Found {
    time: Option<DateTime>,
    file: String,
    offset: usize,
    line: String,
}

/// `org-journal--created-re`
fn created_regex() -> Regex {
    search_regex(r"^ *:CREATED: +.*$")
}

/// `org-ts-regexp`
fn timestamp_regex() -> Regex {
    search_regex(r"<([0-9]{4}-[0-9]{2}-[0-9]{2}(?: .*?)?)>")
}

/// An Emacs regexp made of text: the characters special to Rust's regex
/// alone stay literal.
fn emacs_literal_regex(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if matches!(c, '(' | ')' | '{' | '}' | '|') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

impl Emacs<'_> {
    /// `org-journal-search` over `period`.
    pub(super) fn search(&mut self, string: &str, period: Period) -> Result<View, Stop> {
        let today = self.now().date();
        let (from, to) = match period {
            Period::Between(from, to) => (from, to),
            Period::Forever => (Date::new(1971, 1, 1), Date::new(2030, 12, 31)),
            Period::Future => (today, Date::new(2030, 12, 31)),
        };
        // The search takes in its first and last day: it starts a second
        // before the first, which is the day before as the results name it,
        // and ends a second after the last.
        let start = DateTime { day: DateTime::new(from, 0, 0).day - 1, minute: 0 };
        let end = DateTime { day: DateTime::new(to, 0, 0).day + 1, minute: 0 };
        if to < from {
            return Err(Stop::Error("Period end cannot be before the start".to_string()));
        }
        let wanted = if string.is_empty() { self.settings.time_prefix.clone() } else { string.to_string() };
        let files = files_between(&self.list_files(), start, end, self.settings)?;
        let mut results: Vec<Found> = Vec::new();
        for file in files.iter().rev() {
            let mut found = self.search_file(file, &wanted)?;
            // org-journal pushes the matches of a file onto its results
            // one by one, then reverses all of them.
            if self.settings.search_results_descending {
                results.splice(0..0, found);
            } else {
                found.reverse();
                results.append(&mut found);
            }
        }
        let label = |time: DateTime| format_time(&self.settings.search_result_date_format, time);
        let mut text = format!(
            "Search results for \"{string}\" between {} and {}: \n\n",
            label(DateTime { day: start.day, minute: 23 * 60 + 59 }),
            label(DateTime { day: end.day - 1, minute: 0 })
        );
        let mut links = Vec::new();
        let mut first = None;
        for found in results {
            // Entries of other days in a weekly, monthly or yearly file
            // are left out.
            let within = found.time.is_some_and(|time| start < time && time < end);
            if !(self.settings.daily() || within) {
                continue;
            }
            let time = found.time.expect("a time in the period");
            first.get_or_insert(time);
            let file = self.files.entry_path(time);
            links.push(Link { at: text.len(), file, source: found.file, offset: found.offset });
            text.push_str(&label(time));
            text.push('\t');
            text.push_str(&found.line);
            text.push('\n');
        }
        let Some(first) = first else {
            return Err(Stop::Error("No more buttons".to_string()));
        };
        // The first result shows in another window.
        self.read_entry(first)?;
        Ok(View { text, links })
    }

    /// The matches of `string` in `file`, in the order they are found
    /// (`org-journal--with-journal`, case folded).
    fn search_file(&mut self, file: &str, string: &str) -> Result<Vec<Found>, Stop> {
        let mut buffer = self.read(file);
        let date = if self.settings.daily() { Some(self.files.date(file)?) } else { None };
        buffer.point = buffer.begv;
        let mut found = Vec::new();
        while let Some(matched) = buffer.search_forward(string, None) {
            let time = match date {
                Some(date) => Some(date.time()),
                None => self.created_before(&buffer)?.map(CalendarDate::time),
            };
            let (start, end) = (buffer.line_start(buffer.point), buffer.line_end(buffer.point));
            found.push(Found {
                time,
                file: file.to_string(),
                offset: matched.start(),
                line: buffer.text[start..end].to_string(),
            });
        }
        Ok(found)
    }

    /// The date in the `CREATED` property above the point, when it is that
    /// of an entry of the first level.
    fn created_before(&self, buffer: &Buffer) -> Result<Option<CalendarDate>, String> {
        let mut probe = buffer.clone();
        if probe.re_search_backward(&created_regex(), false).is_none() {
            return Ok(None);
        }
        let at = probe.point;
        probe.org_back_to_heading(false)?;
        if probe.outline_level() != 1 {
            return Ok(None);
        }
        entry_date(self.settings, &probe, at).map(Some)
    }

    /// `org-journal-read-or-display-entry`: the entry of `time` in its
    /// file, which becomes the current buffer; false when there is none.
    pub(super) fn read_entry(&mut self, time: DateTime) -> Result<bool, Stop> {
        let file = self.files.entry_path(time);
        if !self.exists(&file) {
            self.messages.push("No journal entry for this date.".to_string());
            return Ok(false);
        }
        let index = self.visit(&file);
        let point = if self.settings.daily() {
            None
        } else {
            let saved = self.buffers[index].point;
            self.buffers[index].point = self.buffers[index].begv;
            let found = self.search_created(CalendarDate::of(time.day), true)?;
            let point = self.buffers[index].point;
            self.buffers[index].point = saved;
            if !found {
                self.messages.push("No journal entry for this date.".to_string());
                return Ok(false);
            }
            Some(point)
        };
        match point {
            Some(point) => self.buffer().point = point,
            None if self.settings.date_prefix_is_heading() => {
                let header = format!("{}{}", self.settings.date_prefix, format_time(&self.settings.date_format, time));
                // The header as a regexp, its stars literal.
                let stars = header.len() - header.trim_start_matches('*').len();
                let pattern = format!("{}{}", regex::escape(&header[..stars]), emacs_literal_regex(&header[stars..]));
                let buffer = self.buffer();
                buffer.point = buffer.begv;
                if buffer.re_search_forward(&search_regex(&pattern), None, false).is_none() {
                    return Err(Stop::Error(format!("Search failed: \"{header}\"")));
                }
            }
            None => {}
        }
        self.finalize_view()?;
        Ok(true)
    }

    /// `org-journal-schedule-view`
    pub(super) fn schedule_view(&mut self) -> Result<View, Stop> {
        let now = self.now();
        let mut text = "#+TITLE: Org-Journal Schedule\n\n".to_string();
        for date in self.list_dates()? {
            let time = date.time();
            if !(now < time) {
                continue;
            }
            // Without the entry, the buffer is the one before, or none.
            if !self.read_entry(time)? && self.current.is_none() {
                return Err(Stop::Error("Before first heading".to_string()));
            }
            text.push_str(&self.settings.date_prefix);
            text.push_str(&format_time(&self.settings.date_format, time));
            text.push('\n');
            let restriction = (self.buffer().begv, self.buffer().zv);
            self.buffer().org_narrow_to_subtree().map_err(|_| {
                let buffer = self.buffers[self.current.expect("a buffer")].clone();
                Stop::Error(format!(
                    "Before first headline at position {} in buffer {}",
                    buffer.point + 1,
                    buffer.name()
                ))
            })?;
            let items = self.items_matching("+TIMESTAMP>=\"<now>\"|+SCHEDULED>=\"<now>\"")?;
            let buffer = self.buffer();
            (buffer.begv, buffer.zv) = restriction;
            if !items.is_empty() {
                // Of each item, the text of the first headline of its
                // path, as org-journal takes it (`cddar`).
                for item in &items {
                    text.push_str(&item[0].text);
                }
                text.push('\n');
            }
        }
        Ok(View { text, links: Vec::new() })
    }

    /// `org-journal-reschedule-scheduled-entry`
    pub(super) fn reschedule(&mut self, date: &ReadDate) -> Result<(), Stop> {
        let given = date.time.is_some();
        let (hour, minute) = date.time.unwrap_or((0, 0));
        let time = DateTime::new(date.date, hour, minute);
        if time < self.now() {
            return Err(Stop::Error("Scheduled time needs to be in the future".to_string()));
        }
        let settings = self.settings;
        let here = self.current.ok_or_else(|| Stop::Error("Before first headline".to_string()))?;
        let buffer = self.buffer();
        buffer.save_point();
        buffer.org_back_to_heading(false)?;
        buffer.org_narrow_to_subtree()?;
        // The time after `org-journal-time-prefix` goes, for the new one.
        let sample = format_time(&settings.time_format, DateTime::new(Date::new(1970, 1, 1), 0, 0));
        let digits: String = emacs_literal_regex(&sample)
            .chars()
            .map(|c| if c.is_ascii_digit() { "[0-9]".to_string() } else { c.to_string() })
            .collect();
        let pattern = format!("^{}(TODO )?({digits})", regex::escape(&settings.time_prefix));
        let line_end = buffer.line_end(buffer.point);
        if let Some(found) = buffer.re_search_forward(&search_regex(&pattern), Some(line_end), false) {
            let old = found.group(2).expect("the time");
            buffer.delete(old.start, old.end);
            buffer.point = old.start;
            if given {
                buffer.insert(&format_time(&settings.time_format, time));
            }
        }
        // The timestamp goes, for the new one.
        buffer.org_back_to_heading(false)?;
        let stamp = format!("{}[ \t]*{}", regex::escape(&settings.scheduled_string), timestamp_regex().as_str());
        match buffer.re_search_forward(&search_regex(&stamp), None, false) {
            Some(found) => {
                buffer.delete(found.start(), found.end());
                buffer.point = found.start();
            }
            None => {
                buffer.org_end_of_subtree(false, false);
                buffer.insert("\n");
            }
        }
        let separator = if settings.scheduled_string.is_empty() { "" } else { " " };
        buffer.insert(&format!("{}{separator}", settings.scheduled_string));
        let end = date.end.as_deref().and_then(super::end_time);
        let extra = end.map(|(hour, minute)| format!("-{hour:02}:{minute:02}")).unwrap_or_default();
        buffer.insert(&crate::stamp::format_timestamp(time, given, false, &extra));
        // `org-cut-subtree`
        let start = buffer.org_back_to_heading(true)?;
        let end = buffer.org_end_of_subtree(true, true);
        let killed = buffer.text[start..end].to_string();
        buffer.delete(start, end);
        buffer.point = start;
        self.messages.push(format!("Cut: Subtree(s) with {} characters", killed.chars().count()));
        self.buffer().widen();
        // Into the entry of the new day, as it is, without carrying over.
        let saved = std::mem::replace(&mut self.carryover_off, true);
        let result = self.new_entry(true, Some(time), false);
        self.carryover_off = saved;
        result?;
        let buffer = self.buffer();
        if buffer.char_before().is_some_and(|c| !matches!(c, '\t' | ' ' | '\n')) {
            buffer.insert("\n");
        }
        // `org-yank`: a subtree yanked at the start of a line leaves the
        // point at the start of the line after it.
        let at_line_start = buffer.bolp();
        buffer.insert(&killed);
        if at_line_start {
            while buffer.point < buffer.zv
                && matches!(buffer.text.as_bytes()[buffer.point], b' ' | b'\t' | b'\n' | b'\r')
            {
                buffer.point += 1;
            }
            buffer.beginning_of_line();
        }
        self.messages.push("Mark set".to_string());
        let there = self.current;
        self.current = Some(here);
        // `org-save-outline-visibility`: the buffer shows as before, with
        // nothing folded; `save-excursion` puts the point back.
        let buffer = self.buffer();
        buffer.show_all();
        buffer.restore_point();
        self.shown = there.map(|index| {
            let buffer = &self.buffers[index];
            let point = if index == here { self.buffers[here].point } else { buffer.point };
            (buffer.file.clone(), point)
        });
        Ok(())
    }
}
