//! A buffer as org-journal works on one: its text, the point, the
//! accessible portion (narrowing) and which characters the headlines'
//! folds hide, with the Emacs and Org primitives org-journal calls, so
//! that its functions can be followed one by one. Positions are byte
//! offsets; searches fold case, as `case-fold-search` does.

use std::ops::Range;
use std::sync::LazyLock;

use regex::{Regex, RegexBuilder};

/// A regex of Emacs's searches: `^` and `$` at line ends, case folded.
pub(super) fn search_regex(pattern: &str) -> Regex {
    RegexBuilder::new(pattern).multi_line(true).case_insensitive(true).build().expect("valid pattern")
}

/// `org-outline-regexp-bol`: a headline.
static HEADING: LazyLock<Regex> = LazyLock::new(|| search_regex(r"^\*+ "));

/// The level of a headline at the start of `line`.
pub(super) fn level_of(line: &str) -> Option<usize> {
    let stars = line.bytes().take_while(|b| *b == b'*').count();
    (stars > 0 && line.as_bytes().get(stars) == Some(&b' ')).then_some(stars)
}

/// What a search matched: each group's range.
#[derive(Debug, Clone)]
pub(super) struct Match(pub Vec<Option<Range<usize>>>);

impl Match {
    pub fn start(&self) -> usize {
        self.0[0].as_ref().expect("whole match").start
    }

    pub fn end(&self) -> usize {
        self.0[0].as_ref().expect("whole match").end
    }

    pub fn group(&self, n: usize) -> Option<Range<usize>> {
        self.0.get(n).cloned().flatten()
    }
}

#[derive(Debug, Clone)]
pub(super) struct Buffer {
    /// The file, relative to the journal directory.
    pub file: String,
    pub text: String,
    pub point: usize,
    /// The accessible portion: `point-min` and `point-max`.
    pub begv: usize,
    pub zv: usize,
    /// Whether each byte is in a headline's fold.
    pub hidden: Vec<bool>,
    /// Markers: positions that follow edits, as `save-excursion` keeps
    /// the point.
    pub markers: Vec<usize>,
    /// Killed with `kill-buffer`.
    pub killed: bool,
    /// Changed since it was visited or saved.
    pub modified: bool,
}

impl Buffer {
    pub fn new(file: &str, text: &str) -> Buffer {
        Buffer {
            file: file.to_string(),
            text: text.to_string(),
            point: 0,
            begv: 0,
            zv: text.len(),
            hidden: vec![false; text.len()],
            markers: Vec::new(),
            killed: false,
            modified: false,
        }
    }

    /// The buffer's name: the file's, without its directory.
    pub fn name(&self) -> String {
        self.file.rsplit('/').next().unwrap_or(&self.file).to_string()
    }

    // Motion.

    pub fn goto(&mut self, at: usize) {
        self.point = at.clamp(self.begv, self.zv);
    }

    pub fn bobp(&self) -> bool {
        self.point == self.begv
    }

    pub fn eobp(&self) -> bool {
        self.point == self.zv
    }

    pub fn bolp(&self) -> bool {
        self.point == self.begv || self.text.as_bytes()[self.point - 1] == b'\n'
    }

    pub fn eolp(&self) -> bool {
        self.point == self.zv || self.text.as_bytes()[self.point] == b'\n'
    }

    /// `line-beginning-position` of `at`.
    pub fn line_start(&self, at: usize) -> usize {
        self.text[self.begv..at].rfind('\n').map_or(self.begv, |newline| self.begv + newline + 1)
    }

    /// `line-end-position` of `at`.
    pub fn line_end(&self, at: usize) -> usize {
        self.text[at..self.zv].find('\n').map_or(self.zv, |newline| at + newline)
    }

    /// `(line-end-position 0)`: the end of the line before, or the start
    /// of the accessible portion on its first line.
    pub fn previous_line_end(&self, at: usize) -> usize {
        let start = self.line_start(at);
        if start == self.begv {
            self.begv
        } else {
            start - 1
        }
    }

    pub fn beginning_of_line(&mut self) {
        self.point = self.line_start(self.point);
    }

    pub fn end_of_line(&mut self) {
        self.point = self.line_end(self.point);
    }

    /// `(forward-line n)`
    pub fn forward_line(&mut self, n: i64) {
        if n > 0 {
            for _ in 0..n {
                match self.text[self.point..self.zv].find('\n') {
                    Some(newline) => self.point += newline + 1,
                    None => {
                        self.point = self.zv;
                        return;
                    }
                }
            }
        } else {
            self.beginning_of_line();
            for _ in 0..-n {
                if self.point == self.begv {
                    return;
                }
                self.point = self.line_start(self.point - 1);
            }
        }
    }

