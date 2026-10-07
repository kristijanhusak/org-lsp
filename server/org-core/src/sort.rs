//! `org-sort-entries` (`C-c ^` on a headline): the children of the entry
//! at the cursor, or the top-level entries before the first headline,
//! sorted as `sort-subr` sorts them. Checked against Emacs in
//! `tests/sort`. Text compares by character code, as
//! `string-collate-lessp` does in the C locale.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::LazyLock;

use org_syntax::{SyntaxKind, SyntaxNode, TodoKeywords};
use regex::Regex;

use crate::date::{Date, DateTime};
use crate::newline::Outcome;
use crate::priority::Priorities;
use crate::structure::{level_at, line, line_start, next_heading, next_line, subtree_end};
use crate::Refusal;

/// What sorting needs to know besides the text.
pub struct Settings<'a> {
    pub keywords: &'a TodoKeywords,
    pub priorities: Priorities,
    /// What an entry without the date sorts by (`current-time`).
    pub now: DateTime,
}

/// The sorting types `org-sort-entries` reads, with what its prompt says
/// of each; a capital reverses the order.
pub const TYPES: &[(char, &str)] = &[
    ('a', "alpha"),
    ('n', "numeric"),
    ('p', "priority"),
    ('r', "property"),
    ('o', "todo order"),
    ('t', "time"),
    ('s', "scheduled"),
    ('d', "deadline"),
    ('c', "created"),
    ('k', "clocking"),
];

/// What `org-sort-entries` sorts at `at`: `"children"` or `"top-level"`.
pub fn what(text: &str, at: usize) -> &'static str {
    if heading_at_or_above(text, at).is_some() {
        "children"
    } else {
        "top-level"
    }
}

fn heading_at_or_above(text: &str, at: usize) -> Option<usize> {
    let mut start = line_start(text, at.min(text.len()));
    loop {
        if level_at(text, start).is_some() {
            return Some(start);
        }
        if start == 0 {
            return None;
        }
        start = line_start(text, start - 1);
    }
}

#[derive(Debug, Clone)]
enum Key {
    Number(f64),
    Text(String),
}

fn compare(a: &Key, b: &Key) -> Ordering {
    match (a, b) {
        (Key::Number(a), Key::Number(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
        (Key::Text(a), Key::Text(b)) => a.cmp(b),
        (Key::Number(_), Key::Text(_)) => Ordering::Less,
        (Key::Text(_), Key::Number(_)) => Ordering::Greater,
    }
}

/// `org-sort-entries` with the sorting type `kind` (`property` for `r`
/// and `R`), the cursor at `at`. The cursor goes to the start of what
/// was sorted: the parent headline, or the first top-level one.
pub fn sort_entries(
    text: &str,
    at: usize,
    kind: char,
    property: Option<&str>,
    settings: &Settings,
) -> Result<Outcome, Refusal> {
    let lower = kind.to_ascii_lowercase();
    if lower == 'f' {
        return Err(Refusal::Failed("Sorting by a function of your own is not supported".to_string()));
    }
    if !TYPES.iter().any(|(known, _)| *known == lower) {
        return Err(Refusal::Failed(format!("Invalid sorting type ‘{kind}’")));
    }
    let mut text = text.to_string();
    let (start, end, beg) = match heading_at_or_above(&text, at) {
        Some(start) => {
            // `org-end-of-subtree`, a newline at its end, and one of the
            // blank lines after it.
            let mut end = subtree_end(&text, start);
            if end == text.len() && !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
                end = text.len();
            }
            let last = text[..end].trim_end_matches([' ', '\t', '\n', '\r']).len();
            let first_blank = next_line(&text, last).min(end);
            let blanks = text[first_blank..end].matches('\n').count();
            if blanks >= 1 {
                end = next_line(&text, first_blank);
            }
            (start, end, next_heading(&text, start))
        }
        None => {
            let start = if level_at(&text, 0).is_some() { 0 } else { next_heading(&text, 0) };
            let last = line_start(&text, text.len());
            if line(&text, last).chars().any(|c| !c.is_whitespace()) {
                text.push('\n');
            }
            (start, text.len(), start)
        }
    };
    if beg >= end {
        return Err(Refusal::Failed("Nothing to sort".to_string()));
    }
    let stars = level_at(&text, beg).unwrap_or(1);
    if stars > 1 {
        let mut region = text[beg..end].to_string();
        if !region.ends_with('\n') {
            region.push('\n');
        }
        let above = region.split_inclusive('\n').any(|line| {
            line.len() > stars - 1
                && line.as_bytes()[..stars - 1].iter().all(|b| *b == b'*')
                && matches!(line.as_bytes()[stars - 1], b' ' | b'\t' | b'\n')
        });
        if above {
            return Err(Refusal::Failed("Region to sort contains a level above the first entry".to_string()));
        }
    }
    let clocked: HashMap<usize, i64> =
        if lower == 'k' { crate::clock::sums(&text, None, None).0.into_iter().collect() } else { HashMap::new() };
    // The records: from a headline of the first one's level to the next
    // of that level, or to the end when one above comes first.
    let mut records: Vec<(Key, usize, usize)> = Vec::new();
    let mut point = beg;
    while point < end {
        let key = key(&text, point, lower, property, settings, &clocked);
        let mut record_end = next_heading(&text, point).min(end);
        while record_end < end {
            match level_at(&text, record_end) {
                Some(level) if level > stars => record_end = next_heading(&text, record_end).min(end),
                Some(level) if level == stars => break,
                _ => {
                    record_end = end;
                    break;
                }
            }
        }
        records.push((key, point, record_end));
        // The next headline of the level, from the end of this record.
        let mut next = record_end;
        while next < end && level_at(&text, next) != Some(stars) {
            next = next_line(&text, next);
        }
        point = next.min(end);
    }
    // `sort-subr`: a stable sort, of the records in reverse order for a
    // reversed sort, whose result is reversed again.
    let reverse = lower != kind;
    let mut sorted = records.clone();
    if reverse {
        sorted.reverse();
    }
    sorted.sort_by(|a, b| compare(&a.0, &b.0));
    if reverse {
        sorted.reverse();
    }
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..beg]);
    for (_, from, to) in &sorted {
        out.push_str(&text[*from..*to]);
    }
    out.push_str(&text[records.last().map_or(end, |record| record.2)..]);
    Ok(Outcome { text: out, cursor: start })
}

