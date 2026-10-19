//! Inline tasks (`org-inlinetask`): a headline of at least
//! `org-inlinetask-min-level` stars is a task inside the text of its
//! entry, not a part of the outline; a line of as many stars and `END`
//! closes it. Org leaves them out where it limits its outline to fewer
//! levels (`org-with-limited-levels`): the outline, folding, motion
//! between headings, the entries of subtrees. Here are what
//! `org-inlinetask.el` defines: the tests on a line, where a task begins
//! and ends, `org-inlinetask-insert-task` (`C-c C-x t`), and
//! `org-inlinetask-promote` and `org-inlinetask-demote`, which `M-<left>`
//! and `M-<right>` run on a task. Checked against Emacs in
//! `tests/inlinetask`.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::indent;
use crate::newline::Outcome;
use crate::structure::{line, line_end, line_start, next_line, odd_levels_in, valid_level};
use crate::Refusal;

/// `org-inlinetask-min-level` as the options set it, for the server's
/// life; 0 when inline tasks are off, as in Emacs before `org-inlinetask`
/// is loaded.
static MIN_LEVEL: AtomicUsize = AtomicUsize::new(0);

/// `org-inlinetask-show-first-star` as the options set it.
static SHOW_FIRST_STAR: AtomicBool = AtomicBool::new(false);

/// Set `org-inlinetask-show-first-star` for the process.
pub fn set_show_first_star(show: bool) {
    SHOW_FIRST_STAR.store(show, Ordering::Relaxed);
}

/// `org-inlinetask-show-first-star`: no star of an inline task is hidden.
pub fn show_first_star() -> bool {
    SHOW_FIRST_STAR.load(Ordering::Relaxed)
}

/// Whether `text` has an inline task whose stars are hidden.
pub fn hides_stars(text: &str) -> bool {
    !show_first_star() && text.split('\n').any(|line| is_task_line(text, line) || is_end_line(text, line))
}

/// Set `org-inlinetask-min-level` for the process: `None` turns inline
/// tasks off.
pub fn set_min_level(level: Option<usize>) {
    MIN_LEVEL.store(level.unwrap_or(0), Ordering::Relaxed);
}

/// `org-inlinetask-min-level`, if inline tasks are on.
pub fn min_level() -> Option<usize> {
    Some(MIN_LEVEL.load(Ordering::Relaxed)).filter(|level| *level > 0)
}

/// The number of stars of the shortest inline task: the minimum level,
/// counted in odd levels with `org-odd-levels-only`
/// (`org-inlinetask-outline-regexp`).
pub fn min_stars(odd: bool) -> Option<usize> {
    min_level().map(|level| if odd { 2 * level - 1 } else { level })
}

/// [`min_stars`] in `text`, with its `#+STARTUP:` words about odd levels.
pub fn min_stars_in(text: &str) -> Option<usize> {
    min_stars(odd_levels_in(text))
}

/// Whether a headline of `stars` stars is an inline task in `text`.
pub fn is_task_level(text: &str, stars: usize) -> bool {
    min_stars_in(text).is_some_and(|min| stars >= min)
}

/// The most stars of a headline that is part of the outline
/// (`org-get-limited-outline-regexp`), if inline tasks are on: those of
/// one level less than the minimum.
pub fn outline_stars(odd: bool) -> Option<usize> {
    min_level().map(|level| {
        let limit = level.saturating_sub(1).max(1);
        if odd {
            2 * limit - 1
        } else {
            limit
        }
    })
}

/// [`outline_stars`] in `text`, with its `#+STARTUP:` words about odd
/// levels.
pub fn outline_stars_in(text: &str) -> Option<usize> {
    outline_stars(odd_levels_in(text))
}

/// The stars of `line` if it matches `org-inlinetask-outline-regexp`,
/// `^\(\*\{N,\}\)[ \t]+`.
fn task_stars(line: &str, min: usize) -> Option<usize> {
    let stars = line.bytes().take_while(|b| *b == b'*').count();
    (stars >= min && matches!(line.as_bytes().get(stars), Some(b' ' | b'\t'))).then_some(stars)
}

