//! Org's table editing commands (`org-table.el`): moving between fields
//! (`TAB`, `S-TAB`), moving, inserting and deleting rows and columns
//! (`M-<arrows>`, `M-S-<arrows>`), horizontal lines (`C-c -`, `C-c RET`),
//! blanking a field (`C-c SPC`) and sorting rows (`C-c ^`). Each is a port
//! of the Emacs command, run on the text as Emacs runs it on a buffer: a
//! point that moves, text inserted and deleted around it, the table
//! aligned when the command aligns it, and the `#+TBLFM:` lines after the
//! table changed to follow the rows and columns they name
//! (`org-table-fix-formulas`).

use std::sync::LazyLock;

use regex::Regex;

use crate::text::char_width;

/// What a table command makes of the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Done {
    pub text: String,
    pub cursor: usize,
    /// What went to the kill ring (`org-table-kill-row`).
    pub killed: Option<String>,
}

/// The table commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// `org-table-next-field` (`TAB`).
    NextField,
    /// `org-table-previous-field` (`S-TAB`).
    PreviousField,
    /// `org-table-move-column` (`M-<left>`, `M-<right>`).
    MoveColumn { left: bool },
    /// `org-table-move-row` (`M-<up>`, `M-<down>`).
    MoveRow { up: bool },
    /// `org-table-insert-column` (`M-S-<right>`).
    InsertColumn,
    /// `org-table-delete-column` (`M-S-<left>`).
    DeleteColumn,
    /// `org-table-insert-row` (`M-S-<down>`), below with its argument.
    InsertRow { below: bool },
    /// `org-table-kill-row` (`M-S-<up>`).
    KillRow,
    /// `org-table-insert-hline` (`C-c -`), above with its argument.
    InsertHline { above: bool },
    /// `org-table-hline-and-move` (`C-c RET`).
    HlineAndMove,
    /// `org-table-blank-field` (`C-c SPC`).
    BlankField,
    /// `org-table-align` (`C-c C-c` in a table).
    Align,
    /// `org-table-sort-lines` (`C-c ^`) with a sorting type (`a`, `n`,
    /// `t`, capital to reverse) and the column to sort by when the cursor
    /// is before the first one, which Org asks for.
    Sort { kind: char, column: Option<usize> },
}

/// The sorting types `org-table-sort-lines` reads; a capital reverses
/// the order.
pub const SORT_TYPES: &[(char, &str)] = &[('a', "alphabetic"), ('n', "numeric"), ('t', "time")];

/// What [`Command::Align`] answers where `C-c C-c` recalculates the
/// whole table instead.
pub const RECALCULATE: &str = "C-c C-c here recalculates the table";

/// Whether `C-c C-c` at `at` recalculates the whole table
/// (`org-table-recalculate`) rather than aligning it: at the start of the
/// table, where `org-element-context` finds the table itself.
pub fn recalculates(text: &str, at: usize) -> bool {
    let buffer = Buffer::new(text, at);
    buffer.at_table() && at == buffer.table_begin()
}

/// What `org-table-sort-lines` asks before the first column.
pub const ASK_COLUMN: &str = "Use column N for sorting";

/// Whether the line at `at` is a table line (`org-table-line-regexp`).
pub fn at_table(text: &str, at: usize) -> bool {
    let start = text[..at].rfind('\n').map_or(0, |index| index + 1);
    text[start..].trim_start_matches([' ', '\t']).starts_with('|')
}

/// Run `command` with the cursor at byte `at`, which is on a table line.
pub fn run(text: &str, at: usize, command: Command) -> Result<Done, String> {
    let mut buffer = Buffer::new(text, at);
    if !buffer.at_table() {
        return Err("Not at a table".to_string());
    }
    let mut killed = None;
    match command {
        Command::NextField => buffer.next_field()?,
        Command::PreviousField => buffer.previous_field()?,
        Command::MoveColumn { left } => buffer.move_column(left)?,
        Command::MoveRow { up } => buffer.move_row(up)?,
        Command::InsertColumn => buffer.insert_column()?,
        Command::DeleteColumn => buffer.delete_column()?,
        Command::InsertRow { below } => buffer.insert_row(below)?,
        Command::KillRow => killed = Some(buffer.kill_row()?),
        Command::InsertHline { above } => buffer.insert_hline(above)?,
        Command::HlineAndMove => buffer.hline_and_move()?,
        Command::BlankField => buffer.blank_field()?,
        Command::Align => {
            if recalculates(text, at) {
                return Err(RECALCULATE.to_string());
            }
            buffer.maybe_eval_formula()?;
            buffer.maybe_recalculate_line()?;
            buffer.align();
        }
        Command::Sort { kind, column } => buffer.sort_lines(kind, column)?,
    }
    Ok(Done { text: buffer.text, cursor: buffer.point, killed })
}

/// The text and the point, edited as Emacs edits a buffer, with the
/// markers a command keeps.
struct Buffer {
    text: String,
    point: usize,
    markers: Vec<usize>,
    /// `org-table-may-need-update`
    may_need_update: bool,
    /// The parts of links that font-lock hides (`org-link-descriptive`),
    /// which take no columns: the text of the document is fontified, what
    /// a command inserts is not until it aligns the table again.
    hidden: Vec<(usize, usize)>,
}

/// `org-link-bracket-re`
static BRACKET_LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[((?:[^\]\[\\]|\\(?:\\\\)*[\]\[]|\\+[^\]\[])+)\](?:\[([\s\S]+?)\])?\]").unwrap());