/// `org-complex-heading-regexp` with the keywords of the file.
fn heading_regex(keywords: &TodoKeywords) -> Regex {
    let names: Vec<String> = keywords.names().map(regex::escape).collect();
    let todo = if names.is_empty() { String::from("[^\\s\\S]") } else { names.join("|") };
    Regex::new(&format!(
        r"^(\*+)(?: +({todo}))?(?: +(\[#(?:[A-Z]|[0-9]|[1-5][0-9]|6[0-4])\]))?(?: +(.*?))??(?:[ \t]+(:[\w@#%:]+:))?[ \t]*$"
    ))
    .expect("the heading regexp is valid")
}

/// The sort key of the record whose headline starts at `heading`.
fn key(
    text: &str,
    heading: usize,
    kind: char,
    property: Option<&str>,
    settings: &Settings,
    clocked: &HashMap<usize, i64>,
) -> Key {
    let entry_end = next_heading(text, heading);
    let entry = &text[heading..entry_end];
    let headline = line(text, heading);
    let now = Key::Number(seconds(settings.now));
    let time = |found: Option<&str>| found.and_then(parse_time).map_or(now.clone(), |time| Key::Number(seconds(time)));
    match kind {
        'n' => Key::Number(string_to_number(&visible(&title(headline, settings.keywords)))),
        'a' => Key::Text(visible(&title(headline, settings.keywords)).to_lowercase()),
        'k' => Key::Number(clocked.get(&heading).copied().unwrap_or(0) as f64),
        't' => {
            static ACTIVE: LazyLock<Regex> =
                LazyLock::new(|| Regex::new(r"<(\d{4}-\d{2}-\d{2}(?: [^\n]*?)?)>").unwrap());
            static BOTH: LazyLock<Regex> =
                LazyLock::new(|| Regex::new(r"[\[<](\d{4}-\d{2}-\d{2}(?: [^\n]*?)?)[\]>]").unwrap());
            let found = ACTIVE.find(entry).or_else(|| BOTH.find(entry));
            time(found.map(|found| found.as_str()))
        }
        'c' => {
            static CREATED: LazyLock<Regex> = LazyLock::new(|| {
                Regex::new(r"(?m)^[ \t]*\[(\d{4})-(\d{2})-(\d{2})(?: *[^\]+0-9>\r\n -]+)?(?: \d{1,2}:\d{2})?\]")
                    .unwrap()
            });
            time(CREATED.find(entry).map(|found| found.as_str()))
        }
        's' | 'd' => {
            static SCHEDULED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bSCHEDULED: *<([^>]+)>").unwrap());
            static DEADLINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bDEADLINE: *<([^>]+)>").unwrap());
            let regex = if kind == 's' { &SCHEDULED } else { &DEADLINE };
            time(regex.captures(entry).and_then(|found| found.get(1)).map(|found| found.as_str()))
        }
        'p' => {
            static PRIORITY: LazyLock<Regex> =
                LazyLock::new(|| Regex::new(r"\[#([A-Z]|[0-9]|[1-5][0-9]|6[0-4])\]").unwrap());
            match PRIORITY.captures(headline).and_then(|found| found.get(1)) {
                Some(value) => Key::Number(priority_value(value.as_str()) as f64),
                None => Key::Number(settings.priorities.default as f64),
            }
        }
        'r' => Key::Text(property.map(|name| entry_property(entry, name)).unwrap_or_default()),
        'o' => {
            let keyword = heading_regex(settings.keywords)
                .captures(headline)
                .and_then(|found| found.get(2))
                .map(|found| found.as_str().to_string());
            let names: Vec<&str> = settings.keywords.names().collect();
            let value = match keyword {
                None => 99,
                Some(keyword) => {
                    let tail = names.iter().position(|name| *name == keyword).map_or(0, |at| names.len() - at) as i64;
                    if settings.keywords.is_done(&keyword) {
                        99 + tail
                    } else {
                        99 - tail
                    }
                }
            };
            Key::Number(value as f64)
        }
        _ => Key::Number(0.0),
    }
}

