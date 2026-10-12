//! Reading dates, times, repeaters and warning periods out of timestamp
//! text.
//!
//! Org's agenda does not work on parsed timestamps. It applies a handful
//! of regular expressions to the raw text, and their quirks decide what
//! shows up where. The functions here are ports of those expressions and
//! of the functions around them in `org.el`; each names its original.

use std::sync::LazyLock;

use regex::Regex;

use crate::date::{Date, DateTime, Day};
use crate::text::{word_end, word_start};

/// The timer value (`h:mm:ss`) on the line of `offset` that holds it
/// (`org-in-regexp` of `org-timer-re`).
pub fn timer_value_at(text: &str, offset: usize) -> Option<String> {
    static TIMER: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"[-+]?[0-9]+:[0-9]{2}:[0-9]{2}").expect("a valid regex"));
    let start = text[..offset].rfind('\n').map_or(0, |at| at + 1);
    let end = text[offset..].find('\n').map_or(text.len(), |at| offset + at);
    TIMER
        .find_iter(&text[start..end])
        .find(|found| (start + found.start()..=start + found.end()).contains(&offset))
        .map(|found| found.as_str().to_string())
}

/// The pieces `org-parse-time-string` extracts: the first `YYYY-MM-DD` in
/// the text and the `H:MM` after it, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeString {
    pub date: Date,
    pub time: Option<(i64, i64)>,
}

fn digits(bytes: &[u8], at: usize, count: usize) -> Option<i64> {
    let slice = bytes.get(at..at + count)?;
    slice.iter().all(u8::is_ascii_digit).then(|| slice.iter().fold(0, |n, b| n * 10 + i64::from(b - b'0')))
}

/// Length of the `YYYY-MM-DD` at byte `at`, which is always 10.
fn date_at(bytes: &[u8], at: usize) -> Option<Date> {
    let year = digits(bytes, at, 4)?;
    let month = digits(bytes, at + 5, 2)?;
    let day = digits(bytes, at + 8, 2)?;
    (bytes[at + 4] == b'-' && bytes[at + 7] == b'-').then_some(Date::new(year, month, day))
}

/// `org-parse-time-string` (`org-ts-regexp0`): the first date in `text`,
/// an optional day name after it, and an optional time after that.
pub fn parse_time_string(text: &str) -> Option<TimeString> {
    let bytes = text.as_bytes();
    let start = (0..bytes.len()).find(|at| date_at(bytes, *at).is_some())?;
    let date = date_at(bytes, start)?;
    let mut at = start + 10;
    // `\( +[^]+0-9>\r\n -]+\)?`: blanks and a day name.
    let blanks = bytes[at..].iter().take_while(|b| **b == b' ').count();
    if blanks > 0 {
        let name = text[at + blanks..]
            .char_indices()
            .find(|(_, c)| matches!(c, ']' | '+' | '0'..='9' | '>' | '\r' | '\n' | ' ' | '-'))
            .map_or(text.len() - at - blanks, |(index, _)| index);
        if name > 0 {
            at += blanks + name;
        }
    }
    // `\( +\([0-9]\{1,2\}\):\([0-9]\{2\}\)\)?`
    let blanks = bytes[at..].iter().take_while(|b| **b == b' ').count();
    let time = (blanks > 0)
        .then(|| {
            let at = at + blanks;
            let hours = bytes[at..].iter().take_while(|b| b.is_ascii_digit()).count().min(2);
            (1..=hours).rev().find_map(|len| {
                let hour = digits(bytes, at, len)?;
                (bytes.get(at + len) == Some(&b':')).then_some(())?;
                Some((hour, digits(bytes, at + len + 1, 2)?))
            })
        })
        .flatten();
    Some(TimeString { date, time })
}

