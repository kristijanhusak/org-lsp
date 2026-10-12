//! One agenda line: how an entry's headline becomes the text shown.
//!
//! This is a port of `org-agenda-format-item` and the helpers around it.
//! The prefix before the headline follows `org-agenda-prefix-format`; by
//! default two blanks, the category padded to twelve columns, the time
//! when there is one, and a leader such as `Scheduled:`.

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
    /// What a diary sexp gives (`org-agenda-calendar-sexp`).
    CalendarSexp,
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
    /// A line of log mode: closed, clocked, or a state change.
    Log,
    /// What a diary sexp line gives (`org-agenda-get-sexps`).
    Sexp,
    /// A line of the search view, which has no `todo-state` in Org.
    Search,
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
    /// The habit a scheduled line is (`org-habit-p`), with `org-habit`.
    pub habit: Option<Box<super::habit::Habit>>,
    /// Where the line's consistency graph starts, and the face of each of
    /// its days.
    pub graph: Option<(usize, Vec<super::habit::Face>)>,
    /// The day of the timestamp the line stands for (Org's `ts-date`): a
    /// deadline's, a scheduled date's, a timestamp's own date.
    pub ts_date: Option<crate::date::Day>,
    /// What a line of log mode logs (Org's `type`): `closed`, `clock` or
    /// `state`.
    pub log: Option<&'static str>,
}

/// What a specifier of `org-agenda-prefix-format` stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    /// `%c`
    Category,
    /// `%t`
    Time,
    /// `%l`: a blank per level.
    Level,
    /// `%s`: the leader, such as `Scheduled: `.
    Extra,
    /// `%i`: no icons here, so always empty.
    Icon,
    /// `%T`: the last tag.
    Tag,
    /// `%e`
    Effort,
    /// `%b`
    Breadcrumbs,
    /// `%(...)`: Emacs Lisp, for the [`Evaluator`] to run.
    Eval,
}

/// A `%(…)` of `org-agenda-prefix-format` on one line, as
/// `org-agenda-format-item` evaluates it: in the entry's buffer, point on
/// its headline, the variables of the format bound.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Eval {
    /// The form, as written.
    pub form: String,
    /// The entry's file (one of the agenda's `file_paths`) and the byte
    /// offset of its headline; none for a line of the time grid.
    pub entry: Option<(String, usize)>,
    /// The variables `category`, `extra`, `time`, `level`, `tag`,
    /// `effort` and `breadcrumbs`, as the line has them. `effort` is nil
    /// without an effort, `breadcrumbs` when the format has no `%b`.
    pub category: String,
    pub extra: String,
    pub time: String,
    pub level: String,
    pub tag: String,
    pub effort: Option<String>,
    pub breadcrumbs: Option<String>,
}

/// What evaluates the `%(…)` of the prefix formats (Emacs): an [`Eval`]
/// to the string `format` makes of what the form returns. Without one,
/// they are empty.
#[derive(Clone, Default)]
pub struct Evaluator(pub Option<std::sync::Arc<Evaluate>>);

/// An [`Eval`] to what it shows.
pub type Evaluate = dyn Fn(&Eval) -> String + Send + Sync;

impl std::fmt::Debug for Evaluator {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str(if self.0.is_some() { "Evaluator(Some)" } else { "Evaluator(None)" })
    }
}

/// The settings are equal whoever evaluates the forms.
impl PartialEq for Evaluator {
    fn eq(&self, _: &Evaluator) -> bool {
        true
    }
}

impl Eq for Evaluator {}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Text(String),
    Field {
        field: Field,
        /// `%?`: nothing at all when the value is empty.
        optional: bool,
        /// `-`: padded on the right.
        left: bool,
        width: usize,
        precision: Option<usize>,
        /// A character appended to a value that is not empty: `%-12:c`.
        suffix: Option<char>,
        /// The Lisp of `%(…)`.
        form: Option<String>,
    },
}

/// `org-compile-prefix-format`: an `org-agenda-prefix-format` string,
/// read once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefixFormat {
    parts: Vec<Part>,
    /// `org-prefix-has-time`: the time moves out of the headline.
    has_time: bool,
    /// `org-prefix-has-tag`
    has_tag: bool,
    pub has_breadcrumbs: bool,
    /// `org-prefix-category-max-length`: the precision of `%c`.
    category_max: Option<usize>,
}

