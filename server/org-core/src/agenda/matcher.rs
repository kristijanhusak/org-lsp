//! `org-make-tags-matcher`: the match strings of tags views and custom
//! agenda commands.
//!
//! `work+urgent-home|@phone`, `PRIORITY="A"`, `LEVEL>2`,
//! `OWNER={^al}`, `SCHEDULED<"<today>"`, and a TODO part after the last
//! slash: `work/NEXT|WAIT`, `/!` for open TODO entries only.

use regex::{Regex, RegexBuilder};

use crate::date::Day;
use crate::outline::{Entry, Outline};
use crate::timestr::parse_time_string;

/// A compiled match string.
#[derive(Debug, Clone)]
pub struct Matcher {
    /// The match string as given.
    pub text: String,
    /// Alternatives of the tags part, each a list of terms that must all
    /// hold. Empty: every entry.
    tags: Vec<Vec<Term>>,
    /// Alternatives of the TODO part.
    todo: Vec<Vec<Term>>,
    /// `/!`: only entries with an open TODO keyword.
    pub todo_only: bool,
}

#[derive(Debug, Clone)]
struct Term {
    negated: bool,
    test: Test,
}

#[derive(Debug, Clone)]
enum Test {
    /// A tag, exactly.
    Tag(String),
    /// Some tag matches the regexp.
    TagRegex(Regex),
    /// The TODO keyword, exactly.
    Keyword(String),
    /// The TODO keyword matches the regexp.
    KeywordRegex(Regex),
    Property {
        name: String,
        op: Op,
        value: Value,
        present: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
    Equal,
    NotEqual,
}

#[derive(Debug, Clone)]
enum Value {
    Number(f64),
    Text(String),
    Regex(Regex),
    /// Minutes, from a time such as `"<today>"` or `"<2026-10-05>"`.
    Time(TimeValue),
}

#[derive(Debug, Clone)]
enum TimeValue {
    Now,
    Today(i64),
    Hours(i64),
    Days(i64),
    At(Option<i64>),
}

/// What the matcher looks at beyond the entry: the moment it runs.
#[derive(Debug, Clone, Copy)]
pub struct Clock {
    pub today: Day,
    /// Minutes since midnight.
    pub minute: i64,
}

impl Matcher {
    /// Compile `text`. An empty string matches every entry.
    pub fn new(text: &str) -> Matcher {
        let mut matcher = Matcher { text: text.to_string(), tags: Vec::new(), todo: Vec::new(), todo_only: false };
        // The TODO part follows the last run of slashes, unless a quote
        // comes after it.
        let mut tags_part = text;
        if let Some(slash) = last_slashes(text) {
            let (start, end) = slash;
            if !text[start..].contains('"') {
                tags_part = &text[..start];
                let mut todo_part = &text[end..];
                if let Some(rest) = todo_part.strip_prefix('!') {
                    matcher.todo_only = true;
                    todo_part = rest;
                }
                if !todo_part.trim().is_empty() {
                    matcher.todo = split_or(todo_part).iter().map(|term| terms(term, true)).collect();
                }
            }
        }
        if tags_part.chars().any(|c| !c.is_whitespace()) {
            matcher.tags = split_or(tags_part).iter().map(|term| terms(term, false)).collect();
        }
        matcher
    }

    /// Whether the entry `index` of `outline` matches, the way
    /// `org-scan-tags` calls the matcher; `todo_only` as for
    /// `tags-todo`.
    pub fn matches(&self, outline: &Outline, index: usize, todo_only: bool, clock: Clock) -> bool {
        let entry = &outline.entries[index];
        let todo = entry.todo.as_deref();
        if (todo_only || self.todo_only) && (todo.is_none() || entry.done) {
            return false;
        }
        let holds = |term: &Term| term.negated != term.test.holds(index, outline, clock);
        let any = |alternatives: &[Vec<Term>]| {
            alternatives.is_empty() || alternatives.iter().any(|terms| terms.iter().all(holds))
        };
        any(&self.tags) && any(&self.todo)
    }
}

/// Where the last run of slashes is.
fn last_slashes(text: &str) -> Option<(usize, usize)> {
    let start = text.rfind('/')?;
    let start = text[..start].trim_end_matches('/').len();
    let end = start + text[start..].bytes().take_while(|b| *b == b'/').count();
    Some((start, end))
}

/// `org-split-string` on `|`, joining parts that a backslash ended.
fn split_or(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for part in text.split('|') {
        match out.last_mut() {
            Some(last) if last.ends_with('\\') => {
                last.push('|');
                last.push_str(part);
            }
            _ => out.push(part.to_string()),
        }
    }
    out.retain(|part| !part.is_empty());
    out
}

fn is_tag_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '@' | '#' | '%')
}