/// The parts of the bracket links of `text[start..end]` that Org hides: all
/// but the description, or the brackets of a link without one.
fn hidden_parts(text: &str, start: usize, end: usize) -> Vec<(usize, usize)> {
    let mut hidden = Vec::new();
    for found in BRACKET_LINK.captures_iter(&text[start..end]) {
        let whole = found.get(0).expect("a match");
        let (from, to) = (start + whole.start(), start + whole.end());
        match found.get(2) {
            Some(description) => {
                hidden.push((from, start + description.start()));
                hidden.push((start + description.end(), to));
            }
            None => {
                hidden.push((from, from + 2));
                hidden.push((to - 2, to));
            }
        }
    }
    hidden
}

static DATA_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^[ \t]*\|[^-]").unwrap());
static HLINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^[ \t]*\|-").unwrap());

impl Buffer {
    fn new(text: &str, point: usize) -> Buffer {
        Buffer {
            text: text.to_string(),
            point,
            markers: Vec::new(),
            may_need_update: true,
            hidden: hidden_parts(text, 0, text.len()),
        }
    }

    // ----- Emacs primitives

    fn bol_at(&self, at: usize) -> usize {
        self.text[..at].rfind('\n').map_or(0, |index| index + 1)
    }

    fn eol_at(&self, at: usize) -> usize {
        self.text[at..].find('\n').map_or(self.text.len(), |index| at + index)
    }

    fn bol(&self) -> usize {
        self.bol_at(self.point)
    }

    fn eol(&self) -> usize {
        self.eol_at(self.point)
    }

    fn bolp(&self) -> bool {
        self.point == self.bol()
    }

    fn eobp(&self) -> bool {
        self.point == self.text.len()
    }

    /// `(line-beginning-position 2)`
    fn next_bol(&self) -> usize {
        let eol = self.eol();
        if eol < self.text.len() {
            eol + 1
        } else {
            eol
        }
    }

    fn line(&self) -> &str {
        &self.text[self.bol()..self.eol()]
    }

    /// `forward-line`: to the start of the line `n` lines away; past the
    /// last line, to the end of the buffer.
    fn forward_line(&mut self, n: i64) {
        if n <= 0 {
            let mut start = self.bol();
            for _ in 0..-n {
                if start == 0 {
                    break;
                }
                start = self.bol_at(start - 1);
            }
            self.point = start;
        } else {
            for _ in 0..n {
                let eol = self.eol();
                if eol == self.text.len() {
                    self.point = eol;
                    return;
                }
                self.point = eol + 1;
            }
        }
    }

    fn end_of_line(&mut self) {
        self.point = self.eol();
    }

    fn rest(&self) -> &str {
        &self.text[self.point..]
    }

    /// Text inserted at `at` with its hidden parts (relative to it): the
    /// markers after it move, those at it too with `before_markers`.
    fn put(&mut self, at: usize, what: &str, hidden: &[(usize, usize)], before_markers: bool) {
        let length = what.len();
        self.text.insert_str(at, what);
        for marker in &mut self.markers {
            if *marker > at || (before_markers && *marker == at) {
                *marker += length;
            }
        }
        let mut ranges = Vec::new();
        for &(start, end) in &self.hidden {
            if end <= at {
                ranges.push((start, end));
            } else if start >= at {
                ranges.push((start + length, end + length));
            } else {
                ranges.push((start, at));
                ranges.push((at + length, end + length));
            }
        }
        ranges.extend(hidden.iter().map(|(start, end)| (at + start, at + end)));
        ranges.sort_unstable();
        self.hidden = ranges;
    }

    /// `insert`: at the point, which goes after it; markers at the point
    /// stay before it.
    fn insert(&mut self, what: &str) {
        let at = self.point;
        self.put(at, what, &[], false);
        self.point += what.len();
    }

    /// `insert-before-markers`
    fn insert_before_markers(&mut self, what: &str) {
        let at = self.point;
        self.put(at, what, &[], true);
        self.point += what.len();
    }

    /// An insertion at `at` with the point kept (`save-excursion`).
    fn insert_at(&mut self, at: usize, what: &str) {
        let point = self.point;
        self.put(at, what, &[], false);
        if point > at {
            self.point = point + what.len();
        }
    }

    /// The text of `start..end` and its hidden parts, relative to it.
    fn take(&self, start: usize, end: usize) -> (String, Vec<(usize, usize)>) {
        let hidden = self
            .hidden
            .iter()
            .filter(|(from, to)| *to > start && *from < end)
            .map(|(from, to)| ((*from).max(start) - start, (*to).min(end) - start))
            .collect();
        (self.text[start..end].to_string(), hidden)
    }

    /// `delete-and-extract-region`: what went, and its hidden parts.
    fn delete_with(&mut self, start: usize, end: usize) -> (String, Vec<(usize, usize)>) {
        let removed = self.take(start, end);
        self.text.replace_range(start..end, "");
        let shift = |position: usize| {
            if position >= end {
                position - (end - start)
            } else if position > start {
                start
            } else {
                position
            }
        };
        for marker in &mut self.markers {
            *marker = shift(*marker);
        }
        self.point = shift(self.point);
        self.hidden =
            self.hidden.iter().map(|(from, to)| (shift(*from), shift(*to))).filter(|(from, to)| from < to).collect();
        removed
    }

