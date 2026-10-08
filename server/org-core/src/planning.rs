//! The commands that ask for a date: `org-schedule`, `org-deadline`,
//! `org-timestamp` and `org-timestamp-inactive`.
//!
//! Each runs in two steps, as Emacs does around its prompt. [`prompt`]
//! says what the prompt starts from: the date the answer is read against
//! and the text it is filled with. [`run`] takes the answer, reads it with
//! [`crate::read_date`], and changes the document. Like Emacs, the
//! expressions here match without regard to case.

use std::sync::LazyLock;

use regex::Regex;

use crate::date::DateTime;
use crate::indent::Adapt;
use crate::read_date::{read_date, ReadDate, Settings};
use crate::stamp::{format_timestamp, Stamp};
use crate::timestr::parse_time_string;
use crate::todo::{headline_level, PlanningChange};
use crate::Refusal;

/// A command that asks for a date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// `org-schedule`
    Schedule,
    /// `org-deadline`
    Deadline,
    /// `org-timestamp`, or `org-timestamp-inactive` when `inactive`.
    Timestamp { inactive: bool },
    /// `org-schedule` with `C-u C-u`: a delay for the scheduled date
    /// (`-2d`); `org-deadline` so (`deadline`): a warning period.
    Delay { deadline: bool },
}

/// What the date prompt starts from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    /// The date and time an answer is read against (DEFAULT-TIME), when
    /// there is a timestamp to start from; otherwise the current time.
    pub default: Option<DateTime>,
    /// The text the prompt is filled with (DEFAULT-INPUT): the time of
    /// day of that timestamp, as `org-get-compact-tod` writes it.
    pub value: String,
}

/// What a command made of the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub text: String,
    /// Where Emacs leaves the cursor, after a timestamp it inserted.
    pub cursor: Option<usize>,
    /// What Emacs says when it is done: `Scheduled to <2026-05-22 Fri>`.
    pub message: Option<String>,
    /// A date the entry had and no longer has, which
    /// `org-log-reschedule` and `org-log-redeadline` may log.
    pub changed: Option<Changed>,
}

/// A planning date changed or taken out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changed {
    pub change: PlanningChange,
    /// The old timestamp, as the entry had it (`<2026-05-18 Mon>`).
    pub old: String,
    /// Byte offset of the entry's headline.
    pub heading: usize,
}

fn rx(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid pattern")
}

macro_rules! regex {
    ($pattern:expr) => {{
        static REGEX: LazyLock<Regex> = LazyLock::new(|| rx($pattern));
        &*REGEX
    }};
}

/// `org-scheduled-time-regexp`, `org-deadline-time-regexp`
fn planning_regexp(command: Command) -> &'static Regex {
    match command {
        Command::Deadline | Command::Delay { deadline: true } => regex!(r"(?i)\bDEADLINE: *<([^>]+)>"),
        _ => regex!(r"(?i)\bSCHEDULED: *<([^>]+)>"),
    }
}

/// `org-closed-time-regexp`
fn closed_regexp() -> &'static Regex {
    regex!(r"(?i)\bCLOSED: *\[([^\]]+)\]")
}

fn keyword(command: Command) -> &'static str {
    match command {
        Command::Deadline | Command::Delay { deadline: true } => "DEADLINE:",
        _ => "SCHEDULED:",
    }
}

fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |index| index + 1)
}

fn line_end(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |index| at + index)
}

/// `forward-line`: the start of the next line, or the end of the text.
fn next_line(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |index| at + index + 1)
}

/// `org-back-to-heading`: the start of the headline at or above `offset`.
fn heading_at(text: &str, offset: usize) -> Result<usize, Refusal> {
    let mut start = line_start(text, offset.min(text.len()));
    loop {
        if headline_level(&text[start..line_end(text, start)]).is_some() {
            return Ok(start);
        }
        if start == 0 {
            return Err(Refusal::NotHere("Before first headline".to_string()));
        }
        start = line_start(text, start - 1);
    }
}

