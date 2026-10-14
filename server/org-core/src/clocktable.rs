//! Clock tables: the `clocktable` dynamic block (`#+BEGIN: clocktable`),
//! which `org-clock-report` (`C-c C-x C-r`) inserts or updates and
//! `org-dblock-update` (`C-c C-x C-u`, `C-c C-c` on the block) fills in
//! again: `org-dblock-write:clocktable`, `org-clock-get-table-data`,
//! `org-clocktable-write-default` and `org-clocktable-steps`, run on the
//! text as Emacs runs them on a buffer. `S-<arrows>` on the `#+BEGIN:`
//! line move its `:block` (`org-clocktable-shift`).

use std::sync::LazyLock;

use org_syntax::TodoKeywords;
use regex::{Regex, RegexBuilder};

use crate::agenda::matcher::{Clock as MatcherClock, Matcher};
use crate::agenda::report::{display_format, heading_title, normalize, shorten, without_cookies};
use crate::clock::{clock_minutes, duration, minutes_of, parse_line};
use crate::clock_range::special_range;
use crate::date::{Date, DateTime, Day};
use crate::format::align_table;
use crate::outline::{Inheritance, Outline};
use crate::stamp::format_timestamp;
use crate::timestr::parse_time_string;
use crate::todo::headline_level;

/// What the clock tables need to know beyond the file.
#[derive(Debug, Clone)]
pub struct Settings {
    pub now: DateTime,
    /// `org-extend-today-until`
    pub extend_today_until: i64,
    /// `org-clock-clocktable-default-properties`, as the text of a plist
    /// (`:maxlevel 2 :scope file`).
    pub default_properties: String,
    /// `org-clock-total-time-cell-format`
    pub total_time_cell_format: String,
    /// `org-clock-file-time-cell-format`
    pub file_time_cell_format: String,
    pub keywords: TodoKeywords,
    pub inheritance: Inheritance,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            now: DateTime { day: Date::new(2026, 1, 1).to_day(), minute: 0 },
            extend_today_until: 0,
            default_properties: ":maxlevel 2".to_string(),
            total_time_cell_format: "*%s*".to_string(),
            file_time_cell_format: "*%s*".to_string(),
            keywords: TodoKeywords::default(),
            inheritance: Inheritance::default(),
        }
    }
}

/// The files beyond the one with the table.
pub trait Files {
    /// `org-agenda-files`, as absolute paths.
    fn agenda_files(&self) -> Vec<String>;
    /// The files `path` archives to that exist (`org-all-archive-files`),
    /// `text` being its text.
    fn archive_files(&self, path: &str, text: &str) -> Vec<String>;
    /// The text of the file at `path`, an absolute path.
    fn read(&self, path: &str) -> Option<String>;
}

/// No files beyond the one with the table.
pub struct NoFiles;

impl Files for NoFiles {
    fn agenda_files(&self) -> Vec<String> {
        Vec::new()
    }
    fn archive_files(&self, _: &str, _: &str) -> Vec<String> {
        Vec::new()
    }
    fn read(&self, _: &str) -> Option<String> {
        None
    }
}

/// The file with the table and what is around it.
pub struct Context<'a> {
    /// The file's absolute path (`buffer-file-name`), when it has one.
    pub path: Option<&'a str>,
    pub settings: &'a Settings,
    pub files: &'a dyn Files,
}

/// The text after a command, and the cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Done {
    pub text: String,
    pub cursor: usize,
    /// What Emacs says when it is done, when it says something worth
    /// showing.
    pub message: Option<String>,
}

// ----- Emacs Lisp values, as `read` reads the parameters of a block

/// A value of Emacs Lisp.
#[derive(Debug, Clone, PartialEq)]
enum Lisp {
    Symbol(String),
    Str(String),
    Int(i64),
    Float(f64),
    List(Vec<Lisp>),
    /// A dotted pair, `(2 . ?a)`.
    Cons(Box<Lisp>, Box<Lisp>),
}

const EOF: &str = "End of file during parsing";

impl Lisp {
    fn is_nil(&self) -> bool {
        match self {
            Lisp::Symbol(name) => name == "nil",
            Lisp::List(items) => items.is_empty(),
            _ => false,
        }
    }

    fn symbol(&self) -> Option<&str> {
        match self {
            Lisp::Symbol(name) if name != "nil" => Some(name),
            _ => None,
        }
    }

    /// `prin1-to-string`
    fn print(&self) -> String {
        match self {
            Lisp::Symbol(name) => name.clone(),
            Lisp::Str(text) => format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\"")),
            Lisp::Int(n) => n.to_string(),
            Lisp::Float(f) => {
                if f.fract() == 0.0 && f.is_finite() {
                    format!("{f:.1}")
                } else {
                    f.to_string()
                }
            }
            Lisp::List(items) if items.len() == 2 && items[0] == Lisp::Symbol("quote".to_string()) => {
                format!("'{}", items[1].print())
            }
            Lisp::List(items) if items.is_empty() => "nil".to_string(),
            Lisp::List(items) => format!("({})", items.iter().map(Lisp::print).collect::<Vec<_>>().join(" ")),
            Lisp::Cons(car, cdr) => {
                let mut parts = vec![car.print()];
                let mut tail = cdr.as_ref();
                loop {
                    match tail {
                        Lisp::Cons(car, cdr) => {
                            parts.push(car.print());
                            tail = cdr;
                        }
                        Lisp::List(items) => {
                            parts.extend(items.iter().map(Lisp::print));
                            break;
                        }
                        other => {
                            parts.push(".".to_string());
                            parts.push(other.print());
                            break;
                        }
                    }
                }
                format!("({})", parts.join(" "))
            }
        }
    }
}

/// `read` over the text of a list's elements.
struct Reader {
    chars: Vec<char>,
    at: usize,
    /// How deep in lists the reader is: a list nested past `NESTING_MAX`
    /// is refused rather than read until the stack runs out.
    depth: usize,
}

/// The deepest a list or form read from a document may nest.
const NESTING_MAX: usize = 200;

impl Reader {
    fn new(text: &str) -> Reader {
        Reader { chars: text.chars().collect(), at: 0, depth: 0 }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn skip_blanks(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.at += 1;
            } else if c == ';' {
                while self.peek().is_some_and(|c| c != '\n') {
                    self.at += 1;
                }
            } else {
                break;
            }
        }
    }

    fn read_all(&mut self) -> Result<Vec<Lisp>, String> {
        let mut items = Vec::new();
        loop {
            self.skip_blanks();
            if self.peek().is_none() {
                return Ok(items);
            }
            items.push(self.read()?);
        }
    }

    fn read(&mut self) -> Result<Lisp, String> {
        if self.depth == NESTING_MAX {
            return Err("Lisp nesting exceeds `max-lisp-eval-depth'".to_string());
        }
        self.depth += 1;
        let read = self.read_form();
        self.depth -= 1;
        read
    }

    fn read_form(&mut self) -> Result<Lisp, String> {
        self.skip_blanks();
        let Some(c) = self.peek() else { return Err(EOF.to_string()) };
        match c {
            '(' | '[' => {
                self.at += 1;
                let close = if c == '(' { ')' } else { ']' };
                let mut items = Vec::new();
                loop {
                    self.skip_blanks();
                    match self.peek() {
                        None => return Err(EOF.to_string()),
                        Some(c) if c == close => {
                            self.at += 1;
                            return Ok(Lisp::List(items));
                        }
                        Some('.')
                            if c == '('
                                && !items.is_empty()
                                && self.chars.get(self.at + 1).is_none_or(|c| c.is_whitespace() || *c == '(') =>
                        {
                            self.at += 1;
                            let tail = self.read()?;
                            self.skip_blanks();
                            if self.peek() != Some(')') {
                                return Err("Invalid read syntax: . in wrong context".to_string());
                            }
                            self.at += 1;
                            let mut out = tail;
                            for item in items.into_iter().rev() {
                                out = Lisp::Cons(Box::new(item), Box::new(out));
                            }
                            return Ok(out);
                        }
                        Some(_) => items.push(self.read()?),
                    }
                }
            }
            ')' | ']' => Err(format!("Invalid read syntax: {c}")),
            '"' => {
                self.at += 1;
                let mut out = String::new();
                loop {
                    let Some(c) = self.peek() else { return Err(EOF.to_string()) };
                    self.at += 1;
                    match c {
                        '"' => return Ok(Lisp::Str(out)),
                        '\\' => {
                            let Some(next) = self.peek() else { return Err(EOF.to_string()) };
                            self.at += 1;
                            match next {
                                'n' => out.push('\n'),
                                't' => out.push('\t'),
                                'r' => out.push('\r'),
                                'e' => out.push('\u{1b}'),
                                's' => out.push(' '),
                                '\n' => {}
                                other => out.push(other),
                            }
                        }
                        other => out.push(other),
                    }
                }
            }
            '?' => {
                self.at += 1;
                let Some(c) = self.peek() else { return Err(EOF.to_string()) };
                self.at += 1;
                let value = if c == '\\' {
                    let Some(next) = self.peek() else { return Err(EOF.to_string()) };
                    self.at += 1;
                    match next {
                        'n' => '\n',
                        't' => '\t',
                        's' => ' ',
                        'e' => '\u{1b}',
                        other => other,
                    }
                } else {
                    c
                };
                Ok(Lisp::Int(value as i64))
            }
            '\'' => {
                self.at += 1;
                Ok(Lisp::List(vec![Lisp::Symbol("quote".to_string()), self.read()?]))
            }
            _ => {
                let mut atom = String::new();
                let mut escaped = false;
                while let Some(c) = self.peek() {
                    if c.is_whitespace() || "()[]\";'".contains(c) {
                        break;
                    }
                    self.at += 1;
                    if c == '\\' {
                        if let Some(next) = self.peek() {
                            self.at += 1;
                            atom.push(next);
                            escaped = true;
                        }
                    } else {
                        atom.push(c);
                    }
                }
                Ok(if escaped { Lisp::Symbol(atom) } else { atom_value(atom) })
            }
        }
    }
}