/// The terms of one alternative, read until something is not a term.
fn terms(mut text: &str, todo: bool) -> Vec<Term> {
    let mut out = Vec::new();
    loop {
        text = text.strip_prefix('&').unwrap_or(text);
        let mut negated = false;
        if let Some(sign) = text.chars().next().filter(|c| matches!(c, '-' | '+' | ':')) {
            negated = sign == '-';
            text = &text[1..];
        }
        let Some((test, rest)) = term(text, todo) else { break };
        out.push(Term { negated, test });
        text = rest;
    }
    out
}

fn term(text: &str, todo: bool) -> Option<(Test, &str)> {
    if let Some(inner) = text.strip_prefix('{') {
        let end = inner.find('}').filter(|end| *end > 0)?;
        let test = if todo {
            Test::KeywordRegex(emacs_regex(&inner[..end])?)
        } else {
            Test::TagRegex(tag_regex(&inner[..end])?)
        };
        return Some((test, &inner[end + 1..]));
    }
    if !todo {
        if let Some(found) = property(text) {
            return Some(found);
        }
    }
    let end = text.find(|c: char| !is_tag_char(c)).unwrap_or(text.len());
    if end == 0 {
        return None;
    }
    let name = text[..end].to_string();
    Some((if todo { Test::Keyword(name) } else { Test::Tag(name) }, &text[end..]))
}

/// `NAME OP [*] VALUE`: a property comparison.
fn property(text: &str) -> Option<(Test, &str)> {
    // The name: word characters, or any character after a backslash.
    let mut name = String::new();
    let mut chars = text.char_indices().peekable();
    let mut end = 0;
    while let Some(&(at, c)) = chars.peek() {
        if c == '\\' {
            chars.next();
            let (_, quoted) = chars.next().filter(|(_, c)| !c.is_whitespace())?;
            name.push(quoted);
            end = at + 1 + quoted.len_utf8();
        } else if c.is_alphanumeric() || c == '_' {
            name.push(c);
            chars.next();
            end = at + c.len_utf8();
        } else {
            break;
        }
    }
    if name.is_empty() {
        return None;
    }
    let rest = &text[end..];
    let (op, length) = [
        ("<=", Op::LessEqual),
        ("=<", Op::LessEqual),
        (">=", Op::GreaterEqual),
        ("=>", Op::GreaterEqual),
        ("<>", Op::NotEqual),
        ("!=", Op::NotEqual),
        ("/=", Op::NotEqual),
        ("==", Op::Equal),
        ("<", Op::Less),
        (">", Op::Greater),
        ("=", Op::Equal),
    ]
    .iter()
    .find(|(spelled, _)| rest.starts_with(spelled))
    .map(|(spelled, op)| (*op, spelled.len()))?;
    let mut rest = &rest[length..];
    let present = rest.starts_with('*');
    if present {
        rest = &rest[1..];
    }
    let (value, rest) = operand(rest)?;
    Some((Test::Property { name: name.to_uppercase(), op, value, present }, rest))
}

fn operand(text: &str) -> Option<(Value, &str)> {
    if let Some(inner) = text.strip_prefix('{') {
        let end = inner.find('}').filter(|end| *end > 0)?;
        return Some((Value::Regex(emacs_regex(&inner[..end])?), &inner[end + 1..]));
    }
    if let Some(inner) = text.strip_prefix('"') {
        let end = inner.find('"')?;
        let value = &inner[..end];
        let rest = &inner[end + 1..];
        let time = time_operand(value);
        return Some((time.map_or_else(|| Value::Text(value.to_string()), Value::Time), rest));
    }
    // -?[.0-9]+([eE][-+]?[0-9]+)?
    let bytes = text.as_bytes();
    let mut end = usize::from(bytes.first() == Some(&b'-'));
    let digits = bytes[end..].iter().take_while(|b| b.is_ascii_digit() || **b == b'.').count();
    if digits == 0 {
        return None;
    }
    end += digits;
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(bytes.get(end + 1), Some(b'+' | b'-')));
        let exponent = bytes[end + 1 + sign..].iter().take_while(|b| b.is_ascii_digit()).count();
        if exponent > 0 {
            end += 1 + sign + exponent;
        }
    }
    Some((Value::Number(string_to_number(&text[..end])), &text[end..]))
}