    pub fn char_before(&self) -> Option<char> {
        if self.point > self.begv {
            self.text[..self.point].chars().next_back()
        } else {
            None
        }
    }

    /// `(forward-char -1)`
    pub fn backward_char(&mut self) {
        if let Some(c) = self.char_before() {
            self.point -= c.len_utf8();
        }
    }

    /// `(skip-chars-backward chars)`
    pub fn skip_backward(&mut self, chars: &[char]) {
        while let Some(c) = self.char_before().filter(|c| chars.contains(c)) {
            self.point -= c.len_utf8();
        }
    }

    /// `(looking-back "[^\t ]" (line-beginning-position))`
    pub fn after_non_blank(&self) -> bool {
        self.point > self.line_start(self.point) && !matches!(self.char_before(), Some(' ' | '\t'))
    }

    // Searches.

    /// `re-search-forward` up to `bound` (the end of the accessible
    /// portion when `None`): the point goes after the match; a failed
    /// search leaves it, or moves it to the bound with `movep`.
    pub fn re_search_forward(&mut self, regex: &Regex, bound: Option<usize>, movep: bool) -> Option<Match> {
        let bound = bound.unwrap_or(self.zv).min(self.zv);
        let found = (self.point <= bound)
            .then(|| regex.captures_at(&self.text[..bound], self.point))
            .flatten()
            .map(|captures| Match(captures.iter().map(|group| group.map(|group| group.range())).collect()));
        match &found {
            Some(found) => self.point = found.end(),
            None if movep => self.point = bound,
            None => {}
        }
        found
    }

    /// `re-search-backward`: the last match that starts before the point
    /// and after the start of the accessible portion; the point goes to
    /// its start. Matches are tried from each position backwards, as
    /// Emacs does, so the match found starts as late as possible.
    pub fn re_search_backward(&mut self, regex: &Regex, movep: bool) -> Option<Match> {
        let mut at = self.point;
        loop {
            if at < self.point {
                if let Some(captures) = regex.captures_at(&self.text[..self.zv], at) {
                    if captures.get(0).is_some_and(|whole| whole.start() == at && whole.end() <= self.point) {
                        self.point = at;
                        return Some(Match(captures.iter().map(|group| group.map(|group| group.range())).collect()));
                    }
                }
            }
            if at == self.begv {
                break;
            }
            at = self.text[..at].char_indices().next_back().map_or(0, |(previous, _)| previous);
        }
        if movep {
            self.point = self.begv;
        }
        None
    }

    /// `search-forward` of `literal`, case folded.
    pub fn search_forward(&mut self, literal: &str, bound: Option<usize>) -> Option<Match> {
        self.re_search_forward(&search_regex(&regex::escape(literal)), bound, false)
    }

    /// `looking-at`
    pub fn looking_at(&self, regex: &Regex) -> Option<Match> {
        regex
            .captures_at(&self.text[..self.zv], self.point)
            .filter(|captures| captures.get(0).is_some_and(|whole| whole.start() == self.point))
            .map(|captures| Match(captures.iter().map(|group| group.map(|group| group.range())).collect()))
    }

    // Editing.

    /// `insert`, at the point, which goes after the text.
    pub fn insert(&mut self, text: &str) {
        let at = self.point;
        self.modified |= !text.is_empty();
        self.text.insert_str(at, text);
        self.hidden.splice(at..at, std::iter::repeat_n(false, text.len()));
        self.zv += text.len();
        self.point += text.len();
        for marker in &mut self.markers {
            if *marker > at {
                *marker += text.len();
            }
        }
    }

    /// `delete-region`
    pub fn delete(&mut self, start: usize, end: usize) {
        let (start, end) = (start.min(end), start.max(end));
        self.modified |= start < end;
        self.text.replace_range(start..end, "");
        self.hidden.drain(start..end);
        let shift = |at: usize| if at >= end { at - (end - start) } else { at.min(start) };
        self.point = shift(self.point);
        self.zv = shift(self.zv);
        self.begv = shift(self.begv);
        for marker in &mut self.markers {
            *marker = shift(*marker);
        }
    }

    /// `save-excursion` within the buffer: a marker at the point.
    pub fn save_point(&mut self) {
        self.markers.push(self.point);
    }

