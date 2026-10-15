//! Element parsing: the line-oriented structure of an Org document.
//!
//! Headlines are found first, purely by line prefix, which fixes the outline.
//! Each section between headlines is then parsed into elements, following
//! the order and the rules of `org-element--current-element`:
//!
//! - Elements that need a closing line (blocks, drawers, LaTeX environments)
//!   are only recognised when that line exists. Otherwise the opening line is
//!   paragraph text.
//! - What ends a paragraph (`ends_paragraph`) is not the same as what starts
//!   an element (`element_at`); Org uses a separate, looser pattern for it.
//! - Affiliated keywords (`#+NAME:`, `#+CAPTION:`, `#+ATTR_*:` …) directly
//!   above an element belong to that element.

use rowan::{GreenNode, GreenNodeBuilder};
use std::collections::HashMap;

use crate::config::TodoKeywords;
use crate::kind::SyntaxKind::{self, *};
use crate::objects::{parse_objects, Mode};

/// One source line: `start..end` is the content, `end..next` the terminator.
///
/// Indentation and blankness are computed once: the list rules consult them
/// for every line below an item, at every nesting level.
#[derive(Clone, Copy, Debug)]
struct Line {
    start: usize,
    end: usize,
    next: usize,
    /// Display width of the leading spaces and tabs.
    indent: usize,
    /// Whether the line holds nothing but whitespace.
    blank: bool,
}

impl Line {
    fn new(src: &str, start: usize, end: usize, next: usize) -> Line {
        let text = &src[start..end];
        Line { start, end, next, indent: indent_width(text), blank: text.trim().is_empty() }
    }
}

fn split_lines(src: &str) -> Vec<Line> {
    let bytes = src.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0;
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'\n' {
            let end = if i > start && bytes[i - 1] == b'\r' { i - 1 } else { i };
            lines.push(Line::new(src, start, end, i + 1));
            start = i + 1;
        }
    }
    if start < bytes.len() {
        lines.push(Line::new(src, start, bytes.len(), bytes.len()));
    }
    lines
}

/// Elements nested deeper than this are kept as plain paragraphs. Real
/// documents stay far below it; the limit keeps recursion bounded.
const MAX_DEPTH: usize = 64;

const PLANNING_KEYWORDS: [&str; 3] = ["SCHEDULED:", "DEADLINE:", "CLOSED:"];

/// Blocks whose contents are not parsed as Org elements.
const RAW_BLOCKS: [&str; 5] = ["SRC", "EXAMPLE", "EXPORT", "COMMENT", "VERSE"];

/// Keywords that attach to the element below them. `ATTR_*` is handled apart.
const AFFILIATED_KEYWORDS: [&str; 13] = [
    "CAPTION", "DATA", "HEADER", "HEADERS", "LABEL", "NAME", "PLOT", "RESNAME", "RESULT", "RESULTS", "SOURCE",
    "SRCNAME", "TBLNAME",
];

/// Affiliated keywords that accept an optional value: `#+CAPTION[short]: long`.
const DUAL_KEYWORDS: [&str; 2] = ["CAPTION", "RESULTS"];

// ----- line classification

fn is_space(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

/// Byte length of the leading spaces and tabs.
fn indent_len(text: &str) -> usize {
    text.bytes().take_while(|b| is_space(*b)).count()
}

/// Display width of the leading spaces and tabs, with tabs at every 8 columns.
fn indent_width(text: &str) -> usize {
    let mut width = 0;
    for b in text.bytes() {
        match b {
            b' ' => width += 1,
            b'\t' => width += 8 - width % 8,
            _ => break,
        }
    }
    width
}

fn trim_end(text: &str) -> &str {
    text.trim_end_matches([' ', '\t'])
}

fn trim_start(text: &str) -> &str {
    text.trim_start_matches([' ', '\t'])
}

fn trim(text: &str) -> &str {
    text.trim_matches([' ', '\t'])
}

/// Split paragraph text into its content and trailing whitespace. Spaces
/// after a final `\_` stay with the content: together they form an entity.
/// (Titles and table cells are trimmed by Org before objects are parsed.)
fn split_trailing(text: &str) -> (&str, &str) {
    let body = trim_end(text);
    if body.ends_with("\\_") {
        let spaces = text[body.len()..].bytes().take_while(|b| *b == b' ').count();
        return text.split_at(body.len() + spaces);
    }
    text.split_at(body.len())
}

fn starts_with_ignore_case(text: &str, prefix: &str) -> bool {
    text.get(..prefix.len()).is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

/// Number of stars if the line is a headline (`*+` followed by a space).
fn headline_level(text: &str) -> Option<usize> {
    let stars = text.bytes().take_while(|b| *b == b'*').count();
    (stars > 0 && text.as_bytes().get(stars) == Some(&b' ')).then_some(stars)
}

fn is_tag_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '@' | '#' | '%')
}

const COMMENT_KEYWORD_TEXT: &str = "COMMENT";

/// Whether `text` starts with `COMMENT` followed by a space or nothing.
fn is_comment_keyword(text: &str) -> bool {
    text.strip_prefix(COMMENT_KEYWORD_TEXT).is_some_and(|after| after.is_empty() || after.starts_with(' '))
}

/// The first priority cookie in `text`: its offset and length.
///
/// The value is one capital letter or a number from 0 to 64 without a
/// leading zero (`org-priority-regexp`). Org's regular expression also
/// takes a small letter wherever it happens to run with case folding on,
/// which the agenda does not; here a small letter never counts.
fn priority_cookie(text: &str) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    text.match_indices("[#").find_map(|(at, _)| {
        let value = &bytes[at + 2..];
        let len = match value {
            [a, b']', ..] if a.is_ascii_uppercase() || a.is_ascii_digit() => 1,
            [b'1'..=b'5', b'0'..=b'9', b']', ..] | [b'6', b'0'..=b'4', b']', ..] => 2,
            _ => return None,
        };
        Some((at, 2 + len + 1))
    })
}

/// Split a headline line from `pos` (after stars, keyword, priority and
/// `COMMENT`) into `(title, gap, tags, trailing whitespace)`.
///
/// Tags are a final `:a:b:` group that starts at or after `pos` and has a
/// blank before it.
fn split_tags(text: &str, pos: usize) -> (&str, &str, &str, &str) {
    let trimmed_end = trim_end(text).len();
    let trailing = &text[trimmed_end.max(pos)..];
    let trimmed = &text[..trimmed_end];
    if trimmed.ends_with(':') {
        let start = trimmed
            .char_indices()
            .rev()
            .take_while(|(_, c)| *c == ':' || is_tag_char(*c))
            .last()
            .map_or(trimmed.len(), |(index, _)| index);
        let tags = &trimmed[start..];
        let after_blank = start >= pos && trimmed[..start].ends_with([' ', '\t']);
        if tags.starts_with(':') && tags.len() >= 3 && after_blank {
            let before = &trimmed[pos..start];
            let title = trim_end(before);
            return (title, &before[title.len()..], tags, trailing);
        }
    }
    (&trimmed[pos.min(trimmed_end)..], "", "", trailing)
}

