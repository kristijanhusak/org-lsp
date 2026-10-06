//! Capturing: a port of `org-capture`. [`fill`] expands a template as
//! `org-capture-fill-template` does, asking for what its `%^` escapes
//! want one answer at a time, and [`place`] puts the result where the
//! template's target says, as `org-capture-place-entry` and
//! `org-capture-place-plain-text` do. The region Emacs narrows the capture
//! buffer to is what the user edits; [`finish`] puts the edited text back.

use std::sync::LazyLock;

use org_syntax::TodoKeywords;
use regex::Regex;

use crate::date::{iso_week, weekday, DateTime, MONTH_NAMES, WEEKDAY_ABBREVIATIONS, WEEKDAY_NAMES};
use crate::outline::complex_heading_title;
use crate::read_date::{read_date, Settings as ReadDateSettings};
use crate::refile::{heading_at, next_heading, shift_levels, subtree_end, Settings as LevelSettings};
use crate::stamp::format_timestamp;
use crate::todo::headline_level;
use crate::Refusal;

/// What a template expands with, besides the answers to its prompts.
#[derive(Debug, Clone)]
pub struct Context {
    /// The time of the capture (`%t`, `%U`, `%<…>`).
    pub now: DateTime,
    /// The file captured from: its name (`%f`) and full path (`%F`).
    pub file: Option<(String, String)>,
    /// `%i`: the initial content, such as a selected region.
    pub initial: String,
    /// `%a`: a link to where the capture started.
    pub annotation: String,
    /// `%n`: `user-full-name`.
    pub user_name: String,
}

/// A question a template asks (`%^{…}`, `%^t`…).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Question {
    /// `%^{Prompt|default|choice…}`: text, with a default and choices.
    Text { prompt: String, default: String, choices: Vec<String> },
    /// `%^t`, `%^T`, `%^u`, `%^U`: a date, with a time for `T` and `U`,
    /// inactive for `u` and `U`.
    Date { prompt: Option<String>, with_time: bool, inactive: bool },
}

/// A template expanded as far as the answers given allow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Filled {
    /// Expanded; `%?` still marks where the cursor goes.
    Text(String),
    /// The next question to answer.
    Ask(Question),
}

static NON_INTERACTIVE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"%(:[-A-Za-z]+|<([^>\n]+)>|[aAcfFikKlLntTuUx])").unwrap());
static INTERACTIVE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"%\^(?:\{([^}]*)\})?([CgGLptTuU])?").unwrap());
static BACK_REFERENCE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"%\\(\*)?([1-9][0-9]*)").unwrap());
static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\[(.*?)\](\[.*?\])?\]").unwrap());

/// Whether the `%` at `at` is escaped by backslashes before it, and how
/// many of those go (`org-capture-escaped-%`, which removes half of them,
/// rounded up).
fn escaped(text: &str, at: usize) -> (bool, usize) {
    let backslashes = text[..at].bytes().rev().take_while(|b| *b == b'\\').count();
    (backslashes % 2 == 1, backslashes.div_ceil(2))
}

/// Replace the matches of `regexp` in `text` with what `replacement` makes
/// of them, leaving escaped ones as they are but for their escape.
fn replace_unescaped(
    text: &str,
    regexp: &Regex,
    mut replacement: impl FnMut(&regex::Captures) -> Result<Option<String>, Refusal>,
) -> Result<String, Refusal> {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for captures in regexp.captures_iter(text) {
        let found = captures.get(0).expect("whole match");
        let (is_escaped, remove) = escaped(text, found.start());
        out.push_str(&text[last..found.start() - remove]);
        if is_escaped {
            out.push_str(found.as_str());
        } else {
            match replacement(&captures)? {
                Some(value) => out.push_str(&value),
                None => out.push_str(found.as_str()),
            }
        }
        last = found.end();
    }
    out.push_str(&text[last..]);
    Ok(out)
}