    fn delete(&mut self, start: usize, end: usize) -> String {
        self.delete_with(start, end).0
    }

    /// `start..end` replaced by `what`, with its hidden parts, the point
    /// where it was or, inside, at the start.
    fn replace(&mut self, start: usize, end: usize, what: &str, hidden: &[(usize, usize)]) {
        let point = self.point;
        self.delete(start, end);
        self.put(start, what, hidden, false);
        self.point = if point >= end { point - (end - start) + what.len() } else { point.min(start) };
        if point <= start {
            self.point = point;
        }
    }

    fn is_hidden(&self, at: usize) -> bool {
        self.hidden.iter().any(|(start, end)| *start <= at && at < *end)
    }

    /// The width of `start..end` shown from the start of a line, hidden
    /// parts taking none (`org-string-width`).
    fn width_of(&self, start: usize, end: usize) -> usize {
        let mut column = 0;
        for (index, c) in self.text[start..end].char_indices() {
            if self.is_hidden(start + index) {
                continue;
            }
            column = if c == '\t' { (column / 8 + 1) * 8 } else { column + char_width(c) };
        }
        column
    }

    /// `re-search-forward` for a literal, the match ending by `bound`.
    fn search_forward(&mut self, what: &str, bound: usize) -> bool {
        match self.text[self.point..].find(what) {
            Some(index) if self.point + index + what.len() <= bound => {
                self.point += index + what.len();
                true
            }
            _ => false,
        }
    }

    /// `search-backward` for a literal, the match starting at `bound` or
    /// after.
    fn search_backward(&mut self, what: &str, bound: usize) -> bool {
        match self.text[..self.point].rfind(what) {
            Some(index) if index >= bound => {
                self.point = index;
                true
            }
            _ => false,
        }
    }

    /// The display width of the text from the start of the line to `at`.
    fn column_at(&self, at: usize) -> usize {
        self.width_of(self.bol_at(at), at)
    }

    /// `current-column`
    fn current_column(&self) -> usize {
        self.column_at(self.point)
    }

    /// `move-to-column`: the first place at the column or after it, or the
    /// end of the line.
    fn move_to_column(&mut self, target: usize) {
        let start = self.bol();
        let end = self.eol();
        let mut column = 0;
        self.point = start;
        for (index, c) in self.text[start..end].char_indices() {
            // Hidden text is passed over before the column counts.
            if self.is_hidden(start + index) {
                continue;
            }
            if column >= target {
                self.point = start + index;
                return;
            }
            column = if c == '\t' { (column / 8 + 1) * 8 } else { column + char_width(c) };
        }
        self.point = end;
    }

    // ----- tables

    fn at_table(&self) -> bool {
        self.line().trim_start_matches([' ', '\t']).starts_with('|')
    }

    /// `org-at-table-hline-p`
    fn at_hline(&self) -> bool {
        self.line().trim_start_matches([' ', '\t']).starts_with("|-")
    }

    /// Whether the line at `start` is out of the table
    /// (`org-table-border-regexp`).
    fn is_border(&self, start: usize) -> bool {
        let rest = self.text[start..].trim_start_matches([' ', '\t']);
        rest.chars().next().is_some_and(|c| c != '|')
    }

    /// `org-table-begin`
    fn table_begin(&self) -> usize {
        let mut start = self.bol();
        while start > 0 {
            let previous = self.bol_at(start - 1);
            if self.is_border(previous) {
                return start;
            }
            start = previous;
        }
        0
    }

    /// `org-table-end`
    fn table_end(&self) -> usize {
        if self.bolp() && self.is_border(self.point) {
            return self.point;
        }
        let mut start = self.next_bol();
        if start == self.eol() {
            // The last line.
            start = self.text.len();
        }
        while start < self.text.len() {
            if self.is_border(start) {
                return start;
            }
            let eol = self.eol_at(start);
            if eol == self.text.len() {
                break;
            }
            start = eol + 1;
        }
        let trimmed = self.text.trim_end_matches([' ', '\t']).len();
        if trimmed == self.bol_at(trimmed) {
            trimmed
        } else {
            self.eol_at(trimmed)
        }
    }

    /// `org-table-current-column`
    fn table_column(&self) -> usize {
        let line = &self.text[self.bol()..self.point];
        match line.find('|') {
            None => 0,
            Some(first) => {
                let rest = &line[first + 1..];
                1 + if self.at_hline() { rest.matches(['|', '+']).count() } else { rest.matches('|').count() }
            }
        }
    }

    /// `org-table-current-dline`
    fn current_dline(&self) -> usize {
        let line = self.bol();
        let mut start = self.table_begin();
        let mut count = 0;
        while start <= line {
            if DATA_LINE.find_at(&self.text, start).is_some_and(|found| found.start() == start) {
                count += 1;
            }
            let eol = self.eol_at(start);
            if eol == self.text.len() {
                break;
            }
            start = eol + 1;
        }
        count
    }

    /// `org-table-goto-column`
    fn goto_column(&mut self, n: usize, on_delim: bool, force: bool) {
        self.forward_line(0);
        if n == 0 {
            return;
        }
        let mut n = n as i64;
        loop {
            n -= 1;
            if n <= -1 {
                break;
            }
            let eol = self.eol();
            if self.search_forward("|", eol) {
                continue;
            }
            if !force {
                break;
            }
            self.end_of_line();
            let back = self.text[self.bol()..self.point].rfind('|').map_or(self.bol(), |bar| self.bol() + bar + 1);
            self.point = back;
            self.insert(" | ");
        }
        if force && !self.text[self.point..self.eol()].contains('|') {
            let eol = self.eol();
            self.insert_at(eol, " | ");
        }
        if on_delim {
            self.point -= 1;
        } else if self.rest().starts_with(' ') {
            self.point += 1;
        }
    }

