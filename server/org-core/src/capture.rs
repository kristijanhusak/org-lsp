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
    /// `%:keyword`: the properties of that link (`org-store-link-plist`),
    /// keys with their colon. `:annotation` and `:initial` are `%a` and
    /// `%i`.
    pub link_props: Vec<(String, String)>,
    /// `%n`: `user-full-name`.
    pub user_name: String,
    /// `%k`: the headline of the entry clocked now, if any.
    pub clock_heading: String,
    /// `%K`: a link to the entry clocked now, if any.
    pub clock_link: String,
    /// `org-tags-column`, for the tags `%^g` puts on a headline.
    pub tags_column: i64,
    /// `%c` and `%x`: what the editor would paste, Emacs's latest kill and
    /// clipboard.
    pub clipboard: String,
}

/// What a template reaches out for, which Emacs reads and runs:
/// the files of `%[file]` and the Lisp of `%(sexp)`.
pub struct Outside<'a> {
    /// The text `%[name]` puts in: the file's, or Org's note that it could
    /// not (`%![could not insert …]`).
    pub insert_file: &'a dyn Fn(&str) -> String,
    /// What `%(sexp)` evaluates to: a string, or nothing.
    pub eval: &'a mut dyn FnMut(&str) -> Result<Option<String>, String>,
}

/// A question a template asks (`%^{…}`, `%^t`…).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Question {
    /// `%^{Prompt|default|choice…}`: text, with a default and choices.
    Text { prompt: String, default: String, choices: Vec<String> },
    /// `%^t`, `%^T`, `%^u`, `%^U`: a date, with a time for `T` and `U`,
    /// inactive for `u` and `U`.
    Date { prompt: Option<String>, with_time: bool, inactive: bool },
    /// `%^g`, `%^G`: tags, separated by colons, from the target file's
    /// tags or (`all_files`) the agenda files'.
    Tags { prompt: Option<String>, all_files: bool },
    /// `%^{NAME}p`: the value of a property, set in the template's entry.
    Property { name: String, default: String },
    /// `%^C`, `%^L`: the initial content or the clipboard, typed
    /// (`Clipboard/kill value: `); a link of it for `%^L`.
    Clipboard { link: bool, default: String, choices: Vec<String> },
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

/// What `%^C` (the text) or `%^L` (a link to it) puts in for `value`.
fn clipboard_value(key: &str, value: &str) -> String {
    if key == "L" && !value.is_empty() {
        format!("[[{value}]]")
    } else if key == "L" {
        String::new()
    } else {
        value.to_string()
    }
}

/// `%[file]`: each name replaced by what `insert` makes of it, the text put
/// in searched again, as Emacs inserts a file before the point.
fn insert_files(template: &str, insert: &dyn Fn(&str) -> String) -> String {
    let mut text = template.to_string();
    let mut from = 0;
    // A file that names itself would never end.
    for _ in 0..1000 {
        let Some(captures) = FILE.captures_at(&text, from) else { break };
        let found = captures.get(0).expect("whole match");
        let (is_escaped, remove) = escaped(&text, found.start());
        let start = found.start() - remove;
        if is_escaped {
            let end = found.end() - remove;
            text.replace_range(start..found.start(), "");
            from = end;
            continue;
        }
        let content = insert(&captures[1]);
        text.replace_range(start..found.end(), &content);
        from = start;
    }
    text
}

/// Mark each `%(sexp)` that is not escaped and reads as a sexp, as
/// `org-capture-expand-embedded-elisp` marks them, by [`SEXP_START`] before
/// it and [`SEXP_END`] after it.
fn mark_sexps(template: &str) -> String {
    let mut out = String::with_capacity(template.len());
    let mut last = 0;
    let mut from = 0;
    while let Some(at) = template[from..].find("%(").map(|at| from + at) {
        let (is_escaped, remove) = escaped(template, at);
        out.push_str(&template[last..at - remove]);
        last = at;
        from = at + 2;
        if is_escaped {
            continue;
        }
        if let Some(end) = sexp_end(template, at + 1) {
            out.push(SEXP_START);
            out.push_str(&template[at..end]);
            out.push(SEXP_END);
            last = end;
            from = end;
        }
    }
    out.push_str(&template[last..]);
    out
}

/// Where the sexp opening at `open` (a `(`) ends, as Emacs Lisp reads it:
/// strings, character literals and comments taken into account. `None`
/// when it does not end.
fn sexp_end(text: &str, open: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut at = open;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at += 1,
            b'?' => {
                // A character: `?a`, `?\(`.
                at += if bytes.get(at + 1) == Some(&b'\\') { 2 } else { 1 };
            }
            b'"' => {
                at += 1;
                while at < bytes.len() && bytes[at] != b'"' {
                    if bytes[at] == b'\\' {
                        at += 1;
                    }
                    at += 1;
                }
            }
            b';' => {
                while at < bytes.len() && bytes[at] != b'\n' {
                    at += 1;
                }
            }
            b'(' | b'[' => depth += 1,
            b')' | b']' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(at + 1);
                }
            }
            _ => {}
        }
        at += 1;
    }
    None
}