/// `org-time-string-to-time` followed by `time-to-days`: the day the
/// timestamp falls on. A time past 24:00 moves it to a later day.
pub fn time_string_to_day(text: &str) -> Option<Day> {
    let parsed = parse_time_string(text)?;
    let (hour, minute) = parsed.time.unwrap_or((0, 0));
    Some(DateTime::new(parsed.date, hour, minute).day)
}

/// A repeater or warning cookie: a count and a unit among `hdwmy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval {
    pub value: i64,
    pub unit: u8,
}

/// The first `+N[hdwmy]` in `text`, which is what `org-closest-date` and
/// the agenda take as the repeater.
pub fn repeater(text: &str) -> Option<Interval> {
    let bytes = text.as_bytes();
    (0..bytes.len()).find_map(|at| {
        if bytes[at] != b'+' {
            return None;
        }
        let count = bytes[at + 1..].iter().take_while(|b| b.is_ascii_digit()).count();
        let unit = *bytes.get(at + 1 + count)?;
        if count == 0 || !b"hdwmy".contains(&unit) {
            return None;
        }
        let value = text[at + 1..at + 1 + count].parse().unwrap_or(i64::MAX);
        Some(Interval { value, unit })
    })
}

/// Which side `closest_date` leans to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prefer {
    Past,
    Future,
    Nearest,
}

/// `org-closest-date`: the occurrence of the repeating timestamp `start`
/// that is closest to `current`. Without a repeater this is `start` itself.
pub fn closest_date(start: &str, current: Day, prefer: Prefer) -> Option<Day> {
    let base = parse_time_string(start)?;
    let interval = match repeater(start) {
        Some(interval) if interval.value != 0 => interval,
        // No repeater, or a void one: the timestamp does not move.
        _ => return time_string_to_day(start),
    };
    let sday = base.date.to_day();
    if current <= sday {
        return Some(sday);
    }
    // A repeater longer than any calendar holds (`+999999999999y`)
    // repeats no more often than this one, and keeps the sums in range.
    let value = interval.value.clamp(1, 10_000_000);
    let target = Date::from_day(current);
    let (n1, n2) = match interval.unit {
        b'h' => {
            let start_hour = base.time.map_or(0, |(hour, _)| hour);
            let missing = (24 * (current - sday) - start_hour).rem_euclid(value);
            let n1 = if missing == 0 { current } else { current - (1 + missing / 24) };
            (n1, current + (value - missing) / 24)
        }
        b'd' | b'w' => {
            let value = if interval.unit == b'w' { 7 * value } else { value };
            let n1 = sday + value * ((current - sday) / value);
            (n1, n1 + value)
        }
        b'm' => {
            // Org's month arithmetic is zero-based modulo twelve, so month 12
            // comes out as month 0; `Date::to_day` normalises it the same
            // way `calendar-absolute-from-gregorian` ends up doing.
            let add_months = |date: Date, n: i64| {
                let month = date.month + n;
                Date::new(month.div_euclid(12) + date.year, month.rem_euclid(12), date.day)
            };
            let complete = 12 * (target.year - base.date.year) + (target.month - base.date.month)
                - i64::from(target.day <= base.date.day);
            let months = complete.div_euclid(value) * value;
            let before = add_months(base.date, months);
            (emacs_absolute(before), emacs_absolute(add_months(before, value)))
        }
        _ => {
            let Date { year, month, day } = base.date;
            let incomplete = !(target.month > month || (target.month == month && target.day > day));
            let years = (target.year - year - i64::from(incomplete)).div_euclid(value) * value;
            (
                emacs_absolute(Date::new(year + years, month, day)),
                emacs_absolute(Date::new(year + years + value, month, day)),
            )
        }
    };
    Some(match prefer {
        Prefer::Past => {
            if current == n2 {
                n2
            } else {
                n1
            }
        }
        Prefer::Future => {
            if current == n1 {
                n1
            } else {
                n2
            }
        }
        Prefer::Nearest => {
            if (current - n1).abs() > (current - n2).abs() {
                n2
            } else {
                n1
            }
        }
    })
}

