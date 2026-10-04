//! Inline ("object") parsing: emphasis, links, timestamps and friends.
//!
//! The rules follow `org-element.el`: which objects exist, how each one is
//! delimited, and which may appear inside which container. Detection
//! (`match_at`) is kept separate from tree building (`emit`) so that pending
//! plain text can be flushed before an object is opened.

use rowan::GreenNodeBuilder;

use crate::entities::{ENTITIES, MAX_SPACE_ENTITY};
use crate::kind::SyntaxKind::{self, *};

/// Which objects are recognised in a piece of text. This mirrors
/// `org-element-object-restrictions`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Mode {
    /// Emphasis, sub/superscripts, entities and LaTeX fragments.
    minimal: bool,
    links: bool,
    footnotes: bool,
    timestamps: bool,
    cookies: bool,
    targets: bool,
    /// Export snippets and macros.
    snippets: bool,
    /// Inline source blocks and inline Babel calls.
    inline_code: bool,
    citations: bool,
}

impl Mode {
    /// Paragraphs, titles, item tags, emphasis and inline footnotes.
    pub(crate) const STANDARD: Mode = Mode {
        minimal: true,
        links: true,
        footnotes: true,
        timestamps: true,
        cookies: true,
        targets: true,
        snippets: true,
        inline_code: true,
        citations: true,
    };
    /// Table cells: no statistics cookies and no inline code.
    pub(crate) const TABLE_CELL: Mode = Mode { cookies: false, inline_code: false, ..Mode::STANDARD };
    /// Link descriptions: no links, footnotes, timestamps, targets or citations.
    const LINK_DESCRIPTION: Mode =
        Mode { links: false, footnotes: false, timestamps: false, targets: false, citations: false, ..Mode::STANDARD };
    /// Planning and clock lines.
    pub(crate) const TIMESTAMPS_ONLY: Mode = Mode {
        minimal: false,
        links: false,
        footnotes: false,
        timestamps: true,
        cookies: false,
        targets: false,
        snippets: false,
        inline_code: false,
        citations: false,
    };
}

/// Link types recognised for plain (`https://…`) and angle (`<https://…>`)
/// links: Org's defaults plus `attachment`.
const LINK_TYPES: &[&str] = &[
    "https",
    "http",
    "ftp",
    "mailto",
    "file+sys",
    "file+emacs",
    "file",
    "id",
    "news",
    "shell",
    "elisp",
    "doi",
    "help",
    "info",
    "attachment",
    "irc",
    "mhe",
    "rmail",
    "gnus",
    "bbdb",
    "bibtex",
    "docview",
    "w3m",
    "eww",
    "shortdoc",
];

enum Match {
    Emphasis {
        kind: SyntaxKind,
        end: usize,
    },
    Script {
        kind: SyntaxKind,
        braces: bool,
        end: usize,
    },
    Link {
        path_end: usize,
        description: Option<(usize, usize)>,
        end: usize,
    },
    AngleLink {
        end: usize,
    },
    InlineFootnote {
        open_end: usize,
        end: usize,
    },
    /// An object kept as a single token of the given kind.
    Leaf {
        node: SyntaxKind,
        token: SyntaxKind,
        end: usize,
    },
}

impl Match {
    fn end(&self) -> usize {
        match *self {
            Match::Emphasis { end, .. }
            | Match::Script { end, .. }
            | Match::Link { end, .. }
            | Match::AngleLink { end }
            | Match::InlineFootnote { end, .. }
            | Match::Leaf { end, .. } => end,
        }
    }

    fn leaf(node: SyntaxKind, token: SyntaxKind) -> impl Fn(usize) -> Match {
        move |end| Match::Leaf { node, token, end }
    }
}

/// Objects nested deeper than this are kept as plain text. Real documents
/// stay far below it; the limit keeps recursion bounded on hostile input.
const MAX_DEPTH: usize = 64;

/// Parse `text` into object nodes and `TEXT` tokens, appending to `builder`.
pub(crate) fn parse_objects(builder: &mut GreenNodeBuilder<'static>, text: &str, mode: Mode) {
    parse_nested(builder, text, mode, 0);
}

