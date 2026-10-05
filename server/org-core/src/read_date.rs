//! Reading the answer to Org's date prompt: a port of `org-read-date`.
//!
//! The answer can be almost anything: `+2d`, `fri`, `2/15`, `feb 15`,
//! `3pm`, `w12`, `10:00-11:30`, `2026-05-20`. [`read_date`] ports
//! `org-read-date-analyze`, which rewrites the answer step by step with
//! regular expressions and then hands the rest to Emacs's
//! `parse-time-string` ([`parse_time_string`], with its ISO 8601 parser).
//! Each step names the expression it ports. Like Emacs, these match
//! without regard to case. Unlike Emacs, a word boundary is only one
//! between a word character and anything else: `_` is part of a word,
//! and a change of script (`日W9`) is no boundary.

use std::sync::LazyLock;

use regex::{Captures, Regex};

use crate::date::{days_in_month, is_leap_year, weekday, Date, DateTime, Day};

/// `org-read-date-prefer-future`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreferFuture {
    /// `nil`: a date without a year is in the default's year.
    No,
    /// `t`: a date without a year or month is the next one to come.
    Yes,
    /// `time`: like `t`, and a time that has passed today means tomorrow.
    Time,
}

/// Settings, named after Org's variables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// `org-read-date-prefer-future`
    pub prefer_future: PreferFuture,
    /// `org-read-date-force-compatible-dates`: keep the year within 1970
    /// to 2037.
    pub force_compatible_dates: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { prefer_future: PreferFuture::Yes, force_compatible_dates: true }
    }
}

/// What an answer means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadDate {
    pub date: Date,
    /// The time, when the answer gave one (`org-time-was-given`).
    pub time: Option<(i64, i64)>,
    /// The end of a time range, as it was typed (`org-end-time-was-given`):
    /// `11:30`, `5pm`.
    pub end: Option<String>,
    /// The year was outside what `force_compatible_dates` allows and was
    /// moved into it (`org-read-date-analyze-forced-year`).
    pub forced_year: bool,
    /// A date without a year or month was taken to be the next one to
    /// come (`org-read-date-analyze-futurep`); the prompt shows `(=>F)`.
    pub future: bool,
}

impl ReadDate {
    /// `YYYY-MM-DD`, with ` HH:MM` when a time was given: what
    /// `org-read-date` returns.
    pub fn to_org(&self) -> String {
        match self.time {
            Some((hour, minute)) => format!("{} {hour:02}:{minute:02}", self.date),
            None => self.date.to_string(),
        }
    }
}

/// The fields of a decoded time that `parse-time-string` found.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Decoded {
    pub second: Option<i64>,
    pub minute: Option<i64>,
    pub hour: Option<i64>,
    pub day: Option<i64>,
    pub month: Option<i64>,
    pub year: Option<i64>,
    /// 0 is Sunday.
    pub weekday: Option<i64>,
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

const MONTHS: [(&str, i64); 23] = [
    ("jan", 1),
    ("feb", 2),
    ("mar", 3),
    ("apr", 4),
    ("may", 5),
    ("jun", 6),
    ("jul", 7),
    ("aug", 8),
    ("sep", 9),
    ("oct", 10),
    ("nov", 11),
    ("dec", 12),
    ("january", 1),
    ("february", 2),
    ("march", 3),
    ("april", 4),
    ("june", 6),
    ("july", 7),
    ("august", 8),
    ("september", 9),
    ("october", 10),
    ("november", 11),
    ("december", 12),
];

/// `parse-time-weekdays`
const WEEKDAYS: [(&str, i64); 14] = [
    ("sun", 0),
    ("mon", 1),
    ("tue", 2),
    ("wed", 3),
    ("thu", 4),
    ("fri", 5),
    ("sat", 6),
    ("sunday", 0),
    ("monday", 1),
    ("tuesday", 2),
    ("wednesday", 3),
    ("thursday", 4),
    ("friday", 5),
    ("saturday", 6),
];

/// `parse-time-zoneinfo`: names that are taken as a zone and otherwise
/// ignored.
const ZONES: [&str; 11] = ["z", "ut", "gmt", "pst", "pdt", "mst", "mdt", "cst", "cdt", "est", "edt"];

