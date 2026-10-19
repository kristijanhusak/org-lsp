//! `org-clock`: clocking in and out of an entry, and what the `CLOCK:`
//! lines add up to.
//!
//! A clock that runs is a `CLOCK:` line with a start and no end. The
//! files say which clock runs, so it survives a restart of the server, as
//! Emacs's `org-clock-persist` and `org-resolve-clocks` let it.

use org_syntax::TodoKeywords;

use crate::date::DateTime;
use crate::indent::Adapt;
use crate::stamp::format_timestamp;
use crate::timestr::parse_time_string;
use crate::todo::{headline_level, Editor, Settings as TodoSettings};

/// How clocking writes: `org-clock-into-drawer` and its relatives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The drawer `CLOCK:` lines go into (`org-clock-drawer-name`), or
    /// none.
    pub drawer: Option<String>,
    /// `org-clock-out-remove-zero-time-clocks`
    pub remove_zero: bool,
    /// `org-adapt-indentation`
    pub adapt: Adapt,
    /// `org-log-into-drawer`, which says whether a drawer is headline data
    /// to indentation.
    pub log_into_drawer: Option<String>,
    /// `org-log-states-order-reversed`: a new clock line goes above the
    /// others.
    pub states_order_reversed: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            drawer: Some("LOGBOOK".to_string()),
            remove_zero: false,
            adapt: Adapt::Off,
            log_into_drawer: None,
            states_order_reversed: true,
        }
    }
}

/// One `CLOCK:` line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clock {
    pub start: Option<DateTime>,
    pub end: Option<DateTime>,
    /// The duration after `=>`, in minutes.
    pub minutes: Option<i64>,
}

impl Clock {
    /// A clock with a start and no end.
    pub fn is_running(&self) -> bool {
        self.start.is_some() && self.end.is_none() && self.minutes.is_none()
    }
}

/// The minute a date and time stand for, counted from day 0.
pub fn minutes_of(time: DateTime) -> i64 {
    time.day * 1440 + time.minute
}

/// `org-current-time` with `org-clock-rounding-minutes`: `now` at the
/// nearest multiple of `rounding` minutes within its hour (a half to the
/// even multiple, as `round` does); with `past`, never later than `now`.
/// Below 1, `now` as it is.
pub fn round_time(now: DateTime, rounding: i64, past: bool) -> DateTime {
    if rounding < 1 {
        return now;
    }
    let minute = now.minute.rem_euclid(60);
    let (quotient, remainder) = (minute / rounding, minute % rounding);
    let nearest = match (2 * remainder).cmp(&rounding) {
        std::cmp::Ordering::Greater => quotient + 1,
        std::cmp::Ordering::Equal => quotient + quotient % 2,
        std::cmp::Ordering::Less => quotient,
    };
    let mut rounded = minutes_of(now) - minute + nearest * rounding;
    if past && rounded > minutes_of(now) {
        rounded -= rounding;
    }
    DateTime { day: rounded.div_euclid(1440), minute: rounded.rem_euclid(1440) }
}

/// `[2026-10-05 Mon 10:30]`: the date and time of an inactive timestamp
/// that starts `text`, and where it ends.
fn stamp(text: &str) -> Option<(DateTime, usize)> {
    let inner = text.strip_prefix('[')?;
    let end = inner.find(']')?;
    let parsed = parse_time_string(&inner[..end])?;
    let (hour, minute) = parsed.time?;
    Some((DateTime::new(parsed.date, hour, minute), end + 2))
}

/// `h:mm`, as `=> h:mm` writes it.
fn read_duration(text: &str) -> Option<i64> {
    let (hours, minutes) = text.trim().split_once(':')?;
    let hours: i64 = hours.trim().parse().ok()?;
    let minutes: i64 = minutes.get(..2)?.parse().ok()?;
    Some(hours * 60 + minutes)
}

