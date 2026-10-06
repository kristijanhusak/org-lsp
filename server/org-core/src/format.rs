//! Formatting a whole document as Org's own commands would: every table
//! aligned (`org-table-align`) and the tags of every headline in their
//! column (`(org-align-tags t)`). Checked against Emacs in `tests/format`.

use std::sync::LazyLock;

use org_syntax::{SyntaxKind::*, TodoKeywords};
use regex::Regex;

use crate::text::width;
use crate::todo::{headline_level, tags_padding};

pub struct Settings {
    /// `org-tags-column`.
    pub tags_column: i64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { tags_column: -77 }
    }
}

/// The lines that change, as `(line number, new text)`. Formatting never
/// adds or removes lines.
pub fn format(text: &str, keywords: &TodoKeywords, settings: &Settings) -> Vec<(usize, String)> {
    format_lines(text, keywords, settings, 0..=usize::MAX)
}

/// `format` for the lines `lines`: the tables with a line among them, whole,
/// and the tags of the headlines among them.
pub fn format_lines(
    text: &str,
    keywords: &TodoKeywords,
    settings: &Settings,
    lines_asked: std::ops::RangeInclusive<usize>,
) -> Vec<(usize, String)> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut changed: Vec<(usize, String)> = Vec::new();
    let root = org_syntax::parse(text, keywords).syntax();
    let starts = line_starts(text);
    // Org takes the lines that start with `|` around the cursor as the
    // table, whatever their indentation: a table the parser sees as two
    // (in and after a list) is aligned as one.
    let mut done = 0;
    for table in root.descendants().filter(|node| node.kind() == TABLE) {
        let first = line_of(&starts, usize::from(table.text_range().start()));
        // A table.el table is left as it is.
        if first < done || !lines[first].trim_start_matches([' ', '\t']).starts_with('|') {
            continue;
        }
        let mut last = first;
        while last + 1 < lines.len() && lines[last + 1].trim_start_matches([' ', '\t']).starts_with('|') {
            last += 1;
        }
        done = last + 1;
        if last < *lines_asked.start() || first > *lines_asked.end() {
            continue;
        }
        for (offset, new) in align_table(&lines[first..=last]).into_iter().enumerate() {
            if new != lines[first + offset] {
                changed.push((first + offset, new));
            }
        }
    }
    for (number, line) in lines.iter().enumerate() {
        if lines_asked.contains(&number) && headline_level(line).is_some() {
            if let Some((blanks, tags, padding)) = tags_padding(line, settings.tags_column) {
                changed.push((number, format!("{}{}{}", &line[..blanks], padding, &line[tags..])));
            }
        }
    }
    changed.sort_by_key(|(number, _)| *number);
    changed
}

/// Apply what `format` returns.
pub fn apply(text: &str, changes: &[(usize, String)]) -> String {
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    for (number, new) in changes {
        lines[*number] = new.clone();
    }
    lines.join("\n")
}

fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0).chain(text.match_indices('\n').map(|(at, _)| at + 1)).collect()
}

fn line_of(starts: &[usize], offset: usize) -> usize {
    starts.partition_point(|start| *start <= offset).saturating_sub(1)
}

/// A row of `org-table-to-lisp`.
enum Row {
    Rule,
    Cells(Vec<String>),
}

/// `org-table-to-lisp` on one line: after the first `|`, a rule if `-`
/// follows, else the fields between the bars, trimmed.
fn row(line: &str) -> Row {
    let rest = &line.trim_start_matches([' ', '\t'])[1..];
    if rest.starts_with('-') {
        return Row::Rule;
    }
    let mut cells = Vec::new();
    let mut rest = rest;
    loop {
        rest = rest.trim_start_matches([' ', '\t']);
        if rest.is_empty() {
            break;
        }
        let end = rest.find('|').unwrap_or(rest.len());
        cells.push(rest[..end].trim_end_matches([' ', '\t']).to_string());
        rest = if end < rest.len() { &rest[end + 1..] } else { "" };
    }
    Row::Cells(cells)
}