/// The marked sexps of `text` replaced by what `outside` evaluates them
/// to (a string, or nothing); without it, left as they are.
fn eval_sexps(text: &str, mut outside: Option<&mut Outside>) -> Result<String, Refusal> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(SEXP_START) {
        out.push_str(&rest[..start]);
        let after = &rest[start + SEXP_START.len_utf8()..];
        let end = after.find(SEXP_END).unwrap_or(after.len());
        let sexp = &after[..end];
        match outside.as_mut() {
            Some(outside) => {
                let lisp = sexp.strip_prefix('%').unwrap_or(sexp);
                if let Some(value) = (outside.eval)(lisp).map_err(Refusal::Failed)? {
                    out.push_str(&value);
                }
            }
            None => out.push_str(sexp),
        }
        rest = after.get(end + SEXP_END.len_utf8()..).unwrap_or("");
    }
    out.push_str(rest);
    Ok(out)
}

/// `format-time-string` for the directives a capture template is likely
/// to use. Others stay as they are.
pub(crate) fn format_time(format: &str, time: DateTime) -> String {
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

/// Where `%(` and the end of its sexp are marked while the template is
/// expanded (`org-embedded-elisp`): characters no template has.
const SEXP_START: char = '\u{E000}';
const SEXP_END: char = '\u{E001}';

static FILE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"%\[(.+)\]").unwrap());

/// Expand `template` (`org-capture-fill-template`). `answers` answer its
/// questions in order; while one is missing, the result is that question.
/// `%[file]` and `%(…)` are left as they are: see [`fill_with`].
pub fn fill(
    template: &str,
    context: &Context,
    answers: &[String],
    settings: &ReadDateSettings,
) -> Result<Filled, Refusal> {
    fill_with(template, context, answers, settings, None)
}

/// [`fill`], with `%[file]` and `%(sexp)` expanded by `outside`: the files
/// first, their text expanded as the template's; then the sexps marked,
/// the escapes inside them expanded with `\` and `"` escaped; then the
/// sexps evaluated, before the questions are asked.
pub fn fill_with(
    template: &str,
    context: &Context,
    answers: &[String],
    settings: &ReadDateSettings,
    outside: Option<Outside>,
) -> Result<Filled, Refusal> {
    fill_counting(template, context, answers, settings, outside).map(|(filled, _)| filled)
}

/// [`fill_with`], with how many of `answers` the template took: for a
/// template expanded after others that take their answers from the same
/// list (org-roam's targets).
pub fn fill_counting(
    template: &str,
    context: &Context,
    answers: &[String],
    settings: &ReadDateSettings,
    mut outside: Option<Outside>,
) -> Result<(Filled, usize), Refusal> {
    let template = match &outside {
        Some(outside) => mark_sexps(&insert_files(template, outside.insert_file)),
        None => template.to_string(),
    };
    let template = template.as_str();
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
        let start = captures.get(0).expect("whole match").start();
        let line_start = template[..start].rfind('\n').map_or(0, |at| at + 1);
        let before = &template[..start];
        let inside = before.matches(SEXP_START).count() > before.matches(SEXP_END).count();
        let value = match value.as_bytes()[0] {
            b'<' => format_time(&captures[2], time),
            b':' => match value {
                ":annotation" => annotation.to_string(),
                ":initial" => context.initial.clone(),
                key => context
                    .link_props
                    .iter()
                    .find(|(name, _)| name == key)
                    .map(|(_, value)| value.clone())
                    .unwrap_or_default(),
            },
            b'i' if inside => context.initial.clone(),
            b'i' => {
                // Lines after the first repeat what stands before `%i`.
                let lead = &template[line_start..start];
                context.initial.replace('\n', &format!("\n{lead}"))
            }
            b'a' => annotation.to_string(),
            b'A' => link("[[\\1][%^{Link description}]]"),
            b'l' => link("[[\\1]]"),
            b'L' => link("\\1"),
            b'c' | b'x' => context.clipboard.clone(),
            b'k' => context.clock_heading.clone(),
            b'K' => context.clock_link.clone(),
            b'f' => file.clone(),
            b'F' => full.clone(),
            b'n' => context.user_name.clone(),
            b't' => format_timestamp(time, false, false, ""),
            b'T' => format_timestamp(time, true, false, ""),
            b'u' => format_timestamp(time, false, true, ""),
            b'U' => format_timestamp(time, true, true, ""),
            _ => return Ok(None),
        };
        // In Lisp, the value is a string's content.
        Ok(Some(if inside { value.replace('\\', "\\\\").replace('"', "\\\"") } else { value }))
    })?;
    // The sexps, evaluated.
    let text = eval_sexps(&text, outside.as_mut())?;
    // The questions, in order.
    let mut asked = 0;
    let mut strings: Vec<String> = Vec::new();
    let mut strings_all: Vec<String> = Vec::new();
    let mut question = None;
    // The lines that got tags, and the properties to set, by line.
    let mut tagged_lines: Vec<usize> = Vec::new();
    let mut properties: Vec<(usize, String, String)> = Vec::new();
    let source = text.clone();
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
            Some(key @ ("g" | "G")) => Question::Tags { prompt: prompt.clone(), all_files: key == "G" },
            Some("p") => Question::Property { name: prompt.clone().unwrap_or_default(), default: default.clone() },
            // The initial content, and what the editor would paste: one
            // of them, asked for when there is a choice.
            Some(key @ ("C" | "L")) => {
                let mut values = vec![context.initial.clone()];
                if !context.clipboard.is_empty() && context.clipboard != context.initial {
                    values.push(context.clipboard.clone());
                }
                if values.len() == 1 {
                    let value = values.remove(0);
                    strings_all.push(value.clone());
                    return Ok(Some(clipboard_value(key, &value)));
                }
                Question::Clipboard { link: key == "L", default: values[0].clone(), choices: values }
            }
            Some(other) => return Err(Refusal::Failed(format!("Unknown template placeholder: \"%^{other}\""))),
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
            Question::Tags { .. } => {
                let tags: Vec<&str> = answer.split(':').map(str::trim).filter(|tag| !tag.is_empty()).collect();
                if tags.is_empty() {
                    return Ok(Some(String::new()));
                }
                let found = captures.get(0).expect("whole match");
                let joined = tags.join(":");
                strings_all.push(format!(":{joined}:"));
                tagged_lines.push(source[..found.start()].matches('\n').count());
                let before = if source[..found.start()].ends_with(':') { "" } else { ":" };
                let after = if source[found.end()..].starts_with(':') { "" } else { ":" };
                Ok(Some(format!("{before}{joined}{after}")))
            }
            Question::Property { name, .. } => {
                let found = captures.get(0).expect("whole match");
                properties.push((source[..found.start()].matches('\n').count(), name, answer.clone()));
                strings_all.push(answer.clone());
                Ok(Some(String::new()))
            }
            Question::Clipboard { link, default, .. } => {
                let value = if answer.is_empty() { default } else { answer.clone() };
                strings_all.push(value.clone());
                Ok(Some(clipboard_value(if link { "L" } else { "C" }, &value)))
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
        return Ok((Filled::Ask(question), asked));
    }
    // Tags put on a headline are aligned (`org-align-tags`).
    let mut text = text;
    for line in tagged_lines {
        let Some(start) = nth_line_start(&text, line) else { continue };
        let end = text[start..].find('\n').map_or(text.len(), |at| start + at);
        if headline_level(&text[start..end]).is_none() {
            continue;
        }
        if let Some((blanks, tags, padding)) = crate::todo::tags_padding(&text[start..end], context.tags_column) {
            text.replace_range(start + blanks..start + tags, &padding);
        }
    }
    // Properties go into the drawer of the entry they are in
    // (`org-set-property`).
    let property_context = crate::properties::Context::default();
    let mut shifts: Vec<(usize, isize)> = Vec::new();
    for (line, name, value) in properties {
        let Some(mut at) = nth_line_start(&text, line) else { continue };
        for (from, delta) in &shifts {
            if at > *from {
                at = (at as isize + delta) as usize;
            }
        }
        let new = crate::properties::put(&text, at, &name, &value, &property_context)?;
        let prefix = text.bytes().zip(new.bytes()).take_while(|(a, b)| a == b).count();
        shifts.push((prefix, new.len() as isize - text.len() as isize));
        text = new;
    }
    // `%\N` and `%\*N`: the answers again.
    let text = replace_unescaped(&text, &BACK_REFERENCE, |captures| {
        let index: usize = captures[2].parse().unwrap_or(0);
        let list = if captures.get(1).is_some() { &strings_all } else { &strings };
        Ok(Some(list.get(index.wrapping_sub(1)).cloned().unwrap_or_default()))
    })?;
    Ok((Filled::Text(tidy(&text)), asked))
}