/// The clock of a `CLOCK:` line.
pub fn parse_line(line: &str) -> Option<Clock> {
    let rest = line.trim_start_matches([' ', '\t']).strip_prefix("CLOCK:")?;
    let rest = rest.trim_start_matches([' ', '\t']);
    let mut clock = Clock { start: None, end: None, minutes: None };
    let mut rest = rest;
    if let Some((start, length)) = stamp(rest) {
        clock.start = Some(start);
        rest = &rest[length..];
        let dashes = rest.bytes().take_while(|b| *b == b'-').count();
        if (1..=3).contains(&dashes) {
            if let Some((end, length)) = stamp(&rest[dashes..]) {
                clock.end = Some(end);
                rest = &rest[dashes + length..];
            }
        }
    }
    if let Some(duration) = rest.trim_start_matches([' ', '\t']).strip_prefix("=>") {
        clock.minutes = read_duration(duration);
    }
    (clock.start.is_some() || clock.minutes.is_some()).then_some(clock)
}

/// The start of every line of `text`, with the line.
fn lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut at = 0;
    text.split_inclusive('\n').map(move |line| {
        let start = at;
        at += line.len();
        (start, line.trim_end_matches(['\n', '\r']))
    })
}

/// The clocks that run in `text`: where each line starts, and when the
/// clock started.
pub fn running(text: &str) -> Vec<(usize, DateTime)> {
    lines(text)
        .filter_map(|(start, line)| {
            let clock = parse_line(line)?;
            clock.is_running().then(|| (start, clock.start.expect("a running clock has a start")))
        })
        .collect()
}

/// `org-duration-from-minutes`, as `org-duration-format` says (`1d 2:30`
/// by default); see [`crate::duration`].
pub fn duration(minutes: i64) -> String {
    crate::duration::from_minutes(minutes as f64)
}

/// `org-duration-to-minutes`: `1:30`, `1:30:00`, `90` (minutes), `2h`,
/// `1h 30min`, `1d 2h`, `1.5h`, the units of `org-duration-units`; none
/// for the empty text and what is no duration.
pub fn duration_to_minutes(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    crate::duration::to_minutes(text)
}

/// The text of the inactive timestamp `org-clock-in` writes.
fn clock_stamp(time: DateTime) -> String {
    format_timestamp(time, true, true, "")
}

/// Whether `line` opens the drawer `name`.
fn opens(line: &str, name: &str) -> bool {
    line.trim_matches([' ', '\t']).eq_ignore_ascii_case(&format!(":{name}:"))
}

/// `org-clock-find-position` with `find-unclosed` (`org-clock-in-resume`):
/// when the clock started on a `CLOCK:` line of the entry at byte
/// `heading` that has no end, up to the next headline.
pub fn unclosed(text: &str, heading: usize) -> Option<DateTime> {
    let body = heading + text[heading..].find('\n').map_or(text.len() - heading, |end| end + 1);
    let end = lines(&text[body..])
        .find(|(_, line)| {
            let stars = line.len() - line.trim_start_matches('*').len();
            stars > 0 && line[stars..].starts_with([' ', '\t'])
        })
        .map_or(text.len(), |(start, _)| body + start);
    running(text).into_iter().find(|(start, _)| (body..end).contains(start)).map(|(_, time)| time)
}