/// `org-get-heading` without the tags, the TODO keyword, the priority
/// and `COMMENT`.
fn title(headline: &str, keywords: &TodoKeywords) -> String {
    let Some(found) = heading_regex(keywords).captures(headline) else { return String::new() };
    let text = found.get(4).map_or("", |found| found.as_str());
    static COMMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^COMMENT[ \t]+").unwrap());
    COMMENT.replace(text, "").into_owned()
}

/// `org-sort-remove-invisible`: the text without emphasis markers, links
/// as their descriptions or, without one, their targets.
fn visible(title: &str) -> String {
    let mut out = String::new();
    write_visible(&org_syntax::parse_objects(title), &mut out);
    out
}

fn write_visible(node: &SyntaxNode, out: &mut String) {
    for child in node.children_with_tokens() {
        if let Some(token) = child.as_token() {
            if token.kind() != SyntaxKind::MARKER {
                out.push_str(token.text());
            }
            continue;
        }
        let Some(child) = child.as_node() else { continue };
        match child.kind() {
            SyntaxKind::LINK => {
                let mut description = String::new();
                let mut in_description = false;
                let mut path = String::new();
                for part in child.children_with_tokens() {
                    match part.kind() {
                        SyntaxKind::LINK_SEPARATOR => in_description = true,
                        SyntaxKind::LINK_OPEN | SyntaxKind::LINK_CLOSE => {}
                        SyntaxKind::LINK_PATH => path.push_str(&part.to_string()),
                        _ if in_description => match part.as_node() {
                            Some(node) => write_visible(node, &mut description),
                            None => description.push_str(&part.to_string()),
                        },
                        _ => {}
                    }
                }
                out.push_str(if in_description { &description } else { &path });
            }
            SyntaxKind::ANGLE_LINK => {
                let text = child.to_string();
                out.push_str(text.strip_prefix('<').and_then(|text| text.strip_suffix('>')).unwrap_or(&text));
            }
            _ => write_visible(child, out),
        }
    }
}

/// `string-to-number`: the number at the start of `text`, after blanks,
/// or 0.
fn string_to_number(text: &str) -> f64 {
    static NUMBER: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^[ \t\n]*([-+]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][-+]?[0-9]+)?)").unwrap());
    NUMBER
        .captures(text)
        .and_then(|found| found.get(1))
        .and_then(|number| number.as_str().trim_end_matches('.').parse().ok())
        .unwrap_or(0.0)
}

/// `org-priority-to-value`
fn priority_value(value: &str) -> i64 {
    match value.parse::<i64>() {
        Ok(number) => number,
        Err(_) => value.chars().next().map_or(0, |c| c as i64),
    }
}

/// `org-parse-time-string` of the first timestamp in `text`: its date,
/// and its time or midnight.
fn parse_time(text: &str) -> Option<DateTime> {
    static TIME: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(\d{4})-(\d{2})-(\d{2})(?: +[^\]+0-9>\r\n -]+)?(?: +(\d{1,2}):(\d{2}))?").unwrap()
    });
    let found = TIME.captures(text)?;
    let number = |index: usize| found.get(index).map_or(0, |part| part.as_str().parse::<i64>().unwrap_or(0));
    Some(DateTime::new(Date::new(number(1), number(2), number(3)), number(4), number(5)))
}

fn seconds(time: DateTime) -> f64 {
    ((time.day * 1440 + time.minute) * 60) as f64
}

/// `org-entry-get` of a property in the property drawer of `entry`,
/// which comes right after the headline or its planning line.
fn entry_property(entry: &str, name: &str) -> String {
    let mut lines = entry.lines().skip(1).peekable();
    let planning = |line: &str| {
        let line = line.trim_start();
        ["SCHEDULED:", "DEADLINE:", "CLOSED:"].iter().any(|word| line.starts_with(word))
    };
    if lines.peek().is_some_and(|line| planning(line)) {
        lines.next();
    }
    if !lines.next().is_some_and(|line| line.trim().eq_ignore_ascii_case(":PROPERTIES:")) {
        return String::new();
    }
    for line in lines {
        let trimmed = line.trim();
        if trimmed.eq_ignore_ascii_case(":END:") {
            break;
        }
        if let Some((key, value)) = trimmed.strip_prefix(':').and_then(|rest| rest.split_once(':')) {
            if key.eq_ignore_ascii_case(name) {
                return value.trim().to_string();
            }
        }
    }
    String::new()
}