/// `format-time-string` for the directives a capture template is likely
/// to use. Others stay as they are.
fn format_time(format: &str, time: DateTime) -> String {
    let date = time.date();
    let (hour, minute) = (time.minute / 60, time.minute % 60);
    let mut out = String::new();
    let mut chars = format.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let Some(directive) = chars.next() else {
            out.push('%');
            break;
        };
        let day = time.day;
        match directive {
            'Y' => out.push_str(&date.year.to_string()),
            'y' => out.push_str(&format!("{:02}", date.year.rem_euclid(100))),
            'm' => out.push_str(&format!("{:02}", date.month)),
            'd' => out.push_str(&format!("{:02}", date.day)),
            'e' => out.push_str(&format!("{:2}", date.day)),
            'H' => out.push_str(&format!("{hour:02}")),
            'I' => out.push_str(&format!("{:02}", if hour % 12 == 0 { 12 } else { hour % 12 })),
            'M' => out.push_str(&format!("{minute:02}")),
            'S' => out.push_str("00"),
            'p' => out.push_str(if hour < 12 { "AM" } else { "PM" }),
            'a' => out.push_str(WEEKDAY_ABBREVIATIONS[weekday(day)]),
            'A' => out.push_str(WEEKDAY_NAMES[weekday(day)]),
            'b' | 'h' => out.push_str(&MONTH_NAMES[(date.month - 1) as usize][..3]),
            'B' => out.push_str(MONTH_NAMES[(date.month - 1) as usize]),
            'u' => out.push_str(&(if weekday(day) == 0 { 7 } else { weekday(day) }).to_string()),
            'w' => out.push_str(&weekday(day).to_string()),
            'V' => out.push_str(&format!("{:02}", iso_week(day).0)),
            'j' => {
                let first = crate::date::Date::new(date.year, 1, 1).to_day();
                out.push_str(&format!("{:03}", day - first + 1));
            }
            'F' => out.push_str(&date.to_string()),
            'R' => out.push_str(&format!("{hour:02}:{minute:02}")),
            'T' => out.push_str(&format!("{hour:02}:{minute:02}:00")),
            '%' => out.push('%'),
            other => {
                out.push('%');
                out.push(other);
            }
        }
    }
    out
}