/// `org-clock-in` on the entry at byte `offset`: a `CLOCK:` line where
/// `org-clock-find-position` puts it. Returns the new text and where the
/// line starts.
pub fn clock_in(
    text: &str,
    keywords: &TodoKeywords,
    offset: usize,
    settings: &Settings,
    now: DateTime,
) -> Result<(String, usize), String> {
    let todo_settings = TodoSettings {
        adapt: settings.adapt,
        log_into_drawer: settings.log_into_drawer.clone(),
        ..TodoSettings::default()
    };
    let mut editor = Editor::new(text, keywords, &todo_settings, now);
    editor.go_to_heading(offset)?;
    let end = editor.entry_end();
    let line_text = format!("CLOCK: {}", clock_stamp(now));
    // The `CLOCK:` line at the start of the line `at`, indented by
    // `org-indent-line`.
    let insert = |editor: &mut Editor, at: usize| {
        editor.replace(at, at, &format!("{line_text}\n"));
        editor.indent_line(at);
        at
    };

    // `org-log-states-order-reversed`, as `#+STARTUP:` may set it.
    let mut reversed = settings.states_order_reversed;
    for word in crate::text::startup_words(text) {
        match word {
            "logstatesreversed" => reversed = true,
            "nologstatesreversed" => reversed = false,
            _ => {}
        }
    }

    // An existing drawer: at its top, or at its bottom.
    if let Some(drawer) = settings.drawer.as_deref() {
        let mut line = editor.next_line(editor.heading);
        while line < end && line > editor.heading {
            if opens(editor.line(line), drawer) && editor.drawer_closes(line, end) {
                let mut at = editor.next_line(line);
                if !reversed {
                    while !editor.line(at).trim_matches([' ', '\t']).eq_ignore_ascii_case(":END:") {
                        at = editor.next_line(at);
                    }
                }
                let at = insert(&mut editor, at);
                return Ok((editor.text, at));
            }
            let next = editor.next_line(line);
            if next == line {
                break;
            }
            line = next;
        }
    }

    // The `CLOCK:` lines of the entry, outside a drawer.
    let mut clocks = Vec::new();
    let mut line = editor.next_line(editor.heading);
    while line < end && line > editor.heading {
        if parse_line(editor.line(line)).is_some() {
            clocks.push(line);
        }
        let next = editor.next_line(line);
        if next == line {
            break;
        }
        line = next;
    }
    let meta = editor.end_of_meta_data();
    match (settings.drawer.as_deref(), clocks.first()) {
        // No drawer: before the first clock line (with
        // `org-log-states-order-reversed` off, the last), or after the
        // meta data.
        (None, first) => {
            let last = if reversed { first } else { clocks.last() };
            let at = last.copied().unwrap_or(meta);
            let (at, prefix) = line_start_for(&editor.text, at);
            editor.replace(at, at, prefix);
            let at = insert(&mut editor, at + prefix.len());
            Ok((editor.text, at))
        }
        // A new drawer, indented by `org-indent-region`, with the line in
        // it.
        (Some(drawer), None) => {
            let (at, prefix) = line_start_for(&editor.text, meta);
            editor.replace(at, at, &format!("{prefix}:{drawer}:\n:END:\n"));
            let start = at + prefix.len();
            let end = editor.next_line(editor.next_line(start));
            editor.indent_region(start, end);
            let at = editor.next_line(start);
            let at = insert(&mut editor, at);
            Ok((editor.text, at))
        }
        // A new drawer, with the clock lines of the entry moved into it,
        // the last first, as Emacs collects them.
        (Some(drawer), Some(_)) => {
            let moved: Vec<String> = clocks.iter().rev().map(|line| editor.line(*line).trim().to_string()).collect();
            for line in clocks.iter().rev() {
                let next = editor.next_line(*line);
                editor.replace(*line, next, "");
            }
            let meta = editor.end_of_meta_data();
            let (at, prefix) = line_start_for(&editor.text, meta);
            let mut drawer_text = format!("{prefix}:{drawer}:\n");
            for line in &moved {
                drawer_text.push_str(line);
                drawer_text.push('\n');
            }
            drawer_text.push_str(":END:\n");
            editor.replace(at, at, &drawer_text);
            let start = at + prefix.len();
            let mut end_line = start;
            for _ in 0..=moved.len() {
                end_line = editor.next_line(end_line);
            }
            // Emacs's `org-indent-region` starts below the drawer's first
            // line: the marker of the point it saved moved past it.
            let first = editor.next_line(start);
            editor.indent_region(first, editor.next_line(end_line));
            // Emacs puts the new line below the first line it moved, or
            // with `org-log-states-order-reversed` off, above the last.
            let mut line = editor.next_line(start);
            for _ in 0..if reversed { 1 } else { moved.len() - 1 } {
                line = editor.next_line(line);
            }
            let at = insert(&mut editor, line);
            Ok((editor.text, at))
        }
    }
}