/// Where line `n` (from 0) of `text` starts.
fn nth_line_start(text: &str, n: usize) -> Option<usize> {
    if n == 0 {
        return Some(0);
    }
    text.match_indices('\n').nth(n - 1).map(|(at, _)| at + 1)
}

/// No blank lines before the text, and nothing blank after it but one
/// newline, unless it is empty; tabs as spaces.
fn tidy(text: &str) -> String {
    let start = text.find(|c: char| !matches!(c, ' ' | '\t' | '\n')).unwrap_or(text.len());
    let line_start = text[..start].rfind('\n').map_or(0, |at| at + 1);
    let text = &text[line_start..];
    let end = text.trim_end_matches([' ', '\t', '\n']).len();
    // To the end of that line: blanks at its end stay.
    let end = text[end..].find('\n').map_or(text.len(), |at| end + at);
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
    /// `(file+regexp … "regexp")`: at the first match of the regexp, its
    /// start with `:prepend`, else its end.
    Regexp(String),
    /// `(file+olp+datetree … "A")`: under the outline path, which may be
    /// empty, the date tree's entry for `date`, made when missing.
    DateTree { path: Vec<String>, date: crate::date::Date, groups: Vec<Group> },
    /// `(clock)`, `(id …)`: under the headline that starts here.
    Heading(usize),
    /// `(function …)`, `(file+function …)`: where the function left the
    /// point, as `file+regexp` at its match.
    Exact(usize),
}

/// A level of a date tree (`:tree-type`): `(year month day)` by default,
/// `week` is `(year week day)`, `month` is `(year month)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Year,
    Quarter,
    Month,
    Week,
    Day,
}

impl Group {
    /// The groups of a `:tree-type`: `day` (or none), `week`, `month`, or
    /// the names of the levels.
    pub fn parse(tree_type: &[String]) -> Result<Vec<Group>, Refusal> {
        match tree_type {
            [] => Ok(vec![Group::Year, Group::Month, Group::Day]),
            [one] if one == "day" => Ok(vec![Group::Year, Group::Month, Group::Day]),
            [one] if one == "week" => Ok(vec![Group::Year, Group::Week, Group::Day]),
            [one] if one == "month" => Ok(vec![Group::Year, Group::Month]),
            names => names
                .iter()
                .map(|name| match name.as_str() {
                    "year" => Ok(Group::Year),
                    "quarter" => Ok(Group::Quarter),
                    "month" => Ok(Group::Month),
                    "week" => Ok(Group::Week),
                    "day" => Ok(Group::Day),
                    other => Err(Refusal::Failed(format!("Unrecognized datetree grouping elements ({other})"))),
                })
                .collect(),
        }
    }
}