fn lookup(table: &[(&str, i64)], name: &str) -> Option<i64> {
    table.iter().find(|(key, _)| *key == name).map(|(_, value)| *value)
}

/// `string-to-number`: the number at the start of `text` after blanks,
/// or 0. Too large a number saturates.
fn string_to_number(text: &str) -> i64 {
    let text = text.trim_start_matches([' ', '\t']);
    let (sign, digits) = match text.as_bytes().first() {
        Some(b'-') => (-1, &text[1..]),
        Some(b'+') => (1, &text[1..]),
        _ => (1, text),
    };
    let digits = &digits[..digits.bytes().take_while(u8::is_ascii_digit).count()];
    sign * digits.parse::<i64>().unwrap_or(if digits.is_empty() { 0 } else { i64::MAX })
}

/// `cl-parse-integer` on `text`: a sign and digits, nothing else.
fn parse_integer(text: &str) -> Result<i64, String> {
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("Not an integer string: `{text}'"));
    }
    Ok(string_to_number(text))
}

fn group<'t>(captures: &Captures<'t>, index: usize) -> Option<&'t str> {
    captures.get(index).map(|found| found.as_str())
}

/// Replace what `found` matched in `text` with `with`.
fn replace(text: &str, found: &regex::Match, with: &str) -> String {
    format!("{}{with}{}", &text[..found.start()], &text[found.end()..])
}

// ----- parse-time-string

/// `parse-time-string`: an ISO 8601 date and time if the whole text is
/// one, else whatever the words and numbers in it look like.
pub fn parse_time_string(text: &str) -> Result<Decoded, String> {
    match iso8601_parse(text)? {
        Some(decoded) => Ok(decoded),
        None => parse_time_words(text),
    }
}

/// A word of `parse-time-tokenize`: a number when it is all digits.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Number(i64),
    Word(String),
}

/// `parse-time-tokenize`: runs of lower-case letters, digits, `+`, `-`
/// and `:`.
fn tokenize(text: &str) -> Vec<Token> {
    let valid = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '+' | '-' | ':');
    text.split(|c: char| !valid(c))
        .filter(|word| !word.is_empty())
        .map(|word| {
            if word.bytes().all(|byte| byte.is_ascii_digit()) {
                Token::Number(string_to_number(word))
            } else {
                Token::Word(word.to_string())
            }
        })
        .collect()
}

