//! `org-habit`: entries with the `STYLE` property `habit`, a scheduled
//! date with a repeater (`.+2d`, or `.+2d/4d` for a deadline too), shown
//! in the agenda with a graph of how consistently they were done.

use std::sync::LazyLock;

use regex::Regex;

use crate::date::Day;
use crate::timestr::time_string_to_day;

/// The `org-habit-…` options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// `org-habit-graph-column`
    pub graph_column: usize,
    /// `org-habit-preceding-days`
    pub preceding_days: i64,
    /// `org-habit-following-days`
    pub following_days: i64,
    /// `org-habit-show-habits`
    pub show_habits: bool,
    /// `org-habit-show-habits-only-for-today`
    pub show_habits_only_for_today: bool,
    /// `org-habit-show-all-today`
    pub show_all_today: bool,
    /// `org-habit-today-glyph`
    pub today_glyph: char,
    /// `org-habit-completed-glyph`
    pub completed_glyph: char,
    /// `org-habit-show-done-always-green`
    pub show_done_always_green: bool,
    /// `org-habit-scheduled-past-days`: none for `org-scheduled-past-days`.
    pub scheduled_past_days: Option<i64>,
    /// The `done` heading of `org-log-note-headings`, whose notes say a
    /// habit was done as `State "DONE"` notes do.
    pub done_heading: String,
    /// `org-log-states-order-reversed`: the latest note first.
    pub states_order_reversed: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            graph_column: 40,
            preceding_days: 21,
            following_days: 7,
            show_habits: true,
            show_habits_only_for_today: true,
            show_all_today: false,
            today_glyph: '!',
            completed_glyph: '*',
            show_done_always_green: false,
            scheduled_past_days: None,
            done_heading: "CLOSING NOTE %t".to_string(),
            states_order_reversed: true,
        }
    }
}

/// A habit as `org-habit-parse-todo` reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Habit {
    /// The scheduled date.
    pub scheduled: Day,
    /// The repeater's interval, in days.
    pub scheduled_repeat: i64,
    /// The deadline the `/4d` of the repeater sets, if it has one.
    pub deadline: Option<Day>,
    /// That interval, in days.
    pub deadline_repeat: Option<i64>,
    /// The days the entry was done, as its log notes say, the earliest
    /// first.
    pub done: Vec<Day>,
    /// `.+`, `++` or `+`.
    pub repeat_type: String,
}

/// The face of a day of the graph (`org-habit-clear-face` and the rest).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    Clear,
    ClearFuture,
    Ready,
    ReadyFuture,
    Alert,
    AlertFuture,
    Overdue,
    OverdueFuture,
}

/// `org-habit-duration-to-days`: the first `N[dwmy]` of `text`, in days.
fn duration_to_days(text: &str) -> Result<i64, String> {
    static DURATION: LazyLock<Regex> = LazyLock::new(|| Regex::new("([0-9]+)([dwmy])").expect("a valid regex"));
    let found = DURATION.captures(text).ok_or_else(|| format!("Invalid duration string: {text}"))?;
    let count: f64 = found[1].parse().unwrap_or(0.0);
    let days = match &found[2] {
        "d" => 1.0,
        "w" => 7.0,
        "m" => 30.4,
        _ => 365.25,
    };
    Ok((count * days).floor() as i64)
}

/// `org-get-repeat` of a timestamp: its repeater, with the deadline part.
fn repeater(stamp: &str) -> Option<&str> {
    static REPEAT: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"<[0-9]{4}-[0-9]{2}-[0-9]{2} [^>\n]*?([.+]?\+[0-9]+[hdwmy](?:/[0-9]+[hdwmy])?)")
            .expect("a valid regex")
    });
    REPEAT.captures(stamp).map(|found| found.get(1).expect("a group").as_str())
}

/// How the notes of a habit are read: the `done` heading of
/// `org-log-note-headings`, and whether the latest note is first
/// (`org-log-states-order-reversed`).
#[derive(Debug, Clone, Copy)]
pub struct Notes<'a> {
    pub done_heading: &'a str,
    pub reversed: bool,
}

