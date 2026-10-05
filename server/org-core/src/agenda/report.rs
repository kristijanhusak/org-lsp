//! The clock report under an agenda (`org-agenda-clockreport-mode`, `R`):
//! the clock table `org-clock-get-clocktable` makes for the agenda files
//! and the days shown, with `org-agenda-clockreport-parameter-plist`'s
//! `:link t :maxlevel 2` and the table defaults (`:indent t`,
//! `:narrow 40!`), aligned as `org-table-align` aligns it.

use super::item::hidden_ranges;
use crate::clock::{clock_minutes, duration};
use crate::date::Day;
use crate::outline::Outline;

/// `:maxlevel`
const MAX_LEVEL: usize = 2;
/// `:narrow 40!`: how long a headline's description may be.
const NARROW: usize = 40;

/// The minutes clocked under each entry of `outline` within `[from,
/// to)`, its subtree included (`org-clock-sum`), and the total of the
/// file.
pub fn entry_minutes(outline: &Outline, from: i64, to: i64) -> (Vec<i64>, i64) {
    let mut own = vec![0; outline.entries.len()];
    for line in &outline.clocks {
        if let Some(entry) = line.entry {
            own[entry] += clock_minutes(&line.clock, Some(from), Some(to));
        }
    }
    // Org walks backwards and adds what it saw since the last headline to
    // every level up to that headline's.
    let mut times = vec![0; outline.entries.len()];
    let mut levels = vec![0i64; 32];
    for (index, entry) in outline.entries.iter().enumerate().rev() {
        let level = entry.level;
        if level >= levels.len() {
            levels.resize(level * 2, 0);
        }
        if own[index] > 0 || levels[level] > 0 {
            for slot in levels.iter_mut().take(level + 1) {
                *slot += own[index];
            }
            times[index] = levels[level];
            for slot in levels.iter_mut().skip(level) {
                *slot = 0;
            }
        }
    }
    (times, levels[0])
}

/// `org-get-heading t t t t`: the title without keyword, priority,
/// `COMMENT` and tags.
fn heading_title(head: &str, todo: Option<&str>) -> String {
    let mut text = head;
    if let Some(todo) = todo {
        text = text.strip_prefix(todo).map_or(text, str::trim_start);
    }
    if text.starts_with("[#") {
        if let Some(end) = text.find(']') {
            text = text[end + 1..].trim_start();
        }
    }
    if let Some(rest) = text.strip_prefix("COMMENT").filter(|rest| rest.is_empty() || rest.starts_with(' ')) {
        text = rest.trim_start();
    }
    if let Some((blanks, _)) = crate::text::tag_group(text) {
        text = &text[..blanks];
    }
    text.trim_end().to_string()
}

/// The headline as Emacs's mode line shows a clocked entry
/// (`org-clock--mode-line-heading`): the title, its links as their
/// descriptions.
pub fn plain_heading(head: &str, todo: Option<&str>) -> String {
    display_format(&heading_title(head, todo)).trim().to_string()
}

