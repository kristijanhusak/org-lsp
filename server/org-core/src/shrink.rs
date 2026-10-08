//! Shrunk table columns (`org-table-shrink`,
//! `org-table-toggle-column-width`): what each field of a shrunk column
//! shows instead of its text, as Org's overlays do (`org-table--shrink-field`).

use std::collections::BTreeSet;

/// `org-table-shrunk-column-indicator`
pub const INDICATOR: &str = "…";

/// Text shown in place of `start..end` (byte offsets in the document).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overlay {
    pub start: usize,
    pub end: usize,
    pub display: String,
}

/// A table of the document: where its lines are (byte offsets, without the
/// line ends), from its first line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub lines: Vec<(usize, usize)>,
}

impl Table {
    /// The line the table starts on, from 0.
    pub fn first_line(&self, text: &str) -> usize {
        text[..self.lines[0].0].matches('\n').count()
    }

    /// The table's lines' range of lines, from 0, the last one included.
    pub fn line_range(&self, text: &str) -> (usize, usize) {
        let first = self.first_line(text);
        (first, first + self.lines.len() - 1)
    }
}

/// The Org tables of `text`: runs of lines that start with `|` after
/// blanks.
pub fn tables(text: &str) -> Vec<Table> {
    let mut out: Vec<Table> = Vec::new();
    let mut previous_in_table = false;
    let mut start = 0;
    for line in text.split_inclusive('\n') {
        let body = line.strip_suffix('\n').unwrap_or(line);
        let body = body.strip_suffix('\r').unwrap_or(body);
        let in_table = body.trim_start_matches([' ', '\t']).starts_with('|');
        if in_table {
            let range = (start, start + body.len());
            match out.last_mut() {
                Some(table) if previous_in_table => table.lines.push(range),
                _ => out.push(Table { lines: vec![range] }),
            }
        }
        previous_in_table = in_table;
        start += line.len();
    }
    out
}

/// The fields of a table line: their byte ranges between the separators,
/// and whether the line is a rule (`|---+---|`).
fn fields(text: &str, line: (usize, usize)) -> (Vec<(usize, usize)>, bool) {
    let body = &text[line.0..line.1];
    let indent = body.len() - body.trim_start_matches([' ', '\t']).len();
    let rule = body[indent..].starts_with("|-");
    let mut out = Vec::new();
    let mut field_start = line.0 + indent + 1;
    for (at, c) in body.char_indices().skip_while(|(at, _)| *at <= indent) {
        if c == '|' || (rule && c == '+') {
            out.push((field_start, line.0 + at));
            field_start = line.0 + at + 1;
        }
    }
    // A last field without its closing `|`.
    if field_start < line.1 && !body[field_start - line.0..].trim().is_empty() {
        out.push((field_start, line.1));
    }
    (out, rule)
}

/// The width cookies of a table (`<10>`, `<l10>`, `<r>`): by column, from
/// 1, the width (none for an alignment alone) and the alignment.
pub fn cookies(text: &str, table: &Table) -> Vec<(usize, Option<usize>, Option<char>)> {
    let mut out: Vec<(usize, Option<usize>, Option<char>)> = Vec::new();
    for line in &table.lines {
        let (fields, rule) = fields(text, *line);
        if rule {
            continue;
        }
        for (index, (start, end)) in fields.into_iter().enumerate() {
            let field = text[start..end].trim();
            let Some(inner) = field.strip_prefix('<').and_then(|rest| rest.strip_suffix('>')) else { continue };
            let (align, digits) = match inner.chars().next() {
                Some(c @ ('l' | 'r' | 'c')) => (Some(c), &inner[1..]),
                _ => (None, inner),
            };
            if !digits.chars().all(|c| c.is_ascii_digit()) || (align.is_none() && digits.is_empty()) {
                continue;
            }
            let width = digits.parse().ok();
            if !out.iter().any(|(column, _, _)| *column == index + 1) {
                out.push((index + 1, width, align));
            }
        }
    }
    out
}