impl PrefixFormat {
    pub fn new(format: &str) -> PrefixFormat {
        let mut parts = Vec::new();
        let mut text = String::new();
        let (mut has_time, mut has_tag, mut has_breadcrumbs, mut category_max) = (false, false, false, None);
        let chars: Vec<char> = format.chars().collect();
        let mut at = 0;
        while at < chars.len() {
            if chars[at] != '%' {
                text.push(chars[at]);
                at += 1;
                continue;
            }
            match parse_specifier(&chars[at + 1..]) {
                Some((spec, length)) => {
                    if !text.is_empty() {
                        parts.push(Part::Text(std::mem::take(&mut text)));
                    }
                    match spec.field {
                        Field::Time => has_time = true,
                        Field::Tag => has_tag = true,
                        Field::Breadcrumbs => has_breadcrumbs = true,
                        Field::Category => category_max = spec.precision,
                        _ => {}
                    }
                    parts.push(Part::Field {
                        field: spec.field,
                        optional: spec.optional,
                        left: spec.left,
                        width: spec.width,
                        precision: spec.precision,
                        suffix: spec.suffix,
                        form: spec.form,
                    });
                    at += 1 + length;
                }
                None => {
                    // `format` reads `%%` as `%`.
                    text.push('%');
                    at += if chars.get(at + 1) == Some(&'%') { 2 } else { 1 };
                }
            }
        }
        if !text.is_empty() {
            parts.push(Part::Text(text));
        }
        PrefixFormat { parts, has_time, has_tag, has_breadcrumbs, category_max }
    }
}

struct Specifier {
    field: Field,
    optional: bool,
    left: bool,
    width: usize,
    precision: Option<usize>,
    suffix: Option<char>,
    form: Option<String>,
}

/// The length of the Lisp form `chars` starts with, at its `(`: up to
/// its closing parenthesis, past strings and characters (`?\)`) that
/// hold parentheses. None when it does not close.
fn form_length(chars: &[char]) -> Option<usize> {
    let mut depth = 0;
    let mut at = 0;
    while let Some(c) = chars.get(at) {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(at + 1);
                }
            }
            '"' => {
                at += 1;
                while *chars.get(at)? != '"' {
                    at += if chars[at] == '\\' { 2 } else { 1 };
                }
            }
            '?' => at += if chars.get(at + 1) == Some(&'\\') { 2 } else { 1 },
            '\\' => at += 1,
            _ => {}
        }
        at += 1;
    }
    None
}

/// A specifier after its `%`:
/// `\(\?\)?\([-+]?[0-9.]*\)\([ .;,:!?=|/<>]?\)\([cltseib]\|(.+?)\)`,
/// letters matched ignoring case as Emacs does; the length it takes.
fn parse_specifier(chars: &[char]) -> Option<(Specifier, usize)> {
    let mut at = 0;
    let optional = chars.first() == Some(&'?');
    if optional {
        at += 1;
    }
    let mut flags = String::new();
    if let Some(sign @ ('-' | '+')) = chars.get(at) {
        flags.push(*sign);
        at += 1;
    }
    while let Some(c) = chars.get(at).filter(|c| c.is_ascii_digit() || **c == '.') {
        flags.push(*c);
        at += 1;
    }
    let field_at = |at: usize| -> Option<(Field, usize)> {
        let c = *chars.get(at)?;
        if c == '(' {
            // A Lisp form, to its closing parenthesis.
            return Some((Field::Eval, form_length(&chars[at..])?));
        }
        let field = match c {
            'c' => Field::Category,
            't' => Field::Time,
            'l' => Field::Level,
            's' => Field::Extra,
            'i' => Field::Icon,
            'T' => Field::Tag,
            'e' => Field::Effort,
            'b' => Field::Breadcrumbs,
            'C' | 'L' | 'S' | 'E' | 'I' | 'B' => Field::Eval,
            _ => return None,
        };
        Some((field, 1))
    };
    let (suffix, (field, length)) = match field_at(at) {
        Some(found) => (None, found),
        None => {
            let suffix = *chars.get(at).filter(|c| " .;,:!?=|/<>".contains(**c))?;
            (Some(suffix), field_at(at + 1)?)
        }
    };
    let left = flags.starts_with('-');
    let digits = flags.trim_start_matches(['-', '+']);
    let (width, precision) = match digits.split_once('.') {
        Some((width, precision)) => (width, Some(precision.parse().unwrap_or(0))),
        None => (digits, None),
    };
    // A format no line could fill is cut to the widest one could.
    let width = width.parse().unwrap_or(0).min(1000);
    let form_at = at + usize::from(suffix.is_some());
    let total = form_at + length;
    let form = (chars.get(form_at) == Some(&'(')).then(|| chars[form_at..total].iter().collect());
    Some((Specifier { field, optional, left, width, precision, suffix, form }, total))
}