/// `outline-next-heading`: where the next headline starts, or the end.
fn next_heading(text: &str, heading: usize) -> usize {
    let mut at = next_line(text, heading);
    while at < text.len() {
        if headline_level(&text[at..line_end(text, at)]).is_some() {
            return at;
        }
        at = next_line(text, at);
    }
    text.len()
}

/// `org-time-string-to-time`: the date and time in a timestamp.
fn time_string_to_time(text: &str) -> Result<DateTime, Refusal> {
    let parsed = parse_time_string(text).ok_or_else(|| Refusal::Failed(format!("Not an Org time string: {text}")))?;
    let (hour, minute) = parsed.time.unwrap_or((0, 0));
    Ok(DateTime::new(parsed.date, hour, minute))
}

/// `org-get-compact-tod`: `10:00`, or `10:00+1:30` for a time range.
fn compact_tod(text: &str) -> Option<String> {
    let pattern = regex!(r"(?i)(([012]?[0-9]):([0-5][0-9]))(-(([012]?[0-9]):([0-5][0-9])))?");
    let captures = pattern.captures(text)?;
    let first = captures[1].to_string();
    let Some(_) = captures.get(5) else { return Some(first) };
    let number = |index: usize| captures[index].parse::<i64>().unwrap_or(0);
    let (mut hours, mut minutes) = (number(6) - number(2), number(7) - number(3));
    if minutes < 0 {
        minutes += 60;
        hours -= 1;
    }
    let minutes = if minutes != 0 { format!(":{minutes:02}") } else { String::new() };
    Some(format!("{first}+{hours}{minutes}"))
}

/// The timestamp at `offset` that `org-timestamp` starts from: one half
/// of a date range, or a timestamp anywhere on the line around `offset`.
fn timestamp_at(text: &str, offset: usize) -> Option<String> {
    // `org-at-date-range-p` with inactive ranges.
    let range = regex!(
        r"(?i)\A[\[<]([0-9]{4}-[0-9]{2}-[0-9]{2}(?: .*?)?)[\]>]--?-?[\[<]([0-9]{4}-[0-9]{2}-[0-9]{2}(?: .*?)?)[\]>]"
    );
    let bytes = text.as_bytes();
    let mut at = offset.min(text.len());
    // Two tries: from the nearest opening bracket before the cursor, and
    // from the one before that.
    for _ in 0..2 {
        while at > 0 && !matches!(bytes[at - 1], b'[' | b'<' | b'\r' | b'\n') {
            at -= 1;
        }
        while at > 0 && matches!(bytes[at - 1], b'[' | b'<') {
            at -= 1;
        }
        if let Some(captures) = range.captures(&text[at..]) {
            let whole = captures.get(0).expect("whole match");
            if at + whole.end() >= offset {
                let second = captures.get(2).expect("second date");
                let index = if offset + 2 < at + second.start() { 1 } else { 2 };
                return Some(captures[index].to_string());
            }
        }
    }
    Stamp::around(text, offset).map(|stamp| text[stamp.start..stamp.end].to_string())
}

/// What the date prompt of `command` starts from, with the cursor at
/// `offset`.
pub fn prompt(command: Command, text: &str, offset: usize) -> Result<Prompt, Refusal> {
    let stamp = match command {
        Command::Schedule | Command::Deadline => {
            let heading = heading_at(text, offset)?;
            let entry = &text[heading..next_heading(text, heading)];
            planning_regexp(command).captures(entry).map(|captures| captures[1].to_string())
        }
        // The date to delay, asked for without a time.
        Command::Delay { deadline } => {
            let heading = heading_at(text, offset)?;
            let (_, rpl) = delayed(text, heading, deadline)?;
            return Ok(Prompt { default: Some(time_string_to_time(&rpl)?), value: String::new() });
        }
        Command::Timestamp { .. } => timestamp_at(text, offset),
    };
    match stamp {
        Some(stamp) => {
            Ok(Prompt { default: Some(time_string_to_time(&stamp)?), value: compact_tod(&stamp).unwrap_or_default() })
        }
        None => Ok(Prompt { default: None, value: String::new() }),
    }
}