/// What a template makes (its type).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Entry,
    Plain,
    Item,
    CheckItem,
    TableLine,
}

/// The template's properties that change where it goes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Properties {
    /// `:prepend`
    pub prepend: bool,
    /// `:empty-lines`
    pub empty_lines: Option<usize>,
    /// `:empty-lines-before`
    pub empty_lines_before: Option<usize>,
    /// `:empty-lines-after`
    pub empty_lines_after: Option<usize>,
    /// `:table-line-pos`: where a table line goes, `II-1` (above the
    /// second rule).
    pub table_line_pos: Option<String>,
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
    /// A table line: the table is aligned when the capture finishes.
    pub align_table: bool,
    /// Where the template went in: `start`, unless the buffer shows more
    /// ([`Placed::unnarrow`]).
    pub begin: usize,
}

impl Placed {
    /// `:unnarrowed`: the capture buffer shows the whole file.
    pub fn unnarrow(&mut self) {
        self.start = 0;
        self.end = self.text.len();
    }

    /// What the capture buffer shows.
    pub fn shown(&self) -> &str {
        &self.text[self.start..self.end]
    }

    /// The file once the capture buffer reads `edited`
    /// (`org-capture-finalize`), and saved as Org mode saves it, with a
    /// newline at the end (`require-final-newline`).
    pub fn finish(&self, edited: &str) -> String {
        let mut text = format!("{}{edited}{}", &self.text[..self.start], &self.text[self.end..]);
        // A table line: the table aligned, when the capture is a table line.
        if self.align_table {
            let start = self.text[..self.begin].rfind('\n').map_or(0, |at| at + 1);
            let is_table = |line: &str| line.trim_start_matches([' ', '\t']).starts_with('|');
            if is_table(line_of(&text, start)) {
                let mut first = start;
                while first > 0 {
                    let previous = text[..first - 1].rfind('\n').map_or(0, |at| at + 1);
                    if !is_table(line_of(&text, previous)) {
                        break;
                    }
                    first = previous;
                }
                let mut end = start;
                while end < text.len() && is_table(line_of(&text, end)) {
                    end = text[end..].find('\n').map_or(text.len(), |at| end + at + 1);
                }
                // With formulas, the table is calculated again, which
                // aligns it (`org-table-fix-formulas` is not ported).
                let recalculated = crate::tblfm::has_formulas(&text, start)
                    .then(|| crate::tblfm::recalculate(&text, start, crate::tblfm::Mode::All, &|_| None).ok())
                    .flatten();
                match recalculated {
                    Some(done) => text = done.text,
                    None => {
                        let lines: Vec<&str> = text[first..end].lines().collect();
                        let mut aligned = crate::format::align_table(&lines).join("\n");
                        if text[first..end].ends_with('\n') {
                            aligned.push('\n');
                        }
                        text.replace_range(first..end, &aligned);
                    }
                }
            }
        }
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text
    }
}

impl Placed {
    /// [`Placed::finish`], then what `org-capture-finalize` does after:
    /// the statistics cookies of the captured entry's parent
    /// (`org-update-parent-todo-statistics`) and of its section
    /// (`org-update-checkbox-count`).
    pub fn finalize(
        &self,
        edited: &str,
        keywords: &TodoKeywords,
        settings: &crate::todo::Settings,
        now: DateTime,
    ) -> String {
        let text = self.finish(edited);
        let Some(heading) = heading_at(&text, self.begin.min(text.len())) else { return text };
        let mut editor = crate::todo::Editor::new(&text, keywords, settings, now);
        editor.heading = heading;
        editor.update_parent_statistics();
        let text = editor.text;
        let heading = heading_at(&text, self.begin.min(text.len())).unwrap_or(heading);
        crate::checkbox::update_counts(&text, keywords, heading, settings.tags_column)
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

/// Where a target puts the template: the headline it is under
/// (`:target-entry-p`), and the place a regexp found (`:exact-position`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Location {
    heading: Option<usize>,
    exact: Option<usize>,
}

fn headlines(text: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        if headline_level(line.trim_end_matches('\n')).is_some() {
            out.push(at);
        }
        at += line.len();
    }
    out
}

