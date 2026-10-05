//! Changing a timestamp where it stands.
//!
//! [`change`] ports `org-timestamp-change`, the function behind Org's
//! `S-<up>`, `S-<down>`, `S-<left>` and `S-<right>` on a timestamp: it
//! changes the year, month, day, hour or minute the cursor is on, the
//! end of a time range, a repeater or a warning period, or it switches
//! between an active and an inactive timestamp. On a `CLOCK:` line it
//! also recomputes the duration.
//!
//! Like Org, the functions here read timestamps with regular expressions
//! on the raw text, so they work on anything that looks like a timestamp,
//! whether the parser takes it for one or not.

use crate::date::{weekday, Date, DateTime, WEEKDAY_ABBREVIATIONS};
use crate::timestr::parse_time_string;
use crate::Refusal;

/// A match of `org-ts-regexp3`: a date in angular or square brackets
/// with an optional day name and time, at most sixteen more characters,
/// and the closing bracket. All positions are byte offsets in the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamp {
    pub start: usize,
    pub end: usize,
    /// The day name.
    name: Option<(usize, usize)>,
    hour: Option<(usize, usize)>,
    minute: Option<(usize, usize)>,
}

/// The part of a timestamp the cursor is on, as `org-at-timestamp-p`
/// names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Bracket,
    After,
    Year,
    Month,
    Day,
    Hour,
    Minute,
    /// This many characters after the time, or after the day name.
    Extra(usize),
}

/// What to change in a timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Year,
    Month,
    Day,
    Hour,
    Minute,
}

/// How to change a timestamp: the arguments of `org-timestamp-change`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shift {
    /// By how many units; negative goes back in time.
    pub n: i64,
    /// The unit, or `None` for the part under the cursor.
    pub what: Option<Unit>,
    /// The change comes from an up or down key: minutes move in steps of
    /// `rounding` unless `counted`.
    pub updown: bool,
    /// The user gave a count.
    pub counted: bool,
}

fn is_date(bytes: &[u8]) -> bool {
    bytes.len() == 10
        && bytes
            .iter()
            .enumerate()
            .all(|(index, b)| if index == 4 || index == 7 { *b == b'-' } else { b.is_ascii_digit() })
}

impl Stamp {
    /// The timestamp that opens at byte `start`, if one does.
    pub fn at(text: &str, start: usize) -> Option<Stamp> {
        let bytes = text.as_bytes();
        if !matches!(bytes.get(start), Some(b'<' | b'[')) || !bytes.get(start + 1..start + 11).is_some_and(is_date) {
            return None;
        }
        let after_date = start + 11;
        // The optional day name: blanks, then characters that are no
        // digits, signs, blanks or brackets.
        let spaces = bytes[after_date..].iter().take_while(|b| **b == b' ').count();
        let name_start = after_date + spaces;
        let name_len: usize = text[name_start..]
            .chars()
            .take_while(|c| !matches!(c, ']' | '+' | '0'..='9' | '>' | '\r' | '\n' | ' ' | '-'))
            .map(char::len_utf8)
            .sum();
        // Org tries the longest name first and no name last.
        let mut names: Vec<(usize, Option<(usize, usize)>)> = text[name_start..name_start + name_len]
            .char_indices()
            .map(|(at, c)| (name_start + at + c.len_utf8(), Some((name_start, name_start + at + c.len_utf8()))))
            .collect();
        names.reverse();
        names.push((after_date, None));
        for (after_name, name) in names {
            let hours = match &bytes[after_name..] {
                [b' ', b'0'..=b'9', b'0'..=b'9', b':', b'0'..=b'9', b'0'..=b'9', ..] => Some(2),
                [b' ', b'0'..=b'9', b':', b'0'..=b'9', b'0'..=b'9', ..] => Some(1),
                _ => None,
            };
            let with_time = hours.map(|hours| {
                let hour = (after_name + 1, after_name + 1 + hours);
                (hour.1 + 3, Some(hour), Some((hour.1 + 1, hour.1 + 3)))
            });
            for (after_time, hour, minute) in with_time.into_iter().chain([(after_name, None, None)]) {
                // At most sixteen characters, then the closing bracket.
                for (count, (at, c)) in text[after_time..].char_indices().enumerate() {
                    if matches!(c, ']' | '>') {
                        return Some(Stamp { start, end: after_time + at + 1, name, hour, minute });
                    }
                    if c == '\n' || count == 16 {
                        break;
                    }
                }
            }
        }
        None
    }