fn parse_nested(builder: &mut GreenNodeBuilder<'static>, text: &str, mode: Mode, depth: usize) {
    if depth >= MAX_DEPTH {
        if !text.is_empty() {
            builder.token(TEXT.into(), text);
        }
        return;
    }
    let mut scanner = Scanner::new(text, mode);
    let mut plain_start = 0;
    let mut i = 0;
    while i < text.len() {
        match scanner.match_at(i) {
            Some(found) => {
                if plain_start < i {
                    builder.token(TEXT.into(), &text[plain_start..i]);
                }
                emit(builder, text, i, &found, depth);
                i = found.end();
                plain_start = i;
            }
            None => i += 1,
        }
    }
    if plain_start < text.len() {
        builder.token(TEXT.into(), &text[plain_start..]);
    }
}

/// A closing delimiter that is searched for by plain substring search.
#[derive(Clone, Copy)]
enum Needle {
    LatexParen,
    LatexBracket,
    Dollars,
    Snippet,
    MacroEnd,
    LinkEnd,
    Angle,
}

impl Needle {
    const COUNT: usize = 7;

    fn text(self) -> &'static str {
        match self {
            Needle::LatexParen => "\\)",
            Needle::LatexBracket => "\\]",
            Needle::Dollars => "$$",
            Needle::Snippet => "@@",
            Needle::MacroEnd => ")}}}",
            Needle::LinkEnd => "]]",
            Needle::Angle => ">",
        }
    }
}

/// A bracket type for which matching pairs are looked up.
#[derive(Clone, Copy)]
enum Bracket {
    Square,
    Curly,
    Round,
}

impl Bracket {
    fn pair(self) -> (u8, u8) {
        match self {
            Bracket::Square => (b'[', b']'),
            Bracket::Curly => (b'{', b'}'),
            Bracket::Round => (b'(', b')'),
        }
    }
}

/// For each opening bracket: where its group ends and how deep it nests.
struct Pairs {
    /// Offset just past the matching closer, or 0 when the bracket at this
    /// index is not an opener or is never closed.
    end: Vec<u32>,
    /// Nesting depth of the group, counting the group itself; saturating.
    depth: Vec<u8>,
}

impl Pairs {
    fn new(bytes: &[u8], bracket: Bracket) -> Pairs {
        let (open, close) = bracket.pair();
        let mut end = vec![0; bytes.len()];
        let mut depth = vec![0; bytes.len()];
        // Each open bracket with the deepest nesting seen inside it so far.
        let mut stack: Vec<(usize, u8)> = Vec::new();
        for (i, b) in bytes.iter().enumerate() {
            if *b == open {
                stack.push((i, 0));
            } else if *b == close {
                if let Some((start, inner)) = stack.pop() {
                    let group = inner.saturating_add(1);
                    end[start] = i as u32 + 1;
                    depth[start] = group;
                    if let Some(parent) = stack.last_mut() {
                        parent.1 = parent.1.max(group);
                    }
                }
            }
        }
        Pairs { end, depth }
    }
}

/// Finds objects in one piece of text.
///
/// Every search for a closing delimiter is remembered, so that text with
/// many unclosed openers is still scanned in linear time.
struct Scanner<'t> {
    text: &'t str,
    bytes: &'t [u8],
    mode: Mode,
    /// Emphasis markers with no closing marker further on. Once a search
    /// from one position fails, every later one fails too.
    unclosed: [bool; 6],
    /// Per needle, the position from which it is known not to occur.
    absent: [usize; Needle::COUNT],
    /// Angle links starting before this position run into a line that
    /// cannot continue them.
    angle_blocked: usize,
    pairs: [Option<Pairs>; 3],
}

impl<'t> Scanner<'t> {
    fn new(text: &'t str, mode: Mode) -> Self {
        Scanner {
            text,
            bytes: text.as_bytes(),
            mode,
            unclosed: [false; 6],
            absent: [usize::MAX; Needle::COUNT],
            angle_blocked: 0,
            pairs: [None, None, None],
        }
    }

    /// Position of the first `needle` at or after `from`.
    fn find(&mut self, needle: Needle, from: usize) -> Option<usize> {
        let slot = needle as usize;
        if from >= self.absent[slot] {
            return None;
        }
        match self.text.get(from..)?.find(needle.text()) {
            Some(offset) => Some(from + offset),
            None => {
                self.absent[slot] = from;
                None
            }
        }
    }

    /// End of the balanced group whose opening bracket is at `i`, unless it
    /// is unclosed or nests deeper than `max_depth`.
    fn balanced_end(&mut self, i: usize, bracket: Bracket, max_depth: usize) -> Option<usize> {
        if self.bytes.get(i) != Some(&bracket.pair().0) {
            return None;
        }
        let bytes = self.bytes;
        let pairs = self.pairs[bracket as usize].get_or_insert_with(|| Pairs::new(bytes, bracket));
        let end = pairs.end[i] as usize;
        (end > 0 && usize::from(pairs.depth[i]) <= max_depth).then_some(end)
    }

