//! Sparse trees (`org-sparse-tree`, `C-c /`): what `org-occur` finds and
//! shows, for a regexp (`org-occur`), TODO entries
//! (`org-show-todo-tree`), deadlines that are due (`org-check-deadlines`)
//! and dates before, after or between days (`org-check-before-date` and
//! the others). Each finds its matches as Org does; showing them, with
//! everything else folded, is the editor's part.

use std::sync::LazyLock;

use org_syntax::TodoKeywords;
use regex::{Regex, RegexBuilder};

use crate::date::{Date, DateTime, Day};
use crate::structure::{level_at, line_end, line_start, next_line};

/// What a sparse tree shows: the lines to reveal (the starts of the lines
/// where the matches end, as `org-show-context` reveals point), the text
/// to highlight (`org-highlight-new-match`), and how many matches.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sparse {
    pub lines: Vec<usize>,
    pub highlights: Vec<(usize, usize)>,
    pub count: usize,
}

impl Sparse {
    fn add(&mut self, text: &str, start: usize, end: usize) {
        self.count += 1;
        self.highlights.push((start, end));
        // The line point is on after the match.
        let line = line_start(text, end);
        if self.lines.last() != Some(&line) {
            self.lines.push(line);
        }
    }

    /// A headline found another way (a tags or property match), its stars
    /// highlighted, as `org-scan-tags` leaves them matched.
    pub fn add_heading(&mut self, text: &str, heading: usize) {
        let stars = text[heading..].bytes().take_while(|b| *b == b'*').count();
        self.count += 1;
        self.highlights.push((heading, heading + stars));
        if self.lines.last() != Some(&heading) {
            self.lines.push(heading);
        }
    }
}

/// `org-fold-hide-archived-subtrees` after a sparse tree, unless
/// `org-sparse-tree-open-archived-trees`: the subtree of each headline
/// tagged `ARCHIVE` itself stays folded, so a line shown in one shows its
/// archived headline instead.
pub fn hide_archived(text: &str, sparse: &Sparse) -> Sparse {
    let mut archived = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let end = line_end(text, at);
        if level_at(text, at).is_some() && crate::tags::local_tags(&text[at..end]).iter().any(|tag| tag == "ARCHIVE") {
            let subtree = crate::structure::subtree_end(text, at);
            archived.push((at, next_line(text, at), subtree));
            at = subtree;
        } else {
            at = next_line(text, at);
        }
    }
    let mut lines = Vec::new();
    for &line in &sparse.lines {
        let shown = archived
            .iter()
            .find(|(_, body, end)| (*body..*end).contains(&line))
            .map_or(line, |(heading, _, _)| *heading);
        if !lines.contains(&shown) {
            lines.push(shown);
        }
    }
    Sparse { lines, ..sparse.clone() }
}

/// `org-occur-case-fold-search`: whether `org-occur` ignores case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CaseFold {
    /// `t`
    #[default]
    Always,
    /// `nil`
    Never,
    /// `smart`: unless the regexp has an upper-case letter.
    Smart,
}

impl CaseFold {
    /// Whether a search for the Emacs regexp `regexp` ignores case.
    pub fn folds(self, regexp: &str) -> bool {
        match self {
            CaseFold::Always => true,
            CaseFold::Never => false,
            CaseFold::Smart => no_upper_case(regexp),
        }
    }
}

/// `isearch-no-upper-case-p` for a regexp: no upper-case letter but those
/// a backslash makes special (`\W`), and no `[:upper:]` or `[:lower:]`.
fn no_upper_case(regexp: &str) -> bool {
    let mut quoted = false;
    for c in regexp.chars() {
        if c == '\\' {
            quoted = !quoted;
            continue;
        }
        if !quoted && c.to_lowercase().next() != Some(c) {
            return false;
        }
        quoted = false;
    }
    !(regexp.contains("[:upper:]") || regexp.contains("[:lower:]"))
}