/// A number when the atom reads as one, a symbol otherwise.
fn atom_value(atom: String) -> Lisp {
    static INTEGER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[-+]?[0-9]+\.?$").expect("a valid regex"));
    static FLOAT: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^[-+]?(?:[0-9]*\.[0-9]+(?:e[-+]?[0-9]+)?|[0-9]+(?:\.[0-9]*)?e[-+]?[0-9]+)$")
            .expect("a valid regex")
    });
    if INTEGER.is_match(&atom) {
        if let Ok(n) = atom.trim_end_matches('.').trim_start_matches('+').parse() {
            return Lisp::Int(n);
        }
    }
    if FLOAT.is_match(&atom) {
        if let Ok(f) = atom.parse() {
            return Lisp::Float(f);
        }
    }
    Lisp::Symbol(atom)
}

/// A property list, as `org-combine-plists` builds one: a key put again
/// keeps its place and takes the new value.
#[derive(Debug, Clone, Default)]
struct Plist(Vec<(Lisp, Lisp)>);

impl Plist {
    fn read(text: &str) -> Result<Plist, String> {
        let items = Reader::new(text).read_all()?;
        let mut plist = Plist::default();
        let mut items = items.into_iter();
        while let Some(key) = items.next() {
            let value = items.next().unwrap_or(Lisp::Symbol("nil".to_string()));
            plist.put(key, value);
        }
        Ok(plist)
    }

    fn put(&mut self, key: Lisp, value: Lisp) {
        let same = |other: &Lisp| match (&key, other) {
            (Lisp::Symbol(a), Lisp::Symbol(b)) => a == b,
            (Lisp::Int(a), Lisp::Int(b)) => a == b,
            _ => false,
        };
        match self.0.iter_mut().find(|(other, _)| same(other)) {
            Some(slot) => slot.1 = value,
            None => self.0.push((key, value)),
        }
    }

    fn set(&mut self, key: &str, value: Lisp) {
        self.put(Lisp::Symbol(format!(":{key}")), value);
    }

    fn combine(&self, other: &Plist) -> Plist {
        let mut out = self.clone();
        for (key, value) in &other.0 {
            out.put(key.clone(), value.clone());
        }
        out
    }

    /// The value of `:key`, unless it is missing or nil.
    fn get(&self, key: &str) -> Option<&Lisp> {
        let key = format!(":{key}");
        self.0.iter().find(|(other, _)| other.symbol() == Some(&key)).map(|(_, value)| value).filter(|v| !v.is_nil())
    }

    fn flag(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    fn int(&self, key: &str) -> Option<i64> {
        match self.get(key) {
            Some(Lisp::Int(n)) => Some(*n),
            _ => None,
        }
    }

    fn string(&self, key: &str) -> Option<&str> {
        match self.get(key) {
            Some(Lisp::Str(text)) => Some(text),
            _ => None,
        }
    }

    /// The name of a block's period, a symbol or a string (`:block
    /// today`, `:block 2026` reads as a number).
    fn block(&self) -> Option<String> {
        match self.get("block")? {
            Lisp::Symbol(name) => Some(name.clone()),
            Lisp::Str(text) => Some(text.clone()),
            Lisp::Int(n) => Some(n.to_string()),
            other => Some(other.print()),
        }
    }
}

/// `org-clocktable-defaults`
fn defaults() -> Plist {
    Plist::read(
        ":maxlevel 2 :lang \"en\" :scope file :block nil :wstart 1 :mstart 1 :tstart nil :tend nil :step nil \
         :stepskip0 nil :fileskip0 nil :tags nil :match nil :emphasize nil :link nil :narrow 40! :indent t \
         :filetitle nil :hidefiles nil :formula nil :timestamp nil :level nil :tcolumns nil :formatter nil",
    )
    .expect("valid defaults")
}

// ----- the text as a buffer

/// `^[ \t]*#\+BEGIN:[ \t]+NAME…`: `org-dblock-start-re`.
static START: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"^[ \t]*#\+(?:BEGIN|begin):[ \t]+(\S+)(?:[ \t]+(.*))?")
        .case_insensitive(true)
        .multi_line(true)
        .build()
        .expect("a valid regex")
});

/// `org-dblock-end-re`
static END: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"^[ \t]*#\+(?:END|end)(?:[: \t\r\n]|$)")
        .case_insensitive(true)
        .multi_line(true)
        .build()
        .expect("a valid regex")
});

fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |index| index + 1)
}

fn line_end(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |index| at + index)
}

/// The start of the next line, or the end of the text.
fn next_line(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |index| at + index + 1)
}

/// `current-column`, tabs eight wide.
fn column(text: &str, at: usize) -> usize {
    let mut column = 0;
    for c in text[line_start(text, at)..at].chars() {
        column = if c == '\t' { column / 8 * 8 + 8 } else { column + crate::text::char_width(c) };
    }
    column
}

/// `org-beginning-of-dblock`: the start of the block `at` is in, from
/// its `#+BEGIN:` line up to the colon of its `#+END:`.
fn beginning_of_block(text: &str, at: usize) -> Option<usize> {
    let eol = line_end(text, at);
    let begin = START.find_iter(&text[..eol]).map(|found| found.start()).last()?;
    let end = END.find_at(text, begin)?;
    (end.end() > at).then_some(begin)
}

/// The name of the dynamic block `at` is in (`clocktable`, `columnview`).
pub fn block_name(text: &str, at: usize) -> Option<String> {
    let begin = beginning_of_block(text, at)?;
    START.captures_at(text, begin).map(|found| found[1].to_string())
}

/// Whether the text has a dynamic block that is not a clock table, which
/// org-lsp leaves to Emacs.
pub fn has_other_blocks(text: &str) -> bool {
    START.captures_iter(text).any(|found| !found[1].eq_ignore_ascii_case("clocktable"))
}

/// Whether `at` is on the `#+BEGIN:` or `#+END:` line of a dynamic block,
/// where `C-c C-c` updates it.
pub fn at_block_line(text: &str, at: usize) -> bool {
    let start = line_start(text, at);
    let line = &text[start..line_end(text, at)];
    if START.is_match(line) {
        return END.find_at(text, start).is_some();
    }
    END.is_match(line) && beginning_of_block(text, at).is_some()
}

/// Whether `at` is in a dynamic block, from its `#+BEGIN:` line to its
/// `#+END:` line, where `org.dblock.update` works.
pub fn in_block(text: &str, at: usize) -> bool {
    beginning_of_block(text, at).is_some()
}

/// Whether `at` is on the `#+BEGIN:` line of a clock table, where
/// `S-<arrows>` shift its block; not on a timestamp there, which they
/// change first.
pub fn at_clocktable_begin(text: &str, at: usize) -> bool {
    static BEGIN: LazyLock<Regex> = LazyLock::new(|| {
        RegexBuilder::new(r"^[ \t]*#\+BEGIN:[ \t]+clocktable\b").case_insensitive(true).build().expect("a valid regex")
    });
    static STAMP: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"[<\[][0-9]{4}-[0-9]{2}-[0-9]{2}[^>\]\n]*[>\]]").expect("a valid regex"));
    let start = line_start(text, at);
    let line = &text[start..line_end(text, at)];
    BEGIN.is_match(line)
        && !STAMP.find_iter(line).any(|found| (start + found.start()..=start + found.end()).contains(&at))
}

