//! `org-agenda-entry-text-mode` (`E`): some text of each entry under its
//! line in the agenda, as `org-agenda-get-some-entry-text` takes it.

use regex::{Regex, RegexBuilder};
use std::sync::LazyLock;

use super::matcher::emacs_buffer_regex;
use crate::outline::Outline;

/// `org-agenda-entry-text-maxlines`, `org-agenda-entry-text-leaders` and
/// `org-agenda-entry-text-exclude-regexps`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub max_lines: usize,
    pub leaders: String,
    /// Emacs regexps whose matches are left out.
    pub exclude: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { max_lines: 5, leaders: "    > ".to_string(), exclude: Vec::new() }
    }
}

/// `org-drawer-regexp`, without regard to case as `re-search-forward`
/// searches.
static DRAWER: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"^[ \t]*:((?:\w|[-_])+):[ \t]*$").multi_line(true).case_insensitive(true).build().unwrap()
});
static DRAWER_END: LazyLock<Regex> =
    LazyLock::new(|| RegexBuilder::new(r"^[ \t]*:END:.*\n?").multi_line(true).case_insensitive(true).build().unwrap());
/// A line that starts with a planning or clock keyword and a timestamp
/// (`org-keyword-time-regexp`); the timestamp may run over lines.
static KEYWORD_TIME: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"^[ \t]*\b(?:CLOCK:|CLOSED:|DEADLINE:|SCHEDULED:) *[\[<][^\]>]+[\]>].*\n?")
        .multi_line(true)
        .case_insensitive(true)
        .build()
        .unwrap()
});

/// Delete every match of `regex` in `text`, searching again from where
/// each was, as a `re-search-forward` and `replace-match` loop does.
fn delete_all(text: &mut String, regex: &Regex) {
    let mut at = 0;
    while at <= text.len() {
        let Some(found) = regex.find_at(text, at) else { break };
        if found.is_empty() {
            // Emacs would search at the same place forever.
            at = found.end() + text[found.end()..].chars().next().map_or(1, char::len_utf8);
            continue;
        }
        at = found.start();
        text.replace_range(found.range(), "");
    }
}

/// Spaces for the tabs of each line, to the next multiple of eight
/// columns (`untabify`).
fn untabify(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut column = 0;
    for c in text.chars() {
        match c {
            '\t' => {
                let next = (column / 8 + 1) * 8;
                out.extend(std::iter::repeat_n(' ', next - column));
                column = next;
            }
            '\n' => {
                out.push(c);
                column = 0;
            }
            _ => {
                out.push(c);
                column += crate::text::char_width(c);
            }
        }
    }
    out
}

fn indentation(line: &str) -> usize {
    line.chars().take_while(|c| *c == ' ' || *c == '\t').count()
}

fn blank(line: &str) -> bool {
    line.chars().all(|c| c == ' ' || c == '\t')
}

/// The lines `E` shows under an agenda line of entry `entry` of `outline`:
/// its text up to the next headline without drawers, planning and clock
/// lines and what `exclude` matches, the common indentation taken away,
/// each line after `leaders`, at most `lines` of them. None when there is
/// nothing but white space.
pub fn entry_text(outline: &Outline, entry: usize, lines: usize, settings: &Settings) -> Vec<String> {
    let source = &outline.text;
    let heading = &outline.entries[entry];
    let start = (heading.line_end + 1).min(source.len());
    let end = outline.entries.get(entry + 1).map_or(source.len(), |next| next.offset).max(start);
    let mut text = source[start..end].to_string();
    // Drawers, up to their `:END:`, or to the end without one.
    let mut at = 0;
    while let Some(found) = DRAWER.find_at(&text, at) {
        let until = DRAWER_END.find_at(&text, found.end()).map_or(text.len(), |end| end.end());
        at = found.start();
        text.replace_range(at..until, "");
    }
    delete_all(&mut text, &KEYWORD_TIME);
    for pattern in &settings.exclude {
        if let Some(regex) = emacs_buffer_regex(pattern) {
            delete_all(&mut text, &regex);
        }
    }
    text.truncate(text.trim_end_matches([' ', '\t', '\n']).len());
    let text = untabify(&text);
    // The indentation every line shares, the first one's even if blank.
    let mut lines_of: Vec<&str> = text.split('\n').collect();
    let mut common = indentation(lines_of[0]);
    for line in &lines_of {
        if !blank(line) {
            common = common.min(indentation(line));
        }
    }
    let unindented: Vec<String> = lines_of
        .drain(..)
        .map(|line| if blank(line) { line.to_string() } else { line[common..].to_string() })
        .collect();
    let mut shown: Vec<String> = if text.is_empty() {
        Vec::new()
    } else {
        unindented.into_iter().map(|line| format!("{}{line}", settings.leaders)).collect()
    };
    // Blank lines at the start, which the leaders may have made not blank.
    while shown.len() > 1 && blank(&shown[0]) {
        shown.remove(0);
    }
    shown.truncate(lines);
    if shown.iter().any(|line| !blank(line)) {
        shown
    } else {
        Vec::new()
    }
}