/// `org-find-olp`: the headline of `path`, each part once at its level
/// inside the last.
fn find_path(text: &str, path: &[String], keywords: &TodoKeywords) -> Result<Option<usize>, Refusal> {
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

/// Where `target` puts the template in `text` (`org-capture-set-target-location`),
/// adding a missing `file+headline` headline at the end and the missing
/// levels of a date tree.
fn find_target(
    text: &mut String,
    target: &Target,
    keywords: &TodoKeywords,
    prepend: bool,
) -> Result<Location, Refusal> {
    let heading = |at: usize| Location { heading: Some(at), exact: None };
    match target {
        Target::File => Ok(Location::default()),
        Target::Heading(at) => {
            let at = (*at).min(text.len());
            let start = text[..at].rfind('\n').map_or(0, |index| index + 1);
            Ok(heading(start))
        }
        Target::Headline(title) => {
            let found = headlines(text).into_iter().find(|at| title_matches(line_of(text, *at), title, keywords));
            if let Some(found) = found {
                return Ok(heading(found));
            }
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            let at = text.len();
            text.push_str(&format!("* {title}\n"));
            Ok(heading(at))
        }
        Target::Path(path) => Ok(Location { heading: find_path(text, path, keywords)?, exact: None }),
        Target::Regexp(regexp) => {
            let Some(regexp) = crate::agenda::matcher::emacs_buffer_regex(regexp) else {
                return Err(Refusal::Failed(format!("Invalid regexp: {regexp}")));
            };
            let found = regexp.find(text).ok_or_else(|| Refusal::Failed("No match for target regexp".to_string()))?;
            let exact = if prepend { found.start() } else { found.end() };
            let start = text[..exact].rfind('\n').map_or(0, |index| index + 1);
            let at_heading = headline_level(line_of(text, start)).is_some();
            Ok(Location { heading: at_heading.then_some(start), exact: Some(exact) })
        }
        Target::Exact(at) => {
            let exact = (*at).min(text.len());
            let start = text[..exact].rfind('\n').map_or(0, |index| index + 1);
            let at_heading = headline_level(line_of(text, start)).is_some();
            Ok(Location { heading: at_heading.then_some(start), exact: Some(exact) })
        }
        Target::DateTree { path, date, groups } => {
            let base = if path.is_empty() { None } else { find_path(text, path, keywords)? };
            Ok(heading(datetree(text, base, *date, groups, keywords)?))
        }
    }
}

fn line_of(text: &str, at: usize) -> &str {
    text[at..].split('\n').next().unwrap_or("")
}

/// `org-entry-end-position`: the next headline after the line of `at`,
/// or the end.
fn entry_end(text: &str, at: usize) -> usize {
    let start = text[..at].rfind('\n').map_or(0, |index| index + 1);
    next_heading(text, start).unwrap_or(text.len())
}

// ----- date trees

/// The titles of the levels of a date tree for `date`, with the regexp
/// whose first group orders a level's headlines
/// (`org-datetree-find-create-entry`).
fn datetree_levels(date: crate::date::Date, groups: &[Group]) -> Vec<(String, &'static str)> {
    let day = date.to_day();
    let (week, iso_year) = iso_week(day);
    let has = |group: Group| groups.contains(&group);
    let year = if has(Group::Week) { iso_year } else { date.year };
    let month = if has(Group::Week) {
        // The month of the week's Thursday, as for its year.
        let fourth = crate::date::Date::new(iso_year, 1, 4).to_day();
        let monday = fourth - ((weekday(fourth) as i64 + 6) % 7) + (week - 1) * 7;
        crate::date::Date::from_day(monday + 3).month
    } else {
        date.month
    };
    let quarter =
        if has(Group::Week) && !has(Group::Month) { (1 + (week - 1) / 13).min(4) } else { 1 + (month - 1) / 3 };
    let mut out = Vec::new();
    for group in [Group::Year, Group::Quarter, Group::Month, Group::Week, Group::Day] {
        if !has(group) {
            continue;
        }
        out.push(match group {
            Group::Year => (year.to_string(), r"([12][0-9]{3})"),
            Group::Quarter => (format!("{year}-Q{quarter}"), r"([12][0-9]{3}-Q[1-4])"),
            Group::Month => {
                (format!("{year:04}-{month:02} {}", MONTH_NAMES[(month - 1) as usize]), r"([12][0-9]{3}-[01][0-9]) \w+")
            }
            Group::Week => (format!("{iso_year:04}-W{week:02}"), r"([12][0-9]{3}-W[0-5][0-9])"),
            Group::Day => (
                format!("{:04}-{:02}-{:02} {}", date.year, date.month, date.day, WEEKDAY_NAMES[weekday(day)]),
                r"([12][0-9]{3}-[01][0-9]-[0123][0-9]) \w+",
            ),
        });
    }
    out
}

/// The end of the subtree at `heading` as `org-narrow-to-subtree` narrows
/// to it within `limit`: before the newline that ends it, unless it ends
/// the region.
fn narrowed_subtree_end(text: &str, heading: usize, limit: usize) -> usize {
    let end = subtree_end(text, heading).min(limit);
    if end == limit {
        end
    } else {
        end - 1
    }
}

/// `org-datetree-find-create-hierarchy`: the headline of `date`'s entry in
/// the date tree under `base` (or the file's, or the one under the
/// headline with a `DATE_TREE` or `WEEK_TREE` property), the missing
/// levels made.
fn datetree(
    text: &mut String,
    base: Option<usize>,
    date: crate::date::Date,
    groups: &[Group],
    keywords: &TodoKeywords,
) -> Result<usize, Refusal> {
    let (mut start, mut end, mut level) = (0, text.len(), 1);
    let legacy = if groups == [Group::Year, Group::Month, Group::Day] || groups == [Group::Year, Group::Month] {
        Some("DATE_TREE")
    } else if groups == [Group::Year, Group::Week, Group::Day] {
        Some("WEEK_TREE")
    } else {
        None
    };
    let root = match base {
        Some(base) => Some(base),
        None => legacy.and_then(|name| property_heading(text, name)),
    };
    if let Some(root) = root {
        start = root;
        end = narrowed_subtree_end(text, root, text.len());
        level = headline_level(line_of(text, root)).unwrap_or(0) + 1;
    }
    let mut found = start;
    for (title, pattern) in datetree_levels(date, groups) {
        let regex = Regex::new(pattern).expect("a valid regex");
        let key = |title: &str| regex.captures(title).map(|found| found[1].to_string());
        let wanted = key(&title).expect("a date tree title matches its regexp");
        let compare = |line: &str| -> Option<std::cmp::Ordering> {
            let level = headline_level(line)?;
            let (from, to) = complex_heading_title(line, level, keywords)?;
            key(&line[from..to]).map(|other| other.cmp(&wanted))
        };
        // The first headline at the level that is not before the date.
        let sibling = headlines(text)
            .into_iter()
            .filter(|at| (start..end).contains(at))
            .filter(|at| headline_level(line_of(text, *at)) == Some(level))
            .find(|at| compare(line_of(text, *at)).is_some_and(|order| order != std::cmp::Ordering::Less));
        let mut buffer = Buffer { text: std::mem::take(text), point: 0 };
        if let Some(sibling) = sibling {
            if compare(line_of(&buffer.text, sibling)) == Some(std::cmp::Ordering::Equal) {
                *text = buffer.text;
                found = sibling;
                start = sibling;
                end = narrowed_subtree_end(text, sibling, end);
                level += 1;
                continue;
            }
            buffer.point = sibling;
        } else {
            buffer.point = end;
            if !buffer.bolp() {
                buffer.insert("\n");
                end += 1;
            }
        }
        // Blanks before the place go, the new headline goes in.
        let kept = start + buffer.text[start..buffer.point].trim_end_matches([' ', '\t', '\n']).len();
        let removed = buffer.point - kept;
        buffer.text.replace_range(kept..buffer.point, "");
        buffer.point = kept;
        end -= removed;
        let blank = blank_before_heading_in(&buffer.text, buffer.point, start, end);
        let inserted = format!("{}\n{} {title}\n", if blank { "\n" } else { "" }, "*".repeat(level));
        buffer.insert(&inserted);
        end += inserted.len();
        let heading = buffer.line_start(buffer.point - 1);
        *text = buffer.text;
        found = heading;
        start = heading;
        end = narrowed_subtree_end(text, heading, end);
        level += 1;
    }
    Ok(found)
}

/// `org--blank-before-heading-p` with the buffer narrowed to
/// `start..end`: whether the line before the headline at `point` (or the
/// next one, at the start of the region) is blank.
fn blank_before_heading_in(text: &str, point: usize, start: usize, end: usize) -> bool {
    let inside: Vec<usize> = headlines(text).into_iter().filter(|at| (start..end).contains(at)).collect();
    let line_start = text[..point].rfind('\n').map_or(0, |at| at + 1).max(start);
    let heading = match inside.iter().rev().find(|at| **at <= line_start) {
        Some(heading) => *heading,
        None => match inside.iter().find(|at| **at > line_start) {
            Some(next) => *next,
            None => return false,
        },
    };
    let blank_before = |at: usize| {
        at > start && {
            let previous = text[..at - 1].rfind('\n').map_or(0, |index| index + 1);
            line_of(text, previous).trim_matches([' ', '\t']).is_empty()
        }
    };
    if heading > start {
        return blank_before(heading);
    }
    match inside.iter().find(|at| **at > heading) {
        Some(next) => blank_before(*next),
        None => false,
    }
}

/// `org-find-property`: the first headline with the property `name`.
fn property_heading(text: &str, name: &str) -> Option<usize> {
    let wanted = format!(":{}:", name.to_lowercase());
    let mut at = 0;
    let mut in_drawer = false;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.eq_ignore_ascii_case(":PROPERTIES:") {
            in_drawer = true;
        } else if trimmed.eq_ignore_ascii_case(":END:") {
            in_drawer = false;
        } else if in_drawer && trimmed.to_lowercase().starts_with(&wanted) {
            return heading_at(text, at);
        }
        at += line.len();
    }
    None
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
    let location = find_target(&mut text, target, keywords, properties.prepend)?;
    let point = location.exact.or(location.heading).unwrap_or(0);
    let mut buffer = Buffer { text, point };
    let (start, end) = match kind {
        Kind::Entry => place_entry(&mut buffer, location.heading, template, properties, levels)?,
        Kind::Plain => place_plain(&mut buffer, location, template, properties),
        Kind::Item | Kind::CheckItem => place_item(&mut buffer, location, template, properties)?,
        Kind::TableLine => place_table_line(&mut buffer, location, template, properties)?,
    };
    // `org-capture--position-cursor`: at the first `%?`, which goes.
    let (mut end, mut cursor) = (end, start);
    if let Some(at) = buffer.text[start..end].find("%?") {
        buffer.text.replace_range(start + at..start + at + 2, "");
        cursor = start + at;
        end -= 2;
    }
    Ok(Placed { text: buffer.text, start, end, cursor, align_table: kind == Kind::TableLine, begin: start })
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
fn place_plain(buffer: &mut Buffer, location: Location, template: &str, properties: &Properties) -> (usize, usize) {
    match (location.heading, location.exact) {
        (Some(heading), _) if properties.prepend => buffer.point = end_of_meta_data(&buffer.text, heading, true),
        (Some(_), _) => buffer.next_heading(),
        (None, Some(exact)) => buffer.point = exact,
        (None, None) if properties.prepend => buffer.point = 0,
        (None, None) => buffer.point = buffer.text.len(),
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

/// `org-remove-indentation`: the indentation the lines share, gone.
fn remove_indentation(text: &str) -> String {
    let common = text.lines().filter(|line| !line.trim().is_empty()).map(crate::list::indentation).min().unwrap_or(0);
    if common == 0 {
        return text.to_string();
    }
    text.split_inclusive('\n')
        .map(|line| {
            let mut column = 0;
            let mut cut = 0;
            for (index, c) in line.char_indices() {
                if column >= common || !matches!(c, ' ' | '\t') {
                    cut = index;
                    break;
                }
                column = if c == '\t' { column / 8 * 8 + 8 } else { column + 1 };
                cut = index + c.len_utf8();
            }
            &line[cut..]
        })
        .collect()
}

/// `org-item-beginning-re`
static ITEM_START: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^[ \t]*(?:[-+*]|[0-9]+[.)])(?:[ \t]+|$)").unwrap());

/// `org-capture-place-item`
fn place_item(
    buffer: &mut Buffer,
    location: Location,
    template: &str,
    properties: &Properties,
) -> Result<(usize, usize), Refusal> {
    static ITEM_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\A[ \t]*(?:[-+*]|[0-9]+[.)])(?:[ \t]+|$)").unwrap());
    static ORDERED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\A[A-Za-z0-9][.)]").unwrap());
    let prepend = properties.prepend;
    let mut template = remove_indentation(template);
    if !ITEM_RE.is_match(&template) {
        template = format!("- {}", template.split('\n').collect::<Vec<_>>().join("\n  "));
    }
    // Where to look for a list.
    let (beg, end) = match (location.exact, location.heading) {
        (Some(exact), _) => (buffer.line_start(exact), entry_end(&buffer.text, exact)),
        (None, Some(heading)) => {
            let next = buffer.text[heading..].find('\n').map_or(buffer.text.len(), |at| heading + at + 1);
            (next, entry_end(&buffer.text, heading))
        }
        (None, None) => (0, buffer.text.len()),
    };
    // The first plain list there.
    let mut list: Option<crate::list::Struct> = None;
    for found in ITEM_START.find_iter(&buffer.text[beg..end.max(beg)]) {
        let at = beg + found.start();
        if headline_level(line_of(&buffer.text, at)).is_some() || !crate::list::at_item(&buffer.text, at) {
            continue;
        }
        let structure = crate::list::structure(&buffer.text, at);
        if structure.is_empty() {
            continue;
        }
        list = Some(structure);
        break;
    }
    match &list {
        Some(items) => {
            buffer.point = if prepend {
                items[0].pos
            } else {
                let last = items.iter().map(|item| item.end).max().unwrap_or(items[0].pos);
                // Before the blank lines after the list.
                let mut at = last;
                while at > items[0].pos {
                    let previous = buffer.line_start(at - 1);
                    if line_of(&buffer.text, previous).trim().is_empty() {
                        at = previous;
                    } else {
                        break;
                    }
                }
                at
            };
        }
        None => {
            buffer.point = beg;
            let before_first = heading_at(&buffer.text, beg).is_none();
            buffer.point = if !prepend {
                end
            } else if before_first {
                beg
            } else {
                let heading = heading_at(&buffer.text, beg).expect("a heading");
                end_of_meta_data(&buffer.text, heading, false).max(beg)
            };
        }
    }
    let item = list.is_some();
    if !buffer.bolp() {
        buffer.insert("\n");
    }
    if !(item && prepend) {
        let lines = if item {
            properties.empty_lines_before.or(properties.empty_lines).unwrap_or(0).min(1)
        } else {
            properties.empty_lines_before.or(properties.empty_lines).unwrap_or(0)
        };
        buffer.empty_lines_before(lines);
    }
    let start = buffer.line_start(buffer.point);
    let inserted = format!("{}\n", template.trim_matches([' ', '\t', '\n', '\r']));
    buffer.insert(&inserted);
    let mut end = buffer.point;
    if let Some(items) = &list {
        let indent = items[0].ind.max(0) as usize;
        if indent > 0 {
            let mut at = start;
            while at < end {
                buffer.text.insert_str(at, &" ".repeat(indent));
                end += indent;
                at = buffer.text[at..].find('\n').map_or(buffer.text.len(), |newline| at + newline + 1);
            }
        }
        // A new first item takes the list's kind of bullet.
        if prepend {
            let ordered = items[0].bullet.trim().starts_with(|c: char| c.is_ascii_digit());
            if ordered != ORDERED.is_match(&template) {
                let (text, marker) =
                    crate::list::set_bullet(&buffer.text, start, if ordered { "1." } else { "-" }, end)?;
                buffer.text = text;
                end = marker;
            }
        }
        let (text, marker) = crate::list::repair_with_marker(&buffer.text, start, end)?;
        buffer.text = text;
        end = marker;
    }
    buffer.point = end;
    if !(item && !prepend) {
        let lines = if item {
            properties.empty_lines_after.or(properties.empty_lines).unwrap_or(0).min(1)
        } else {
            properties.empty_lines_after.or(properties.empty_lines).unwrap_or(0)
        };
        buffer.empty_lines_after(lines);
    }
    Ok((start, (end - 1).max(start)))
}