/// `org-update-dblock` with the block's `#+BEGIN:` line at `begin`. Gives
/// the text, and the change in length around the block's end line.
fn update_block(text: &str, begin: usize, context: &Context) -> Result<String, String> {
    let found = START.captures_at(text, begin).filter(|c| c.get(0).is_some_and(|m| m.start() == begin));
    let Some(found) = found else { return Err("Not at a dynamic block".to_string()) };
    let name = found[1].to_string();
    let mut params = Plist::read(found.get(2).map_or("", |m| m.as_str()))?;
    let begdel = (found.get(0).expect("a match").end() + 1).min(text.len());
    let indentation = column(text, begin + text[begin..].len() - text[begin..].trim_start_matches([' ', '\t']).len());
    let Some(end) = END.find_at(text, begin) else { return Err("Dynamic block not terminated".to_string()) };
    let content = text[begdel.min(end.start())..end.start()].to_string();
    params.set("content", Lisp::Str(content));
    params.set("name", Lisp::Str(name.clone()));
    let mut buffer =
        Buffer { text: format!("{}\n{}", &text[..begdel.min(end.start())], &text[end.start()..]), point: 0 };
    buffer.point = begdel.min(end.start());
    if name != "clocktable" {
        return Err(format!("Symbol’s function definition is void: org-dblock-write:{name}"));
    }
    write_clocktable(&mut buffer, &params, context)?;
    if indentation > 0 {
        let indent = " ".repeat(indentation);
        let mut at = next_line(&buffer.text, begin);
        while at < buffer.text.len() && !END.is_match(&buffer.text[at..line_end(&buffer.text, at)]) {
            buffer.text.insert_str(at, &indent);
            at = next_line(&buffer.text, at);
        }
        if at < buffer.text.len() {
            let blanks = buffer.text[at..].len() - buffer.text[at..].trim_start_matches([' ', '\t']).len();
            buffer.text.replace_range(at..at + blanks, &indent);
        }
    }
    Ok(buffer.text)
}

/// Where a cursor at `at` goes when the block at `begin` of `old` became
/// `new`: it stays before the block's content, and keeps its distance from
/// the end after it.
fn follow(old: &str, new: &str, begin: usize, at: usize) -> usize {
    let content = next_line(old, begin);
    if at < content {
        return at;
    }
    let end = END.find_at(old, begin).map_or(old.len(), |end| end.start());
    let at = at.max(end);
    (at + new.len()).saturating_sub(old.len()).min(new.len())
}

/// `org-dblock-update`: the block the cursor is in filled in again, or
/// every block of the file with `all` (`C-u`).
pub fn update(text: &str, cursor: usize, all: bool, context: &Context) -> Result<Done, String> {
    if all {
        return update_all(text, cursor, context);
    }
    let begin = if START.find_at(text, cursor).is_some_and(|found| found.start() == cursor) {
        cursor
    } else {
        beginning_of_block(text, cursor).ok_or("Not in a dynamic block")?
    };
    let text = update_block(text, begin, context)?;
    Ok(Done { text, cursor: begin, message: None })
}

/// `C-c C-c` on a block's `#+BEGIN:` or `#+END:` line: the block filled in
/// again, the cursor where it was.
pub fn update_here(text: &str, cursor: usize, context: &Context) -> Result<Done, String> {
    let begin = beginning_of_block(text, cursor).ok_or("Not in a dynamic block")?;
    let new = update_block(text, begin, context)?;
    let cursor = follow(text, &new, begin, cursor);
    Ok(Done { text: new, cursor, message: None })
}

/// `org-update-all-dblocks`: every block, one after the other; one that
/// fails is left as it is.
fn update_all(text: &str, cursor: usize, context: &Context) -> Result<Done, String> {
    let mut text = text.to_string();
    let mut cursor = cursor;
    let mut at = 0;
    let mut message = None;
    while let Some(found) = START.find_at(&text, at) {
        let begin = found.start();
        match update_block(&text, begin, context) {
            Ok(new) => {
                cursor = follow(&text, &new, begin, cursor);
                text = new;
            }
            Err(_) => message = Some("Error during update of dynamic block".to_string()),
        }
        let Some(end) = END.find_at(&text, begin) else { return Err("Dynamic block not terminated".to_string()) };
        at = end.end();
    }
    Ok(Done { text, cursor, message })
}

/// `org-in-clocktable-p`: the start of the clock table the cursor is in.
fn in_clocktable(text: &str, at: usize) -> Option<usize> {
    static BEGIN: LazyLock<Regex> = LazyLock::new(|| {
        RegexBuilder::new(r"^[ \t]*#\+BEGIN:[ \t]+clocktable")
            .case_insensitive(true)
            .multi_line(true)
            .build()
            .expect("a valid regex")
    });
    static FINISH: LazyLock<Regex> = LazyLock::new(|| {
        RegexBuilder::new(r"^[ \t]*#\+END:.*").case_insensitive(true).multi_line(true).build().expect("a valid regex")
    });
    let eol = line_end(text, at);
    let start = BEGIN.find_iter(&text[..eol]).map(|found| found.start()).last()?;
    let end = FINISH.find_at(text, start)?;
    (end.end() >= at).then_some(start)
}

/// `org-find-dblock`: the first clock table of the file.
fn find_clocktable(text: &str) -> Option<usize> {
    static FIRST: LazyLock<Regex> = LazyLock::new(|| {
        RegexBuilder::new(r"^[ \t]*#\+(?:BEGIN|begin):[ \t]+clocktable\b")
            .case_insensitive(true)
            .multi_line(true)
            .build()
            .expect("a valid regex")
    });
    FIRST.find(text).map(|found| found.start())
}

/// `org-clock-report`: the clock table the cursor is in filled in again,
/// or a new one where it is; with `first` (`C-u`), the first of the file.
pub fn report(text: &str, cursor: usize, first: bool, context: &Context) -> Result<Done, String> {
    let mut point = cursor;
    if first {
        if let Some(found) = find_clocktable(text) {
            point = found;
        }
    }
    let (text, begin) = match in_clocktable(text, point) {
        Some(start) => (text.to_string(), start),
        None => {
            let before_first = first_heading(text).is_none_or(|heading| heading > line_start(text, point));
            let mut plist = Plist::default();
            plist.set("scope", Lisp::Symbol(if before_first { "file" } else { "subtree" }.to_string()));
            let plist = plist.combine(&Plist::read(&context.settings.default_properties)?);
            create_block(text, point, &plist)
        }
    };
    let text = update_block(&text, begin, context)?;
    Ok(Done { text, cursor: begin, message: None })
}

fn first_heading(text: &str) -> Option<usize> {
    let mut at = 0;
    while at < text.len() {
        if headline_level(&text[at..line_end(text, at)]).is_some() {
            return Some(at);
        }
        at = next_line(text, at);
    }
    None
}

/// `org-create-dblock`: an empty clock table at `point`, on a line of its
/// own. Gives the text and where the block starts.
fn create_block(text: &str, point: usize, plist: &Plist) -> (String, usize) {
    let mut text = text.to_string();
    let mut point = point;
    let start = line_start(&text, point);
    if text[start..line_end(&text, point)].chars().any(|c| !c.is_whitespace()) {
        point = line_end(&text, point);
        text.insert(point, '\n');
        point += 1;
    }
    let indent = column(&text, point);
    let mut block = String::from("#+BEGIN: clocktable");
    for (key, value) in &plist.0 {
        block.push(' ');
        block.push_str(&key.print());
        block.push(' ');
        block.push_str(&value.print());
    }
    block.push_str("\n\n");
    block.push_str(&" ".repeat(indent));
    block.push_str("#+END:\n");
    text.insert_str(point, &block);
    let begin = line_start(&text, point);
    (text, begin)
}

// ----- shifting the block