    /// `org-table-clean-line` on the point's line: only bars and blanks,
    /// each field with text as wide as Org shows it.
    fn clean_line(&self) -> String {
        let (start, end) = (self.bol(), self.eol());
        let line = &self.text[start..end];
        if line.trim_start_matches([' ', '\t']).starts_with("|-") {
            return line.chars().map(|c| if c == '|' || c == '+' { '|' } else { ' ' }).collect();
        }
        let bars: Vec<usize> = line.match_indices('|').map(|(index, _)| index).collect();
        let mut out = String::new();
        let mut last = 0;
        for pair in bars.windows(2) {
            let (from, to) = (pair[0] + 1, pair[1]);
            if line[from..to].trim_matches([' ', '\t', '\r']).is_empty() {
                continue;
            }
            out.push_str(&line[last..from]);
            out.push_str(&" ".repeat(self.width_of(start + from, start + to)));
            last = to;
        }
        out.push_str(&line[last..]);
        out
    }

    /// `org-table-check-inside-data-field`
    fn check_inside_data_field(&self) -> Result<(), String> {
        let before = self.text[self.bol()..self.point].trim_end_matches([' ', '\t']);
        let rest = &self.text[self.point..self.eol()];
        if !self.at_table() || before.is_empty() || self.at_hline() || rest.trim_start_matches([' ', '\t']).is_empty() {
            return Err("Not in table data field".to_string());
        }
        Ok(())
    }

    /// `org-table-find-dataline`
    fn find_dataline(&mut self) -> Result<(), String> {
        if !self.at_hline() {
            return Ok(());
        }
        let column = self.current_column();
        let end = self.table_end();
        self.forward_line(1);
        while self.point < end && self.at_hline() {
            self.forward_line(1);
        }
        if self.point >= end {
            return Err("Cannot find data row for column operation".to_string());
        }
        self.move_to_column(column);
        Ok(())
    }

    /// `org-table-align`, with `org-table-save-field`.
    fn align(&mut self) {
        let begin = self.table_begin();
        let end = self.table_end();
        let line_index = self.text[begin..self.bol()].matches('\n').count();
        let column = self.table_column();
        // `font-lock-ensure`: the table as Org shows it.
        let fontified = hidden_parts(&self.text, begin, end);
        self.hidden.retain(|(from, to)| *to <= begin || *from >= end);
        self.hidden.extend(fontified);
        self.hidden.sort_unstable();
        let text = self.text.clone();
        let lines: Vec<&str> = text[begin..end].split_terminator('\n').collect();
        if !lines.is_empty() {
            let aligned = crate::format::align_table(&lines);
            let mut out = String::new();
            let mut at = begin;
            for (old, new) in lines.iter().zip(&aligned) {
                let line_end = at + old.len();
                let has_newline = line_end < text.len() && text.as_bytes()[line_end] == b'\n';
                if old == new {
                    out.push_str(old);
                    if has_newline {
                        out.push('\n');
                    }
                } else {
                    out.push_str(new);
                    out.push('\n');
                }
                at = line_end + usize::from(has_newline);
            }
            // The new lines are made of the fields, which keep what
            // font-lock did to them.
            let kept = hidden_parts(&out, 0, out.len());
            let markers = self.markers.clone();
            self.delete(begin, at);
            self.put(begin, &out, &kept, false);
            for (marker, old) in self.markers.iter_mut().zip(markers) {
                if old >= at {
                    *marker = old - (at - begin) + out.len();
                }
            }
        }
        // Back to the line, then to the field.
        let mut start = begin;
        for _ in 0..line_index {
            let eol = self.eol_at(start);
            start = if eol < self.text.len() { eol + 1 } else { eol };
        }
        self.point = start;
        self.goto_column(column, false, false);
        self.may_need_update = false;
    }

    /// `org-table-get-field`: the text of the field at the point, which
    /// goes to the start of the field (just after it when it has text).
    fn get_field(&mut self) -> String {
        let back = self.text[..self.point].rfind(['|', '\n']).map_or(0, |index| index + 1);
        self.point = back;
        let rest = &self.text[self.point..self.eol()];
        if self.bolp() || rest.trim_start_matches([' ', '\t']).is_empty() {
            return String::new();
        }
        let end = rest.find(['|', '\r']).unwrap_or(rest.len());
        let value = rest[..end].to_string();
        let next = self.rest().chars().next().map_or(0, char::len_utf8);
        self.point = (self.point + next).min(self.eol());
        value
    }

    /// `org-table-maybe-eval-formula`: a field that is a formula
    /// (`=...`, `:=...`) is evaluated, which org-lsp refuses.
    fn maybe_eval_formula(&mut self) -> Result<(), String> {
        static FORMULA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^:?=(.*[^=])$").unwrap());
        let field = self.get_field();
        if FORMULA.is_match(field.trim_matches([' ', '\t', '\n', '\r'])) {
            return Err("A formula in a field is not evaluated; put it in a #+TBLFM: line".to_string());
        }
        Ok(())
    }

