//! Table formulas, as `org-table-recalculate` applies the `#+TBLFM:` line
//! under a table.
//!
//! Column formulas (`$3=$1*$2`) apply to every data line below the
//! header, or to the marked lines only when the table marks lines; field
//! and range formulas (`@2$3=...`, `@>$2=vsum(@I..@II)`) to their fields,
//! after the columns. References, names, parameters (`$` and `_`/`^`
//! rows), constants (`#+CONSTANTS:`, `$PROP_name`) and the modes after
//! `;` are read as `org-table-eval-formula` reads them, and the formulas
//! go to [`crate::calc`] as Org gives them to Calc.
//!
//! Not supported, refused with a message: Emacs Lisp formulas, remote
//! references, units (`;u`), fractions (`;F`), symbolic mode (`;S`)
//! and engineering notation (`;e`).

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

use crate::calc::{self, FloatFormat, Modes};
use crate::format::align_table;

/// How much `org-table-recalculate` recalculates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// `C-c *`: the column formulas on the cursor's line, and every field
    /// formula.
    Line,
    /// `C-u C-c *`: every line.
    All,
    /// `C-u C-u C-c *`: every line, again until the table does not change
    /// (`org-table-iterate`), at most ten times.
    Iterate,
}

/// The table after the formulas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recalculated {
    pub text: String,
    /// What Emacs says when it is done, when it says something worth
    /// showing.
    pub message: Option<String>,
}

/// Whether the cursor at `offset` is in a table that has formulas, or on
/// the `#+TBLFM:` line of one.
pub fn has_formulas(text: &str, offset: usize) -> bool {
    locate(text, offset).is_ok_and(|place| !place.formulas.trim().is_empty())
}

/// Whether the cursor at `offset` is on the `#+TBLFM:` line of a table.
pub fn on_formula_line(text: &str, offset: usize) -> bool {
    locate(text, offset).is_ok_and(|place| place.on_formulas)
}

/// Recalculate the table at `offset`, or the one whose `#+TBLFM:` line it
/// is on (`org-table-calc-current-TBLFM`: the formulas of that line).
/// `property` reads an Org property at the table, for `$PROP_name`.
pub fn recalculate(
    text: &str,
    offset: usize,
    mode: Mode,
    property: &dyn Fn(&str) -> Option<String>,
) -> Result<Recalculated, String> {
    let place = locate(text, offset)?;
    let lines: Vec<&str> = text[place.start..place.end].split_terminator('\n').collect();
    let constants = constants(text);
    let names = Names { constants: &constants, property };
    let formulas = stored_formulas(&place.formulas)?;
    let mode = if place.on_formulas && mode == Mode::Line { Mode::All } else { mode };
    let mut current: Vec<String> = lines.iter().map(|line| (*line).to_string()).collect();
    let mut message = None;
    if mode == Mode::Iterate {
        let mut iterations = 0;
        loop {
            iterations += 1;
            let next = recalculate_once(&current, &formulas, true, place.cursor_line, &names)?;
            if next == current {
                message = Some(if iterations > 1 {
                    format!("Convergence after {iterations} iterations")
                } else {
                    "Table was already stable".to_string()
                });
                break;
            }
            current = next;
            if iterations == 10 {
                return Err(format!("No convergence after {iterations} iterations"));
            }
        }
    } else {
        current = recalculate_once(&current, &formulas, mode == Mode::All, place.cursor_line, &names)?;
    }
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..place.start]);
    for line in &current {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(&text[place.end..]);
    Ok(Recalculated { text: out, message })
}

// ----- where

/// The table the cursor is in, and the formulas that apply.
struct Place {
    /// The table's lines, through the newline of the last one.
    start: usize,
    end: usize,
    /// The cursor's line, from the table's first; the table's last when
    /// the cursor is on a formula line.
    cursor_line: usize,
    /// The formulas: the text after `#+TBLFM:`.
    formulas: String,
    on_formulas: bool,
}

static TBLFM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^[ \t]*#\+TBLFM: *(.*)$").expect("a valid regex"));

fn is_table_line(line: &str) -> bool {
    line.trim_start_matches([' ', '\t']).starts_with('|')
}

fn locate(text: &str, offset: usize) -> Result<Place, String> {
    let mut starts = vec![0];
    starts.extend(text.match_indices('\n').map(|(index, _)| index + 1));
    if starts.last() == Some(&text.len()) && !text.is_empty() {
        starts.pop();
    }
    let lines: Vec<&str> = starts
        .iter()
        .map(|&start| {
            let rest = &text[start..];
            &rest[..rest.find('\n').unwrap_or(rest.len())]
        })
        .collect();
    let line_end = |index: usize| starts.get(index + 1).copied().unwrap_or(text.len());
    let cursor = starts.partition_point(|&start| start <= offset).saturating_sub(1);
    let (first, last, cursor_line, formulas, on_formulas) = if lines.get(cursor).is_some_and(|line| is_table_line(line))
    {
        let first = (0..=cursor).rev().take_while(|&index| is_table_line(lines[index])).last().unwrap_or(cursor);
        let last = (cursor..lines.len()).take_while(|&index| is_table_line(lines[index])).last().unwrap_or(cursor);
        // The first `#+TBLFM:` line after the table, past blank lines.
        let mut next = last + 1;
        while next < lines.len() && lines[next].trim_matches([' ', '\t']).is_empty() {
            next += 1;
        }
        let formulas = lines
            .get(next)
            .and_then(|line| TBLFM.captures(line))
            .map(|captures| captures[1].to_string())
            .unwrap_or_default();
        (first, last, cursor - first, formulas, false)
    } else if let Some(captures) = lines.get(cursor).and_then(|line| TBLFM.captures(line)) {
        // On a formula line of the table above, directly after it or
        // after its other formula lines.
        let mut above = cursor;
        while above > 0 && TBLFM.is_match(lines[above - 1]) {
            above -= 1;
        }
        if above == 0 || !is_table_line(lines[above - 1]) {
            return Err("Not at a table".to_string());
        }
        let last = above - 1;
        let first = (0..=last).rev().take_while(|&index| is_table_line(lines[index])).last().unwrap_or(last);
        (first, last, last - first, captures[1].to_string(), true)
    } else {
        return Err("Not at a table".to_string());
    };
    Ok(Place { start: starts[first], end: line_end(last), cursor_line, formulas, on_formulas })
}