/// `org-clocktable-shift`: the `:block` of the clock table whose
/// `#+BEGIN:` line the cursor is on moved `count` steps later (`right`,
/// `up`) or earlier, and the table filled in again.
pub fn shift(text: &str, cursor: usize, later: bool, count: Option<i64>, context: &Context) -> Result<Done, String> {
    static LINE: LazyLock<Regex> = LazyLock::new(|| {
        RegexBuilder::new(r"^[ \t]*#\+BEGIN:[ \t]+clocktable\b.*?:block[ \t]+(\S+)")
            .case_insensitive(true)
            .build()
            .expect("a valid regex")
    });
    static NAMED: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(today|thisweek|thismonth|thisyear|thisq)([-+][0-9]+)?$").expect("a valid regex")
    });
    static DATED: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"([0-9]+)(-([wWqQ]?)([0-9]{1,2})(-([0-9]{1,2}))?)?").expect("a valid regex"));
    let mut n = count.unwrap_or(1);
    if !later {
        n = -n;
    }
    let begin = line_start(text, cursor);
    let line = &text[begin..line_end(text, cursor)];
    let found = LINE.captures(line).ok_or("Line needs a :block definition before this command works".to_string())?;
    let span = found.get(1).expect("a block");
    let (b, e) = (begin + span.start(), begin + span.end());
    let mut name = span.as_str().to_string();
    name = match name.as_str() {
        "yesterday" => "today-1".to_string(),
        "lastweek" => "thisweek-1".to_string(),
        "lastmonth" => "thismonth-1".to_string(),
        "lastyear" => "thisyear-1".to_string(),
        "lastq" => "thisq-1".to_string(),
        _ => name,
    };
    let inserted = if let Some(named) = NAMED.captures(&name) {
        let shift = named.get(2).map_or(0, |m| m.as_str().parse::<i64>().unwrap_or(0)) + n;
        if shift == 0 {
            named[1].to_string()
        } else {
            format!("{}{shift:+}", &named[1])
        }
    } else if let Some(dated) = DATED.captures(&name) {
        let mut year: i64 = dated[1].parse().map_err(|_| "Cannot shift clocktable block".to_string())?;
        let kind = dated.get(3).map(|m| m.as_str()).filter(|kind| !kind.is_empty());
        let number = dated.get(4).and_then(|m| m.as_str().parse::<i64>().ok());
        if dated.get(6).is_some() {
            // Org gives `encode-time` no month here.
            return Err("Wrong type argument: fixnump, nil".to_string());
        }
        match (kind, number) {
            (Some("w" | "W"), Some(week)) => {
                let fourth = Date::new(year, 1, 4).to_day();
                let monday = fourth - ((crate::date::weekday(fourth) as i64 + 6) % 7) + (week + n - 1) * 7;
                let (week, year) = crate::date::iso_week(monday);
                format!("{year}-W{week:02}")
            }
            (Some(_), Some(quarter)) => {
                let mut quarter = quarter;
                if quarter + n > 4 {
                    quarter = 0;
                    year += 1;
                }
                if quarter + n == 0 {
                    quarter = 5;
                    year -= 1;
                }
                if !(1..=4).contains(&(quarter + n)) {
                    return Err("Wrong type argument: number-or-marker-p, nil".to_string());
                }
                format!("{year}-Q{}", quarter + n)
            }
            (_, Some(month)) => {
                let date = Date::from_day(Date::new(year, month + n, 1).to_day());
                format!("{:04}-{:02}", date.year, date.month)
            }
            _ => (year + n).to_string(),
        }
    } else {
        return Err("Cannot shift clocktable block".to_string());
    };
    let mut new = text.to_string();
    new.replace_range(b..e, &inserted);
    let cursor = if cursor > b { (cursor + inserted.len()).saturating_sub(e - b).max(b) } else { cursor };
    let text = update_block(&new, begin, context)?;
    Ok(Done { text, cursor, message: None })
}

// ----- times

/// `org-matcher-time`: the minute a time value of a match or of
/// `:tstart`, `:tend` stands for: `<now>`, `<today>`, `<tomorrow>`,
/// `<yesterday>`, `<-2d>`, a timestamp; the start of 1970 for anything
/// else.
fn matcher_time(text: &str, now: DateTime) -> i64 {
    static RELATIVE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^<([-+][0-9]+)([hdwmy])>$").expect("a valid regex"));
    let today = now.day * 1440;
    match text {
        "<now>" => minutes_of(now),
        "<today>" => today,
        "<tomorrow>" => today + 1440,
        "<yesterday>" => today - 1440,
        _ => {
            if let Some(found) = RELATIVE.captures(text) {
                let n: i64 = found[1].trim_start_matches('+').parse().unwrap_or(0);
                let (base, unit) = match &found[2] {
                    "h" => (minutes_of(now), 60),
                    "d" => (today, 1440),
                    "w" => (today, 10080),
                    "m" => (today, 44640),
                    _ => (today, 525960),
                };
                return base + n * unit;
            }
            match parse_time_string(text) {
                Some(parsed) => {
                    let (hour, minute) = parsed.time.unwrap_or((0, 0));
                    minutes_of(DateTime::new(parsed.date, hour, minute))
                }
                None => Date::new(1970, 1, 1).to_day() * 1440,
            }
        }
    }
}

/// A `:tstart` or `:tend` as `org-clock-get-table-data` reads it.
fn time_value(value: Option<&Lisp>, now: DateTime) -> Result<Option<i64>, String> {
    match value {
        None => Ok(None),
        Some(Lisp::Str(text)) => Ok(Some(matcher_time(text, now))),
        Some(Lisp::Int(day)) => Ok(Some(day * 1440)),
        Some(other) => Err(format!("Wrong type argument: stringp, {}", other.print())),
    }
}

/// The window of the table: `:block`, else `:tstart` and `:tend`.
fn window(params: &Plist, settings: &Settings) -> Result<(Option<i64>, Option<i64>), String> {
    if let Some(block) = params.block() {
        let range = special_range(
            &block,
            settings.now,
            params.int("wstart"),
            params.int("mstart"),
            settings.extend_today_until,
        )?;
        return Ok((range.start.map(minutes_of), Some(minutes_of(range.end))));
    }
    Ok((time_value(params.get("tstart"), settings.now)?, time_value(params.get("tend"), settings.now)?))
}

// ----- the data

/// A headline of a table.
struct Row {
    level: usize,
    headline: String,
    tags: Vec<String>,
    timestamp: Option<String>,
    time: i64,
    properties: Vec<(String, String)>,
}

/// The data of one file.
struct Table {
    /// The file as the scope names it, for the file column.
    name: Option<String>,
    /// Its `#+TITLE:`, for `:filetitle`.
    title: Option<String>,
    time: i64,
    rows: Vec<Row>,
}

/// `org-clock-sum` over `text[start..end]`: the time of each headline
/// with one, its subtree included, within `[from, to)`; only headlines
/// `include` takes count, and their parents show what is under them. Gives
/// the start of each headline with its time, and the total.
fn clock_sum(
    text: &str,
    (start, end): (usize, usize),
    (from, to): (Option<i64>, Option<i64>),
    include: Option<&dyn Fn(usize) -> bool>,
) -> (Vec<(usize, i64)>, i64) {
    let mut lines = Vec::new();
    let mut at = start;
    while at < end {
        let stop = line_end(text, at).min(end);
        lines.push((at, &text[at..stop]));
        at = next_line(text, at);
    }
    let headings: Vec<(usize, usize)> =
        lines.iter().filter_map(|(at, line)| headline_level(line).map(|level| (*at, level))).collect();
    let mut forced: Vec<usize> = Vec::new();
    let mut found = Vec::new();
    let mut levels = vec![0i64; 32];
    let mut pending = 0;
    for (at, line) in lines.iter().rev() {
        if let Some(level) = headline_level(line) {
            if level >= levels.len() {
                levels.resize(level * 2, 0);
            }
            let included = include.is_none_or(|include| include(*at));
            let is_forced = forced.contains(at);
            if pending > 0 || levels[level] > 0 {
                if included || is_forced {
                    if included {
                        for slot in levels.iter_mut().take(level + 1) {
                            *slot += pending;
                        }
                    }
                    found.push((*at, levels[level]));
                    if include.is_some() {
                        // The parents are shown, whatever they are.
                        let mut child = level;
                        for (other, other_level) in headings.iter().rev().filter(|(other, _)| other < at) {
                            if *other_level < child {
                                forced.push(*other);
                                child = *other_level;
                            }
                        }
                    }
                }
                pending = 0;
                for slot in levels.iter_mut().skip(level) {
                    *slot = 0;
                }
            }
        } else if let Some(clock) = parse_line(line) {
            pending += clock_minutes(&clock, from, to);
        }
    }
    found.reverse();
    (found, levels[0])
}

/// The first timestamp in `text` that is active (`<…>`) or inactive, with
/// the other half of a range.
fn first_stamp(text: &str, active: bool) -> Option<String> {
    // A range is of the kind of its first timestamp, whatever the
    // brackets of the second: `<a>--[b]` is active.
    static ACTIVE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"<[0-9]{4}-[0-9]{2}-[0-9]{2}[^>\n]*>(?:--(?:<[0-9]{4}-[0-9]{2}-[0-9]{2}[^>\n]*>|\[[0-9]{4}-[0-9]{2}-[0-9]{2}[^\]\n]*\]))?")
            .expect("a valid regex")
    });
    static INACTIVE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\[[0-9]{4}-[0-9]{2}-[0-9]{2}[^\]\n]*\](?:--(?:\[[0-9]{4}-[0-9]{2}-[0-9]{2}[^\]\n]*\]|<[0-9]{4}-[0-9]{2}-[0-9]{2}[^>\n]*>))?")
            .expect("a valid regex")
    });
    let (regex, other_end) = if active { (&ACTIVE, "]--") } else { (&INACTIVE, ">--") };
    // Not the second timestamp of a range of the other kind.
    regex
        .find_iter(text)
        .find(|found| !text[..found.start()].ends_with(other_end))
        .map(|found| found.as_str().to_string())
}

