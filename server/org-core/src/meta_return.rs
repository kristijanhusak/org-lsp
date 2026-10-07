//! M-RET and its neighbours: `org-meta-return` (in a table, the field
//! split into the next row; in a list, a new item; else a new heading),
//! `org-insert-todo-heading` (M-S-RET: an item with a checkbox, or a
//! heading with a TODO keyword), and their `respect-content` forms
//! (C-RET, C-S-RET: a heading after the subtree). Empty lines before the
//! new heading follow `org-blank-before-new-entry`'s `auto`: as before
//! the heading the cursor is in. Checked against Emacs in
//! `tests/meta-return`.

use std::sync::LazyLock;

use org_syntax::{SyntaxKind::TABLE, TodoKeywords};
use regex::Regex;

use crate::date::{Date, DateTime};
use crate::format;
use crate::list;
use crate::newline::{ancestors_at, is_table_line, next_row, Outcome};
use crate::text::tag_group;
use crate::todo::{self, headline_level, tags_padding, Editor};
use crate::Refusal;

pub struct Settings {
    /// `org-tags-column`.
    pub tags_column: i64,
    /// The new heading, item or table row goes below the line of the
    /// cursor, as nvim-orgmode's `<Leader><CR>` puts it: as Emacs does
    /// from the end of the line with `org-M-RET-may-split-line` nil, a
    /// table row with `org-table-insert-row`'s argument, and an item after
    /// the item even where its bullet has nothing after it. Nothing is
    /// split, and nothing goes above.
    pub below: bool,
}

/// Text and a point, edited as Emacs edits a buffer.
struct Buffer {
    text: String,
    point: usize,
}

impl Buffer {
    fn bol(&self) -> usize {
        line_start(&self.text, self.point)
    }

    fn eol(&self) -> usize {
        line_end(&self.text, self.point)
    }

    /// `insert`: the point after what is inserted.
    fn insert(&mut self, what: &str) {
        self.text.insert_str(self.point, what);
        self.point += what.len();
    }

    /// An insertion at the point inside `save-excursion`: the point stays.
    fn insert_before(&mut self, what: &str) {
        self.text.insert_str(self.point, what);
    }

    fn at_heading(&self) -> bool {
        headline_level(line(&self.text, self.bol())).is_some()
    }
}

fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |newline| newline + 1)
}

fn line_end(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |newline| at + newline)
}

fn line(text: &str, start: usize) -> &str {
    &text[start..line_end(text, start)]
}

fn is_blank(line: &str) -> bool {
    line.bytes().all(|b| b == b' ' || b == b'\t')
}

/// The heading the line at `at` belongs to (`org-back-to-heading`).
fn heading_at_or_above(text: &str, at: usize) -> Option<usize> {
    let mut start = line_start(text, at);
    loop {
        if headline_level(line(text, start)).is_some() {
            return Some(start);
        }
        if start == 0 {
            return None;
        }
        start = line_start(text, start - 1);
    }
}

/// `outline-next-heading`: the next heading after `at`, not at it.
fn next_heading(text: &str, at: usize) -> Option<usize> {
    let mut start = line_end(text, at);
    while start < text.len() {
        start += 1;
        if headline_level(line(text, start)).is_some() {
            return Some(start);
        }
        start = line_end(text, start);
    }
    None
}

/// `org-previous-line-empty-p` at `at`.
fn previous_line_empty(text: &str, at: usize) -> bool {
    if at == 0 {
        return false;
    }
    let start = line_start(text, at);
    let previous = if start == 0 { 0 } else { line_start(text, start - 1) };
    is_blank(line(text, previous))
}