/// `"<today>"`, `"<+2d>"`, `"<2026-10-05 Mon>"`: a time to compare with.
fn time_operand(value: &str) -> Option<TimeValue> {
    let looks_like_time =
        (value.starts_with('<') && value.ends_with('>')) || (value.starts_with('[') && value.ends_with(']'));
    let inner = value.get(1..value.len().checked_sub(1)?)?;
    let first = inner.chars().next()?;
    if !looks_like_time
        || !(first.is_ascii_digit()
            || first == '+'
            || first == '-'
            || inner.starts_with("now")
            || inner.starts_with("today")
            || inner.starts_with("tomorrow"))
    {
        return None;
    }
    Some(match value {
        "<now>" => TimeValue::Now,
        "<today>" => TimeValue::Today(0),
        "<tomorrow>" => TimeValue::Today(1),
        "<yesterday>" => TimeValue::Today(-1),
        _ => {
            let relative = inner.strip_prefix(['+', '-']).and_then(|rest| {
                let unit = rest.chars().last()?;
                let number: i64 = rest[..rest.len() - unit.len_utf8()].parse().ok()?;
                let number = if inner.starts_with('-') { -number } else { number };
                match unit {
                    'h' => Some(TimeValue::Hours(number)),
                    'd' => Some(TimeValue::Days(number)),
                    'w' => Some(TimeValue::Days(number.saturating_mul(7))),
                    'm' => Some(TimeValue::Days(number.saturating_mul(30))),
                    'y' => Some(TimeValue::Days(number.saturating_mul(365))),
                    _ => None,
                }
            });
            relative.unwrap_or_else(|| TimeValue::At(stamp_minutes(inner)))
        }
    })
}

/// The minute a timestamp's text stands for, counted from day 0.
fn stamp_minutes(text: &str) -> Option<i64> {
    let parsed = parse_time_string(text)?;
    let (hour, minute) = parsed.time.unwrap_or((0, 0));
    Some(parsed.date.to_day() * 1440 + hour * 60 + minute)
}

impl TimeValue {
    fn minutes(&self, clock: Clock) -> f64 {
        let today = clock.today * 1440;
        match self {
            TimeValue::Now => (today + clock.minute) as f64,
            TimeValue::Today(days) | TimeValue::Days(days) => today.saturating_add(days.saturating_mul(1440)) as f64,
            TimeValue::Hours(hours) => (today + clock.minute).saturating_add(hours.saturating_mul(60)) as f64,
            // `org-matcher-time` gives 0 for what it cannot read.
            TimeValue::At(minutes) => minutes.unwrap_or(0) as f64,
        }
    }
}

/// `string-to-number`: the number at the start of `text`, or 0.
fn string_to_number(text: &str) -> f64 {
    let text = text.trim_start();
    let mut end = 0;
    let bytes = text.as_bytes();
    if matches!(bytes.first(), Some(b'-' | b'+')) {
        end = 1;
    }
    end += bytes[end..].iter().take_while(|b| b.is_ascii_digit() || **b == b'.').count();
    let mut number = &text[..end];
    // A second dot ends the number.
    if let Some(first) = number.find('.') {
        if let Some(second) = number[first + 1..].find('.') {
            number = &number[..first + 1 + second];
        }
    }
    number.parse().unwrap_or(0.0)
}

