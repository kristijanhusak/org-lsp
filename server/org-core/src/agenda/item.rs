//! One agenda line: how an entry's headline becomes the text shown.
//!
//! This is a port of `org-agenda-format-item` and the helpers around it.
//! The format is the default one: two blanks, the category padded to
//! twelve columns, the time when there is one, a leader such as
//! `Scheduled:`, then the headline with its tags.

use crate::outline::Tag;
use crate::text::{pad_left, pad_right, trim, width};
use crate::timestr::{plain_time_range, stamp_time_range, time_of_day, TimeOfDay};

/// How an agenda line is styled. The names follow the Emacs faces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    /// Scheduled for a later day.
    Scheduled,
    ScheduledToday,
    /// Scheduled in the past and still open.
    ScheduledPreviously,
    /// A deadline that is due or overdue.
    DeadlineWarning,
    /// A deadline in the second half of its warning period.
    DeadlineUpcoming,
    /// A deadline in the first half of its warning period.
    DeadlineDistant,
    /// A plain timestamp: an appointment.
    CalendarEvent,
    /// The entry is done.
    Done,
    TimeGrid,
    CurrentTime,
    /// A date range or a line of the TODO list.
    Default,
}

/// What produced a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Deadline,
    Scheduled,
    Block,
    Timestamp,
    Todo,
    Grid,
}

/// Where an agenda line comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Source {
    /// Index into the list of files the agenda was built from.
    pub file: usize,
    /// Index into that file's entries.
    pub entry: usize,
}

/// One line of the agenda, before tags are aligned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub text: String,
    /// Byte offset in `text` where the headline starts, after the prefix.
    pub heading: usize,
    /// Military time for sorting, when the line carries a time of day.
    pub time_of_day: Option<i64>,
    pub priority: i64,
    pub kind: ItemKind,
    pub face: Face,
    pub source: Option<Source>,
    /// Length of the TODO keyword at `heading`, if the headline has one.
    pub keyword_len: usize,
    pub keyword_done: bool,
}

/// Which prefix format a line uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prefix {
    /// ` %i %-12:c%?-12t% s`
    Agenda,
    /// ` %i %-12:c`
    Todo,
}

/// Where to look for a time of day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dotime<'a> {
    /// The line never carries a time.
    No,
    /// Only in the headline.
    Headline,
    /// In this text first, then in the headline.
    Text(&'a str),
}

pub struct FormatInput<'a> {
    pub prefix: Prefix,
    /// The leader, such as `Scheduled: `; may be empty.
    pub extra: &'a str,
    /// The headline after the stars.
    pub head: &'a str,
    pub category: &'a str,
    pub tags: &'a [Tag],
    pub dotime: Dotime<'a>,
    /// Remove active timestamps from the headline (plain timestamps only).
    pub remove_stamps: bool,
    /// What follows a start time without an end time, usually `......`.
    pub time_trailer: &'a str,
}

pub struct Formatted {
    pub text: String,
    pub heading: usize,
    pub time_of_day: Option<i64>,
}

/// Whether `c` may appear in a tag.
fn is_tag_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '@' | '#' | '%')
}

/// `org-tag-group-re`: a tag group at the end of `text`, with the blanks
/// before it. Returns where the blanks start and where the tags start.
pub fn tag_group(text: &str) -> Option<(usize, usize)> {
    let body = text.trim_end_matches([' ', '\t']);
    let run: usize = body.chars().rev().take_while(|c| *c == ':' || is_tag_char(*c)).map(char::len_utf8).sum();
    let tags_start = body.len() - run;
    let tags = &body[tags_start..];
    let preceded_by_blank = body[..tags_start].ends_with([' ', '\t']);
    if !preceded_by_blank || tags.len() < 3 || !tags.starts_with(':') || !tags.ends_with(':') {
        return None;
    }
    let blanks = body[..tags_start].len() - body[..tags_start].trim_end_matches([' ', '\t']).len();
    Some((tags_start - blanks, tags_start))
}