    /// `org-table-maybe-recalculate-line`: a line marked `#` is
    /// recalculated, which org-lsp refuses here.
    fn maybe_recalculate_line(&self) -> Result<(), String> {
        static MARKED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[ \t]*\| *# *(\||$)").unwrap());
        if MARKED.is_match(self.line()) {
            return Err("Recalculate the marked line with org.table.recalculate.line".to_string());
        }
        Ok(())
    }

    // ----- the commands

    /// `org-table-next-field`
    fn next_field(&mut self) -> Result<(), String> {
        self.maybe_eval_formula()?;
        self.maybe_recalculate_line()?;
        if self.may_need_update {
            self.align();
        }
        let end = self.table_end();
        if self.at_hline() {
            self.end_of_line();
        }
        let moved = (|| {
            if !self.search_forward("|", end) {
                return false;
            }
            if self.text[self.point..self.eol()].trim_start_matches([' ', '\t']).is_empty()
                && !self.search_forward("|", end)
            {
                return false;
            }
            if self.rest().starts_with('-') {
                // `org-table-tab-jumps-over-hlines`: to the next data line.
                if let Some(found) = DATA_LINE.find_at(&self.text, self.point).filter(|found| found.end() <= end) {
                    self.point = found.end() - found.as_str().chars().last().map_or(0, char::len_utf8);
                }
            }
            true
        })();
        if !moved {
            return self.insert_row(true);
        }
        if self.rest().starts_with('-') {
            self.forward_line(-1);
            return self.insert_row(true);
        }
        if self.rest().starts_with(' ') {
            self.point += 1;
        }
        Ok(())
    }

    /// `org-table-previous-field`
    fn previous_field(&mut self) -> Result<(), String> {
        self.maybe_recalculate_line()?;
        if self.may_need_update {
            self.align();
        }
        if self.at_hline() {
            self.end_of_line();
        }
        let start = self.table_begin();
        let origin = self.point;
        let found = (|| {
            for _ in 0..2 {
                if !self.search_backward("|", start) {
                    return false;
                }
            }
            loop {
                let rest = &self.text[self.point + 1..self.eol()];
                if !(rest.starts_with('-') || rest.trim_start_matches([' ', '\t']).is_empty()) {
                    return true;
                }
                if !self.search_backward("|", start) {
                    return false;
                }
            }
        })();
        if !found {
            self.point = origin;
            return Err("Cannot move to previous table field".to_string());
        }
        if self.rest().starts_with('|') {
            self.point += 1;
            if self.rest().starts_with(' ') {
                self.point += 1;
            }
        }
        Ok(())
    }

    /// `org-table-move-column`
    fn move_column(&mut self, left: bool) -> Result<(), String> {
        static LAST: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\A[^|\n]*\|[^|\n]*$").unwrap());
        static PAIR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\A\|([^|\n]+)\|([^|\n]+)\|").unwrap());
        self.find_dataline()?;
        self.check_inside_data_field()?;
        let column = self.table_column();
        let first = if left { column - 1 } else { column };
        let to = if left { column - 1 } else { column + 1 };
        if left && column == 1 {
            return Err("Cannot move column further left".to_string());
        }
        let line_rest = &self.text[self.point..self.eol()];
        if !left && LAST.is_match(line_rest) {
            return Err("Cannot move column further right".to_string());
        }
        let begin = self.table_begin();
        let end = self.table_end();
        self.markers.push(end);
        let marker = self.markers.len() - 1;
        let line = self.bol();
        self.markers.push(line);
        let line_marker = self.markers.len() - 1;
        self.point = begin;
        while self.point < self.markers[marker] {
            if !self.at_hline() {
                self.goto_column(first, true, false);
                let pair = PAIR.captures(self.rest()).map(|found| {
                    (found.get(1).unwrap().range(), found.get(2).unwrap().range(), found.get(0).unwrap().len())
                });
                if let Some((one, two, length)) = pair {
                    // `transpose-regions`: the fields swap, with what
                    // font-lock did to them.
                    let start = self.point;
                    let (a, a_hidden) = self.take(start + one.start, start + one.end);
                    let (b, b_hidden) = self.take(start + two.start, start + two.end);
                    let whole = format!("|{b}|{a}|");
                    let mut hidden: Vec<(usize, usize)> =
                        b_hidden.iter().map(|(from, to)| (1 + from, 1 + to)).collect();
                    let offset = 2 + b.len();
                    hidden.extend(a_hidden.iter().map(|(from, to)| (offset + from, offset + to)));
                    self.replace(start, start + length, &whole, &hidden);
                    self.point = start;
                }
            }
            let before = self.point;
            self.forward_line(1);
            if self.point == before {
                break;
            }
        }
        self.point = self.markers[line_marker];
        self.markers.truncate(marker);
        self.goto_column(column, false, false);
        self.goto_column(to, false, false);
        self.align();
        let (from, into) = (column.to_string(), to.to_string());
        self.fix_formulas('$', &[(&from, &into), (&into, &from)], None, 0, None)?;
        Ok(())
    }