/// Whether a field is a number, as `org-table-number-regexp` takes it.
fn is_number(field: &str) -> bool {
    let field = field.trim();
    let body = field.strip_prefix(['-', '+']).unwrap_or(field);
    !body.is_empty()
        && body.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | ' ' | '%' | 'e' | 'E'))
        && body.chars().any(|c| c.is_ascii_digit())
}

/// The alignment of a column: its cookie's, else right when most of its
/// fields are numbers (`org-table-number-fraction`), else left.
fn alignment(text: &str, table: &Table, column: usize, cookie: Option<char>) -> char {
    if let Some(align) = cookie {
        return align;
    }
    let (mut numbers, mut filled) = (0, 0);
    for line in &table.lines {
        let (fields, rule) = fields(text, *line);
        if rule {
            continue;
        }
        if let Some((start, end)) = fields.get(column - 1) {
            let field = text[*start..*end].trim();
            if !field.is_empty() && !(field.starts_with('<') && field.ends_with('>')) {
                filled += 1;
                numbers += usize::from(is_number(field));
            }
        }
    }
    if filled > 0 && numbers * 2 >= filled { 'r' } else { 'l' }
}

/// The overlays of the column `column` (from 1) of `table` shrunk to
/// `width` characters (0: the indicator alone), as
/// `org-table--shrink-columns` makes them.
pub fn shrink_column(text: &str, table: &Table, column: usize, width: usize) -> Vec<Overlay> {
    let cookie = cookies(text, table).into_iter().find(|(c, _, _)| *c == column).and_then(|(_, _, align)| align);
    let align = alignment(text, table, column, cookie);
    let mut out = Vec::new();
    for line in &table.lines {
        let (fields, rule) = fields(text, *line);
        if let Some((start, end)) = fields.get(column - 1) {
            out.extend(shrink_field(text, width, align, *start, *end, rule));
        }
    }
    out
}

/// `org-table--shrink-field`: what shows of the field `start..end`.
fn shrink_field(text: &str, width: usize, align: char, start: usize, end: usize, rule: bool) -> Vec<Overlay> {
    let overlay = |start: usize, end: usize, display: String, pre: bool| Overlay {
        start,
        end,
        display: if pre { format!(" {display}") } else { format!("{display}{INDICATOR}") },
    };
    let field = &text[start..end];
    if start == end {
        return Vec::new();
    }
    if width == 0 {
        return vec![overlay(start, end, String::new(), false)];
    }
    if rule {
        return vec![overlay(start, end, "-".repeat(width + 1), false)];
    }
    let contents = field.trim_matches(' ');
    if contents.is_empty() {
        let w = crate::text::width(field);
        let full = width + 2;
        return vec![if w <= full {
            overlay(end - 1, end, " ".repeat(full - w), false)
        } else {
            overlay(end - (w - full) - 1, end, String::new(), false)
        }];
    }
    let lead = field.len() - field.trim_start_matches(' ').len();
    let trail = field.len() - field.trim_end_matches(' ').len();
    let contents_width = crate::text::width(contents);
    let mut out = Vec::new();
    if width <= contents_width {
        // Too wide: one blank before it at most, cut where `width`
        // characters show.
        if lead > 0 {
            out.push(overlay(start, start + lead, String::new(), true));
        }
        let begin = start + lead;
        let wanted = if lead == 0 { width + 1 } else { width };
        let mut cut = end - 1;
        let mut shown = 0;
        for (at, c) in text[begin..end].char_indices() {
            if shown >= wanted {
                cut = begin + at;
                break;
            }
            shown += crate::text::char_width(c);
        }
        out.push(overlay(cut.min(end - 1).max(begin), end, String::new(), false));
        return out;
    }
    // Narrow enough: blanks added or taken on each side, as aligned.
    let required = (width - contents_width) as isize;
    let before: isize = if lead == 0 {
        -1
    } else {
        match align {
            'r' => required,
            'c' => required / 2,
            _ => 0,
        }
    };
    let after = required - before;
    let (lead, trail) = (lead as isize, trail as isize);
    if lead != 0 && lead - 1 != before {
        if lead - 1 > before {
            out.push(overlay(start, start + (lead - before) as usize, String::new(), true));
        } else {
            out.push(overlay(start, start + 1, " ".repeat((before - (lead - 1)) as usize), true));
        }
    }
    let end_i = end as isize;
    out.push(if trail - 1 == after {
        overlay(end - 1, end, String::new(), false)
    } else if trail - 1 > after {
        overlay((after + (end_i - trail)) as usize, end, String::new(), false)
    } else {
        overlay(end - 1, end, " ".repeat((after - (trail - 1)) as usize), false)
    });
    out
}

