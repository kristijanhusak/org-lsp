//! Changing the priority of an entry: a port of `org-priority`.
//!
//! A priority is a capital letter or a number from 0 to 64, written as a
//! cookie in the headline: `[#A]`, `[#3]`. Like Org, the code handles both
//! as numbers: a letter stands for its character code, so `A` is 65 and a
//! *higher* priority is a *smaller* number.

use org_syntax::TodoKeywords;

use crate::todo::{headline_level, tags_padding};
use crate::Refusal;

/// The priorities in use: `org-priority-highest`, `org-priority-lowest`
/// and `org-priority-default`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Priorities {
    pub highest: i64,
    pub lowest: i64,
    pub default: i64,
}

impl Default for Priorities {
    fn default() -> Self {
        Priorities { highest: i64::from(b'A'), lowest: i64::from(b'C'), default: i64::from(b'B') }
    }
}

/// `org-priority-to-value`: the number in `text`, or else the code of
/// its first character.
pub fn value(text: &str) -> Option<i64> {
    let digits: String = text.chars().skip_while(|c| !c.is_ascii_digit()).take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        text.chars().next().map(|c| i64::from(u32::from(c)))
    } else {
        digits.parse().ok()
    }
}

/// `org-priority-to-string`
fn name(value: i64) -> String {
    if (0..=64).contains(&value) {
        value.to_string()
    } else {
        u32::try_from(value).ok().and_then(char::from_u32).map(String::from).unwrap_or_default()
    }
}

impl Priorities {
    /// The value of a `#+PRIORITIES:` line: highest, lowest and default.
    pub fn from_keyword(text: &str) -> Option<Priorities> {
        let mut words = text.split_whitespace();
        Some(Priorities {
            highest: value(words.next()?)?,
            lowest: value(words.next()?)?,
            default: value(words.next()?)?,
        })
    }

    /// Whether priorities are numbers (`org-priority-number-p` of the
    /// lowest one).
    pub fn numeric(&self) -> bool {
        (0..=64).contains(&self.lowest)
    }

    /// `org-priority-valid-value-p`
    fn allows(&self, value: i64) -> bool {
        let known = (0..=64).contains(&value) || (i64::from(b'A')..=i64::from(b'Z')).contains(&value);
        known && self.lowest >= value && value >= self.highest
    }
}

/// Settings, named after Org's variables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub priorities: Priorities,
    /// `org-priority-start-cycle-with-default`
    pub start_cycle_with_default: bool,
    /// `org-tags-column`
    pub tags_column: i64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { priorities: Priorities::default(), start_cycle_with_default: true, tags_column: -77 }
    }
}

/// What to do with the priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// One step higher. From the highest priority the next step removes
    /// the cookie.
    Up,
    /// One step lower.
    Down,
    Remove,
    /// A priority as Org counts it: a letter's code, or 0 to 64.
    Set(i64),
}

/// What `org-priority` asks for: `Priority A-C, SPC to remove`.
pub fn question(priorities: &Priorities) -> String {
    format!("Priority {}-{}, SPC to remove", name(priorities.highest), name(priorities.lowest))
}

/// The answer to [`question`], read as `org-priority` reads it. Emacs
/// takes a single key, so only the first character counts, unless the
/// priorities are numbers that reach 10: then the whole answer is read. A
/// space removes the priority; a letter counts in either case. Whether
/// the priority is in range is for [`change`] to say.
pub fn read_answer(answer: &str, priorities: &Priorities) -> Result<Action, Refusal> {
    let read: String = if priorities.numeric() && priorities.lowest >= 10 {
        answer.to_string()
    } else {
        answer.chars().take(1).collect()
    };
    if read == " " {
        return Ok(Action::Remove);
    }
    if !priorities.numeric() {
        // `(string-to-char (upcase s))`
        let first = read.chars().flat_map(char::to_uppercase).next();
        return Ok(Action::Set(first.map_or(0, |c| i64::from(u32::from(c)))));
    }
    // `org-priority-value-regexp`, unanchored and without regard to case:
    // any letter or digit will do. Then `string-to-number`.
    if !read.chars().any(|c| c.is_ascii_alphanumeric()) {
        return Err(Refusal::Failed(format!(
            "Priority must be a number between `{}' and `{}'",
            name(priorities.highest),
            name(priorities.lowest)
        )));
    }
    string_to_number(&read).map(Action::Set).ok_or_else(|| {
        Refusal::Failed(format!(
            "Priority must be between `{}' and `{}'",
            name(priorities.highest),
            name(priorities.lowest)
        ))
    })
}