/// `org-entry-get` of `TIMESTAMP` (`active`) or `TIMESTAMP_IA`: the first
/// such timestamp in the headline, then in the section after the planning
/// line. Only a timestamp of the syntax tree counts: not one in a clock
/// line, a property, a comment, a keyword, a fixed-width line, or a block
/// whose contents Org does not parse (`src`, `example`, `export`,
/// `comment`).
pub(crate) fn entry_stamp(text: &str, outline: &Outline, index: usize, active: bool) -> Option<String> {
    static PLANNING: LazyLock<Regex> = LazyLock::new(|| {
        RegexBuilder::new(r"^[ \t]*(?:CLOSED|DEADLINE|SCHEDULED):")
            .case_insensitive(true)
            .build()
            .expect("a valid regex")
    });
    let entry = &outline.entries[index];
    if let Some(found) = first_stamp(&text[entry.offset..entry.line_end], active) {
        return Some(found);
    }
    let end = outline.entries.get(index + 1).map_or(text.len(), |next| next.offset);
    let mut at = next_line(text, entry.line_end);
    let planning = at < end && PLANNING.is_match(&text[at..line_end(text, at)]);
    if planning {
        at = next_line(text, at);
    }
    static BLOCK: LazyLock<Regex> = LazyLock::new(|| {
        RegexBuilder::new(r"^#\+begin_(src|example|export|comment)(?:[ \t]|$)")
            .case_insensitive(true)
            .build()
            .expect("a valid regex")
    });
    let mut in_properties = false;
    // The property drawer comes first, or after the planning line and
    // blank lines; a drawer named so further down holds plain text.
    let mut drawer_allowed = true;
    // The end line of the block whose lines are raw text.
    let mut raw_block: Option<String> = None;
    while at < end {
        let line = &text[at..line_end(text, at).min(end)];
        let trimmed = line.trim_matches([' ', '\t']);
        let first = drawer_allowed;
        drawer_allowed = first && planning && trimmed.is_empty();
        if let Some(close) = &raw_block {
            if trimmed.eq_ignore_ascii_case(close) {
                raw_block = None;
            }
        } else if let Some(found) = BLOCK.captures(trimmed) {
            raw_block = Some(format!("#+end_{}", &found[1]));
        } else if first && trimmed.eq_ignore_ascii_case(":PROPERTIES:") {
            in_properties = true;
        } else if in_properties {
            in_properties = !trimmed.eq_ignore_ascii_case(":END:");
        } else if trimmed == "#"
            || trimmed.starts_with("# ")
            || trimmed.starts_with("#+")
            || trimmed.starts_with(": ")
            || trimmed == ":"
        {
            // A comment, a keyword or a fixed-width line.
        } else if !trimmed.get(..6).is_some_and(|start| start.eq_ignore_ascii_case("CLOCK:")) {
            // A clock line (`org-clock-line-re`), even one whose times
            // do not read, holds no timestamp objects.
            if let Some(found) = first_stamp(line, active) {
                return Some(found);
            }
        }
        at = next_line(text, at);
    }
    None
}

/// The timestamp of a planning keyword, as written.
fn planning_stamp(text: &str, offset: usize) -> String {
    let close = text[offset + 1..].find([']', '>']).map_or(text.len(), |index| offset + 1 + index + 1);
    text[offset..close].to_string()
}

/// `org-entry-get` of a property for the `:properties` columns.
fn entry_value(
    text: &str,
    path: Option<&str>,
    outline: &Outline,
    index: usize,
    name: &str,
    inherit: bool,
) -> Option<String> {
    let entry = &outline.entries[index];
    let tags = |local: bool| {
        let names: Vec<&str> =
            entry.tags.iter().filter(|tag| !local || !tag.inherited).map(|tag| tag.name.as_str()).collect();
        (!names.is_empty()).then(|| format!(":{}:", names.join(":")))
    };
    match name.to_uppercase().as_str() {
        "TODO" => entry.todo.clone(),
        "CATEGORY" => Some(entry.category.clone()),
        "TAGS" => tags(true),
        "ALLTAGS" => tags(false),
        "FILE" => path.map(str::to_string),
        "SCHEDULED" => entry.scheduled.as_ref().map(|planning| planning_stamp(text, planning.offset)),
        "DEADLINE" => entry.deadline.as_ref().map(|planning| planning_stamp(text, planning.offset)),
        "CLOSED" => entry.closed.as_ref().map(|planning| planning_stamp(text, planning.offset)),
        "TIMESTAMP" => entry_stamp(text, outline, index, true),
        "TIMESTAMP_IA" => entry_stamp(text, outline, index, false),
        _ => outline.entry_property(Some(index), name, inherit),
    }
}

/// The `#+TITLE:` of a file.
fn title(text: &str) -> Option<String> {
    static TITLE: LazyLock<Regex> = LazyLock::new(|| {
        RegexBuilder::new(r"^[ \t]*#\+TITLE:[ \t]*(.*?)[ \t]*$")
            .case_insensitive(true)
            .multi_line(true)
            .build()
            .expect("a valid regex")
    });
    let parts: Vec<&str> = TITLE.captures_iter(text).map(|found| found.get(1).map_or("", |m| m.as_str())).collect();
    let joined = parts.join(" ");
    (!joined.trim().is_empty()).then_some(joined)
}

/// The outline of `text`, as the settings say to read it.
fn outline_of(text: &str, path: Option<&str>, settings: &Settings) -> Outline {
    let name = path
        .map(std::path::Path::new)
        .and_then(|path| path.file_stem())
        .map_or(String::new(), |stem| stem.to_string_lossy().into_owned());
    Outline::with_inheritance(&org_syntax::parse(text, &settings.keywords), text, &name, &settings.inheritance)
}

/// `org-clock-get-table-data` over `text[region]`.
fn table_data(
    text: &str,
    region: (usize, usize),
    name: Option<&str>,
    path: Option<&str>,
    params: &Plist,
    settings: &Settings,
) -> Result<Table, String> {
    let maxlevel = params.int("maxlevel").unwrap_or(3).max(0) as usize;
    let window = window(params, settings)?;
    let outline = outline_of(text, path, settings);
    let matcher = params.string("match").map(Matcher::new);
    let clock = MatcherClock { today: settings.now.day, minute: settings.now.minute };
    let include = |heading: usize| match (&matcher, outline.entry_at(heading)) {
        (Some(matcher), Some(index)) => matcher.matches(&outline, index, false, clock),
        _ => true,
    };
    let (times, total) = clock_sum(text, region, window, matcher.as_ref().map(|_| &include as &dyn Fn(usize) -> bool));
    let link = params.flag("link");
    let properties: Vec<String> = match params.get("properties") {
        Some(Lisp::List(items)) => items
            .iter()
            .map(|item| match item {
                Lisp::Str(text) => text.clone(),
                other => other.print(),
            })
            .collect(),
        _ => Vec::new(),
    };
    let inherit = params.flag("inherit-props");
    let mut rows = Vec::new();
    for (heading, time) in times {
        let Some(index) = outline.entry_at(heading).filter(|index| outline.entries[*index].offset == heading) else {
            continue;
        };
        let entry = &outline.entries[index];
        if time <= 0 || entry.level > maxlevel {
            continue;
        }
        let headline = heading_title(&entry.head, entry.todo.as_deref());
        let headline = if link {
            let search = format!("*{}", normalize(&headline));
            let target = match path {
                Some(path) => format!("file:{path}::{search}"),
                None => search,
            };
            crate::link::make_string(&target, Some(display_format(&without_cookies(&headline)).trim()))
                .unwrap_or_default()
        } else {
            headline
        };
        let tags =
            if params.flag("tags") { entry.tags.iter().map(|tag| tag.name.clone()).collect() } else { Vec::new() };
        let timestamp = if params.flag("timestamp") {
            ["SCHEDULED", "DEADLINE", "TIMESTAMP", "TIMESTAMP_IA"]
                .iter()
                .find_map(|name| entry_value(text, path, &outline, index, name, false))
        } else {
            None
        };
        let properties = properties
            .iter()
            .filter_map(|name| entry_value(text, path, &outline, index, name, inherit).map(|v| (name.clone(), v)))
            .collect();
        rows.push(Row { level: entry.level, headline, tags, timestamp, time, properties });
    }
    Ok(Table { name: name.map(str::to_string), title: title(text), time: total, rows })
}