    /// The end of `save-excursion`: the point back at its marker.
    pub fn restore_point(&mut self) {
        if let Some(at) = self.markers.pop() {
            self.point = at.clamp(self.begv, self.zv);
        }
    }

    /// The whole text replaced by `text`, as an Org command that edits
    /// elsewhere leaves it; the point stays where it was, or as far.
    pub fn replace_text(&mut self, text: String) {
        self.modified |= text != self.text;
        let grown = text.len() as i64 - self.text.len() as i64;
        let (mut prefix, suffix) = crate::text::common_ends(&self.text, &text);
        // Text only inserted goes in as early as it can, as Org inserts a
        // property drawer at the end of the headline's line: the point or a
        // marker where the line ended goes after it.
        if grown > 0 && prefix + suffix >= self.text.len() {
            let (old, new) = (self.text.as_bytes(), text.as_bytes());
            while prefix > 0 && old[prefix - 1] == new[prefix - 1 + grown as usize] {
                prefix -= 1;
            }
            while !self.text.is_char_boundary(prefix) {
                prefix += 1;
            }
        }
        let shift = |at: usize| if at > prefix { (at as i64 + grown).max(prefix as i64) as usize } else { at };
        self.point = shift(self.point);
        for marker in &mut self.markers {
            *marker = shift(*marker);
        }
        self.zv = (self.zv as i64 + grown) as usize;
        let mut hidden = vec![false; text.len()];
        let keep = prefix.min(self.hidden.len());
        hidden[..keep].copy_from_slice(&self.hidden[..keep]);
        self.hidden = hidden;
        self.text = text;
    }

    pub fn widen(&mut self) {
        self.begv = 0;
        self.zv = self.text.len();
    }

    pub fn narrow(&mut self, start: usize, end: usize) {
        self.begv = start;
        self.zv = end;
        self.point = self.point.clamp(start, end);
    }

    // Headlines.

    /// `org-outline-level` at the point: the stars of the headline the
    /// line starts with, or 0.
    pub fn outline_level(&self) -> usize {
        let start = self.line_start(self.point);
        level_of(&self.text[start..self.zv]).unwrap_or(0)
    }

    /// `org-at-heading-p`; `visible` only when the headline is not folded.
    pub fn at_heading(&self, visible: bool) -> bool {
        let start = self.line_start(self.point);
        level_of(&self.text[start..self.zv]).is_some() && !(visible && self.invisible(start))
    }

    pub fn invisible(&self, at: usize) -> bool {
        self.hidden.get(at).copied().unwrap_or(false)
    }

    /// The headline containing `at`, as `org-element-at-point`'s lineage
    /// finds it, in the whole text: its start.
    pub fn containing_heading(&self, at: usize) -> Option<usize> {
        let mut start = self.text[..at].rfind('\n').map_or(0, |newline| newline + 1);
        loop {
            if level_of(&self.text[start..]).is_some() {
                return Some(start);
            }
            if start == 0 {
                return None;
            }
            start = self.text[..start - 1].rfind('\n').map_or(0, |newline| newline + 1);
        }
    }

    /// The end of the headline at `heading` as Org's parser has it: the
    /// start of the next headline as high, or the end of the text.
    pub fn element_end(&self, heading: usize) -> usize {
        let level = level_of(&self.text[heading..]).unwrap_or(0);
        let mut at = heading;
        while let Some(newline) = self.text[at..].find('\n') {
            at += newline + 1;
            if level_of(&self.text[at..]).is_some_and(|other| other <= level) {
                return at;
            }
        }
        self.text.len()
    }

    /// The headline above `heading` of a lower level.
    pub fn parent_heading(&self, heading: usize) -> Option<usize> {
        let level = level_of(&self.text[heading..])?;
        let mut at = heading;
        while at > 0 {
            at = self.text[..at - 1].rfind('\n').map_or(0, |newline| newline + 1);
            if level_of(&self.text[at..]).is_some_and(|other| other < level) {
                return Some(at);
            }
        }
        None
    }

    /// `org-back-to-heading`
    pub fn org_back_to_heading(&mut self, invisible_ok: bool) -> Result<usize, String> {
        self.beginning_of_line();
        if self.at_heading(!invisible_ok) {
            return Ok(self.point);
        }
        let mut heading = self.containing_heading(self.point);
        while let Some(at) = heading {
            if invisible_ok || !self.invisible(at) {
                self.point = at;
                return Ok(at);
            }
            heading = self.parent_heading(at);
        }
        let position = self.text[..self.point].chars().count() + 1;
        Err(format!("Before first headline at position {position} in buffer {}", self.name()))
    }