    /// The object starting at byte `i`, if any. Candidates are tried in the
    /// order `org-element--object-lex` uses.
    fn match_at(&mut self, i: usize) -> Option<Match> {
        let (text, bytes, mode) = (self.text, self.bytes, self.mode);
        let timestamp = Match::leaf(TIMESTAMP, TIMESTAMP_TEXT);
        let cookie = Match::leaf(STATISTICS_COOKIE, COOKIE_TEXT);
        match bytes[i] {
            b'[' => match bytes.get(i + 1) {
                Some(b'[') if mode.links => self.match_bracket_link(i),
                Some(b'f') if mode.footnotes => self.match_footnote(i),
                Some(b'c') if mode.citations => self.match_citation(i).map(Match::leaf(CITATION, TEXT)),
                Some(b'%' | b'/') if mode.cookies => match_cookie(text, i).map(cookie),
                _ => {
                    if mode.timestamps {
                        if let Some(end) = match_timestamp(text, i) {
                            return Some(timestamp(end));
                        }
                    }
                    if mode.cookies {
                        return match_cookie(text, i).map(cookie);
                    }
                    None
                }
            },
            b'<' => {
                if bytes.get(i + 1) == Some(&b'<') {
                    return if mode.targets { match_target(text, i) } else { None };
                }
                if mode.timestamps {
                    if let Some(end) = match_timestamp(text, i) {
                        return Some(timestamp(end));
                    }
                }
                if mode.links {
                    return self.match_angle_link(i).map(|end| Match::AngleLink { end });
                }
                None
            }
            b'_' if mode.minimal => self.match_emphasis(i).or_else(|| self.match_script(i)),
            b'^' if mode.minimal => self.match_script(i),
            b'*' | b'/' | b'+' | b'=' | b'~' if mode.minimal => self.match_emphasis(i),
            b'\\' if mode.minimal => match_entity(text, i)
                .map(Match::leaf(ENTITY, TEXT))
                .or_else(|| self.match_latex_command(i).map(Match::leaf(LATEX_FRAGMENT, TEXT))),
            b'$' if mode.minimal => self.match_latex_math(i).map(Match::leaf(LATEX_FRAGMENT, TEXT)),
            b'@' if mode.snippets => self.match_export_snippet(i).map(Match::leaf(EXPORT_SNIPPET, TEXT)),
            b'{' if mode.snippets => self.match_macro(i).map(Match::leaf(MACRO, TEXT)),
            b'a'..=b'z' | b'A'..=b'Z' => {
                let rest = &text[i..];
                if rest.starts_with("src_") {
                    return if mode.inline_code {
                        self.match_inline_src(i).map(Match::leaf(INLINE_SRC_BLOCK, TEXT))
                    } else {
                        None
                    };
                }
                if rest.starts_with("call_") {
                    return if mode.inline_code {
                        self.match_inline_call(i).map(Match::leaf(INLINE_BABEL_CALL, TEXT))
                    } else {
                        None
                    };
                }
                if mode.links {
                    return match_plain_link(text, i).map(Match::leaf(PLAIN_LINK, LINK_PATH));
                }
                None
            }
            _ => None,
        }
    }
}