/// Expand `template` (`org-capture-fill-template`). `answers` answer its
/// questions in order; while one is missing, the result is that question.
/// `%[file]` and `%(…)` are not expanded: there is no Emacs to read the
/// file in, or to run the Lisp.
pub fn fill(
    template: &str,
    context: &Context,
    answers: &[String],
    settings: &ReadDateSettings,
) -> Result<Filled, Refusal> {
    let time = context.now;
    let annotation = if context.annotation == "[[]]" { "" } else { context.annotation.as_str() };
    let link = |with: &str| -> String {
        match LINK.captures(annotation) {
            Some(captures) => {
                let found = captures.get(0).expect("whole match");
                format!(
                    "{}{}{}",
                    &annotation[..found.start()],
                    with.replace("\\1", &captures[1]),
                    &annotation[found.end()..]
                )
            }
            None => annotation.to_string(),
        }
    };
    let (file, full) = context.file.clone().unwrap_or_default();
    // The non-interactive escapes.
    let text = replace_unescaped(template, &NON_INTERACTIVE, |captures| {
        let value = &captures[1];
        let line_start =
            captures.get(0).map_or(0, |found| template[..found.start()].rfind('\n').map_or(0, |at| at + 1));
        Ok(Some(match value.as_bytes()[0] {
            b'<' => format_time(&captures[2], time),
            b':' => String::new(),
            b'i' => {
                // Lines after the first repeat what stands before `%i`.
                let lead = &template[line_start..captures.get(0).expect("whole match").start()];
                context.initial.replace('\n', &format!("\n{lead}"))
            }
            b'a' => annotation.to_string(),
            b'A' => link("[[\\1][%^{Link description}]]"),
            b'l' => link("[[\\1]]"),
            b'L' => link("\\1"),
            b'c' | b'x' | b'k' | b'K' => String::new(),
            b'f' => file.clone(),
            b'F' => full.clone(),
            b'n' => context.user_name.clone(),
            b't' => format_timestamp(time, false, false, ""),
            b'T' => format_timestamp(time, true, false, ""),
            b'u' => format_timestamp(time, false, true, ""),
            b'U' => format_timestamp(time, true, true, ""),
            _ => return Ok(None),
        }))
    })?;
    // The questions, in order.
    let mut asked = 0;
    let mut strings: Vec<String> = Vec::new();
    let mut strings_all: Vec<String> = Vec::new();
    let mut question = None;
    let text = replace_unescaped(&text, &INTERACTIVE, |captures| {
        if question.is_some() {
            return Ok(None);
        }
        let items: Option<Vec<String>> =
            captures.get(1).map(|found| found.as_str().split('|').map(str::to_string).collect());
        let prompt = items.as_ref().and_then(|items| items.first().cloned());
        let default = items.as_ref().and_then(|items| items.get(1).cloned()).unwrap_or_default();
        let choices = items.as_ref().map(|items| items.iter().skip(2).cloned().collect()).unwrap_or_default();
        let key = captures.get(2).map(|found| found.as_str());
        let wanted = match key {
            None => Question::Text {
                prompt: prompt.clone().unwrap_or_else(|| "Enter string".to_string()),
                default: default.clone(),
                choices,
            },
            Some(key @ ("t" | "T" | "u" | "U")) => Question::Date {
                prompt: prompt.clone(),
                with_time: key == "T" || key == "U",
                inactive: key == "u" || key == "U",
            },
            Some(other) => {
                return Err(Refusal::Failed(format!("Capture templates with `%^{other}` are not supported yet")));
            }
        };
        let Some(answer) = answers.get(asked) else {
            question = Some(wanted);
            return Ok(None);
        };
        asked += 1;
        match wanted {
            Question::Text { default, .. } => {
                // An empty answer takes the default.
                let value = if answer.is_empty() { default } else { answer.clone() };
                strings.push(value.clone());
                strings_all.push(value.clone());
                Ok(Some(value))
            }
            Question::Date { with_time, inactive, .. } => {
                let mut read = read_date(answer, None, time, settings)?;
                // `%^T` and `%^U` have a time, the current one when none
                // was typed.
                if with_time && read.time.is_none() {
                    read.time = Some((time.minute / 60, time.minute % 60));
                }
                let stamp = crate::planning::timestamp_for(&read, inactive);
                strings_all.push(stamp.clone());
                Ok(Some(stamp))
            }
        }
    })?;
    if let Some(question) = question {
        return Ok(Filled::Ask(question));
    }
    // `%\N` and `%\*N`: the answers again.
    let text = replace_unescaped(&text, &BACK_REFERENCE, |captures| {
        let index: usize = captures[2].parse().unwrap_or(0);
        let list = if captures.get(1).is_some() { &strings_all } else { &strings };
        Ok(Some(list.get(index.wrapping_sub(1)).cloned().unwrap_or_default()))
    })?;
    Ok(Filled::Text(tidy(&text)))
}

/// No blank lines before the text, and nothing blank after it but one
/// newline, unless it is empty; tabs as spaces.
fn tidy(text: &str) -> String {
    let start = text.find(|c: char| !matches!(c, ' ' | '\t' | '\n')).unwrap_or(text.len());
    let line_start = text[..start].rfind('\n').map_or(0, |at| at + 1);
    let text = &text[line_start..];
    let end = text.trim_end_matches([' ', '\t', '\n']).len();
    let mut out = if text[..end].trim_start_matches([' ', '\t', '\n']).is_empty() {
        String::new()
    } else {
        // To the end of the last line with text.
        format!("{}\n", &text[..end])
    };
    if out.contains('\t') {
        out = untabify(&out);
    }
    out
}

/// Tabs as the spaces up to the next multiple of eight columns.
fn untabify(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let mut column = 0;
        for c in line.chars() {
            if c == '\t' {
                let spaces = 8 - column % 8;
                out.push_str(&" ".repeat(spaces));
                column += spaces;
            } else {
                out.push(c);
                column += unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
            }
        }
    }
    out
}

/// Where a template goes (the target of `org-capture-templates`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// `(file …)`: the top level of the file.
    File,
    /// `(file+headline … "Headline")`: under the first headline with that
    /// title, which is added at the end of the file when there is none.
    Headline(String),
    /// `(file+olp … "A" "B")`: under this outline path.
    Path(Vec<String>),
}