/// The fallback of `parse-time-string`: `parse-time-rules` applied to
/// each word. A rule fills fields that are still empty; the first rule
/// that applies takes the word.
fn parse_time_words(text: &str) -> Result<Decoded, String> {
    let mut time = Decoded::default();
    let mut zone = false;
    for token in tokenize(&text.to_lowercase()) {
        let word = match &token {
            Token::Word(word) => Some(word.as_str()),
            Token::Number(_) => None,
        };
        let number = match token {
            Token::Number(number) => Some(number),
            Token::Word(_) => None,
        };
        let shape = |length: usize, colons: &[usize]| {
            word.filter(|word| word.len() == length && colons.iter().all(|at| word.as_bytes()[*at] == b':'))
        };
        let piece = |word: &str, start: usize, end: usize| parse_integer(&word[start..end]);
        if let (None, Some(day)) = (time.weekday, word.and_then(|word| lookup(&WEEKDAYS, word))) {
            time.weekday = Some(day);
        } else if let (None, Some(day)) = (time.day, number.filter(|n| (1..=31).contains(n))) {
            time.day = Some(day);
        } else if let (None, Some(month)) = (time.month, word.and_then(|word| lookup(&MONTHS, word))) {
            time.month = Some(month);
        } else if let (None, Some(year)) = (time.year, number.filter(|n| *n >= 100)) {
            time.year = Some(year);
        } else if let (None, Some(word)) = (time.hour, shape(8, &[2, 5])) {
            (time.hour, time.minute, time.second) =
                (Some(piece(word, 0, 2)?), Some(piece(word, 3, 5)?), Some(piece(word, 6, 8)?));
        } else if !zone && word.is_some_and(|word| ZONES.contains(&word)) {
            zone = true;
        } else if let (false, Some(word)) =
            (zone, word.filter(|word| word.len() == 5 && matches!(word.as_bytes()[0], b'+' | b'-')))
        {
            piece(word, 3, 5)?;
            piece(word, 1, 3)?;
            zone = true;
        } else if let (None, Some(word)) = (
            time.year,
            word.filter(|word| word.len() == 10 && word.as_bytes()[4] == b'-' && word.as_bytes()[7] == b'-'),
        ) {
            (time.year, time.month, time.day) =
                (Some(piece(word, 0, 4)?), Some(piece(word, 5, 7)?), Some(piece(word, 8, 10)?));
        } else if let (None, Some(word)) = (time.hour, shape(5, &[2])) {
            (time.hour, time.minute, time.second) = (Some(piece(word, 0, 2)?), Some(piece(word, 3, 5)?), Some(0));
        } else if let (None, Some(word)) = (time.hour, shape(4, &[1])) {
            (time.hour, time.minute, time.second) = (Some(piece(word, 0, 1)?), Some(piece(word, 2, 4)?), Some(0));
        } else if let (None, Some(word)) = (time.hour, shape(7, &[1])) {
            (time.hour, time.minute, time.second) =
                (Some(piece(word, 0, 1)?), Some(piece(word, 2, 4)?), Some(piece(word, 5, 7)?));
        } else if let (None, Some(year)) = (time.year, number.filter(|n| (50..=110).contains(n))) {
            time.year = Some(1900 + year);
        } else if let (None, Some(year)) = (time.year, number.filter(|n| (0..=49).contains(n))) {
            time.year = Some(2000 + year);
        }
    }
    Ok(time)
}

const ISO_YEAR: &str = r"[+-]?[0-9]{4}";
const ISO_FULL_DATE: &str = r"([+-]?[0-9]{4})-?([0-9]{2})-?([0-9]{2})";
const ISO_WITHOUT_DAY: &str = r"([+-]?[0-9]{4})-([0-9]{2})";
const ISO_OUTDATED_DATE: &str = r"--([0-9]{2})-?([0-9]{2})";
const ISO_OUTDATED_REDUCED: &str = r"---?([0-9]{2})";
const ISO_WEEK_DATE: &str = r"([+-]?[0-9]{4})-?W([0-9]{2})-?([0-9])?";
const ISO_ORDINAL_DATE: &str = r"([+-]?[0-9]{4})-?([0-9]{3})";
const ISO_TIME: &str = r"([0-9]{2}):?([0-9]{2})?:?([0-9]{2})?[.,]?([0-9]*)";
const ISO_ZONE: &str = r"(?:Z|[+-][0-9]{2}:?(?:[0-9]{2})?)";

/// `iso8601-parse`, or `None` where it signals that `text` is not ISO
/// 8601 (`iso8601-valid-p`).
fn iso8601_parse(text: &str) -> Result<Option<Decoded>, String> {
    static COMBINED: LazyLock<Regex> = LazyLock::new(|| {
        let shy = |pattern: &str| pattern.replace('(', "(?:");
        let dates = [
            ISO_YEAR,
            ISO_FULL_DATE,
            ISO_WITHOUT_DAY,
            ISO_OUTDATED_DATE,
            ISO_OUTDATED_REDUCED,
            ISO_WEEK_DATE,
            ISO_ORDINAL_DATE,
        ]
        .map(|pattern| format!("(?:{})", shy(pattern)))
        .join("|");
        rx(&format!(r"(?i)\A({dates})(?:T({})({ISO_ZONE})?)?\z", shy(ISO_TIME)))
    });
    let Some(captures) = COMBINED.captures(text) else { return Ok(None) };
    let mut decoded = iso8601_parse_date(group(&captures, 1).unwrap_or(""))?;
    if let Some(time) = group(&captures, 2) {
        let found = regex!(&format!(r"(?i)\A{ISO_TIME}\z")).captures(time).ok_or("Invalid time")?;
        decoded.hour = Some(string_to_number(group(&found, 1).unwrap_or("")));
        decoded.minute = Some(group(&found, 2).map_or(0, string_to_number));
        decoded.second = Some(group(&found, 3).map_or(0, string_to_number));
    }
    Ok(Some(decoded))
}