fn emit(builder: &mut GreenNodeBuilder<'static>, text: &str, start: usize, found: &Match, depth: usize) {
    match *found {
        Match::Emphasis { kind, end } => {
            builder.start_node(kind.into());
            builder.token(MARKER.into(), &text[start..start + 1]);
            let inner = &text[start + 1..end - 1];
            if kind == CODE || kind == VERBATIM {
                builder.token(TEXT.into(), inner);
            } else {
                parse_nested(builder, inner, Mode::STANDARD, depth + 1);
            }
            builder.token(MARKER.into(), &text[end - 1..end]);
            builder.finish_node();
        }
        Match::Script { kind, braces, end } => {
            builder.start_node(kind.into());
            builder.token(MARKER.into(), &text[start..start + 1]);
            if braces {
                builder.token(MARKER.into(), "{");
                parse_nested(builder, &text[start + 2..end - 1], Mode::STANDARD, depth + 1);
                builder.token(MARKER.into(), "}");
            } else {
                parse_nested(builder, &text[start + 1..end], Mode::STANDARD, depth + 1);
            }
            builder.finish_node();
        }
        Match::Link { path_end, description, end: _ } => {
            builder.start_node(LINK.into());
            builder.token(LINK_OPEN.into(), "[[");
            builder.token(LINK_PATH.into(), &text[start + 2..path_end]);
            if let Some((desc_start, desc_end)) = description {
                builder.token(LINK_SEPARATOR.into(), "][");
                parse_nested(builder, &text[desc_start..desc_end], Mode::LINK_DESCRIPTION, depth + 1);
            }
            builder.token(LINK_CLOSE.into(), "]]");
            builder.finish_node();
        }
        Match::AngleLink { end } => {
            builder.start_node(ANGLE_LINK.into());
            builder.token(ANGLE_OPEN.into(), "<");
            builder.token(LINK_PATH.into(), &text[start + 1..end - 1]);
            builder.token(ANGLE_CLOSE.into(), ">");
            builder.finish_node();
        }
        Match::InlineFootnote { open_end, end } => {
            builder.start_node(FOOTNOTE_REFERENCE.into());
            builder.token(FOOTNOTE_OPEN.into(), &text[start..open_end]);
            parse_nested(builder, &text[open_end..end - 1], Mode::STANDARD, depth + 1);
            builder.token(FOOTNOTE_CLOSE.into(), "]");
            builder.finish_node();
        }
        Match::Leaf { node, token, end } => {
            builder.start_node(node.into());
            builder.token(token.into(), &text[start..end]);
            builder.finish_node();
        }
    }
}

// ----- character helpers

fn prev_char(text: &str, i: usize) -> Option<char> {
    text[..i].chars().next_back()
}

fn next_char(text: &str, i: usize) -> Option<char> {
    text.get(i..)?.chars().next()
}

/// Whether a word in Latin script can start at `i`. Org's link and inline
/// code patterns begin with a word boundary, and Emacs sees one between
/// characters of different scripts, so `日本https://…` is still a link.
fn at_word_start(text: &str, i: usize) -> bool {
    match prev_char(text, i) {
        None => true,
        Some(c) if c.is_ascii() => !(c.is_ascii_alphanumeric() || matches!(c, '$' | '%' | '\'')),
        Some(c) => !(c.is_alphanumeric() && matches!(c as u32, 0xAA..=0x24F | 0x1E00..=0x1EFF)),
    }
}

// ----- emphasis

fn is_emphasis_pre(c: char) -> bool {
    c.is_whitespace() || matches!(c, '-' | '(' | '{' | '\'' | '"')
}

fn is_emphasis_post(c: char) -> bool {
    c.is_whitespace() || matches!(c, '-' | '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '"' | ')' | '}' | '\\' | '[')
}

fn emphasis_kind(marker: u8) -> Option<(SyntaxKind, usize)> {
    Some(match marker {
        b'*' => (BOLD, 0),
        b'/' => (ITALIC, 1),
        b'_' => (UNDERLINE, 2),
        b'+' => (STRIKE_THROUGH, 3),
        b'=' => (VERBATIM, 4),
        b'~' => (CODE, 5),
        _ => return None,
    })
}

impl Scanner<'_> {
    /// `PRE MARKER CONTENTS MARKER POST`, where the contents neither start nor
    /// end with whitespace. The start and end of the text count as PRE and POST.
    fn match_emphasis(&mut self, i: usize) -> Option<Match> {
        let (text, bytes) = (self.text, self.bytes);
        let marker = bytes[i];
        let (kind, slot) = emphasis_kind(marker)?;
        if self.unclosed[slot] {
            return None;
        }
        if let Some(before) = prev_char(text, i) {
            if !is_emphasis_pre(before) {
                return None;
            }
        }
        let first = next_char(text, i + 1)?;
        if first.is_whitespace() {
            return None;
        }
        let mut j = i + 1 + first.len_utf8();
        while j < bytes.len() {
            if bytes[j] == marker && !prev_char(text, j).is_some_and(char::is_whitespace) {
                match next_char(text, j + 1) {
                    None => return Some(Match::Emphasis { kind, end: j + 1 }),
                    Some(after) if is_emphasis_post(after) => return Some(Match::Emphasis { kind, end: j + 1 }),
                    Some(_) => {}
                }
            }
            j += 1;
        }
        self.unclosed[slot] = true;
        None
    }
}

// ----- sub- and superscripts

/// Nesting depth Org allows inside `_{…}` and `_(…)`.
const SCRIPT_DEPTH: usize = 3;