/// What a template makes (its type).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Entry,
    Plain,
}

/// The template's properties that change where it goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Properties {
    /// `:prepend`
    pub prepend: bool,
    /// `:empty-lines`
    pub empty_lines: Option<usize>,
    /// `:empty-lines-before`
    pub empty_lines_before: Option<usize>,
    /// `:empty-lines-after`
    pub empty_lines_after: Option<usize>,
}

/// A template placed in its target file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    /// The file with the template in it.
    pub text: String,
    /// The part the capture buffer shows, as a byte range of `text`.
    pub start: usize,
    pub end: usize,
    /// Where the cursor goes, in `text`.
    pub cursor: usize,
}

impl Placed {
    /// What the capture buffer shows.
    pub fn shown(&self) -> &str {
        &self.text[self.start..self.end]
    }

    /// The file once the capture buffer reads `edited`
    /// (`org-capture-finalize`), and saved as Org mode saves it, with a
    /// newline at the end (`require-final-newline`).
    pub fn finish(&self, edited: &str) -> String {
        let mut text = format!("{}{edited}{}", &self.text[..self.start], &self.text[self.end..]);
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text
    }
}

/// A buffer with a point, for following Emacs's steps.
struct Buffer {
    text: String,
    point: usize,
}

impl Buffer {
    fn bolp(&self) -> bool {
        self.point == 0 || self.text.as_bytes()[self.point - 1] == b'\n'
    }

    fn eobp(&self) -> bool {
        self.point == self.text.len()
    }

    fn insert(&mut self, text: &str) {
        self.text.insert_str(self.point, text);
        self.point += text.len();
    }

    fn line_start(&self, at: usize) -> usize {
        self.text[..at].rfind('\n').map_or(0, |index| index + 1)
    }

    fn line(&self, at: usize) -> &str {
        self.text[at..].split('\n').next().unwrap_or("")
    }

    fn at_heading(&self) -> bool {
        headline_level(self.line(self.line_start(self.point))).is_some()
    }

    /// `org-previous-line-empty-p`: on the first line, the line itself.
    fn previous_line_empty(&self) -> bool {
        if self.point == 0 {
            return false;
        }
        let start = self.line_start(self.point);
        let previous = if start == 0 { 0 } else { self.line_start(start - 1) };
        self.line(previous).trim_start_matches([' ', '\t']).is_empty()
    }

    /// `org-back-over-empty-lines`, as with `org-blank-before-new-entry`
    /// set for headings: back over blanks, to the start of the line after
    /// the last text.
    fn back_over_empty_lines(&mut self) {
        let start = self.text[..self.point].trim_end_matches([' ', '\t', '\n', '\r']).len();
        let after = match self.text[start..].find('\n') {
            Some(newline) => start + newline + 1,
            None => self.text.len(),
        };
        self.point = after.min(self.point);
    }

    /// `org-capture-empty-lines-before`
    fn empty_lines_before(&mut self, n: usize) {
        let pos = self.point;
        self.back_over_empty_lines();
        self.text.replace_range(self.point..pos, "");
        self.insert(&"\n".repeat(n));
    }

    /// `org-capture-empty-lines-after`: the point stays before the lines.
    fn empty_lines_after(&mut self, n: usize) {
        self.back_over_empty_lines();
        // `(while (looking-at "[ \t]*\n") (replace-match ""))`
        loop {
            let rest = &self.text[self.point..];
            let blanks = rest.bytes().take_while(|b| matches!(b, b' ' | b'\t')).count();
            if rest.as_bytes().get(blanks) == Some(&b'\n') {
                self.text.replace_range(self.point..self.point + blanks + 1, "");
            } else {
                break;
            }
        }
        let pos = self.point;
        self.insert(&"\n".repeat(n));
        self.point = pos;
    }

    /// `outline-next-heading`: the next headline after this line, or the
    /// end of the buffer.
    fn next_heading(&mut self) {
        self.point = next_heading(&self.text, self.line_start(self.point)).unwrap_or(self.text.len());
    }
}