/// Where a line inserted at `at` starts: a line break first when `at` is
/// the end of a text that does not end with one.
fn line_start_for(text: &str, at: usize) -> (usize, &'static str) {
    if at == text.len() && !text.is_empty() && !text.ends_with('\n') {
        (at, "\n")
    } else {
        (at, "")
    }
}

/// What `org-clock-out` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockedOut {
    pub text: String,
    pub minutes: i64,
    /// What Emacs says: `Clock stopped at [2026-10-05 Mon 11:00] after 0:30`.
    pub message: String,
}

/// `org-clock-in-switch-to-state` and `org-clock-out-switch-to-state`: the
/// entry whose headline starts at `heading` put in `state`, unless its
/// headline starts with it already. No note is asked for: a time is
/// logged where one would be.
pub fn switch_state(
    text: &str,
    keywords: &TodoKeywords,
    heading: usize,
    state: &str,
    settings: &TodoSettings,
    now: DateTime,
) -> Result<String, String> {
    let line = text[heading..].lines().next().unwrap_or("");
    let after = line.trim_start_matches('*').trim_start_matches([' ', '\t']);
    let has = after.len() >= state.len()
        && after.is_char_boundary(state.len())
        && after[..state.len()].eq_ignore_ascii_case(state)
        && after[state.len()..].chars().next().is_none_or(|c| c == ' ' || c == '\t');
    if has {
        return Ok(text.to_string());
    }
    let target = crate::todo::Target::State(Some(state.to_string()));
    crate::todo::change_state_noted(text, keywords, heading, &target, settings, now, crate::todo::NoteText::TimeOnly)
        .map(|changed| changed.text)
}

/// `org-clock-out`: end the clock on the line starting at `line`.
pub fn clock_out(text: &str, line: usize, now: DateTime, settings: &Settings) -> Result<ClockedOut, String> {
    let line_end = text[line..].find('\n').map_or(text.len(), |end| line + end);
    let current = &text[line..line_end];
    let clock = parse_line(current).filter(Clock::is_running).ok_or("Clock start time is gone")?;
    let start = clock.start.expect("a running clock has a start");
    // After the start's timestamp, the rest of the line is replaced.
    let after_keyword = current.find("CLOCK:").expect("a clock line") + "CLOCK:".len();
    let stamp_start = after_keyword + current[after_keyword..].find('[').ok_or("Clock start time is gone")?;
    let stamp_end = stamp_start + current[stamp_start..].find(']').ok_or("Clock start time is gone")? + 1;
    let minutes = (minutes_of(now) - minutes_of(start)).max(0);
    let end_stamp = clock_stamp(now);
    let closing = format!("--{end_stamp} => {:2}:{:02}", minutes / 60, minutes % 60);
    let mut out = String::with_capacity(text.len() + closing.len());
    out.push_str(&text[..line + stamp_end]);
    out.push_str(&closing);
    out.push_str(&text[line_end..]);
    let removed = settings.remove_zero && minutes == 0;
    if removed {
        out = remove_line(&out, line);
        if let Some(drawer) = settings.drawer.as_deref() {
            out = remove_empty_drawer_above(&out, line, drawer);
        }
    }
    let suffix = if removed { " => LINE REMOVED" } else { "" };
    let message = format!("Clock stopped at {end_stamp} after {}{suffix}", duration(minutes));
    Ok(ClockedOut { text: out, minutes, message })
}

/// `org-clock-cancel`: the running clock's line removed, and its drawer
/// with it when nothing else is in it.
pub fn cancel(text: &str, line: usize, settings: &Settings) -> Result<String, String> {
    let line_end = text[line..].find('\n').map_or(text.len(), |end| line + end);
    parse_line(&text[line..line_end]).filter(Clock::is_running).ok_or("Clock gone")?;
    let out = remove_line(text, line);
    Ok(match settings.drawer.as_deref() {
        Some(drawer) => remove_empty_drawer_above(&out, line, drawer),
        None => out,
    })
}