/// Emacs's `string-to-number` for a whole number: blanks, a sign, digits;
/// 0 when there are none. A fraction makes a float, which is no priority.
fn string_to_number(text: &str) -> Option<i64> {
    let text = text.trim_start_matches([' ', '\t']);
    let (sign, rest) = match text.as_bytes().first() {
        Some(b'-') => (-1, &text[1..]),
        Some(b'+') => (1, &text[1..]),
        _ => (1, text),
    };
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let after = &rest[digits.len()..];
    let fraction = after.starts_with('.') && after[1..].starts_with(|c: char| c.is_ascii_digit());
    let exponent = !digits.is_empty()
        && after.starts_with(['e', 'E'])
        && after[1..].trim_start_matches(['+', '-']).starts_with(|c: char| c.is_ascii_digit());
    if fraction || exponent {
        return None;
    }
    Some(sign * digits.parse::<i64>().unwrap_or(0))
}

/// The first priority cookie on a headline, as `org-priority` finds it:
/// where it starts, how long its value is, and whether a space follows.
/// Unlike the agenda, this command takes a small letter for a priority,
/// because Org matches with case folding on.
fn cookie(line: &str) -> Option<(usize, usize, bool)> {
    let bytes = line.as_bytes();
    line.match_indices("[#").find_map(|(at, _)| {
        let len = match bytes[at + 2..] {
            [a, b']', ..] if a.is_ascii_alphabetic() || a.is_ascii_digit() => 1,
            [b'1'..=b'5', b'0'..=b'9', b']', ..] | [b'6', b'0'..=b'4', b']', ..] => 2,
            _ => return None,
        };
        Some((at, len, bytes.get(at + 3 + len) == Some(&b' ')))
    })
}

/// Where a new cookie goes on a headline without one, and whether it
/// follows a TODO keyword (`org-todo-line-regexp`).
fn insertion_point(line: &str, keywords: &TodoKeywords) -> (usize, bool) {
    let stars = line.bytes().take_while(|b| *b == b'*').count();
    let spaces = line[stars..].bytes().take_while(|b| *b == b' ').count();
    let rest = &line[stars + spaces..];
    let keyword = keywords.names().find(|name| {
        rest.strip_prefix(name)
            .is_some_and(|after| after.starts_with(' ') || after.bytes().all(|b| matches!(b, b' ' | b'\t')))
    });
    match keyword {
        Some(name) => (stars + spaces + name.len(), true),
        None => (stars + spaces, false),
    }
}