/// `org--blank-before-heading-p` with `org-blank-before-new-entry` set to
/// `auto` for headings: whether the heading at or before the point has a
/// blank line before it, or the one after when there is none before.
fn blank_before_heading(buffer: &Buffer) -> bool {
    let text = &buffer.text;
    let heading = match heading_at(text, buffer.point) {
        Some(heading) => heading,
        None => match next_heading(text, buffer.line_start(buffer.point)) {
            // Before the first heading, the first one counts.
            Some(next) if headline_level(buffer.line(0)).is_none() => next,
            _ if headline_level(buffer.line(0)).is_some() => 0,
            _ => return false,
        },
    };
    let probe = Buffer { text: text.clone(), point: heading };
    if heading > 0 {
        return probe.previous_line_empty();
    }
    match next_heading(text, 0) {
        Some(next) => Buffer { text: text.clone(), point: next }.previous_line_empty(),
        None => {
            // At the last line, trailing blanks aside.
            let end = text.trim_end_matches([' ', '\t']).len();
            let at_bol = end == 0 || text.as_bytes()[end - 1] == b'\n';
            at_bol && Buffer { text: text.clone(), point: end }.previous_line_empty()
        }
    }
}

/// Whether `line` is a headline whose title is `title`, as
/// `org-complex-heading-regexp-format` matches it: keyword, priority,
/// `COMMENT` and statistics cookies around it aside.
fn title_matches(line: &str, title: &str, keywords: &TodoKeywords) -> bool {
    static COOKIE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(?:\[[0-9%/]+\] *)*(.*?)(?: *\[[0-9%/]+\])*$").unwrap());
    let Some(level) = headline_level(line) else { return false };
    let Some((from, to)) = complex_heading_title(line, level, keywords) else { return false };
    let found = &line[from..to];
    let found = found.strip_prefix("COMMENT").map_or(found, |rest| {
        if rest.starts_with(' ') {
            rest.trim_start_matches(' ')
        } else {
            found
        }
    });
    COOKIE.captures(found).is_some_and(|captures| &captures[1] == title)
}

/// The headline `target` names in `text`, adding a missing
/// `file+headline` one at the end, or none for the file itself.
fn find_target(text: &mut String, target: &Target, keywords: &TodoKeywords) -> Result<Option<usize>, Refusal> {
    let headlines = |text: &str| -> Vec<usize> {
        let mut out = Vec::new();
        let mut at = 0;
        for line in text.split_inclusive('\n') {
            if headline_level(line.trim_end_matches('\n')).is_some() {
                out.push(at);
            }
            at += line.len();
        }
        out
    };
    match target {
        Target::File => Ok(None),
        Target::Headline(title) => {
            let found = headlines(text).into_iter().find(|at| title_matches(line_of(text, *at), title, keywords));
            if let Some(found) = found {
                return Ok(Some(found));
            }
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            let at = text.len();
            text.push_str(&format!("* {title}\n"));
            Ok(Some(at))
        }
        Target::Path(path) => {
            // `org-find-olp`: each part once at its level, inside the last.
            let (mut lmin, mut lmax) = (1, 1);
            let (mut from, mut to) = (0, text.len());
            let mut found = None;
            for title in path {
                let matches: Vec<usize> = headlines(text)
                    .into_iter()
                    .filter(|at| (from..to).contains(at))
                    .filter(|at| title_matches(line_of(text, *at), title, keywords))
                    .filter(|at| headline_level(line_of(text, *at)).is_some_and(|level| (lmin..=lmax).contains(&level)))
                    .collect();
                match matches.as_slice() {
                    [] => return Err(Refusal::Failed(format!("Heading not found on level {lmax}: {title}"))),
                    [one] => {
                        let level = headline_level(line_of(text, *one)).unwrap_or(1);
                        found = Some(*one);
                        from = *one;
                        to = subtree_end(text, *one);
                        lmin = level + 1;
                        lmax = lmin;
                    }
                    _ => return Err(Refusal::Failed(format!("Heading not unique on level {lmax}: {title}"))),
                }
            }
            Ok(found)
        }
    }
}