/// The columns of a table Org shrinks when it shrinks it whole
/// (`org-table-shrink`): those with a width cookie.
pub fn cookie_columns(text: &str, table: &Table) -> BTreeSet<usize> {
    cookies(text, table).into_iter().filter(|(_, width, _)| width.is_some()).map(|(column, _, _)| column).collect()
}

/// What a shrunk column shows: as wide as its cookie says, or the
/// indicator alone.
pub fn shrunk_width(text: &str, table: &Table, column: usize) -> usize {
    cookies(text, table).into_iter().find(|(c, _, _)| *c == column).and_then(|(_, width, _)| width).unwrap_or(0)
}

/// The overlays of every shrunk column of `table`.
pub fn overlays(text: &str, table: &Table, columns: &BTreeSet<usize>) -> Vec<Overlay> {
    let mut out: Vec<Overlay> =
        columns.iter().flat_map(|column| shrink_column(text, table, *column, shrunk_width(text, table, *column))).collect();
    out.sort_by_key(|overlay| overlay.start);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The overlays of `columns` of the first table, as `(start, end, display)`.
    fn shown(text: &str, columns: &[usize]) -> Vec<(usize, usize, String)> {
        let table = &tables(text)[0];
        overlays(text, table, &columns.iter().copied().collect())
            .into_iter()
            .map(|overlay| (overlay.start, overlay.end, overlay.display))
            .collect()
    }

    fn expected(list: &[(usize, usize, &str)]) -> Vec<(usize, usize, String)> {
        list.iter().map(|(start, end, display)| (*start, *end, display.to_string())).collect()
    }

    // Each case is what Emacs's overlays are (`org-table--shrink-columns`).
    #[test]
    fn shrinks_as_org_does() {
        let text = "| <3>  | b |\n|------+---|\n| abcdef | x |\n| ab   | y |\n|      | z |\n";
        assert_eq!(
            shown(text, &[1]),
            expected(&[
                (1, 2, " "),
                (5, 7, "…"),
                (14, 20, "----…"),
                (27, 28, " "),
                (31, 35, "…"),
                (46, 48, "…"),
                (59, 61, "…"),
            ])
        );
        // Without a cookie: the indicator alone.
        assert_eq!(shown("| a | 12 |\n| bb | 3 |\n", &[2]), expected(&[(5, 9, "…"), (17, 20, "…")]));
        // Right aligned.
        assert_eq!(
            shown("| <r4> | b |\n| 12 | x |\n| 1234567 | y |\n", &[1]),
            expected(&[(1, 2, " "), (6, 7, "…"), (14, 15, "   "), (17, 18, "…"), (25, 26, " "), (30, 34, "…")])
        );
        assert_eq!(shown("| a | b |\n|---+---|\n| cc | d |\n", &[1]), expected(&[(1, 4, "…"), (11, 14, "…"), (21, 25, "…")]));
    }

    #[test]
    fn finds_tables_and_their_cookie_columns() {
        let text = "text\n| <5> | a |\n| x | y |\n\n| b |\n";
        let found = tables(text);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].line_range(text), (1, 2));
        assert_eq!(cookie_columns(text, &found[0]), [1].into_iter().collect());
        assert!(cookie_columns(text, &found[1]).is_empty());
    }
}