impl Test {
    fn holds(&self, index: usize, outline: &Outline, clock: Clock) -> bool {
        let entry = &outline.entries[index];
        match self {
            Test::Tag(tag) => entry.tags.iter().any(|candidate| candidate.name == *tag),
            Test::TagRegex(regex) => entry.tags.iter().any(|tag| regex.is_match(&tag.name)),
            Test::Keyword(keyword) => entry.todo.as_deref() == Some(keyword.as_str()),
            Test::KeywordRegex(regex) => regex.is_match(entry.todo.as_deref().unwrap_or("")),
            Test::Property { name, op, value, present } => {
                let got = property_value(index, outline, name);
                if *present && got.is_none() {
                    return false;
                }
                let got = got.unwrap_or_default();
                match value {
                    Value::Regex(regex) => (*op == Op::NotEqual) != regex.is_match(&got),
                    Value::Text(text) => compare_text(&got, *op, text),
                    Value::Number(number) => compare(string_to_number(&got), *op, *number),
                    Value::Time(time) => {
                        // `org-2ft`: what is not a timestamp is 0, and
                        // `org-time<` and the others hold only when both
                        // sides are times. Org's `org-time<>` is written
                        // `(\= a b)`, which is `=`: the same here.
                        let got = stamp_minutes(got.trim_matches(['<', '>', '[', ']'])).unwrap_or(0) as f64;
                        let wanted = time.minutes(clock);
                        let op = if *op == Op::NotEqual { Op::Equal } else { *op };
                        got > 0.0 && wanted > 0.0 && compare(got, op, wanted)
                    }
                }
            }
        }
    }
}

fn compare(a: f64, op: Op, b: f64) -> bool {
    match op {
        Op::Less => a < b,
        Op::Greater => a > b,
        Op::LessEqual => a <= b,
        Op::GreaterEqual => a >= b,
        Op::Equal => a == b,
        Op::NotEqual => a != b,
    }
}

fn compare_text(a: &str, op: Op, b: &str) -> bool {
    match op {
        Op::Less => a < b,
        Op::Greater => a > b,
        Op::LessEqual => a <= b,
        Op::GreaterEqual => a >= b,
        Op::Equal => a == b,
        Op::NotEqual => a != b,
    }
}

/// `org-entry-get` for the matcher: the entry's own property, or one of
/// Org's special properties.
fn property_value(index: usize, outline: &Outline, name: &str) -> Option<String> {
    let entry = &outline.entries[index];
    let stamp = |planning: &Option<crate::outline::Planning>| {
        planning.as_ref().map(|planning| {
            let (open, close) = if planning.active { ('<', '>') } else { ('[', ']') };
            format!("{open}{}{close}", planning.raw)
        })
    };
    let tag_string = |tags: Vec<&str>| (!tags.is_empty()).then(|| format!(":{}:", tags.join(":")));
    match name {
        "LEVEL" => Some(entry.level.to_string()),
        "CATEGORY" => Some(entry.category.clone()),
        "TODO" => entry.todo.clone(),
        "PRIORITY" => {
            let default = outline.priorities.unwrap_or_default().default;
            let value = cookie(&entry.head).unwrap_or(default);
            Some(char::from_u32(value as u32).map(String::from).unwrap_or_else(|| value.to_string()))
        }
        "SCHEDULED" => stamp(&entry.scheduled),
        "DEADLINE" => stamp(&entry.deadline),
        "CLOSED" => stamp(&entry.closed),
        "TIMESTAMP" => crate::clocktable::entry_stamp(&outline.text, outline, index, true),
        "TIMESTAMP_IA" => crate::clocktable::entry_stamp(&outline.text, outline, index, false),
        "TAGS" => tag_string(entry.tags.iter().filter(|tag| !tag.inherited).map(|tag| tag.name.as_str()).collect()),
        "ALLTAGS" => tag_string(entry.tags.iter().map(|tag| tag.name.as_str()).collect()),
        "ITEM" => Some(item_title(entry)),
        _ => outline.property(index, name),
    }
}

/// The priority cookie right after the keyword, as a value.
fn cookie(head: &str) -> Option<i64> {
    let rest = head.find("[#").map(|at| &head[at + 2..])?;
    let end = rest.find(']')?;
    crate::priority::value(&rest[..end])
}