impl Scanner<'_> {
    /// `CHAR_SCRIPT` or `CHAR^SCRIPT`, where `SCRIPT` is `{…}`, `(…)`, `*`, or a
    /// signed run of alphanumerics, dots, commas and backslashes ending in an
    /// alphanumeric. `CHAR` is any non-blank character.
    fn match_script(&mut self, i: usize) -> Option<Match> {
        let (text, bytes) = (self.text, self.bytes);
        let kind = if bytes[i] == b'_' { SUBSCRIPT } else { SUPERSCRIPT };
        if prev_char(text, i).is_none_or(char::is_whitespace) {
            return None;
        }
        match *bytes.get(i + 1)? {
            b'{' => self.balanced_end(i + 1, Bracket::Curly, SCRIPT_DEPTH).map(|end| Match::Script {
                kind,
                braces: true,
                end,
            }),
            b'(' => self.balanced_end(i + 1, Bracket::Round, SCRIPT_DEPTH).map(|end| Match::Script {
                kind,
                braces: false,
                end,
            }),
            b'*' => Some(Match::Script { kind, braces: false, end: i + 2 }),
            first => {
                let start = if matches!(first, b'+' | b'-') { i + 2 } else { i + 1 };
                let mut end = None;
                for (offset, c) in text[start..].char_indices() {
                    if c.is_alphanumeric() {
                        end = Some(start + offset + c.len_utf8());
                    } else if !matches!(c, '.' | ',' | '\\') {
                        break;
                    }
                }
                end.map(|end| Match::Script { kind, braces: false, end })
            }
        }
    }
}

// ----- entities and LaTeX fragments

/// `\NAME` where `NAME` is a known entity and is not followed by a letter,
/// optionally closed with `{}`; or `\_` and spaces.
fn match_entity(text: &str, i: usize) -> Option<usize> {
    let rest = &text[i + 1..];
    if let Some(spaces) = rest.strip_prefix('_') {
        let count = spaces.bytes().take_while(|b| *b == b' ').count();
        return (1..=MAX_SPACE_ENTITY).contains(&count).then_some(i + 2 + count);
    }
    // A few names end in digits; they are tried before the plain letters.
    let numbered = ["there4", "sup1", "sup2", "sup3", "frac12", "frac14", "frac32", "frac34"];
    let terminated = |name: &str| !rest[name.len()..].chars().next().is_some_and(char::is_alphabetic);
    let name = numbered.into_iter().find(|name| rest.starts_with(name) && terminated(name)).or_else(|| {
        let len = rest.bytes().take_while(u8::is_ascii_alphabetic).count();
        (len > 0 && terminated(&rest[..len])).then_some(&rest[..len])
    })?;
    ENTITIES.binary_search(&name).ok()?;
    let braces = if rest[name.len()..].starts_with("{}") { 2 } else { 0 };
    Some(i + 1 + name.len() + braces)
}

impl Scanner<'_> {
    /// `\(…\)`, `\[…\]`, or `\command` with any `[…]` and `{…}` arguments.
    fn match_latex_command(&mut self, i: usize) -> Option<usize> {
        let bytes = self.bytes;
        match *bytes.get(i + 1)? {
            b'(' => self.find(Needle::LatexParen, i).map(|close| close + 2),
            b'[' => self.find(Needle::LatexBracket, i).map(|close| close + 2),
            first if first.is_ascii_alphabetic() => {
                let mut j = i + 1 + bytes[i + 1..].iter().take_while(|b| b.is_ascii_alphabetic()).count();
                if bytes.get(j) == Some(&b'*') {
                    j += 1;
                }
                // Arguments hold no brackets, braces or newlines of their own.
                loop {
                    let (close, stop): (u8, &[u8]) = match bytes.get(j) {
                        Some(b'[') => (b']', b"[]{}\n"),
                        Some(b'{') => (b'}', b"{}\n"),
                        _ => break,
                    };
                    match bytes[j + 1..].iter().position(|b| stop.contains(b)) {
                        Some(offset) if bytes[j + 1 + offset] == close => j += offset + 2,
                        _ => break,
                    }
                }
                Some(j)
            }
            _ => None,
        }
    }

    /// `$$…$$`, or `$…$` under Org's rules for telling maths from money.
    fn match_latex_math(&mut self, i: usize) -> Option<usize> {
        let (text, bytes) = (self.text, self.bytes);
        if bytes.get(i + 1) == Some(&b'$') {
            return self.find(Needle::Dollars, i + 2).map(|close| close + 2);
        }
        if prev_char(text, i) == Some('$') || matches!(bytes.get(i + 1)?, b' ' | b'\t' | b'\n' | b',' | b'.' | b';') {
            return None;
        }
        let close = i + 1 + text[i + 1..].find('$')?;
        if matches!(bytes[close - 1], b' ' | b'\t' | b'\n' | b',' | b'.') {
            return None;
        }
        // The closing `$` must be followed by whitespace, punctuation or a bracket.
        let allowed = match next_char(text, close + 1) {
            None => true,
            Some(c) if c.is_ascii() => c.is_ascii_whitespace() || "!#,.:;?@^`\"'()<>[]{}".contains(c),
            Some(c) => !c.is_alphanumeric(),
        };
        allowed.then_some(close + 1)
    }
}