/// `org-insert-timestamp`: the timestamp for `read`, with the end of a
/// time range when one was given as hours and minutes.
fn insert_timestamp(read: &ReadDate, inactive: bool, with_end: bool) -> String {
    let (hour, minute) = read.time.unwrap_or((0, 0));
    let time = DateTime::new(read.date, hour, minute);
    let end = read.end.as_deref().filter(|_| with_end).and_then(|end| {
        let captures = regex!(r"([0-9]+):([0-9]+)").captures(end)?;
        let number = |index: usize| captures[index].parse::<i64>().unwrap_or(0);
        Some(format!("-{:02}:{:02}", number(1), number(2)))
    });
    format_timestamp(time, read.time.is_some(), inactive, end.as_deref().unwrap_or(""))
}

/// Run `command` with the cursor at byte `offset` of `text`, reading
/// `answer` as the date. `repeated` says that a timestamp command ran just
/// before, which makes `org-timestamp` add the end of a date range.
/// `adapt` (`org-adapt-indentation`) indents a new planning line.
#[allow(clippy::too_many_arguments)]
pub fn run(
    command: Command,
    text: &str,
    offset: usize,
    answer: &str,
    repeated: bool,
    now: DateTime,
    settings: &Settings,
    adapt: Adapt,
) -> Result<Outcome, Refusal> {
    let prompt = prompt(command, text, offset)?;
    match command {
        Command::Schedule | Command::Deadline => schedule(command, text, offset, answer, &prompt, now, settings, adapt),
        Command::Timestamp { inactive } => timestamp(text, offset, answer, &prompt, inactive, repeated, now, settings),
        Command::Delay { deadline } => delay(deadline, text, offset, answer, &prompt, now, settings),
    }
}

/// Whether the cursor at `offset` is on a timestamp, and if so whether it
/// is an inactive one: what a command that changes the timestamp there
/// keeps.
pub fn timestamp_kind(text: &str, offset: usize) -> Option<bool> {
    Stamp::around(text, offset.min(text.len())).map(|stamp| text[stamp.start..].starts_with('['))
}

/// The timestamp a read answer makes, with the end of a time range.
pub fn timestamp_for(read: &ReadDate, inactive: bool) -> String {
    insert_timestamp(read, inactive, true)
}

/// The timestamp that `answer` would make, as the date prompt shows it
/// while the answer is typed (`org-read-date-display`).
pub fn preview(
    command: Command,
    text: &str,
    offset: usize,
    answer: &str,
    now: DateTime,
    settings: &Settings,
) -> Result<String, Refusal> {
    let prompt = prompt(command, text, offset)?;
    let read = read_date(answer, prompt.default, now, settings)?;
    Ok(insert_timestamp(&read, matches!(command, Command::Timestamp { inactive: true }), true))
}

/// The text before the answer at the date prompt of `command`: `SCHEDULED
/// Date+time [2026-05-20]: `, as `org-read-date` writes it for the date the
/// answer is read against.
pub fn label(command: Command, default: Option<DateTime>, now: DateTime) -> String {
    let date = default.unwrap_or(now).date();
    let prompt = match command {
        Command::Schedule => "SCHEDULED ",
        Command::Deadline => "DEADLINE ",
        Command::Timestamp { .. } => "",
        Command::Delay { deadline: false } => "Delay until ",
        Command::Delay { deadline: true } => "Warn starting from ",
    };
    format!("{prompt}Date+time [{date}]: ")
}