/// `text` without the line starting at `line`.
fn remove_line(text: &str, line: usize) -> String {
    let next = text[line..].find('\n').map_or(text.len(), |end| line + end + 1);
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..line]);
    out.push_str(&text[next..]);
    out
}

/// `org-remove-empty-drawer-at`: when the line at `at` is the `:END:` of
/// the drawer `name` opened on the line above, both go.
fn remove_empty_drawer_above(text: &str, at: usize, name: &str) -> String {
    if at == 0 || at > text.len() {
        return text.to_string();
    }
    let opening_start = text[..at - 1].rfind('\n').map_or(0, |index| index + 1);
    let opening = &text[opening_start..at - 1];
    let end_line_end = text[at..].find('\n').map_or(text.len(), |end| at + end);
    let closing = &text[at..end_line_end];
    if !opens(opening, name) || !closing.trim_matches([' ', '\t']).eq_ignore_ascii_case(":END:") {
        return text.to_string();
    }
    let next = (end_line_end + 1).min(text.len());
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..opening_start]);
    out.push_str(&text[next..]);
    out
}

/// The minutes of `clock` within `[from, to)`, as `org-clock-sum` counts
/// them: a range is clipped to the window, a bare `=> h:mm` counts whole,
/// a running clock not at all.
pub fn clock_minutes(clock: &Clock, from: Option<i64>, to: Option<i64>) -> i64 {
    match (clock.start, clock.end) {
        (Some(start), Some(end)) => {
            let start = minutes_of(start).max(from.unwrap_or(i64::MIN));
            let end = minutes_of(end).min(to.unwrap_or(i64::MAX));
            (end - start).max(0)
        }
        (None, None) => clock.minutes.unwrap_or(0),
        _ => 0,
    }
}

/// `org-clock-sum`: the minutes clocked under each headline of `text`,
/// its subtree included, within `[from, to)` (minutes as [`minutes_of`]
/// counts them). Returns the start of each headline with a time, and the
/// time; and the total of the file.
pub fn sums(text: &str, from: Option<i64>, to: Option<i64>) -> (Vec<(usize, i64)>, i64) {
    // Org walks backwards, adding the clocks seen since the last headline
    // to every level up to that headline's.
    let mut found: Vec<(usize, i64)> = Vec::new();
    let mut levels: Vec<i64> = vec![0; 32];
    let mut pending = 0;
    let all: Vec<(usize, &str)> = lines(text).collect();
    for (start, line) in all.into_iter().rev() {
        if let Some(level) = headline_level(line) {
            if level >= levels.len() {
                levels.resize(level * 2, 0);
            }
            if pending > 0 || levels[level] > 0 {
                for slot in levels.iter_mut().take(level + 1) {
                    *slot += pending;
                }
                found.push((start, levels[level]));
                pending = 0;
                for slot in levels.iter_mut().skip(level) {
                    *slot = 0;
                }
            }
        } else if let Some(clock) = parse_line(line) {
            pending += clock_minutes(&clock, from, to);
        }
    }
    found.reverse();
    (found, levels[0])
}

/// `org-clock-sum-current-item`: the minutes clocked in the subtree of
/// the headline at `heading`, since `since` if given.
pub fn subtree_minutes(text: &str, heading: usize, since: Option<i64>) -> i64 {
    let level = text[heading..].lines().next().and_then(headline_level).unwrap_or(1);
    let mut end = text.len();
    let mut at = text[heading..].find('\n').map_or(text.len(), |end| heading + end + 1);
    while at < text.len() {
        let line_end = text[at..].find('\n').map_or(text.len(), |end| at + end);
        if headline_level(&text[at..line_end]).is_some_and(|other| other <= level) {
            end = at;
            break;
        }
        at = line_end + 1;
    }
    let mut total = 0;
    for (_, line) in lines(&text[heading..end]) {
        if let Some(clock) = parse_line(line) {
            total += clock_minutes(&clock, since, None);
        }
    }
    total
}