// ----- snippets, macros and inline code

/// Length of the name after `src_` or `call_`: anything up to a blank or one
/// of the two `stop` brackets, which must then follow.
fn inline_name_len(rest: &str, stop: [u8; 2]) -> Option<usize> {
    let bytes = rest.as_bytes();
    let len = bytes.iter().take_while(|b| !matches!(b, b' ' | b'\t' | b'\n') && !stop.contains(b)).count();
    (len > 0 && bytes.get(len).is_some_and(|b| stop.contains(b))).then_some(len)
}

impl Scanner<'_> {
    /// `@@BACKEND:VALUE@@`.
    fn match_export_snippet(&mut self, i: usize) -> Option<usize> {
        let rest = self.text[i..].strip_prefix("@@")?;
        let backend = rest.bytes().take_while(|b| b.is_ascii_alphanumeric() || *b == b'-').count();
        if backend == 0 || rest.as_bytes().get(backend) != Some(&b':') {
            return None;
        }
        self.find(Needle::Snippet, i + 2 + backend + 1).map(|close| close + 2)
    }

    /// `{{{NAME}}}` or `{{{NAME(ARGUMENTS)}}}`.
    fn match_macro(&mut self, i: usize) -> Option<usize> {
        let rest = self.text[i..].strip_prefix("{{{")?;
        let bytes = rest.as_bytes();
        if !bytes.first()?.is_ascii_alphabetic() {
            return None;
        }
        let name = bytes.iter().take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')).count();
        let after = i + 3 + name;
        if rest[name..].starts_with("}}}") {
            return Some(after + 3);
        }
        if rest[name..].starts_with('(') {
            return self.find(Needle::MacroEnd, after).map(|close| close + 4);
        }
        None
    }

    /// `src_LANG{BODY}` or `src_LANG[HEADERS]{BODY}`.
    fn match_inline_src(&mut self, i: usize) -> Option<usize> {
        if !at_word_start(self.text, i) {
            return None;
        }
        let mut j = i + 4 + inline_name_len(&self.text[i + 4..], *b"[{")?;
        j = self.balanced_end(j, Bracket::Square, usize::MAX).unwrap_or(j);
        self.balanced_end(j, Bracket::Curly, usize::MAX)
    }

    /// `call_NAME(ARGUMENTS)`, with optional `[HEADERS]` before and after the arguments.
    fn match_inline_call(&mut self, i: usize) -> Option<usize> {
        if !at_word_start(self.text, i) {
            return None;
        }
        let mut j = i + 5 + inline_name_len(&self.text[i + 5..], *b"[(")?;
        j = self.balanced_end(j, Bracket::Square, usize::MAX).unwrap_or(j);
        j = self.balanced_end(j, Bracket::Round, usize::MAX)?;
        Some(self.balanced_end(j, Bracket::Square, usize::MAX).unwrap_or(j))
    }

    /// `[cite:…@key…]` or `[cite/STYLE:…]`, with balanced brackets and at least one key.
    fn match_citation(&mut self, i: usize) -> Option<usize> {
        let text = self.text;
        let rest = text[i..].strip_prefix("[cite")?;
        let style = match rest.strip_prefix('/') {
            Some(style) => {
                let len = style
                    .char_indices()
                    .find(|(_, c)| !(c.is_alphanumeric() || matches!(c, '/' | '_' | '-')))
                    .map_or(style.len(), |(index, _)| index);
                if len == 0 {
                    return None;
                }
                1 + len
            }
            None => 0,
        };
        if !rest[style..].starts_with(':') {
            return None;
        }
        let prefix_end = i + 5 + style + 1;
        let end = self.balanced_end(i, Bracket::Square, usize::MAX)?;
        let is_key_char = |c: char| c.is_alphanumeric() || "-.:?!`'/*@+|(){}<>&_^$#%~".contains(c);
        let body = &text[prefix_end..end - 1];
        body.match_indices('@')
            .any(|(index, _)| body[index + 1..].chars().next().is_some_and(is_key_char))
            .then_some(end)
    }
}