/// What the date prompt shows after an answer (`org-read-date-display`):
/// `=> <2026-05-22 Fri>`, with ` (=>F)` when a date without a year or
/// month was taken to be the next one to come.
pub fn display(
    command: Command,
    answer: &str,
    default: Option<DateTime>,
    now: DateTime,
    settings: &Settings,
) -> String {
    match read_date(answer, default, now, settings) {
        Ok(read) => {
            let inactive = matches!(command, Command::Timestamp { inactive: true });
            let future = if read.future { " (=>F)" } else { "" };
            format!("=> {}{future}", insert_timestamp(&read, inactive, true))
        }
        Err(_) => String::new(),
    }
}

/// `org--deadline-or-schedule` without a prefix argument, and
/// `org-add-planning-info` under it.
#[allow(clippy::too_many_arguments)]
fn schedule(
    command: Command,
    text: &str,
    offset: usize,
    answer: &str,
    prompt: &Prompt,
    now: DateTime,
    settings: &Settings,
    adapt: Adapt,
) -> Result<Outcome, Refusal> {
    let heading = heading_at(text, offset)?;
    let keyword = keyword(command);
    // The repeater of the old date (`org-entry-get`), put back afterwards.
    let planning_line = next_line(text, heading);
    let planning = is_planning(text, planning_line);
    let old_date = old_date(text, heading, keyword);
    if let Some(old) = &old_date {
        time_string_to_time(old)?;
    }
    let repeater = old_date.as_deref().and_then(|old| {
        let pattern = regex!(r"(?i)([.+-]+[0-9]+[hdwmy](?:[/ ][-+]?[0-9]+[hdwmy])?)");
        pattern.captures(old).map(|captures| captures[1].to_string())
    });

    let read = read_date(answer, prompt.default, now, settings)?;
    let mut out = text.to_string();
    let insert_at = if planning {
        let line = planning_line;
        let indent = line + text[line..].bytes().take_while(|b| matches!(b, b' ' | b'\t')).count();
        // Take out the old timestamp of this kind and any CLOSED one, each
        // up to the next keyword or the end of the line.
        for pattern in [planning_regexp(command), closed_regexp()] {
            let end = line_end(&out, indent);
            let Some(found) = pattern.find(&out[indent..end]) else { continue };
            let start = indent + found.start();
            let next = regex!(r"(?i)\b(?:SCHEDULED:|DEADLINE:|CLOSED:) *[\[<]([^\]>]+)[\]>]");
            let stop =
                next.find(&out[indent + found.end()..end]).map_or(end, |next| indent + found.end() + next.start());
            out.replace_range(start..stop, "");
        }
        // No blanks left at the end of the line.
        let end = line_end(&out, indent);
        let trimmed = indent + out[indent..end].trim_end_matches([' ', '\t']).len();
        out.replace_range(trimmed..end, "");
        indent
    } else {
        // A new line, past the stars with `org-adapt-indentation`.
        let end = line_end(&out, heading);
        let indentation = match adapt {
            Adapt::Off => String::new(),
            _ => " ".repeat(out[heading..].bytes().take_while(|b| *b == b'*').count() + 1),
        };
        out.insert_str(end, &format!("\n{indentation}"));
        end + 1 + indentation.len()
    };
    let stamp = insert_timestamp(&read, false, true);
    // `org-add-log-setup` when the date is another (the repeater aside).
    let change = if command == Command::Deadline { PlanningChange::Redeadline } else { PlanningChange::Reschedule };
    let changed = old_date.clone().filter(|old| *old != stamp).map(|old| Changed { change, old, heading });
    let mut inserted = format!("{keyword} {stamp}");
    let after = insert_at + inserted.len();
    if !matches!(out[insert_at..].chars().next(), None | Some('\n')) {
        inserted.push(' ');
    }
    out.insert_str(insert_at, &inserted);
    // The old repeater goes back in, before the closing bracket.
    let mut stamp = stamp;
    if let Some(repeater) = repeater {
        out.insert_str(after - 1, &format!(" {repeater}"));
        stamp.insert_str(stamp.len() - 1, &format!(" {repeater}"));
    }
    let message = match command {
        Command::Deadline => format!("Deadline on {stamp}"),
        _ => format!("Scheduled to {stamp}"),
    };
    Ok(Outcome { text: out, cursor: None, message: Some(message), changed })
}