/// An Emacs regexp to search a buffer with, case folded as `case` says
/// for it.
pub fn occur_regex(regexp: &str, case: CaseFold) -> Option<Regex> {
    crate::agenda::matcher::emacs_buffer_regex_folded(regexp, case.folds(regexp))
}

/// `org-occur`: the matches of `regex`, one after the other, those `keep`
/// accepts.
pub fn occur(text: &str, regex: &Regex, keep: impl Fn(&regex::Captures) -> bool) -> Sparse {
    let mut sparse = Sparse::default();
    let mut at = 0;
    while at <= text.len() {
        let Some(found) = regex.captures_at(text, at) else { break };
        let whole = found.get(0).expect("a match");
        if keep(&found) {
            sparse.add(text, whole.start(), whole.end());
        }
        // An empty match moves on by one character.
        at = if whole.end() > whole.start() {
            whole.end()
        } else {
            match text[whole.end()..].chars().next() {
                Some(c) => whole.end() + c.len_utf8(),
                None => break,
            }
        };
    }
    sparse
}

/// `org-show-todo-tree`: the headlines with a TODO keyword not done, or
/// with one of `keywords` (`T`, `KWD1|KWD2`).
pub fn todo_tree(text: &str, todo: &TodoKeywords, keywords: Option<&[String]>, case: CaseFold) -> Sparse {
    let (pattern, words) = match keywords {
        None => {
            let open: Vec<&str> = todo.names().filter(|name| !todo.is_done(name)).collect();
            if open.is_empty() {
                return Sparse::default();
            }
            let escaped: Vec<String> = open.iter().map(|name| regex::escape(name)).collect();
            (format!(r"(?m)^\*+  *\b(?:{})\b\s", escaped.join("|")), open.join(""))
        }
        Some(keywords) => {
            let wanted: Vec<String> = keywords.iter().map(|keyword| regex::escape(keyword)).collect();
            (format!(r"(?m)^\*+  *(?:{})(?:[ \t]|$)", wanted.join("|")), keywords.join(""))
        }
    };
    match RegexBuilder::new(&pattern).case_insensitive(case.folds(&words)).build() {
        Ok(regex) => occur(text, &regex, |_| true),
        Err(_) => Sparse::default(),
    }
}

/// The headline above `at`, if any.
fn heading_above(text: &str, at: usize) -> Option<usize> {
    let mut start = line_start(text, at);
    loop {
        if level_at(text, start).is_some() {
            return Some(start);
        }
        if start == 0 {
            return None;
        }
        start = line_start(text, start - 1);
    }
}

/// `org-entry-is-done-p` at `at`.
fn entry_is_done(text: &str, at: usize, todo: &TodoKeywords) -> bool {
    let Some(heading) = heading_above(text, at) else { return false };
    let line = &text[heading..line_end(text, heading)];
    let word = line.trim_start_matches('*').split_whitespace().next().unwrap_or("");
    todo.is_done(word)
}

/// `org-time-string-to-time` of a timestamp's inside: its date, and its
/// time or midnight.
fn time_of(text: &str) -> Option<DateTime> {
    static TIME: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(\d{4})-(\d{2})-(\d{2})(?: +[^\]+0-9>\r\n -]+)?(?: +(\d{1,2}):(\d{2}))?").unwrap()
    });
    let found = TIME.captures(text)?;
    let number = |index: usize| found.get(index).map_or(0, |part| part.as_str().parse::<i64>().unwrap_or(0));
    Some(DateTime::new(Date::new(number(1), number(2), number(3)), number(4), number(5)))
}

/// `org-check-deadlines`: deadlines no more than `days` days ahead, past
/// ones included, of entries not done.
pub fn deadlines(text: &str, todo: &TodoKeywords, today: Day, days: i64, case: CaseFold) -> Sparse {
    let Ok(deadline) = RegexBuilder::new(r"\bDEADLINE: *<([^>]+)>").case_insensitive(case.folds("DEADLINE")).build()
    else {
        return Sparse::default();
    };
    occur(text, &deadline, |found| {
        let whole = found.get(0).expect("a match");
        let close =
            found.get(1).and_then(|inside| time_of(inside.as_str())).is_some_and(|time| time.day - today <= days);
        close && !entry_is_done(text, whole.end(), todo)
    })
}