    /// `org-table-move-row`
    fn move_row(&mut self, up: bool) -> Result<(), String> {
        let column = self.current_column();
        let position = self.point;
        let hline1 = self.at_hline();
        let dline1 = self.current_dline();
        let dline2 = if up { dline1 as i64 - 1 } else { dline1 as i64 + 1 };
        if up && self.bol() == 0 {
            return Err("Cannot move row further".to_string());
        }
        self.forward_line(if up { -1 } else { 1 });
        if (!up && self.eobp()) || !self.at_table() {
            self.point = position;
            return Err("Cannot move row further".to_string());
        }
        let hline2 = self.at_hline();
        self.point = position;
        let (start, end) = (self.bol(), self.next_bol());
        let (row, hidden) = self.delete_with(start, end);
        self.forward_line(if up { -1 } else { 1 });
        if !self.bolp() {
            self.insert("\n");
        }
        let at = self.point;
        self.put(at, &row, &hidden, false);
        self.point = at + row.len();
        if !self.bolp() {
            self.insert("\n");
        }
        self.forward_line(-1);
        self.move_to_column(column);
        if !(hline1 || hline2) {
            let (one, two) = (dline1.to_string(), dline2.to_string());
            self.fix_formulas('@', &[(&one, &two), (&two, &one)], None, 0, None)?;
        }
        Ok(())
    }

    /// `org-table-insert-column`
    fn insert_column(&mut self) -> Result<(), String> {
        if self.eobp() {
            let at = self.point;
            self.insert_at(at, "\n");
        }
        if !ends_with_bar(self.line()) {
            self.align();
        }
        self.find_dataline()?;
        let column = self.table_column().max(1);
        let begin = self.table_begin();
        let end = self.table_end();
        self.markers.push(end);
        let marker = self.markers.len() - 1;
        let line = self.bol();
        self.markers.push(line);
        self.point = begin;
        while self.point < self.markers[marker] {
            if !self.at_hline() {
                self.goto_column(column, true, false);
                self.insert("|");
            }
            let before = self.point;
            self.forward_line(1);
            if self.point == before {
                break;
            }
        }
        self.point = self.markers[marker + 1];
        self.markers.truncate(marker);
        self.goto_column(column, false, false);
        self.align();
        self.fix_formulas('$', &[], Some(column - 1), 1, None)?;
        Ok(())
    }

    /// `org-table-delete-column`
    fn delete_column(&mut self) -> Result<(), String> {
        static FIELD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\A\|[^|\n]+\|").unwrap());
        self.find_dataline()?;
        if self.text[self.point..self.eol()].trim_start_matches([' ', '\t']).is_empty() && !self.search_backward("|", 0)
        {
            return Err("Search failed: \"|\"".to_string());
        }
        self.check_inside_data_field()?;
        let column = self.table_column();
        let begin = self.table_begin();
        let end = self.table_end();
        self.markers.push(end);
        let marker = self.markers.len() - 1;
        let line = self.bol();
        self.markers.push(line);
        self.point = begin;
        while self.point < self.markers[marker] {
            if !self.at_hline() {
                self.goto_column(column, true, false);
                if let Some(found) = FIELD.find(self.rest()) {
                    let (start, length) = (self.point, found.len());
                    self.delete(start + 1, start + length);
                }
            }
            let before = self.point;
            self.forward_line(1);
            if self.point == before {
                break;
            }
        }
        self.point = self.markers[marker + 1];
        self.markers.truncate(marker);
        self.goto_column(column, false, false);
        self.align();
        let invalid = column.to_string();
        self.fix_formulas('$', &[(&invalid, "INVALID")], Some(column), -1, Some(column))?;
        Ok(())
    }