struct ItemPrefix {
    indent: usize,
    bullet: usize,
    ordered: bool,
}

/// The bullet at the start of a list item line, if there is one.
fn item_prefix(text: &str) -> Option<ItemPrefix> {
    let indent = indent_len(text);
    let rest = &text.as_bytes()[indent..];
    let (bullet, ordered) = match *rest.first()? {
        b'-' | b'+' => (1, false),
        b'*' if indent > 0 => (1, false),
        // `org-item-re`: a number, or a letter with
        // `org-list-allow-alphabetical`, ended as
        // `org-plain-list-ordered-item-terminator` says.
        _ => (crate::lists::list_syntax().ordered_len(rest)?, true),
    };
    match rest.get(bullet) {
        None => Some(ItemPrefix { indent, bullet, ordered }),
        Some(b) if is_space(*b) => Some(ItemPrefix { indent, bullet, ordered }),
        Some(_) => None,
    }
}

/// Length of `#+KEY:` at the start of `text` (already left-trimmed). The key
/// runs to the last colon of the first word, so `#+a:b: c` has the key `a:b`.
fn keyword_key_len(text: &str) -> Option<usize> {
    let rest = text.strip_prefix("#+")?;
    let word = &rest[..rest.find(char::is_whitespace).unwrap_or(rest.len())];
    let colon = word.rfind(':')?;
    (colon > 0).then_some(2 + colon + 1)
}

/// Whether the line is an affiliated keyword such as `#+NAME:`.
fn is_affiliated(text: &str) -> bool {
    let Some(rest) = trim_start(text).strip_prefix("#+") else { return false };
    let key_len = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-')).unwrap_or(rest.len());
    let key = &rest[..key_len];
    let after = &rest[key_len..];
    if starts_with_ignore_case(key, "ATTR_") && key.len() > 5 {
        return after.starts_with(':');
    }
    if !AFFILIATED_KEYWORDS.iter().any(|keyword| key.eq_ignore_ascii_case(keyword)) {
        return false;
    }
    if after.starts_with(':') {
        return true;
    }
    // `#+CAPTION[short]: long`: the bracket runs to the last `]:` on the line.
    DUAL_KEYWORDS.iter().any(|keyword| key.eq_ignore_ascii_case(keyword))
        && after.starts_with('[')
        && after.contains("]:")
}

/// The `NAME` of a `#+BEGIN_NAME` line (already left-trimmed).
fn block_name(text: &str) -> Option<&str> {
    if !starts_with_ignore_case(text, "#+begin_") {
        return None;
    }
    let rest = &text[8..];
    let len = rest.find(char::is_whitespace).unwrap_or(rest.len());
    (len > 0).then_some(&rest[..len])
}

#[cfg(test)]
fn is_block_end(text: &str, name: &str) -> bool {
    let text = trim(text);
    starts_with_ignore_case(text, "#+end_") && text[6..].eq_ignore_ascii_case(name)
}

/// `#+BEGIN: NAME …` (already left-trimmed).
fn is_dynamic_block_begin(text: &str) -> bool {
    starts_with_ignore_case(text, "#+begin:")
        && trim_start(&text[8..]).chars().next().is_some_and(char::is_alphanumeric)
}

fn is_dynamic_block_end(text: &str) -> bool {
    let text = trim(text);
    text.eq_ignore_ascii_case("#+end:") || text.eq_ignore_ascii_case("#+end")
}

/// `:NAME:` alone on a line.
fn is_drawer_begin(text: &str) -> bool {
    let text = trim(text);
    text.len() >= 3
        && text.starts_with(':')
        && text.ends_with(':')
        && text[1..text.len() - 1].chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

fn is_drawer_end(text: &str) -> bool {
    trim(text).eq_ignore_ascii_case(":END:")
}

/// Length of the `:KEY:` word of a node property line (already left-trimmed).
fn property_key_len(text: &str) -> Option<usize> {
    let len = text.find([' ', '\t']).unwrap_or(text.len());
    let key = &text[..len];
    (key.len() >= 3 && key.starts_with(':') && key.ends_with(':')).then_some(len)
}

/// `# text` or a lone `#` (already left-trimmed).
fn is_comment(text: &str) -> bool {
    text == "#" || text.starts_with("# ")
}

/// `: text` or a lone `:` (already left-trimmed).
fn is_fixed_width(text: &str) -> bool {
    text == ":" || text.starts_with(": ")
}

/// Five or more dashes (already left-trimmed).
fn is_rule(text: &str) -> bool {
    let text = trim_end(text);
    text.len() >= 5 && text.bytes().all(|b| b == b'-')
}

/// A `+---+---+` line of a table.el table (already left-trimmed).
fn is_table_el_rule(text: &str) -> bool {
    let text = trim_end(text);
    text.len() >= 3
        && text.starts_with('+')
        && text.ends_with('+')
        && text.bytes().all(|b| b == b'+' || b == b'-')
        && !text.contains("++")
}

/// Length of `[fn:LABEL]` at the start of a line.
fn footnote_definition_len(text: &str) -> Option<usize> {
    let rest = text.strip_prefix("[fn:")?;
    let len = rest
        .char_indices()
        .find(|(_, c)| !(c.is_alphanumeric() || *c == '-' || *c == '_'))
        .map_or(rest.len(), |(index, _)| index);
    (len > 0 && rest[len..].starts_with(']')).then_some(4 + len + 1)
}

/// The environment of a `\begin{NAME}` line (already left-trimmed).
fn latex_environment_name(text: &str) -> Option<&str> {
    let rest = text.strip_prefix("\\begin{")?;
    let len = rest.find('}')?;
    let name = &rest[..len];
    (!name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'*')).then_some(name)
}

/// Whether `text` is one inactive timestamp, `[DATE]` or `[DATE REST]`.
///
/// Org matches the part after the date lazily but anchors the whole clock
/// line, so `REST` may itself contain brackets.
fn is_inactive_timestamp(text: &str) -> bool {
    let bytes = text.as_bytes();
    let Some(date) = bytes.get(1..11) else { return false };
    let is_date = date.iter().enumerate().all(|(index, b)| match index {
        4 | 7 => *b == b'-',
        _ => b.is_ascii_digit(),
    });
    bytes[0] == b'[' && is_date && text.ends_with(']') && (bytes.len() == 12 || bytes[11] == b' ')
}