/// The scheduled date or deadline `org-schedule` and `org-deadline` delay
/// with `C-u C-u`, searched for in the headline and the line after it:
/// where its timestamp is written, and the timestamp without a delay.
fn delayed(text: &str, heading: usize, deadline: bool) -> Result<((usize, usize), String), Refusal> {
    let command = Command::Delay { deadline };
    let limit = line_end(text, next_line(text, heading).min(text.len()));
    let Some(captures) = planning_regexp(command).captures(&text[heading..limit]) else {
        let what = if deadline { "deadline" } else { "scheduled" };
        return Err(Refusal::Failed(format!("No {what} information to update")));
    };
    let whole = captures.get(0).expect("a match");
    let rpl = regex!(r" -[0-9]+[hdwmy]").replace_all(&captures[1], "").into_owned();
    Ok(((heading + whole.start(), heading + whole.end()), rpl))
}

/// `org-schedule` and `org-deadline` with `C-u C-u`: the delay or warning
/// period to the day `answer` names, counted from the date
/// (`SCHEDULED: <2026-05-20 Wed -2d>`).
fn delay(
    deadline: bool,
    text: &str,
    offset: usize,
    answer: &str,
    prompt: &Prompt,
    now: DateTime,
    settings: &Settings,
) -> Result<Outcome, Refusal> {
    let heading = heading_at(text, offset)?;
    let ((start, end), rpl) = delayed(text, heading, deadline)?;
    let old = time_string_to_time(&rpl)?;
    let read = read_date(answer, prompt.default, now, settings)?;
    let days = (read.date.to_day() - old.date().to_day()).abs();
    let mut out = text.to_string();
    out.replace_range(start..end, &format!("{} <{rpl} -{days}d>", keyword(Command::Delay { deadline })));
    Ok(Outcome { text: out, cursor: None, message: None, changed: None })
}

/// Whether the line at `at` is a planning line.
fn is_planning(text: &str, at: usize) -> bool {
    at < text.len() && regex!(r"(?i)\A[ \t]*(?:CLOSED:|DEADLINE:|SCHEDULED:)").is_match(&text[at..line_end(text, at)])
}

/// The date `keyword` gives the entry of the headline at `heading`
/// (`org-entry-get`), as written.
fn old_date(text: &str, heading: usize, keyword: &str) -> Option<String> {
    let planning_line = next_line(text, heading);
    if !is_planning(text, planning_line) {
        return None;
    }
    let line = &text[planning_line..line_end(text, planning_line)];
    let at = line.to_uppercase().rfind(keyword)?;
    let rest = line[at + keyword.len()..].trim_start_matches([' ', '\t']);
    let stamp = regex!(r"\A[\[<][0-9]{4}-[0-9]{2}-[0-9]{2}(?: .*?)?[\]>]").find(rest)?;
    Some(stamp.as_str().to_string())
}