#[cfg(test)]
mod tests {
    #[test]
    fn finds_the_clock_an_entry_left_running() {
        let text = "* A\n:LOGBOOK:\nCLOCK: [2026-05-17 Sun 09:00]--[2026-05-17 Sun 10:00] =>  1:00\n:END:\n\
                    * B\n:LOGBOOK:\nCLOCK: [2026-05-18 Mon 09:00]\n:END:\n";
        assert_eq!(unclosed(text, 0), None);
        let b = text.find("* B").unwrap();
        let start = unclosed(text, b).unwrap();
        assert_eq!((start.minute, crate::date::Date::from_day(start.day).day), (9 * 60, 18));
    }

    /// `org-current-time` at 23:MM:30 on 2025-12-31 for every minute,
    /// each rounding, without and with PAST, as Emacs gives it
    /// (`tests/clock/rounding.txt`: rounding, past, then `DD HH:MM` each).
    #[test]
    fn rounds_times_as_emacs_does() {
        let recorded = include_str!("../../../tests/clock/rounding.txt");
        for line in recorded.lines() {
            let mut parts = line.splitn(3, ' ');
            let rounding: i64 = parts.next().unwrap().parse().unwrap();
            let past = parts.next() == Some("t");
            let day = Date::new(2025, 12, 31).to_day();
            for (minute, expected) in parts.next().unwrap().split(',').enumerate() {
                let now = DateTime { day, minute: 23 * 60 + minute as i64 };
                let got = round_time(now, rounding, past);
                let shown = format!("{:02} {:02}:{:02}", Date::from_day(got.day).day, got.minute / 60, got.minute % 60);
                assert_eq!(shown, expected, "rounding {rounding}, past {past}, minute {minute}");
            }
        }
    }

    use super::*;
    use crate::date::Date;

    fn at(hour: i64, minute: i64) -> DateTime {
        DateTime::new(Date::new(2026, 10, 5), hour, minute)
    }

    fn keywords() -> TodoKeywords {
        TodoKeywords::default()
    }

    fn clock_in_at(text: &str, offset: usize, settings: &Settings) -> String {
        clock_in(text, &keywords(), offset, settings, at(10, 30)).unwrap().0
    }

    #[test]
    fn reads_clock_lines() {
        let closed = parse_line("CLOCK: [2026-10-05 Mon 08:00]--[2026-10-05 Mon 09:15] =>  1:15").unwrap();
        assert_eq!((closed.start, closed.end, closed.minutes), (Some(at(8, 0)), Some(at(9, 15)), Some(75)));
        assert!(!closed.is_running());
        let open = parse_line("  CLOCK: [2026-10-05 Mon 10:30]").unwrap();
        assert!(open.is_running());
        assert_eq!(parse_line("CLOCK: => 1:30").unwrap().minutes, Some(90));
        assert_eq!(parse_line("Some CLOCK: text"), None);
        assert_eq!(running("* a\n:LOGBOOK:\nCLOCK: [2026-10-05 Mon 10:30]\n:END:\n"), [(14, at(10, 30))]);
    }