/// `org-table-formula-constants-local`: the `#+CONSTANTS:` of the file,
/// a later value of a name over an earlier one.
fn constants(text: &str) -> Vec<(String, String)> {
    static KEYWORD: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?im)^[ \t]*#\+CONSTANTS:[ \t]*(.*)$").expect("a valid regex"));
    static PAIR: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^([a-zA-Z0][_a-zA-Z0-9]*)=(.*)").expect("a valid regex"));
    let mut found: Vec<(String, String)> = Vec::new();
    for captures in KEYWORD.captures_iter(text) {
        for pair in captures[1].split_whitespace() {
            if let Some(pair) = PAIR.captures(pair) {
                match found.iter_mut().find(|(name, _)| *name == pair[1]) {
                    Some(old) => old.1 = pair[2].to_string(),
                    None => found.push((pair[1].to_string(), pair[2].to_string())),
                }
            }
        }
    }
    found
}

/// `org-table-get-stored-formulas`: `(lhs, rhs)` in the order written.
fn stored_formulas(line: &str) -> Result<Vec<(String, String)>, String> {
    static FORMULA: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(@[-+I<>0-9.$@]+|\$([_a-zA-Z0-9]+|[<>]+)) *= *(.*[^ \t])").expect("a valid regex")
    });
    static COLUMN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\$([0-9]+|[<>]+)$").expect("a valid regex"));
    static SEPARATOR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r" *:: *").expect("a valid regex"));
    let mut found: Vec<(String, String)> = Vec::new();
    for string in split_string(line, &SEPARATOR) {
        let Some(captures) = FORMULA.captures(string) else { continue };
        let whole = &captures[1];
        let lhs = match captures.get(2) {
            None => whole.to_string(),
            Some(_) if COLUMN.is_match(whole) => whole.to_string(),
            // A named field.
            Some(name) => name.as_str().to_string(),
        };
        if found.iter().any(|(seen, _)| *seen == lhs) {
            return Err(format!("Double definition `{lhs}=' in TBLFM line, please fix by hand"));
        }
        found.push((lhs, captures[3].to_string()));
    }
    Ok(found)
}

/// `org-split-string`: split at `separator`, but not at the start or end.
fn split_string<'a>(string: &'a str, separator: &Regex) -> Vec<&'a str> {
    let mut parts = Vec::new();
    let mut at = 0;
    for found in separator.find_iter(string) {
        if found.start() == 0 && at == 0 {
            at = found.end();
            continue;
        }
        parts.push(&string[at..found.start()]);
        at = found.end();
    }
    if at < string.len() || (at == 0 && string.is_empty()) {
        parts.push(&string[at..]);
    }
    parts
}

// ----- the table

/// The lines of a table: the fields of the data lines, as Org reads them,
/// and the rules.
struct Table {
    /// The text before the first `|` of each line.
    indent: String,
    rows: Vec<Option<Vec<String>>>,
    /// The rules as written.
    rules: Vec<String>,
}

impl Table {
    fn read(lines: &[String]) -> Table {
        let indent = lines.first().map_or("", |line| &line[..line.find('|').unwrap_or(0)]).to_string();
        let rows = lines
            .iter()
            .map(|line| {
                let rest = &line.trim_start_matches([' ', '\t'])[1..];
                (!rest.starts_with('-')).then(|| fields(line))
            })
            .collect();
        Table { indent, rows, rules: lines.to_vec() }
    }

    fn is_data(&self, line: usize) -> bool {
        self.rows.get(line).is_some_and(Option::is_some)
    }

    /// The field in column `column` (from 1) of `line`, trimmed; empty
    /// beyond the last.
    fn get(&self, line: usize, column: usize) -> String {
        match self.rows.get(line) {
            Some(Some(fields)) if column > 0 => fields.get(column - 1).cloned().unwrap_or_default(),
            _ => String::new(),
        }
    }

    fn set(&mut self, line: usize, column: usize, value: String) {
        if let Some(Some(fields)) = self.rows.get_mut(line) {
            if fields.len() < column {
                fields.resize(column, String::new());
            }
            fields[column - 1] = value;
        }
    }

    /// The lines, aligned as `org-table-align` aligns them.
    fn write(&self) -> Vec<String> {
        let lines: Vec<String> = self
            .rows
            .iter()
            .zip(&self.rules)
            .map(|(row, rule)| match row {
                Some(fields) if fields.is_empty() => format!("{}|", self.indent),
                Some(fields) => format!("{}| {} |", self.indent, fields.join(" | ")),
                None => rule.clone(),
            })
            .collect();
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        align_table(&lines)
    }
}

/// The fields of a table line, as `org-split-string` on `" *| *"` makes
/// them of the trimmed line.
fn fields(line: &str) -> Vec<String> {
    static SEPARATOR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r" *\| *").expect("a valid regex"));
    split_string(line.trim_matches([' ', '\t', '\n', '\r']), &SEPARATOR)
        .into_iter()
        .map(|field| field.trim_matches([' ', '\t']).to_string())
        .collect()
}

/// What `org-table-analyze` finds.
struct Analysis {
    /// Whether each line is a rule, with a rule after the last.
    rules: Vec<bool>,
    /// The data lines: `dlines[n]` is the line of the nth, from 1.
    dlines: Vec<usize>,
    columns: usize,
    /// Column names, from the `!` line.
    column_names: Vec<(String, usize)>,
    /// Parameters: the `$` lines' and the named fields' values, the one
    /// found first taking precedence.
    parameters: Vec<(String, String)>,
    /// Named fields: name, line, column.
    named: Vec<(String, usize, usize)>,
}

static NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-zA-Z][_a-zA-Z0-9]*$").expect("a valid regex"));

fn first_field(line: &[String]) -> &str {
    line.first().map_or("", String::as_str)
}

fn analyze(table: &Table) -> Result<Analysis, String> {
    let rows = &table.rows;
    let mut column_names = Vec::new();
    if let Some(fields) = rows.iter().flatten().find(|fields| first_field(fields) == "!") {
        for (index, name) in fields[1..].iter().enumerate() {
            if NAME.is_match(name) {
                column_names.push((name.clone(), index + 2));
            }
        }
    }
    let mut parameters: Vec<(String, String)> = Vec::new();
    static PARAMETER: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^([a-zA-Z][_a-zA-Z0-9]*|%) *= *(.*)").expect("a valid regex"));
    for fields in rows.iter().flatten().filter(|fields| first_field(fields) == "$") {
        for field in &fields[1..] {
            if let Some(captures) = PARAMETER.captures(field) {
                parameters.insert(0, (captures[1].to_string(), captures[2].to_string()));
            }
        }
    }
    let mut named = Vec::new();
    for (line, row) in rows.iter().enumerate() {
        let Some(fields) = row else { continue };
        let target = match first_field(fields) {
            "_" => line + 1,
            "^" if line > 0 => line - 1,
            _ => continue,
        };
        let Some(Some(values)) = rows.get(target) else { continue };
        for (index, (name, value)) in fields[1..].iter().zip(values.iter().skip(1)).enumerate() {
            if NAME.is_match(name) {
                parameters.insert(0, (name.clone(), value.clone()));
                named.insert(0, (name.clone(), target, index + 2));
            }
        }
    }
    let mut rules: Vec<bool> = rows.iter().map(Option::is_none).collect();
    rules.push(true);
    let mut dlines = vec![0];
    dlines.extend((0..rows.len()).filter(|&line| rows[line].is_some()));
    let columns = rows.iter().flatten().next().map(Vec::len).ok_or("No table data line around here")?;
    Ok(Analysis { rules, dlines, columns, column_names, parameters, named })
}