/// `org-table-dataline-regexp`, `org-table-hline-regexp`
fn is_table_line(line: &str) -> bool {
    line.trim_start_matches([' ', '\t']).starts_with('|')
}

fn is_hline(line: &str) -> bool {
    line.trim_start_matches([' ', '\t']).starts_with("|-")
}

/// `org-capture-place-table-line`
fn place_table_line(
    buffer: &mut Buffer,
    location: Location,
    template: &str,
    properties: &Properties,
) -> Result<(usize, usize), Refusal> {
    static POSITION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(I+)([-+][0-9]+)").unwrap());
    let template = template.trim_matches([' ', '\t', '\n', '\r']);
    let starts_bare = template.trim_start_matches([' ', '\t']).chars().next().is_some_and(|c| c != '|');
    let text = if starts_bare { format!("| {template}") } else { format!("{template}\n") };
    let (beg, end) = match (location.exact, location.heading) {
        (Some(exact), _) => (buffer.line_start(exact), entry_end(&buffer.text, exact)),
        (None, None) => (0, buffer.text.len()),
        (None, Some(heading)) => {
            let next = buffer.text[heading..].find('\n').map_or(buffer.text.len(), |at| heading + at + 1);
            (next, next_heading(&buffer.text, heading).unwrap_or(buffer.text.len()))
        }
    };
    // The first table there, or a new one.
    let mut table: Option<(usize, usize)> = None;
    let mut at = beg;
    while at < end {
        let line = line_of(&buffer.text, at);
        let data = is_table_line(line) && !is_hline(line);
        if data && !line.trim_start().starts_with("+-") {
            let mut first = at;
            while first > 0 {
                let previous = buffer.line_start(first - 1);
                if !is_table_line(line_of(&buffer.text, previous)) {
                    break;
                }
                first = previous;
            }
            let mut last = at;
            while last < buffer.text.len() && is_table_line(line_of(&buffer.text, last)) {
                last = buffer.text[last..].find('\n').map_or(buffer.text.len(), |newline| last + newline + 1);
            }
            table = Some((first, last));
            break;
        }
        at = buffer.text[at..].find('\n').map_or(buffer.text.len(), |newline| at + newline + 1);
    }
    let (first, last) = match table {
        Some(found) => found,
        None => {
            buffer.point = end;
            if !buffer.bolp() {
                buffer.insert("\n");
            }
            let origin = buffer.point;
            buffer.insert("|   |\n|---|\n");
            (origin, buffer.point)
        }
    };
    // The place in the table.
    let lines: Vec<usize> = {
        let mut out = Vec::new();
        let mut at = first;
        while at < last {
            out.push(at);
            at = buffer.text[at..].find('\n').map_or(last, |newline| at + newline + 1);
        }
        out
    };
    let position = properties.table_line_pos.as_deref().and_then(|spec| POSITION.captures(spec));
    buffer.point = if let Some(found) = position {
        let hlines: Vec<usize> =
            lines.iter().enumerate().filter(|(_, at)| is_hline(line_of(&buffer.text, **at))).map(|(n, _)| n).collect();
        let spec = properties.table_line_pos.clone().unwrap_or_default();
        let invalid = || Refusal::Failed(format!("Invalid table line specification \"{spec}\""));
        let line = *hlines.get(found[1].len() - 1).ok_or_else(invalid)?;
        let delta: i64 = found[2].parse().map_err(|_| invalid())?;
        let target = line as i64 + delta + if delta < 0 { 0 } else { -1 } + 1;
        let target = target.clamp(0, lines.len() as i64) as usize;
        lines.get(target).copied().unwrap_or(last)
    } else if properties.prepend {
        match lines.iter().position(|at| is_hline(line_of(&buffer.text, *at))) {
            None => first,
            Some(hline) => {
                lines[hline + 1..].iter().copied().find(|at| !is_hline(line_of(&buffer.text, *at))).unwrap_or(last)
            }
        }
    } else {
        last
    };
    if !buffer.bolp() {
        buffer.insert("\n");
    }
    let start = buffer.point;
    buffer.insert(&text);
    let end = buffer.point;
    Ok((start, (end - 1).max(start)))
}