/// `org--blank-before-heading-p`, with `org-blank-before-new-entry` at
/// `auto` for headings.
fn blank_before_heading(text: &str, at: usize) -> bool {
    let heading = match heading_at_or_above(text, at) {
        Some(heading) => heading,
        None => match next_heading(text, at) {
            Some(heading) => heading,
            None => return false,
        },
    };
    if heading != 0 {
        return previous_line_empty(text, heading);
    }
    if let Some(next) = next_heading(text, heading) {
        return previous_line_empty(text, next);
    }
    let end = text.trim_end_matches([' ', '\t']).len();
    if line_start(text, end) == end {
        return previous_line_empty(text, end);
    }
    false
}

/// `org-N-empty-lines-before-current`
fn empty_lines_before(buffer: &mut Buffer, count: usize) {
    let column = buffer.point - buffer.bol();
    buffer.point = buffer.bol();
    if buffer.point > 0 {
        let back = buffer.text[..buffer.point].trim_end_matches([' ', '\r', '\t', '\n']).len();
        let start = line_end(&buffer.text, back);
        let previous_end = buffer.point - 1;
        if start < previous_end {
            buffer.text.replace_range(start..previous_end, "");
            buffer.point -= previous_end - start;
        }
    }
    buffer.insert(&"\n".repeat(count));
    let eol = buffer.eol();
    buffer.point = (buffer.point + column).min(eol);
}

/// The `maybe-add-blank-after` of `org-insert-heading`: an empty line
/// kept before the heading after the new one.
fn add_blank_after(buffer: &mut Buffer, blank: bool) {
    let eol = buffer.eol();
    if eol < buffer.text.len() && blank && headline_level(line(&buffer.text, eol + 1)).is_some() {
        buffer.text.insert(eol + 1, '\n');
    }
}

/// Group 4 of `org-complex-heading-regexp`: the title of the headline
/// `line`, without the keyword, the priority and the tags.
fn title_range(line: &str, keywords: &TodoKeywords) -> Option<(usize, usize)> {
    let stars = headline_level(line)?;
    let skip_blanks = |at: usize| at + line[at..].len() - line[at..].trim_start_matches(' ').len();
    let mut start = skip_blanks(stars);
    let word = line[start..].split([' ', '\t']).next().unwrap_or("");
    if !word.is_empty() && keywords.contains(word) && start > stars {
        let after = start + word.len();
        if after == line.len() || line[after..].starts_with([' ', '\t']) {
            start = skip_blanks(after);
        }
    }
    let rest = &line[start..];
    if rest.len() >= 4 && rest.starts_with("[#") && rest.as_bytes()[3] == b']' && rest.is_char_boundary(4) {
        let after = start + 4;
        if after == line.len() || line[after..].starts_with(' ') {
            start = skip_blanks(after);
        }
    }
    let end = match tag_group(line) {
        Some((blanks, _)) => blanks,
        None => line.len(),
    };
    let end = line[..end.max(start)].trim_end_matches([' ', '\t']).len();
    (end > start).then_some((start, end))
}