/// `calendar-absolute-from-gregorian` on a date whose month may be 0 and
/// whose day may exceed the month: it adds up day counts without checking.
fn emacs_absolute(date: Date) -> Day {
    use crate::date::is_leap_year;
    let Date { year, month, day } = date;
    // `calendar-day-number`: day of the year, valid months or not.
    let mut day_of_year = day + 31 * (month - 1);
    if month > 2 {
        day_of_year -= (23 + 4 * month) / 10;
        if is_leap_year(year) {
            day_of_year += 1;
        }
    }
    let prior = year - 1;
    day_of_year + 365 * prior + prior.div_euclid(4) - prior.div_euclid(100) + prior.div_euclid(400)
}

/// `org-get-wdays`: the warning period of a deadline in days, or with
/// `delay` the delay of a scheduled item. `default` is
/// `org-deadline-warning-days` or `org-scheduled-delay-days`.
pub fn warning_days(text: &str, default: i64, delay: bool, zero_delay: bool) -> i64 {
    if (delay && default < 0) || (delay && zero_delay && default <= 0) || (!delay && default <= 0) {
        // The setting is enforced whatever the timestamp says.
        return -default;
    }
    let bytes = text.as_bytes();
    let cookie = (0..bytes.len()).find_map(|at| {
        if bytes[at] != b'-' {
            return None;
        }
        let count = bytes[at + 1..].iter().take_while(|b| b.is_ascii_digit()).count();
        let unit = *bytes.get(at + 1 + count)?;
        let after = bytes.get(at + 2 + count);
        if count == 0 || !b"hdwmy".contains(&unit) || !matches!(after, None | Some(b'>' | b' ')) {
            return None;
        }
        let value: f64 = text[at + 1..at + 1 + count].parse().ok()?;
        let factor = match unit {
            b'd' => 1.0,
            b'w' => 7.0,
            b'm' => 30.4,
            b'y' => 365.25,
            _ => 0.041667,
        };
        Some((value * factor).floor() as i64)
    });
    cookie.unwrap_or(default)
}

/// A time of day found in text, with the byte range it was found at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeOfDay {
    pub hours: i64,
    pub minutes: i64,
    pub start: usize,
    pub end: usize,
}

impl TimeOfDay {
    /// Military time, `HHMM` as a number: what the agenda sorts by.
    pub fn military(self) -> i64 {
        self.hours * 100 + self.minutes
    }

    pub fn total_minutes(self) -> i64 {
        self.hours * 60 + self.minutes
    }
}

/// One time at byte `at`, in the form `org-get-time-of-day` accepts:
/// `H:MM`, `HH:MM`, either with `am`/`pm`, or `Ham`/`Hpm`. Word boundaries
/// are required on both sides.
fn time_at(text: &str, at: usize) -> Option<TimeOfDay> {
    if !word_start(text, at) {
        return None;
    }
    let bytes = text.as_bytes();
    let is_ampm = |at: usize| {
        bytes.get(at..at + 2).filter(|s| matches!(s[0], b'a' | b'A' | b'p' | b'P') && matches!(s[1], b'm' | b'M'))
    };
    // `[012]?[0-9]`: try two digits first, as the regexp does.
    let lengths: &[usize] = if matches!(bytes.get(at), Some(b'0'..=b'2')) { &[2, 1] } else { &[1] };
    lengths.iter().find_map(|len| {
        let hour = digits(bytes, at, *len)?;
        let after = at + len;
        let (minutes, mut end) = match bytes.get(after) {
            Some(b':') => {
                let minutes = digits(bytes, after + 1, 2).filter(|_| matches!(bytes[after + 1], b'0'..=b'5'))?;
                (Some(minutes), after + 3)
            }
            _ => (None, after),
        };
        let ampm = is_ampm(end);
        if ampm.is_some() {
            end += 2;
        }
        let try_match = |ampm: Option<&[u8]>, end: usize| {
            if minutes.is_none() && ampm.is_none() {
                return None;
            }
            if !word_end(text, end) {
                return None;
            }
            let hours = match ampm {
                None => hour,
                Some(marker) => {
                    let am = matches!(marker[0], b'a' | b'A');
                    match hour {
                        12 if am => 0,
                        12 => 12,
                        hour if am => hour,
                        hour => hour + 12,
                    }
                }
            };
            Some(TimeOfDay { hours, minutes: minutes.unwrap_or(0), start: at, end })
        };
        // The am/pm part is optional after minutes: fall back to without it.
        try_match(ampm, end).or_else(|| if ampm.is_some() { try_match(None, end - 2) } else { None })
    })
}