/// `org-inlinetask-end-p` on `line`: the stars of a task, `END` and
/// blanks, in any case.
fn end_line(line: &str, min: usize) -> bool {
    task_stars(line, min).is_some_and(|stars| line[stars..].trim_matches([' ', '\t']).eq_ignore_ascii_case("END"))
}

/// Whether `line` is an inline task's `END` line in `text`.
pub fn is_end_line(text: &str, line: &str) -> bool {
    min_stars_in(text).is_some_and(|min| end_line(line, min))
}

/// Whether `line` is the first line of an inline task in `text`
/// (`org-inlinetask-at-task-p`).
pub fn is_task_line(text: &str, line: &str) -> bool {
    min_stars_in(text).is_some_and(|min| task_stars(line, min).is_some() && !end_line(line, min))
}

/// `^\*+[ \t]+`: the line could be a headline, as `org-inlinetask-in-task-p`
/// looks for one.
fn heading_like(line: &str) -> bool {
    let stars = line.bytes().take_while(|b| *b == b'*').count();
    stars > 0 && matches!(line.as_bytes().get(stars), Some(b' ' | b'\t'))
}

/// The start of the first line from `from` on (the line at `from` too) for
/// which `test` holds, as `re-search-forward` of a regexp anchored at the
/// start of a line finds it.
fn search_forward(text: &str, from: usize, test: impl Fn(&str) -> bool) -> Option<usize> {
    let mut at = if from == line_start(text, from) { from } else { next_line(text, from) };
    while at < text.len() {
        if test(line(text, at)) {
            return Some(at);
        }
        at = next_line(text, at);
    }
    None
}

/// The start of the last line that starts before `from` for which `test`
/// holds, as `re-search-backward` finds it.
fn search_backward(text: &str, from: usize, test: impl Fn(&str) -> bool) -> Option<usize> {
    let mut at = line_start(text, from.min(text.len()));
    loop {
        if at < from && test(line(text, at)) {
            return Some(at);
        }
        if at == 0 {
            return None;
        }
        at = line_start(text, at - 1);
    }
}

/// `forward-line`: the start of the next line, or the end of the text.
fn forward_line(text: &str, at: usize) -> usize {
    next_line(text, at)
}

/// The tests and motions of `org-inlinetask.el`, with the shortest task's
/// stars.
struct Tasks<'a> {
    text: &'a str,
    min: usize,
}

impl Tasks<'_> {
    fn line(&self, at: usize) -> &str {
        line(self.text, line_start(self.text, at))
    }

    fn end_p(&self, at: usize) -> bool {
        end_line(self.line(at), self.min)
    }

    fn looking_at_task(&self, at: usize) -> bool {
        task_stars(&self.text[at..line_end(self.text, at)], self.min).is_some()
    }

    /// `org-inlinetask-in-task-p`
    fn in_task_p(&self, at: usize) -> bool {
        let start = line_start(self.text, at);
        if task_stars(line(self.text, start), self.min).is_some() {
            return true;
        }
        search_forward(self.text, start, heading_like).is_some_and(|found| self.end_p(found))
    }

    /// `org-inlinetask-goto-beginning`: the start of the task line, or
    /// where the search leaves the point when there is none.
    fn goto_beginning(&self, at: usize) -> usize {
        let eol = line_end(self.text, at);
        let is_task = |line: &str| task_stars(line, self.min).is_some();
        let Some(found) = search_backward(self.text, eol, is_task) else { return eol };
        if self.end_p(found) {
            return search_backward(self.text, found, is_task).unwrap_or(found);
        }
        found
    }

    /// `org-inlinetask-goto-end`
    fn goto_end(&self, at: usize) -> usize {
        let start = line_start(self.text, at);
        let is_task = |line: &str| task_stars(line, self.min).is_some();
        if self.end_p(start) {
            return forward_line(self.text, start);
        }
        if self.looking_at_task(start) {
            let next = forward_line(self.text, start);
            if self.end_p(next) {
                return forward_line(self.text, next);
            }
            if self.looking_at_task(next) {
                return next;
            }
            if self.in_task_p(next) {
                // `re-search-forward` from the point, which may be at the
                // end of the last line.
                return match search_forward(self.text, next, is_task) {
                    Some(found) => forward_line(self.text, found),
                    None => forward_line(self.text, next),
                };
            }
            return next;
        }
        match search_forward(self.text, start, is_task) {
            Some(found) => forward_line(self.text, found),
            None => forward_line(self.text, start),
        }
    }

    /// `org-inlinetask-get-task-level`: the stars of the task line at or
    /// before the line at `at`.
    fn task_level(&self, at: usize) -> usize {
        let eol = line_end(self.text, at);
        search_backward(self.text, eol, |line| task_stars(line, self.min).is_some())
            .and_then(|found| task_stars(line(self.text, found), self.min))
            .unwrap_or(0)
    }
}