/// `org-table-number-regexp`, matched as Emacs matches it: ignoring case.
static NUMBER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^(?:[<>]?[-+^.0-9]*[0-9][-+^.0-9eEdDx()%:]*|[<>]?[-+]?0[xX][0-9a-f.]+|[<>]?[-+]?[0-9]+#[0-9a-zA-Z.]+|nan|[-+u]?inf)$",
    )
    .expect("a valid regex")
});

static COOKIE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^<([lrc])[0-9]*>$").expect("a valid regex"));

/// The width of a field as Org shows it: a link shows its description, or
/// its target when it has none (`org-link-descriptive`).
fn shown_width(cell: &str) -> usize {
    let mut total = 0;
    let mut rest = cell;
    while let Some(open) = rest.find("[[") {
        let Some(close) = rest[open + 2..].find("]]").map(|at| open + 2 + at) else { break };
        total += width(&rest[..open]);
        let inside = &rest[open + 2..close];
        total += width(inside.split_once("][").map_or(inside, |(_, description)| description));
        rest = &rest[close + 2..];
    }
    total + width(rest)
}

/// `org-table--align-field`.
fn align_field(field: &str, width: usize, align: char) -> String {
    let spaces = width.saturating_sub(shown_width(field));
    let prefix = match align {
        'r' => spaces,
        'c' => spaces / 2,
        _ => 0,
    };
    format!(" {}{}{} ", " ".repeat(prefix), field, " ".repeat(spaces - prefix))
}

/// `org-table-align`: the lines of a table, aligned.
fn align_table(lines: &[&str]) -> Vec<String> {
    let table: Vec<Row> = lines.iter().map(|line| row(line)).collect();
    let rows: Vec<&Vec<String>> = table
        .iter()
        .filter_map(|row| match row {
            Row::Cells(cells) => Some(cells),
            Row::Rule => None,
        })
        .collect();
    let (widths, alignments) = if rows.is_empty() {
        // Only rules: as many columns as the first has `+`, one wide.
        let columns = 1 + lines[0].matches('+').count();
        (vec![1; columns], vec!['l'; columns])
    } else {
        let columns = rows.iter().map(|row| row.len()).max().unwrap_or(0);
        let mut widths = Vec::new();
        let mut alignments = Vec::new();
        for column in 0..columns {
            let mut max_width = 1;
            let mut fixed = None;
            let mut numbers = 0;
            let mut non_empty = 0;
            for row in &rows {
                let cell = row.get(column).map_or("", String::as_str);
                max_width = max_width.max(shown_width(cell));
                if fixed.is_some() || cell.is_empty() {
                    continue;
                }
                if let Some(cookie) = COOKIE.captures(cell) {
                    fixed = cookie[1].chars().next();
                } else {
                    non_empty += 1;
                    if NUMBER.is_match(cell) {
                        numbers += 1;
                    }
                }
            }
            widths.push(max_width);
            // `org-table-number-fraction` is 0.5.
            alignments.push(fixed.unwrap_or(if 2 * numbers >= non_empty { 'r' } else { 'l' }));
        }
        (widths, alignments)
    };
    let indent = &lines[0][..lines[0].find('|').unwrap_or(0) + 1];
    let rule = widths.iter().map(|width| "-".repeat(width + 2)).collect::<Vec<_>>().join("+");
    table
        .iter()
        .map(|row| match row {
            Row::Rule => format!("{indent}{rule}|"),
            Row::Cells(cells) => {
                let fields: Vec<String> = (0..widths.len())
                    .map(|column| {
                        align_field(cells.get(column).map_or("", String::as_str), widths[column], alignments[column])
                    })
                    .collect();
                format!("{indent}{}|", fields.join("|"))
            }
        })
        .collect()
}