/// `org-insert-heading`, without the `C-u C-u` form. `respect_content`
/// puts the heading after the subtree.
fn insert_heading(buffer: &mut Buffer, keywords: &TodoKeywords, settings: &Settings, respect_content: bool) {
    let blank = blank_before_heading(&buffer.text, buffer.point);
    let current_heading = heading_at_or_above(&buffer.text, buffer.point);
    let level = current_heading.and_then(|heading| headline_level(line(&buffer.text, heading)));
    let stars = "*".repeat(level.unwrap_or(1));
    if respect_content {
        buffer.point = match current_heading {
            None => next_heading(&buffer.text, buffer.point).unwrap_or(buffer.text.len()),
            Some(heading) => subtree_end(&buffer.text, heading, level.unwrap_or(1)),
        };
        if buffer.point != buffer.bol() {
            buffer.insert("\n");
        }
        if blank && buffer.point > 0 && heading_at_or_above(&buffer.text, buffer.point - 1).is_none() {
            buffer.insert("\n");
            buffer.point -= 1;
        }
        if level.is_none() && buffer.point < buffer.text.len() && buffer.point > 0 {
            if buffer.at_heading() {
                buffer.insert("\n");
            }
            buffer.point -= 1;
        }
        if !(blank && previous_line_empty(&buffer.text, buffer.point)) {
            empty_lines_before(buffer, usize::from(blank));
        }
        buffer.insert(&format!("{stars} \n"));
        buffer.point -= 1;
        add_blank_after(buffer, blank);
        return;
    }
    if buffer.at_heading() {
        let bol = buffer.bol();
        if buffer.point == bol {
            if blank {
                buffer.insert_before("\n");
            }
            buffer.insert_before(&format!("{stars} \n"));
            if !(blank && previous_line_empty(&buffer.text, buffer.point)) {
                empty_lines_before(buffer, usize::from(blank));
            }
            buffer.point = buffer.eol();
            return;
        }
        let title = title_range(line(&buffer.text, bol), keywords).filter(|_| !settings.below);
        if let Some((_, end)) = title.filter(|(start, end)| (bol + start..=bol + end).contains(&buffer.point)) {
            let split = buffer.text[buffer.point..bol + end].to_string();
            buffer.text.replace_range(buffer.point..bol + end, "");
            let eol = buffer.eol();
            if is_blank(&buffer.text[buffer.point..eol]) {
                buffer.text.replace_range(buffer.point..eol, "");
            } else if let Some((blanks, tags, padding)) = tags_padding(line(&buffer.text, bol), settings.tags_column) {
                buffer.text.replace_range(bol + blanks..bol + tags, &padding);
            }
            buffer.point = buffer.eol();
            if blank {
                buffer.insert("\n");
            }
            buffer.insert(&format!("\n{stars} "));
            add_blank_after(buffer, blank);
            if !split.trim().is_empty() {
                buffer.insert(&split);
            }
            return;
        }
        buffer.point = buffer.eol();
        if blank {
            buffer.insert("\n");
        }
        buffer.insert(&format!("\n{stars} "));
        add_blank_after(buffer, blank);
        return;
    }
    if buffer.point == buffer.bol() {
        buffer.insert(&format!("{stars} "));
    } else {
        buffer.insert(&format!("\n{stars} "));
    }
    if !(blank && previous_line_empty(&buffer.text, buffer.point)) {
        empty_lines_before(buffer, usize::from(blank));
    }
    add_blank_after(buffer, blank);
}

/// `org-end-of-subtree` with `to-heading`: the start of the next heading
/// at `level` or above, or the end of the text.
fn subtree_end(text: &str, heading: usize, level: usize) -> usize {
    let mut at = heading;
    while let Some(next) = next_heading(text, at) {
        if headline_level(line(text, next)).is_some_and(|next_level| next_level <= level) {
            return next;
        }
        at = next;
    }
    text.len()
}

/// The keyword `org-insert-todo-heading` gives the new heading at
/// `heading`: the one of the previous heading at its level unless that one
/// is done or has none, then the first keyword.
fn new_mark<'k>(text: &str, heading: usize, keywords: &'k TodoKeywords) -> Option<&'k str> {
    let level = headline_level(line(text, heading))?;
    let mut sibling = heading;
    let mut at = heading;
    while at > 0 {
        at = line_start(text, at - 1);
        if let Some(found) = headline_level(line(text, at)) {
            if found < level {
                break;
            }
            if found == level {
                sibling = at;
                break;
            }
        }
    }
    let line = line(text, sibling);
    let stars = headline_level(line)?;
    let rest = &line[stars..];
    let word = rest.trim_start_matches(' ');
    let word = if word.len() < rest.len() { word.split([' ', '\t']).next().unwrap_or("") } else { "" };
    let first = keywords.names().next();
    match keywords.names().find(|keyword| *keyword == word) {
        Some(keyword) if !keywords.is_done(keyword) => Some(keyword),
        _ => first,
    }
}