// ----- names

struct Names<'a> {
    constants: &'a [(String, String)],
    property: &'a dyn Fn(&str) -> Option<String>,
}

/// `org-table-formula-handle-first/last-rc`: `@<`, `@>`, `$<`, `$>` as
/// the numbers of the first and last rows and columns.
fn handle_first_last(formula: &str, analysis: &Analysis) -> Result<String, String> {
    static MARK: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"([@$])(<+|>+)|(remote\([^)]+\))").expect("a valid regex"));
    let mut out = String::new();
    let mut at = 0;
    for captures in MARK.captures_iter(formula) {
        let whole = captures.get(0).expect("a match");
        out.push_str(&formula[at..whole.start()]);
        at = whole.end();
        if captures.get(3).is_some() {
            out.push_str(whole.as_str());
            continue;
        }
        let sign = &captures[1];
        let max = if sign == "@" { analysis.dlines.len() - 1 } else { analysis.columns };
        let marks = &captures[2];
        let n = if marks.starts_with('<') { marks.len() as i64 } else { max as i64 - marks.len() as i64 + 1 };
        if n < 1 || n > max as i64 {
            return Err(format!("Reference \"{}\" in expression \"{formula}\" points outside table", whole.as_str()));
        }
        out.push_str(&format!("{sign}{n}"));
    }
    out.push_str(&formula[at..]);
    Ok(out)
}

/// Whether `c` is a constituent of words, which `\>` ends a name before.
fn is_word(c: char) -> bool {
    c.is_alphanumeric()
}

/// `org-table-formula-substitute-names`: column names as their numbers,
/// parameters and constants as their values.
fn substitute_names(formula: &str, analysis: &Analysis, names: &Names) -> String {
    // Column names: the longest that ends a word.
    let mut new = String::new();
    let mut rest = formula;
    while let Some(dollar) = rest.find('$') {
        new.push_str(&rest[..dollar]);
        let after = &rest[dollar + 1..];
        let found = analysis
            .column_names
            .iter()
            .filter(|(name, _)| {
                after.starts_with(name.as_str()) && !after[name.len()..].chars().next().is_some_and(is_word)
            })
            .max_by_key(|(name, _)| name.len());
        match found {
            Some((name, column)) => {
                new.push_str(&format!("${column}"));
                rest = &after[name.len()..];
            }
            None => {
                new.push('$');
                rest = after;
            }
        }
    }
    new.push_str(rest);
    // Parameters and constants.
    static PARAMETER: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\$([a-zA-Z][_a-zA-Z0-9]*)|(\bremote\([^)]*\))").expect("a valid regex"));
    let parenthesize = !formula.starts_with('\'');
    static DURATION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r";.*[Tt].*$").expect("a valid regex"));
    let duration = DURATION.is_match(formula);
    let mut start = 0;
    while let Some(captures) = PARAMETER.captures_at(&new, start) {
        let whole = captures.get(0).expect("a match");
        if captures.get(2).is_some() {
            start = whole.end();
            continue;
        }
        start = whole.start() + 1;
        let mut value = constant(&captures[1], analysis, names);
        if duration && !value.trim().is_empty() {
            value = time_string_to_seconds(&value);
        }
        let replacement = if parenthesize { format!("({value})") } else { value };
        new.replace_range(whole.range(), &replacement);
    }
    new
}

/// `org-table-get-constant`.
fn constant(name: &str, analysis: &Analysis, names: &Names) -> String {
    if let Some((_, value)) = analysis.parameters.iter().find(|(parameter, _)| parameter == name) {
        return value.clone();
    }
    if let Some((_, value)) = names.constants.iter().find(|(constant, _)| constant == name) {
        return value.clone();
    }
    if let Some(property) = name.strip_prefix("PROP_") {
        if let Some(value) = (names.property)(property) {
            return value;
        }
    }
    "#UNDEFINED_NAME".to_string()
}

// ----- rows and ranges

/// `org-table--descriptor-line`: the line a row descriptor (`3`, `-1`,
/// `I`, `II+2`, ...) stands for, seen from line `cline`. `None` for `0`,
/// which is the current line.
fn descriptor_line(desc: &str, cline: usize, analysis: &Analysis) -> Result<Option<usize>, String> {
    if desc.bytes().all(|b| b.is_ascii_digit()) {
        let n: usize = desc.parse().map_err(|_| format!("Invalid row descriptor `{desc}'"))?;
        if n == 0 {
            return Ok(None);
        }
        return analysis.dlines.get(n).copied().map(Some).ok_or_else(|| format!("Invalid row number in @{desc}"));
    }
    static DESCRIPTOR: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(([-+])?(I+))?(([-+])?([0-9]+))?").expect("a valid regex"));
    let captures = DESCRIPTOR.captures(desc).expect("matches at least empty");
    let (hlines, hdir) = (captures.get(3), captures.get(2).map(|m| m.as_str()));
    let (odir, on) = (captures.get(5).map(|m| m.as_str()), captures.get(6));
    if (hlines.is_none() && on.is_none()) || (hlines.is_some() && on.is_some() && odir.is_none()) {
        return Err(format!("Invalid row descriptor `{desc}'"));
    }
    let mut cline = cline as i64;
    let mut hn = hlines.map(|m| m.len() as i64);
    let mut hdir = hdir;
    let relative = on.is_some() && odir.is_some();
    if let (Some(n), None) = (hn, hdir) {
        cline = 0;
        hdir = Some("+");
        if analysis.rules[0] {
            hn = Some(n - 1);
        }
    }
    if let Some(n) = hn {
        cline = row_type(true, n, cline, hdir == Some("-"), desc, analysis)?;
    }
    if let Some(on) = on {
        let n: i64 = on.as_str().parse().unwrap_or(0);
        cline = row_type(false, n, cline, odir == Some("-"), desc, analysis)?;
    }
    let _ = relative;
    Ok(Some(cline as usize))
}

/// `org-table--row-type`: the line of the nth rule (`rule`) or data line
/// from line `i`. Relative references may cross rules
/// (`org-table-relative-ref-may-cross-hline` is t).
fn row_type(rule: bool, n: i64, mut i: i64, backwards: bool, desc: &str, analysis: &Analysis) -> Result<i64, String> {
    let l = analysis.rules.len() as i64;
    for _ in 0..n {
        loop {
            i += if backwards { -1 } else { 1 };
            if i < 0 || i >= l || analysis.rules[i as usize] == rule {
                break;
            }
        }
    }
    if i < 0 || i >= l {
        return Err(format!("Row descriptor {desc} leads outside table"));
    }
    // The rule after the last line is not there: the last line instead.
    Ok(if i == l - 1 { i - 1 } else { i })
}