    /// The timestamp on the line of `pos` that `pos` is in, or directly
    /// after (`org-at-timestamp-p` with `lax`).
    pub fn around(text: &str, pos: usize) -> Option<Stamp> {
        let line_start = text[..pos].rfind('\n').map_or(0, |index| index + 1);
        let line_end = text[pos..].find('\n').map_or(text.len(), |index| pos + index);
        let mut at = line_start;
        while at < line_end && at <= pos {
            match Stamp::at(text, at) {
                Some(stamp) if stamp.end >= pos => return Some(stamp),
                Some(stamp) => at = stamp.end,
                None => at += 1,
            }
        }
        None
    }

    fn part(&self, text: &str, pos: usize) -> Part {
        // Org's ranges include the position after their last character.
        let within = |range: Option<(usize, usize)>| range.is_some_and(|(start, end)| pos >= start && pos <= end);
        let date = self.start + 1;
        if pos == self.start || pos == self.end - 1 {
            Part::Bracket
        } else if pos == self.end {
            Part::After
        } else if within(Some((date, date + 4))) {
            Part::Year
        } else if within(Some((date + 5, date + 7))) {
            Part::Month
        } else if within(self.hour) {
            Part::Hour
        } else if within(self.minute) {
            Part::Minute
        } else if within(Some((date + 8, date + 10))) || within(self.name) {
            Part::Day
        } else {
            match self.minute.or(self.name) {
                Some((_, last)) if pos > last => Part::Extra(text[last..pos].chars().count()),
                _ => Part::Day,
            }
        }
    }
}

/// `[2026-05-18 Mon 10:30]` or `<2026-05-18 Mon>`, with `extra` before
/// the closing bracket.
pub(crate) fn format_timestamp(time: DateTime, with_time: bool, inactive: bool, extra: &str) -> String {
    let date = time.date();
    let (open, close) = if inactive { ('[', ']') } else { ('<', '>') };
    let name = WEEKDAY_ABBREVIATIONS[weekday(time.day)];
    let clock = if with_time { format!(" {:02}:{:02}", time.minute / 60, time.minute % 60) } else { String::new() };
    format!("{open}{:04}-{:02}-{:02} {name}{clock}{extra}{close}", date.year, date.month, date.day)
}

/// Whether `b` is a unit of a repeater or warning period. Org's own
/// commands run with case folding on and take capital letters as well.
fn is_unit(b: Option<&u8>, hours: bool, fold: bool) -> bool {
    b.is_some_and(|b| {
        let b = if fold { b.to_ascii_lowercase() } else { *b };
        matches!(b, b'd' | b'w' | b'm' | b'y') || (hours && b == b'h')
    })
}

/// Length of `[-+]?…N[hdwmy](/N[hdwmy])?` cookies as `org-timestamp-change`
/// keeps them: a blank run, an optional `.` or `+`, an optional `-`, a
/// sign, digits, a unit, and an optional `/N[hdwmy]`.
fn cookie_len(text: &str, fold: bool) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut at = bytes.iter().take_while(|b| **b == b' ').count();
    if at == 0 {
        return None;
    }
    if matches!(bytes.get(at), Some(b'.' | b'+')) && matches!(bytes.get(at + 1), Some(b'-' | b'+')) {
        // The optional prefix, when a sign still follows it.
        let after =
            if bytes.get(at + 1) == Some(&b'-') && matches!(bytes.get(at + 2), Some(b'-' | b'+')) { 2 } else { 1 };
        at += after;
    } else if bytes.get(at) == Some(&b'-') && matches!(bytes.get(at + 1), Some(b'-' | b'+')) {
        at += 1;
    }
    if !matches!(bytes.get(at), Some(b'-' | b'+')) {
        return None;
    }
    at += 1;
    let unit_after = |at: usize| -> Option<usize> {
        let digits = bytes[at..].iter().take_while(|b| b.is_ascii_digit()).count();
        (digits > 0 && is_unit(bytes.get(at + digits), true, fold)).then_some(at + digits + 1)
    };
    at = unit_after(at)?;
    if bytes.get(at) == Some(&b'/') {
        if let Some(end) = unit_after(at + 1) {
            at = end;
        }
    }
    Some(at)
}

/// What `org-timestamp-change` keeps of a timestamp besides date and
/// time: the end of a time range and the repeater and warning cookies
/// that stand directly before the closing bracket.
fn timestamp_extra(stamp: &str, fold: bool) -> &str {
    let inner = &stamp[..stamp.len() - 1];
    let matches_fully = |mut rest: &str| {
        let bytes = rest.as_bytes();
        let range = matches!(bytes, [b'-', b'0'..=b'2', b'0'..=b'9', b':', b'0'..=b'5', b'0'..=b'9', ..]);
        if range {
            rest = &rest[6..];
        }
        while let Some(len) = cookie_len(rest, fold) {
            rest = &rest[len..];
        }
        rest.is_empty()
    };
    inner.char_indices().map(|(at, _)| &inner[at..]).find(|rest| matches_fully(rest)).unwrap_or("")
}