/// The region of the scope in the file with the table: a subtree or a
/// tree around `at`, or the whole text.
fn scope_region(text: &str, at: usize, scope: Option<&str>) -> Result<(usize, usize), String> {
    let Some(scope) = scope else { return Ok((0, text.len())) };
    if scope == "file" || scope == "file-with-archives" || scope == "agenda" || scope == "agenda-with-archives" {
        return Ok((0, text.len()));
    }
    // The headlines up to `at`, and the subtree of one.
    let mut headings = Vec::new();
    let mut line = 0;
    while line < text.len() {
        if let Some(level) = headline_level(&text[line..line_end(text, line)]) {
            headings.push((line, level));
        }
        line = next_line(text, line);
    }
    let current = headings
        .iter()
        .rposition(|(start, _)| *start <= at)
        .ok_or_else(|| format!("Before first headline at position {}", text[..at].chars().count() + 1))?;
    let parent = |index: usize| {
        let level = headings[index].1;
        headings[..index].iter().rposition(|(_, other)| *other < level)
    };
    let mut index = current;
    if scope == "tree" {
        while let Some(up) = parent(index) {
            index = up;
        }
    } else if let Some(limit) = scope.strip_prefix("tree").and_then(|n| n.parse::<usize>().ok()) {
        while let Some(up) = parent(index) {
            index = up;
            if headings[index].1 <= limit {
                break;
            }
        }
    }
    let (start, level) = headings[index];
    let end = headings[index + 1..].iter().find(|(_, other)| *other <= level).map_or(text.len(), |(next, _)| *next);
    Ok((start, end))
}

// ----- the writing

struct Buffer {
    text: String,
    point: usize,
}

/// `org-clock-clocktable-language-setup`
const LANGUAGES: &[(&str, [&str; 9])] = &[
    ("en", ["File", "L", "Timestamp", "Headline", "Time", "ALL", "Total time", "File time", "Clock summary at"]),
    ("de", ["Datei", "E", "Zeitstempel", "Kopfzeile", "Dauer", "GESAMT", "Gesamtdauer", "Dateizeit", "Erstellt am"]),
    (
        "es",
        [
            "Archivo",
            "N",
            "Fecha y hora",
            "Tarea",
            "Duración",
            "TODO",
            "Duración total",
            "Tiempo archivo",
            "Generado el",
        ],
    ),
    (
        "fr",
        [
            "Fichier",
            "N",
            "Horodatage",
            "En-tête",
            "Durée",
            "TOUT",
            "Durée totale",
            "Durée fichier",
            "Horodatage sommaire à",
        ],
    ),
    (
        "nl",
        ["Bestand", "N", "Tijdstip", "Rubriek", "Duur", "ALLES", "Totale duur", "Bestandstijd", "Klok overzicht op"],
    ),
    ("nn", ["Fil", "N", "Tidspunkt", "Overskrift", "Tid", "ALLE", "Total tid", "Filtid", "Tidsoversyn"]),
    (
        "pl",
        [
            "Plik",
            "P",
            "Data i godzina",
            "Nagłówek",
            "Czas",
            "WSZYSTKO",
            "Czas całkowity",
            "Czas pliku",
            "Poddumowanie zegara na",
        ],
    ),
    (
        "pt-BR",
        [
            "Arquivo",
            "N",
            "Data e hora",
            "Título",
            "Hora",
            "TODOS",
            "Hora total",
            "Hora do arquivo",
            "Resumo das horas em",
        ],
    ),
    (
        "sk",
        ["Súbor", "L", "Časová značka", "Záhlavie", "Čas", "VŠETKO", "Celkový čas", "Čas súboru", "Časový súhrn pre"],
    ),
];

/// `org-clock--translate`
fn translate(term: &'static str, language: &str) -> &'static str {
    let index = LANGUAGES[0].1.iter().position(|english| *english == term).expect("a known term");
    LANGUAGES.iter().find(|(name, _)| name.eq_ignore_ascii_case(language)).map_or(term, |(_, terms)| terms[index])
}

/// `format` with one `%s`.
fn format_cell(format: &str, value: &str) -> String {
    format.replacen("%s", value, 1).replace("%%", "%")
}

/// `org-dblock-write:clocktable` at the buffer's point. Gives the total
/// time, or nothing for tables in steps.
fn write_clocktable(buffer: &mut Buffer, params: &Plist, context: &Context) -> Result<Option<i64>, String> {
    let settings = context.settings;
    let params = defaults().combine(params);
    let scope = params.get("scope").cloned();
    enum Files {
        Current,
        Listed(Vec<(String, String)>),
    }
    let absolute = |name: &str| -> String {
        if std::path::Path::new(name).is_absolute() {
            return name.to_string();
        }
        match context.path.and_then(|path| std::path::Path::new(path).parent()) {
            Some(directory) => directory.join(name).to_string_lossy().into_owned(),
            None => name.to_string(),
        }
    };
    let with_archives = |paths: Vec<String>, text_of: &dyn Fn(&str) -> String| {
        let mut out: Vec<String> = Vec::new();
        for path in paths {
            let archives = context.files.archive_files(&path, &text_of(&path));
            for file in std::iter::once(path).chain(archives) {
                if !out.contains(&file) {
                    out.push(file);
                }
            }
        }
        out
    };
    let text_of = |path: &str| -> String {
        if Some(path) == context.path {
            buffer.text.clone()
        } else {
            context.files.read(path).unwrap_or_default()
        }
    };
    let pairs = |paths: Vec<String>| Files::Listed(paths.into_iter().map(|path| (path.clone(), path)).collect());
    let files = match &scope {
        None => Files::Current,
        Some(Lisp::Symbol(name)) => match name.as_str() {
            "agenda" => pairs(context.files.agenda_files()),
            "agenda-with-archives" => pairs(with_archives(context.files.agenda_files(), &text_of)),
            "file-with-archives" => match context.path {
                Some(path) => pairs(with_archives(vec![path.to_string()], &text_of)),
                None => Files::Current,
            },
            "file" | "subtree" | "tree" => Files::Current,
            other
                if other
                    .strip_prefix("tree")
                    .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())) =>
            {
                Files::Current
            }
            other => return Err(format!("Unknown scope: {other}")),
        },
        Some(Lisp::List(items)) => Files::Listed(
            items
                .iter()
                .map(|item| match item {
                    Lisp::Str(name) => (name.clone(), absolute(name)),
                    other => (other.print(), absolute(&other.print())),
                })
                .collect(),
        ),
        Some(other) => return Err(format!("Unknown scope: {}", other.print())),
    };
    let (start, end) = window(&params, settings)?;
    if params.get("step").is_some() {
        if params.block().is_none() && (start.is_none() || end.is_none()) {
            return Err("Clocktable `:step' can only be used with `:block' or `:tstart', `:tend'".to_string());
        }
        steps(buffer, &params, context)?;
        return Ok(None);
    }
    let scope_name = scope.as_ref().and_then(Lisp::symbol);
    let tables = match &files {
        Files::Current => {
            let region = scope_region(&buffer.text, buffer.point, scope_name)?;
            vec![table_data(&buffer.text, region, None, context.path, &params, settings)?]
        }
        Files::Listed(files) => {
            let mut tables = Vec::new();
            for (name, path) in files {
                let text = text_of(path);
                tables.push(table_data(&text, (0, text.len()), Some(name), Some(path), &params, settings)?);
            }
            tables
        }
    };
    let multifile =
        !params.flag("hidefiles") && matches!(files, Files::Listed(_)) && scope_name != Some("file-with-archives");
    write_default(buffer, &tables, &params, multifile, settings).map(Some)
}