/// Change the priority of the entry that contains byte `offset` and
/// return the new text.
///
/// `repeated` says that the same command ran just before on this entry:
/// stepping on from "no priority" then wraps around to the other end.
pub fn change(
    text: &str,
    keywords: &TodoKeywords,
    offset: usize,
    action: Action,
    repeated: bool,
    settings: &Settings,
) -> Result<String, Refusal> {
    // The headline at or above the cursor.
    let line_start = |at: usize| text[..at].rfind('\n').map_or(0, |index| index + 1);
    let line_at = |start: usize| text[start..].split('\n').next().unwrap_or("");
    let mut heading = line_start(offset.min(text.len()));
    while headline_level(line_at(heading)).is_none() {
        if heading == 0 {
            return Err(Refusal::NotHere("Before first headline".to_string()));
        }
        heading = line_start(heading - 1);
    }
    let line = line_at(heading);
    let priorities = &settings.priorities;
    let found = cookie(line);
    let current = found.and_then(|(at, len, _)| value(&line[at + 2..at + 2 + len]));
    let stepping = matches!(action, Action::Up | Action::Down);
    let between = || format!("between `{}' and `{}'", name(priorities.highest), name(priorities.lowest));

    let wanted = match action {
        Action::Remove => None,
        Action::Set(value) if priorities.allows(value) => Some(value),
        Action::Set(_) => return Err(Refusal::Failed(format!("Priority must be {}", between()))),
        Action::Up | Action::Down => {
            let step = if action == Action::Up { -1 } else { 1 };
            let value = match current {
                Some(current) => current + step,
                // From no priority: around to the far end, or the default.
                None if repeated && action == Action::Up => priorities.lowest,
                None if repeated => priorities.highest,
                None if settings.start_cycle_with_default => priorities.default,
                None => priorities.default + step,
            };
            if priorities.allows(value) {
                Some(value)
            } else if current.is_none() && !repeated {
                return Err(Refusal::Failed("The default can not be set, see `org-priority-default' why".to_string()));
            } else {
                // Past the highest or lowest priority comes none.
                None
            }
        }
    };
    debug_assert!(stepping || action == Action::Remove || wanted.is_some());

    let mut new_line = line.to_string();
    match (found, wanted) {
        (Some((at, len, space)), None) => new_line.replace_range(at..at + 3 + len + usize::from(space), ""),
        (Some((at, len, _)), Some(value)) => new_line.replace_range(at + 2..at + 2 + len, &name(value)),
        (None, None) => return Err(Refusal::Failed("No priority cookie found in line".to_string())),
        (None, Some(value)) => {
            let (at, after_keyword) = insertion_point(line, keywords);
            let cookie = if after_keyword { format!(" [#{}]", name(value)) } else { format!("[#{}] ", name(value)) };
            new_line.insert_str(at, &cookie);
        }
    }
    if let Some((blanks, tags, padding)) = tags_padding(&new_line, settings.tags_column) {
        new_line.replace_range(blanks..tags, &padding);
    }
    Ok(format!("{}{new_line}{}", &text[..heading], &text[heading + line.len()..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(marked: &str, action: Action, repeated: bool) -> Result<String, Refusal> {
        let offset = marked.find('¦').expect("cursor marker");
        let text = marked.replacen('¦', "", 1);
        change(&text, &TodoKeywords::default(), offset, action, repeated, &Settings::default())
    }

    fn up(marked: &str) -> String {
        run(marked, Action::Up, false).unwrap()
    }

    fn down(marked: &str) -> String {
        run(marked, Action::Down, false).unwrap()
    }

    #[test]
    fn steps_through_the_priorities() {
        assert_eq!(up("* a¦\n"), "* [#B] a\n");
        assert_eq!(up("* [#B] a¦\n"), "* [#A] a\n");
        assert_eq!(up("* [#A] a¦\n"), "* a\n");
        assert_eq!(down("* a¦\n"), "* [#B] a\n");
        assert_eq!(down("* [#B] a¦\n"), "* [#C] a\n");
        assert_eq!(down("* [#C] a¦\n"), "* a\n");
        // Pressed again after the cookie went away: on from the far end.
        assert_eq!(run("* a¦\n", Action::Up, true).unwrap(), "* [#C] a\n");
        assert_eq!(run("* a¦\n", Action::Down, true).unwrap(), "* [#A] a\n");
    }

    #[test]
    fn puts_the_cookie_after_the_keyword() {
        assert_eq!(up("* TODO a\nbo¦dy\n"), "* TODO [#B] a\nbody\n");
        assert_eq!(up("* TODO¦\n"), "* TODO [#B]\n");
        assert_eq!(up("* ¦\n"), "* [#B] \n");
        assert_eq!(up("** TODOS a¦"), "** [#B] TODOS a");
    }

    #[test]
    fn realigns_the_tags() {
        assert_eq!(up("* [#A] a¦ :t:\n"), format!("* a{}:t:\n", " ".repeat(71)));
        assert_eq!(up("* a¦ :t:\n"), format!("* [#B] a{}:t:\n", " ".repeat(66)));
    }

    #[test]
    fn sets_and_removes() {
        assert_eq!(run("* a¦", Action::Set(65), false).unwrap(), "* [#A] a");
        assert_eq!(run("* [#C] a¦", Action::Remove, false).unwrap(), "* a");
        assert_eq!(
            run("* a¦", Action::Set(70), false),
            Err(Refusal::Failed("Priority must be between `A' and `C'".to_string()))
        );
        assert_eq!(
            run("* a¦", Action::Remove, false),
            Err(Refusal::Failed("No priority cookie found in line".to_string()))
        );
        assert_eq!(run("intro¦\n* a", Action::Up, false), Err(Refusal::NotHere("Before first headline".to_string())));
    }

    #[test]
    fn follows_the_configured_range() {
        let numbers = Priorities::from_keyword("1 5 3").unwrap();
        assert_eq!(numbers, Priorities { highest: 1, lowest: 5, default: 3 });
        let settings = Settings { priorities: numbers, ..Settings::default() };
        let run =
            |text: &str, action| change(text, &TodoKeywords::default(), text.len(), action, false, &settings).unwrap();
        assert_eq!(run("* a", Action::Up), "* [#3] a");
        assert_eq!(run("* [#1] a", Action::Up), "* a");
        assert_eq!(run("* [#5] a", Action::Up), "* [#4] a");
        assert_eq!(Priorities::from_keyword("A E"), None);
        assert_eq!(Priorities::from_keyword("A E C").unwrap().lowest, 69);
    }

    #[test]
    fn asks_and_reads_the_answer() {
        let letters = Priorities::default();
        assert_eq!(question(&letters), "Priority A-C, SPC to remove");
        assert_eq!(read_answer("b", &letters), Ok(Action::Set(i64::from(b'B'))));
        assert_eq!(read_answer("ab", &letters), Ok(Action::Set(i64::from(b'A'))), "one key");
        assert_eq!(read_answer(" ", &letters), Ok(Action::Remove));
        let small = Priorities { highest: 1, lowest: 9, default: 5 };
        assert_eq!(question(&small), "Priority 1-9, SPC to remove");
        assert_eq!(read_answer("12", &small), Ok(Action::Set(1)), "one key");
        assert_eq!(read_answer("x", &small), Ok(Action::Set(0)), "`string-to-number`");
        assert!(read_answer("", &small).is_err());
        let large = Priorities { highest: 1, lowest: 15, default: 5 };
        assert_eq!(read_answer("12", &large), Ok(Action::Set(12)), "the whole answer");
        assert_eq!(read_answer(" ", &large), Ok(Action::Remove));
        assert!(read_answer("1.5", &large).is_err(), "a float");
    }
}