/// A clock line: `CLOCK:`, then a timestamp, a timestamp range with its
/// duration (`[…]--[…] => H:MM`), or a duration alone.
fn is_clock_line(text: &str) -> bool {
    let text = trim_start(text);
    if !starts_with_ignore_case(text, "CLOCK:") {
        return false;
    }
    let rest = &text["CLOCK:".len()..];
    if !rest.starts_with([' ', '\t']) {
        return false;
    }
    let body = trim(rest);
    if is_inactive_timestamp(body) {
        return true;
    }
    // Otherwise the line ends in `=> H:MM`, after a range or after nothing.
    let Some((range, duration)) = body.rsplit_once("=>") else { return false };
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
    let duration_ok = duration.starts_with([' ', '\t'])
        && trim_start(duration)
            .split_once(':')
            .is_some_and(|(hours, minutes)| digits(hours) && digits(minutes) && minutes.len() == 2);
    let range_ok = range.is_empty()
        || (range.ends_with([' ', '\t'])
            && trim_end(range).match_indices("]--[").any(|(index, _)| {
                is_inactive_timestamp(&range[..index + 1]) && is_inactive_timestamp(trim_end(&range[index + 3..]))
            }));
    duration_ok && range_ok
}

/// Whether the line starts with `CLOCK:`, which is enough to keep
/// affiliated keywords from attaching to it.
fn starts_clock(text: &str) -> bool {
    starts_with_ignore_case(trim_start(text), "CLOCK:")
}

/// Start of the whitespace before a description separator (`TERM :: text`).
fn find_tag_separator(rest: &str) -> Option<usize> {
    rest.rmatch_indices("::").map(|(index, _)| index).find(|index| {
        *index > 0
            && is_space(rest.as_bytes()[*index - 1])
            && rest.as_bytes().get(*index + 2).is_none_or(|b| is_space(*b))
    })
}

/// Length of a counter cookie: `[@5]`, `[@start:5]` or `[@a]`.
fn counter_len(text: &str) -> Option<usize> {
    let rest = text.strip_prefix("[@")?;
    let close = rest.find(']')?;
    let value = &rest[..close];
    let value = value.strip_prefix("start:").unwrap_or(value);
    let valid = (!value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()))
        || (value.len() == 1 && value.as_bytes()[0].is_ascii_alphabetic());
    valid.then_some(2 + close + 1)
}

/// How an element starts, together with what was learned while checking.
enum Start {
    Drawer { end: usize },
    Block { end: usize, raw: bool },
    DynamicBlock { end: usize },
    LatexEnvironment { end: usize },
    Keyword,
    FixedWidth,
    Rule,
    DiarySexp,
    Table,
    TableEl { end: usize },
    List,
    FootnoteDefinition,
}

/// Where a property drawer may still appear before the first headline.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Top {
    /// Not in the first section, or too late in it.
    No,
    /// No element has been seen yet.
    First,
    /// The only element so far is a comment.
    AfterComment,
}

pub(crate) fn parse(src: &str, todo: &TodoKeywords) -> GreenNode {
    let mut parser = Parser {
        src,
        todo,
        builder: GreenNodeBuilder::new(),
        affiliated: Vec::new(),
        depth: 0,
        closings: HashMap::new(),
    };
    let lines = split_lines(src);
    parser.document(&lines);
    parser.builder.finish()
}

struct Parser<'a> {
    src: &'a str,
    todo: &'a TodoKeywords,
    builder: GreenNodeBuilder<'static>,
    /// Affiliated keyword lines waiting for the element they belong to.
    affiliated: Vec<Line>,
    /// How many containers (blocks, drawers, items …) enclose the current lines.
    depth: usize,
    /// The closing lines of the ranges of lines searched, by the byte
    /// where each range ends: an opener's closing line is looked up there
    /// rather than searched for, which keeps documents full of unclosed
    /// openers from being rescanned for each one.
    closings: HashMap<usize, Closings>,
}

/// The lines that close something in a range of lines that ends at a
/// given byte, from `start` on.
struct Closings {
    start: usize,
    /// The start of each line that each closer matches, in order.
    lines: HashMap<Closer, Vec<usize>>,
}

/// What kind of closing line an opener needs.
#[derive(Clone, PartialEq, Eq, Hash)]
enum Closer {
    /// `#+END_NAME`, with the name in lower case.
    Block(String),
    /// `#+END:` or `#+END`, which closes a dynamic block.
    DynamicBlock,
    /// `#+END:` exactly, which is what the list rules look for.
    DynamicBlockStrict,
    /// `:END:`.
    Drawer,
    /// A line ending in `\end{NAME}`.
    Latex(String),
}

impl Closer {
    /// Every closer `line` matches: what `matches` says yes to.
    fn all_matching(line: &str) -> Vec<Closer> {
        let mut closers = Vec::new();
        let trimmed = trim(line);
        if starts_with_ignore_case(trimmed, "#+end_") {
            closers.push(Closer::Block(trimmed[6..].to_ascii_lowercase()));
        }
        if is_dynamic_block_end(line) {
            closers.push(Closer::DynamicBlock);
        }
        if trimmed.eq_ignore_ascii_case("#+end:") {
            closers.push(Closer::DynamicBlockStrict);
        }
        if is_drawer_end(line) {
            closers.push(Closer::Drawer);
        }
        if let Some(body) = trim_end(line).strip_suffix('}') {
            for (at, _) in body.match_indices("\\end{") {
                closers.push(Closer::Latex(body[at + 5..].to_string()));
            }
        }
        closers
    }

    /// Whether `line` closes what needs this closer: `all_matching` says
    /// the same of every line, and the tests hold it to that.
    #[cfg(test)]
    fn matches(&self, line: &str) -> bool {
        match self {
            Closer::Block(name) => is_block_end(line, name),
            Closer::DynamicBlock => is_dynamic_block_end(line),
            Closer::DynamicBlockStrict => trim(line).eq_ignore_ascii_case("#+end:"),
            Closer::Drawer => is_drawer_end(line),
            Closer::Latex(name) => trim_end(line)
                .strip_suffix('}')
                .and_then(|line| line.strip_suffix(name.as_str()))
                .is_some_and(|line| line.ends_with("\\end{")),
        }
    }
}

impl<'a> Parser<'a> {
    // ----- small helpers