/// Whether the timestamp has a time of day: `^.{10}.*?[0-9]+:[0-9][0-9]`.
fn has_time(stamp: &str) -> bool {
    let rest: String = stamp.chars().skip(10).collect();
    let bytes = rest.as_bytes();
    (1..bytes.len().saturating_sub(2)).any(|at| {
        bytes[at] == b':'
            && bytes[at - 1].is_ascii_digit()
            && bytes[at + 1].is_ascii_digit()
            && bytes[at + 2].is_ascii_digit()
    })
}

/// A number followed by a unit among `dmwy`, after `sign` and at least
/// one blank: where the number starts, and where the unit is.
fn interval(bytes: &[u8], from: usize, sign: u8, fold: bool) -> Option<(usize, usize)> {
    let blanks = bytes[from..].iter().take_while(|b| **b == b' ').count();
    let number = from + blanks + 1;
    if blanks == 0 || bytes.get(number - 1) != Some(&sign) {
        return None;
    }
    let digits = bytes[number..].iter().take_while(|b| b.is_ascii_digit()).count();
    let unit = number + digits;
    (digits > 0 && is_unit(bytes.get(unit), false, fold)).then_some((number, unit))
}

/// `org-modify-ts-extra`: change the part of `extra` at character `pos`
/// by `n`: the end of a time range, or the number or the unit of a
/// repeater or of a warning period. Minutes move in steps of `step`.
fn modify_extra(extra: &str, pos: usize, n: i64, step: i64, fold: bool) -> Result<String, Refusal> {
    let bytes = extra.as_bytes();
    let within = |start: usize, end: usize| pos >= start && pos <= end;
    let range = matches!(bytes, [b'-', b'0'..=b'2', b'0'..=b'9', b':', b'0'..=b'5', b'0'..=b'9', ..]);
    let after_range = if range { 6 } else { 0 };
    let repeater = interval(bytes, after_range, b'+', fold);
    let warning = interval(bytes, repeater.map_or(after_range, |(_, unit)| unit + 1), b'-', fold);
    let number = |start: usize, end: usize| extra[start..end].parse::<i64>().unwrap_or(0);
    // Units in the order `d`, `w`, `m`, `y`; a step off either end stays.
    let unit = |at: usize| {
        let index = match bytes[at] {
            b'd' => 0,
            b'w' => 1,
            b'm' => 2,
            b'y' => 3,
            // Org stops with an error on a capital letter.
            _ => return Err(Refusal::Failed("Cannot change this unit".to_string())),
        };
        Ok(match index + n {
            -1 | 0 => "d",
            1 => "w",
            2 => "m",
            3 | 4 => "y",
            _ => "",
        }
        .to_string())
    };
    let (start, end, new) = if range && within(1, 6) {
        let (mut hour, mut minute) = (number(1, 3), number(4, 6));
        if within(1, 3) {
            hour += n;
        } else {
            let n = step * n;
            let rest = minute % step;
            if rest != 0 {
                minute += if n > 0 { -rest } else { step - rest };
            }
            minute += n;
        }
        // Org carries a single hour, and writes minutes like `70` or `-5`
        // after a larger step. A range has to stay readable, so every
        // hour is carried here.
        let total = (hour * 60 + minute).rem_euclid(1440);
        (0, 6, format!("-{:02}:{:02}", total / 60, total % 60))
    } else {
        match (repeater, warning) {
            (Some((_, at)), _) if within(at, at + 1) => (at, at + 1, unit(at)?),
            (Some((from, at)), _) if within(from, at) => (from, at, (n + number(from, at)).max(1).to_string()),
            (_, Some((_, at))) if within(at, at + 1) => (at, at + 1, unit(at)?),
            (_, Some((from, at))) if within(from, at) => (from, at, (n + number(from, at)).max(0).to_string()),
            _ => return Ok(extra.to_string()),
        }
    };
    Ok(format!("{}{new}{}", &extra[..start], &extra[end..]))
}

/// The parts of a `CLOCK:` line with two timestamps: the inside of the
/// first and of the second, and where ` => 1:23` starts, relative to the
/// start of the line.
struct Clock {
    first: (usize, usize),
    second: (usize, usize),
    duration: Option<usize>,
}

