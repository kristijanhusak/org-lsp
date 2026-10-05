//! One agenda line: how an entry's headline becomes the text shown.
//!
//! This is a port of `org-agenda-format-item` and the helpers around it.
//! The format is the default one: two blanks, the category padded to
//! twelve columns, the time when there is one, a leader such as
//! `Scheduled:`, then the headline with its tags.

use crate::outline::Tag;
use crate::priority::Priorities;
use crate::text::{pad_left, pad_right, tag_group, trim, width};
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
    /// A plain timestamp, or a range within one day: an appointment.
    CalendarEvent,
    /// A date range of several days.
    CalendarDateRange,
    /// The entry is done.
    Done,
    TimeGrid,
    CurrentTime,
    /// A line of the TODO list.
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
    /// Byte offset in the file of the timestamp the line stands for, where
    /// Emacs keeps its marker: the opening bracket, or the end of a date
    /// range.
    pub marker: Option<usize>,
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

/// `org-agenda-fix-displayed-tags`: replace the tags written in the
/// headline by the full list, inherited ones first and set off by `::`.
///
/// Org marks a tag as inherited with a text property, which an empty tag
/// (`:a::b:`) cannot carry. So an empty tag always looks like the entry's
/// own, and the separator lands in front of it.
fn fix_displayed_tags(text: &str, tags: &[Tag]) -> String {
    let mut out = match tag_group(text) {
        Some((blanks, _)) => text[..blanks].to_string(),
        None => text.to_string(),
    };
    if tags.is_empty() {
        return out;
    }
    let looks_inherited = |tag: &Tag| tag.inherited && !tag.name.is_empty();
    out.push_str(" :");
    let mut in_inherited = looks_inherited(&tags[0]);
    for (index, tag) in tags.iter().enumerate() {
        if index > 0 {
            out.push(':');
        }
        if in_inherited && !looks_inherited(tag) {
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

/// Remove every timestamp, active or not (`org-ts-regexp-both`), from `text`.
fn remove_timestamps(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(open) = text[at..].find(['<', '[']).map(|index| at + index) {
        out.push_str(&text[at..open]);
        match crate::outline::timestamp_end(text, open) {
            Some(end) => at = end,
            None => {
                out.push_str(&text[open..open + 1]);
                at = open + 1;
            }
        }
    }
    out.push_str(&text[at..]);
    out
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

    // The text searched for a time: the given one, then the headline
    // without its timestamps.
    let searched = match input.dotime {
        Dotime::No => None,
        Dotime::Headline => Some(remove_timestamps(&txt)),
        Dotime::Text(text) => Some(format!("{text}{}", remove_timestamps(&txt))),
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

/// `org-get-priority`: the value of the first priority cookie in `text`,
/// or the default priority. Higher is more urgent.
///
/// A cookie holds one capital letter or a number from 0 to 64.
pub fn priority(text: &str, priorities: &Priorities) -> i64 {
    let bytes = text.as_bytes();
    let digit = |b: u8| i64::from(b - b'0');
    let value = text.match_indices("[#").find_map(|(at, _)| match bytes[at + 2..] {
        [letter, b']', ..] if letter.is_ascii_uppercase() => Some(i64::from(letter)),
        [ones, b']', ..] if ones.is_ascii_digit() => Some(digit(ones)),
        [tens @ b'1'..=b'5', ones @ b'0'..=b'9', b']', ..] | [tens @ b'6', ones @ b'0'..=b'4', b']', ..] => {
            Some(10 * digit(tens) + digit(ones))
        }
        _ => None,
    });
    1000 * (priorities.lowest - value.unwrap_or(priorities.default))
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

/// The parts of `text` Org does not display in the agenda: of a link
/// `[[target][description]]` everything but the description, and of
/// `[[target]]` the brackets. Returned as byte ranges in order.
pub fn hidden_ranges(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut hidden = Vec::new();
    let mut at = 0;
    while let Some(open) = text[at..].find("[[").map(|index| at + index) {
        at = open + 1;
        // The target: anything but brackets, which a backslash escapes.
        let mut end = open + 2;
        while end < bytes.len() && !matches!(bytes[end], b'[' | b']') {
            if bytes[end] == b'\\' {
                let slashes = bytes[end..].iter().take_while(|b| **b == b'\\').count();
                let escapes = slashes % 2 == 1 && matches!(bytes.get(end + slashes), Some(b'[' | b']'));
                end += slashes + usize::from(escapes);
            } else {
                end += 1;
            }
        }
        if end == open + 2 || bytes.get(end) != Some(&b']') {
            continue;
        }
        match bytes.get(end + 1) {
            Some(b']') => {
                hidden.push((open, open + 2));
                hidden.push((end, end + 2));
                at = end + 2;
            }
            Some(b'[') => {
                // The description: at least one character, up to the first `]]`.
                let description = end + 2;
                let after_first = description + text[description..].chars().next().map_or(0, char::len_utf8);
                let Some(close) = text.get(after_first..).and_then(|rest| rest.find("]]")) else { continue };
                hidden.push((open, description));
                hidden.push((after_first + close, after_first + close + 2));
                at = after_first + close + 2;
            }
            _ => {}
        }
    }
    hidden
}

/// The column the agenda display reaches after `text`, which starts a
/// line: links are folded and a tab moves to the next multiple of eight.
pub fn display_column(text: &str) -> usize {
    const TAB_WIDTH: usize = 8;
    let hidden = hidden_ranges(text);
    let mut hidden = hidden.iter().peekable();
    let mut column = 0;
    for (at, c) in text.char_indices() {
        while hidden.peek().is_some_and(|(_, end)| *end <= at) {
            hidden.next();
        }
        if hidden.peek().is_some_and(|(start, _)| *start <= at) {
            continue;
        }
        column = if c == '\t' { (column / TAB_WIDTH + 1) * TAB_WIDTH } else { column + crate::text::char_width(c) };
    }
    column
}

/// `org-agenda-align-tags` for one line: put the tag group so that it
/// ends at column `-column`, or starts at `column` when that is positive.
/// Columns are counted the way the agenda displays the line.
pub fn align_tags(text: &str, column: i64) -> String {
    let Some((blanks, tags_start)) = tag_group(text) else { return text.to_string() };
    let tags_end = text.trim_end_matches([' ', '\t']).len();
    let tags = &text[tags_start..tags_end];
    let target = if column < 0 { -column - width(tags) as i64 } else { column };
    let current = display_column(&text[..blanks]) as i64;
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
        // An empty tag never looks inherited.
        let with_empty = [tag("a", true), tag("", true), tag("b", true), tag("own", false)];
        assert_eq!(fix_displayed_tags("Title", &with_empty[..3]), "Title :a:::b:");
        assert_eq!(fix_displayed_tags("Title", &with_empty), "Title :a:::b:own:");
        assert_eq!(fix_displayed_tags("Title", &with_empty[1..3]), "Title ::b:");
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
        let priority_of = |text: &str| priority(text, &Priorities::default());
        assert_eq!(priority_of("TODO Task"), 1000);
        assert_eq!(priority_of("TODO [#A] Task"), 2000);
        assert_eq!(priority_of("TODO [#C] Task"), 0);
        assert_eq!(priority_of("x [#a] y"), 1000);
        assert_eq!(priority_of("x [#Z] y"), -23000);
        assert_eq!(priority_of("x [#5] y"), 62000);
        assert_eq!(priority_of("x [#64] y"), 3000);
        assert_eq!(priority_of("x [#65] [#05] y"), 1000);
        assert_eq!(priority_of("[#] [#B]"), 1000);
        let wide = Priorities { highest: 65, lowest: 69, default: 67 };
        assert_eq!(priority("TODO Task", &wide), 2000);
        assert_eq!(priority("[#A]", &wide), 4000);
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
        // Folded links do not count.
        assert_eq!(align_tags("[[a][b]] :t:", -10), "[[a][b]]      :t:");
        // A tab reaches the next tab stop.
        assert_eq!(align_tags("a\tb :t:", -14), "a\tb  :t:");
        assert_eq!(display_column("ab\t[[x][日本]]\t"), 16);
    }

    #[test]
    fn hidden_link_parts() {
        fn hidden(text: &str) -> Vec<&str> {
            hidden_ranges(text).into_iter().map(|(start, end)| &text[start..end]).collect()
        }
        assert_eq!(hidden("a [[link][desc]] b [[plain]] c"), ["[[link][", "]]", "[[", "]]"]);
        assert_eq!(hidden("[[a\\]b][d]] [[]] [[a][]] [[x]"), ["[[a\\]b][", "]]"]);
        assert_eq!(hidden("[[a][b] c]] [[[x]]"), ["[[a][", "]]", "[[", "]]"]);
    }
}