/// Where the inline task at the line at `at` hides its text when it is
/// folded (`org-inlinetask-toggle-visibility`): from the end of its first
/// line to the end of its `END` line, as byte offsets. None where it hides
/// nothing, or the line is no task.
pub fn fold_range(text: &str, at: usize) -> Option<(usize, usize)> {
    let min = min_stars_in(text)?;
    let tasks = Tasks { text, min };
    let start = line_start(text, at);
    if !is_task_line(text, line(text, start)) {
        return None;
    }
    let end = tasks.goto_end(start);
    let end = if end > 0 && text[..end].ends_with('\n') { end - 1 } else { end };
    let first = line_end(text, tasks.goto_beginning(start));
    (end > first).then_some((first, end))
}

/// `org-inlinetask-insert-task` (`C-c C-x t`) at `at`, around `region`
/// when there is one: a task of `org-inlinetask-min-level` stars with its
/// `END` line, its TODO keyword `state` (`org-inlinetask-default-state`,
/// none when the prefix argument says so).
pub fn insert_task(
    text: &str,
    at: usize,
    region: Option<(usize, usize)>,
    state: Option<&str>,
) -> Result<Outcome, Refusal> {
    let min = min_stars_in(text).ok_or_else(|| Refusal::NotHere("Inline tasks are off".to_string()))?;
    let tasks = Tasks { text, min };
    let start = line_start(text, at);
    if tasks.in_task_p(at) && !(is_task_line(text, line(text, start)) && at == start) {
        return Err(Refusal::Failed("Cannot nest inline tasks".to_string()));
    }
    let mut text = text.to_string();
    let mut point = at;
    let mut region = region;
    // `(or (bolp) (newline))`: the region's end moves with the text.
    if point != line_start(&text, point) {
        text.insert(point, '\n');
        point += 1;
        region = region.map(|(begin, end)| (shift(begin, at), shift(end, at)));
    }
    let stars = "*".repeat(min);
    let (rbeg, rend) = region.unwrap_or((point, point));
    text.insert_str(rend, &format!("\n{stars} END\n"));
    point = rbeg;
    if point != line_start(&text, point) {
        text.insert(point, '\n');
        point += 1;
    }
    let state = state.map(|state| format!("{state} ")).unwrap_or_default();
    let head = format!("{stars} {state}{}", if rend == rbeg { "" } else { "\n" });
    text.insert_str(point, &head);
    point += head.len();
    if rend != rbeg {
        // `(end-of-line 0)`: the end of the line before.
        point = line_end(&text, line_start(&text, point).saturating_sub(1));
    }
    Ok(Outcome { text, cursor: point })
}

/// Where a position after `at` goes when a newline is inserted at `at`.
fn shift(position: usize, at: usize) -> usize {
    if position >= at {
        position + 1
    } else {
        position
    }
}