    /// `org-table-insert-row`
    fn insert_row(&mut self, below: bool) -> Result<(), String> {
        static MARK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[ \t]*\| *[#*$] *\|").unwrap());
        if self.eobp() {
            let at = self.point;
            self.insert_at(at, "\n");
        }
        if !ends_with_bar(self.line()) {
            self.align();
        }
        let line = self.line().to_string();
        let mut new = self.clean_line();
        if let Some(mark) = MARK.find(&line) {
            if new.is_char_boundary(mark.end()) {
                new.replace_range(mark.range(), mark.as_str());
            }
        }
        self.forward_line(if below { 1 } else { 0 });
        if !self.bolp() {
            self.insert("\n");
        }
        self.insert_before_markers(&format!("{new}\n"));
        self.forward_line(-1);
        let eol = self.eol();
        if self.search_forward("|", eol) && self.rest().starts_with(' ') && self.point < eol {
            self.point += 1;
        }
        if self.may_need_update {
            self.align();
        }
        let dline = self.current_dline();
        self.fix_formulas('@', &[], Some(dline.saturating_sub(1)), 1, None)?;
        Ok(())
    }

    /// `org-table-kill-row`: the row deleted, and what went.
    fn kill_row(&mut self) -> Result<String, String> {
        let column = self.current_column();
        let dline = (!self.at_hline()).then(|| self.current_dline());
        let start = self.bol();
        let end = (self.eol() + 1).min(self.text.len());
        let killed = self.delete(start, end);
        if !self.at_table() {
            self.forward_line(-1);
        }
        self.move_to_column(column);
        if let Some(dline) = dline {
            let invalid = dline.to_string();
            self.fix_formulas('@', &[(&invalid, "INVALID")], Some(dline), -1, Some(dline))?;
        }
        Ok(killed)
    }

    /// `org-table-insert-hline`
    fn insert_hline(&mut self, above: bool) -> Result<(), String> {
        static BLANKS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\|( +)\|").unwrap());
        if self.eobp() {
            let at = self.point;
            self.insert_at(at, "\n");
        }
        if !ends_with_bar(self.line()) {
            self.align();
        }
        let mut line = self.clean_line();
        let column = self.current_column();
        while let Some(found) = BLANKS.captures(&line) {
            let blanks = found.get(1).unwrap().len();
            let range = found.get(0).unwrap().range();
            line.replace_range(range, &format!("+{}|", "-".repeat(blanks)));
        }
        if let Some(plus) = line.find('+') {
            line.replace_range(plus..plus + 1, "|");
        }
        self.forward_line(if above { 0 } else { 1 });
        self.insert(&format!("{line}\n"));
        self.forward_line(if above { 0 } else { -2 });
        self.move_to_column(column);
        Ok(())
    }

    /// `org-table-hline-and-move`
    fn hline_and_move(&mut self) -> Result<(), String> {
        self.maybe_eval_formula()?;
        self.maybe_recalculate_line()?;
        self.insert_hline(false)?;
        self.forward_line(1);
        self.end_of_line();
        let next = &self.text[self.point..];
        let rule_follows =
            next.strip_prefix('\n').is_some_and(|rest| rest.trim_start_matches([' ', '\t']).starts_with("|-"));
        if rule_follows {
            self.insert("\n|");
            self.align();
        } else {
            self.next_field()?;
        }
        Ok(())
    }

    /// `org-table-blank-field`
    fn blank_field(&mut self) -> Result<(), String> {
        static FIELD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\A\|[^|\n]+").unwrap());
        self.check_inside_data_field()?;
        let bol = self.bol();
        self.point = self.text[bol..self.point].rfind('|').map_or(bol, |index| bol + index);
        if let Some(length) = FIELD.find(self.rest()).map(|found| found.len()) {
            let start = self.point;
            let width = self.width_of(start, start + length);
            let blank = format!("|{}", " ".repeat(width.saturating_sub(1)));
            self.replace(start, start + length, &blank, &[]);
            self.point = start + 2;
        }
        Ok(())
    }

    /// `org-table-sort-lines` with the sorting type `kind`: the data lines
    /// between the rules around the cursor (or the whole table), stably
    /// by the field of the cursor's column.
    fn sort_lines(&mut self, kind: char, asked: Option<usize>) -> Result<(), String> {
        // The point must be in a field, or before a data line.
        {
            let saved = self.point;
            if self.text[self.bol()..self.point].trim_end_matches([' ', '\t']).is_empty() {
                let eol = self.eol();
                self.search_forward("|", eol);
            }
            let checked = self.check_inside_data_field();
            self.point = saved;
            checked?;
        }
        let column = match (self.table_column(), asked) {
            (0, Some(column)) => column,
            (0, None) => return Err(ASK_COLUMN.to_string()),
            (column, _) => column,
        };
        let lower = kind.to_ascii_lowercase();
        if lower == 'f' {
            return Err("Sorting by a function of your own is not supported".to_string());
        }
        if !SORT_TYPES.iter().any(|(known, _)| *known == lower) {
            return Err(format!("Invalid sorting type ‘{kind}’"));
        }
        let start = self.table_begin();
        let end = self.table_end();
        // Between the rules around the point.
        let line = self.bol();
        let mut region_start = start;
        let mut scan = line;
        while scan > start {
            scan = self.bol_at(scan - 1);
            if HLINE.find_at(&self.text, scan).is_some_and(|found| found.start() == scan) {
                region_start = self.eol_at(scan) + 1;
                break;
            }
        }
        let region_end = HLINE
            .find_at(&self.text, self.point)
            .filter(|found| found.end() <= end && found.start() > self.point)
            .map_or(end, |found| found.start());
        let coordinates = (self.text[region_start..line].matches('\n').count(), self.current_column());
        let region = self.text[region_start..region_end].to_string();
        // The records: each line, from its start to its end.
        let mut records: Vec<(usize, usize)> = Vec::new();
        let mut at = 0;
        while at < region.len() {
            let line_end = region[at..].find('\n').map_or(region.len(), |index| at + index);
            records.push((at, line_end));
            at = line_end + 1;
        }
        let key = |record: &(usize, usize)| -> Key {
            let field = nth_field(&region[record.0..record.1], column);
            match lower {
                'n' => Key::Number(crate::sort::string_to_number(&field)),
                'a' => Key::Text(crate::sort::visible(&field).to_lowercase()),
                _ => Key::Number(time_key(&field)),
            }
        };
        let keyed: Vec<((usize, usize), Key)> = records.iter().map(|record| (*record, key(record))).collect();
        let mut sorted = keyed.clone();
        let reverse = lower != kind;
        if reverse {
            sorted.reverse();
        }
        sorted.sort_by(|a, b| a.1.compare(&b.1));
        if reverse {
            sorted.reverse();
        }
        let mut out = String::with_capacity(region.len());
        let mut hidden = Vec::new();
        let mut copy = |out: &mut String, from: usize, to: usize| {
            let (_, parts) = self.take(region_start + from, region_start + to);
            hidden.extend(parts.into_iter().map(|(a, b)| (out.len() + a, out.len() + b)));
            out.push_str(&region[from..to]);
        };
        let mut last = 0;
        for ((old, _), (new, _)) in keyed.iter().zip(&sorted) {
            copy(&mut out, last, old.0);
            copy(&mut out, new.0, new.1);
            last = old.1;
        }
        copy(&mut out, last, region.len());
        self.replace(region_start, region_end, &out, &hidden);
        // Back to where the point was.
        self.point = region_start;
        self.forward_line(coordinates.0 as i64);
        self.move_to_column(coordinates.1);
        Ok(())
    }

    /// `org-table-fix-formulas`: in the `#+TBLFM:` lines after the table,
    /// each reference `key` + number replaced as `replace` says, or past
    /// `limit` moved by `delta`; with `remove`, the formulas for that
    /// column or row go.
    fn fix_formulas(
        &mut self,
        key: char,
        replace: &[(&str, &str)],
        limit: Option<usize>,
        delta: i64,
        remove: Option<usize>,
    ) -> Result<(), String> {
        static TBLFM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\A[ \t]*#\+tblfm:").unwrap());
        static REMOTE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"remote\([^)]+?\)").unwrap());
        let reference = Regex::new(&format!(r"{}([0-9]+)", regex::escape(&key.to_string()))).unwrap();
        let removal = remove.map(|remove| {
            if key == '$' {
                Regex::new(&format!(r"(@[0-9]+)?\${remove}=.*?(::|$)")).unwrap()
            } else {
                Regex::new(&format!(r"@{remove}\$[0-9]+=.*?(::|$)")).unwrap()
            }
        });
        let mut at = self.table_end();
        while at < self.text.len() && TBLFM.is_match(&self.text[at..]) {
            let end = self.eol_at(at);
            let mut line = self.text[at..end].to_string();
            // `org-in-regexp` at the end of a match.
            let in_remote =
                |line: &str, at: usize| REMOTE.find_iter(line).any(|found| found.start() <= at && at <= found.end());
            if let Some(removal) = &removal {
                let mut from = 0;
                while let Some((range, term)) =
                    removal.find_at(&line, from).map(|found| (found.range(), found.as_str().to_string()))
                {
                    if in_remote(&line, range.end) {
                        from = range.end.max(range.start + 1);
                        continue;
                    }
                    if range.start > 0 && line.as_bytes()[range.start - 1] == b'.' {
                        return Err(format!("Change makes TBLFM term {term} invalid, use undo to recover"));
                    }
                    from = range.start;
                    line.replace_range(range, "");
                    if from >= line.len() {
                        break;
                    }
                }
            }
            let mut out = String::new();
            let mut last = 0;
            for found in reference.captures_iter(&line) {
                let whole = found.get(0).unwrap();
                if in_remote(&line, whole.end()) {
                    continue;
                }
                let number = &found[1];
                let value: usize = number.parse().unwrap_or(0);
                let new = if let Some((_, to)) = replace.iter().find(|(from, _)| *from == number) {
                    Some(format!("{key}{to}"))
                } else {
                    limit.filter(|limit| value > *limit).map(|_| format!("{key}{}", value as i64 + delta))
                };
                if let Some(new) = new {
                    out.push_str(&line[last..whole.start()]);
                    out.push_str(&new);
                    last = whole.end();
                }
            }
            out.push_str(&line[last..]);
            self.replace(at, end, &out, &[]);
            let new_end = at + out.len();
            if new_end >= self.text.len() {
                break;
            }
            at = new_end + 1;
        }
        Ok(())
    }
}

