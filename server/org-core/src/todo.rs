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
    /// The next keyword, or none after the last one.
    Next,
    /// The previous keyword, or none before the first one.
    Previous,
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

/// Settings, named after Org's variables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// `org-log-done`
    pub log_done: Option<Logging>,
    /// `org-log-repeat`
    pub log_repeat: Option<Logging>,
    /// `org-log-into-drawer`: the drawer state notes go into.
    pub log_into_drawer: Option<String>,
    /// `org-tags-column`
    pub tags_column: i64,
    /// `org-todo-repeat-to-state`
    pub repeat_to_state: RepeatTo,
    /// `org-adapt-indentation`: how the lines Org writes are indented.
    pub adapt: Adapt,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            log_done: None,
            log_repeat: Some(Logging::Time),
            log_into_drawer: None,
            tags_column: -77,
            repeat_to_state: RepeatTo::Head,
            adapt: Adapt::Off,
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

/// A state note that waits to be written.
struct Note {
    /// `State "DONE" from "TODO"` rather than `CLOSING NOTE`.
    state_change: bool,
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
        Editor { text: text.to_string(), keywords, settings, now, heading: 0, note: None }
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
    fn line_end(&self, at: usize) -> usize {
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
    fn property_drawer(&self) -> Option<(usize, usize)> {
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
    fn update_parent_statistics(&mut self) {
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
                    let stamp = format_timestamp(self.now, true, true, "");
                    let space = if content == self.line_end(line) { "" } else { " " };
                    self.replace(content, content, &format!("CLOSED: {stamp}{space}"));
                }
            }
            None if add => {
                let at = self.line_end(self.heading);
                let stamp = format_timestamp(self.now, true, true, "");
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
        let mut at = self.next_line(drawer);
        while at < end {
            let text = self.line(at).trim_start_matches([' ', '\t']);
            let is_it = text.get(..13).is_some_and(|key| key.eq_ignore_ascii_case(":LAST_REPEAT:"))
                && text[13..].chars().next().is_none_or(|c| c == ' ' || c == '\t');
            if is_it {
                let line_end = self.line_end(at);
                self.replace(at, line_end, &format!(":LAST_REPEAT: {value}"));
                self.indent_line(at);
                return;
            }
            at = self.next_line(at);
        }
        self.replace(end, end, &format!(":LAST_REPEAT: {value}\n"));
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
        let quoted = |state: &Option<String>| state.as_ref().map(|state| format!("\"{state}\"")).unwrap_or_default();
        let time = format_timestamp(self.now, true, true, "");
        let heading = if note.state_change {
            format!("State {:<12} from {:<12} {time}", quoted(&note.state), quoted(&note.previous))
        } else {
            format!("CLOSING NOTE {time}")
        };

        let mut at = match self.settings.log_into_drawer.clone() {
            Some(drawer) => self.log_drawer(&drawer),
            None => {
                let meta = self.end_of_meta_data();
                // Past blank lines, to the start of the next line with text.
                let skipped = self.text[meta..].bytes().take_while(|b| matches!(b, b' ' | b'\t' | b'\n')).count();
                self.line_start(meta + skipped).max(meta)
            }
        };
        // The note needs a line of its own.
        if at > 0 && self.text.as_bytes()[at - 1] != b'\n' {
            self.replace(at, at, "\n");
            at += 1;
        } else if self.line(at).bytes().any(|b| !is_blank(b)) {
            self.replace(at, at, "\n");
        }
        // `org-indent-line` on the note's line.
        let blanks = leading_blanks(self.line(at));
        let indent = match indent::line_column(&self.text, at, &self.indent_settings()) {
            Some(column) => " ".repeat(column),
            None => self.line(at)[..blanks].to_string(),
        };
        let lines = note_lines(text);
        let mut item = format!("{indent}- {heading}");
        if !lines.is_empty() {
            item.push_str(" \\\\");
        }
        // The text below the heading, indented to the item's body.
        for line in lines {
            item.push('\n');
            if !line.is_empty() {
                item.push_str(&indent);
                item.push_str("  ");
            }
            item.push_str(line);
        }
        self.replace(at, at + blanks, &item);
    }

    /// `org-log-beginning` with a drawer: the line below the drawer's
    /// first line. The drawer is created after the entry's planning line
    /// and property drawer when it does not exist.
    pub(crate) fn log_drawer(&mut self, name: &str) -> usize {
        let meta = self.end_of_meta_data();
        let end = self.entry_end();
        let opening = format!(":{name}:");
        let mut line = meta;
        while line < end {
            if self.line(line).trim_matches([' ', '\t']).eq_ignore_ascii_case(&opening) && self.drawer_closes(line, end)
            {
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

    /// The first repeater of the entry, as `org-get-repeat` finds it.
    fn first_repeater(&self) -> Option<String> {
        let (parse, _, end) = self.entry_outline();
        let mut at = self.heading;
        while let Some((start, repeater_end)) = next_repeater(&self.text, at, end) {
            if self.in_timestamp(&parse, repeater_end.min(end - 1)) {
                let stamp = &self.text[start..repeater_end];
                let repeater_start = stamp.rfind(' ').map_or(0, |index| index + 1);
                return Some(stamp[repeater_start..].to_string());
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
        self.set_state(&Target::State(state), false)?;

        self.planning_closed(false);
        if self.settings.log_repeat.is_some() || self.has_clock() {
            self.put_last_repeat();
        }
        if self.settings.log_repeat.is_some() && self.note.is_none() {
            self.note = Some(Note {
                state_change: true,
                state: done_word
                    .clone()
                    .or_else(|| self.keywords.names().find(|name| self.keywords.is_done(name)).map(str::to_string)),
                // Org writes `""` here for an entry that had no keyword.
                previous: Some(previous.clone().unwrap_or_default()),
                text: self.settings.log_repeat == Some(Logging::Note),
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
            let (count, unit) = if unit == 'w' { (count * 7, 'd') } else { (count, unit) };
            if unit == 'h' && !has_clock_time(&stamp) {
                return Err(format!("Cannot repeat in {count} hour(s) because no hour has been set"));
            }
            match kind {
                // `.+`: count from today, or from now when repeating by hours.
                Some('.') => {
                    let time = time_of(&stamp)?;
                    if unit == 'h' {
                        let minutes = (self.now.day - time.day) * 1440 + self.now.minute - time.minute;
                        point = self.shift_timestamp(point, minutes, 'M', false)?.cursor;
                    } else {
                        point = self.shift_timestamp(point, self.now.day - time.day, 'd', false)?.cursor;
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
                            self.now.day >= time.day
                        };
                        if shifts > 0 && !behind {
                            break;
                        }
                        shifts += 1;
                        point = self.shift_timestamp(point, count, unit, false)?.cursor;
                        time = self.timestamp_time(point)?;
                    }
                    point = self.shift_timestamp(point, -count, unit, false)?.cursor;
                }
                _ => {}
            }
            at = self.shift_timestamp(point, count, unit, true)?.stamp.1;
        }
        Ok(())
    }

    // ----- the command

    /// `org-todo` on the entry at `self.heading`.
    fn set_state(&mut self, target: &Target, top: bool) -> Result<(), String> {
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
            Target::State(None) => None,
            Target::State(Some(state)) if state.is_empty() => None,
            Target::State(Some(state)) => {
                if !self.keywords.contains(state) {
                    return Err(format!("State `{state}' not valid in this file"));
                }
                Some(state.clone())
            }
        };
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
        if top && (self.keywords.logs_any() || self.settings.log_done.is_some()) {
            let record = state
                .as_deref()
                .and_then(|state| self.keywords.logs_entering(state))
                .or_else(|| this.as_deref().and_then(|this| self.keywords.logs_leaving(this)));
            if state.is_none() || (is_open(&state) && !is_open(&this)) {
                // No longer done: a `CLOSED:` timestamp is out of date.
                self.planning_closed(false);
            }
            if now_done && self.settings.log_done.is_some() {
                self.planning_closed(true);
                if record.is_none() && self.settings.log_done == Some(Logging::Note) {
                    let (state, previous) = (state.clone(), this.clone());
                    self.note = Some(Note { state_change: false, state, previous, text: true });
                }
            }
            if state.is_some() && record.is_some() {
                let text = record == Some(Logging::Note);
                self.note = Some(Note { state_change: true, state: state.clone(), previous: this.clone(), text });
            }
        }
        self.align_tags(self.heading);
        self.update_parent_statistics();
        if now_done {
            self.auto_repeat(&state, &this)?;
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
        return Ok(Some(Target::Next));
    }
    let settings = Settings::default();
    let mut editor = Editor {
        text: text.to_string(),
        keywords,
        settings: &settings,
        now: DateTime::new(crate::date::Date::new(1970, 1, 1), 0, 0),
        heading: 0,
        note: None,
    };
    editor.go_to_heading(offset)?;
    let (current, ..) = editor.keyword_match();
    Ok(match answer.chars().next() {
        Some(' ') => Some(Target::State(None)),
        Some(key) => keywords.fast_key(key, current.as_deref()).map(|name| Target::State(Some(name.to_string()))),
        None => None,
    })
}

/// A state change that may want a note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changed {
    pub text: String,
    /// The note the change wants, when no text was given for it. The text
    /// is then without the note.
    pub wants_note: Option<WantedNote>,
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
    let mut editor = Editor { text: text.to_string(), keywords, settings, now, heading: 0, note: None };
    editor.go_to_heading(offset)?;
    editor.set_state(target, true)?;
    let wants_note = match (editor.note.take(), note) {
        // `org-store-log-note` with `org-note-abort`: nothing is stored.
        (Some(wanted), NoteText::Skip) if wanted.text => None,
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
            let purpose = if wanted.state_change {
                let name = |state: &Option<String>| state.clone().unwrap_or_default();
                format!("state change from \"{}\" to \"{}\"", name(&wanted.previous), name(&wanted.state))
            } else {
                "closed todo item".to_string()
            };
            Some(WantedNote { purpose, state: wanted.state })
        }
        (None, _) => None,
    };
    Ok(Changed { text: editor.text, wants_note })
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
        return vec![LineEdit { start: prefix, end: prefix + a.len(), text: b.concat() }];
    }
    // Longest common subsequence of the lines in between.
    let mut table = vec![vec![0u32; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            table[i][j] = if a[i] == b[j] { table[i + 1][j + 1] + 1 } else { table[i + 1][j].max(table[i][j + 1]) };
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
        if j < b.len() && (i == a.len() || table[i][j + 1] >= table[i + 1][j]) {
            edit.text.push_str(b[j]);
            j += 1;
        } else {
            edit.end += 1;
            i += 1;
        }
    }
    out.extend(open);
    out
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
}