fn line_of(text: &str, at: usize) -> &str {
    text[at..].split('\n').next().unwrap_or("")
}

/// Put the expanded `template` into `text` as `kind` at `target`.
pub fn place(
    text: &str,
    keywords: &TodoKeywords,
    target: &Target,
    kind: Kind,
    template: &str,
    properties: &Properties,
    levels: &LevelSettings,
) -> Result<Placed, Refusal> {
    let mut text = text.to_string();
    let heading = find_target(&mut text, target, keywords)?;
    let mut buffer = Buffer { text, point: heading.unwrap_or(0) };
    let (start, end) = match kind {
        Kind::Entry => place_entry(&mut buffer, heading, template, properties, levels)?,
        Kind::Plain => place_plain(&mut buffer, heading, template, properties),
    };
    // `org-capture--position-cursor`: at the first `%?`, which goes.
    let (mut end, mut cursor) = (end, start);
    if let Some(at) = buffer.text[start..end].find("%?") {
        buffer.text.replace_range(start + at..start + at + 2, "");
        cursor = start + at;
        end -= 2;
    }
    Ok(Placed { text: buffer.text, start, end, cursor })
}

/// `org-capture-place-entry`
fn place_entry(
    buffer: &mut Buffer,
    heading: Option<usize>,
    template: &str,
    properties: &Properties,
    levels: &LevelSettings,
) -> Result<(usize, usize), Refusal> {
    let template = if template.split('\n').any(|line| headline_level(line).is_some()) {
        template.to_string()
    } else {
        format!("* {template}")
    };
    if !is_subtree(&template) {
        return Err(Refusal::Failed("Template is not a valid Org entry or tree".to_string()));
    }
    let mut level = 1;
    match heading {
        Some(heading) => {
            level = headline_level(buffer.line(heading)).unwrap_or(0) + 1;
            if properties.prepend {
                buffer.next_heading();
            } else {
                buffer.point = subtree_end(&buffer.text, heading);
            }
        }
        None if properties.prepend => {
            buffer.point = 0;
            if !buffer.at_heading() {
                buffer.next_heading();
            }
        }
        None => buffer.point = buffer.text.len(),
    }
    if !buffer.bolp() {
        buffer.insert("\n");
    }
    let before = properties
        .empty_lines_before
        .or(properties.empty_lines)
        .unwrap_or_else(|| usize::from(blank_before_heading(buffer) && !buffer.previous_line_empty()));
    buffer.empty_lines_before(before);
    let start = buffer.point;
    // `org-paste-subtree` with the level, for yanking: before the next
    // headline unless at one.
    if !(buffer.bolp() && buffer.at_heading()) {
        buffer.next_heading();
        if !buffer.bolp() {
            buffer.insert("\n");
        }
    }
    let mut pasted = shift_levels(&template, level, levels, false);
    if !pasted.ends_with('\n') {
        pasted.push('\n');
    }
    buffer.insert(&pasted);
    let after = properties.empty_lines_after.or(properties.empty_lines).unwrap_or(0);
    buffer.empty_lines_after(after);
    if !buffer.at_heading() || !buffer.bolp() {
        buffer.next_heading();
    }
    let end = if buffer.eobp() { buffer.point } else { buffer.point - 1 };
    Ok((start, end.max(start)))
}

/// `org-capture-place-plain-text`
fn place_plain(buffer: &mut Buffer, heading: Option<usize>, template: &str, properties: &Properties) -> (usize, usize) {
    match heading {
        Some(heading) if properties.prepend => buffer.point = end_of_meta_data(&buffer.text, heading),
        Some(_) => buffer.next_heading(),
        None if properties.prepend => buffer.point = 0,
        None => buffer.point = buffer.text.len(),
    }
    if !buffer.bolp() {
        buffer.insert("\n");
    }
    let before = properties.empty_lines_before.or(properties.empty_lines).unwrap_or(0);
    buffer.empty_lines_before(before);
    let start = buffer.point;
    buffer.insert(template);
    if !buffer.bolp() {
        buffer.insert("\n");
    }
    let end = (buffer.point - 1).max(start);
    let after = properties.empty_lines_after.or(properties.empty_lines).unwrap_or(0);
    buffer.empty_lines_after(after);
    (start, end)
}