/// `org-agenda-fix-displayed-tags`: replace the tags written in the
/// headline by the full list, inherited ones first and set off by `::`.
fn fix_displayed_tags(text: &str, tags: &[Tag]) -> String {
    let mut out = match tag_group(text) {
        Some((blanks, _)) => text[..blanks].to_string(),
        None => text.to_string(),
    };
    if tags.is_empty() {
        return out;
    }
    out.push_str(" :");
    let mut in_inherited = tags[0].inherited;
    for (index, tag) in tags.iter().enumerate() {
        if index > 0 {
            out.push(':');
        }
        if in_inherited && !tag.inherited {
            in_inherited = false;
            out.push(':');
        }
        out.push_str(&tag.name);
    }
    out.push_str(if in_inherited { "::" } else { ":" });
    out
}

/// `org-get-time-of-day` with a string result: `H:MM` in 24-hour form.
fn clock_text(time: TimeOfDay) -> String {
    format!("{}:{:02}", time.hours, time.minutes)
}

/// The `overtime` form: times past midnight become `+H:MM`.
fn overtime_text(time: TimeOfDay) -> String {
    if time.hours > 24 || (time.hours == 24 && time.minutes > 0) {
        format!("+{}:{:02}", time.hours - 24, time.minutes)
    } else {
        clock_text(time)
    }
}

/// Remove every active timestamp (`org-ts-regexp`) from `text`.
fn remove_active_timestamps(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(open) = text[at..].find('<').map(|index| at + index) {
        out.push_str(&text[at..open]);
        match crate::outline::active_timestamp_len(&text[open..]) {
            Some(len) => at = open + len,
            None => {
                out.push('<');
                at = open + 1;
            }
        }
    }
    out.push_str(&text[at..]);
    out
}

/// `org-agenda-format-item`.
pub fn format_item(input: &FormatInput<'_>) -> Formatted {
    let mut txt = fix_displayed_tags(trim(input.head), input.tags);

    // The text searched for a time: the given one, then the headline.
    let searched = match input.dotime {
        Dotime::No => None,
        Dotime::Headline => Some(txt.clone()),
        Dotime::Text(text) => Some(format!("{text}{txt}")),
    };
    let found = searched.as_deref().and_then(|ts| time_of_day(ts).map(|time| (ts, time)));
    let mut time = String::new();
    let mut time_of_day_value = None;
    if let Some((ts, first_time)) = found {
        time_of_day_value = Some(first_time.military());
        // A full timestamp wins over a bare time; either may be a range.
        let (matched, start, end) = match stamp_time_range(ts) {
            Some(stamp) => (
                &ts[stamp.matched.0..stamp.matched.1],
                time_of_day(&ts[stamp.start.0..stamp.start.1]),
                stamp.end.and_then(|(a, b)| time_of_day(&ts[a..b])),
            ),
            None => match plain_time_range(ts) {
                Some((first, second)) => {
                    let end = second.map_or(first.end, |second| second.end);
                    (&ts[first.start..end], Some(first), second)
                }
                None => ("", None, None),
            },
        };
        // The time moves to the prefix, so take it out of the headline,
        // unless what follows it is a closing bracket.
        if input.prefix == Prefix::Agenda && !matched.is_empty() {
            if let Some(at) = txt.find(matched) {
                let after = at + matched.len();
                let spaces = txt[after..].bytes().take_while(|b| *b == b' ').count();
                if !txt[after + spaces..].starts_with(']') {
                    txt.replace_range(at..after + spaces, "");
                }
            }
        }
        time = match (start, end) {
            (Some(start), Some(end)) => format!("{}-{}", pad_left(&overtime_text(start), 5), overtime_text(end)),
            (Some(start), None) => format!("{}{}", pad_left(&overtime_text(start), 5), input.time_trailer),
            _ => String::new(),
        };
    }

    if input.remove_stamps {
        txt = remove_active_timestamps(&txt);
    }

    let mut text = String::from("  ");
    if input.category.is_empty() {
        text.push_str(&pad_right("", 12));
    } else {
        text.push_str(&pad_right(&format!("{}:", input.category), 12));
    }
    if input.prefix == Prefix::Agenda {
        if !time.is_empty() {
            text.push_str(&pad_right(&time, 12));
        }
        if !input.extra.is_empty() {
            text.push_str(input.extra);
            text.push(' ');
        }
    }
    let heading = text.len();
    text.push_str(&txt);
    Formatted { text, heading, time_of_day: time_of_day_value }
}