/// `iso8601-parse-date`
fn iso8601_parse_date(text: &str) -> Result<Decoded, String> {
    let number = |captures: &Captures, index| group(captures, index).map(string_to_number);
    static PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
        [
            ISO_YEAR,
            ISO_FULL_DATE,
            ISO_WITHOUT_DAY,
            ISO_OUTDATED_DATE,
            ISO_WEEK_DATE,
            ISO_ORDINAL_DATE,
            "--([0-9]{2})",
            "---([0-9]{2})",
        ]
        .iter()
        .map(|pattern| rx(&format!(r"(?i)\A{pattern}\z")))
        .collect()
    });
    let date = |year, month, day| Decoded { year, month, day, ..Decoded::default() };
    for (index, pattern) in PATTERNS.iter().enumerate() {
        let Some(captures) = pattern.captures(text) else { continue };
        return match index {
            0 => Ok(date(Some(string_to_number(text)), None, None)),
            1 => Ok(date(number(&captures, 1), number(&captures, 2), number(&captures, 3))),
            2 => Ok(date(Some(string_to_number(text)), number(&captures, 2), None)),
            3 => Ok(date(None, number(&captures, 1), number(&captures, 2))),
            4 => {
                let mut year = string_to_number(text);
                let week = number(&captures, 2).unwrap_or(0);
                let day_of_week = number(&captures, 3);
                let jan_start = weekday(Date::new(year, 1, 4).to_day()) as i64;
                let correction = if jan_start == 0 { 7 } else { jan_start } + 3;
                let mut ordinal = week * 7 + day_of_week.unwrap_or(0) - correction;
                let year_length = |year| if is_leap_year(year) { 366 } else { 365 };
                if ordinal < 1 {
                    year -= 1;
                    ordinal += year_length(year);
                } else if ordinal > year_length(year) {
                    ordinal -= year_length(year);
                    year += 1;
                }
                let (month, day) = date_ordinal_to_time(year, ordinal)?;
                Ok(date(Some(year), Some(month), Some(day)))
            }
            5 => {
                let year = number(&captures, 1).unwrap_or(0);
                let (month, day) = date_ordinal_to_time(year, number(&captures, 2).unwrap_or(0))?;
                Ok(date(Some(year), Some(month), Some(day)))
            }
            6 => Ok(date(None, number(&captures, 1), None)),
            _ => Ok(date(None, None, number(&captures, 1))),
        };
    }
    Err(format!("Invalid date: {text}"))
}

/// `date-ordinal-to-time`: the month and day of day `ordinal` of `year`.
fn date_ordinal_to_time(year: i64, mut ordinal: i64) -> Result<(i64, i64), String> {
    let mut month = 1;
    loop {
        if !(1..=12).contains(&month) {
            return Err(format!("Month {month} is invalid"));
        }
        if ordinal <= days_in_month(year, month) {
            return Ok((month, ordinal));
        }
        ordinal -= days_in_month(year, month);
        month += 1;
    }
}

// ----- org-read-date

/// `org-small-year-to-year`: a two-digit year in the thirty years to come,
/// or else in the past.
fn small_year_to_year(year: i64, current: i64) -> i64 {
    if year >= 100 {
        return year;
    }
    let century = current / 100;
    let offset = year - current % 100;
    if offset > 30 {
        (century - 1) * 100 + year
    } else if offset > -70 {
        century * 100 + year
    } else {
        (century + 1) * 100 + year
    }
}

/// A shift that `org-read-date-get-relative` found: `n` units `what`,
/// from the default date when `from_default`.
struct Relative {
    n: i64,
    what: String,
    from_default: bool,
}