// ----- links

impl Scanner<'_> {
    /// `[[PATH]]` or `[[PATH][DESCRIPTION]]`.
    fn match_bracket_link(&mut self, i: usize) -> Option<Match> {
        let bytes = self.bytes;
        let mut j = i + 2;
        loop {
            match *bytes.get(j)? {
                b'\\' => j += 2,
                b']' => break,
                b'[' => return None,
                _ => j += 1,
            }
        }
        if j == i + 2 {
            return None;
        }
        let path_end = j;
        match bytes.get(j + 1) {
            Some(b']') => Some(Match::Link { path_end, description: None, end: j + 2 }),
            Some(b'[') => {
                // The description is at least one character long, so a `]`
                // right after `][` belongs to it.
                let desc_start = j + 2;
                let first = next_char(self.text, desc_start)?.len_utf8();
                let desc_end = self.find(Needle::LinkEnd, desc_start + first)?;
                Some(Match::Link { path_end, description: Some((desc_start, desc_end)), end: desc_end + 2 })
            }
            _ => None,
        }
    }

    /// `<TYPE:PATH>`.
    fn match_angle_link(&mut self, i: usize) -> Option<usize> {
        let path_start = link_type_at(self.text, i + 1)?;
        if path_start < self.angle_blocked {
            return None;
        }
        let close = self.find(Needle::Angle, path_start)?;
        // The path may continue on the next line unless that line is blank
        // or starts with the closing bracket.
        let bytes = self.bytes;
        let mut j = path_start;
        while j < close {
            if bytes[j] == b'\n' {
                j += 1;
                while matches!(bytes.get(j), Some(b' ' | b'\t')) {
                    j += 1;
                }
                if matches!(bytes.get(j), None | Some(b'>' | b'\n')) {
                    // Any link opened before this line fails here as well.
                    self.angle_blocked = j;
                    return None;
                }
            } else {
                j += 1;
            }
        }
        Some(close + 1)
    }
}

fn link_type_at(text: &str, i: usize) -> Option<usize> {
    // Org matches link types without regard to case.
    let rest = text.get(i..)?;
    LINK_TYPES
        .iter()
        .find(|ty| {
            rest.get(..ty.len()).is_some_and(|head| head.eq_ignore_ascii_case(ty))
                && rest.as_bytes().get(ty.len()) == Some(&b':')
        })
        .map(|ty| i + ty.len() + 1)
}

/// A character a plain link's path may contain outside brackets.
fn is_link_char(c: char) -> bool {
    !c.is_whitespace() && !matches!(c, '[' | ']' | '(' | ')' | '<' | '>')
}

/// A bracketed group in a plain link's path: an opening bracket of any
/// kind, link characters and groups nested one level deep, a closing bracket
/// of any kind. Returns where it ends.
fn link_group_end(text: &str, start: usize) -> Option<usize> {
    let mut pos = start + 1;
    loop {
        let c = text[pos..].chars().next()?;
        match c {
            ']' | ')' | '>' => return Some(pos + 1),
            '[' | '(' | '<' => {
                let inner: usize = text[pos + 1..].chars().take_while(|c| is_link_char(*c)).map(char::len_utf8).sum();
                let close = pos + 1 + inner;
                if !matches!(text.as_bytes().get(close), Some(b']' | b')' | b'>')) {
                    return None;
                }
                pos = close + 1;
            }
            c if is_link_char(c) => pos += c.len_utf8(),
            _ => return None,
        }
    }
}

/// `TYPE:PATH` in running text, such as `https://example.com`.
///
/// The path is a run of link characters and bracketed groups. It ends in a
/// group, a slash, a dash or a character that is not punctuation, and holds
/// at least one unit before that.
fn match_plain_link(text: &str, i: usize) -> Option<usize> {
    if !at_word_start(text, i) {
        return None;
    }
    let mut pos = link_type_at(text, i)?;
    let mut units = 0;
    let mut end = None;
    while let Some(c) = text[pos..].chars().next() {
        let (next, may_end) = if matches!(c, '[' | '(' | '<') {
            match link_group_end(text, pos) {
                Some(next) => (next, true),
                None => break,
            }
        } else if is_link_char(c) {
            let punctuation = if c.is_ascii() { c.is_ascii_punctuation() } else { !c.is_alphanumeric() };
            (pos + c.len_utf8(), !punctuation || c == '-' || c == '/')
        } else {
            break;
        };
        units += 1;
        if may_end && units >= 2 {
            end = Some(next);
        }
        pos = next;
    }
    end
}