/// `org-insert-todo-heading` once the heading is in: the keyword after its
/// stars, and the TODO statistics of its parent.
fn mark_todo(buffer: &mut Buffer, keywords: &TodoKeywords, settings: &Settings) {
    let heading = buffer.bol();
    // `forward-line 0`: a line that is no heading gets no keyword, and the
    // cursor is at its start.
    buffer.point = heading;
    let Some(mark) = new_mark(&buffer.text, heading, keywords) else { return };
    let Some(stars) = headline_level(line(&buffer.text, heading)) else { return };
    buffer.point = heading + stars + 1;
    buffer.insert(&format!("{mark} "));
    let todo_settings = todo::Settings { tags_column: settings.tags_column, ..todo::Settings::default() };
    let mut editor = Editor::new(&buffer.text, keywords, &todo_settings, DateTime::new(Date::new(1970, 1, 1), 0, 0));
    editor.heading = heading;
    editor.update_parent_statistics();
    let point = follow(&buffer.text, &editor.text, buffer.point);
    buffer.text = editor.text;
    buffer.point = point;
}

/// Where `at` of `old` is in `new`, when what changed is one stretch.
fn follow(old: &str, new: &str, at: usize) -> usize {
    let (prefix, suffix) = crate::text::common_ends(old, new);
    if at <= prefix {
        return at;
    }
    if at >= old.len() - suffix {
        at + new.len() - old.len()
    } else {
        prefix
    }
}

/// Where the command acts: below the line, from its end.
fn place(text: &str, at: usize, settings: &Settings) -> usize {
    if settings.below {
        line_end(text, at)
    } else {
        at
    }
}

fn at_table(text: &str, at: usize, keywords: &TodoKeywords) -> bool {
    let start = line_start(text, at);
    if !is_table_line(line(text, start)) {
        return false;
    }
    let root = org_syntax::parse(text, keywords).syntax();
    ancestors_at(&root, at).iter().any(|node| node.kind() == TABLE)
}

/// M-RET (`org-meta-return`) at byte `at` of `text`.
pub fn meta_return(text: &str, at: usize, keywords: &TodoKeywords, settings: &Settings) -> Result<Outcome, Refusal> {
    let at = place(text, at, settings);
    // From the end of the line, the table is the line's.
    if at_table(text, if settings.below { line_start(text, at) } else { at }, keywords) {
        return if settings.below { Ok(insert_row_below(text, at)) } else { wrap_field(text, at) };
    }
    if let Some(inserted) = list::insert_item(text, at, false, settings.below, keywords, settings.tags_column)? {
        return Ok(Outcome { text: inserted.text, cursor: inserted.cursor });
    }
    let mut buffer = Buffer { text: text.to_string(), point: at };
    insert_heading(&mut buffer, keywords, settings, false);
    Ok(Outcome { text: buffer.text, cursor: buffer.point })
}

/// M-S-RET (`org-insert-todo-heading`): in a list, a new item with a
/// checkbox; else a new heading with a TODO keyword. With
/// `respect_content` (C-S-RET), always a heading, after the subtree.
pub fn insert_todo_heading(
    text: &str,
    at: usize,
    keywords: &TodoKeywords,
    settings: &Settings,
    respect_content: bool,
) -> Result<Outcome, Refusal> {
    let at = place(text, at, settings);
    if !respect_content {
        if let Some(inserted) = list::insert_item(text, at, true, settings.below, keywords, settings.tags_column)? {
            return Ok(Outcome { text: inserted.text, cursor: inserted.cursor });
        }
    }
    let mut buffer = Buffer { text: text.to_string(), point: at };
    insert_heading(&mut buffer, keywords, settings, respect_content);
    mark_todo(&mut buffer, keywords, settings);
    Ok(Outcome { text: buffer.text, cursor: buffer.point })
}

/// C-RET (`org-insert-heading-respect-content`): a heading after the
/// subtree.
pub fn insert_heading_after(text: &str, at: usize, keywords: &TodoKeywords, settings: &Settings) -> Outcome {
    let mut buffer = Buffer { text: text.to_string(), point: at };
    insert_heading(&mut buffer, keywords, settings, true);
    Outcome { text: buffer.text, cursor: buffer.point }
}