/// `org-inlinetask-promote` (`change` -1) and `org-inlinetask-demote` (1)
/// on the inline task at `at`: its stars and those of its `END` line, and
/// with `org-adapt-indentation` `t` its text (`org-fixup-indentation`).
pub fn change_level(
    text: &str,
    at: usize,
    change: isize,
    odd: bool,
    indent: &indent::Settings,
) -> Result<Outcome, Refusal> {
    let min = min_stars(odd).ok_or_else(|| Refusal::NotHere("Inline tasks are off".to_string()))?;
    let tasks = Tasks { text, min };
    if !tasks.in_task_p(at) {
        return Err(Refusal::Failed("Not in an inline task".to_string()));
    }
    let level = tasks.task_level(at);
    let next = valid_level(level, change, odd);
    let diff = next as isize - level as isize;
    if change < 0 && min_level().is_some_and(|min| next < min) {
        return Err(Refusal::Failed("Cannot promote an inline task at minimum level".to_string()));
    }
    let stars = "*".repeat(next);
    let mut buffer = Buffer { text: text.to_string(), point: at };
    let beg = tasks.goto_beginning(at);
    if let Some(old) = task_stars(line(text, line_start(text, beg)), min) {
        buffer.replace(beg, beg + old, &stars);
    }
    let tasks = Tasks { text: &buffer.text, min };
    let end = tasks.goto_end(beg);
    // `(forward-line 0)` at the end of a last `END` line, else
    // `(forward-line -1)`.
    let end_start = line_start(&buffer.text, end);
    let ends_in_end = line(&buffer.text, end_start).trim_end_matches([' ', '\t']).to_ascii_uppercase().ends_with("END");
    let last = if end == buffer.text.len() && end != end_start && ends_in_end {
        end_start
    } else if end_start == 0 {
        0
    } else {
        line_start(&buffer.text, end_start - 1)
    };
    if last != beg {
        if let Some(old) = task_stars(line(&buffer.text, last), min) {
            buffer.replace(last, last + old, &stars);
        }
        if indent.adapt == indent::Adapt::All {
            let tasks = Tasks { text: &buffer.text, min };
            let end = tasks.goto_end(beg);
            let entry = buffer.text[beg..end].to_string();
            let fixed = indent::fixup_entry(&entry, diff, indent);
            if fixed != entry {
                // Only what changed, so that the point after it stays
                // where it is, as the lines are indented one at a time.
                let old: Vec<&str> = entry.split_inclusive('\n').collect();
                let new: Vec<&str> = fixed.split_inclusive('\n').collect();
                let same = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
                let same_end =
                    old[same..].iter().rev().zip(new[same..].iter().rev()).take_while(|(a, b)| a == b).count();
                let start: usize = old[..same].iter().map(|line| line.len()).sum();
                let old_end: usize = old[..old.len() - same_end].iter().map(|line| line.len()).sum();
                let new_end: usize = new[..new.len() - same_end].iter().map(|line| line.len()).sum();
                buffer.replace(beg + start, beg + old_end, &fixed[start..new_end]);
            }
        }
    }
    Ok(Outcome { text: buffer.text, cursor: buffer.point })
}

/// Text and the point, which moves as a marker of `save-excursion` does.
struct Buffer {
    text: String,
    point: usize,
}

impl Buffer {
    fn replace(&mut self, start: usize, end: usize, new: &str) {
        if self.point >= end {
            self.point = self.point + new.len() - (end - start);
        } else if self.point > start {
            self.point = start;
        }
        self.text.replace_range(start..end, new);
    }
}

/// `org-inlinetask-in-task-p` at `at`, with `org-odd-levels-only` `odd`.
pub fn in_task(text: &str, at: usize, odd: bool) -> bool {
    min_stars(odd).is_some_and(|min| Tasks { text, min }.in_task_p(at.min(text.len())))
}