/// `format` with `%-W.Ps` or `%W.Ps`.
fn format_string(value: &str, left: bool, width: usize, precision: Option<usize>) -> String {
    let value: String = match precision {
        Some(precision) => value.chars().take(precision).collect(),
        None => value.to_string(),
    };
    if left {
        pad_right(&value, width)
    } else {
        pad_left(&value, width)
    }
}

/// `org-agenda-todo-keyword-format` and the like: `%s` in `format` given
/// `value`, `%%` a percent sign.
pub fn format_with(format: &str, value: &str) -> String {
    let chars: Vec<char> = format.chars().collect();
    let mut out = String::new();
    let mut at = 0;
    let mut used = false;
    while at < chars.len() {
        if chars[at] != '%' {
            out.push(chars[at]);
            at += 1;
            continue;
        }
        let mut end = at + 1;
        let left = chars.get(end) == Some(&'-');
        if left {
            end += 1;
        }
        let digits_start = end;
        while chars.get(end).is_some_and(|c| c.is_ascii_digit() || *c == '.') {
            end += 1;
        }
        let digits: String = chars[digits_start..end].iter().collect();
        match chars.get(end) {
            Some('%') => out.push('%'),
            Some('s') if !used => {
                used = true;
                let (width, precision) = match digits.split_once('.') {
                    Some((width, precision)) => (width, Some(precision.parse().unwrap_or(0))),
                    None => (digits.as_str(), None),
                };
                out.push_str(&format_string(value, left, width.parse().unwrap_or(0), precision));
            }
            Some(other) => {
                out.push('%');
                out.extend(chars[at + 1..end].iter());
                out.push(*other);
            }
            None => {
                out.extend(chars[at..end].iter());
                break;
            }
        }
        at = end + 1;
    }
    out
}

/// `format` with `%d` directives (`Sched.%2dx: `, `(%d/%d): `), each
/// taking the next of `numbers`.
pub fn format_numbers(format: &str, numbers: &[i64]) -> String {
    let chars: Vec<char> = format.chars().collect();
    let mut out = String::new();
    let mut at = 0;
    let mut next = numbers.iter();
    while at < chars.len() {
        if chars[at] != '%' {
            out.push(chars[at]);
            at += 1;
            continue;
        }
        let mut end = at + 1;
        let mut flags = String::new();
        while let Some(flag) = chars.get(end).filter(|c| matches!(c, '-' | '0' | '+' | ' ')) {
            flags.push(*flag);
            end += 1;
        }
        let digits_start = end;
        while chars.get(end).is_some_and(char::is_ascii_digit) {
            end += 1;
        }
        let width: usize = chars[digits_start..end].iter().collect::<String>().parse().unwrap_or(0);
        match chars.get(end) {
            Some('%') => out.push('%'),
            Some('d') => {
                let number = next.next().copied().unwrap_or(0);
                let sign = if number < 0 {
                    "-"
                } else if flags.contains('+') {
                    "+"
                } else {
                    ""
                };
                let digits = number.unsigned_abs().to_string();
                let length = sign.len() + digits.len();
                let padding = width.saturating_sub(length);
                if flags.contains('-') {
                    out.push_str(&format!("{sign}{digits}{}", " ".repeat(padding)));
                } else if flags.contains('0') {
                    out.push_str(&format!("{sign}{}{digits}", "0".repeat(padding)));
                } else {
                    out.push_str(&format!("{}{sign}{digits}", " ".repeat(padding)));
                }
            }
            Some(other) => {
                out.extend(chars[at..end].iter());
                out.push(*other);
            }
            None => {
                out.extend(chars[at..end].iter());
                break;
            }
        }
        at = end + 1;
    }
    out
}