    /// `outline-back-to-heading`, as Org makes it (`org-compat`):
    /// backwards to a headline line in the accessible portion.
    pub fn outline_back_to_heading(&mut self, invisible_ok: bool) -> Result<usize, String> {
        self.beginning_of_line();
        if self.at_heading(!invisible_ok) {
            return Ok(self.point);
        }
        let saved = self.point;
        loop {
            if self.re_search_backward(&HEADING, false).is_none() {
                self.point = saved;
                return Err("Before first heading".to_string());
            }
            if invisible_ok || !self.invisible(self.point) {
                return Ok(self.point);
            }
        }
    }

    /// `outline-next-heading`
    pub fn outline_next_heading(&mut self) -> bool {
        if self.bolp() && !self.eobp() {
            self.point += 1;
        }
        match self.re_search_forward(&HEADING, None, true) {
            Some(found) => {
                self.point = found.start();
                true
            }
            None => false,
        }
    }

    /// `outline-end-of-subtree`
    pub fn outline_end_of_subtree(&mut self) -> Result<(), String> {
        self.outline_back_to_heading(false)?;
        let level = self.outline_level();
        let mut first = true;
        while !self.eobp() && (first || self.outline_level() > level) {
            first = false;
            self.outline_next_heading();
        }
        if self.bolp() && !self.eolp() {
            self.backward_char();
        }
        Ok(())
    }

    /// `org-end-of-subtree`
    pub fn org_end_of_subtree(&mut self, invisible_ok: bool, to_heading: bool) -> usize {
        let heading = match self.org_back_to_heading(invisible_ok) {
            Ok(heading) => Some(heading),
            Err(_) => {
                self.point = self.begv;
                None
            }
        };
        match heading.filter(|heading| level_of(&self.text[*heading..]).is_some()) {
            Some(heading) => self.goto(self.element_end(heading)),
            None => self.point = self.zv,
        }
        if !to_heading && matches!(self.char_before(), Some('\n' | '\r')) {
            self.backward_char();
            self.skip_backward(&['\n', '\r', '\t', ' ']);
        }
        self.point
    }

    /// `org-up-heading-safe`: to the parent of the headline at the point,
    /// with its level; to that headline itself, without, when it has no
    /// parent in the accessible portion.
    pub fn org_up_heading_safe(&mut self) -> Option<usize> {
        let current = self.containing_heading(self.point)?;
        match self.parent_heading(current).filter(|parent| *parent >= self.begv) {
            Some(parent) => {
                self.point = parent;
                level_of(&self.text[parent..])
            }
            None => {
                if current >= self.begv {
                    self.point = current;
                }
                None
            }
        }
    }

    /// `org-goto-first-child`
    pub fn org_goto_first_child(&mut self) -> bool {
        let (begin, end) = match self.containing_heading(self.point) {
            Some(heading) => {
                let line_end = self.text[heading..].find('\n').map(|newline| heading + newline + 1);
                let end = self.element_end(heading);
                match line_end.filter(|begin| *begin < end) {
                    Some(begin) => (begin, end),
                    None => return false,
                }
            }
            None => (0, self.text.len()),
        };
        match HEADING.find_at(&self.text[..end], begin) {
            Some(found) => {
                self.point = found.start();
                true
            }
            None => false,
        }
    }

    /// `org-narrow-to-subtree`
    pub fn org_narrow_to_subtree(&mut self) -> Result<(), String> {
        let heading = self.containing_heading(self.point).ok_or_else(|| "Before first heading".to_string())?;
        if heading < self.begv {
            return Err("Before first heading".to_string());
        }
        let end = self.element_end(heading);
        let end = if end >= self.zv { self.zv } else { end - 1 };
        self.narrow(heading, end);
        Ok(())
    }

    /// `org-next-visible-heading` 1, in a buffer whose headlines show:
    /// the next headline after the line of the point, or the end.
    pub fn org_next_visible_heading(&mut self) {
        self.end_of_line();
        if self.re_search_forward(&HEADING, None, true).is_some() {
            self.beginning_of_line();
        } else {
            self.point = self.zv;
        }
    }

    // Folding.

    /// `org-fold-region` for headlines.
    pub fn fold(&mut self, from: usize, to: usize, flag: bool) {
        let (from, to) = (from.max(self.begv), to.min(self.zv));
        for hidden in self.hidden.iter_mut().take(to).skip(from) {
            *hidden = flag;
        }
    }