/// `org-schedule` and `org-deadline` with `C-u`: the entry's dates of that
/// kind taken out (`org-remove-timestamp-with-keyword`), and the one it
/// had in `changed`, for `org-log-reschedule` and `org-log-redeadline`.
pub fn remove(command: Command, text: &str, offset: usize) -> Result<Outcome, Refusal> {
    let heading = heading_at(text, offset)?;
    let keyword = keyword(command);
    let deadline = command == Command::Deadline;
    let Some(old) = old_date(text, heading, keyword) else {
        let message = if deadline { "Entry had no deadline to remove" } else { "Entry was not scheduled" };
        return Ok(Outcome { text: text.to_string(), cursor: None, message: Some(message.to_string()), changed: None });
    };
    // The entry: up to the next headline.
    let start = next_line(text, heading);
    let end = regex!(r"(?m)^\*+ ").find_at(text, start).map_or(text.len(), |found| found.start());
    let pattern = Regex::new(&format!(r"\b{} +<[^>\n]+>[ \t]*", regex::escape(keyword))).expect("a valid regex");
    let found: Vec<(usize, usize)> =
        pattern.find_iter(&text[heading..end]).map(|found| (heading + found.start(), heading + found.end())).collect();
    let mut out = text.to_string();
    for (from, to) in found.into_iter().rev() {
        out.replace_range(from..to, "");
        let line = line_start(&out, from);
        if out[line..from].bytes().any(|b| !matches!(b, b' ' | b'\t')) {
            if from > 0 && out.as_bytes()[from - 1] == b' ' {
                out.replace_range(from - 1..from, "");
            }
        } else if out[line..line_end(&out, line)].bytes().all(|b| matches!(b, b' ' | b'\t')) {
            let stop = (line_end(&out, line) + 1).min(out.len());
            out.replace_range(line..stop, "");
        }
    }
    let message = if deadline { "Entry no longer has a deadline." } else { "Entry is no longer scheduled." };
    let change = if deadline { PlanningChange::Undeadline } else { PlanningChange::Unschedule };
    Ok(Outcome {
        text: out,
        cursor: None,
        message: Some(message.to_string()),
        changed: Some(Changed { change, old, heading }),
    })
}

/// The timestamp that `org-in-regexp` matched last when it found none
/// around `pos`: it searches the line from its start and stops at the
/// first timestamp that starts after `pos`, or at the end of the line.
fn last_scanned(text: &str, pos: usize) -> Option<Stamp> {
    let (start, end) = (line_start(text, pos), line_end(text, pos));
    let mut at = start;
    let mut last = None;
    while at < end {
        match Stamp::at(text, at).filter(|stamp| stamp.end <= end) {
            Some(stamp) => {
                last = Some(stamp);
                if stamp.start > pos {
                    break;
                }
                at = stamp.end;
            }
            None => at += 1,
        }
    }
    last
}