/// `org-table-wrap-region` without a region: the field split at the
/// cursor, what follows it put in front of the field one row down.
fn wrap_field(text: &str, at: usize) -> Result<Outcome, Refusal> {
    let start = line_start(text, at);
    let end = line_end(text, at);
    let current = &text[start..end];
    if is_blank(&text[start..at]) || current.trim_start().starts_with("|-") || is_blank(&text[at..end]) {
        return Err(Refusal::Failed("Not in table data field".to_string()));
    }
    let Some(bar) = text[at..].find('|').filter(|bar| *bar > 0) else {
        // `org-table-next-row` aligns the table first: its last row ends
        // with a newline then.
        let mut outcome = next_row(text, at);
        if !outcome.text.ends_with('\n')
            && is_table_line(line(&outcome.text, line_start(&outcome.text, outcome.text.len())))
        {
            outcome.text.push('\n');
        }
        return Ok(outcome);
    };
    let field = text[at..at + bar].trim_matches([' ', '\t', '\n', '\r']).to_string();
    let cut = format!("{} |{}", &text[..at], &text[at + bar + 1..]);
    let moved = next_row(&cut, at);
    let mut out = moved.text;
    out.insert_str(moved.cursor, &format!("{field} "));
    Ok(align(&out, moved.cursor))
}

/// `org-table-insert-row` with its argument, from the end of the row at
/// `at`: a row of blanks like it below it, the cursor in its first field,
/// and the formulas after the table that name the rows after it by number
/// (`@3`) moved along with them (`org-table-fix-formulas`).
fn insert_row_below(text: &str, at: usize) -> Outcome {
    static END_BAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\|[ \t]*$").expect("valid"));
    static MARKER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[ \t]*\| *[#*$] *\|").expect("valid"));
    let mut text = text.to_string();
    let mut at = at;
    if at == text.len() {
        text.push('\n');
    }
    if !END_BAR.is_match(line(&text, line_start(&text, at))) {
        let aligned = align(&text, at);
        text = aligned.text;
        at = aligned.cursor;
    }
    let start = line_start(&text, at);
    let end = line_end(&text, start);
    let current = text[start..end].to_string();
    let mut new = clean_line(&current);
    // Fix the first field if necessary.
    if let Some(marker) = MARKER.find(&current) {
        if new.is_char_boundary(marker.end()) {
            new.replace_range(..marker.end(), marker.as_str());
        }
    }
    let row = (end + 1).min(text.len());
    text.insert_str(row, &format!("{new}\n"));
    let bar = new.find('|').map_or(0, |bar| bar + 1);
    let cursor = row + bar + usize::from(new[bar..].starts_with(' '));
    // The data lines from the table's start to the new row.
    let mut first = row;
    while first > 0 {
        let previous = line_start(&text, first - 1);
        if !is_table_line(line(&text, previous)) {
            break;
        }
        first = previous;
    }
    let dline = text[first..row].split_terminator('\n').filter(|line| is_data_line(line)).count() + 1;
    let text = fix_formulas(&text, row, dline - 1);
    Outcome { text, cursor }
}

/// `org-table-dataline-regexp`
fn is_data_line(line: &str) -> bool {
    let rest = line.trim_start_matches([' ', '\t']);
    rest.starts_with('|') && rest[1..].chars().next().is_some_and(|c| c != '-' && c != '\n')
}

/// `org-table-clean-line`: the row with only bars and blanks, as wide as
/// it was.
fn clean_line(line: &str) -> String {
    static FIELD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\|([ \t]*?[^ \t\r\n|][^\r\n|]*)\|").expect("valid"));
    if line.trim_start_matches([' ', '\t']).starts_with("|-") {
        return line.chars().map(|c| if c == '|' || c == '+' { '|' } else { ' ' }).collect();
    }
    let mut line = line.to_string();
    while let Some(field) = FIELD.captures(&line).and_then(|found| found.get(1)).map(|field| field.range()) {
        let blanks = " ".repeat(crate::text::width(&line[field.clone()]));
        line.replace_range(field, &blanks);
    }
    line
}

/// `org-table-fix-formulas` for `@`: in the `#+TBLFM` lines after the table
/// that the row at `row` is in, the row numbers above `limit` one more.
fn fix_formulas(text: &str, row: usize, limit: usize) -> String {
    static TBLFM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^[ \t]*#\+tblfm:").expect("valid"));
    static ROW: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"@([0-9]+)").expect("valid"));
    static REMOTE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"remote\([^)]+?\)").expect("valid"));
    let mut at = row;
    while at < text.len() && is_table_line(line(text, at)) {
        at = (line_end(text, at) + 1).min(text.len());
    }
    let mut out = text[..at].to_string();
    while at < text.len() && TBLFM.is_match(line(text, at)) {
        let current = line(text, at);
        let remote: Vec<_> = REMOTE.find_iter(current).map(|found| found.range()).collect();
        let fixed = ROW.replace_all(current, |found: &regex::Captures| {
            let whole = found.get(0).expect("a match");
            let number = found[1].parse::<usize>().unwrap_or(0);
            if number > limit && !remote.iter().any(|span| span.start <= whole.start() && whole.start() < span.end) {
                format!("@{}", number + 1)
            } else {
                whole.as_str().to_string()
            }
        });
        out.push_str(&fixed);
        at += current.len();
        if at < text.len() {
            out.push('\n');
            at += 1;
        }
    }
    out.push_str(&text[at..]);
    out
}

