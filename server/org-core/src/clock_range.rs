//! `org-clock-special-range`: the two times around a named span — today,
//! last week, this month, a quarter, a year — for `org-clock-display`
//! and clock tables (`:block`).

use crate::date::{iso_week, weekday, Date, DateTime, Day};

/// A span of time: from `start` (none for `untilnow`) to `end`, and how
/// clock tables name it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Range {
    pub start: Option<DateTime>,
    pub end: DateTime,
    pub text: String,
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WEEKDAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Key {
    Day,
    Week,
    Month,
    Quarter,
    Year,
    UntilNow,
}

/// The time of `day`, `hour` and `minute` with every field allowed out of
/// range, as `encode-time` takes them.
fn encode(year: i64, month: i64, day: i64, hour: i64, minute: i64) -> DateTime {
    let first = Date::new(year, month, 1).to_day();
    let minutes = (first + day - 1) * 1440 + hour * 60 + minute;
    DateTime { day: minutes.div_euclid(1440), minute: minutes.rem_euclid(1440) }
}

/// The first day of ISO week `week` of `year`.
fn iso_week_start(year: i64, week: i64) -> Day {
    // The Monday of the week that holds January 4th is that of week 1.
    let fourth = Date::new(year, 1, 4).to_day();
    let monday = fourth - ((weekday(fourth) as i64 + 6) % 7);
    monday + (week - 1) * 7
}

fn ordinal(n: i64) -> &'static str {
    match n {
        1 => "1st",
        2 => "2nd",
        3 => "3rd",
        _ => "4th",
    }
}