/// `org-read-date-get-relative`: `+2d`, `-1w`, `++3`, `fri`, `-2tue` at
/// the start of `text`. Returns the shift and where it ends.
fn relative(text: &str, today: Day, default: Day) -> Result<Option<(Relative, usize)>, String> {
    let pattern = regex!(
        r"(?i)\A[ \t]*([-+]{0,2})([0-9]+)?([hdwmy]|(sun|mon|tue|wed|thu|fri|sat|sunday|monday|tuesday|wednesday|thursday|friday|saturday))?([ \t]|$)"
    );
    let Some(captures) = pattern.captures(text) else { return Ok(None) };
    let sign = group(&captures, 1).unwrap_or("");
    if sign.is_empty() && captures.get(4).is_none() {
        return Ok(None);
    }
    let end = captures.get(0).map_or(0, |found| found.end());
    let backward = sign.ends_with('-');
    let from_default = sign.len() == 2;
    let n = match group(&captures, 2) {
        Some(digits) => digits.parse::<i64>().map_err(|_| format!("Number too large: {digits}"))?,
        None => 1,
    };
    let what = group(&captures, 3).unwrap_or("d");
    let shift = match lookup(&WEEKDAYS, &what.to_lowercase()) {
        Some(target) => {
            let date = if from_default { default } else { today };
            let mut delta = (7 + target - weekday(date) as i64).rem_euclid(7);
            if delta == 0 {
                delta = 7;
            }
            if backward {
                delta -= 7;
                if delta == 0 {
                    delta = -7;
                }
            }
            if n > 1 {
                delta += (n - 1) * if backward { -7 } else { 7 };
            }
            Relative { n: delta, what: "d".to_string(), from_default }
        }
        None => Relative { n: if backward { -n } else { n }, what: what.to_string(), from_default },
    };
    Ok(Some((shift, end)))
}

/// A time of day as `decode-time` has it.
#[derive(Debug, Clone, Copy)]
struct Fields {
    minute: i64,
    hour: i64,
    day: i64,
    month: i64,
    year: i64,
}

impl Fields {
    fn of(time: DateTime) -> Fields {
        let date = time.date();
        Fields { minute: time.minute % 60, hour: time.minute / 60, day: date.day, month: date.month, year: date.year }
    }
}