/// `org-table-align` on the table at `at`, the cursor at the start of its
/// field after (`org-table-save-field`).
fn align(text: &str, at: usize) -> Outcome {
    let start = line_start(text, at);
    let mut first = start;
    while first > 0 {
        let previous = line_start(text, first - 1);
        if !is_table_line(&text[previous..first - 1]) {
            break;
        }
        first = previous;
    }
    let mut last_end = line_end(text, at);
    while last_end < text.len() {
        let next_end = line_end(text, last_end + 1);
        if !is_table_line(&text[last_end + 1..next_end]) {
            break;
        }
        last_end = next_end;
    }
    let lines: Vec<&str> = text[first..last_end].split('\n').collect();
    let row = text[first..start].matches('\n').count();
    let column = text[start..at].matches('|').count();
    let aligned = format::align_table(&lines);
    let row_start = first + aligned[..row].iter().map(|line| line.len() + 1).sum::<usize>();
    let target = &aligned[row];
    let bars: Vec<usize> = target.match_indices('|').map(|(bar, _)| bar + 1).collect();
    let mut cursor = if column == 0 { 0 } else { bars.get(column - 1).or(bars.last()).copied().unwrap_or(0) };
    if target[cursor..].starts_with(' ') {
        cursor += 1;
    }
    let mut out = format!("{}{}{}", &text[..first], aligned.join("\n"), &text[last_end..]);
    // Each aligned row ends with a newline, the last one too.
    if last_end == text.len() {
        out.push('\n');
    }
    Outcome { text: out, cursor: row_start + cursor }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// From the keys of modal editors the new item goes below even an item
    /// with nothing after its bullet, which Emacs puts it before.
    #[test]
    fn below_an_empty_item() {
        let keywords = TodoKeywords::default();
        let settings = Settings { tags_column: -77, below: true };
        for (text, at, expected, cursor) in
            [("1. \n2. two\n", 0, "1. \n2. \n3. two\n", 7), ("- [ ] \n", 2, "- [ ] \n- [ ] \n", 13)]
        {
            let outcome = if text.contains('[') {
                insert_todo_heading(text, at, &keywords, &settings, false)
            } else {
                meta_return(text, at, &keywords, &settings)
            }
            .unwrap();
            assert_eq!((outcome.text.as_str(), outcome.cursor), (expected, cursor), "{text:?}");
        }
    }
}