/// `org-clocktable-write-default`: the table of `tables` at the buffer's
/// point, aligned, sorted and calculated as the parameters say.
fn write_default(
    buffer: &mut Buffer,
    tables: &[Table],
    params: &Plist,
    multifile: bool,
    settings: &Settings,
) -> Result<i64, String> {
    let lang = params.string("lang").unwrap_or("en");
    let maxlevel = params.int("maxlevel").unwrap_or(3);
    let emphasize = params.flag("emphasize");
    let compact = params.flag("compact");
    let mut narrow = params.get("narrow").cloned();
    if narrow.is_none() && compact {
        narrow = Some(Lisp::Symbol("40!".to_string()));
    }
    let level_column = !compact && params.flag("level");
    let timestamp = params.flag("timestamp");
    let tags = params.flag("tags");
    let properties: Vec<String> = match params.get("properties") {
        Some(Lisp::List(items)) => items
            .iter()
            .map(|item| match item {
                Lisp::Str(text) => text.clone(),
                other => other.print(),
            })
            .collect(),
        _ => Vec::new(),
    };
    let time_columns = if compact || maxlevel < 2 {
        1
    } else {
        let deepest = tables
            .iter()
            .filter(|table| table.time != 0)
            .flat_map(|table| table.rows.iter().map(|row| row.level))
            .max();
        let tcolumns = params.int("tcolumns").unwrap_or(100);
        (maxlevel.min(tcolumns).min(deepest.map_or(1, |level| level as i64))).max(0) as usize
    };
    let indent = compact || params.flag("indent");
    let formula = params.get("formula").cloned();
    let percent = formula.as_ref().is_some_and(|formula| formula.symbol() == Some("%"));
    let total: i64 = tables.iter().map(|table| table.time).sum();

    // `:narrow`: a width, or a width to cut headlines at (`40!`).
    let mut cut = false;
    let mut width = None;
    match &narrow {
        None => {}
        Some(Lisp::Int(n)) => {
            if params.flag("link") {
                cut = true;
            }
            width = Some(*n);
        }
        Some(Lisp::Symbol(name))
            if name.strip_suffix('!').is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())) =>
        {
            cut = true;
            width = name.trim_end_matches('!').parse().ok();
        }
        Some(other) => return Err(format!("Invalid value {} of :narrow property in clock table", other.print())),
    }

    let ipos = buffer.point;
    let mut out = String::new();
    match params.get("header") {
        Some(Lisp::Str(header)) => out.push_str(header),
        Some(other) => out.push_str(&other.print()),
        None => {
            let range = match params.block() {
                Some(block) => {
                    let range = special_range(
                        &block,
                        settings.now,
                        params.int("wstart"),
                        params.int("mstart"),
                        settings.extend_today_until,
                    )?;
                    format!(", for {}.", range.text)
                }
                None => String::new(),
            };
            out.push_str(&format!(
                "#+CAPTION: {} {}{range}\n",
                translate("Clock summary at", lang),
                format_timestamp(settings.now, true, true, "")
            ));
        }
    }
    if let (Some(width), false) = (width, cut) {
        out.push('|');
        if multifile {
            out.push('|');
        }
        if level_column {
            out.push('|');
        }
        if timestamp {
            out.push('|');
        }
        if tags {
            out.push('|');
        }
        out.push_str(&"|".repeat(properties.len()));
        out.push_str(&format!("<{width}>| |\n"));
    }
    // The header line.
    out.push('|');
    if multifile {
        out.push_str(&format!("{}|", translate("File", lang)));
    }
    if level_column {
        out.push_str(&format!("{}|", translate("L", lang)));
    }
    if timestamp {
        out.push_str(&format!("{}|", translate("Timestamp", lang)));
    }
    if tags {
        out.push_str("Tags |");
    }
    if !properties.is_empty() {
        out.push_str(&format!("{}|", properties.join("|")));
    }
    out.push_str(&format!("{}|", translate("Headline", lang)));
    out.push_str(&format!("{}|", translate("Time", lang)));
    out.push_str(&"|".repeat(time_columns.saturating_sub(1)));
    out.push_str(if percent { "%|\n" } else { "\n" });
    // The total.
    out.push_str("|-\n|");
    if multifile {
        out.push_str(&format!("| {} ", translate("ALL", lang)));
    }
    if level_column {
        out.push('|');
    }
    if timestamp {
        out.push('|');
    }
    if tags {
        out.push('|');
    }
    out.push_str(&"|".repeat(properties.len()));
    out.push_str(&format_cell(&settings.total_time_cell_format, translate("Total time", lang)));
    out.push_str("| ");
    out.push_str(&format_cell(&settings.total_time_cell_format, &duration(total)));
    out.push('|');
    out.push_str(&"|".repeat(time_columns.saturating_sub(1)));
    if percent {
        out.push_str(if total == 0 { "0.0|" } else { "100.0|" });
    }
    out.push('\n');
    let share = |time: i64| format!("{:.1}", 100.0 * time as f64 / total as f64);
    if total > 0 {
        for table in tables {
            if table.time <= 0 && params.flag("fileskip0") {
                continue;
            }
            out.push_str("|-\n");
            if multifile {
                let name = table.name.as_deref().unwrap_or("");
                let shown = std::path::Path::new(name)
                    .file_name()
                    .map_or(name.to_string(), |file| file.to_string_lossy().into_owned());
                let shown = if params.flag("filetitle") { table.title.clone().unwrap_or(shown) } else { shown };
                out.push_str(&format!(
                    "| {shown} {} | {}{}{}{} | *{}*|{}{}\n",
                    if level_column { "| " } else { "" },
                    if timestamp { "| " } else { "" },
                    if tags { "| " } else { "" },
                    "|".repeat(properties.len()),
                    format_cell(&settings.file_time_cell_format, translate("File time", lang)),
                    duration(table.time),
                    "|".repeat(time_columns.saturating_sub(1)),
                    if percent { format!(" {} |", share(table.time)) } else { String::new() },
                ));
            }
            if maxlevel <= 0 {
                continue;
            }
            for row in &table.rows {
                let mut headline = row.headline.clone();
                if let (true, Some(width)) = (cut, width) {
                    let width = width.max(0) as usize;
                    headline = match link_parts(&headline) {
                        Some((target, Some(description))) => {
                            format!("[[{target}][{}]]", shorten(&description, width))
                        }
                        _ => shorten(&headline, width),
                    };
                }
                let field = |text: &str| {
                    if !emphasize {
                        format!("{text} |")
                    } else if row.level == 1 {
                        format!("*{text}* |")
                    } else if row.level == 2 {
                        format!("/{text}/ |")
                    } else {
                        format!("{text} |")
                    }
                };
                out.push('|');
                if multifile {
                    out.push('|');
                }
                if level_column {
                    out.push_str(&format!("{}|", row.level));
                }
                if timestamp {
                    out.push_str(&format!("{}|", row.timestamp.as_deref().unwrap_or("")));
                }
                if tags {
                    out.push_str(&format!("{}|", row.tags.join(", ")));
                }
                if !properties.is_empty() {
                    let values: Vec<&str> = properties
                        .iter()
                        .map(|name| {
                            row.properties.iter().find(|(key, _)| key == name).map_or("", |(_, value)| value.as_str())
                        })
                        .collect();
                    out.push_str(&format!("{}|", values.join("|")));
                }
                if indent && row.level > 1 {
                    out.push_str(&format!("\\_{}", " ".repeat(2 * (row.level - 1))));
                }
                out.push_str(&field(&headline));
                out.push_str(&"|".repeat(row.level.min(time_columns).saturating_sub(1)));
                out.push_str(&field(&duration(row.time)));
                out.push_str(&"|".repeat(time_columns.saturating_sub(row.level)));
                if percent {
                    out.push_str(&format!("{} |", share(row.time)));
                }
                out.push('\n');
            }
        }
    }
    out.pop();
    buffer.text.insert_str(ipos, &out);
    buffer.point = ipos + out.len();
    // Formulas: those of the table before, or `:formula`.
    let mut recalculate = false;
    match &formula {
        None => {}
        Some(Lisp::Str(formula)) => {
            let line = format!("\n#+TBLFM: {formula}");
            buffer.text.insert_str(buffer.point, &line);
            recalculate = true;
        }
        Some(other) if other.symbol() == Some("%") => {}
        Some(_) => return Err("Invalid :formula parameter in clocktable".to_string()),
    }
    if formula.is_none() || percent {
        static TBLFM: LazyLock<Regex> = LazyLock::new(|| {
            RegexBuilder::new(r"^([ \t]*#\+tblfm:.*)")
                .case_insensitive(true)
                .multi_line(true)
                .build()
                .expect("a valid regex")
        });
        let content = params.string("content").unwrap_or("");
        if content.chars().any(|c| !c.is_whitespace()) {
            if let Some(found) = TBLFM.captures(content) {
                buffer.text.insert_str(buffer.point, &format!("\n{}", &found[1]));
                recalculate = true;
            }
        }
    }
    // Align the table, sort it, calculate it.
    let Some(bar) = buffer.text[ipos..].find('|').map(|index| ipos + index) else { return Ok(total) };
    let table_start = line_start(&buffer.text, bar);
    let mut table_end = table_start;
    while table_end < buffer.text.len()
        && buffer.text[table_end..line_end(&buffer.text, table_end)].trim_start_matches([' ', '\t']).starts_with('|')
    {
        table_end = next_line(&buffer.text, table_end);
    }
    let block = buffer.text[table_start..table_end].to_string();
    let lines: Vec<&str> = block.lines().collect();
    let aligned = align_table(&lines);
    let mut replaced: String = aligned.join("\n");
    if block.ends_with('\n') {
        replaced.push('\n');
    }
    buffer.text.replace_range(table_start..table_end, &replaced);
    if let Some(sort) = params.get("sort") {
        let (column, kind) = match sort {
            Lisp::Cons(column, kind) => match (column.as_ref(), kind.as_ref()) {
                (Lisp::Int(column), Lisp::Int(kind)) => (*column, char::from_u32(*kind as u32).unwrap_or('a')),
                _ => return Err("Invalid :sort parameter in clocktable".to_string()),
            },
            _ => return Err("Invalid :sort parameter in clocktable".to_string()),
        };
        // The third line that is not a rule, in the sort column.
        let mut at = table_start;
        let mut seen = 0;
        while at < buffer.text.len() {
            let line = &buffer.text[at..line_end(&buffer.text, at)];
            if !line.trim_start_matches([' ', '\t']).starts_with('|') {
                break;
            }
            if !line.trim_start_matches([' ', '\t']).starts_with("|-") {
                seen += 1;
                if seen == 3 {
                    break;
                }
            }
            at = next_line(&buffer.text, at);
        }
        if seen == 3 {
            let line = &buffer.text[at..line_end(&buffer.text, at)];
            let mut field = at;
            let mut bars = 0;
            for (index, c) in line.char_indices() {
                if c == '|' {
                    bars += 1;
                    if bars == column {
                        field = at + index + 1;
                        if buffer.text[field..].starts_with(' ') {
                            field += 1;
                        }
                        break;
                    }
                }
            }
            let done = crate::table::run(&buffer.text, field, crate::table::Command::Sort { kind, column: None })?;
            buffer.text = done.text;
        }
    }
    if recalculate {
        let done = crate::tblfm::recalculate(&buffer.text, bar, crate::tblfm::Mode::All, &|_| None)?;
        buffer.text = done.text;
    }
    buffer.point = bar;
    Ok(total)
}