/// `org-habit-parse-todo` of the entry whose text is `entry` (its headline
/// `head`), scheduled at `scheduled` (as written), the days it was done
/// read from its log notes: the first `limit` found from the headline
/// down, or with `notes.reversed` off, from its end up.
pub fn parse(
    scheduled: &str,
    head: &str,
    entry: &str,
    done_keywords: &[String],
    limit: usize,
    notes: Notes,
) -> Result<Habit, String> {
    let stamp = if scheduled.starts_with('<') { scheduled.to_string() } else { format!("<{scheduled}>") };
    let day = time_string_to_day(&stamp).ok_or_else(|| format!("Habit {head} has no scheduled date"))?;
    let repeat = repeater(&stamp)
        .ok_or_else(|| format!("Habit `{head}' has no scheduled repeat period or has an incorrect one"))?;
    let scheduled_repeat = duration_to_days(repeat)?;
    let repeat_type = if repeat.starts_with(".+") {
        ".+"
    } else if repeat.starts_with("++") {
        "++"
    } else {
        "+"
    };
    if scheduled_repeat <= 0 {
        return Err(format!("Habit {head} scheduled repeat period is less than 1d"));
    }
    let (deadline, deadline_repeat) = match repeat.split_once('/') {
        Some((_, deadline)) => {
            let days = duration_to_days(deadline)?;
            if days <= scheduled_repeat {
                return Err(format!(
                    "Habit {head} deadline repeat period is less than or equal to scheduled ({repeat})"
                ));
            }
            (Some(day + days - scheduled_repeat), Some(days))
        }
        None => (None, None),
    };
    let found = done_days(entry, done_keywords, notes.done_heading);
    let mut done: Vec<Day> = if notes.reversed {
        found.into_iter().take(limit).collect()
    } else {
        found.into_iter().rev().take(limit).collect()
    };
    done.sort_unstable();
    Ok(Habit {
        scheduled: day,
        scheduled_repeat,
        deadline,
        deadline_repeat,
        done,
        repeat_type: repeat_type.to_string(),
    })
}