/// `answer` without the times it gives (`10:00`, `3pm`, `10:00-11:30`,
/// `10:00+1:30`), so that a time picked another way can take their place.
pub fn without_time(answer: &str) -> String {
    let duration = regex!(r"(?i)\b[012]?[0-9]:[0-6][0-9]\+[012]?[0-9](:[0-5][0-9])?");
    let time = regex!(
        r"(?i)\b[012]?[0-9]((:[0-5][0-9]([ap]m)?)|([ap]m))\b(--?\b[012]?[0-9]((:[0-5][0-9]([ap]m)?)|([ap]m))\b)?"
    );
    let out = duration.replace_all(answer, "");
    let out = time.replace_all(&out, "");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `org-read-date` called with `answer` as the text typed at its prompt,
/// with `org-time-was-given` and `org-end-time-was-given` bound as
/// `org-time-stamp` binds them. `default` is the date the prompt starts
/// from (DEFAULT-TIME), `now` the current time.
pub fn read_date(
    answer: &str,
    default: Option<DateTime>,
    now: DateTime,
    settings: &Settings,
) -> Result<ReadDate, String> {
    let nowdecode = Fields::of(now);
    let defdecode = Fields::of(default.unwrap_or(now));
    let current_year = nowdecode.year;
    let mut ans = answer.to_string();
    let mut kill_year = false;

    if regex!(r"\A[ \t]*\.[ \t]*\z").is_match(&ans) {
        ans = "+0".to_string();
    }

    let today = now.day;
    let delta = match relative(&ans, today, default.unwrap_or(now).day)? {
        Some((delta, end)) => {
            ans = ans[end..].to_string();
            Some(delta)
        }
        None => None,
    };

    // An ISO week date: `w12`, `2026-w12-3`.
    let mut iso = None;
    let week = regex!(r"(?i)\b(?:([0-9]+)-)?w([0-9]{1,2})(?:-([0-6]))?([ \t]|$)");
    if let Some(captures) = week.captures(&ans) {
        let year = group(&captures, 1).map(|year| small_year_to_year(string_to_number(year), current_year));
        let day = group(&captures, 3).map(string_to_number);
        let number = string_to_number(group(&captures, 2).unwrap_or(""));
        iso = Some((year, day, number));
        let found = captures.get(0).expect("whole match");
        ans = replace(&ans, &found, "");
    }

    // ISO dates with one-digit months or days: `2006-8-11`, `8-11`.
    let short_iso = regex!(r"(?i)\A *(([0-9]+)-)?([0-1]?[0-9])-([0-3]?[0-9])([^-0-9]|$)");
    if let Some(captures) = short_iso.captures(&ans) {
        let year = match group(&captures, 2) {
            Some(year) => string_to_number(year),
            None => {
                kill_year = true;
                current_year
            }
        };
        let year = small_year_to_year(year, current_year);
        let (month, day) = (string_to_number(&captures[3]), string_to_number(&captures[4]));
        let with = format!("{year:04}-{month:02}-{day:02}{}", group(&captures, 5).unwrap_or(""));
        ans = replace(&ans, &captures.get(0).expect("whole match"), &with);
    }

    // Dotted European dates: `15.6.`, `15. 6. 2027`.
    let dotted = regex!(r"(?i)\A *(3[01]|0?[1-9]|[12][0-9])\. ?(0?[1-9]|1[012])\.( ?[1-9][0-9]{3})?");
    if let Some(captures) = dotted.captures(&ans) {
        let year = match group(&captures, 3) {
            Some(year) => string_to_number(year),
            None => {
                kill_year = true;
                current_year
            }
        };
        let (day, month) = (string_to_number(&captures[1]), string_to_number(&captures[2]));
        ans = replace(&ans, &captures.get(0).expect("whole match"), &format!("{year:04}-{month:02}-{day:02}"));
    }

    // American dates: `5/30`, `5/30/7`.
    let american = regex!(r"(?i)\A *(0?[1-9]|1[012])/(0?[1-9]|[12][0-9]|3[01])(/([0-9]+))?([^/0-9]|$)");
    if let Some(captures) = american.captures(&ans) {
        let year = match group(&captures, 4) {
            Some(year) => string_to_number(year),
            None => {
                kill_year = true;
                current_year
            }
        };
        let year = small_year_to_year(year, current_year);
        let (month, day) = (string_to_number(&captures[1]), string_to_number(&captures[2]));
        let with = format!("{year:04}-{month:02}-{day:02}{}", group(&captures, 5).unwrap_or(""));
        ans = replace(&ans, &captures.get(0).expect("whole match"), &with);
    }

    // Times with am or pm, and times like `10h30`, when no `HH:MM` is
    // there; twice, for the end of a range.
    let has_time = regex!(r"(?i)(?:\A|[^+])[012]?[0-9]:[0-9][0-9](?:[ \t\n]|$)");
    let am_pm = regex!(r"(?i)([012]?[0-9])(:([0-5][0-9]))?(am|pm)\b");
    for _ in 0..2 {
        if has_time.is_match(&ans) {
            break;
        }
        let Some(captures) = am_pm.captures(&ans) else { break };
        let mut hour = string_to_number(&captures[1]);
        let minute = group(&captures, 3).map_or(0, string_to_number);
        let pm = captures[4].to_lowercase().starts_with('p');
        if hour == 12 && !pm {
            hour = 0;
        } else if pm && hour < 12 {
            hour += 12;
        }
        ans = replace(&ans, &captures.get(0).expect("whole match"), &format!("{hour:02}:{minute:02}"));
    }
    let h_time =
        regex!(r"(?i)(?:(?P<h1>[012]?[0-9])?h(?P<m1>[0-5][0-9]))|(?:(?P<h2>[012]?[0-9])h(?P<m2>[0-5][0-9])?\b)");
    for _ in 0..2 {
        if has_time.is_match(&ans) {
            break;
        }
        let Some(captures) = h_time.captures(&ans) else { break };
        let hour = captures.name("h1").or(captures.name("h2")).map_or(0, |found| string_to_number(found.as_str()));
        let minute = captures.name("m1").or(captures.name("m2")).map_or(0, |found| string_to_number(found.as_str()));
        ans = replace(&ans, &captures.get(0).expect("whole match"), &format!("{hour:02}:{minute:02}"));
    }

    // A range given as a duration: `10:00+1:30`.
    let duration = regex!(r"(?i)([012]?[0-9]):([0-6][0-9])\+([012]?[0-9])(:([0-5][0-9]))?");
    if let Some(captures) = duration.captures(&ans) {
        let hour = string_to_number(&captures[1]);
        let minute = string_to_number(&captures[2]);
        let mut h2 = hour + string_to_number(&captures[3]);
        let mut m2 = minute + group(&captures, 5).map_or(0, string_to_number);
        if m2 >= 60 {
            h2 += 1;
            m2 -= 60;
        }
        ans = replace(&ans, &captures.get(0).expect("whole match"), &format!("{hour:02}:{minute:02}-{h2:02}:{m2:02}"));
    }

    // A time range (`org-plain-time-of-day-regexp`): its end is taken out.
    let mut end = None;
    let time_of_day = regex!(
        r"(?i)(\b[012]?[0-9]((:([0-5][0-9]([AaPp][Mm])?))|([AaPp][Mm]))\b)(--?(\b[012]?[0-9]((:([0-5][0-9]([AaPp][Mm])?))|([AaPp][Mm]))\b))?"
    );
    if let Some(captures) = time_of_day.captures(&ans) {
        if let (Some(range), Some(second)) = (captures.get(7), group(&captures, 8)) {
            end = Some(second.to_string());
            ans = replace(&ans, &range, "");
        }
    }

    let tl = parse_time_string(&ans)?;
    let mut futurep = false;
    let mut day = tl.day.unwrap_or(defdecode.day);
    let prefer = settings.prefer_future;
    let mut month = match tl.month {
        Some(month) => month,
        None if prefer == PreferFuture::No => defdecode.month,
        None if tl.day.is_some() => {
            futurep = true;
            if day < nowdecode.day {
                nowdecode.month + 1
            } else {
                nowdecode.month
            }
        }
        None => defdecode.month,
    };
    let mut year = match tl.year.filter(|_| !kill_year) {
        Some(year) => year,
        None if prefer == PreferFuture::No => defdecode.year,
        None if futurep => {
            if month > nowdecode.month || day >= nowdecode.day {
                nowdecode.year
            } else {
                nowdecode.year + 1
            }
        }
        None if tl.month.is_some() => {
            futurep = true;
            if month > nowdecode.month {
                nowdecode.year
            } else if month < nowdecode.month || day < nowdecode.day {
                nowdecode.year + 1
            } else {
                nowdecode.year
            }
        }
        None => defdecode.year,
    };
    let mut hour = tl.hour.unwrap_or(defdecode.hour);
    let minute = tl.minute.unwrap_or(defdecode.minute);
    let second = tl.second.unwrap_or(0);
    let wday = tl.weekday;

    if prefer == PreferFuture::Time
        && tl.day.is_none()
        && tl.month.is_none()
        && tl.year.is_none()
        && (day, month, year) == (nowdecode.day, nowdecode.month, nowdecode.year)
        && tl.hour.is_some_and(|hour| {
            hour < nowdecode.hour || (hour == nowdecode.hour && tl.minute.is_some_and(|m| m < nowdecode.minute))
        })
    {
        day += 1;
        futurep = true;
    }

    let mut time_given = false;
    if let Some((iso_year, iso_day, week)) = iso {
        // `calendar-iso-to-absolute`
        futurep = false;
        year = iso_year.unwrap_or(year);
        let day_of_week = iso_day.or(wday).unwrap_or(1);
        let fourth = Date::new(year, 1, 1).to_day() + 3;
        let monday = fourth - (fourth - 1) % 7;
        let absolute = monday + 7 * (week - 1) + if day_of_week == 0 { 6 } else { day_of_week - 1 };
        let date = Date::from_day(absolute);
        (year, month, day) = (date.year, date.month, date.day);
    } else if let Some(delta) = delta {
        futurep = false;
        if !delta.from_default {
            (day, month, year) = (nowdecode.day, nowdecode.month, nowdecode.year);
        }
        let n = delta.n;
        let add = |value: i64, by: i64| value.checked_add(by).ok_or_else(|| "Date out of range".to_string());
        match delta.what.as_str() {
            "h" => {
                time_given = true;
                hour = add(hour, n)?;
            }
            "d" => day = add(day, n)?,
            "w" => day = add(day, n.checked_mul(7).ok_or("Date out of range")?)?,
            "m" => month = add(month, n)?,
            "y" => year = add(year, n)?,
            _ => {}
        }
    } else if let (Some(wday), None) = (wday, tl.day) {
        // A weekday without a day: that weekday on or after the date.
        let wday1 = weekday(Date::new(year, month, day).to_day()) as i64;
        if wday != wday1 {
            day += (wday - wday1 + 7) % 7;
        }
    }
    if tl.hour.is_some() {
        time_given = true;
    }
    if year < 100 {
        year += 2000;
    }
    let mut forced_year = false;
    if settings.force_compatible_dates {
        if year < 1970 {
            (year, forced_year) = (1970, true);
        }
        if year > 2037 {
            (year, forced_year) = (2037, true);
        }
    }
    if [day, month, hour, minute, second].iter().any(|field| field.abs() > 1_000_000_000_000) {
        return Err("Specified time is not representable".to_string());
    }

    // `encode-time` puts every field back in range.
    let seconds = (hour * 60 + minute) * 60 + second;
    let day_number = Date::new(year, month, day).to_day() + seconds.div_euclid(86_400);
    let seconds = seconds.rem_euclid(86_400);
    Ok(ReadDate {
        date: Date::from_day(day_number),
        time: time_given.then_some((seconds / 3600, seconds / 60 % 60)),
        end,
        forced_year,
        future: futurep,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_the_times_out_of_an_answer() {
        for (answer, without) in [
            ("10:00", ""),
            ("fri 3pm", "fri"),
            ("10:00-11:30 +1w", "+1w"),
            ("10:00+1:30", ""),
            ("2/15 10:00", "2/15"),
            ("+2d", "+2d"),
            ("w12", "w12"),
        ] {
            assert_eq!(without_time(answer), without, "{answer}");
        }
    }

    fn read(answer: &str) -> String {
        let now = DateTime::new(Date::new(2026, 5, 18), 10, 30);
        let read = read_date(answer, None, now, &Settings::default()).unwrap();
        match &read.end {
            Some(end) => format!("{} | {end}", read.to_org()),
            None => read.to_org(),
        }
    }

    #[test]
    fn reads_what_the_org_manual_lists() {
        assert_eq!(read("3-2-5"), "2003-02-05");
        assert_eq!(read("feb 15"), "2027-02-15");
        assert_eq!(read("sep 12 9"), "2009-09-12");
        assert_eq!(read("12:45"), "2026-05-18 12:45");
        assert_eq!(read("22 sept 0:34"), "2026-05-22 00:34");
        assert_eq!(read("12"), "2026-06-12");
        assert_eq!(read("fri"), "2026-05-22");
        assert_eq!(read("-tue"), "2026-05-12");
        assert_eq!(read("+4d"), "2026-05-22");
        assert_eq!(read("+2w"), "2026-06-01");
        assert_eq!(read("w12"), "2026-03-16");
        assert_eq!(read("2026-w4-3"), "2026-01-21");
        assert_eq!(read("10am-11:30"), "2026-05-18 | 11:30");
        assert_eq!(read("10:00+1:30"), "2026-05-18 10:00 | 11:30");
        assert_eq!(read("3pm-5pm"), "2026-05-18 15:00 | 17:00");
    }

    #[test]
    fn parse_time_string_like_emacs() {
        let words = parse_time_string("Wed, 15 Jan 2020 16:12:21").unwrap();
        assert_eq!(
            (words.weekday, words.day, words.month, words.year, words.hour, words.minute, words.second),
            (Some(3), Some(15), Some(1), Some(2020), Some(16), Some(12), Some(21))
        );
        let iso = parse_time_string("2026-W12-3").unwrap();
        assert_eq!((iso.year, iso.month, iso.day), (Some(2026), Some(3), Some(18)));
        assert!(parse_time_string("2026-400").is_err());
    }
}