/// `org-clock-special-range` for `key` around `now`: `today`,
/// `yesterday`, `thisweek`, `lastweek`, `thismonth`, `lastmonth`,
/// `thisq`, `lastq`, `thisyear`, `lastyear`, `untilnow`, those with a
/// shift (`thisweek-2`, `today+1`), or a year (`2026`), a month
/// (`2026-05`), an ISO week (`2026-W21`), a quarter (`2026-Q2`) or a day
/// (`2026-05-18`). A week starts on `week_start` (1 is Monday), a month
/// on `month_start`; a day at `extend_today_until` o'clock.
pub fn special_range(
    key: &str,
    now: DateTime,
    week_start: Option<i64>,
    month_start: Option<i64>,
    extend_today_until: i64,
) -> Result<Range, String> {
    let today = Date::from_day(now.day);
    let (mut y, mut month, mut d) = (today.year, today.month, today.day);
    let mut dow = weekday(now.day) as i64;
    let mut q = (month - 1) / 3 + 1;
    let mut shift = 0;
    let mut name = key.to_string();
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
    let parts: Vec<&str> = key.split('-').collect();
    let named = if digits(key) {
        y = key.parse().map_err(|_| format!("No such time block {key}"))?;
        month = 1;
        d = 1;
        Some(Key::Year)
    } else if parts.len() == 2 && digits(parts[0]) && digits(parts[1]) && parts[1].len() <= 2 {
        y = parts[0].parse().unwrap_or(y);
        month = parts[1].parse().unwrap_or(1);
        d = 1;
        Some(Key::Month)
    } else if parts.len() == 2
        && digits(parts[0])
        && (parts[1].starts_with('w') || parts[1].starts_with('W'))
        && digits(&parts[1][1..])
        && parts[1].len() <= 3
    {
        let start = Date::from_day(iso_week_start(parts[0].parse().unwrap_or(y), parts[1][1..].parse().unwrap_or(1)));
        (y, month, d, dow) = (start.year, start.month, start.day, 1);
        Some(Key::Week)
    } else if parts.len() == 2
        && digits(parts[0])
        && (parts[1] == "q1"
            || parts[1] == "q2"
            || parts[1] == "q3"
            || parts[1] == "q4"
            || parts[1] == "Q1"
            || parts[1] == "Q2"
            || parts[1] == "Q3"
            || parts[1] == "Q4")
    {
        q = parts[1][1..].parse().unwrap_or(1);
        y = parts[0].parse().unwrap_or(y);
        month = 3 * (q - 1) + 1;
        d = 1;
        dow = 1;
        Some(Key::Quarter)
    } else if parts.len() == 3 && parts.iter().all(|part| digits(part)) && parts[1].len() <= 2 && parts[2].len() <= 2 {
        y = parts[0].parse().unwrap_or(y);
        month = parts[1].parse().unwrap_or(1);
        d = parts[2].parse().unwrap_or(1);
        Some(Key::Day)
    } else {
        // A shift at the end: `thisweek-1`, `today+2`.
        if let Some(at) = key.rfind(['-', '+']).filter(|at| *at > 0 && digits(&key[at + 1..])) {
            shift = key[at..].parse().map_err(|_| format!("No such time block {key}"))?;
            name = key[..at].to_string();
            if matches!(name.as_str(), "quarter" | "thisq") && shift > 0 {
                return Err("Looking forward with quarters isn't implemented".to_string());
            }
        }
        None
    };
    let key = match named {
        Some(key) => key,
        None => {
            if shift == 0 {
                (name, shift) = match name.as_str() {
                    "yesterday" => ("today".to_string(), -1),
                    "lastweek" => ("week".to_string(), -1),
                    "lastmonth" => ("month".to_string(), -1),
                    "lastyear" => ("year".to_string(), -1),
                    "lastq" => ("quarter".to_string(), -1),
                    _ => (name, 0),
                };
            }
            match name.as_str() {
                "day" | "today" => Key::Day,
                "week" | "thisweek" => Key::Week,
                "month" | "thismonth" => Key::Month,
                "quarter" | "thisq" => Key::Quarter,
                "year" | "thisyear" => Key::Year,
                "untilnow" => Key::UntilNow,
                _ => return Err(format!("No such time block {key}")),
            }
        }
    };
    let h = extend_today_until;
    let (start, end, shown_year, shown_q) = match key {
        Key::Day => {
            let d = d + shift;
            (encode(y, month, d, h, 0), encode(y, month, d, h + 24, 0), y, q)
        }
        Key::Week => {
            let ws = week_start.unwrap_or(1);
            let diff = -7 * shift + (dow + 7 - ws).rem_euclid(7);
            let d = d - diff;
            (encode(y, month, d, h, 0), encode(y, month, d + 7, h, 0), y, q)
        }
        Key::Month => {
            let d = month_start.unwrap_or(1);
            let month = month + shift;
            (encode(y, month, d, h, 0), encode(y, month + 1, d, h, 0), y, q)
        }
        Key::Quarter => {
            if q - 1 + shift < 0 {
                let interval = -(q - 1 + shift);
                let (whole, rest) = (interval.div_euclid(4), interval.rem_euclid(4));
                let (sy, sm, sq) =
                    if rest == 0 { (y - whole, 1, 1) } else { (y - (1 + whole), 13 - 3 * rest, 5 - rest) };
                (encode(sy, sm, 1, h, 0), encode(sy, sm + 3, 1, h, 0), sy, sq)
            } else {
                let sq = q + shift;
                let qshift = 3 * (sq - 1);
                (encode(y, 1 + qshift, 1, h, 0), encode(y, 4 + qshift, 1, h, 0), y, sq)
            }
        }
        Key::Year => {
            let y = y + shift;
            (encode(y, 1, 1, h, 0), encode(y + 1, 1, 1, h, 0), y, q)
        }
        Key::UntilNow => {
            return Ok(Range { start: None, end: now, text: "now".to_string() });
        }
    };
    let date = Date::from_day(start.day);
    let text = match key {
        Key::Day => format!(
            "{}, {} {:02}, {}",
            WEEKDAYS[weekday(start.day)],
            MONTHS[(date.month - 1) as usize],
            date.day,
            date.year
        ),
        Key::Week => {
            let (week, year) = iso_week(start.day);
            format!("week {year}-W{week:02}")
        }
        Key::Month => format!("{} {}", MONTHS[(date.month - 1) as usize], date.year),
        Key::Year => format!("the year {}", date.year),
        Key::Quarter => format!("{} quarter of {shown_year}", ordinal(shown_q)),
        Key::UntilNow => unreachable!(),
    };
    let _ = shown_year;
    Ok(Range { start: Some(start), end, text })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(year: i64, month: i64, day: i64) -> DateTime {
        DateTime { day: Date::new(year, month, day).to_day(), minute: 630 }
    }

    fn span(key: &str) -> (String, String, String) {
        let range = special_range(key, at(2026, 5, 20), None, None, 0).unwrap();
        let show = |time: DateTime| {
            let date = Date::from_day(time.day);
            format!("{date} {:02}:{:02}", time.minute / 60, time.minute % 60)
        };
        (range.start.map(show).unwrap_or_default(), show(range.end), range.text)
    }

    #[test]
    fn names_spans_around_a_day() {
        assert_eq!(
            span("today"),
            ("2026-05-20 00:00".into(), "2026-05-21 00:00".into(), "Wednesday, May 20, 2026".into())
        );
        assert_eq!(span("yesterday").0, "2026-05-19 00:00");
        assert_eq!(span("thisweek"), ("2026-05-18 00:00".into(), "2026-05-25 00:00".into(), "week 2026-W21".into()));
        assert_eq!(span("lastweek").0, "2026-05-11 00:00");
        assert_eq!(span("thismonth"), ("2026-05-01 00:00".into(), "2026-06-01 00:00".into(), "May 2026".into()));
        assert_eq!(span("lastmonth").0, "2026-04-01 00:00");
        assert_eq!(span("thisyear"), ("2026-01-01 00:00".into(), "2027-01-01 00:00".into(), "the year 2026".into()));
        assert_eq!(span("thisq"), ("2026-04-01 00:00".into(), "2026-07-01 00:00".into(), "2nd quarter of 2026".into()));
        assert_eq!(span("lastq").2, "1st quarter of 2026");
        assert_eq!(span("thisq-2").2, "4th quarter of 2025");
        assert_eq!(span("2025-W01").0, "2024-12-30 00:00");
        assert_eq!(span("2026-03").2, "March 2026");
        assert_eq!(span("2026-03-04").2, "Wednesday, March 04, 2026");
        assert!(special_range("someday", at(2026, 5, 20), None, None, 0).is_err());
    }
}
