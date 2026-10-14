//! Which TODO entries the global TODO list leaves out for a date of their
//! own (`org-agenda-todo-ignore-with-date`, `-scheduled`, `-deadlines`,
//! `-timestamp`), as `org-agenda-check-for-timestamp-as-reason-to-ignore-todo-item`
//! decides it: by regular expressions over the text of the entry, from its
//! headline to the next one.

use std::sync::LazyLock;

use regex::Regex;

use super::Now;
use crate::timestr::{parse_time_string, warning_days};

/// When a date of an entry leaves it out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    /// `future`: a date after today.
    Future,
    /// `past`: today or before.
    Past,
    /// A number of days N: N days from now or more, or with a negative N,
    /// N days ago or before.
    Days(i64),
    /// `all`: any date (and for deadlines, the only value that means it).
    All,
    /// `far`: a deadline not yet in its warning period.
    Far,
    /// Any other value, `t`: any scheduled date or timestamp, a deadline in
    /// its warning period.
    Other,
}

impl When {
    /// The value of the option, as JSON: `true`, a name or a number.
    pub fn parse(value: &serde_json::Value) -> Option<When> {
        match value {
            serde_json::Value::Bool(true) => Some(When::Other),
            serde_json::Value::Number(number) => number.as_i64().map(When::Days),
            serde_json::Value::String(name) => Some(match name.as_str() {
                "future" => When::Future,
                "past" => When::Past,
                "all" => When::All,
                "far" => When::Far,
                _ => When::Other,
            }),
            _ => None,
        }
    }
}

/// The `org-agenda-todo-ignore-*` options.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TodoIgnore {
    /// `org-agenda-todo-ignore-with-date`
    pub with_date: bool,
    /// `org-agenda-todo-ignore-scheduled`
    pub scheduled: Option<When>,
    /// `org-agenda-todo-ignore-deadlines`
    pub deadlines: Option<When>,
    /// `org-agenda-todo-ignore-timestamp`
    pub timestamp: Option<When>,
    /// `org-agenda-todo-ignore-time-comparison-use-seconds`
    pub use_seconds: bool,
}

impl TodoIgnore {
    fn any(&self) -> bool {
        self.with_date || self.scheduled.is_some() || self.deadlines.is_some() || self.timestamp.is_some()
    }
}

/// `org-ts-regexp`: an active timestamp, its contents the first group.
static TIMESTAMP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<([0-9]{4}-[0-9]{2}-[0-9]{2}(?: .*?)?)>").expect("a valid regex"));
/// `org-scheduled-time-regexp`
static SCHEDULED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bSCHEDULED: *<([^>]+)>").expect("a valid regex"));
/// `org-deadline-time-regexp`
static DEADLINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bDEADLINE: *<([^>]+)>").expect("a valid regex"));
/// Either of them.
static PLANNING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bSCHEDULED: *<[^>]+>|\bDEADLINE: *<[^>]+>").expect("a valid regex"));

/// `org-timestamp-to-now`: how far `stamp` is from now, in days, or in
/// seconds with `seconds`. None for a stamp Org cannot read.
fn to_now(stamp: &str, now: Now, seconds: bool) -> Option<i64> {
    let parsed = parse_time_string(stamp)?;
    let (hour, minute) = parsed.time.unwrap_or((0, 0));
    let at = crate::date::DateTime::new(parsed.date, hour, minute);
    Some(if seconds { (at.day - now.today) * 86_400 + (at.minute - now.minute) * 60 } else { at.day - now.today })
}

/// `future`, `past` and a number of days, for `stamp`; any other value
/// leaves the entry out.
fn by_time(when: When, stamp: &str, now: Now, seconds: bool) -> bool {
    let Some(distance) = to_now(stamp, now, seconds) else { return false };
    match when {
        When::Future => distance > 0,
        When::Past => distance <= 0,
        // `org-agenda-todo-custom-ignore-p`
        When::Days(n) if n >= 0 => distance >= n,
        When::Days(n) => distance <= n,
        _ => true,
    }
}

/// `org-deadline-close-p`: the deadline `stamp` is in its warning period,
/// and the entry is not done.
fn deadline_close(stamp: &str, now: Now, warning_default: i64, done: bool) -> bool {
    let days = warning_days(stamp, warning_default, false, false);
    to_now(stamp, now, false).is_some_and(|distance| distance <= days) && !done
}