/// `org-timestamp`
#[allow(clippy::too_many_arguments)]
fn timestamp(
    text: &str,
    offset: usize,
    answer: &str,
    prompt: &Prompt,
    inactive: bool,
    repeated: bool,
    now: DateTime,
    settings: &Settings,
) -> Result<Outcome, Refusal> {
    let offset = offset.min(text.len());
    let found = timestamp_at(text, offset);
    let repeater = found
        .as_deref()
        .and_then(|stamp| regex!(r"(?i)([.+-]+[0-9]+[hdwmy] ?)+").find(stamp).map(|found| found.as_str().to_string()));
    let read = read_date(answer, prompt.default, now, settings)?;
    let mut out = text.to_string();
    if found.is_some() && repeated {
        let inserted = format!("--{}", insert_timestamp(&read, inactive, false));
        out.insert_str(offset, &inserted);
        return Ok(Outcome { text: out, cursor: Some(offset + inserted.len()), message: None, changed: None });
    }
    if found.is_some() {
        // On a timestamp; from the dashes of a range, on the one after.
        // Off any timestamp, Emacs replaces the one its search for a
        // timestamp around the cursor matched last.
        let after = offset + text[offset..].bytes().take_while(|b| *b == b'-').count();
        let stamp =
            Stamp::around(text, offset).or_else(|| Stamp::around(text, after)).or_else(|| last_scanned(text, after));
        let stamp = stamp.ok_or_else(|| Refusal::Failed("No timestamp here".to_string()))?;
        let new = insert_timestamp(&read, inactive, true);
        out.replace_range(stamp.start..stamp.end, &new);
        let mut cursor = stamp.start + new.len();
        if let Some(repeater) = repeater {
            cursor -= 1;
            let inserted = format!(" {repeater}");
            out.insert_str(cursor, &inserted);
            cursor += inserted.len();
        }
        return Ok(Outcome {
            text: out,
            cursor: Some(cursor),
            message: Some("Timestamp updated".to_string()),
            changed: None,
        });
    }
    let new = insert_timestamp(&read, inactive, true);
    out.insert_str(offset, &new);
    Ok(Outcome { text: out, cursor: Some(offset + new.len()), message: None, changed: None })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::date::Date;

    fn run_at(marked: &str, command: Command, answer: &str) -> String {
        let offset = marked.find('¦').expect("cursor marker");
        let text = marked.replacen('¦', "", 1);
        let now = DateTime::new(Date::new(2026, 5, 18), 10, 30);
        let outcome = run(command, &text, offset, answer, false, now, &Settings::default(), Adapt::Off).unwrap();
        match outcome.cursor {
            Some(cursor) => format!("{}¦{}", &outcome.text[..cursor], &outcome.text[cursor..]),
            None => outcome.text,
        }
    }

    #[test]
    fn schedules_and_keeps_the_repeater() {
        assert_eq!(run_at("* TODO a¦\n", Command::Schedule, "fri"), "* TODO a\nSCHEDULED: <2026-05-22 Fri>\n");
        assert_eq!(
            run_at("* TODO a¦\nCLOSED: [2026-05-01 Fri] DEADLINE: <2026-05-20 Wed>\n", Command::Schedule, "sat"),
            "* TODO a\nSCHEDULED: <2026-05-23 Sat> DEADLINE: <2026-05-20 Wed>\n"
        );
        assert_eq!(
            run_at("* TODO a\nSCHEDULED: <2026-05-20 Wed 10:00 +1w>\n¦", Command::Schedule, "++1"),
            "* TODO a\nSCHEDULED: <2026-05-21 Thu +1w>\n"
        );
    }

    #[test]
    fn says_what_it_did_as_emacs_does() {
        let now = DateTime::new(Date::new(2026, 5, 18), 10, 30);
        let message = |text: &str, command, answer| {
            run(command, text, 0, answer, false, now, &Settings::default(), Adapt::Off).unwrap().message
        };
        let repeating = "* TODO a\nSCHEDULED: <2026-05-20 Wed +1w>\n";
        assert_eq!(message(repeating, Command::Schedule, "fri"), Some("Scheduled to <2026-05-22 Fri +1w>".to_string()));
        assert_eq!(message("* TODO a\n", Command::Deadline, "fri"), Some("Deadline on <2026-05-22 Fri>".to_string()));
        let timestamp = Command::Timestamp { inactive: false };
        assert_eq!(message("<2026-05-20 Wed>", timestamp, "fri"), Some("Timestamp updated".to_string()));
        assert_eq!(message("text", timestamp, "fri"), None);
    }

    #[test]
    fn knows_the_kind_of_timestamp_at_the_cursor() {
        let text = "a <2026-05-20 Wed> b [2026-05-21 Thu]";
        assert_eq!(timestamp_kind(text, 5), Some(false));
        assert_eq!(timestamp_kind(text, 25), Some(true));
        assert_eq!(timestamp_kind(text, 0), None);
    }

    #[test]
    fn prompts_with_the_time_of_the_old_date() {
        let text = "* TODO a\nSCHEDULED: <2026-05-20 Wed 10:00-11:30 +1w>\n";
        let prompt = prompt(Command::Schedule, text, 0).unwrap();
        assert_eq!(prompt.value, "10:00+1:30");
        assert_eq!(prompt.default, Some(DateTime::new(Date::new(2026, 5, 20), 10, 0)));
    }

    #[test]
    fn inserts_and_replaces_timestamps() {
        let timestamp = Command::Timestamp { inactive: false };
        assert_eq!(run_at("* a\ntext ¦here\n", timestamp, "fri"), "* a\ntext <2026-05-22 Fri>¦here\n");
        assert_eq!(
            run_at("* a\ntext <2026-05-2¦0 Wed +1w> here\n", timestamp, "3pm"),
            "* a\ntext <2026-05-20 Wed 15:00 +1w¦> here\n"
        );
    }
}