/// `org-end-of-meta-data`: past the planning line and the property
/// drawer, and with `full` clock lines, other drawers and blank lines.
pub(crate) fn end_of_meta_data(text: &str, heading: usize, full: bool) -> usize {
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
    if full && at < text.len() && headline_level(line(at)).is_none() {
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
    fn fills_the_properties_of_the_link() {
        let context = Context {
            now: now(),
            file: None,
            initial: "start".into(),
            annotation: "[[id:1][Head]]".into(),
            link_props: vec![(":link".into(), "id:1".into()), (":type".into(), "id".into())],
            user_name: String::new(),
            clock_heading: String::new(),
            clock_link: String::new(),
            tags_column: -77,
            clipboard: String::new(),
        };
        let settings = ReadDateSettings::default();
        let filled = fill("%:type %:link %:annotation %:initial [%:description] [%:Type]", &context, &[], &settings);
        assert_eq!(filled.unwrap(), Filled::Text("id id:1 [[id:1][Head]] start [] []\n".into()));
    }

    #[test]
    fn fills_escapes_and_asks() {
        let context = Context {
            now: now(),
            file: Some(("a.org".into(), "/x/a.org".into())),
            initial: String::new(),
            annotation: String::new(),
            link_props: Vec::new(),
            user_name: String::new(),
            clock_heading: String::new(),
            clock_link: String::new(),
            tags_column: -77,
            clipboard: String::new(),
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