fn clock_parts(line: &str, fold: bool) -> Option<Clock> {
    let bytes = line.as_bytes();
    let indent = bytes.iter().take_while(|b| matches!(b, b' ' | b'\t')).count();
    let keyword = bytes.get(indent..indent + 6)?;
    if !(keyword == b"CLOCK:" || (fold && keyword.eq_ignore_ascii_case(b"CLOCK:"))) {
        return None;
    }
    let mut at = indent + 6;
    at += bytes[at..].iter().take_while(|b| **b == b' ').count();
    // `[[<]\([^]>]+\)[]>]`
    let bracketed = |at: usize| -> Option<(usize, usize)> {
        if !matches!(bytes.get(at), Some(b'[' | b'<')) {
            return None;
        }
        let len = bytes[at + 1..].iter().position(|b| matches!(b, b']' | b'>'))?;
        (len > 0).then_some((at + 1, at + 1 + len))
    };
    let first = bracketed(at)?;
    let dashes = bytes[first.1 + 1..].iter().take_while(|b| **b == b'-').count();
    if dashes == 0 {
        return None;
    }
    let second = bracketed(first.1 + 1 + dashes)?;
    let after = second.1 + 1;
    let blanks = bytes[after..].iter().take_while(|b| matches!(b, b' ' | b'\t')).count();
    let duration = line[after + blanks..].starts_with("=>").then_some(after);
    Some(Clock { first, second, duration })
}

/// `org-clock-update-time-maybe`: on a `CLOCK:` line with two timestamps,
/// write both in the standard form and recompute the duration. `point` is
/// the cursor, which is on the line; returns where it is afterwards.
fn update_clock(text: &mut String, point: usize, rounding: i64, fold: bool) -> Result<usize, Refusal> {
    let line_start = text[..point].rfind('\n').map_or(0, |index| index + 1);
    let line = |text: &str| text[line_start..].split('\n').next().unwrap_or("").to_string();
    // The cursor was at a timestamp, whatever else is wrong with the line.
    let failed = |refusal: Refusal| Refusal::Failed(refusal.message().to_string());
    let rewrite = Shift { n: 0, what: Some(Unit::Day), updown: false, counted: false };
    let alone = Around { clock: false, drop_delay: false, fold };
    // Emacs keeps the cursor as a marker while it rewrites the line, and
    // then does not let it end up behind the place it was at, which it
    // remembers as a count of characters.
    let mut marker = point;
    let column = text[line_start..point].chars().count();
    let limit = |text: &str, marker: usize| {
        let line = &text[line_start..];
        marker.min(line_start + line.char_indices().nth(column).map_or(line.len(), |(at, _)| at))
    };
    for second in [false, true] {
        let Some(clock) = clock_parts(&line(text), fold) else { return Ok(point) };
        let inside = line_start + if second { clock.second.0 } else { clock.first.0 };
        let old = Stamp::around(text, inside);
        let new = change_in_place(text, inside, &rewrite, rounding, alone).map_err(failed)?.stamp;
        if let Some(old) = old {
            if marker > old.end {
                marker = marker + (new.1 - new.0) - (old.end - old.start);
            } else if marker >= old.start {
                marker = new.1;
            }
        }
    }
    let now = line(text);
    let Some(clock) = clock_parts(&now, fold) else { return Ok(limit(text, marker)) };
    let minutes = |range: (usize, usize)| {
        let parsed = parse_time_string(&now[range.0..range.1]);
        parsed.map(|parsed| {
            let (hour, minute) = parsed.time.unwrap_or((0, 0));
            let time = DateTime::new(parsed.date, hour, minute);
            time.day * 1440 + time.minute
        })
    };
    let (Some(from), Some(to)) = (minutes(clock.first), minutes(clock.second)) else {
        return Err(Refusal::Failed("Not an Org time string".to_string()));
    };
    let length = (to - from).abs();
    let duration = if to < from {
        format!(" => -{}:{:02}", length / 60, length % 60)
    } else {
        format!(" => {:2}:{:02}", length / 60, length % 60)
    };
    let line_end = line_start + now.len();
    match clock.duration {
        Some(start) => {
            marker = marker.min(line_start + start);
            text.replace_range(line_start + start..line_end, &duration);
        }
        None => text.insert_str(line_end, &duration),
    }
    Ok(limit(text, marker))
}

/// The matches of `org-ts-regexp3` on the line that starts at
/// `line_start`, one after the other.
fn on_line(text: &str, line_start: usize) -> Vec<Stamp> {
    let line_end = text[line_start..].find('\n').map_or(text.len(), |index| line_start + index);
    let mut found = Vec::new();
    let mut at = line_start;
    while at < line_end {
        match Stamp::at(text, at) {
            Some(stamp) => {
                at = stamp.end;
                found.push(stamp);
            }
            None => at += 1,
        }
    }
    found
}