/// `org-agenda-remove-tags`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoveTags {
    No,
    Yes,
    /// Only when the prefix shows a tag (`%T`).
    Prefix,
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
    pub format: &'a PrefixFormat,
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
    /// `%l`: a blank per level of the headline; empty for grid lines.
    pub level: usize,
    /// `%e`: the entry's `EFFORT` property.
    pub effort: Option<&'a str>,
    /// `%b`: the titles of the headlines above, each followed by the
    /// separator.
    pub breadcrumbs: &'a str,
    pub remove_tags: RemoveTags,
    /// The entry's file and the byte offset of its headline, for `%(…)`;
    /// none for a line of the time grid.
    pub entry: Option<(&'a str, usize)>,
    /// What evaluates `%(…)`.
    pub evaluator: &'a Evaluator,
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
        if input.format.has_time && !matched.is_empty() {
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

    // Tags in the prefix may take them out of the headline.
    let remove_tags = match input.remove_tags {
        RemoveTags::No => false,
        RemoveTags::Yes => true,
        RemoveTags::Prefix => input.format.has_tag,
    };
    if let Some((blanks, tags)) = tag_group(&txt) {
        if remove_tags {
            txt.truncate(blanks);
        } else {
            // The tags at column 50 of the headline at least one blank
            // after it, where they stay when nothing aligns them.
            let padding = 50usize.saturating_sub(txt.chars().count()).max(1);
            let group = txt[tags..].trim_end_matches([' ', '\t']).to_string();
            txt.truncate(blanks);
            txt.push_str(&" ".repeat(padding));
            txt.push_str(&group);
        }
    }

    if input.remove_stamps {
        txt = remove_active_timestamps(&txt);
    }

    // A category of `%.Nc` or more characters is cut to one less.
    let mut category = input.category.to_string();
    if let Some(max) = input.format.category_max.filter(|max| category.chars().count() >= *max) {
        category = category.chars().take(max.saturating_sub(1)).collect();
    }
    let level = " ".repeat(input.level);
    let tag = input.tags.last().map_or("", |tag| tag.name.as_str());
    let mut text = String::new();
    for part in &input.format.parts {
        match part {
            Part::Text(literal) => text.push_str(literal),
            // `org-compile-prefix-format` gives a form neither `%?` nor
            // the character after the value: what it returns, formatted.
            Part::Field { field: Field::Eval, left, width, precision, form, .. } => {
                let value = match (form, &input.evaluator.0) {
                    (Some(form), Some(evaluate)) => evaluate(&Eval {
                        form: form.clone(),
                        entry: input.entry.map(|(file, offset)| (file.to_string(), offset)),
                        category: category.clone(),
                        extra: input.extra.to_string(),
                        time: time.clone(),
                        level: level.clone(),
                        tag: tag.to_string(),
                        effort: input.effort.map(str::to_string),
                        breadcrumbs: input.format.has_breadcrumbs.then(|| input.breadcrumbs.to_string()),
                    }),
                    _ => String::new(),
                };
                text.push_str(&format_string(&value, *left, *width, *precision));
            }
            Part::Field { field, optional, left, width, precision, suffix, .. } => {
                let value = match field {
                    Field::Category => category.as_str(),
                    Field::Time => time.as_str(),
                    Field::Level => level.as_str(),
                    Field::Extra => input.extra,
                    Field::Icon | Field::Eval => "",
                    Field::Tag => tag,
                    Field::Effort => input.effort.unwrap_or(""),
                    Field::Breadcrumbs => input.breadcrumbs,
                };
                if value.is_empty() && *optional {
                    continue;
                }
                let value = if value.is_empty() {
                    String::new()
                } else {
                    format!("{value}{}", suffix.map(String::from).unwrap_or_default())
                };
                text.push_str(&format_string(&value, *left, *width, *precision));
            }
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
    format: &str,
) -> Option<usize> {
    let rest = &text[heading..];
    let dots = rest.bytes().take_while(|b| *b == b'.').count();
    let after_dots = &rest[dots..];
    let keyword =
        keywords.find(|keyword| after_dots.strip_prefix(keyword).is_some_and(|after| after.starts_with(' ')))?;
    let keyword_at = heading + dots;
    let spaces_at = keyword_at + keyword.len();
    let spaces = text[spaces_at..].bytes().take_while(|b| *b == b' ').count();
    // `org-agenda-todo-keyword-format`, then one blank; nothing for an
    // empty format.
    if format.is_empty() {
        text.replace_range(keyword_at..spaces_at + spaces, "");
        return Some(0);
    }
    let formatted = format!("{} ", format_with(format, keyword));
    text.replace_range(keyword_at..spaces_at + spaces, &formatted);
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

    fn formatted(format: &str, extra: &str, head: &str, tags: &[Tag], dotime: Dotime<'_>) -> String {
        format_item(&FormatInput {
            format: &PrefixFormat::new(format),
            extra,
            head,
            category: "work",
            tags,
            dotime,
            remove_stamps: false,
            time_trailer: "......",
            level: 2,
            effort: Some("1:30"),
            breadcrumbs: "Top->Sub->",
            remove_tags: RemoveTags::No,
            entry: None,
            evaluator: &Evaluator::default(),
        })
        .text
    }

    #[test]
    fn evaluates_forms_as_org_compile_prefix_format_does() {
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let record = seen.clone();
        let evaluator = Evaluator(Some(std::sync::Arc::new(move |eval: &Eval| {
            record.lock().unwrap().push(eval.clone());
            if eval.form == "(a)" {
                "x".to_string()
            } else {
                String::new()
            }
        })));
        let text = format_item(&FormatInput {
            // A string or a character holding a parenthesis does not end
            // the form; `%?` and the character after a value are not for
            // forms.
            format: &PrefixFormat::new("%-4(a)|%?-3(b \")\" ?\\))|%:(a)|%c "),
            extra: "Scheduled: ",
            head: "Task",
            category: "work",
            tags: &[tag("z", false)],
            dotime: Dotime::No,
            remove_stamps: false,
            time_trailer: "......",
            level: 2,
            effort: Some("1:30"),
            breadcrumbs: "Top->",
            remove_tags: RemoveTags::No,
            entry: Some(("/org/a.org", 42)),
            evaluator: &evaluator,
        })
        .text;
        assert_eq!(text, "x   |   |x|work Task                                          :z:");
        let seen = seen.lock().unwrap();
        assert_eq!(seen.iter().map(|eval| eval.form.as_str()).collect::<Vec<_>>(), ["(a)", "(b \")\" ?\\))", "(a)"]);
        assert_eq!(
            seen[0],
            Eval {
                form: "(a)".to_string(),
                entry: Some(("/org/a.org".to_string(), 42)),
                category: "work".to_string(),
                extra: "Scheduled: ".to_string(),
                time: String::new(),
                level: "  ".to_string(),
                tag: "z".to_string(),
                effort: Some("1:30".to_string()),
                breadcrumbs: None,
            }
        );
        // Without an evaluator, a form is empty, its width kept.
        assert_eq!(formatted("%-4(a)|%c", "", "Task", &[], Dotime::No), "    |workTask");
    }

    fn agenda(extra: &str, head: &str, tags: &[Tag], dotime: Dotime<'_>) -> String {
        formatted(" %i %-12:c%?-12t% s", extra, head, tags, dotime)
    }

    #[test]
    fn prefix_formats() {
        let plain =
            |format: &str| formatted(format, "Scheduled: ", "Task", &[tag("a", false), tag("z", false)], Dotime::No);
        assert_eq!(plain(" %i %-12:c"), "  work:       Task                                        :a:z:");
        assert_eq!(plain("%l%c|%e|%T|%b"), "  work|1:30|z|Top->Sub->Task                                        :a:z:");
        assert_eq!(plain("%?-12t%s"), "Scheduled: Task                                        :a:z:");
        assert_eq!(plain("%-6.3:c|"), "wo:   |Task                                        :a:z:");
        assert_eq!(
            plain("%8c %(org-entry-get nil \"X\") 100%%"),
            "    work  100%Task                                        :a:z:"
        );
        assert_eq!(plain("%x %c"), "%x workTask                                        :a:z:");
    }

    #[test]
    fn formats() {
        assert_eq!(format_with("%-4s", "OK"), "OK  ");
        assert_eq!(format_with("[%s]", "TODO"), "[TODO]");
        assert_eq!(format_numbers("Sched.%2dx: ", &[3]), "Sched. 3x: ");
        assert_eq!(format_numbers("In %3d d.: ", &[12]), "In  12 d.: ");
        assert_eq!(format_numbers("(%d/%d): ", &[2, 5]), "(2/5): ");
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
            "  work:       14:00-15:30 Meeting                                       :m:"
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
        assert_eq!(normalise_keyword(&mut text, 14, keywords.into_iter(), "%-1s"), Some(4));
        assert_eq!(text, "  work:       TODO Task");
        let mut text = "  work:       TODOX Task".to_string();
        assert_eq!(normalise_keyword(&mut text, 14, keywords.into_iter(), "%-1s"), Some(5));
        let mut text = "  work:       TODO".to_string();
        assert_eq!(normalise_keyword(&mut text, 14, keywords.into_iter(), "%-1s"), None);
        let mut text = "  work:       DONE  Task".to_string();
        assert_eq!(normalise_keyword(&mut text, 14, keywords.into_iter(), "%-6s"), Some(4));
        assert_eq!(text, "  work:       DONE   Task");
        let mut text = "  work:       DONE  Task".to_string();
        assert_eq!(normalise_keyword(&mut text, 14, keywords.into_iter(), ""), Some(0));
        assert_eq!(text, "  work:       Task");
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
