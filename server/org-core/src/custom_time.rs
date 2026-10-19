//! Timestamps shown in formats of one's own (`org-display-custom-times`,
//! `org-timestamp-custom-formats`): what `org-activate-dates` lays over
//! each one with `org-display-custom-time`.

use std::sync::LazyLock;

use regex::Regex;

use crate::date::{Date, DateTime};

/// `org-timestamp-custom-formats`, Org's default: the date, and the date
/// with a time.
pub const DEFAULT_FORMATS: (&str, &str) = ("%m/%d/%y %a", "%m/%d/%y %a %H:%M");

/// `org-tsr-regexp-both`: a timestamp, or a range of two; the innards of
/// each in a group.
static RANGE: LazyLock<Regex> = LazyLock::new(|| {
    let stamp = r"[\[<]([0-9]{4}-[0-9]{2}-[0-9]{2}(?: .*?)?)[\]>]";
    let second = stamp.replace("([0-9]", "(?P<second>[0-9]");
    Regex::new(&format!(r"{stamp}(?:--?-?{second})?")).expect("a valid regex")
});

/// `org-ts-regexp0`: the date, a day name, a time.
static TIME0: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"([0-9]{4})-([0-9]{2})-([0-9]{2})(?: +[^\]+0-9>\r\n -]+)?(?: +([0-9]{1,2}):([0-9]{2}))?")
        .expect("a valid regex")
});

/// What `org-display-custom-time` leaves out at the end: a time range's
/// end and a repeater.
static TAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:-[0-9]+:[0-9]+)?(?: [.+]?\+[0-9]+[hdwmy](?:/[0-9]+[hdwmy])?)?$").expect("a valid regex")
});

/// The text each timestamp of `text` shows in place of its own, as
/// `(start, end, shown)`, byte offsets: the date and time inside the
/// brackets in `formats` (`org-timestamp-custom-formats`), its brackets
/// dropped; a time range's end and a repeater stay as they are.
pub fn displays(text: &str, formats: (&str, &str)) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(found) = RANGE.captures_at(text, at) {
        let whole = found.get(0).expect("a match");
        at = whole.end().max(whole.start() + 1);
        // Not one inside brackets, as `[[<…>]]`.
        if text[..whole.start()].ends_with('[') {
            at = whole.start() + 1;
            continue;
        }
        for inner in [found.get(1), found.name("second")].into_iter().flatten() {
            if let Some(shown) = display(inner.as_str(), formats) {
                let tail = TAIL.find(inner.as_str()).map_or(0, |tail| tail.len());
                out.push((inner.start(), inner.end() - tail, shown));
            }
        }
    }
    out
}

/// The innards of one timestamp in `formats`.
fn display(stamp: &str, formats: (&str, &str)) -> Option<String> {
    let found = TIME0.captures(stamp)?;
    let number = |index: usize| found.get(index).and_then(|part| part.as_str().parse::<i64>().ok());
    let date = Date::new(number(1)?, number(2)?, number(3)?);
    let time = number(4).zip(number(5));
    let format = if time.is_some() { formats.1 } else { formats.0 };
    let (hour, minute) = time.unwrap_or((0, 0));
    Some(crate::capture::format_time(strip_brackets(format), DateTime::new(date, hour, minute)))
}

/// `org-time-stamp-format` with `no-brackets`: a format without the
/// brackets around it.
fn strip_brackets(format: &str) -> &str {
    let bracketed =
        (format.starts_with('<') && format.ends_with('>')) || (format.starts_with('[') && format.ends_with(']'));
    if bracketed && format.len() >= 2 {
        &format[1..format.len() - 1]
    } else {
        format
    }
}
