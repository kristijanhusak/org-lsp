//! Sparse trees (`org-sparse-tree`, `C-c /`): what `org-occur` finds and
//! shows, for a regexp (`org-occur`), TODO entries
//! (`org-show-todo-tree`), deadlines that are due (`org-check-deadlines`)
//! and dates before, after or between days (`org-check-before-date` and
//! the others). Each finds its matches as Org does; showing them, with
//! everything else folded, is the editor's part.

use std::sync::LazyLock;

use org_syntax::TodoKeywords;
use regex::Regex;

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

/// `org-occur`: the matches of `regex` (case folded, as with
/// `org-occur-case-fold-search`), one after the other, those `keep`
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
pub fn todo_tree(text: &str, todo: &TodoKeywords, keywords: Option<&[String]>) -> Sparse {
    let pattern = match keywords {
        None => {
            let open: Vec<String> = todo.names().filter(|name| !todo.is_done(name)).map(regex::escape).collect();
            if open.is_empty() {
                return Sparse::default();
            }
            format!(r"(?im)^\*+  *\b(?:{})\b\s", open.join("|"))
        }
        Some(keywords) => {
            let wanted: Vec<String> = keywords.iter().map(|keyword| regex::escape(keyword)).collect();
            format!(r"(?im)^\*+  *(?:{})(?:[ \t]|$)", wanted.join("|"))
        }
    };
    match Regex::new(&pattern) {
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
pub fn deadlines(text: &str, todo: &TodoKeywords, today: Day, days: i64) -> Sparse {
    static DEADLINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\bDEADLINE: *<([^>]+)>").unwrap());
    occur(text, &DEADLINE, |found| {
        let whole = found.get(0).expect("a match");
        let close =
            found.get(1).and_then(|inside| time_of(inside.as_str())).is_some_and(|time| time.day - today <= days);
        close && !entry_is_done(text, whole.end(), todo)
    })
}

/// `org-at-planning-p` at `at`: the line right after a headline, a
/// planning line.
fn at_planning(text: &str, at: usize) -> bool {
    static PLANNING: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^[ \t]*(?:SCHEDULED|DEADLINE|CLOSED):").unwrap());
    let line = line_start(text, at);
    line > 0
        && PLANNING.is_match(&text[line..line_end(text, line)])
        && level_at(text, line_start(text, line - 1)).is_some()
        && next_line(text, line_start(text, line - 1)) == line
}

/// `org-check-before-date`, `org-check-after-date` and
/// `org-check-dates-range`: scheduled and deadline dates of planning
/// lines from `from` and before `to`.
pub fn dates(text: &str, from: Option<DateTime>, to: Option<DateTime>) -> Sparse {
    static PLANNED: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\b(?:DEADLINE:|SCHEDULED:) *<([^>]+)>").unwrap());
    occur(text, &PLANNED, |found| {
        let whole = found.get(0).expect("a match");
        let Some(time) = found.get(1).and_then(|inside| time_of(inside.as_str())) else { return false };
        at_planning(text, whole.end()) && from.is_none_or(|from| time >= from) && to.is_none_or(|to| time < to)
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