    /// What Emacs 29 with Org 9.8 writes for `org-clock-in` under
    /// `org-adapt-indentation`, clock lines shown as `CLOCK: STAMP`.
    #[test]
    fn indents_what_it_writes_as_org_does() {
        let stamp = regex::Regex::new(r"CLOCK: \[[^]]*\](--\[[^]]*\] => +[0-9:]+)?").unwrap();
        let cases = [
            ("** Task\nBody\n", Adapt::All, true, "** Task\n   :LOGBOOK:\n   CLOCK: STAMP\n   :END:\nBody\n"),
            ("** Task\nBody\n", Adapt::HeadlineData, true, "** Task\n   :LOGBOOK:\n   CLOCK: STAMP\n   :END:\nBody\n"),
            ("** Task\nBody\n", Adapt::Off, true, "** Task\n:LOGBOOK:\nCLOCK: STAMP\n:END:\nBody\n"),
            (
                "** Task\nSCHEDULED: <2026-10-05 Mon>\nBody\n",
                Adapt::All,
                true,
                "** Task\nSCHEDULED: <2026-10-05 Mon>\n:LOGBOOK:\nCLOCK: STAMP\n:END:\nBody\n",
            ),
            (
                "** Task\n   :LOGBOOK:\n   CLOCK: [2026-10-05 Mon 09:00]--[2026-10-05 Mon 10:00] =>  1:00\n   :END:\n",
                Adapt::All,
                true,
                "** Task\n   :LOGBOOK:\n   CLOCK: STAMP\n   CLOCK: STAMP\n   :END:\n",
            ),
            (
                "** Task\nCLOCK: [2026-10-05 Mon 09:00]--[2026-10-05 Mon 10:00] =>  1:00\nBody\n",
                Adapt::All,
                true,
                "** Task\n:LOGBOOK:\nCLOCK: STAMP\nCLOCK: STAMP\n:END:\nBody\n",
            ),
            ("** Task\nBody\n", Adapt::All, false, "** Task\n   CLOCK: STAMP\nBody\n"),
            ("** Task\n   Body\n", Adapt::HeadlineData, false, "** Task\n   CLOCK: STAMP\n   Body\n"),
        ];
        for (text, adapt, drawer, expected) in cases {
            let settings = Settings { adapt, drawer: drawer.then(|| "LOGBOOK".to_string()), ..Settings::default() };
            let out = clock_in_at(text, 0, &settings);
            assert_eq!(stamp.replace_all(&out, "CLOCK: STAMP"), expected, "{text:?} {adapt:?} {drawer}");
        }
    }

    #[test]
    fn clocks_in_as_org_clock_in_does() {
        let settings = Settings::default();
        // A new drawer after the planning line and the properties.
        let text = "* TODO a\nSCHEDULED: <2026-10-05 Mon>\n:PROPERTIES:\n:EFFORT: 1:00\n:END:\nText.\n";
        assert_eq!(
            clock_in_at(text, 0, &settings),
            "* TODO a\nSCHEDULED: <2026-10-05 Mon>\n:PROPERTIES:\n:EFFORT: 1:00\n:END:\n\
             :LOGBOOK:\nCLOCK: [2026-10-05 Mon 10:30]\n:END:\nText.\n"
        );
        // At the top of the drawer there is.
        let text = "* b\n:LOGBOOK:\n- Note taken on [2026-10-01 Thu 10:00]\nCLOCK: [2026-10-02 Fri 14:00]--[2026-10-02 Fri 15:00] =>  1:00\n:END:\n";
        let (clocked, line) = clock_in(text, &keywords(), 0, &settings, at(10, 30)).unwrap();
        assert_eq!(
            clocked,
            "* b\n:LOGBOOK:\nCLOCK: [2026-10-05 Mon 10:30]\n- Note taken on [2026-10-01 Thu 10:00]\nCLOCK: [2026-10-02 Fri 14:00]--[2026-10-02 Fri 15:00] =>  1:00\n:END:\n"
        );
        assert_eq!(line, 14);
        // A headline that ends the text.
        assert_eq!(clock_in_at("* c", 0, &settings), "* c\n:LOGBOOK:\nCLOCK: [2026-10-05 Mon 10:30]\n:END:\n");
        // Clock lines outside a drawer move into a new one.
        let text = "* d\nCLOCK: [2026-10-01 Thu 09:00]--[2026-10-01 Thu 10:00] =>  1:00\nBody\n";
        assert_eq!(
            clock_in_at(text, 0, &settings),
            "* d\n:LOGBOOK:\nCLOCK: [2026-10-01 Thu 09:00]--[2026-10-01 Thu 10:00] =>  1:00\nCLOCK: [2026-10-05 Mon 10:30]\n:END:\nBody\n"
        );
        // An indented drawer, after the body.
        assert_eq!(
            clock_in_at("* e\nBody\n  :LOGBOOK:\n  - x\n  :END:\n", 0, &settings),
            "* e\nBody\n  :LOGBOOK:\n  CLOCK: [2026-10-05 Mon 10:30]\n  - x\n  :END:\n"
        );
        // Without a drawer: after the meta data.
        let none = Settings { drawer: None, ..Settings::default() };
        assert_eq!(clock_in_at("* e\nBody\n", 0, &none), "* e\nCLOCK: [2026-10-05 Mon 10:30]\nBody\n");
        assert_eq!(clock_in("text", &keywords(), 0, &settings, at(10, 30)), Err("Before first headline".to_string()));
    }