/// `org-get-priority`: the value of the first `[#X]` cookie in `text`,
/// or the default priority. Higher is more urgent.
pub fn priority(text: &str) -> i64 {
    const LOWEST: i64 = b'C' as i64;
    const DEFAULT: i64 = b'B' as i64;
    let bytes = text.as_bytes();
    let cookie = text.match_indices("[#").find_map(|(at, _)| {
        let value: usize = bytes[at + 2..].iter().take_while(|b| b.is_ascii_uppercase() || b.is_ascii_digit()).count();
        (value > 0 && bytes.get(at + 2 + value) == Some(&b']')).then(|| &text[at + 2..at + 2 + value])
    });
    let value = match cookie {
        None => DEFAULT,
        Some(cookie) => {
            // Digits anywhere make it a numeric priority.
            let digits: String =
                cookie.chars().skip_while(|c| !c.is_ascii_digit()).take_while(char::is_ascii_digit).collect();
            match digits.parse::<i64>() {
                Ok(number) => number,
                Err(_) if digits.is_empty() => i64::from(cookie.as_bytes()[0]),
                Err(_) => i64::MAX / 2000,
            }
        }
    };
    1000 * (LOWEST - value)
}

/// `org-agenda-highlight-todo`: find the keyword at the start of the
/// headline and leave exactly one space after it. Returns the keyword's
/// length when there is one.
pub fn normalise_keyword<'k>(
    text: &mut String,
    heading: usize,
    mut keywords: impl Iterator<Item = &'k str>,
) -> Option<usize> {
    let rest = &text[heading..];
    let dots = rest.bytes().take_while(|b| *b == b'.').count();
    let after_dots = &rest[dots..];
    let keyword =
        keywords.find(|keyword| after_dots.strip_prefix(keyword).is_some_and(|after| after.starts_with(' ')))?;
    let spaces_at = heading + dots + keyword.len();
    let spaces = text[spaces_at..].bytes().take_while(|b| *b == b' ').count();
    text.replace_range(spaces_at..spaces_at + spaces, " ");
    Some(keyword.len())
}