/// `org-table-range-regexp`.
static RANGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@([-+]?I*[-+]?[0-9]*)(\$[-+]?[0-9]+)?(\.\.@?([-+]?I*[-+]?[0-9]*)(\$[-+]?[0-9]+)?)?")
        .expect("a valid regex")
});

/// What a reference reads: a field, or the fields of a range.
enum Read {
    Field(String),
    Range(Vec<String>),
}

/// The corners of a range: first line, first column, last line, last
/// column.
type Corners = (usize, usize, usize, usize);

/// `org-table-get-range` on the reference `desc`, seen from `line` and
/// `column`.
fn get_range(
    desc: &str,
    line: usize,
    column: usize,
    table: &Table,
    analysis: &Analysis,
    corners_only: bool,
) -> Result<Result<Read, Corners>, String> {
    let captures = RANGE
        .captures(desc)
        .filter(|captures| captures.get(0).is_some_and(|m| m.start() == 0))
        .ok_or_else(|| format!("Invalid table range specifier `{desc}'"))?;
    let row = |group: usize| -> Result<usize, String> {
        match captures.get(group).map(|m| m.as_str()).filter(|r| !r.trim().is_empty()) {
            Some(r) => Ok(descriptor_line(r, line, analysis)?.unwrap_or(line)),
            None => Ok(line),
        }
    };
    let col = |group: usize| -> usize {
        match captures.get(group).map(|m| &m.as_str()[1..]) {
            None => column,
            Some(c) => {
                let n: i64 = c.parse().unwrap_or(0);
                if n == 0 {
                    column
                } else if c.starts_with(['-', '+']) {
                    (column as i64 + n).max(0) as usize
                } else {
                    n as usize
                }
            }
        }
    };
    let rangep = captures.get(3).is_some();
    let (r1, r2, c1, c2) = (row(1)?, row(4)?, col(2), col(5));
    if !corners_only && (!rangep || (r1 == r2 && c1 == c2)) {
        let mut target = r1;
        while target < table.rows.len() && !table.is_data(target) {
            target += 1;
        }
        return Ok(Ok(Read::Field(table.get(target, c1))));
    }
    let (mut first_row, mut last_row) = (r1.min(r2), r1.max(r2));
    let (first_column, last_column) = (c1.min(c2), c1.max(c2));
    if corners_only {
        return Ok(Err((first_row, first_column, last_row, last_column)));
    }
    while first_row < table.rows.len() && !table.is_data(first_row) {
        first_row += 1;
    }
    while last_row > 0 && !table.is_data(last_row) {
        last_row -= 1;
    }
    // Beyond the last field, the cursor stops after the last bar.
    let reach = |line: usize, column: usize| match table.rows.get(line) {
        Some(Some(fields)) => column.min(fields.len() + 1),
        _ => column,
    };
    let (start, end) = (reach(first_row, first_column), reach(last_row, last_column));
    let (start, end) = (start.min(end), start.max(end));
    let mut values = Vec::new();
    for row in first_row..=last_row {
        if table.is_data(row) {
            for column in start..=end {
                values.push(table.get(row, column));
            }
        }
    }
    Ok(Ok(Read::Range(values)))
}

/// `org-table-line-to-dline`: the data line a line is, or the first data
/// line below it (`above`: above it).
fn line_to_dline(line: usize, above: bool, analysis: &Analysis) -> Option<usize> {
    let dlines = &analysis.dlines[1..];
    if dlines.is_empty() || line < dlines[0] || line > *dlines.last()? {
        return None;
    }
    match dlines.binary_search(&line) {
        Ok(index) => Some(index + 1),
        Err(index) => Some(if above { index } else { index + 1 }),
    }
}

/// `org-table-make-reference`: what a reference becomes in the formula.
fn make_reference(read: &Read, keep_empty: bool, numbers: bool) -> String {
    let blank = |value: &str| value.trim().is_empty();
    match read {
        Read::Field(value) if !blank(value) => {
            let value = if numbers { number_to_string(string_to_number(value)) } else { value.clone() };
            format!("({value})")
        }
        Read::Field(_) => if !keep_empty || numbers { "(0)" } else { "nan" }.to_string(),
        Read::Range(values) => {
            let values = values
                .iter()
                .filter(|value| keep_empty || !blank(value))
                .map(|value| {
                    if blank(value) {
                        if numbers { "0" } else { "nan" }.to_string()
                    } else if numbers {
                        number_to_string(string_to_number(value))
                    } else {
                        value.clone()
                    }
                })
                .collect::<Vec<_>>();
            format!("[{}]", values.join(","))
        }
    }
}

// ----- recalculation