/// `org-at-planning-p` at `at`: the line right after a headline, a
/// planning line.
fn at_planning(text: &str, at: usize, folds: bool) -> bool {
    static PLANNING: LazyLock<(Regex, Regex)> = LazyLock::new(|| {
        let pattern = r"^[ \t]*(?:SCHEDULED|DEADLINE|CLOSED):";
        (RegexBuilder::new(pattern).case_insensitive(true).build().unwrap(), Regex::new(pattern).unwrap())
    });
    let planning = if folds { &PLANNING.0 } else { &PLANNING.1 };
    let line = line_start(text, at);
    line > 0
        && planning.is_match(&text[line..line_end(text, line)])
        && level_at(text, line_start(text, line - 1)).is_some()
        && next_line(text, line_start(text, line - 1)) == line
}

/// `org-sparse-tree-default-date-type`: the timestamps the date sparse
/// trees look at (`org-re-timestamp`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DateType {
    /// `nil`: scheduled and deadline dates.
    #[default]
    Planned,
    All,
    Scheduled,
    Deadline,
    Active,
    Inactive,
    Closed,
}

impl DateType {
    /// The next type `c` turns to in `org-sparse-tree`.
    pub fn next(self) -> DateType {
        match self {
            DateType::Planned => DateType::All,
            DateType::All => DateType::Scheduled,
            DateType::Scheduled => DateType::Deadline,
            DateType::Deadline => DateType::Active,
            DateType::Active => DateType::Inactive,
            DateType::Inactive => DateType::Closed,
            DateType::Closed => DateType::Planned,
        }
    }

    /// How `org-sparse-tree` names it.
    pub fn describe(self) -> &'static str {
        match self {
            DateType::Planned => "scheduled/deadline",
            DateType::All => "all timestamps",
            DateType::Scheduled => "only scheduled",
            DateType::Deadline => "only deadline",
            DateType::Active => "only active timestamps",
            DateType::Inactive => "only inactive timestamps",
            DateType::Closed => "with a closed timestamp",
        }
    }

    /// The Lisp symbol, as a setting writes it (`nil` for the default).
    pub fn name(self) -> &'static str {
        match self {
            DateType::Planned => "nil",
            DateType::All => "all",
            DateType::Scheduled => "scheduled",
            DateType::Deadline => "deadline",
            DateType::Active => "active",
            DateType::Inactive => "inactive",
            DateType::Closed => "closed",
        }
    }

    /// The type a Lisp symbol names.
    pub fn from_name(name: &str) -> Option<DateType> {
        Some(match name {
            "all" => DateType::All,
            "scheduled" => DateType::Scheduled,
            "deadline" => DateType::Deadline,
            "active" => DateType::Active,
            "inactive" => DateType::Inactive,
            "closed" => DateType::Closed,
            _ => return None,
        })
    }

    /// `org-re-timestamp`, and the word in it that decides `smart` case
    /// folding.
    fn pattern(self) -> (&'static str, &'static str) {
        const INSIDE: &str = r"\d{4}-\d{2}-\d{2}(?: [^\n]*?)?";
        static ALL: LazyLock<String> = LazyLock::new(|| format!(r"[\[<]({INSIDE})[\]>]"));
        static ACTIVE: LazyLock<String> = LazyLock::new(|| format!(r"<({INSIDE})>"));
        static INACTIVE: LazyLock<String> = LazyLock::new(|| format!(r"\[({INSIDE})\]"));
        match self {
            DateType::Planned => (r"\b(?:DEADLINE:|SCHEDULED:) *<([^>]+)>", "DEADLINESCHEDULED"),
            DateType::All => (ALL.as_str(), ""),
            DateType::Active => (ACTIVE.as_str(), ""),
            DateType::Inactive => (INACTIVE.as_str(), ""),
            DateType::Scheduled => (r"\bSCHEDULED: *<([^>]+)>", "SCHEDULED"),
            DateType::Deadline => (r"\bDEADLINE: *<([^>]+)>", "DEADLINE"),
            DateType::Closed => (r"\bCLOSED: *\[([^\]]+)\]", "CLOSED"),
        }
    }
}