/// `org-end-of-meta-data` with FULL `t`: past the planning line, the
/// property drawer, clock lines, other drawers and blank lines.
fn end_of_meta_data(text: &str, heading: usize) -> usize {
    static PLANNING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[ \t]*(?:CLOSED|DEADLINE|SCHEDULED):").unwrap());
    static DRAWER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[ \t]*:((?:\w|[-_])+):[ \t]*$").unwrap());
    static END: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[ \t]*:END:[ \t]*$").unwrap());
    let next_line = |at: usize| text[at..].find('\n').map_or(text.len(), |newline| at + newline + 1);
    let line = |at: usize| text[at..].split('\n').next().unwrap_or("");
    let mut at = next_line(heading);
    if at < text.len() && PLANNING.is_match(line(at)) {
        at = next_line(at);
    }
    // The property drawer, when it is closed.
    if at < text.len() && line(at).trim_matches([' ', '\t']).eq_ignore_ascii_case(":PROPERTIES:") {
        let mut inner = next_line(at);
        while inner < text.len() && !END.is_match(line(inner)) && line(inner).trim_start().starts_with(':') {
            inner = next_line(inner);
        }
        if inner < text.len() && END.is_match(line(inner)) {
            at = next_line(inner);
        }
    }
    if at < text.len() && headline_level(line(at)).is_none() {
        let entry_end = next_heading(text, heading).unwrap_or(text.len());
        while at < text.len() {
            let current = line(at);
            if current.trim_matches([' ', '\t']).is_empty() || current.trim_start().starts_with("CLOCK:") {
                at = next_line(at);
            } else if DRAWER.is_match(current) {
                let mut inner = next_line(at);
                while inner < entry_end && !END.is_match(line(inner)) {
                    inner = next_line(inner);
                }
                if inner < entry_end {
                    at = next_line(inner);
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }
    at.min(text.len())
}

/// `org-kill-is-subtree-p`: the text starts with a headline, after blank
/// lines, and no later headline is above it.
fn is_subtree(text: &str) -> bool {
    let mut lines = text.split('\n').skip_while(|line| line.trim_matches([' ', '\t', '\r']).is_empty());
    let Some(first) = lines.next() else { return false };
    let stars = first.bytes().take_while(|b| *b == b'*').count();
    if stars == 0 || !matches!(first.as_bytes().get(stars), Some(b' ' | b'\t')) {
        return false;
    }
    lines.all(|line| headline_level(line).is_none_or(|level| level >= stars))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::date::Date;

    fn now() -> DateTime {
        DateTime::new(Date::new(2026, 5, 18), 10, 30)
    }

    #[test]
    fn fills_escapes_and_asks() {
        let context = Context {
            now: now(),
            file: Some(("a.org".into(), "/x/a.org".into())),
            initial: String::new(),
            annotation: String::new(),
            user_name: String::new(),
        };
        let settings = ReadDateSettings::default();
        let filled = fill("* TODO %?\n  %U %t %f %<%Y-%m-%d %A> \\%t", &context, &[], &settings).unwrap();
        assert_eq!(
            filled,
            Filled::Text("* TODO %?\n  [2026-05-18 Mon 10:30] <2026-05-18 Mon> a.org 2026-05-18 Monday %t\n".into())
        );
        let template = "* %^{Who|Bob} and %^{What}: %\\1";
        assert_eq!(
            fill(template, &context, &[], &settings).unwrap(),
            Filled::Ask(Question::Text { prompt: "Who".into(), default: "Bob".into(), choices: vec![] })
        );
        assert_eq!(
            fill(template, &context, &["".into()], &settings).unwrap(),
            Filled::Ask(Question::Text { prompt: "What".into(), default: String::new(), choices: vec![] })
        );
        assert_eq!(
            fill(template, &context, &["".into(), "lunch".into()], &settings).unwrap(),
            Filled::Text("* Bob and lunch: Bob\n".into())
        );
    }
}