fn recalculate_once(
    lines: &[String],
    stored: &[(String, String)],
    all: bool,
    cursor_line: usize,
    names: &Names,
) -> Result<Vec<String>, String> {
    // Without formulas, nothing changes: the table is not even aligned.
    if stored.is_empty() {
        return Ok(lines.to_vec());
    }
    let mut table = Table::read(lines);
    let analysis = analyze(&table)?;
    let mut sorted = stored.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut columns: Vec<(usize, String)> = Vec::new();
    let mut fields: Vec<(String, String)> = Vec::new();
    static COLUMN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\$[0-9]+$").expect("a valid regex"));
    static HLINE_LHS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^@-?I+").expect("a valid regex"));
    for (old_lhs, rhs) in &sorted {
        let rhs = substitute_names(&handle_first_last(rhs, &analysis)?, &analysis, names);
        let lhs = if HLINE_LHS.is_match(old_lhs) {
            return Err("Can't assign to hline relative reference".to_string());
        } else if old_lhs.starts_with("$<") || old_lhs.starts_with("$>") {
            let new = handle_first_last(old_lhs, &analysis)?;
            if sorted.iter().any(|(lhs, _)| *lhs == new) {
                return Err(format!("\"{old_lhs}=\" formula tries to overwrite existing formula for column {new}"));
            }
            new
        } else {
            old_lhs.clone()
        };
        let lhs = handle_first_last(&lhs, &analysis)?;
        if COLUMN.is_match(&lhs) {
            columns.push((lhs[1..].parse().map_err(|_| format!("Invalid column {lhs}"))?, rhs));
        } else {
            fields.push((lhs, rhs));
        }
    }
    let fields = expand_lhs_ranges(fields, cursor_line, &table, &analysis)?;

    // The lines the column formulas apply to.
    let marked = |table: &Table, line: usize, marks: &str| {
        table.rows[line].as_ref().is_some_and(|fields| {
            let first = first_field(fields);
            first.len() == 1 && marks.contains(first)
        })
    };
    let (start, only_marked) = if all {
        if (0..table.rows.len()).any(|line| marked(&table, line, "!$^_#*")) {
            (0, true)
        } else {
            let first_data = (0..table.rows.len()).find(|&line| table.is_data(line));
            let rule = first_data.and_then(|data| (data..table.rows.len()).find(|&line| !table.is_data(line)));
            let below = rule.and_then(|rule| (rule..table.rows.len()).find(|&line| table.is_data(line)));
            (below.unwrap_or(0), false)
        }
    } else {
        (cursor_line, false)
    };
    let end = if all { table.rows.len() } else { (cursor_line + 1).min(table.rows.len()) };

    // Fields that field formulas set: column formulas leave them alone.
    let mut untouchable: HashSet<(usize, usize)> = HashSet::new();
    let mut seen: Vec<String> = Vec::new();
    static ABSOLUTE_ROW: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^@([0-9]+)").expect("a valid regex"));
    for (name, _) in &fields {
        let location = analysis.named.iter().find(|(named, _, _)| named == name);
        let eq_line = match location {
            Some((_, line, _)) => Some(*line),
            None => match ABSOLUTE_ROW.captures(name) {
                Some(captures) => {
                    let n: usize = captures[1].parse().unwrap_or(usize::MAX);
                    Some(*analysis.dlines.get(n).ok_or_else(|| format!("Invalid row number in {name}"))?)
                }
                None => None,
            },
        };
        let reference = match location {
            Some((_, line, column)) => {
                format!("@{}${column}", line_to_dline(*line, false, &analysis).unwrap_or(0))
            }
            None => name.clone(),
        };
        if seen.contains(&reference) {
            return Err(format!("Several field/range formulas try to set {reference}"));
        }
        seen.push(reference);
        if all || eq_line == Some(cursor_line) {
            untouchable.insert(goto_field(name, &analysis)?);
        }
    }

    // The column formulas, line by line.
    for line in start..end {
        let applies = if only_marked { marked(&table, line, "#*") } else { table.is_data(line) };
        if !applies {
            continue;
        }
        if table.rows[line].as_ref().is_some_and(|fields| {
            let first = first_field(fields);
            first.len() == 1 && "_^!$/".contains(first)
        }) {
            continue;
        }
        for (column, formula) in &columns {
            if *column == 0 || untouchable.contains(&(line, *column)) {
                continue;
            }
            // `org-table-goto-column` with `force` makes the column.
            if let Some(Some(row)) = table.rows.get_mut(line) {
                if row.len() < *column {
                    row.resize(*column, String::new());
                }
            }
            eval_formula(&mut table, &analysis, line, *column, formula)?;
        }
    }

    // The field formulas.
    for (reference, formula) in &fields {
        let (line, column) = goto_field(reference, &analysis)?;
        if column > 1000 {
            return Err("Formula column target too large".to_string());
        }
        let count = table.rows.get(line).and_then(Option::as_ref).map_or(0, Vec::len);
        if column > count {
            return Err("Missing columns in the table.  Aborting".to_string());
        }
        eval_formula(&mut table, &analysis, line, column, formula)?;
    }
    Ok(table.write())
}

/// `org-table-goto-field`: the line and column of a named field or of an
/// absolute reference.
fn goto_field(reference: &str, analysis: &Analysis) -> Result<(usize, usize), String> {
    if let Some((_, line, column)) = analysis.named.iter().find(|(name, _, _)| name == reference) {
        return Ok((*line, *column));
    }
    static ABSOLUTE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^@([1-9][0-9]*)\$([1-9][0-9]*)$").expect("a valid regex"));
    let captures = ABSOLUTE.captures(reference).ok_or_else(|| format!("Unknown field: {reference}"))?;
    let row: usize = captures[1].parse().unwrap_or(usize::MAX);
    let line = *analysis.dlines.get(row).ok_or_else(|| format!("Invalid row number in {reference}"))?;
    Ok((line, captures[2].parse().unwrap_or(usize::MAX)))
}

/// `org-table-expand-lhs-ranges`: a row or range on the left as a formula
/// for each of its fields.
fn expand_lhs_ranges(
    formulas: Vec<(String, String)>,
    cursor_line: usize,
    table: &Table,
    analysis: &Analysis,
) -> Result<Vec<(String, String)>, String> {
    static FIELD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^@[-+0-9]+\$-?[0-9]+$").expect("a valid regex"));
    static ROW: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^@[0-9]+$").expect("a valid regex"));
    static COLUMN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\$[0-9]+$").expect("a valid regex"));
    let mut out = Vec::new();
    for (lhs, rhs) in formulas {
        if FIELD.is_match(&lhs) || NAME.is_match(&lhs) || COLUMN.is_match(&lhs) {
            out.push((lhs, rhs));
        } else if ROW.is_match(&lhs) {
            for column in 1..=analysis.columns {
                out.push((format!("{lhs}${column}"), rhs.clone()));
            }
        } else {
            let Err((first, c1, last, c2)) = get_range(&lhs, cursor_line, 1, table, analysis, true)? else {
                unreachable!("corners only")
            };
            let r1 = line_to_dline(first, false, analysis).unwrap_or(0);
            let r2 = line_to_dline(last, true, analysis).unwrap_or(0);
            for row in r1..=r2 {
                for column in c1..=c2 {
                    out.push((format!("@{row}${column}"), rhs.clone()));
                }
            }
        }
    }
    Ok(out)
}

/// The modes after a formula's `;`.
struct FormulaModes {
    calc: Modes,
    /// `;T`, `;t`, `;U`: fields are durations, `HH:MM[:SS]`, and so is
    /// the result.
    duration: Option<Duration>,
    numbers: bool,
    keep_empty: bool,
    /// What remains: a `printf` format for the result.
    format: Option<String>,
}

/// How a duration is written: `HH:MM:SS` (`;T`), `HH:MM` (`;U`), or
/// hours with two decimals (`;t`, as `org-table-duration-custom-format`
/// is by default).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Duration {
    Clock,
    HoursMinutes,
    Hours,
}