/// How `change_in_place` treats what surrounds the timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Around {
    /// Recompute the duration of a `CLOCK:` line.
    pub clock: bool,
    /// Drop one-time delays like ` --2d`, which have served their purpose
    /// once the timestamp moves on.
    pub drop_delay: bool,
    /// Read keywords and units without regard to case, as Org does when
    /// the command is called from the keyboard.
    pub fold: bool,
}

/// What `change_in_place` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Placed {
    /// Where the timestamp is now.
    pub stamp: (usize, usize),
    /// Where Emacs leaves the cursor.
    pub cursor: usize,
}

/// `org-timestamp-change` with the cursor at byte `pos`.
pub(crate) fn change_in_place(
    text: &mut String,
    pos: usize,
    shift: &Shift,
    rounding: i64,
    around: Around,
) -> Result<Placed, Refusal> {
    let not_here = || Refusal::NotHere("Not at a timestamp".to_string());
    let stamp = Stamp::around(text, pos).ok_or_else(not_here)?;
    let part = stamp.part(text, pos);
    if shift.what.is_none() && part == Part::Bracket {
        // `org-toggle-timestamp-type`
        let toggled: String = text[stamp.start..stamp.end]
            .chars()
            .map(|c| match c {
                '[' => '<',
                ']' => '>',
                '<' => '[',
                '>' => ']',
                other => other,
            })
            .collect();
        text.replace_range(stamp.start..stamp.end, &toggled);
        return Ok(Placed { stamp: (stamp.start, stamp.end), cursor: pos });
    }
    let unit = shift.what.or(match part {
        Part::Year => Some(Unit::Year),
        Part::Month => Some(Unit::Month),
        Part::Day => Some(Unit::Day),
        Part::Hour => Some(Unit::Hour),
        Part::Minute => Some(Unit::Minute),
        Part::Bracket | Part::After | Part::Extra(_) => None,
    });
    let old = text[stamp.start..stamp.end].to_string();
    let inactive = old.starts_with('[');
    let mut extra = timestamp_extra(&old, around.fold).to_string();
    if around.drop_delay {
        // ` --[0-9]+[hdwmy]`
        while let Some(at) = extra.find(" --") {
            let digits = extra[at + 3..].bytes().take_while(u8::is_ascii_digit).count();
            if digits == 0 || !is_unit(extra.as_bytes().get(at + 3 + digits), true, around.fold) {
                break;
            }
            extra.replace_range(at..at + 3 + digits + 1, "");
        }
    }
    let parsed = parse_time_string(&old).ok_or_else(not_here)?;
    let (hour, mut minute) = parsed.time.unwrap_or((0, 0));

    let mut step = rounding.max(1);
    let mut increment = shift.n;
    if shift.updown && unit == Some(Unit::Minute) && !shift.counted {
        // One rounding step, from a time rounded to such steps.
        increment = step * shift.n.signum();
        let rest = minute % step;
        if rest != 0 {
            minute += if shift.n > 0 { -rest } else { step - rest };
        }
    } else {
        step = 1;
    }
    let add = |what: Unit| if unit == Some(what) { increment } else { 0 };
    let date = Date::new(
        parsed.date.year + add(Unit::Year),
        parsed.date.month + add(Unit::Month),
        parsed.date.day + add(Unit::Day),
    );
    let time = DateTime::new(date, hour + add(Unit::Hour), minute + add(Unit::Minute));

    // A change to the start of a time range moves its end as well.
    let has_range = extra
        .as_bytes()
        .windows(6)
        .any(|window| matches!(window, [b'-', b'0'..=b'2', b'0'..=b'9', b':', b'0'..=b'5', b'0'..=b'9']));
    match unit {
        Some(Unit::Hour) if has_range => extra = modify_extra(&extra, 2, shift.n, step, around.fold)?,
        Some(Unit::Minute) if has_range => extra = modify_extra(&extra, 5, shift.n, step, around.fold)?,
        _ => {}
    }
    if shift.what.is_none() {
        if let Part::Extra(at) = part {
            extra = modify_extra(&extra, at, shift.n, step, around.fold)?;
        }
    }
    // The cursor counted in characters from the start of the timestamp.
    let origin = text[stamp.start..pos].chars().count();
    let new = format_timestamp(time, has_time(&old), inactive, &extra);
    text.replace_range(stamp.start..stamp.end, &new);
    let end = stamp.start + new.len();

    // The cursor stays in the part it was in, though that part may have
    // moved or shrunk. Elsewhere it keeps its distance from the start,
    // even if that is past the end of a timestamp that became shorter.
    let origin = match origin.checked_sub(new.chars().count()) {
        None => stamp.start + new.char_indices().nth(origin).map_or(new.len(), |(at, _)| at),
        Some(past) => end + text[end..].char_indices().nth(past).map_or(text.len() - end, |(at, _)| at),
    };
    let mut cursor = match (part, Stamp::at(text, stamp.start)) {
        (Part::After, Some(now)) => now.end,
        (Part::Day, Some(now)) => {
            let last = match (now.name, now.hour) {
                (Some((_, name_end)), _) => name_end - 1,
                (None, Some((hour, _))) => hour,
                (None, None) => now.start + 10,
            };
            last.min(origin)
        }
        (Part::Hour, Some(Stamp { hour: Some((_, hour_end)), .. })) => hour_end.min(origin),
        (Part::Minute, Some(Stamp { minute: Some((_, minute_end)), .. })) => (minute_end - 1).min(origin),
        (Part::Extra(_), Some(now)) => (now.end - 1).min(origin),
        _ => origin,
    };
    let mut range = (stamp.start, end);
    if around.clock {
        // The clock update looks at the line the cursor is on, and may
        // rewrite a timestamp before this one.
        let line_start = text[..stamp.start].rfind('\n').map_or(0, |index| index + 1);
        let ordinal = on_line(text, line_start).iter().position(|other| other.start == stamp.start);
        cursor = update_clock(text, cursor, rounding, around.fold)?;
        if let Some(moved) = ordinal.and_then(|ordinal| on_line(text, line_start).get(ordinal).copied()) {
            range = (moved.start, moved.end);
        }
    }
    Ok(Placed { stamp: range, cursor })
}