/// The headline without its keyword, priority and tags.
fn item_title(entry: &Entry) -> String {
    let mut text = entry.head.as_str();
    if let Some(todo) = entry.todo.as_deref() {
        text = text.strip_prefix(todo).unwrap_or(text).trim_start();
    }
    if text.starts_with("[#") {
        if let Some(end) = text.find(']') {
            text = text[end + 1..].trim_start();
        }
    }
    let text = text.trim_end();
    match text.rsplit_once([' ', '\t']) {
        Some((title, tags)) if tags.len() > 1 && tags.starts_with(':') && tags.ends_with(':') => {
            title.trim_end().to_string()
        }
        _ => text.to_string(),
    }
}

/// `org-tags-expand`: each group tag of `text` (`Work`, `+Work`, `-Work`)
/// in place of a regexp of the tags it stands for, its own tags too
/// (`{\<\(?:Work\|Lab\)\>}`); regexps (`{...}`) are left alone, and
/// the names compare without regard to case.
pub fn expand_groups(text: &str, groups: &[(String, Vec<String>)]) -> String {
    if groups.is_empty() || text.trim().is_empty() {
        return text.to_string();
    }
    let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '@';
    // The regexps of the match, left as they are.
    let mut regexps: Vec<(usize, usize)> = Vec::new();
    let mut from = 0;
    while let Some(open) = text[from..].find('{').map(|index| from + index) {
        let Some(close) =
            text[open + 1..].char_indices().skip(1).find(|(_, c)| *c == '}').map(|(index, _)| open + 1 + index)
        else {
            break;
        };
        regexps.push((open, close + 1));
        from = close + 1;
    }
    let mut out = String::new();
    let mut at = 0;
    'scan: while at < text.len() {
        let before = text[..at].chars().next_back();
        if before.is_none_or(|c| !is_word(c)) {
            for (key, _) in groups {
                let end = at + key.len();
                let candidate = text.get(at..end);
                if !candidate.is_some_and(|candidate| {
                    candidate.eq_ignore_ascii_case(key) || candidate.to_lowercase() == key.to_lowercase()
                }) {
                    continue;
                }
                if text[end..].chars().next().is_some_and(is_word) {
                    continue;
                }
                if regexps.iter().any(|(start, stop)| *start <= at && at < *stop) {
                    continue;
                }
                let token = &text[at..end];
                let mut expanded: Vec<String> = Vec::new();
                expand_group(&[token.to_string()], groups, &mut expanded);
                let (regexp_tags, regular): (Vec<String>, Vec<String>) =
                    expanded.into_iter().partition(|tag| tag.starts_with('{') && tag.ends_with('}') && tag.len() > 2);
                let regexp_tags: Vec<String> =
                    regexp_tags.iter().map(|tag| tag[1..tag.len() - 1].to_string()).collect();
                let regular_re =
                    format!("\\(?:{}\\)", regular.iter().map(|tag| regex_quote(tag)).collect::<Vec<_>>().join("\\|"));
                let regexp = regexp_tags.join("\\|");
                let replacement = if regular.is_empty() {
                    format!("{{{regexp}}}")
                } else if regexp_tags.is_empty() {
                    format!("{{\\<{regular_re}\\>}}")
                } else {
                    format!("{{\\<{regular_re}\\>\\|{regexp}}}")
                };
                out.push_str(&replacement);
                at = end;
                continue 'scan;
            }
        }
        let c = text[at..].chars().next().expect("a character");
        out.push(c);
        at += c.len_utf8();
    }
    out
}

/// `org--tags-expand-group`: the tags of `group` and, for each that is a
/// group tag itself, its tags, each once.
fn expand_group(group: &[String], groups: &[(String, Vec<String>)], expanded: &mut Vec<String>) {
    for tag in group {
        if expanded.contains(tag) {
            continue;
        }
        expanded.push(tag.clone());
        if let Some((_, members)) = groups.iter().find(|(key, _)| key == tag) {
            expand_group(members, groups, expanded);
        }
    }
}