fn parse_modes(modes: &str) -> Result<FormulaModes, String> {
    static NUMERIC: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([pnfse])(-?[0-9]+)").expect("a valid regex"));
    static FLAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[tTUNLEDRFSu]").expect("a valid regex"));
    let mut fmt = modes.to_string();
    let mut calc = Modes::default();
    let (mut numbers, mut keep_empty, mut duration) = (false, false, None);
    while let Some(captures) = NUMERIC.captures(&fmt) {
        let n: i64 = captures[2].parse().unwrap_or(0);
        let digits = n.max(0) as usize;
        match &captures[1] {
            "p" => {}
            "n" => calc.float_format = FloatFormat::Float(digits),
            "f" => calc.float_format = FloatFormat::Fix(digits),
            "s" => calc.float_format = FloatFormat::Sci(digits),
            _ => return Err("Engineering notation (;e) is not supported in table formulas".to_string()),
        }
        let range = captures.get(0).expect("a match").range();
        fmt.replace_range(range, "");
    }
    while let Some(found) = FLAG.find(&fmt) {
        match found.as_str() {
            "t" | "T" | "U" => {
                numbers = true;
                duration = Some(match found.as_str() {
                    "t" => Duration::Hours,
                    "T" => Duration::Clock,
                    _ => Duration::HoursMinutes,
                });
            }
            "N" => numbers = true,
            "L" => {}
            "E" => keep_empty = true,
            "D" => calc.radians = false,
            "R" => calc.radians = true,
            "F" => return Err("Fractions (;F) are not supported in table formulas".to_string()),
            "S" => return Err("Symbolic mode (;S) is not supported in table formulas".to_string()),
            _ => return Err("Units (;u) are not supported in table formulas".to_string()),
        }
        let range = found.range();
        fmt.replace_range(range, "");
    }
    calc.number_result = numbers && !keep_empty;
    let format = (!fmt.trim().is_empty()).then_some(fmt);
    Ok(FormulaModes { calc, duration, numbers, keep_empty, format })
}

/// `org-table-eval-formula`: the result of `formula` in the field at
/// `line` and `column`, put there.
fn eval_formula(
    table: &mut Table,
    analysis: &Analysis,
    line: usize,
    column: usize,
    formula: &str,
) -> Result<(), String> {
    // The modes follow the last `;`.
    let (formula, modes) = match formula.rfind(';') {
        Some(at) => {
            let percent = analysis.parameters.iter().find(|(name, _)| name == "%").map_or("", |(_, value)| value);
            (&formula[..at], Some(format!("{percent}{}", &formula[at + 1..])))
        }
        None => (formula, None),
    };
    let modes = match &modes {
        Some(modes) => parse_modes(modes)?,
        None => {
            FormulaModes { calc: Modes::default(), duration: None, numbers: false, keep_empty: false, format: None }
        }
    };
    if formula.len() > 2 && formula.starts_with("'(") {
        return Err("Emacs Lisp formulas are not supported".to_string());
    }
    let mut fields: Vec<String> = table.rows[line].clone().unwrap_or_default();
    if modes.duration.is_some() {
        fields = fields.iter().map(|field| time_string_to_seconds(field)).collect();
    }
    if modes.numbers {
        for field in &mut fields {
            if !field.trim().is_empty() {
                *field = number_to_string(string_to_number(field));
            }
        }
    }
    let mut form = formula.to_string();
    // The row and column of the field.
    let dline = (0..=line).filter(|&index| table.is_data(index)).count();
    form = form.replace("@#", &dline.to_string()).replace("$#", &column.to_string());
    static OLD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"&[-+0-9I]").expect("a valid regex"));
    if OLD.is_match(&form) {
        return Err("Formula contains old &row reference, please rewrite using @-syntax".to_string());
    }
    static REMOTE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bremote\(").expect("a valid regex"));
    if REMOTE.is_match(&form) {
        return Err("Remote references are not supported".to_string());
    }
    // Ranges and fields given by row.
    while let Some(found) = RANGE.find(&form).filter(|found| found.len() > 1) {
        let read = get_range(found.as_str(), line, column, table, analysis, false)?;
        let Ok(mut read) = read else { unreachable!("not corners") };
        if modes.duration.is_some() {
            read = match read {
                Read::Field(value) => Read::Field(time_string_to_seconds(&value)),
                Read::Range(values) => Read::Range(values.iter().map(|value| time_string_to_seconds(value)).collect()),
            };
        }
        let replacement = make_reference(&read, modes.keep_empty, modes.numbers);
        if replacement.contains(&form) {
            return Err(format!("Spreadsheet error: invalid reference \"{form}\""));
        }
        form.replace_range(found.range(), &replacement);
    }
    // Ranges within the line.
    static SIMPLE_RANGE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\$(([-+])?[0-9]+)\.\.\$(([-+])?[0-9]+)").expect("a valid regex"));
    while let Some(captures) = SIMPLE_RANGE.captures(&form) {
        let bound = |group: usize, sign: usize| {
            let n: i64 = captures[group].parse().unwrap_or(0);
            n + if captures.get(sign).is_some() { column as i64 } else { 0 }
        };
        let (from, to) = (bound(1, 2) - 1, bound(3, 4));
        if from < 0 || to < from || to as usize > fields.len() {
            return Err(format!("Invalid field specifier \"{}\"", &captures[0]));
        }
        let read = Read::Range(fields[from as usize..to as usize].to_vec());
        let replacement = make_reference(&read, modes.keep_empty, modes.numbers);
        let range = captures.get(0).expect("a match").range();
        form.replace_range(range, &replacement);
    }
    // Fields of the line.
    static FIELD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$(([-+])?[0-9]+)").expect("a valid regex"));
    while let Some(captures) = FIELD.captures(&form) {
        let n: i64 =
            captures[1].parse::<i64>().unwrap_or(0) + if captures.get(2).is_some() { column as i64 } else { 0 };
        let index = if n == 0 { column as i64 } else { n.max(1) };
        let value =
            fields.get(index as usize - 1).ok_or_else(|| format!("Invalid field specifier \"{}\"", &captures[0]))?;
        let replacement = make_reference(&Read::Field(value.clone()), modes.keep_empty, modes.numbers);
        if replacement.contains(formula) {
            return Err(format!("Invalid field specifier \"{}\"", &captures[0]));
        }
        let range = captures.get(0).expect("a match").range();
        form.replace_range(range, &replacement);
    }
    // Timestamps as Calc's date forms: active, and in Org's own format.
    let form = INACTIVE.replace_all(&form, "<$1>");
    let form = ACTIVE.replace_all(&form, |captures: &regex::Captures| normalize_timestamp(&captures[1]));
    static CLOCK: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^[0-9]+:[0-9]+(?::[0-9]+)?$").expect("a valid regex"));
    let evaluated = match modes.duration {
        Some(_) if CLOCK.is_match(&form) => Ok(form.into_owned()),
        _ => calc::eval(&form, &modes.calc),
    };
    let evaluated = match (evaluated, modes.duration) {
        (Ok(value), Some(duration)) if !value.is_empty() => {
            let seconds = if CLOCK.is_match(&value) {
                string_to_number(&time_string_to_seconds(&value))
            } else {
                string_to_number(&value)
            };
            Ok(seconds_to_string(seconds, duration))
        }
        (other, _) => other,
    };
    let value = match evaluated {
        Ok(value) => match &modes.format {
            Some(format) => printf(format, string_to_number(&value)),
            // A date in the table is data, not an appointment.
            None => ACTIVE.replace_all(&value, "[$1]").into_owned(),
        },
        Err(_) => "#ERROR".to_string(),
    };
    table.set(line, column, value.replace('\n', " "));
    Ok(())
}