/// The days of the notes of `entry` that say it was done, in its order:
/// `State "DONE" … [date]` items, and those with the `done` heading
/// (`CLOSING NOTE [date]`), its escapes as patterns.
fn done_days(entry: &str, done_keywords: &[String], done_heading: &str) -> Vec<Day> {
    if done_keywords.is_empty() {
        return Vec::new();
    }
    let keywords = done_keywords.iter().map(|keyword| regex::escape(keyword)).collect::<Vec<_>>().join("|");
    let stamp = r"\[([0-9]{4}-[0-9]{2}-[0-9]{2}(?: [^\n]*?)?)\]";
    // `org-replace-escapes` on the quoted heading; Org takes an empty one
    // for a pattern that matches any item, which this leaves out.
    const PATTERNS: [(char, &str); 8] = [
        ('d', r"\[([0-9]{4}-[0-9]{2}-[0-9]{2}(?: .*?)?)\]"),
        ('t', r"\[([0-9]{4}-[0-9]{2}-[0-9]{2}(?: .*?)?)\]"),
        ('D', r"<([0-9]{4}-[0-9]{2}-[0-9]{2}(?: .*?)?)>"),
        ('T', r"<([0-9]{4}-[0-9]{2}-[0-9]{2}(?: .*?)?)>"),
        ('s', r#""\S+""#),
        ('S', r#""\S+""#),
        ('u', ".*?"),
        ('U', ".*?"),
    ];
    let marked = crate::text::replace_escapes(done_heading, |c| {
        PATTERNS.iter().position(|(key, _)| *key == c).map(|index| format!("\0{index}\0"))
    });
    let mut done = regex::escape(&marked);
    for (index, (_, regexp)) in PATTERNS.iter().enumerate() {
        done = done.replace(&format!("\0{index}\0"), regexp);
    }
    let alternative = if done_heading.is_empty() { String::new() } else { format!("|{done}") };
    let pattern = format!(r#"(?m)^[ \t]*-[ \t]+(?:State "(?:{keywords})".*{stamp}{alternative})"#);
    let Ok(note) = Regex::new(&pattern) else { return Vec::new() };
    note.captures_iter(entry)
        .filter_map(|found| {
            // The first date the match has.
            let text = found.iter().skip(1).flatten().next()?.as_str();
            time_string_to_day(&format!("<{text}>"))
        })
        .collect()
}

impl Habit {
    /// `org-habit-deadline`
    pub fn deadline(&self) -> Day {
        self.deadline.unwrap_or(self.scheduled)
    }

    /// `org-habit-deadline-repeat`
    fn deadline_repeat(&self) -> i64 {
        self.deadline_repeat.unwrap_or(self.scheduled_repeat)
    }

    /// `org-habit-get-urgency` on `now`.
    pub fn urgency(&self, now: Day) -> i64 {
        let deadline = self.deadline();
        let mut urgency = 1000 + (now - self.scheduled) * 10;
        if self.scheduled != deadline && now == deadline {
            urgency += 50;
        }
        let slip = now - (deadline - 1);
        urgency + if slip > 0 { slip * 100 } else { slip * 10 }
    }

    /// `org-habit-get-faces`: the face of `now`, and of `now` in the
    /// future, when the habit was scheduled for `scheduled` (by default
    /// its date) and `done` says whether it was done then.
    fn faces(&self, now: Day, scheduled: Option<Day>, done: bool, settings: &Settings) -> (Face, Face) {
        let deadline = match scheduled {
            Some(scheduled) => scheduled + (self.deadline_repeat() - self.scheduled_repeat),
            None => self.deadline(),
        };
        let scheduled = scheduled.unwrap_or(self.scheduled);
        if now < scheduled {
            (Face::Clear, Face::ClearFuture)
        } else if now < deadline || (now == deadline && done) || (settings.show_done_always_green && done) {
            (Face::Ready, Face::ReadyFuture)
        } else if now == deadline {
            (Face::Alert, Face::AlertFuture)
        } else {
            (Face::Overdue, Face::OverdueFuture)
        }
    }

    /// `org-habit-build-graph` from `start` to `end` (not included) with
    /// `now` the current day: a glyph and a face a day. (Org's graph then
    /// has a blank, without a face.)
    pub fn graph(&self, start: Day, now: Day, end: Day, settings: &Settings) -> Vec<(char, Face)> {
        let all = &self.done;
        let mut done = all.as_slice();
        let mut last_done = None;
        while let Some((&first, rest)) = done.split_first() {
            if first >= start {
                break;
            }
            last_done = Some(first);
            done = rest;
        }
        let scheduled = self.scheduled;
        let repeat = self.scheduled_repeat;
        let mut graph = Vec::new();
        for day in start..end {
            let past = day < now;
            let today = day == now;
            let done_today = done.first() == Some(&day);
            let faces = if past && last_done.is_none() && scheduled >= now {
                if all.first() == Some(&day) {
                    (Face::Ready, Face::ReadyFuture)
                } else {
                    (Face::Clear, Face::ClearFuture)
                }
            } else {
                // The date the habit was scheduled for on `day`.
                let then = match (past, last_done) {
                    (true, Some(last)) => Some(if done.is_empty() {
                        scheduled
                    } else if self.repeat_type == ".+" {
                        last + repeat
                    } else if self.repeat_type == "+" {
                        scheduled - done.len() as i64 * repeat
                    } else {
                        self.replayed(last)
                    }),
                    _ => None,
                };
                self.faces(day, then, done_today, settings)
            };
            let mut marked = false;
            let glyph = if done_today {
                marked = true;
                while done.first() == Some(&day) {
                    last_done = Some(day);
                    done = &done[1..];
                }
                settings.completed_glyph
            } else if today {
                settings.today_glyph
            } else {
                ' '
            };
            let mut face = if past || today { faces.0 } else { faces.1 };
            if past && face != Face::Overdue && !marked {
                face = faces.1;
            }
            graph.push((glyph, face));
        }
        graph
    }

    /// For `++` repeaters: the date the habit was scheduled for after it
    /// was done on `last`, from the first time it was done on, each done
    /// moving it on by as many intervals as it takes to get past that day.
    fn replayed(&self, last: Day) -> Day {
        let all = &self.done;
        let first = all[0];
        let repeat = self.scheduled_repeat;
        let shift = (self.scheduled - first).rem_euclid(repeat);
        let mut scheduled = first + if shift == 0 { repeat } else { shift };
        if first == last {
            return scheduled;
        }
        for &done in &all[1..] {
            scheduled += (1 + (done - scheduled).max(0) / repeat) * repeat;
            if done == last {
                return scheduled;
            }
        }
        scheduled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTES: Notes = Notes { done_heading: "CLOSING NOTE %t", reversed: true };

    /// The faces of each day of the graphs of `tests/agenda/habits`, on
    /// 2026-05-18, as `org-habit-build-graph` gives them (`c` clear, `r`
    /// ready, `a` alert, `o` overdue; capitals for the future faces).
    #[test]
    fn graphs_as_emacs_draws_them() {
        let text = include_str!("../../../../tests/agenda/habits/habits.org");
        let keywords = org_syntax::TodoKeywords::from_document(text).unwrap();
        let parsed = org_syntax::parse(text, &keywords);
        let outline = crate::outline::Outline::new(&parsed, text, "habits");
        let today = time_string_to_day("<2026-05-18>").unwrap();
        let settings = Settings::default();
        let done: Vec<String> = keywords.names().filter(|name| keywords.is_done(name)).map(str::to_string).collect();
        let expected = [
            ("Shave", "    *   *  *   *  *  !      ", "CCCCcCRRrCRrCRRrCRrCRrAOOOOO"),
            ("Exercise", "              * ** **!      ", "CCCCCCCCCCCCCCrAorAoraOOOOOO"),
            ("Read", "         *      *    !      ", "CCCCCCCCCrCCCCCCrCCCCcCAOOOO"),
            ("Water plants", "      *   *          !      ", "CCCCCCcCCAoCCAooooooooOOOOOO"),
            ("Done habit", "                     !      ", "CCCCCCCCCCCCCCCCCCCCCaOOOOOO"),
            ("New habit, never done", "                     !      ", "CCCCCCCCCCCCCCCCCCCCCcAOOOOO"),
        ];
        for (head, glyphs, faces) in expected {
            let index = outline.entries.iter().position(|entry| entry.head.contains(head)).unwrap();
            let entry = &outline.entries[index];
            let end = outline.entries.get(index + 1).map_or(text.len(), |next| next.offset);
            let scheduled = entry.scheduled.as_ref().unwrap().raw.as_str();
            let habit = parse(scheduled, head, &text[entry.offset..end], &done, 28, NOTES).unwrap();
            let graph = habit.graph(today - 21, today, today + 7, &settings);
            let drawn: String = graph.iter().map(|(glyph, _)| *glyph).collect();
            let named: String = graph
                .iter()
                .map(|(_, face)| match face {
                    Face::Clear => 'c',
                    Face::ClearFuture => 'C',
                    Face::Ready => 'r',
                    Face::ReadyFuture => 'R',
                    Face::Alert => 'a',
                    Face::AlertFuture => 'A',
                    Face::Overdue => 'o',
                    Face::OverdueFuture => 'O',
                })
                .collect();
            assert_eq!((drawn.as_str(), named.as_str()), (glyphs, faces), "{head}");
        }
    }

    #[test]
    fn reads_durations_and_repeaters() {
        assert_eq!(duration_to_days(".+2d"), Ok(2));
        assert_eq!(duration_to_days("+1w"), Ok(7));
        assert_eq!(duration_to_days("+1m"), Ok(30));
        assert_eq!(duration_to_days("+1y"), Ok(365));
        assert_eq!(repeater("<2026-05-18 Mon .+2d/4d>"), Some(".+2d/4d"));
        assert_eq!(repeater("<2026-05-18 Mon 10:00 ++1w>"), Some("++1w"));
        assert_eq!(repeater("<2026-05-18 Mon>"), None);
    }

    #[test]
    fn reads_the_days_it_was_done() {
        let entry = "* TODO Shave\nSCHEDULED: <2026-05-18 Mon .+2d>\n:LOGBOOK:\n\
                     - State \"DONE\"       from \"TODO\"       [2026-05-16 Sat 08:00]\n\
                     - State \"DONE\"       from \"TODO\"       [2026-05-14 Thu]\n\
                     - State \"TODO\"       from \"DONE\"       [2026-05-13 Wed]\n\
                     - CLOSING NOTE [2026-05-10 Sun 09:00] \\\\\n  done\n:END:\n";
        let habit = parse("<2026-05-18 Mon .+2d>", "Shave", entry, &["DONE".to_string()], 28, NOTES).unwrap();
        let day = |text: &str| time_string_to_day(&format!("<{text}>")).unwrap();
        assert_eq!(habit.done, [day("2026-05-10"), day("2026-05-14"), day("2026-05-16")]);
        assert_eq!(habit.repeat_type, ".+");
        assert!(parse("<2026-05-18 Mon>", "Shave", entry, &[], 28, NOTES).is_err());
        // Other headings for closing notes, and the latest notes last.
        let notes = "* TODO Shave\n- Closed <2026-05-10 Sun> by me\n- Closed <2026-05-12 Tue> by me\n\
                     - Closed <2026-05-14 Thu> by me\n";
        let custom = Notes { done_heading: "Closed %D by %u", reversed: false };
        let habit = parse("<2026-05-18 Mon .+2d>", "Shave", notes, &["DONE".to_string()], 2, custom).unwrap();
        let day = |d| crate::date::Date::new(2026, 5, d).to_day();
        assert_eq!(habit.done, [day(12), day(14)]);
        let first = Notes { reversed: true, ..custom };
        let habit = parse("<2026-05-18 Mon .+2d>", "Shave", notes, &["DONE".to_string()], 2, first).unwrap();
        assert_eq!(habit.done, [day(10), day(12)]);
        assert!(parse("<2026-05-18 Mon .+2d/1d>", "Shave", entry, &[], 28, NOTES).is_err());
    }
}