/// Where the match of `org-ts-regexp` that opens at `start` ends: an
/// active timestamp, read up to the first closing bracket.
fn active_end(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.get(start) != Some(&b'<') || !bytes.get(start + 1..start + 11).is_some_and(is_date) {
        return None;
    }
    match bytes.get(start + 11) {
        Some(b'>') => Some(start + 12),
        Some(b' ') => {
            let close = bytes[start + 12..].iter().position(|b| matches!(b, b'>' | b'\n'))?;
            (bytes[start + 12 + close] == b'>').then_some(start + 13 + close)
        }
        _ => None,
    }
}

/// `org-at-date-range-p`: the date range `<…>--<…>` that `pos` is in or
/// directly after. Returns where its two timestamps start.
pub fn date_range_at(text: &str, pos: usize) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut at = pos.min(text.len());
    // Two tries: from the nearest opening bracket before `pos`, and from
    // the one before that.
    for _ in 0..2 {
        while at > 0 && !matches!(bytes[at - 1], b'[' | b'<' | b'\r' | b'\n') {
            at -= 1;
        }
        while at > 0 && matches!(bytes[at - 1], b'[' | b'<') {
            at -= 1;
        }
        let range = active_end(text, at).and_then(|first_end| {
            let dashes = bytes[first_end..].iter().take_while(|b| **b == b'-').count();
            let second = first_end + dashes;
            let end = active_end(text, second).filter(|_| (1..=3).contains(&dashes))?;
            (end >= pos).then_some((at, second))
        });
        if range.is_some() {
            return range;
        }
    }
    None
}

/// A changed text and where Emacs leaves the cursor in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changed {
    pub text: String,
    pub cursor: usize,
}