/// `org-ts-regexp` and `org-ts-regexp-inactive`.
static ACTIVE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<([0-9]{4}-[0-9]{2}-[0-9]{2}(?: [^\n]*?)?)>").expect("a valid regex"));
static INACTIVE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([0-9]{4}-[0-9]{2}-[0-9]{2}(?: [^\n]*?)?)\]").expect("a valid regex"));

/// A timestamp's text as `format-time-string` writes it back with
/// `org-time-stamp-format`: the date, its weekday, and the time when it
/// has one.
fn normalize_timestamp(inside: &str) -> String {
    use crate::date::{weekday, Date, WEEKDAY_ABBREVIATIONS};
    static TIME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([0-9]{1,2}):([0-9]{2})").expect("a valid regex"));
    let number = |range: std::ops::Range<usize>| inside[range].parse::<i64>().unwrap_or(0);
    let date = Date::new(number(0..4), number(5..7), number(8..10));
    let day = date.to_day();
    let date = Date::from_day(day);
    let name = WEEKDAY_ABBREVIATIONS[weekday(day)];
    match TIME.captures(&inside[10..]) {
        Some(time) => {
            let hour: i64 = time[1].parse().unwrap_or(0);
            let minute: i64 = time[2].parse().unwrap_or(0);
            format!("<{date} {name} {hour:02}:{minute:02}>")
        }
        None => format!("<{date} {name}>"),
    }
}

/// `org-table-time-string-to-seconds`: `HH:MM:SS` or `HH:MM` as seconds;
/// another number as it is.
fn time_string_to_seconds(text: &str) -> String {
    static FULL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(-?)([0-9]+):([0-9]+):([0-9]+)").expect("a valid regex"));
    static SHORT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(-?)([0-9]+):([0-9]+)").expect("a valid regex"));
    if text.is_empty() {
        return String::new();
    }
    let part = |captures: &regex::Captures, group: usize| captures[group].parse::<i64>().unwrap_or(0);
    let seconds = if let Some(captures) = FULL.captures(text) {
        let total = part(&captures, 2) * 3600 + part(&captures, 3) * 60 + part(&captures, 4);
        Lisp::Int(if captures[1].is_empty() { total } else { -total })
    } else if let Some(captures) = SHORT.captures(text).filter(|_| !ACTIVE.is_match(text) && !INACTIVE.is_match(text)) {
        let total = part(&captures, 2) * 3600 + part(&captures, 3) * 60;
        Lisp::Int(if captures[1].is_empty() { total } else { -total })
    } else {
        string_to_number(text)
    };
    number_to_string(seconds)
}

/// `org-table-time-seconds-to-string`.
fn seconds_to_string(seconds: Lisp, duration: Duration) -> String {
    let value = match seconds {
        Lisp::Int(n) => n as f64,
        Lisp::Float(f) => f,
    };
    let magnitude = value.abs();
    // `format-seconds` counts whole seconds.
    let whole = magnitude.floor() as i64;
    let clock = format!("{:02}:{:02}:{:02}", whole / 3600, whole % 3600 / 60, whole % 60);
    let text = match duration {
        Duration::Hours => format!("{:.2}", magnitude / 3600.0),
        Duration::Clock => clock,
        Duration::HoursMinutes => clock[..clock.len() - 3].to_string(),
    };
    if value < 0.0 {
        format!("-{text}")
    } else {
        text
    }
}

// ----- Emacs's numbers

/// A number as Emacs Lisp has it.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Lisp {
    Int(i64),
    Float(f64),
}

/// `string-to-number`: the number at the start of the text, 0 if none.
fn string_to_number(text: &str) -> Lisp {
    static NUMBER: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^[ \t\n]*([-+]?)([0-9]*)(\.[0-9]*)?(?:([eE][-+]?[0-9]+))?").expect("a valid regex")
    });
    let Some(captures) = NUMBER.captures(text) else { return Lisp::Int(0) };
    let sign = &captures[1];
    let whole = &captures[2];
    let fraction = captures.get(3).map_or("", |m| m.as_str());
    let exponent = captures.get(4).map_or("", |m| m.as_str());
    let has_fraction_digits = fraction.len() > 1;
    if whole.is_empty() && !has_fraction_digits {
        return Lisp::Int(0);
    }
    if !has_fraction_digits && exponent.is_empty() {
        // `5.` is the integer 5.
        return whole.parse::<i64>().map_or_else(
            |_| Lisp::Float(format!("{sign}{whole}").parse().unwrap_or(0.0)),
            |n| Lisp::Int(if sign == "-" { -n } else { n }),
        );
    }
    let number = format!("{sign}{}{}{exponent}", if whole.is_empty() { "0" } else { whole }, fraction);
    Lisp::Float(number.trim_end_matches('.').parse().unwrap_or(0.0))
}

/// `number-to-string`.
fn number_to_string(number: Lisp) -> String {
    match number {
        Lisp::Int(n) => n.to_string(),
        Lisp::Float(f) => float_to_string(f),
    }
}

/// How Emacs prints a float: the fewest digits from 15 on that read back
/// the same, as `%g` writes them, with `.0` on a whole number.
fn float_to_string(value: f64) -> String {
    if value.is_nan() {
        return if value.is_sign_negative() { "-0.0e+NaN" } else { "0.0e+NaN" }.to_string();
    }
    if value.is_infinite() {
        return if value > 0.0 { "1.0e+INF" } else { "-1.0e+INF" }.to_string();
    }
    let mut text = String::new();
    for precision in 15..=17 {
        text = format_g(value, precision, false);
        if text.parse::<f64>().ok() == Some(value) {
            break;
        }
    }
    if !text.contains(['.', 'e', 'n', 'i']) {
        text.push_str(".0");
    }
    text
}

/// C's `%.{precision}g`, with `#` keeping trailing zeros.
fn format_g(value: f64, precision: usize, alternate: bool) -> String {
    let precision = precision.max(1);
    if value == 0.0 {
        let zeros = if alternate { format!(".{}", "0".repeat(precision - 1)) } else { String::new() };
        return format!("{}0{zeros}", if value.is_sign_negative() { "-" } else { "" });
    }
    let scientific = format!("{:.*e}", precision - 1, value);
    let exponent: i64 = scientific.split_once('e').map_or(0, |(_, e)| e.parse().unwrap_or(0));
    let strip = |text: String| {
        if alternate || !text.contains('.') {
            text
        } else {
            text.trim_end_matches('0').trim_end_matches('.').to_string()
        }
    };
    if exponent < -4 || exponent >= precision as i64 {
        let (mantissa, _) = scientific.split_once('e').unwrap_or((&scientific, ""));
        let mantissa = strip(mantissa.to_string());
        format!("{mantissa}e{}{:02}", if exponent < 0 { '-' } else { '+' }, exponent.abs())
    } else {
        let decimals = (precision as i64 - 1 - exponent).max(0) as usize;
        strip(format!("{value:.decimals$}"))
    }
}