    /// `org-fold-hide-sublevels`, which `outline-hide-sublevels` is in Org.
    pub fn hide_sublevels(&mut self, levels: usize) {
        let saved = self.point;
        self.point = self.begv;
        if !self.at_heading(false) {
            self.outline_next_heading();
        }
        let mut begin = self.point;
        self.point = self.zv;
        let mut end = if self.bolp() { self.begv.max(self.point.saturating_sub(1)) } else { self.point };
        if end < begin {
            std::mem::swap(&mut begin, &mut end);
        }
        self.fold(begin, end, true);
        // `org-map-region`: unfold the headlines as high.
        let show = |buffer: &mut Buffer| {
            if buffer.outline_level() <= levels {
                let start = buffer.line_start(buffer.point);
                let from = buffer.previous_line_end(start);
                let to = buffer.line_end(start);
                buffer.fold(from, to, false);
            }
        };
        self.point = begin;
        if let Some(found) = self.re_search_forward(&HEADING, None, false) {
            if self.point < end {
                self.point = found.start();
                show(self);
            }
        }
        while self.outline_next_heading() && self.point < end && !self.eobp() {
            show(self);
        }
        // The last newline shows.
        self.point = self.zv;
        if self.bolp() && !self.bobp() && self.invisible(self.point - 1) {
            self.fold(self.point - 1, self.point, false);
        }
        self.point = saved;
    }

    /// `outline-next-preface`
    fn next_preface(&mut self) {
        static PREFACE: LazyLock<Regex> = LazyLock::new(|| search_regex(r"\n\*+ "));
        if let Some(found) = self.re_search_forward(&PREFACE, None, true) {
            self.point = found.start();
        }
        if self.bolp() && self.eobp() && !self.bobp() {
            self.backward_char();
        }
    }

    /// `outline-show-entry`
    pub fn show_entry(&mut self) -> Result<(), String> {
        let saved = self.point;
        let heading = self.outline_back_to_heading(true)?;
        self.next_preface();
        let end = if self.zv - self.point == 1 { self.zv } else { self.point };
        self.fold(heading.saturating_sub(1), end, false);
        self.point = saved;
        Ok(())
    }

    /// `outline-up-heading` 1 with INVISIBLE-OK
    fn outline_up_heading(&mut self) -> Result<(), String> {
        self.outline_back_to_heading(true)?;
        let start_level = self.outline_level();
        if start_level <= 1 {
            return Err("Already at top level of the outline".to_string());
        }
        if !self.bobp() {
            let mut level = start_level;
            while !(level < start_level || self.bobp()) {
                // `outline-previous-heading`
                self.re_search_backward(&HEADING, true);
                level = self.outline_level();
            }
        }
        Ok(())
    }

    /// `outline-hide-other`
    pub fn hide_other(&mut self) -> Result<(), String> {
        self.hide_sublevels(1);
        let saved = self.point;
        self.outline_back_to_heading(true)?;
        self.show_entry()?;
        while self.outline_up_heading().is_ok() && !self.bobp() {
            let next = self.text[self.point..self.zv].find('\n').map_or(self.zv, |newline| self.point + newline + 1);
            self.fold(self.point.saturating_sub(1), next, false);
        }
        self.point = saved;
        Ok(())
    }

    /// `outline-show-subtree`
    pub fn show_subtree(&mut self) -> Result<(), String> {
        let saved = self.point;
        self.outline_back_to_heading(false)?;
        // `outline-end-of-heading`
        if self.re_search_forward(&search_regex("\n"), None, true).is_some() {
            self.backward_char();
        }
        let from = self.point;
        self.outline_end_of_subtree()?;
        let to = self.point;
        self.fold(from, to, false);
        self.point = saved;
        Ok(())
    }

    /// `outline-show-all`
    pub fn show_all(&mut self) {
        self.fold(self.begv, self.zv, false);
    }

    /// The lines whose start a headline's fold hides, counted from 1.
    pub fn hidden_lines(&self) -> Vec<usize> {
        let mut lines = Vec::new();
        let mut start = 0;
        let mut number = 1;
        while start < self.text.len() {
            if self.hidden[start] {
                lines.push(number);
            }
            start = self.text[start..].find('\n').map_or(self.text.len(), |newline| start + newline + 1);
            number += 1;
        }
        lines
    }
}