    #[test]
    fn clocks_out_and_cancels() {
        let settings = Settings::default();
        let text = "* a\n:LOGBOOK:\nCLOCK: [2026-10-05 Mon 10:30]\n:END:\n";
        let out = clock_out(text, 14, at(11, 5), &settings).unwrap();
        assert_eq!(out.text, "* a\n:LOGBOOK:\nCLOCK: [2026-10-05 Mon 10:30]--[2026-10-05 Mon 11:05] =>  0:35\n:END:\n");
        assert_eq!(out.message, "Clock stopped at [2026-10-05 Mon 11:05] after 0:35");
        assert_eq!(clock_out(&out.text, 14, at(12, 0), &settings).unwrap_err(), "Clock start time is gone");
        // Cancelled: the line goes, and the drawer that held only it.
        assert_eq!(cancel(text, 14, &settings).unwrap(), "* a\n");
        let kept = "* a\n:LOGBOOK:\nCLOCK: [2026-10-05 Mon 10:30]\n- Note\n:END:\n";
        assert_eq!(cancel(kept, 14, &settings).unwrap(), "* a\n:LOGBOOK:\n- Note\n:END:\n");
        // Zero minutes, removed when asked.
        let remove = Settings { remove_zero: true, ..Settings::default() };
        let out = clock_out(text, 14, at(10, 30), &remove).unwrap();
        assert_eq!(out.text, "* a\n");
        assert_eq!(out.message, "Clock stopped at [2026-10-05 Mon 10:30] after 0:00 => LINE REMOVED");
    }

    #[test]
    fn sums_subtrees_within_a_window() {
        let text = "* a\n:LOGBOOK:\nCLOCK: [2026-10-05 Mon 08:00]--[2026-10-05 Mon 09:15] =>  1:15\n:END:\n\
                    ** b\nCLOCK: [2026-10-05 Mon 09:30]--[2026-10-05 Mon 10:00] =>  0:30\n\
                    * c\n* d\nCLOCK: => 0:20\n";
        let (found, total) = sums(text, None, None);
        assert_eq!(found, [(0, 105), (text.find("** b").unwrap(), 30), (text.find("* d").unwrap(), 20)]);
        assert_eq!(total, 125);
        // From 9:00: clipped.
        let from = minutes_of(at(9, 0));
        let (found, total) = sums(text, Some(from), None);
        assert_eq!(found[0], (0, 45));
        assert_eq!(total, 65);
        assert_eq!(subtree_minutes(text, 0, None), 105);
        assert_eq!(subtree_minutes(text, text.find("** b").unwrap(), None), 30);
    }

    #[test]
    fn durations() {
        assert_eq!(duration(0), "0:00");
        assert_eq!(duration(75), "1:15");
        assert_eq!(duration(1565), "1d 2:05");
        assert_eq!(duration(1440), "1d 0:00");
        assert_eq!(duration_to_minutes("1:30"), Some(90.0));
        assert_eq!(duration_to_minutes("45"), Some(45.0));
        assert_eq!(duration_to_minutes("2h"), Some(120.0));
        assert_eq!(duration_to_minutes("1h 30min"), Some(90.0));
        assert_eq!(duration_to_minutes("1d"), Some(1440.0));
        assert_eq!(duration_to_minutes("soon"), None);
    }
}