/// `org-get-time-of-day`: the first time of day in `text`.
pub fn time_of_day(text: &str) -> Option<TimeOfDay> {
    text.char_indices().find_map(|(at, _)| time_at(text, at))
}

/// `org-plain-time-of-day-regexp`: the first time in `text`, and the
/// second one when it is written as a range (`8am-13:15`, `9:00--10:00`).
/// The full match spans both.
pub fn plain_time_range(text: &str) -> Option<(TimeOfDay, Option<TimeOfDay>)> {
    text.char_indices().find_map(|(at, _)| {
        let first = plain_time_at(text, at)?;
        let bytes = text.as_bytes();
        let second = [2, 1].into_iter().find_map(|dashes| {
            let all_dashes = bytes.get(first.end..first.end + dashes)?.iter().all(|b| *b == b'-');
            if !all_dashes {
                return None;
            }
            plain_time_at(text, first.end + dashes)
        });
        Some((first, second))
    })
}

/// One time in the stricter form of `org-plain-time-of-day-regexp`, where
/// `am`/`pm` directly after minutes is part of the same word.
fn plain_time_at(text: &str, at: usize) -> Option<TimeOfDay> {
    time_at(text, at)
}

/// `org-stamp-time-of-day-regexp`: an active timestamp with a two-digit
/// hour, and its end time when it has one (`<… 10:00-11:30>` or
/// `<… 10:00>--<… 11:30>` on the same day).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StampTime {
    /// The whole match, as text.
    pub matched: (usize, usize),
    pub start: (usize, usize),
    pub end: Option<(usize, usize)>,
}