    fn text(&self, line: Line) -> &'a str {
        &self.src[line.start..line.end]
    }

    fn token(&mut self, kind: SyntaxKind, text: &str) {
        if !text.is_empty() {
            self.builder.token(kind.into(), text);
        }
    }

    fn start(&mut self, kind: SyntaxKind) {
        self.builder.start_node(kind.into());
    }

    /// Open an element node; pending affiliated keywords become its first children.
    fn start_element(&mut self, kind: SyntaxKind) {
        self.start(kind);
        for line in std::mem::take(&mut self.affiliated) {
            self.keyword(line);
        }
    }

    fn finish(&mut self) {
        self.builder.finish_node();
    }

    fn newline(&mut self, line: Line) {
        let src = self.src;
        self.token(NEWLINE, &src[line.end..line.next]);
    }

    fn objects(&mut self, text: &str, mode: Mode) {
        parse_objects(&mut self.builder, text, mode);
    }

    /// Emit the whitespace at `pos` in `text` and return the position after it.
    fn whitespace_at(&mut self, text: &str, pos: usize) -> usize {
        let len = indent_len(&text[pos..]);
        self.token(WHITESPACE, &text[pos..pos + len]);
        pos + len
    }

    /// Emit `indent, body as one token, trailing whitespace, newline`.
    fn simple_line(&mut self, line: Line, kind: SyntaxKind) {
        let text = self.text(line);
        let indent = indent_len(text);
        let body = trim_end(&text[indent..]);
        self.token(WHITESPACE, &text[..indent]);
        self.token(kind, body);
        self.token(WHITESPACE, &text[indent + body.len()..]);
        self.newline(line);
    }

    fn blank_line(&mut self, line: Line) {
        let src = self.src;
        self.token(BLANK_LINE, &src[line.start..line.next]);
    }

    /// First line at or after `from` that matches `closer`.
    ///
    /// The closing lines of `lines` are found once, for every closer, and
    /// kept by where `lines` ends; a range that starts earlier, with the
    /// same end, adds the lines before.
    fn find_closing(&mut self, lines: &[Line], from: usize, closer: Closer) -> Option<usize> {
        let (first, end) = (lines.first()?.start, lines.last()?.next);
        let known = self.closings.get(&end).map_or(end, |closings| closings.start);
        if first < known {
            let src = self.src;
            let mut added: HashMap<Closer, Vec<usize>> = HashMap::new();
            for line in lines.iter().take_while(|line| line.start < known) {
                for matching in Closer::all_matching(&src[line.start..line.end]) {
                    added.entry(matching).or_default().push(line.start);
                }
            }
            let closings = self.closings.entry(end).or_insert_with(|| Closings { start: end, lines: HashMap::new() });
            for (matching, mut starts) in added {
                let known = closings.lines.entry(matching).or_default();
                starts.append(known);
                *known = starts;
            }
            closings.start = first;
        }
        let start = lines.get(from)?.start;
        let starts = self.closings.get(&end)?.lines.get(&closer)?;
        let found = *starts.get(starts.partition_point(|line| *line < start))?;
        Some(lines.partition_point(|line| line.start < found))
    }

    /// The line closing the block or drawer that opens at `lines[i]`, by the
    /// rules Org uses when it skips over them inside list items.
    fn closing_line(&mut self, lines: &[Line], i: usize) -> Option<usize> {
        let text = trim_start(self.text(lines[i]));
        let closer = if let Some(name) = block_name(text) {
            Closer::Block(name.to_ascii_lowercase())
        } else if starts_with_ignore_case(text, "#+begin:") {
            Closer::DynamicBlockStrict
        } else if is_drawer_begin(text) {
            Closer::Drawer
        } else {
            return None;
        };
        self.find_closing(lines, i + 1, closer)
    }

    /// The line holding `\end{NAME}` for the environment opening at
    /// `lines[i]`; it may be that same line.
    fn latex_environment_end(&mut self, lines: &[Line], i: usize) -> Option<usize> {
        let name = latex_environment_name(trim_start(self.text(lines[i])))?;
        self.find_closing(lines, i, Closer::Latex(name.to_string()))
    }

    // ----- outline

    fn document(&mut self, lines: &[Line]) {
        self.start(DOCUMENT);
        let is_headline = |parser: &Self, line: &Line| headline_level(parser.text(*line)).is_some();
        let first = lines.iter().position(|line| is_headline(self, line)).unwrap_or(lines.len());
        if first > 0 {
            self.start(SECTION);
            self.elements(&lines[..first], false, true);
            self.finish();
            self.closings.clear();
        }
        let mut open: Vec<usize> = Vec::new();
        let mut i = first;
        while i < lines.len() {
            let level = headline_level(self.text(lines[i])).expect("headline line");
            while open.last().is_some_and(|top| *top >= level) {
                self.finish();
                open.pop();
            }
            self.start(HEADLINE);
            open.push(level);
            self.headline_line(lines[i], level);
            let mut j = i + 1;
            while j < lines.len() && !is_headline(self, &lines[j]) {
                j += 1;
            }
            if j > i + 1 {
                self.headline_section(&lines[i + 1..j]);
            }
            i = j;
        }
        for _ in open {
            self.finish();
        }
        self.finish();
    }

    fn headline_line(&mut self, line: Line, level: usize) {
        let text = self.text(line);
        self.token(STARS, &text[..level]);
        let mut pos = self.whitespace_at(text, level);

        // A keyword is followed by a space or ends the line.
        let word_len = text[pos..].find([' ', '\t']).unwrap_or(text.len() - pos);
        let word = &text[pos..pos + word_len];
        let after_word = &text[pos + word_len..];
        if self.todo.contains(word) && (after_word.is_empty() || after_word.starts_with(' ')) {
            self.token(TODO_KEYWORD, word);
            pos = self.whitespace_at(text, pos + word_len);
        }

        // The first priority cookie on the line counts, wherever it is.
        // Org drops what stands before it from the title; here it stays
        // in the tree as plain text.
        if let Some((start, len)) = priority_cookie(&text[pos..]) {
            if start > 0 {
                let before = &text[pos..pos + start];
                let kept = trim_end(before);
                self.token(TEXT, kept);
                self.token(WHITESPACE, &before[kept.len()..]);
            }
            let cookie_end = pos + start + len;
            self.token(PRIORITY, &text[pos + start..cookie_end]);
            // One space belongs to the cookie; `COMMENT` must follow at once.
            let after_cookie = cookie_end + usize::from(text[cookie_end..].starts_with(' '));
            if is_comment_keyword(&text[after_cookie..]) {
                self.token(WHITESPACE, &text[cookie_end..after_cookie]);
                pos = after_cookie;
            } else {
                pos = self.whitespace_at(text, cookie_end);
            }
        }

        if is_comment_keyword(&text[pos..]) {
            self.token(COMMENT_KEYWORD, COMMENT_KEYWORD_TEXT);
            pos = self.whitespace_at(text, pos + COMMENT_KEYWORD_TEXT.len());
        }
        let (title, gap, tags, trailing) = split_tags(text, pos);
        if !title.is_empty() {
            self.start(TITLE);
            self.objects(title, Mode::STANDARD);
            self.finish();
        }
        self.token(WHITESPACE, gap);
        if !tags.is_empty() {
            self.start(TAGS);
            let mut rest = tags;
            while !rest.is_empty() {
                if let Some(after) = rest.strip_prefix(':') {
                    self.token(COLON, ":");
                    rest = after;
                } else {
                    let len = rest.find(':').unwrap_or(rest.len());
                    self.token(TAG, &rest[..len]);
                    rest = &rest[len..];
                }
            }
            self.finish();
        }
        self.token(WHITESPACE, trailing);
        self.newline(line);
    }

    /// The lines between a headline and the next one. Planning is only
    /// valid directly below the headline, the property drawer after either.
    fn headline_section(&mut self, lines: &[Line]) {
        self.start(SECTION);
        let mut i = 0;
        let first = trim_start(self.text(lines[0]));
        if PLANNING_KEYWORDS.iter().any(|keyword| starts_with_ignore_case(first, keyword)) {
            self.planning(lines[0]);
            i = 1;
        }
        // The drawer comes directly after the headline, or after the
        // planning line, where blank lines may come between.
        let drawer_at = if i == 1 { i + lines[i..].iter().take_while(|line| line.blank).count() } else { 0 };
        if let Some(end) = self.property_drawer_end(lines, drawer_at) {
            for line in &lines[i..drawer_at] {
                self.blank_line(*line);
            }
            self.drawer(&lines[drawer_at..=end], PROPERTY_DRAWER);
            i = end + 1;
        }
        self.elements(&lines[i..], false, false);
        self.finish();
        // Searches never cross a headline, so what was learned about this
        // section is of no use to the next one.
        self.closings.clear();
    }

    fn planning(&mut self, line: Line) {
        self.start(PLANNING);
        let text = self.text(line);
        let indent = indent_len(text);
        let body = trim_end(&text[indent..]);
        self.token(WHITESPACE, &text[..indent]);
        // Where each keyword comes next, from where it was last looked
        // for: a keyword missing from the rest of the line is not looked
        // for again after each timestamp.
        let mut next_at: [Option<Option<usize>>; 3] = [None; 3];
        let mut at = 0;
        while at < body.len() {
            let rest = &body[at..];
            if let Some(keyword) = PLANNING_KEYWORDS.iter().find(|keyword| rest.starts_with(**keyword)) {
                self.token(PLANNING_KEYWORD, keyword);
                at += keyword.len();
                continue;
            }
            let mut next = body.len();
            for (keyword, known) in PLANNING_KEYWORDS.iter().zip(&mut next_at) {
                let found = match *known {
                    Some(found) if found.is_none_or(|found| found >= at) => found,
                    _ => rest.find(keyword).map(|offset| at + offset),
                };
                *known = Some(found);
                next = next.min(found.unwrap_or(body.len()));
            }
            let chunk = &body[at..next];
            let space = indent_len(chunk);
            self.token(WHITESPACE, &chunk[..space]);
            self.objects(&chunk[space..], Mode::TIMESTAMPS_ONLY);
            at = next;
        }
        self.token(WHITESPACE, &text[indent + body.len()..]);
        self.newline(line);
        self.finish();
    }

    /// Index of the `:END:` line if `lines[i]` opens a valid property drawer.
    fn property_drawer_end(&self, lines: &[Line], i: usize) -> Option<usize> {
        let first = self.text(*lines.get(i)?);
        if !trim(first).eq_ignore_ascii_case(":PROPERTIES:") {
            return None;
        }
        for (offset, line) in lines[i + 1..].iter().enumerate() {
            let text = self.text(*line);
            if is_drawer_end(text) {
                return Some(i + 1 + offset);
            }
            property_key_len(trim_start(text))?;
        }
        None
    }

    // ----- elements

    /// Parse a run of lines into elements.
    ///
    /// `inline_first` is set when the first line continues an item or a
    /// footnote definition, so it always starts a paragraph. `zeroth` is set
    /// for the text before the first headline, where a property drawer may
    /// open the document or directly follow a leading comment.
    fn elements(&mut self, lines: &[Line], inline_first: bool, zeroth: bool) {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            self.plain_paragraphs(lines);
        } else {
            self.elements_unbounded(lines, inline_first, zeroth);
        }
        self.depth -= 1;
    }

    /// Every run of non-blank lines as one paragraph: what is left of the
    /// element rules once nesting is too deep.
    fn plain_paragraphs(&mut self, lines: &[Line]) {
        let mut i = 0;
        while i < lines.len() {
            if lines[i].blank {
                self.blank_line(lines[i]);
                i += 1;
                continue;
            }
            let end = lines[i..].iter().position(|line| line.blank).map_or(lines.len(), |offset| i + offset);
            self.emit_paragraph(lines, i, end);
            i = end;
        }
    }

    fn elements_unbounded(&mut self, lines: &[Line], inline_first: bool, zeroth: bool) {
        let mut top = if zeroth { Top::First } else { Top::No };
        let mut i = 0;
        if inline_first && !lines.is_empty() {
            i = self.paragraph(lines, 0);
        }
        // Where a run of affiliated keywords known to attach to nothing
        // ends: its lines are keywords of their own, and the run is not
        // looked over again from each of them.
        let mut orphaned_until = 0;
        while i < lines.len() {
            let text = self.text(lines[i]);
            if lines[i].blank {
                self.blank_line(lines[i]);
                i += 1;
                continue;
            }
            if top != Top::No {
                if let Some(end) = self.property_drawer_end(lines, i) {
                    self.drawer(&lines[i..=end], PROPERTY_DRAWER);
                    i = end + 1;
                    top = Top::No;
                    continue;
                }
            }
            // Comments and clock lines are checked before affiliated keywords.
            if is_comment(trim_start(text)) {
                i = self.line_run(lines, i, COMMENT, COMMENT_TEXT, is_comment);
                top = if top == Top::First { Top::AfterComment } else { Top::No };
                continue;
            }
            top = Top::No;
            if is_clock_line(text) {
                self.clock(lines[i]);
                i += 1;
                continue;
            }
            // Affiliated keywords attach to the element below, unless what
            // follows is a blank line, a comment, a clock line or the end
            // of the container.
            let mut at = i;
            while i >= orphaned_until && at < lines.len() && is_affiliated(self.text(lines[at])) {
                at += 1;
            }
            let orphaned = at == lines.len() || lines[at].blank || {
                let next = self.text(lines[at]);
                is_comment(trim_start(next)) || starts_clock(next)
            };
            if at > i && !orphaned {
                self.affiliated = lines[i..at].to_vec();
            } else {
                orphaned_until = orphaned_until.max(at);
                at = i;
            }
            i = match self.element_at(lines, at) {
                Some(start) => self.element(lines, at, start),
                None => self.paragraph(lines, at),
            };
        }
    }

    /// Decide whether an element other than a paragraph starts at `lines[i]`.
    /// Comments and clock lines are handled by the caller.
    fn element_at(&mut self, lines: &[Line], i: usize) -> Option<Start> {
        let full = self.text(lines[i]);
        let text = trim_start(full);
        match *text.as_bytes().first()? {
            b'\\' => self.latex_environment_end(lines, i).map(|end| Start::LatexEnvironment { end }),
            b':' => {
                if is_drawer_begin(text) {
                    return self.closing_line(lines, i).map(|end| Start::Drawer { end });
                }
                is_fixed_width(text).then_some(Start::FixedWidth)
            }
            b'#' => {
                if let Some(name) = block_name(text) {
                    let raw = RAW_BLOCKS.iter().any(|raw| name.eq_ignore_ascii_case(raw));
                    return self.closing_line(lines, i).map(|end| Start::Block { end, raw });
                }
                if is_dynamic_block_begin(text) {
                    return self
                        .find_closing(lines, i + 1, Closer::DynamicBlock)
                        .map(|end| Start::DynamicBlock { end });
                }
                keyword_key_len(text).map(|_| Start::Keyword)
            }
            b'[' => footnote_definition_len(full).map(|_| Start::FootnoteDefinition),
            b'-' if is_rule(text) => Some(Start::Rule),
            b'%' if full.starts_with("%%(") => Some(Start::DiarySexp),
            b'|' => Some(Start::Table),
            b'+' if is_table_el_rule(text) => self.table_el_end(lines, i).map(|end| Start::TableEl { end }),
            // A letter starts an item with `org-list-allow-alphabetical`.
            b'-' | b'+' | b'*' | b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' => item_prefix(full).map(|_| Start::List),
            _ => None,
        }
    }

    /// Whether `lines[j]` ends a paragraph that started above it. This is
    /// `org-element-paragraph-separate` plus the checks Org makes on its
    /// matches; it is looser than `element_at` in places.
    fn ends_paragraph(&mut self, lines: &[Line], j: usize) -> bool {
        let full = self.text(lines[j]);
        if lines[j].blank || footnote_definition_len(full).is_some() || full.starts_with("%%(") {
            return true;
        }
        let text = trim_start(full);
        match text.as_bytes()[0] {
            b'|' => true,
            b'#' => {
                if is_comment(text) {
                    return true;
                }
                let Some(rest) = text.strip_prefix("#+") else { return false };
                if block_name(text).is_some() {
                    return self.closing_line(lines, j).is_some();
                }
                // `#+KEY[…]:` only counts for keywords that take an optional value.
                let word = &rest[..rest.find(char::is_whitespace).unwrap_or(rest.len())];
                if let Some(bracket) = word.find('[').filter(|index| *index > 0 && rest[*index..].contains("]:")) {
                    return DUAL_KEYWORDS.iter().any(|keyword| word[..bracket].eq_ignore_ascii_case(keyword));
                }
                word.rfind(':').is_some_and(|colon| colon > 0)
            }
            // A drawer line needs an `:END:` below it, and so does a stray
            // `:END:`, which looks like a drawer.
            b':' => is_fixed_width(text) || (is_drawer_begin(text) && self.closing_line(lines, j).is_some()),
            b'\\' => self.latex_environment_end(lines, j).is_some(),
            b'C' | b'c' => is_clock_line(text) || item_prefix(full).is_some(),
            b'-' if is_rule(text) => true,
            b'+' if is_table_el_rule(text) => true,
            // A lone `*`, or one followed by a tab, is not a headline but
            // still separates paragraphs.
            b'*' => text.len() == 1 || is_space(text.as_bytes()[1]),
            b'-' | b'+' | b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' => item_prefix(full).is_some(),
            _ => false,
        }
    }

    /// Emit the element starting at `lines[i]`; returns the next line index.
    fn element(&mut self, lines: &[Line], i: usize, start: Start) -> usize {
        match start {
            Start::Drawer { end } => {
                self.drawer(&lines[i..=end], DRAWER);
                end + 1
            }
            Start::Block { end, raw } => {
                self.block(&lines[i..=end], BLOCK, raw);
                end + 1
            }
            Start::DynamicBlock { end } => {
                self.block(&lines[i..=end], DYNAMIC_BLOCK, false);
                end + 1
            }
            Start::LatexEnvironment { end } => {
                self.start_element(LATEX_ENVIRONMENT);
                for line in &lines[i..=end] {
                    let text = self.text(*line);
                    self.token(TEXT, text);
                    self.newline(*line);
                }
                self.finish();
                end + 1
            }
            Start::Keyword => {
                self.start_element(KEYWORD);
                self.keyword_line(lines[i]);
                self.finish();
                i + 1
            }
            Start::FixedWidth => self.line_run(lines, i, FIXED_WIDTH, FIXED_WIDTH_TEXT, is_fixed_width),
            Start::Rule => {
                self.start_element(HORIZONTAL_RULE);
                self.simple_line(lines[i], RULE);
                self.finish();
                i + 1
            }
            Start::DiarySexp => {
                self.start_element(DIARY_SEXP);
                self.simple_line(lines[i], TEXT);
                self.finish();
                i + 1
            }
            Start::Table => self.table(lines, i),
            Start::TableEl { end } => self.table_el(lines, i, end),
            Start::List => self.list(lines, i),
            Start::FootnoteDefinition => self.footnote_definition(lines, i),
        }
    }

    fn paragraph(&mut self, lines: &[Line], i: usize) -> usize {
        let mut end = i + 1;
        while end < lines.len() && !self.ends_paragraph(lines, end) {
            end += 1;
        }
        self.emit_paragraph(lines, i, end);
        end
    }

    /// A paragraph made of `lines[i..end]`.
    fn emit_paragraph(&mut self, lines: &[Line], i: usize, end: usize) {
        let src = self.src;
        let last = lines[end - 1];
        let text = &src[lines[i].start..last.end];
        let indent = indent_len(text);
        let (body, trailing) = split_trailing(&text[indent..]);
        self.start_element(PARAGRAPH);
        self.token(WHITESPACE, &text[..indent]);
        self.objects(body, Mode::STANDARD);
        self.token(WHITESPACE, trailing);
        self.newline(last);
        self.finish();
    }

    /// Consecutive lines of the same simple kind (comments, fixed width).
    fn line_run(
        &mut self,
        lines: &[Line],
        i: usize,
        node: SyntaxKind,
        token: SyntaxKind,
        matches: fn(&str) -> bool,
    ) -> usize {
        self.start_element(node);
        let mut j = i;
        while j < lines.len() && matches(trim_start(self.text(lines[j]))) {
            self.simple_line(lines[j], token);
            j += 1;
        }
        self.finish();
        j
    }

    /// A `KEYWORD` node for one `#+KEY: value` line.
    fn keyword(&mut self, line: Line) {
        self.start(KEYWORD);
        self.keyword_line(line);
        self.finish();
    }

    fn keyword_line(&mut self, line: Line) {
        let text = self.text(line);
        let indent = indent_len(text);
        // Affiliated keywords with an optional value (`#+CAPTION[short]:`)
        // have their key end at the `]:`.
        let key_len = keyword_key_len(&text[indent..])
            .or_else(|| text[indent..].find("]:").map(|index| index + 2))
            .expect("keyword line");
        self.token(WHITESPACE, &text[..indent]);
        self.token(KEYWORD_KEY, &text[indent..indent + key_len]);
        let pos = self.whitespace_at(text, indent + key_len);
        let value = trim_end(&text[pos..]);
        self.token(KEYWORD_VALUE, value);
        self.token(WHITESPACE, &text[pos + value.len()..]);
        self.newline(line);
    }

    fn clock(&mut self, line: Line) {
        self.start(CLOCK);
        let text = self.text(line);
        let indent = indent_len(text);
        let keyword_end = indent + "CLOCK:".len();
        self.token(WHITESPACE, &text[..indent]);
        self.token(CLOCK_KEYWORD, &text[indent..keyword_end]);
        let pos = self.whitespace_at(text, keyword_end);
        let rest = trim_end(&text[pos..]);
        self.objects(rest, Mode::TIMESTAMPS_ONLY);
        self.token(WHITESPACE, &text[pos + rest.len()..]);
        self.newline(line);
        self.finish();
    }

    /// A drawer from its `:NAME:` line through its `:END:` line.
    fn drawer(&mut self, lines: &[Line], kind: SyntaxKind) {
        self.start_element(kind);
        self.simple_line(lines[0], DRAWER_BEGIN);
        let inner = &lines[1..lines.len() - 1];
        if kind == PROPERTY_DRAWER {
            for line in inner {
                self.node_property(*line);
            }
        } else {
            self.elements(inner, false, false);
        }
        self.simple_line(lines[lines.len() - 1], DRAWER_END);
        self.finish();
    }

    fn node_property(&mut self, line: Line) {
        self.start(NODE_PROPERTY);
        let text = self.text(line);
        let indent = indent_len(text);
        let key_len = property_key_len(&text[indent..]).expect("node property line");
        self.token(WHITESPACE, &text[..indent]);
        self.token(PROPERTY_KEY, &text[indent..indent + key_len]);
        let pos = self.whitespace_at(text, indent + key_len);
        let value = trim_end(&text[pos..]);
        self.token(PROPERTY_VALUE, value);
        self.token(WHITESPACE, &text[pos + value.len()..]);
        self.newline(line);
        self.finish();
    }

    /// A block from its `#+BEGIN` line through its `#+END` line.
    fn block(&mut self, lines: &[Line], kind: SyntaxKind, raw: bool) {
        self.start_element(kind);
        let first = lines[0];
        let text = self.text(first);
        let indent = indent_len(text);
        let body = trim_end(&text[indent..]);
        let begin_len = body.find([' ', '\t']).unwrap_or(body.len());
        self.token(WHITESPACE, &text[..indent]);
        self.token(BLOCK_BEGIN, &body[..begin_len]);
        let parameters = &body[begin_len..];
        let gap = indent_len(parameters);
        self.token(WHITESPACE, &parameters[..gap]);
        self.token(BLOCK_PARAMETERS, &parameters[gap..]);
        self.token(WHITESPACE, &text[indent + body.len()..]);
        self.newline(first);

        let inner = &lines[1..lines.len() - 1];
        if raw {
            for line in inner {
                let text = self.text(*line);
                self.token(TEXT, text);
                self.newline(*line);
            }
        } else {
            self.elements(inner, false, false);
        }

        self.simple_line(lines[lines.len() - 1], BLOCK_END);
        self.finish();
    }

    fn table(&mut self, lines: &[Line], i: usize) -> usize {
        self.start_element(TABLE);
        let mut j = i;
        while j < lines.len() && trim_start(self.text(lines[j])).starts_with('|') {
            self.table_row(lines[j]);
            j += 1;
        }
        // Formulas directly below the table belong to it.
        while j < lines.len() && starts_with_ignore_case(trim_start(self.text(lines[j])), "#+tblfm: ") {
            self.keyword(lines[j]);
            j += 1;
        }
        self.finish();
        j
    }

    /// Where the table.el table whose first rule is `lines[i]` ends, if it
    /// is one. Org has no strict definition; it wants a full rule first, a
    /// full rule last, and more than that one line.
    fn table_el_end(&self, lines: &[Line], i: usize) -> Option<usize> {
        let is_table_line = |line: &Line| trim_start(self.text(*line)).starts_with(['+', '|']);
        let end =
            lines[i + 1..].iter().position(|line| !is_table_line(line)).map_or(lines.len(), |offset| i + 1 + offset);
        (end > i + 1 && is_table_el_rule(trim_start(self.text(lines[end - 1])))).then_some(end)
    }

    /// A table.el table: its lines are kept as text.
    fn table_el(&mut self, lines: &[Line], i: usize, end: usize) -> usize {
        self.start_element(TABLE);
        for line in &lines[i..end] {
            self.simple_line(*line, TEXT);
        }
        let mut j = end;
        while j < lines.len() && starts_with_ignore_case(trim_start(self.text(lines[j])), "#+tblfm: ") {
            self.keyword(lines[j]);
            j += 1;
        }
        self.finish();
        j
    }

    fn table_row(&mut self, line: Line) {
        self.start(TABLE_ROW);
        let text = self.text(line);
        let indent = indent_len(text);
        let body = trim_end(&text[indent..]);
        self.token(WHITESPACE, &text[..indent]);
        if body.starts_with("|-") {
            self.token(TABLE_RULE, body);
        } else {
            self.token(PIPE, "|");
            let mut rest = &body[1..];
            while !rest.is_empty() {
                let (cell, closed) = match rest.find('|') {
                    Some(index) => (&rest[..index], true),
                    None => (rest, false),
                };
                self.start(TABLE_CELL);
                let lead = indent_len(cell);
                let inner = trim_end(&cell[lead..]);
                self.token(WHITESPACE, &cell[..lead]);
                self.objects(inner, Mode::TABLE_CELL);
                self.token(WHITESPACE, &cell[lead + inner.len()..]);
                self.finish();
                if closed {
                    self.token(PIPE, "|");
                    rest = &rest[cell.len() + 1..];
                } else {
                    rest = "";
                }
            }
        }
        self.token(WHITESPACE, &text[indent + body.len()..]);
        self.newline(line);
        self.finish();
    }

    // ----- lists and footnote definitions

    /// Number of lines belonging to the item that starts at `lines[i]`, and
    /// whether two blank lines ended it (which also ends the whole list).
    ///
    /// Lines belong to the item while they are indented deeper than its
    /// bullet. The contents of a block or drawer opened inside the item are
    /// skipped whatever their indentation.
    fn item_extent(&mut self, lines: &[Line], i: usize, indent: usize) -> (usize, bool) {
        let mut last = i;
        let mut blanks = 0;
        let mut j = i + 1;
        while j < lines.len() {
            if lines[j].blank {
                blanks += 1;
                if blanks >= 2 {
                    return (last + 1, true);
                }
            } else if lines[j].indent > indent {
                blanks = 0;
                last = self.closing_line(lines, j).unwrap_or(j);
                j = last;
            } else {
                break;
            }
            j += 1;
        }
        (last + 1, false)
    }

    fn list(&mut self, lines: &[Line], i: usize) -> usize {
        let indent = lines[i].indent;
        self.start_element(LIST);
        let mut item = i;
        let end = loop {
            let (item_end, ended) = self.item_extent(lines, item, indent);
            self.list_item(&lines[item..item_end]);
            if ended {
                break item_end;
            }
            // A single blank line may separate two items of the same list.
            let mut next = item_end;
            while next < lines.len() && lines[next].blank {
                next += 1;
            }
            let continues = next < lines.len()
                && next - item_end < 2
                && lines[next].indent == indent
                && item_prefix(self.text(lines[next])).is_some();
            if !continues {
                break item_end;
            }
            for line in &lines[item_end..next] {
                self.blank_line(*line);
            }
            item = next;
        };
        self.finish();
        end
    }

    fn list_item(&mut self, lines: &[Line]) {
        self.start(LIST_ITEM);
        let first = lines[0];
        let text = self.text(first);
        let prefix = item_prefix(text).expect("list item line");
        self.token(WHITESPACE, &text[..prefix.indent]);
        self.token(BULLET, &text[prefix.indent..prefix.indent + prefix.bullet]);
        let mut pos = self.whitespace_at(text, prefix.indent + prefix.bullet);

        if let Some(len) = counter_len(&text[pos..]) {
            self.token(COUNTER_SET, &text[pos..pos + len]);
            pos = self.whitespace_at(text, pos + len);
        }

        if let Some(checkbox) = text.get(pos..pos + 3) {
            let followed = text.as_bytes().get(pos + 3).is_none_or(|b| is_space(*b));
            if matches!(checkbox, "[ ]" | "[X]" | "[x]" | "[-]") && followed {
                self.token(CHECKBOX, checkbox);
                pos = self.whitespace_at(text, pos + 3);
            }
        }

        // `TERM :: description`, in unordered lists only.
        if !prefix.ordered {
            if let Some(separator) = find_tag_separator(&text[pos..]) {
                let region = &text[pos..pos + separator];
                let mut tag = trim_end(region);
                // Org keeps all but the last blank in the tag, which only
                // matters for the entity `\_ `, whose name ends in spaces.
                if tag.ends_with("\\_") && region.len() > tag.len() + 1 {
                    tag = &region[..region.len() - 1];
                }
                if !tag.is_empty() {
                    self.start(ITEM_TAG);
                    self.objects(tag, Mode::STANDARD);
                    self.finish();
                }
                self.token(WHITESPACE, &text[pos + tag.len()..pos + separator]);
                self.token(TAG_SEPARATOR, "::");
                pos = self.whitespace_at(text, pos + separator + 2);
            }
        }

        self.item_contents(lines, pos);
        self.finish();
    }

    /// Contents of an item or footnote definition whose first line has been
    /// consumed up to byte `pos`.
    fn item_contents(&mut self, lines: &[Line], pos: usize) {
        let first = lines[0];
        let text = self.text(first);
        if text[pos..].trim().is_empty() {
            self.token(WHITESPACE, &text[pos..]);
            self.newline(first);
            self.elements(&lines[1..], false, false);
        } else {
            // The rest of the first line starts the contents, as a paragraph.
            let mut contents = lines.to_vec();
            contents[0] = Line { start: first.start + pos, indent: 0, blank: false, ..first };
            self.elements(&contents, true, false);
        }
    }

    fn footnote_definition(&mut self, lines: &[Line], i: usize) -> usize {
        let mut last = i;
        let mut blanks = 0;
        let mut j = i + 1;
        while j < lines.len() {
            let text = self.text(lines[j]);
            if lines[j].blank {
                blanks += 1;
                if blanks >= 2 {
                    break;
                }
            } else if footnote_definition_len(text).is_some() {
                // Affiliated keywords above the next definition belong to it.
                while last > i && is_affiliated(self.text(lines[last])) {
                    last -= 1;
                }
                break;
            } else {
                last = j;
                blanks = 0;
            }
            j += 1;
        }
        let end = last + 1;
        self.start_element(FOOTNOTE_DEFINITION);
        let text = self.text(lines[i]);
        let label_len = footnote_definition_len(text).expect("footnote definition line");
        self.token(FOOTNOTE_LABEL, &text[..label_len]);
        let pos = self.whitespace_at(text, label_len);
        self.item_contents(&lines[i..end], pos);
        self.finish();
        end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_closers_each_line_matches() {
        let lines = [
            "#+end_src",
            "  #+END_Quote  ",
            "#+end:",
            "#+END",
            ":end:",
            "\\end{equation}",
            "x \\end{a\\end{b}",
            "#+end_",
            "#+ending",
            "text",
            "\\end{}",
            ":END: x",
        ];
        let closers = [
            Closer::Block("src".to_string()),
            Closer::Block("quote".to_string()),
            Closer::Block(String::new()),
            Closer::DynamicBlock,
            Closer::DynamicBlockStrict,
            Closer::Drawer,
            Closer::Latex("equation".to_string()),
            Closer::Latex("b".to_string()),
            Closer::Latex("a\\end{b".to_string()),
            Closer::Latex(String::new()),
        ];
        for line in lines {
            let all = Closer::all_matching(line);
            for closer in &closers {
                assert_eq!(all.contains(closer), closer.matches(line), "{line:?}");
            }
        }
    }
}