/// Change the timestamp at byte `pos` of `text`. `rounding` is the second
/// number of `org-time-stamp-rounding-minutes`.
pub fn change(text: &str, pos: usize, shift: &Shift, rounding: i64) -> Result<Changed, Refusal> {
    let mut pos = pos.min(text.len());
    while !text.is_char_boundary(pos) {
        pos -= 1;
    }
    let mut text = text.to_string();
    let around = Around { clock: true, drop_delay: false, fold: true };
    let cursor = change_in_place(&mut text, pos, shift, rounding, around)?.cursor;
    Ok(Changed { text, cursor })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Apply `shift` with the cursor at `¦`.
    fn run(marked: &str, shift: Shift) -> Result<String, Refusal> {
        let pos = marked.find('¦').expect("cursor marker");
        Ok(change(&marked.replacen('¦', "", 1), pos, &shift, 5)?.text)
    }

    /// Like `up`, with the cursor marked in the result.
    fn up_with_cursor(marked: &str) -> String {
        let pos = marked.find('¦').expect("cursor marker");
        let shift = Shift { n: 1, what: None, updown: true, counted: false };
        let mut changed = change(&marked.replacen('¦', "", 1), pos, &shift, 5).unwrap();
        changed.text.insert(changed.cursor, '¦');
        changed.text
    }

    fn up(marked: &str) -> String {
        run(marked, Shift { n: 1, what: None, updown: true, counted: false }).unwrap()
    }

    fn down(marked: &str) -> String {
        run(marked, Shift { n: -1, what: None, updown: true, counted: false }).unwrap()
    }

    #[test]
    fn finds_the_timestamp_around_a_position() {
        let text = "a <2026-05-18 Mon> b [2026-05-19 Tue 9:30] c";
        assert_eq!(Stamp::around(text, 1), None);
        assert_eq!(Stamp::around(text, 2).map(|stamp| (stamp.start, stamp.end)), Some((2, 18)));
        // Directly after a timestamp still counts.
        assert_eq!(Stamp::around(text, 18).map(|stamp| stamp.start), Some(2));
        assert_eq!(Stamp::around(text, 19), None);
        assert_eq!(Stamp::around(text, 30).map(|stamp| (stamp.start, stamp.end)), Some((21, 42)));
        assert_eq!(Stamp::at("<2026-05-10 Sun 10:00-11:00 +1m -2d> x", 0).map(|stamp| stamp.end), Some(36));
        assert_eq!(Stamp::at("<2026-05-10 Sun and far too many characters here>", 0), None);
        assert_eq!(Stamp::at("<2026-05-10 Sun\n>", 0), None);
    }

    #[test]
    fn changes_the_part_under_the_cursor() {
        assert_eq!(up("<20¦26-05-18 Mon>"), "<2027-05-18 Tue>");
        assert_eq!(up("<2026-0¦5-18 Mon>"), "<2026-06-18 Thu>");
        assert_eq!(up("<2026-05-1¦8 Mon>"), "<2026-05-19 Tue>");
        assert_eq!(down("<2026-05-18 M¦on>"), "<2026-05-17 Sun>");
        assert_eq!(up("[2026-05-18 Mon 1¦0:30]"), "[2026-05-18 Mon 11:30]");
        // A month too far rolls over like `encode-time` does.
        assert_eq!(up("<2026-0¦1-31 Sat>"), "<2026-03-03 Tue>");
        assert_eq!(down("<2026-01-0¦1 Thu>"), "<2025-12-31 Wed>");
    }

    #[test]
    fn minutes_move_in_rounded_steps() {
        assert_eq!(up("<2026-05-18 Mon 10:5¦8>"), "<2026-05-18 Mon 11:00>");
        assert_eq!(down("<2026-05-18 Mon 10:5¦8>"), "<2026-05-18 Mon 10:55>");
        assert_eq!(up("<2026-05-18 Mon 23:5¦5>"), "<2026-05-19 Tue 00:00>");
        let counted = Shift { n: 3, what: None, updown: true, counted: true };
        assert_eq!(run("<2026-05-18 Mon 10:5¦8>", counted).unwrap(), "<2026-05-18 Mon 11:01>");
    }

    #[test]
    fn keeps_and_changes_what_follows_the_time() {
        assert_eq!(up("<2026-05-18 Mon 10:5¦8-11:00 +1w -2d>"), "<2026-05-18 Mon 11:00-11:05 +1w -2d>");
        assert_eq!(up("<2026-05-18 Mon 1¦0:58-11:00>"), "<2026-05-18 Mon 11:58-12:00>");
        // The end of a range moves alone, and by single minutes.
        assert_eq!(up("<2026-05-18 Mon 10:58-11:0¦0 +1w>"), "<2026-05-18 Mon 10:58-11:01 +1w>");
        assert_eq!(down("<2026-05-18 Mon 10:58-1¦1:00>"), "<2026-05-18 Mon 10:58-10:00>");
        assert_eq!(up("<2026-05-18 Mon +¦1w -2d>"), "<2026-05-18 Mon +2w -2d>");
        assert_eq!(down("<2026-05-18 Mon +¦1w>"), "<2026-05-18 Mon +1w>");
        assert_eq!(up("<2026-05-18 Mon +1¦w>"), "<2026-05-18 Mon +1m>");
        assert_eq!(down("<2026-05-18 Mon +1w -¦2d>"), "<2026-05-18 Mon +1w -1d>");
        assert_eq!(down("<2026-05-18 Mon +1w -2¦d>"), "<2026-05-18 Mon +1w -2d>");
        // Other repeaters are kept as they are.
        assert_eq!(up("<2026-05-18 Mon .+¦1w>"), "<2026-05-18 Mon .+1w>");
    }

    #[test]
    fn keeps_what_a_timestamp_carries() {
        assert_eq!(timestamp_extra("<2026-05-10 Sun 10:00-11:00 +1m -2d>", false), "-11:00 +1m -2d");
        assert_eq!(timestamp_extra("<2026-05-10 Sun +1w>", false), " +1w");
        assert_eq!(timestamp_extra("<2026-05-10 Sun>", false), "");
        assert_eq!(timestamp_extra("<2026-05-10 Sun foo .+2d/4d --1d>", false), " .+2d/4d --1d");
        // A capital unit only counts where Org folds case.
        assert_eq!(timestamp_extra("<2026-05-10 Sun +1W>", false), "");
        assert_eq!(timestamp_extra("<2026-05-10 Sun +1W>", true), " +1W");
    }

    #[test]
    fn brackets_switch_the_type() {
        assert_eq!(up("¦<2026-05-18 Mon>"), "[2026-05-18 Mon]");
        assert_eq!(down("[2026-05-18 Mon 10:00¦]"), "<2026-05-18 Mon 10:00>");
        // With a unit given, the brackets are a place like any other.
        let day = Shift { n: 1, what: Some(Unit::Day), updown: true, counted: false };
        assert_eq!(run("¦<2026-05-18>", day).unwrap(), "<2026-05-19 Tue>");
        assert_eq!(run("<2026-05-18 Mon>¦", day).unwrap(), "<2026-05-19 Tue>");
    }

    #[test]
    fn a_clock_line_gets_its_duration_again() {
        assert_eq!(
            up("CLOCK: [2026-05-18 Mon 10:5¦8]--[2026-05-18 Mon 11:20] =>  0:22"),
            "CLOCK: [2026-05-18 Mon 11:00]--[2026-05-18 Mon 11:20] =>  0:20"
        );
        let back = Shift { n: -30, what: None, updown: true, counted: true };
        assert_eq!(
            run("  CLOCK: [2026-05-18 Mon 10:58]--[2026-05-18 Mon 11:2¦0]", back).unwrap(),
            "  CLOCK: [2026-05-18 Mon 10:58]--[2026-05-18 Mon 10:50] => -0:08"
        );
        // A running clock has nothing to recompute.
        assert_eq!(up("CLOCK: [2026-05-18 Mon 10:5¦8]"), "CLOCK: [2026-05-18 Mon 11:00]");
    }

    #[test]
    fn the_cursor_stays_in_its_part() {
        assert_eq!(up_with_cursor("<2026-05-1¦8 Mon>"), "<2026-05-1¦9 Tue>");
        // The time grows by a digit; the cursor does not move with it.
        assert_eq!(up_with_cursor("<2026-05-18 Mon 9:5¦8>"), "<2026-05-18 Mon 10:¦00>");
        // A long day name becomes a short one.
        assert_eq!(up_with_cursor("<2026-05-18 Monda¦y>"), "<2026-05-19 Tu¦e>");
        assert_eq!(up_with_cursor("<2026-05-18 Monday>¦ x"), "<2026-05-18 Mon>¦ x");
        assert_eq!(up_with_cursor("<2026-05-18 Mon +¦1w>"), "<2026-05-18 Mon +¦2w>");
    }

    #[test]
    fn capital_units_are_kept_but_not_changed() {
        assert_eq!(up("<2026-05-1¦8 Mon +1W>"), "<2026-05-19 Tue +1W>");
        assert_eq!(up("<2026-05-18 Mon +¦1W>"), "<2026-05-18 Mon +2W>");
        let on_unit = run("<2026-05-18 Mon +1¦W>", Shift { n: 1, what: None, updown: true, counted: false });
        assert_eq!(on_unit, Err(Refusal::Failed("Cannot change this unit".to_string())));
    }

    #[test]
    fn finds_date_ranges() {
        let text = "a <2026-05-18 Mon>--<2026-05-20 Wed 10:00> b <2026-05-25 Mon>";
        assert_eq!(date_range_at(text, 3), Some((2, 20)));
        assert_eq!(date_range_at(text, 25), Some((2, 20)));
        // Directly after the range, where the agenda keeps its marker.
        assert_eq!(date_range_at(text, 42), Some((2, 20)));
        assert_eq!(date_range_at(text, 43), None);
        assert_eq!(date_range_at(text, 50), None);
        assert_eq!(date_range_at("[2026-05-18 Mon]--[2026-05-20 Wed]", 5), None);
        assert_eq!(date_range_at("<2026-05-18 Mon>----<2026-05-20 Wed>", 5), None);
    }

    #[test]
    fn refuses_outside_a_timestamp() {
        let refused = run("no ¦stamp <2026-05-18 Mon>", Shift { n: 1, what: None, updown: true, counted: false });
        assert_eq!(refused, Err(Refusal::NotHere("Not at a timestamp".to_string())));
    }
}