/// C's `%e`.
fn format_e(value: f64, precision: usize) -> String {
    let scientific = format!("{value:.precision$e}");
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let exponent: i64 = exponent.parse().unwrap_or(0);
    format!("{mantissa}e{}{:02}", if exponent < 0 { '-' } else { '+' }, exponent.abs())
}

/// Emacs's `format` with one number, for the `printf` formats of table
/// formulas (`%.2f`, `%d`, `%5.1f%%`, ...).
fn printf(format: &str, number: Lisp) -> String {
    let as_float = match number {
        Lisp::Int(n) => n as f64,
        Lisp::Float(f) => f,
    };
    let as_int = match number {
        Lisp::Int(n) => n,
        Lisp::Float(f) => f.trunc() as i64,
    };
    let mut out = String::new();
    let mut chars = format.chars().peekable();
    let mut used = false;
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let mut flags = String::new();
        while let Some(&flag) = chars.peek().filter(|c| "-+ 0#".contains(**c)) {
            flags.push(flag);
            chars.next();
        }
        let mut width = String::new();
        while let Some(&digit) = chars.peek().filter(|c| c.is_ascii_digit()) {
            width.push(digit);
            chars.next();
        }
        let mut precision: Option<usize> = None;
        if chars.peek() == Some(&'.') {
            chars.next();
            let mut digits = String::new();
            while let Some(&digit) = chars.peek().filter(|c| c.is_ascii_digit()) {
                digits.push(digit);
                chars.next();
            }
            precision = Some(digits.parse().unwrap_or(0));
        }
        let Some(conversion) = chars.next() else { break };
        if conversion == '%' {
            out.push('%');
            continue;
        }
        let value = if used { 0.0 } else { as_float };
        let int = if used { 0 } else { as_int };
        used = true;
        let left = flags.contains('-');
        let plus = flags.contains('+');
        let space = flags.contains(' ');
        let zero = flags.contains('0') && !left;
        let alternate = flags.contains('#');
        let sign_of = |negative: bool| {
            if negative {
                "-"
            } else if plus {
                "+"
            } else if space {
                " "
            } else {
                ""
            }
        };
        let (sign, body, numeric) = match conversion {
            'd' | 'i' => {
                let mut digits = int.unsigned_abs().to_string();
                if let Some(precision) = precision {
                    if digits.len() < precision {
                        digits = format!("{}{digits}", "0".repeat(precision - digits.len()));
                    }
                }
                (sign_of(int < 0), digits, precision.is_none())
            }
            'o' => (sign_of(int < 0), format!("{:o}", int.unsigned_abs()), true),
            'x' => (sign_of(int < 0), format!("{:x}", int.unsigned_abs()), true),
            'X' => (sign_of(int < 0), format!("{:X}", int.unsigned_abs()), true),
            'c' => ("", char::from_u32(int as u32).map(String::from).unwrap_or_default(), false),
            'f' | 'F' => {
                let precision = precision.unwrap_or(6);
                let mut body = format!("{:.precision$}", value.abs());
                if alternate && precision == 0 {
                    body.push('.');
                }
                (sign_of(value.is_sign_negative() && value != 0.0 || value < 0.0), body, true)
            }
            'e' | 'E' => {
                let body = format_e(value.abs(), precision.unwrap_or(6));
                let body = if conversion == 'E' { body.to_uppercase() } else { body };
                (sign_of(value < 0.0), body, true)
            }
            'g' | 'G' => {
                let body = format_g(value.abs(), precision.unwrap_or(6), alternate);
                let body = if conversion == 'G' { body.to_uppercase() } else { body };
                (sign_of(value < 0.0), body, true)
            }
            // `%s` and anything else: the number as Emacs prints it.
            _ => {
                let mut text = number_to_string(number);
                if let Some(precision) = precision {
                    text = text.chars().take(precision).collect();
                }
                ("", text, false)
            }
        };
        let width: usize = width.parse().unwrap_or(0);
        let length = sign.len() + body.chars().count();
        let padding = width.saturating_sub(length);
        if left {
            out.push_str(sign);
            out.push_str(&body);
            out.push_str(&" ".repeat(padding));
        } else if zero && numeric {
            out.push_str(sign);
            out.push_str(&"0".repeat(padding));
            out.push_str(&body);
        } else {
            out.push_str(&" ".repeat(padding));
            out.push_str(sign);
            out.push_str(&body);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_prints_numbers_as_emacs() {
        assert_eq!(string_to_number("5."), Lisp::Int(5));
        assert_eq!(string_to_number("1e3"), Lisp::Float(1000.0));
        assert_eq!(string_to_number("  12"), Lisp::Int(12));
        assert_eq!(string_to_number(".5"), Lisp::Float(0.5));
        assert_eq!(string_to_number("x + 2"), Lisp::Int(0));
        assert_eq!(string_to_number("1.5e"), Lisp::Float(1.5));
        for (value, expected) in [
            (100000.0, "100000.0"),
            (1e16, "1e+16"),
            (0.0001, "0.0001"),
            (0.00001, "1e-05"),
            (1e15, "1e+15"),
            (123456789012345.0, "123456789012345.0"),
            (1234567890123456.0, "1234567890123456.0"),
            (5.0, "5.0"),
        ] {
            assert_eq!(float_to_string(value), expected);
        }
        assert_eq!(printf("%d", Lisp::Float(2.7)), "2");
        assert_eq!(printf("%d", Lisp::Float(-2.7)), "-2");
        assert_eq!(printf("%.2f", Lisp::Int(3)), "3.00");
        assert_eq!(printf("%5.1f|", Lisp::Float(2.25)), "  2.2|");
        assert_eq!(printf("%-6d|", Lisp::Int(3)), "3     |");
        assert_eq!(printf("%05.1f", Lisp::Float(2.5)), "002.5");
        assert_eq!(printf("%x", Lisp::Int(255)), "ff");
        assert_eq!(printf("%e", Lisp::Float(1234.5)), "1.234500e+03");
        assert_eq!(printf("%g", Lisp::Float(0.0001)), "0.0001");
        assert_eq!(printf("%g", Lisp::Float(1e7)), "1e+07");
        assert_eq!(printf("%.1f%%", Lisp::Float(12.34)), "12.3%");
    }
}