/// Whether the entry whose text is `text` (from its headline to the next
/// one) is left out of the TODO list for a date. `done` is whether its
/// keyword is a done one; `warning_default` is `org-deadline-warning-days`.
pub fn ignored(text: &str, done: bool, settings: &TodoIgnore, now: Now, warning_default: i64) -> bool {
    if !settings.any() {
        return false;
    }
    let seconds = settings.use_seconds;
    // One cursor for every search, as in Emacs: a search that finds a
    // date but does not leave the entry out moves it on.
    let mut at = 0;
    if settings.with_date && TIMESTAMP.is_match(text) {
        return true;
    }
    if let Some(when) = settings.scheduled {
        if let Some(found) = SCHEDULED.captures_at(text, at) {
            at = found.get(0).expect("the whole match").end();
            if by_time(when, &found[1], now, seconds) {
                return true;
            }
        }
    }
    if let Some(when) = settings.deadlines {
        if let Some(found) = DEADLINE.captures_at(text, at) {
            at = found.get(0).expect("the whole match").end();
            let stamp = &found[1];
            let ignore = match when {
                When::All => true,
                When::Far => !deadline_close(stamp, now, warning_default, done),
                When::Future | When::Past | When::Days(_) => by_time(when, stamp, now, seconds),
                When::Other => deadline_close(stamp, now, warning_default, done),
            };
            if ignore {
                return true;
            }
        }
    }
    if let Some(when) = settings.timestamp {
        // The rest of the entry without its SCHEDULED and DEADLINE dates.
        let rest = PLANNING.replace_all(&text[at..], "");
        if let Some(found) = TIMESTAMP.captures(&rest) {
            return by_time(when, &found[1], now, seconds);
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::date::Date;

    fn now() -> Now {
        Now { today: Date::new(2026, 5, 18).to_day(), minute: 600 }
    }

    fn ignored_by(text: &str, settings: TodoIgnore) -> bool {
        ignored(text, false, &settings, now(), 14)
    }

    #[test]
    fn scheduled_dates_by_time() {
        let future = TodoIgnore { scheduled: Some(When::Future), ..TodoIgnore::default() };
        assert!(ignored_by("* TODO a\nSCHEDULED: <2026-05-19 Tue>\n", future.clone()));
        assert!(!ignored_by("* TODO a\nSCHEDULED: <2026-05-18 Mon>\n", future.clone()));
        let past = TodoIgnore { scheduled: Some(When::Past), ..TodoIgnore::default() };
        assert!(ignored_by("* TODO a\nSCHEDULED: <2026-05-18 Mon>\n", past));
        let days = TodoIgnore { scheduled: Some(When::Days(2)), ..TodoIgnore::default() };
        assert!(ignored_by("* TODO a\nSCHEDULED: <2026-05-20 Wed>\n", days.clone()));
        assert!(!ignored_by("* TODO a\nSCHEDULED: <2026-05-19 Tue>\n", days));
        let seconds = TodoIgnore { scheduled: Some(When::Future), use_seconds: true, ..TodoIgnore::default() };
        assert!(ignored_by("* TODO a\nSCHEDULED: <2026-05-18 Mon 10:01>\n", seconds));
    }

    #[test]
    fn deadlines_near_and_far() {
        let near = TodoIgnore { deadlines: Some(When::Other), ..TodoIgnore::default() };
        assert!(ignored_by("* TODO a\nDEADLINE: <2026-05-30 Sat>\n", near.clone()));
        assert!(!ignored_by("* TODO a\nDEADLINE: <2026-06-30 Tue>\n", near.clone()));
        assert!(!ignored("* DONE a\nDEADLINE: <2026-05-30 Sat>\n", true, &near, now(), 14));
        let far = TodoIgnore { deadlines: Some(When::Far), ..TodoIgnore::default() };
        assert!(!ignored_by("* TODO a\nDEADLINE: <2026-06-30 Tue -60d>\n", far.clone()));
        assert!(ignored_by("* TODO a\nDEADLINE: <2026-06-30 Tue>\n", far));
    }

    #[test]
    fn timestamps_leave_planning_out_and_the_cursor_moves_on() {
        let any = TodoIgnore { timestamp: Some(When::Other), ..TodoIgnore::default() };
        assert!(!ignored_by("* TODO a\nSCHEDULED: <2026-05-19 Tue>\n", any.clone()));
        assert!(ignored_by("* TODO a <2026-05-19 Tue>\n", any));
        // The scheduled date is not in the future: the timestamp search
        // starts after it, past the timestamp before it.
        let both = TodoIgnore { scheduled: Some(When::Future), timestamp: Some(When::Other), ..TodoIgnore::default() };
        assert!(!ignored_by("* TODO a <2026-05-01 Fri>\nSCHEDULED: <2026-05-10 Sun>\n", both));
    }
}