pub fn stamp_time_range(text: &str) -> Option<StampTime> {
    let bytes = text.as_bytes();
    let hhmm = |at: usize| -> Option<(usize, usize)> {
        let slice = bytes.get(at..at + 5)?;
        let ok = matches!(slice[0], b'0'..=b'2')
            && slice[1].is_ascii_digit()
            && slice[2] == b':'
            && matches!(slice[3], b'0'..=b'5')
            && slice[4].is_ascii_digit();
        ok.then_some((at, at + 5))
    };
    (0..bytes.len()).find_map(|open| {
        if bytes[open] != b'<' {
            return None;
        }
        date_at(bytes, open + 1)?;
        // ` +\sw+ +`: blanks, a day name, blanks.
        let mut at = open + 11;
        let blanks = bytes[at..].iter().take_while(|b| **b == b' ').count();
        let name: usize =
            text[at + blanks..].chars().take_while(|c| crate::text::is_word(*c)).map(char::len_utf8).sum();
        at += blanks + name;
        let blanks2 = bytes[at..].iter().take_while(|b| **b == b' ').count();
        if blanks == 0 || name == 0 || blanks2 == 0 {
            return None;
        }
        at += blanks2;
        let prefix = &text[open + 1..at];
        let start = hhmm(at)?;
        at = start.1;
        let mut end = None;
        if bytes.get(at) == Some(&b'-') {
            if let Some(second) = hhmm(at + 1) {
                end = Some(second);
                at = second.1;
            }
        }
        // `[^\n\r>]*?>`
        let close = at + bytes[at..].iter().position(|b| matches!(b, b'\n' | b'\r' | b'>'))?;
        if bytes[close] != b'>' {
            return None;
        }
        let matched_end = close + 1;
        // `\(--?<\1\([012][0-9]:[0-5][0-9]\)>\)?`
        let range = [2, 1].into_iter().find_map(|dashes| {
            let rest = text.get(matched_end..)?;
            let rest = rest.strip_prefix(&"--"[..dashes])?.strip_prefix('<')?.strip_prefix(prefix)?;
            let at = text.len() - rest.len();
            let second = hhmm(at)?;
            (bytes.get(second.1) == Some(&b'>')).then_some(second)
        });
        if let Some(second) = range {
            // The end time inside the first timestamp wins when both exist.
            return Some(StampTime { matched: (open, second.1 + 1), start, end: end.or(Some(second)) });
        }
        Some(StampTime { matched: (open, matched_end), start, end })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i64, m: i64, d: i64) -> Day {
        Date::new(y, m, d).to_day()
    }

    #[test]
    fn parses_time_strings() {
        let parsed = parse_time_string("2026-10-05 Mon 09:30 +1w").unwrap();
        assert_eq!((parsed.date, parsed.time), (Date::new(2026, 10, 5), Some((9, 30))));
        assert_eq!(parse_time_string("<2026-10-05>").unwrap().time, None);
        assert_eq!(parse_time_string("2026-10-05 9:05").unwrap().time, Some((9, 5)));
        assert_eq!(parse_time_string("2026-10-05 Mon +1w").unwrap().time, None);
        assert_eq!(parse_time_string("x-52026-10-05").unwrap().date, Date::new(2026, 10, 5));
        assert_eq!(parse_time_string("no date"), None);
        assert_eq!(time_string_to_day("2026-10-05 Mon 25:00"), Some(day(2026, 10, 6)));
        assert_eq!(time_string_to_day("2026-02-30"), Some(day(2026, 3, 2)));
    }

    #[test]
    fn finds_repeaters_and_warnings() {
        assert_eq!(repeater("2026-10-05 Mon +1w"), Some(Interval { value: 1, unit: b'w' }));
        assert_eq!(repeater("2026-10-05 Mon .+2d -3d"), Some(Interval { value: 2, unit: b'd' }));
        assert_eq!(repeater("2026-10-05 Mon ++12m"), Some(Interval { value: 12, unit: b'm' }));
        assert_eq!(repeater("2026-10-05 Mon"), None);
        assert_eq!(warning_days("2026-10-05 Mon -3d", 14, false, false), 3);
        assert_eq!(warning_days("2026-10-05 Mon -2w", 14, false, false), 14);
        assert_eq!(warning_days("2026-10-05 Mon", 14, false, false), 14);
        assert_eq!(warning_days("2026-10-05 Mon -1m>", 14, false, false), 30);
        assert_eq!(warning_days("2026-10-05 Mon -3dx", 14, false, false), 14);
        assert_eq!(warning_days("2026-10-05 Mon -3d", 0, false, false), 0);
        assert_eq!(warning_days("2026-10-05 Mon -3d", -5, false, false), 5);
        assert_eq!(warning_days("2026-10-05 Mon -3d", 0, true, false), 3);
        assert_eq!(warning_days("2026-10-05 Mon -3d", 0, true, true), 0);
    }

    #[test]
    fn closest_dates() {
        let weekly = "2026-09-01 Tue +1w";
        assert_eq!(closest_date(weekly, day(2026, 10, 5), Prefer::Past), Some(day(2026, 9, 29)));
        assert_eq!(closest_date(weekly, day(2026, 10, 5), Prefer::Future), Some(day(2026, 10, 6)));
        assert_eq!(closest_date(weekly, day(2026, 10, 6), Prefer::Past), Some(day(2026, 10, 6)));
        assert_eq!(closest_date(weekly, day(2026, 10, 6), Prefer::Future), Some(day(2026, 10, 6)));
        assert_eq!(closest_date(weekly, day(2026, 8, 1), Prefer::Future), Some(day(2026, 9, 1)));
        assert_eq!(closest_date("2026-09-01 Tue", day(2026, 10, 5), Prefer::Past), Some(day(2026, 9, 1)));
        assert_eq!(closest_date("2026-09-01 Tue +0d", day(2026, 10, 5), Prefer::Past), Some(day(2026, 9, 1)));
        let monthly = "2026-01-31 Sat +1m";
        assert_eq!(closest_date(monthly, day(2026, 3, 15), Prefer::Past), Some(day(2026, 3, 3)));
        let yearly = "2020-02-29 Sat +1y";
        assert_eq!(closest_date(yearly, day(2026, 10, 5), Prefer::Past), Some(day(2026, 3, 1)));
        assert_eq!(closest_date(yearly, day(2026, 10, 5), Prefer::Future), Some(day(2027, 3, 1)));
    }

    #[test]
    fn times_of_day() {
        let military = |text: &str| time_of_day(text).map(TimeOfDay::military);
        assert_eq!(military("at 10:30 sharp"), Some(1030));
        assert_eq!(military("9:05"), Some(905));
        assert_eq!(military("call 3pm"), Some(1500));
        assert_eq!(military("12am"), Some(0));
        assert_eq!(military("12pm"), Some(1200));
        assert_eq!(military("8:15PM"), Some(2015));
        assert_eq!(military("x10:30"), None);
        assert_eq!(military("10:3"), None);
        assert_eq!(military("10:70"), None);
        assert_eq!(military("34:00"), None);
        assert_eq!(military("version 2 of 3"), None);
        assert_eq!(military("10:30am5"), None);
        assert_eq!(military("24:30"), Some(2430));
    }

    #[test]
    fn plain_ranges() {
        fn range(text: &str) -> Option<(i64, Option<i64>, &str)> {
            let (first, second) = plain_time_range(text)?;
            let end = second.map_or(first.end, |second| second.end);
            Some((first.military(), second.map(TimeOfDay::military), &text[first.start..end]))
        }
        assert_eq!(range("Meet 10:00-11:30 here"), Some((1000, Some(1130), "10:00-11:30")));
        assert_eq!(range("8am--13:15"), Some((800, Some(1315), "8am--13:15")));
        assert_eq!(range("10:00 - 11:00"), Some((1000, None, "10:00")));
        assert_eq!(range("nothing"), None);
    }

    #[test]
    fn stamp_ranges() {
        let text = "x <2026-10-06 Tue 14:00-15:30> y";
        let stamp = stamp_time_range(text).unwrap();
        assert_eq!(&text[stamp.matched.0..stamp.matched.1], "<2026-10-06 Tue 14:00-15:30>");
        assert_eq!(&text[stamp.start.0..stamp.start.1], "14:00");
        assert_eq!(stamp.end.map(|(a, b)| &text[a..b]), Some("15:30"));

        let text = "<2026-10-06 Tue 14:00 +1w>--<2026-10-06 Tue 16:00>";
        let stamp = stamp_time_range(text).unwrap();
        assert_eq!(stamp.matched, (0, text.len()));
        assert_eq!(stamp.end.map(|(a, b)| &text[a..b]), Some("16:00"));

        assert_eq!(stamp_time_range("<2026-10-06 Tue 9:00>"), None);
        assert_eq!(stamp_time_range("<2026-10-06 14:00>"), None);
        assert_eq!(stamp_time_range("[2026-10-06 Tue 14:00]"), None);
    }

    #[test]
    fn repeats_by_a_repeater_too_long_for_any_calendar() {
        for unit in ["h", "d", "w", "m", "y"] {
            let start = format!("<2026-01-01 Thu +999999999999999999{unit}>");
            let _ = closest_date(&start, Date::new(2026, 10, 12).to_day(), Prefer::Nearest);
        }
    }
}