/// A key to sort rows by.
#[derive(Debug, Clone)]
enum Key {
    Number(f64),
    Text(String),
}

impl Key {
    fn compare(&self, other: &Key) -> std::cmp::Ordering {
        match (self, other) {
            (Key::Number(a), Key::Number(b)) => a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal),
            (Key::Text(a), Key::Text(b)) => a.cmp(b),
            _ => std::cmp::Ordering::Equal,
        }
    }
}

/// The time key of a field: a timestamp's time, a duration's minutes, a
/// `H:MM` in it, or 0.
fn time_key(field: &str) -> f64 {
    static STAMP: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"[\[<]([0-9]{4}-[0-9]{2}-[0-9]{2} ?[^\]\r\n>]*?)[\]>]").unwrap());
    static HOURS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b[0-9]+:[0-9]{2}\b").unwrap());
    if let Some(found) = STAMP.find(field) {
        if let Some(time) = crate::sort::parse_time(found.as_str()) {
            return crate::sort::seconds(time);
        }
    }
    let bare_number = field.trim().parse::<f64>().is_ok();
    if !field.is_empty() && !bare_number && crate::properties::is_duration(field) {
        return crate::clock::duration_to_minutes(field).unwrap_or(0.0);
    }
    if let Some(found) = HOURS.find(field) {
        return crate::clock::duration_to_minutes(found.as_str()).unwrap_or(0.0);
    }
    0.0
}

/// The field of column `n` of a table line, trimmed
/// (`org-table-get-field`): past as many bars as there are, so the last
/// field of a line with fewer.
fn nth_field(line: &str, n: usize) -> String {
    let mut rest = line;
    let mut found = 0;
    for _ in 0..n {
        match rest.find('|') {
            Some(bar) => {
                rest = &rest[bar + 1..];
                found += 1;
            }
            None => break,
        }
    }
    if found == 0 || rest.trim_start_matches([' ', '\t']).is_empty() {
        return String::new();
    }
    let end = rest.find('|').unwrap_or(rest.len());
    rest[..end].trim_matches([' ', '\t', '\n', '\r']).to_string()
}

/// Whether a line ends with a bar (`|[ \t]*$`).
fn ends_with_bar(line: &str) -> bool {
    line.trim_end_matches([' ', '\t']).ends_with('|')
}