/// Statistics cookies out (`\[[0-9]*\(?:%\|/[0-9]*\)\]`).
fn without_cookies(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let digits = after.bytes().take_while(u8::is_ascii_digit).count();
        let tail = &after[digits..];
        let cookie_end = if let Some(stripped) = tail.strip_prefix("%]") {
            Some(after.len() - stripped.len())
        } else if let Some(stripped) = tail.strip_prefix('/') {
            let more = stripped.bytes().take_while(u8::is_ascii_digit).count();
            stripped[more..].starts_with(']').then(|| digits + 1 + more + 1)
        } else {
            None
        };
        match cookie_end {
            Some(length) => rest = &after[length..],
            None => {
                out.push('[');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// `org-link--normalize-string`: cookies out, blanks packed.
fn normalize(text: &str) -> String {
    without_cookies(text).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `org-link-display-format`: links as their descriptions.
fn display_format(text: &str) -> String {
    let hidden = hidden_ranges(text);
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    for (start, end) in &hidden {
        out.push_str(&text[copied..*start]);
        copied = *end;
    }
    out.push_str(&text[copied..]);
    out
}

/// `org-link-escape`: brackets and the backslashes before them.
fn escape(target: &str) -> String {
    let mut out = String::with_capacity(target.len());
    let chars: Vec<char> = target.chars().collect();
    for (index, c) in chars.iter().enumerate() {
        match c {
            '[' | ']' => {
                out.push('\\');
                out.push(*c);
            }
            '\\' => {
                // Backslashes before a bracket, or at the end, double.
                let next = chars[index + 1..].iter().find(|c| **c != '\\');
                if matches!(next, None | Some('[') | Some(']')) {
                    out.push('\\');
                }
                out.push('\\');
            }
            _ => out.push(*c),
        }
    }
    out
}

/// `org-shorten-string`
fn shorten(text: &str, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max {
        return text.to_string();
    }
    // Up to `max - 4` characters and a non-blank, then a blank or the end.
    let limit = max.saturating_sub(4).max(1);
    let mut best = None;
    for end in (2..=limit + 1).rev() {
        if end > chars.len() {
            continue;
        }
        let last = chars[end - 1];
        let follows = chars.get(end).copied();
        if last != ' ' && matches!(follows, None | Some(' ')) {
            best = Some(end);
            break;
        }
    }
    match best {
        Some(end) => format!("{}...", chars[..end].iter().collect::<String>()),
        None => format!("{}...", chars[..max.saturating_sub(3)].iter().collect::<String>()),
    }
}

/// `org-table-number-regexp`
fn is_number(cell: &str) -> bool {
    let cell = cell.trim();
    if cell.is_empty() {
        return false;
    }
    let body = cell.strip_prefix(['<', '>']).unwrap_or(cell);
    let leading = body.chars().take_while(|c| matches!(c, '-' | '+' | '^' | '.') || c.is_ascii_digit()).count();
    let has_digit = body.chars().take(leading).any(|c| c.is_ascii_digit());
    if has_digit {
        let rest: String = body.chars().skip(leading).collect();
        return rest.chars().all(|c| {
            matches!(c, '-' | '+' | '^' | '.' | 'e' | 'E' | 'd' | 'D' | 'x' | '(' | ')' | '%' | ':')
                || c.is_ascii_digit()
        });
    }
    matches!(cell, "nan" | "inf" | "-inf" | "+inf" | "uinf")
}

/// How wide a cell shows: links as their descriptions.
fn shown_width(cell: &str) -> usize {
    crate::text::width(&display_format(cell))
}

/// `org-table-align`: rows of cells, `None` for a rule.
fn align(rows: &[Option<Vec<String>>]) -> Vec<String> {
    let columns = rows.iter().flatten().map(Vec::len).max().unwrap_or(1);
    let mut widths = vec![1; columns];
    let mut right = vec![false; columns];
    for column in 0..columns {
        let (mut numbers, mut non_empty) = (0, 0);
        for row in rows.iter().flatten() {
            let cell = row.get(column).map_or("", String::as_str);
            widths[column] = widths[column].max(shown_width(cell));
            if !cell.is_empty() {
                non_empty += 1;
                if is_number(cell) {
                    numbers += 1;
                }
            }
        }
        // `org-table-number-fraction`: 0.5.
        right[column] = non_empty > 0 && 2 * numbers >= non_empty;
    }
    rows.iter()
        .map(|row| match row {
            None => format!("|{}|", widths.iter().map(|width| "-".repeat(width + 2)).collect::<Vec<_>>().join("+")),
            Some(cells) => {
                let mut line = String::from("|");
                for column in 0..columns {
                    let cell = cells.get(column).map_or("", String::as_str);
                    let padding = " ".repeat(widths[column] - shown_width(cell));
                    if right[column] {
                        line.push_str(&format!(" {padding}{cell} |"));
                    } else {
                        line.push_str(&format!(" {cell}{padding} |"));
                    }
                }
                line
            }
        })
        .collect()
}

/// The clock table for `files` (outline, path, file name) and the days
/// `[from, to)`, as lines.
pub fn clock_table(files: &[(&Outline, &str, &str)], from: Day, to: Day) -> Vec<String> {
    let (start, end) = (from * 1440, to * 1440);
    struct FileTable {
        name: String,
        time: i64,
        rows: Vec<(usize, String, i64)>,
    }
    let mut tables = Vec::new();
    for (outline, path, name) in files {
        let (times, total) = entry_minutes(outline, start, end);
        let mut rows = Vec::new();
        for (index, entry) in outline.entries.iter().enumerate() {
            if times[index] <= 0 || entry.level > MAX_LEVEL {
                continue;
            }
            let headline = heading_title(&entry.head, entry.todo.as_deref());
            let search = format!("*{}", normalize(&headline));
            let description = shorten(display_format(&without_cookies(&headline)).trim(), NARROW);
            let link = format!("[[file:{}][{description}]]", escape(&format!("{path}::{search}")));
            rows.push((entry.level, link, times[index]));
        }
        tables.push(FileTable { name: name.to_string(), time: total, rows });
    }
    let total: i64 = tables.iter().map(|table| table.time).sum();
    let levels = tables.iter().filter(|table| table.time > 0).flat_map(|table| table.rows.iter().map(|row| row.0));
    let time_columns = levels.max().map_or(1, |level| level.min(MAX_LEVEL));
    let cells = |values: Vec<&str>| Some(values.into_iter().map(str::to_string).collect::<Vec<_>>());
    let mut rows: Vec<Option<Vec<String>>> = Vec::new();
    let mut header = vec!["File", "Headline", "Time"];
    header.extend(std::iter::repeat_n("", time_columns - 1));
    rows.push(cells(header));
    rows.push(None);
    let total_time = format!("*{}*", duration(total));
    let mut total_row = vec!["", "ALL *Total time*", &total_time];
    total_row.extend(std::iter::repeat_n("", time_columns - 1));
    rows.push(cells(total_row));
    if total > 0 {
        for table in &tables {
            rows.push(None);
            let file_time = format!("*{}*", duration(table.time));
            let mut file_row = vec![table.name.as_str(), "*File time*", &file_time];
            file_row.extend(std::iter::repeat_n("", time_columns - 1));
            rows.push(cells(file_row));
            for (level, link, time) in &table.rows {
                let indent = if *level == 1 { String::new() } else { format!("\\_{}", " ".repeat(2 * (level - 1))) };
                let mut row = vec![String::new(), format!("{indent}{link}")];
                // The time goes in its level's column.
                row.extend(std::iter::repeat_n(String::new(), (*level).min(time_columns) - 1));
                row.push(duration(*time));
                row.extend(std::iter::repeat_n(String::new(), time_columns.saturating_sub(*level)));
                rows.push(Some(row));
            }
        }
    }
    align(&rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use org_syntax::TodoKeywords;

    #[test]
    fn shortens_as_org_does() {
        assert_eq!(shorten("short", 40), "short");
        assert_eq!(shorten("a very long headline that goes on and on and on", 20), "a very long...");
        assert_eq!(shorten("unbreakablewordthatgoesonandon", 10), "unbreak...");
    }

    #[test]
    fn numbers_and_cookies() {
        assert!(is_number("1:45") && is_number("-3") && is_number("12%"));
        assert!(!is_number("*1:45*") && !is_number("Time") && !is_number(""));
        assert_eq!(without_cookies("Plan [1/3] it [50%] [x]"), "Plan  it  [x]");
        assert_eq!(escape("a::*b [c]"), "a::*b \\[c\\]");
    }

    #[test]
    fn a_table_for_two_files() {
        let text =
            "* TODO Write :office:\n:LOGBOOK:\nCLOCK: [2026-10-05 Mon 08:00]--[2026-10-05 Mon 09:15] =>  1:15\n:END:\n\
                    ** Outline\nCLOCK: [2026-10-05 Mon 09:30]--[2026-10-05 Mon 10:00] =>  0:30\n";
        let outline = Outline::new(&org_syntax::parse(text, &TodoKeywords::default()), text, "work");
        let empty = Outline::new(&org_syntax::parse("* x\n", &TodoKeywords::default()), "* x\n", "empty");
        let day = crate::date::Date::new(2026, 10, 5).to_day();
        let lines =
            clock_table(&[(&outline, "/o/work.org", "work.org"), (&empty, "/o/empty.org", "empty.org")], day, day + 1);
        assert_eq!(
            lines,
            [
                "| File      | Headline         | Time   |      |",
                "|-----------+------------------+--------+------|",
                "|           | ALL *Total time* | *1:45* |      |",
                "|-----------+------------------+--------+------|",
                "| work.org  | *File time*      | *1:45* |      |",
                "|           | [[file:/o/work.org::*Write][Write]]            | 1:45   |      |",
                "|           | \\_  [[file:/o/work.org::*Outline][Outline]]      |        | 0:30 |",
                "|-----------+------------------+--------+------|",
                "| empty.org | *File time*      | *0:00* |      |",
            ]
        );
        // Nothing clocked: one time column.
        let lines = clock_table(&[(&empty, "/o/empty.org", "empty.org")], day, day + 1);
        assert_eq!(
            lines,
            [
                "| File | Headline         | Time   |",
                "|------+------------------+--------|",
                "|      | ALL *Total time* | *0:00* |"
            ]
        );
    }
}