/// `regexp-quote`
fn regex_quote(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if matches!(c, '[' | '*' | '.' | '\\' | '?' | '+' | '^' | '$') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// An Emacs regexp as a regex of this crate, matching without regard to
/// case as the matcher does (`case-fold-search` is on).
pub fn emacs_regex(pattern: &str) -> Option<Regex> {
    RegexBuilder::new(&translate(pattern)).case_insensitive(true).build().ok()
}

/// [`emacs_regex`] for searching a whole buffer: `^` and `$` match at the
/// start and end of every line, as in Emacs.
pub fn emacs_buffer_regex(pattern: &str) -> Option<Regex> {
    RegexBuilder::new(&translate(pattern)).case_insensitive(true).multi_line(true).build().ok()
}

/// [`emacs_regex`] for matching tags, where `@` and `_` are parts of
/// words, as in Org's tags syntax table: `\<@home\>` matches `@home`.
fn tag_regex(pattern: &str) -> Option<Regex> {
    let translated = translate_words(pattern, r"(?:^|[^\w@])", r"(?:$|[^\w@])");
    RegexBuilder::new(&translated).case_insensitive(true).build().ok()
}

pub(crate) fn translate(pattern: &str) -> String {
    translate_words(pattern, r"\b", r"\b")
}

/// [`translate`] with `\<` and `\>` as `start` and `end`.
fn translate_words(pattern: &str, start: &str, end: &str) -> String {
    let mut out = String::new();
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('(') => {
                    // `\(?:` and `\(?N:` are groups as well.
                    if chars.peek() == Some(&'?') {
                        chars.next();
                        for c in chars.by_ref() {
                            if c == ':' {
                                break;
                            }
                        }
                    }
                    out.push_str("(?:");
                }
                Some(')') => out.push(')'),
                Some('|') => out.push('|'),
                Some('{') => out.push('{'),
                Some('}') => out.push('}'),
                Some('`') => out.push_str(r"\A"),
                Some('\'') => out.push_str(r"\z"),
                Some('<') => out.push_str(start),
                Some('>') => out.push_str(end),
                Some('b') => out.push_str(r"\b"),
                Some('w') => out.push_str(r"\w"),
                Some('W') => out.push_str(r"\W"),
                Some('s') => {
                    let class = chars.next();
                    out.push_str(if class == Some('-') || class == Some(' ') { r"\s" } else { r"\S" });
                }
                Some(other) => out.push_str(&regex::escape(&other.to_string())),
                None => out.push_str(r"\\"),
            },
            '(' | ')' | '|' | '{' | '}' => {
                out.push('\\');
                out.push(c);
            }
            '[' => {
                // A bracket expression: a backslash is itself in Emacs,
                // `[:alpha:]` classes are copied.
                out.push('[');
                if chars.peek() == Some(&'^') {
                    chars.next();
                    out.push('^');
                }
                if chars.peek() == Some(&']') {
                    chars.next();
                    out.push_str(r"\]");
                }
                while let Some(c) = chars.next() {
                    match c {
                        ']' => {
                            out.push(']');
                            break;
                        }
                        '[' if chars.peek() == Some(&':') => {
                            out.push('[');
                            for c in chars.by_ref() {
                                out.push(c);
                                if c == ']' {
                                    break;
                                }
                            }
                        }
                        '\\' | '[' | '&' | '~' => {
                            out.push('\\');
                            out.push(c);
                        }
                        _ => out.push(c),
                    }
                }
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use org_syntax::TodoKeywords;

    use super::*;

    fn outline(text: &str) -> Outline {
        let todo = TodoKeywords::from_document(text).unwrap_or_default();
        Outline::new(&org_syntax::parse(text, &todo), text, "notes")
    }

    const CLOCK: Clock = Clock { today: 20_000, minute: 600 };

    /// The headlines that `text` matches.
    fn matched(file: &str, text: &str) -> Vec<String> {
        let outline = outline(file);
        let matcher = Matcher::new(text);
        (0..outline.entries.len())
            .filter(|index| matcher.matches(&outline, *index, false, CLOCK))
            .map(|index| outline.entries[index].head.clone())
            .collect()
    }

    const FILE: &str = "#+TODO: TODO NEXT | DONE\n\
        * TODO [#A] a :work:\n:PROPERTIES:\n:EFFORT: 2\n:OWNER: alice\n:END:\n\
        ** NEXT b :phone:\n\
        * DONE c :work:home:\n\
        * d :home:\n:PROPERTIES:\n:OWNER: bob\n:EFFORT: 10\n:END:\n";

    #[test]
    fn tags_with_and_or_and_not() {
        assert_eq!(matched(FILE, "work"), ["TODO [#A] a :work:", "NEXT b :phone:", "DONE c :work:home:"]);
        assert_eq!(matched(FILE, "work-home"), ["TODO [#A] a :work:", "NEXT b :phone:"]);
        assert_eq!(matched(FILE, "+work+phone"), ["NEXT b :phone:"]);
        assert_eq!(matched(FILE, "phone|home"), ["NEXT b :phone:", "DONE c :work:home:", "d :home:"]);
        assert_eq!(matched(FILE, "{^ho}"), ["DONE c :work:home:", "d :home:"]);
        assert_eq!(matched(FILE, ""), matched(FILE, "work|home"));
    }

    #[test]
    fn properties_and_special_ones() {
        assert_eq!(matched(FILE, "OWNER=\"alice\""), ["TODO [#A] a :work:"]);
        assert_eq!(matched(FILE, "EFFORT>5"), ["d :home:"]);
        assert_eq!(matched(FILE, "EFFORT<5"), ["TODO [#A] a :work:", "NEXT b :phone:", "DONE c :work:home:"]);
        assert_eq!(matched(FILE, "EFFORT<*5"), ["TODO [#A] a :work:"]);
        assert_eq!(matched(FILE, "OWNER={^b}"), ["d :home:"]);
        assert_eq!(matched(FILE, "LEVEL>1"), ["NEXT b :phone:"]);
        assert_eq!(matched(FILE, "PRIORITY=\"A\""), ["TODO [#A] a :work:"]);
        assert_eq!(matched(FILE, "TODO=\"NEXT\""), ["NEXT b :phone:"]);
        assert_eq!(matched(FILE, "ITEM=\"a\""), ["TODO [#A] a :work:"]);
        assert_eq!(matched(FILE, "home+OWNER<>\"bob\""), ["DONE c :work:home:"]);
    }

    #[test]
    fn the_todo_part() {
        assert_eq!(matched(FILE, "work/NEXT"), ["NEXT b :phone:"]);
        assert_eq!(matched(FILE, "/TODO|DONE"), ["TODO [#A] a :work:", "DONE c :work:home:"]);
        assert_eq!(matched(FILE, "work/!"), ["TODO [#A] a :work:", "NEXT b :phone:"]);
        assert_eq!(matched(FILE, "/-DONE"), ["TODO [#A] a :work:", "NEXT b :phone:", "d :home:"]);
    }

    #[test]
    fn times() {
        let file = "* a\nSCHEDULED: <2026-10-01 Thu>\n* b\nSCHEDULED: <2026-10-09 Fri>\n* c\n";
        let today = crate::date::Date::new(2026, 10, 5).to_day();
        let outline = outline(file);
        let clock = Clock { today, minute: 600 };
        let matched = |text: &str| -> Vec<&str> {
            let matcher = Matcher::new(text);
            (0..3).filter(|index| matcher.matches(&outline, *index, false, clock)).map(|i| ["a", "b", "c"][i]).collect()
        };
        // Without a SCHEDULED, `c` is no time: no comparison holds.
        assert_eq!(matched("SCHEDULED<\"<today>\""), ["a"]);
        assert_eq!(matched("SCHEDULED>=\"<today>\""), ["b"]);
        assert_eq!(matched("SCHEDULED<*\"<+1w>\""), ["a", "b"]);
        assert_eq!(matched("SCHEDULED=\"<2026-10-09>\""), ["b"]);
        // Org's `org-time<>` is `=`.
        assert_eq!(matched("SCHEDULED<>\"<2026-10-09>\""), ["b"]);
    }

    #[test]
    fn emacs_regexps() {
        assert!(emacs_regex(r"^\(?:wo\|ho\)").unwrap().is_match("Home"));
        assert!(emacs_regex(r"a(b)").unwrap().is_match("a(b)"));
        assert!(emacs_regex(r"[[:alpha:]]x").unwrap().is_match("ax"));
    }

    #[test]
    fn reads_a_relative_time_with_a_wide_unit() {
        // The unit of `<+5日>` is not one Org knows, and three bytes long.
        let _ = Matcher::new("SCHEDULED<\"<+5日>\"");
        let _ = Matcher::new("SCHEDULED<\"<+999999999999999999y>\"");
    }
}