/// The target and description of a text that is one bracket link.
fn link_parts(text: &str) -> Option<(String, Option<String>)> {
    static LINK: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^\[\[((?:[^\]\[\\]|\\(?:\\\\)*[\]\[]|\\+[^\]\[])+)\](?:\[([\s\S]+?)\])?\]$")
            .expect("a valid regex")
    });
    let found = LINK.captures(text)?;
    Some((found[1].to_string(), found.get(2).map(|m| m.as_str().to_string())))
}

/// `org-clocktable-steps`: a table for each day, week, month… of the
/// window, each under a line that says when it starts.
fn steps(buffer: &mut Buffer, params: &Plist, context: &Context) -> Result<(), String> {
    let settings = context.settings;
    let step = params.get("step").map(Lisp::print).unwrap_or_default();
    let header = match step.as_str() {
        "day" => "Daily report: ",
        "week" => "Weekly report starting on: ",
        "semimonth" => "Semimonthly report starting on: ",
        "month" => "Monthly report starting on: ",
        "year" => "Annual report starting on: ",
        "quarter" => "Quarterly report starting on: ",
        _ => return Err(format!("Unknown `:step' specification: {step}")),
    };
    let week_start = params.int("wstart").unwrap_or(1);
    let month_start = params.int("mstart").unwrap_or(1);
    let extend = settings.extend_today_until;
    let range = match params.block() {
        Some(block) => Some(special_range(&block, settings.now, Some(week_start), Some(month_start), extend)?),
        None => None,
    };
    let at_day = |day: Day| day * 1440 + extend * 60;
    let start = match &range {
        Some(range) => match range.start {
            Some(start) => minutes_of(start),
            None => matcher_time("<2003-01-01 Thu 00:00>", settings.now),
        },
        None => match params.get("tstart") {
            Some(Lisp::Int(day)) => at_day(*day),
            Some(Lisp::Str(text)) => matcher_time(text, settings.now),
            _ => matcher_time("<2003-01-01 Thu 00:00>", settings.now),
        },
    };
    let end = match &range {
        Some(range) => minutes_of(range.end),
        None => match params.get("tend") {
            Some(Lisp::Int(day)) => at_day(*day),
            Some(Lisp::Str(text)) => matcher_time(text, settings.now),
            _ => return Err("Wrong type argument: stringp, nil".to_string()),
        },
    };
    let encode = |year: i64, month: i64, day: i64, hour: i64| {
        let first = Date::new(year, month, 1).to_day();
        (first + day - 1) * 1440 + hour * 60
    };
    let stamp = |minutes: i64, with_time: bool| {
        let time = DateTime { day: minutes.div_euclid(1440), minute: minutes.rem_euclid(1440) };
        format_timestamp(time, with_time, true, "")
    };
    let mut start = start;
    while start < end {
        if buffer.point > 0 && buffer.text.as_bytes()[buffer.point - 1] != b'\n' {
            buffer.text.insert(buffer.point, '\n');
            buffer.point += 1;
        }
        let line = format!("\n{header}{}\n", stamp(start, false));
        buffer.text.insert_str(buffer.point, &line);
        buffer.point += line.len();
        let table_begin = line_start(&buffer.text, buffer.point - 1);
        let date = Date::from_day(start.div_euclid(1440));
        let dow = crate::date::weekday(start.div_euclid(1440)) as i64;
        let next = match step.as_str() {
            "day" => encode(date.year, date.month, date.day + 1, extend),
            "week" => {
                let offset = if dow == week_start { 7 } else { (week_start - dow).rem_euclid(7) };
                encode(date.year, date.month, date.day + offset, extend)
            }
            "semimonth" => {
                if date.day < 16 {
                    encode(date.year, date.month, 16, 0)
                } else {
                    encode(date.year, date.month + 1, 1, 0)
                }
            }
            "month" => encode(date.year, date.month + 1, month_start, 0),
            "quarter" => encode(date.year, date.month + 3, month_start, 0),
            _ => encode(date.year + 1, 1, 1, extend),
        };
        let mut each = params.clone();
        each.set("header", Lisp::Str(String::new()));
        each.set("step", Lisp::Symbol("nil".to_string()));
        each.set("block", Lisp::Symbol("nil".to_string()));
        each.set("tstart", Lisp::Str(stamp(start, true)));
        each.set("tend", Lisp::Str(stamp(end.min(next), true)));
        let time = write_clocktable(buffer, &each, context)?;
        // The `#+END:` line of the block.
        let end_line = END.find_at(&buffer.text, buffer.point).map_or(buffer.text.len(), |found| found.start());
        let mut end_line_start = end_line;
        if params.flag("stepskip0") && time == Some(0) {
            buffer.text.replace_range(table_begin..end_line, "");
            end_line_start = table_begin;
        }
        start = next;
        // The end of the line before the `#+END:` line.
        buffer.point = end_line_start.saturating_sub(1);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_parameters() {
        let plist = Plist::read(":maxlevel 3 :scope (\"a.org\" b) :sort (2 . ?a) :narrow 40! :lang \"de\"").unwrap();
        assert_eq!(plist.int("maxlevel"), Some(3));
        assert_eq!(plist.get("sort").unwrap().print(), "(2 . 97)");
        assert_eq!(plist.get("scope").unwrap().print(), "(\"a.org\" b)");
        assert_eq!(plist.get("narrow"), Some(&Lisp::Symbol("40!".to_string())));
        assert_eq!(plist.string("lang"), Some("de"));
        assert!(Plist::read(":scope (a").is_err());
        let combined = defaults().combine(&Plist::read(":maxlevel 4 :maxlevel 5").unwrap());
        assert_eq!(combined.int("maxlevel"), Some(5));
    }

    #[test]
    fn reads_match_times() {
        let now = DateTime::new(Date::new(2026, 5, 20), 10, 30);
        let day = Date::new(2026, 5, 20).to_day() * 1440;
        assert_eq!(matcher_time("<today>", now), day);
        assert_eq!(matcher_time("<-1w>", now), day - 10080);
        assert_eq!(matcher_time("<+2h>", now), day + 750);
        assert_eq!(matcher_time("<2026-05-12>", now), Date::new(2026, 5, 12).to_day() * 1440);
        assert_eq!(matcher_time("[2026-05-12 Tue 10:00]", now), Date::new(2026, 5, 12).to_day() * 1440 + 600);
    }

    #[test]
    fn refuses_a_list_nested_past_the_stack() {
        let nested = format!("{}1{}", "(".repeat(100_000), ")".repeat(100_000));
        assert!(Reader::new(&nested).read_all().is_err());
        assert!(Reader::new("(1 (2 3))").read_all().is_ok());
    }
}