/// `org-agenda-align-tags` for one line: put the tag group so that it
/// ends at column `-column`, or starts at `column` when that is positive.
pub fn align_tags(text: &str, column: i64) -> String {
    let Some((blanks, tags_start)) = tag_group(text) else { return text.to_string() };
    let tags_end = text.trim_end_matches([' ', '\t']).len();
    let tags = &text[tags_start..tags_end];
    let target = if column < 0 { -column - width(tags) as i64 } else { column };
    let current = width(&text[..blanks]) as i64;
    let padding = (target - current).max(1) as usize;
    format!("{}{}{}", &text[..blanks], " ".repeat(padding), &text[tags_start..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(name: &str, inherited: bool) -> Tag {
        Tag { name: name.to_string(), inherited }
    }

    fn agenda(extra: &str, head: &str, tags: &[Tag], dotime: Dotime<'_>) -> String {
        format_item(&FormatInput {
            prefix: Prefix::Agenda,
            extra,
            head,
            category: "work",
            tags,
            dotime,
            remove_stamps: false,
            time_trailer: "......",
        })
        .text
    }

    #[test]
    fn tag_groups() {
        assert_eq!(tag_group("Title :a:b:"), Some((5, 6)));
        assert_eq!(tag_group("Title   :a:  "), Some((5, 8)));
        assert_eq!(tag_group(":a:"), None);
        assert_eq!(tag_group("Title :a b:"), None);
        assert_eq!(tag_group("Title ::"), None);
        assert_eq!(tag_group("Title :::"), Some((5, 6)));
        assert_eq!(tag_group("Title :a: :b:"), Some((9, 10)));
        assert_eq!(tag_group("Title :a-b:"), None);
    }

    #[test]
    fn displayed_tags() {
        let both = [tag("file", true), tag("p", true), tag("own", false)];
        assert_eq!(fix_displayed_tags("Title :own:", &both), "Title :file:p::own:");
        assert_eq!(fix_displayed_tags("Title", &both[..2]), "Title :file:p::");
        assert_eq!(fix_displayed_tags("Title :own:", &both[2..]), "Title :own:");
        assert_eq!(fix_displayed_tags("Title :gone:", &[]), "Title");
    }

    #[test]
    fn prefixes() {
        assert_eq!(agenda("Scheduled: ", "TODO Task", &[], Dotime::Headline), "  work:       Scheduled:  TODO Task");
        assert_eq!(
            agenda("Scheduled: ", "TODO Task", &[], Dotime::Text("08:00 +1w ")),
            "  work:        8:00...... Scheduled:  TODO Task"
        );
        assert_eq!(
            agenda(
                "",
                "Meeting <2026-10-06 Tue 14:00-15:30>",
                &[tag("m", false)],
                Dotime::Text("<2026-10-06 Tue 14:00-15:30>")
            ),
            "  work:       14:00-15:30 Meeting :m:"
        );
        assert_eq!(agenda("", "Call at 3pm today", &[], Dotime::Headline), "  work:       15:00...... Call at today");
        assert_eq!(agenda("", "Late 25:30", &[], Dotime::Headline), "  work:       +1:30...... Late ");
        assert_eq!(agenda("", "Late 24:30 x", &[], Dotime::Headline), "  work:       +0:30...... Late x");
        assert_eq!(agenda("", "No time", &[], Dotime::No), "  work:       No time");
    }

    #[test]
    fn priorities() {
        assert_eq!(priority("TODO Task"), 1000);
        assert_eq!(priority("TODO [#A] Task"), 2000);
        assert_eq!(priority("TODO [#C] Task"), 0);
        assert_eq!(priority("x [#a] y"), 1000);
        assert_eq!(priority("x [#Z] y"), -23000);
        assert_eq!(priority("x [#5] y"), 62000);
        assert_eq!(priority("[#] [#B]"), 1000);
    }

    #[test]
    fn keyword_spacing() {
        let keywords = ["TODO", "TODOX", "DONE"];
        let mut text = "  work:       TODO   Task".to_string();
        assert_eq!(normalise_keyword(&mut text, 14, keywords.into_iter()), Some(4));
        assert_eq!(text, "  work:       TODO Task");
        let mut text = "  work:       TODOX Task".to_string();
        assert_eq!(normalise_keyword(&mut text, 14, keywords.into_iter()), Some(5));
        let mut text = "  work:       TODO".to_string();
        assert_eq!(normalise_keyword(&mut text, 14, keywords.into_iter()), None);
    }

    #[test]
    fn tag_alignment() {
        assert_eq!(align_tags("ab :t:", -10), "ab     :t:");
        assert_eq!(align_tags("abcdefghij :t:", -10), "abcdefghij :t:");
        assert_eq!(align_tags("ab   :t:", 4), "ab  :t:");
        assert_eq!(align_tags("日本 :t:", -10), "日本   :t:");
        assert_eq!(align_tags("no tags", -10), "no tags");
    }
}