// ----- timestamps

fn is_date_at(bytes: &[u8], i: usize) -> bool {
    let Some(date) = bytes.get(i..i + 10) else { return false };
    date.iter().enumerate().all(|(index, b)| match index {
        4 | 7 => *b == b'-',
        _ => b.is_ascii_digit(),
    })
}

/// `<DATE>` or `<DATE REST>` with either bracket type: the date is followed
/// directly by the closing bracket, or by a space and anything up to it.
fn match_single_timestamp(text: &str, i: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if !matches!(bytes.get(i), Some(b'<' | b'[')) || !is_date_at(bytes, i + 1) {
        return None;
    }
    match bytes.get(i + 11)? {
        b'>' | b']' => Some(i + 12),
        b' ' => first_closer(bytes, i + 12),
        _ => None,
    }
}

fn first_closer(bytes: &[u8], from: usize) -> Option<usize> {
    for (offset, b) in bytes.get(from..)?.iter().enumerate() {
        match b {
            b'\n' => return None,
            b'>' | b']' => return Some(from + offset + 1),
            _ => {}
        }
    }
    None
}

/// `<%%(SEXP)…>`: a diary timestamp. The object ends at the first closing
/// bracket of either kind.
fn match_diary_timestamp(text: &str, i: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let inner_start = i + text[i..].starts_with("<%%(").then_some(4)?;
    let close = inner_start + bytes[inner_start..].iter().position(|b| matches!(b, b'>' | b'\n'))?;
    let closes_sexp = bytes[inner_start..close].iter().skip(1).any(|b| *b == b')');
    if bytes[close] != b'>' || !closes_sexp {
        return None;
    }
    first_closer(bytes, i + 1)
}

/// A timestamp, optionally followed by `--` and a second one.
fn match_timestamp(text: &str, i: usize) -> Option<usize> {
    let end = match_single_timestamp(text, i).or_else(|| match_diary_timestamp(text, i))?;
    if text[end..].starts_with("--") {
        if let Some(range_end) = match_single_timestamp(text, end + 2) {
            return Some(range_end);
        }
    }
    Some(end)
}

// ----- footnotes, cookies, targets

impl Scanner<'_> {
    /// `[fn:LABEL]`, `[fn:LABEL:DEFINITION]` or `[fn::DEFINITION]`.
    fn match_footnote(&mut self, i: usize) -> Option<Match> {
        let text = self.text;
        let label_start = i + text[i..].starts_with("[fn:").then_some(4)?;
        let mut label_end = label_start;
        for (offset, c) in text[label_start..].char_indices() {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                label_end = label_start + offset + c.len_utf8();
            } else {
                break;
            }
        }
        match self.bytes.get(label_end)? {
            b']' if label_end > label_start => {
                Some(Match::Leaf { node: FOOTNOTE_REFERENCE, token: FOOTNOTE_LABEL, end: label_end + 1 })
            }
            b':' => self
                .balanced_end(i, Bracket::Square, usize::MAX)
                .map(|end| Match::InlineFootnote { open_end: label_end + 1, end }),
            _ => None,
        }
    }
}

/// `[N/M]` or `[N%]`; the numbers may be missing.
fn match_cookie(text: &str, i: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut j = i + 1;
    while bytes.get(j).is_some_and(u8::is_ascii_digit) {
        j += 1;
    }
    match bytes.get(j)? {
        b'%' => (bytes.get(j + 1) == Some(&b']')).then_some(j + 2),
        b'/' => {
            j += 1;
            while bytes.get(j).is_some_and(u8::is_ascii_digit) {
                j += 1;
            }
            (bytes.get(j) == Some(&b']')).then_some(j + 1)
        }
        _ => None,
    }
}

/// `<<TARGET>>` or `<<<RADIO TARGET>>>`.
fn match_target(text: &str, i: usize) -> Option<Match> {
    let rest = &text[i..];
    let (open, node) = if rest.starts_with("<<<") { (3, RADIO_TARGET) } else { (2, TARGET) };
    let body = &rest[open..];
    let len = body.find(['<', '>', '\n', '\r'])?;
    let inner = &body[..len];
    let close = &">>>"[..open];
    if inner.is_empty()
        || inner.starts_with([' ', '\t'])
        || inner.ends_with([' ', '\t'])
        || !body[len..].starts_with(close)
    {
        return None;
    }
    Some(Match::Leaf { node, token: TARGET_TEXT, end: i + open + len + open })
}