/// Whether the character before `end` is in a timestamp object, as
/// `org-element-context` finds it there: not one of a planning line.
fn in_timestamp(parse: &org_syntax::Parse, end: usize) -> bool {
    let Some(at) = end.checked_sub(1).and_then(|at| u32::try_from(at).ok()) else { return false };
    let Some(token) = parse.syntax().token_at_offset(org_syntax::TextSize::from(at)).right_biased() else {
        return false;
    };
    let mut timestamp = false;
    for node in token.parent_ancestors() {
        match node.kind() {
            // On a planning line `org-element-context` stops at the line.
            org_syntax::SyntaxKind::PLANNING => return false,
            org_syntax::SyntaxKind::TIMESTAMP => timestamp = true,
            _ => {}
        }
    }
    timestamp
}

/// `org-check-before-date`, `org-check-after-date` and
/// `org-check-dates-range`: the dates of `kind` from `from` and before
/// `to`. Planned dates count on planning lines only, the others where
/// they are timestamps.
pub fn dates(
    text: &str,
    parse: &org_syntax::Parse,
    from: Option<DateTime>,
    to: Option<DateTime>,
    kind: DateType,
    case: CaseFold,
) -> Sparse {
    let (pattern, words) = kind.pattern();
    let folds = case.folds(words);
    let Ok(regex) = RegexBuilder::new(pattern).case_insensitive(folds).build() else { return Sparse::default() };
    let timestamps = matches!(kind, DateType::All | DateType::Active | DateType::Inactive);
    occur(text, &regex, |found| {
        let whole = found.get(0).expect("a match");
        let Some(time) = found.get(1).and_then(|inside| time_of(inside.as_str())) else { return false };
        let counted = if timestamps { in_timestamp(parse, whole.end()) } else { at_planning(text, whole.end(), folds) };
        counted && from.is_none_or(|from| time >= from) && to.is_none_or(|to| time < to)
    })
}

/// `org-match-sparse-tree`: the entries of `outline` (of `text`) that
/// `matcher` selects, as `org-scan-tags` finds them for a tags view. With
/// `sublevels` false (`org-tags-match-list-sublevels`) the subtree of an
/// entry found is passed over.
pub fn tags_match(
    text: &str,
    outline: &crate::outline::Outline,
    matcher: &crate::agenda::Matcher,
    clock: crate::agenda::matcher::Clock,
    sublevels: bool,
) -> Sparse {
    let mut sparse = Sparse::default();
    let mut passing: Option<usize> = None;
    for (index, entry) in outline.entries.iter().enumerate() {
        if passing.is_some_and(|above| entry.level > above) {
            continue;
        }
        passing = None;
        if matcher.matches(outline, index, false, clock) {
            sparse.add_heading(text, entry.offset);
            if !sublevels {
                passing = Some(entry.level);
            }
        }
    }
    sparse
}

#[cfg(test)]
mod tests {
    use super::*;

    /// As Emacs leaves `org-occur "foo"`: the matches in archived subtrees
    /// show their archived headline only.
    #[test]
    fn keeps_archived_subtrees_folded() {
        let text = "* Top\n** Old :ARCHIVE:\nfoo here\n*** Deep\nfoo too\n** New\nfoo new\n* Other :ARCHIVE:\nfoo\n";
        let found = occur(text, &Regex::new("(?i)foo").unwrap(), |_| true);
        let line = |at: usize| text[..at].matches('\n').count();
        let shown: Vec<usize> = hide_archived(text, &found).lines.into_iter().map(line).collect();
        assert_eq!(shown, [1, 6, 7]);
        assert_eq!(found.lines.into_iter().map(line).collect::<Vec<_>>(), [2, 4, 6, 8]);
    }
}
