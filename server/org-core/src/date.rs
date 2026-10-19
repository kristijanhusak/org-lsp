//! Calendar arithmetic on day numbers.
//!
//! A day is counted the way Emacs's calendar does: day 1 is 0001-01-01 in
//! the proleptic Gregorian calendar, and `day % 7` is the weekday with
//! Sunday as 0. Org's agenda logic works on these numbers throughout.

use std::fmt;

/// An absolute day number.
pub type Day = i64;

/// A calendar date. Fields are not required to be in range: converting to
/// a day number normalises them the way `encode-time` does, so month 13 is
/// January of the next year and day 0 is the last day of the previous month.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub year: i64,
    pub month: i64,
    pub day: i64,
}

pub const WEEKDAY_NAMES: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
pub const WEEKDAY_ABBREVIATIONS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
pub const MONTH_NAMES: [&str; 12] = [
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

pub fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Number of days in `month` (1 to 12) of `year`.
pub fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if is_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

impl Date {
    pub fn new(year: i64, month: i64, day: i64) -> Date {
        Date { year, month, day }
    }

    /// The day number of this date, normalising out-of-range fields.
    pub fn to_day(self) -> Day {
        let months = self.year * 12 + (self.month - 1);
        let (year, month) = (months.div_euclid(12), months.rem_euclid(12) + 1);
        // Days before the first of this month, counted from 0001-01-01.
        let before_year =
            365 * (year - 1) + (year - 1).div_euclid(4) - (year - 1).div_euclid(100) + (year - 1).div_euclid(400);
        let before_month: i64 = (1..month).map(|m| days_in_month(year, m)).sum();
        before_year + before_month + self.day
    }

    pub fn from_day(day: Day) -> Date {
        // Shift to a 400-year cycle that starts on 0001-01-01 (day 1).
        let d0 = day - 1;
        let n400 = d0.div_euclid(146_097);
        let d1 = d0.rem_euclid(146_097);
        let n100 = (d1 / 36_524).min(3);
        let d2 = d1 - n100 * 36_524;
        let n4 = d2 / 1_461;
        let d3 = d2 % 1_461;
        let n1 = (d3 / 365).min(3);
        let day_of_year = d3 - n1 * 365;
        let year = 400 * n400 + 100 * n100 + 4 * n4 + n1 + 1;
        let mut month = 1;
        let mut rest = day_of_year;
        while rest >= days_in_month(year, month) {
            rest -= days_in_month(year, month);
            month += 1;
        }
        Date { year, month, day: rest + 1 }
    }

    /// Whether every field is in range for a real calendar date.
    pub fn is_valid(self) -> bool {
        (1..=12).contains(&self.month) && self.day >= 1 && self.day <= days_in_month(self.year, self.month)
    }
}

impl fmt::Display for Date {
    /// `YYYY-MM-DD`, the form used inside Org timestamps.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// Weekday of a day number: 0 is Sunday.
pub fn weekday(day: Day) -> usize {
    day.rem_euclid(7) as usize
}

/// The ISO 8601 week number and week-year of a day.
pub fn iso_week(day: Day) -> (i64, i64) {
    // ISO weeks start on Monday; week 1 holds the year's first Thursday.
    let monday_based = (weekday(day) as i64 + 6) % 7;
    let thursday = day - monday_based + 3;
    let year = Date::from_day(thursday).year;
    let first = Date::new(year, 1, 1).to_day();
    ((thursday - first) / 7 + 1, year)
}

/// A point in time: a day and the minutes since midnight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DateTime {
    pub day: Day,
    pub minute: i64,
}

impl DateTime {
    pub fn new(date: Date, hour: i64, minute: i64) -> DateTime {
        let total = hour * 60 + minute;
        DateTime { day: date.to_day() + total.div_euclid(1440), minute: total.rem_euclid(1440) }
    }

    pub fn date(self) -> Date {
        Date::from_day(self.day)
    }

    pub fn hour(self) -> i64 {
        self.minute / 60
    }

    /// `org-today`: the day it still is `extend_today_until` hours past
    /// midnight (`org-extend-today-until`).
    pub fn org_today(self, extend_today_until: i64) -> Day {
        (self.day * 1440 + self.minute - 60 * extend_today_until).div_euclid(1440)
    }

    /// Before `extend_today_until` o'clock, the last minute of yesterday,
    /// as `org-read-date` and `org-capture-fill-template` take the time to
    /// be then; otherwise this time.
    pub fn late_night(self, extend_today_until: i64) -> DateTime {
        if self.hour() < extend_today_until {
            DateTime { day: self.day - 1, minute: 23 * 60 + 59 }
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_numbers_match_emacs() {
        // (calendar-absolute-from-gregorian '(month day year))
        assert_eq!(Date::new(1, 1, 1).to_day(), 1);
        assert_eq!(Date::new(1970, 1, 1).to_day(), 719_163);
        assert_eq!(Date::new(2026, 10, 5).to_day(), 739_894);
        assert_eq!(Date::new(2000, 2, 29).to_day(), 730_179);
        assert_eq!(weekday(Date::new(2026, 10, 5).to_day()), 1);
        assert_eq!(weekday(Date::new(1970, 1, 1).to_day()), 4);
    }

    #[test]
    fn round_trips() {
        for day in (700_000..760_000).chain(-800..800).chain(140_000..150_000) {
            let date = Date::from_day(day);
            assert!(date.is_valid(), "{day} -> {date}");
            assert_eq!(date.to_day(), day);
        }
        // Consecutive days are consecutive dates.
        let mut previous = Date::from_day(730_000);
        for day in 730_001..731_500 {
            let date = Date::from_day(day);
            assert!(date > previous);
            previous = date;
        }
    }

    #[test]
    fn out_of_range_fields_are_normalised() {
        assert_eq!(Date::from_day(Date::new(2026, 2, 30).to_day()), Date::new(2026, 3, 2));
        assert_eq!(Date::from_day(Date::new(2026, 13, 1).to_day()), Date::new(2027, 1, 1));
        assert_eq!(Date::from_day(Date::new(2026, 0, 1).to_day()), Date::new(2025, 12, 1));
        assert_eq!(Date::from_day(Date::new(2026, 3, 0).to_day()), Date::new(2026, 2, 28));
        assert_eq!(Date::from_day(Date::new(2026, 1, 366).to_day()), Date::new(2027, 1, 1));
        assert!(!Date::new(2026, 2, 30).is_valid());
        assert!(!Date::new(2026, 13, 1).is_valid());
    }

    #[test]
    fn iso_weeks() {
        let week = |y, m, d| iso_week(Date::new(y, m, d).to_day());
        assert_eq!(week(2026, 10, 5), (41, 2026));
        assert_eq!(week(2026, 1, 1), (1, 2026));
        assert_eq!(week(2027, 1, 1), (53, 2026));
        assert_eq!(week(2024, 12, 30), (1, 2025));
        assert_eq!(week(2021, 1, 3), (53, 2020));
    }

    #[test]
    fn time_overflow_moves_the_day() {
        let base = Date::new(2026, 10, 5);
        assert_eq!(DateTime::new(base, 25, 30), DateTime { day: base.to_day() + 1, minute: 90 });
        assert_eq!(DateTime::new(base, 9, 99).minute, 10 * 60 + 39);
        assert_eq!(DateTime::new(base, 0, 0).hour(), 0);
    }
}
