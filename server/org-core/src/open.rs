//! `org-open-at-point` (`C-c C-o`): what opening the thing at a place
//! does (a link, a timestamp, a footnote, a tag, the links of an entry),
//! and `org-link-search`: where a search string leads in an Org text.
//!
//! Places in the same text are resolved here. A file or an `ID` elsewhere
//! is for the caller to look up, with `search_in_file` and `find_id`.

use std::sync::LazyLock;

use org_syntax::{Parse, SyntaxKind, SyntaxNode, TextSize};
use regex::Regex;

use crate::date::Date;

/// What opening the thing at a place does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Open {
    /// Point moves to this offset of the same text.
    Here(usize),
    /// A file, at the place `search` names (`file:notes.org::*Heading`).
    /// The path is as written: relative to the text's own file.
    /// `system`: the system's application opens it, not the editor.
    File { path: String, search: Option<String>, system: bool },
    /// The entry with this `ID` property, in some Org file.
    Id(String),
    /// A URL, for the system to open.
    External(String),
    /// An agenda of `span` days from `date` (a timestamp, a date range).
    Agenda { date: Date, span: i64 },
    /// The tags view for a tag.
    Tags(String),
    /// The links of an entry to pick from, by key.
    Choose(Vec<Choice>),
    /// A citation (`org-cite-follow`): the key of the reference at the
    /// place, or else all keys of the citation, one to pick.
    Citation(Vec<String>),
    /// Nothing to open; what Emacs says then.
    Nothing(String),
    /// A `roam:` link: the title or alias of an org-roam node.
    Roam(String),
    /// An `attachment:` link: a file of the entry's attachment directory,
    /// which Emacs knows (`org-attach-expand`), and a search after `::`.
    Attachment(String),
    /// A link to a headline of this text that is not there, which
    /// `org-link-search-must-match-exact-headline` offers to make: the
    /// search, as the link has it.
    Create(String),
}

/// `org-link-search-must-match-exact-headline`: what a search for a
/// headline that is not there does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExactHeadline {
    /// `nil`: the words are looked for in the text.
    No,
    /// `t`: an error.
    Yes,
    /// `query-to-create`: the headline is offered to be made.
    #[default]
    QueryToCreate,
}

/// How links are followed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// `org-link-search-must-match-exact-headline`
    pub exact_headline: ExactHeadline,
    /// `org-link-doi-server-url`
    pub doi_server: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { exact_headline: ExactHeadline::default(), doi_server: "https://doi.org/".to_string() }
    }
}

/// A link offered by `org-offer-links-in-entry`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub key: char,
    pub label: String,
}

/// Expands a link whose abbreviation calls a function (`%(…)`), which
/// only Emacs can (`org-link-expand-abbrev`): given the link and the
/// abbreviations in force, the file's first, the link it stands for.
pub type CallAbbreviation<'a> = &'a dyn Fn(&str, &[(String, String)]) -> Result<String, String>;

/// The link abbreviations in force, what expands those that call a
/// function, if anything, and how links are followed.
struct Abbreviations<'a> {
    list: Vec<(String, String)>,
    call: Option<CallAbbreviation<'a>>,
    settings: &'a Settings,
}

/// `org-open-at-point` at `offset`. `answer` is the key picked when the
/// links of an entry were offered. `global` is `org-link-abbrev-alist`.
pub fn at_point(
    text: &str,
    parse: &Parse,
    offset: usize,
    answer: Option<char>,
    global: &[(String, String)],
) -> Result<Open, String> {
    at_point_calling(text, parse, offset, answer, global, None, &Settings::default())
}

/// `at_point`, with `call` for the abbreviations that call a function.
pub fn at_point_calling(
    text: &str,
    parse: &Parse,
    offset: usize,
    answer: Option<char>,
    global: &[(String, String)],
    call: Option<CallAbbreviation>,
    settings: &Settings,
) -> Result<Open, String> {
    let abbreviations = Abbreviations { list: abbreviations(text, global), call, settings };
    // A word a radio target makes a link: to the target, when it is there
    // as written.
    // The link takes the blanks after it in.
    let blanks = |end: usize| end + text[end..].len() - text[end..].trim_start_matches([' ', '\t']).len();
    let radio =
        crate::radio::links(text, parse).into_iter().find(|link| link.start <= offset && offset < blanks(link.end));
    if let Some(radio) = radio {
        return radio.target.map(Open::Here).ok_or_else(|| NO_LINK.to_string());
    }
    let root = parse.syntax();
    let Some(token) = token_at(&root, offset) else { return Err(NO_LINK.to_string()) };
    // The nearest of the things Org opens, as `org-element-lineage` finds
    // it: the object under the cursor, else the element around it.
    for node in token.parent_ancestors() {
        match node.kind() {
            SyntaxKind::LINK | SyntaxKind::PLAIN_LINK | SyntaxKind::ANGLE_LINK => {
                return open_link(&node, text, parse, &abbreviations)
            }
            SyntaxKind::TIMESTAMP => return timestamp(&node.text().to_string()).ok_or_else(|| NO_LINK.to_string()),
            SyntaxKind::CITATION => {
                let start = usize::from(node.text_range().start());
                let references = crate::cite::references(&node.text().to_string(), start);
                // On a reference, its key; elsewhere, all keys.
                let keys =
                    match references.iter().find(|reference| reference.part.0 <= offset && offset < reference.part.1) {
                        Some(reference) => vec![reference.key.clone()],
                        None => references.iter().map(|reference| reference.key.clone()).collect(),
                    };
                return Ok(Open::Citation(keys));
            }
            SyntaxKind::FOOTNOTE_REFERENCE => return footnote_reference(&node, &root),
            SyntaxKind::FOOTNOTE_DEFINITION => {
                // On the label only, not on the blanks after it.
                let label = node.first_token().filter(|token| token.kind() == SyntaxKind::FOOTNOTE_LABEL);
                return match label {
                    Some(label) if offset < usize::from(label.text_range().end()) => {
                        previous_reference(&footnote_label(label.text()), &root, offset)
                    }
                    _ => Err(NO_LINK.to_string()),
                };
            }
            // Not on their timestamp, which is an object of its own.
            SyntaxKind::PLANNING | SyntaxKind::CLOCK => return Err(NO_LINK.to_string()),
            SyntaxKind::COMMENT | SyntaxKind::NODE_PROPERTY | SyntaxKind::KEYWORD => {
                return open_in_line(text, offset, &abbreviations);
            }
            SyntaxKind::BLOCK => match block_type(&node).as_deref() {
                Some("comment") => return open_in_line(text, offset, &abbreviations),
                Some("src") => return Err("Source block results are not supported".to_string()),
                _ => {}
            },
            SyntaxKind::HEADLINE => return headline(&node, text, parse, offset, answer, global, call, settings),
            _ => {}
        }
    }
    Err(NO_LINK.to_string())
}

const NO_LINK: &str = "No link found";

/// Whether RET follows a link at `offset` with `org-return-follows-link`
/// set (`org-return`): on a link or a citation, not on the blanks after
/// it, or within what reads as a timestamp or a link on the line, as
/// `org-in-regexp` finds them. Not in a table, where RET moves down.
pub fn return_follows(text: &str, parse: &Parse, offset: usize) -> bool {
    static TIMESTAMP: LazyLock<Regex> = LazyLock::new(|| {
        let one = r"[\[<][[:digit:]]{4}-[[:digit:]]{2}-[[:digit:]]{2}(?: .*?)?[\]>]";
        Regex::new(&format!("{one}(?:--?-?{one})?")).expect("a valid regex")
    });
    static BRACKET: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\[\[(?:[^\]\[\\]|\\(?:\\\\)*[\]\[]|\\+[^\]\[])+\](?:\[.+?\])?\]").expect("a valid regex")
    });
    let root = parse.syntax();
    let Some(token) = token_at(&root, offset) else { return false };
    let start = text[..offset].rfind('\n').map_or(0, |at| at + 1);
    let end = text[offset..].find('\n').map_or(text.len(), |at| offset + at);
    let line = &text[start..end];
    if line.trim_start().starts_with('|') && token.parent_ancestors().any(|node| node.kind() == SyntaxKind::TABLE) {
        return false;
    }
    let on_object = token.parent_ancestors().find(|node| {
        matches!(node.kind(), SyntaxKind::LINK | SyntaxKind::PLAIN_LINK | SyntaxKind::ANGLE_LINK | SyntaxKind::CITATION)
    });
    if let Some(node) = on_object {
        let node_start = usize::from(node.text_range().start());
        let node_text = node.text().to_string();
        // `org-element-context` on a citation's closing bracket finds no
        // citation.
        let closing = usize::from(node.kind() == SyntaxKind::CITATION);
        if offset + closing < node_start + node_text.trim_end_matches([' ', '\t']).len() {
            return true;
        }
    }
    // `org-in-regexp` with VISUALLY: a match on the line that starts at
    // or before the point and ends after it.
    let at = offset - start;
    let within =
        |regex: &Regex| regex.find_iter(line).take_while(|found| found.start() <= at).any(|found| found.end() > at);
    if within(&TIMESTAMP) || within(&BRACKET) {
        return true;
    }
    let angle = line.match_indices('<').take_while(|(from, _)| *from <= at).any(|(from, _)| {
        let Some(close) = line[from..].find('>') else { return false };
        let inner = &line[from + 1..from + close];
        from + close + 1 > at && inner.split_once(':').is_some_and(|(kind, _)| org_syntax::link_types().contains(&kind))
    });
    if angle {
        return true;
    }
    // A plain link: a link type at the start of a word.
    line.char_indices().take_while(|(from, _)| *from <= at).any(|(from, _)| {
        let word_start = line[..from].chars().next_back().is_none_or(|before| !before.is_alphanumeric());
        word_start && org_syntax::plain_link_end(line, from).is_some_and(|link_end| link_end > at)
    })
}

/// A link of a text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// Where it is.
    pub start: usize,
    pub end: usize,
    /// The link as Org reads it (`org-element-link-parser`'s raw link).
    pub raw: String,
    /// Where its path is written, and whether it is a bracket link, whose
    /// path is escaped.
    pub path: (usize, usize),
    pub bracket: bool,
    /// What opening it does.
    pub open: Result<Open, String>,
}

fn link_of(node: &SyntaxNode, text: &str, parse: &Parse, abbreviations: &Abbreviations) -> Link {
    let range = node.text_range();
    let path = node
        .children_with_tokens()
        .filter_map(|child| child.into_token())
        .find(|token| token.kind() == SyntaxKind::LINK_PATH)
        .map_or(range, |token| token.text_range());
    let (raw, bracket) = raw_link(node);
    Link {
        start: usize::from(range.start()),
        end: usize::from(range.end()),
        raw,
        path: (usize::from(path.start()), usize::from(path.end())),
        bracket,
        open: open_link(node, text, parse, abbreviations),
    }
}

fn is_link(node: &SyntaxNode) -> bool {
    matches!(node.kind(), SyntaxKind::LINK | SyntaxKind::PLAIN_LINK | SyntaxKind::ANGLE_LINK)
}

/// The links of a text: bracket, angle and plain links, in order.
/// `global` is `org-link-abbrev-alist`.
pub fn links(text: &str, parse: &Parse, global: &[(String, String)], settings: &Settings) -> Vec<Link> {
    let abbreviations = Abbreviations { list: abbreviations(text, global), call: None, settings };
    let mut links: Vec<Link> =
        parse.syntax().descendants().filter(is_link).map(|node| link_of(&node, text, parse, &abbreviations)).collect();
    // And the words radio targets make links.
    links.extend(crate::radio::links(text, parse).into_iter().map(|radio| Link {
        start: radio.start,
        end: radio.end,
        raw: text[radio.start..radio.end].to_string(),
        path: (radio.start, radio.end),
        bracket: false,
        open: radio.target.map(Open::Here).ok_or_else(|| NO_LINK.to_string()),
    }));
    links.sort_by_key(|link| link.start);
    links
}

/// The link the character at `offset` is in.
pub fn link_at(
    text: &str,
    parse: &Parse,
    offset: usize,
    global: &[(String, String)],
    settings: &Settings,
) -> Option<Link> {
    let token = token_at(&parse.syntax(), offset)?;
    let node = token.parent_ancestors().find(is_link)?;
    Some(link_of(&node, text, parse, &Abbreviations { list: abbreviations(text, global), call: None, settings }))
}

/// The link abbreviations of `text`: its `#+LINK:` lines, the last first
/// (`org-link-abbrev-alist-local`), then `global`.
pub fn abbreviations(text: &str, global: &[(String, String)]) -> Vec<(String, String)> {
    static LINK_LINE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(?im)^[ \t]*#\+LINK:[ \t]*(.*?)[ \t]*$"#).expect("a valid regex"));
    static QUOTED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"\A"(.+[^\\])"[ \t]+(.+)"#).expect("a valid regex"));
    static PLAIN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\A(\S+)[ \t]+(.+)").expect("a valid regex"));
    let mut local: Vec<(String, String)> = LINK_LINE
        .captures_iter(text)
        .filter_map(|line| {
            let value = line.get(1).map_or("", |found| found.as_str());
            QUOTED
                .captures(value)
                .or_else(|| PLAIN.captures(value))
                .map(|found| (found[1].to_string(), found[2].to_string()))
        })
        .collect();
    local.reverse();
    local.extend(global.iter().cloned());
    local
}

/// `org-link-expand-abbrev`: `raw` with its abbreviation (`key:tag`, or
/// `key` alone) replaced, `%s` by the tag, `%h` by the tag URL-encoded,
/// else the tag after it.
pub fn expand_abbreviation(raw: &str, abbreviations: &[(String, String)]) -> Result<String, String> {
    let Some((replacement, tag)) = abbreviation_of(raw, abbreviations) else { return Ok(raw.to_string()) };
    let tag = tag.unwrap_or("");
    if replacement.contains("%(") {
        return Err(format!("Link abbreviations calling a function are not supported: {replacement}"));
    }
    Ok(if replacement.contains("%s") {
        replacement.replacen("%s", tag, 1)
    } else if replacement.contains("%h") {
        replacement.replacen("%h", &hexify(tag), 1)
    } else {
        format!("{replacement}{tag}")
    })
}

/// The replacement of the abbreviation `raw` uses (`key:tag`, or `key`
/// alone), and its tag.
fn abbreviation_of<'a>(raw: &'a str, abbreviations: &'a [(String, String)]) -> Option<(&'a str, Option<&'a str>)> {
    let (key, tag) = match raw.find(':') {
        Some(at) => {
            let rest = &raw[at + 1..];
            (&raw[..at], Some(rest.strip_prefix(':').unwrap_or(rest)))
        }
        None => (raw, None),
    };
    abbreviations.iter().find(|(name, _)| name == key).map(|(_, replacement)| (replacement.as_str(), tag))
}

/// `url-hexify-string`: all but the unreserved characters percent-encoded.
fn hexify(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// The token the character at `offset` is in.
fn token_at(root: &SyntaxNode, offset: usize) -> Option<org_syntax::SyntaxToken> {
    let end = usize::from(root.text_range().end());
    if offset >= end {
        return root.last_token();
    }
    root.token_at_offset(TextSize::try_from(offset).ok()?).right_biased()
}

/// `src`, `comment`… of a block.
fn block_type(block: &SyntaxNode) -> Option<String> {
    let begin = block.children_with_tokens().find(|child| child.kind() == SyntaxKind::BLOCK_BEGIN)?;
    let text = begin.as_token()?.text().to_lowercase();
    Some(text.trim_start().trim_start_matches("#+begin_").to_string())
}

/// A link node: its raw link as `org-element-link-parser` reads it, and
/// whether it is a bracket link.
fn raw_link(node: &SyntaxNode) -> (String, bool) {
    let path = node
        .children_with_tokens()
        .filter_map(|child| child.into_token())
        .find(|token| token.kind() == SyntaxKind::LINK_PATH)
        .map(|token| token.text().to_string())
        .unwrap_or_default();
    match node.kind() {
        SyntaxKind::LINK => (collapse_newlines(&unescape(&path)), true),
        _ => (path, false),
    }
}

/// `org-link-unescape`: `\]` and `\[` stand for brackets, a backslash
/// before them for itself.
fn unescape(path: &str) -> String {
    let mut out = String::new();
    let mut slashes = 0;
    for c in path.chars() {
        match c {
            '\\' => slashes += 1,
            '[' | ']' if slashes > 0 => {
                out.extend(std::iter::repeat_n('\\', slashes / 2));
                out.push(c);
                slashes = 0;
            }
            _ => {
                out.extend(std::iter::repeat_n('\\', slashes));
                out.push(c);
                slashes = 0;
            }
        }
    }
    out.extend(std::iter::repeat_n('\\', slashes));
    out
}

/// A newline and the indentation around it read as one space.
fn collapse_newlines(text: &str) -> String {
    if !text.contains('\n') {
        return text.to_string();
    }
    let mut out = String::new();
    for (index, line) in text.split('\n').enumerate() {
        if index == 0 {
            out.push_str(line.trim_end_matches([' ', '\t']));
        } else {
            out.push(' ');
            out.push_str(line.trim_start_matches([' ', '\t']).trim_end_matches([' ', '\t']));
        }
    }
    out
}

/// The type of a link (`https` of `https://…`) among those Org knows, and
/// what follows its colon.
fn link_type(raw: &str) -> Option<(&str, &str)> {
    let (head, rest) = raw.split_once(':')?;
    org_syntax::link_types().iter().find(|known| known.eq_ignore_ascii_case(head)).map(|_| (head, rest))
}

fn open_link(node: &SyntaxNode, text: &str, parse: &Parse, abbreviations: &Abbreviations) -> Result<Open, String> {
    let (raw, bracket) = raw_link(node);
    // Only a bracket link's abbreviation is expanded; one that calls a
    // function, by Emacs.
    let list = &abbreviations.list;
    let calls = abbreviation_of(&raw, list).is_some_and(|(replacement, _)| replacement.contains("%("));
    let raw = match abbreviations.call {
        Some(call) if bracket && calls => call(&raw, list)?,
        _ if bracket => expand_abbreviation(&raw, list)?,
        _ => raw,
    };
    // org-roam's link type, which the server knows only with org-roam on.
    if let Some(title) = raw.strip_prefix("roam:").filter(|_| bracket) {
        return Ok(Open::Roam(title.to_string()));
    }
    // Only bracket links can be files without a type, or internal.
    if bracket && (raw.starts_with('/') || raw.starts_with('~') || raw.starts_with("./") || raw.starts_with("../")) {
        return Ok(file(&raw, None));
    }
    if let Some((kind, path)) = link_type(&raw) {
        return match kind.to_lowercase().as_str() {
            "file" => Ok(file(path, None)),
            "file+emacs" => Ok(file(path, Some("emacs"))),
            "file+sys" => Ok(file(path, Some("sys"))),
            "attachment" => Ok(Open::Attachment(path.to_string())),
            // `org-id-open`: an ID, and a search after `::`.
            "id" => Ok(Open::Id(path.split("::").next().unwrap_or(path).to_string())),
            "http" | "https" | "ftp" | "mailto" | "news" => Ok(Open::External(raw.clone())),
            // `org-link-doi-server-url`.
            "doi" => Ok(Open::External(format!("{}{path}", abbreviations.settings.doi_server))),
            other => Err(format!("Links of type \"{other}\" are not supported")),
        };
    }
    if !bracket {
        return Err(NO_LINK.to_string());
    }
    // Internal: a coderef `(name)`, a custom ID `#id`, or fuzzy, which
    // does not find itself.
    let exact = abbreviations.settings.exact_headline;
    let avoid = usize::from(node.text_range().start()) + 2;
    match search_with(text, parse, &raw, Some(avoid), exact) {
        Ok(at) => Ok(Open::Here(at)),
        Err(_) if exact == ExactHeadline::QueryToCreate && may_create(&raw) => Ok(Open::Create(raw)),
        Err(error) => Err(error),
    }
}

/// The regexp of a search `/regexp/`, which `org-link-search` gives to
/// `org-occur`.
pub fn occur_regexp(search: &str) -> Option<&str> {
    search.strip_prefix('/')?.strip_suffix('/').filter(|regexp| !regexp.is_empty())
}

/// A `file:` link to `path` (with a search after `::`), as [`file`] opens
/// it: an attachment once its directory is known.
pub fn file_link(path: &str) -> Open {
    file(path, None)
}

/// A `file:` link: the path, the search option after `::`, and who opens
/// it. `org-file-apps` sends PDF, HTML and FreeMind files to the system's
/// application by default, the rest to the editor.
fn file(path: &str, application: Option<&str>) -> Open {
    let (path, search) = match path.rfind("::") {
        Some(at) => (&path[..at], Some(path[at + 2..].to_string())),
        None => (path, None),
    };
    // `file:///home/…` is `/home/…`.
    let trimmed = path.trim_start_matches('/');
    let path = if path.starts_with("//") { format!("/{trimmed}") } else { path.to_string() };
    let lower = path.to_lowercase();
    let system_type = [".pdf", ".html", ".htm", ".xhtml", ".mm"].iter().any(|extension| lower.ends_with(extension));
    let system = match application {
        Some("sys") => true,
        Some("emacs") => false,
        _ => system_type,
    };
    Open::File { path, search, system }
}

/// `org-follow-timestamp-link`: the agenda of the day of a timestamp, or
/// of the days of a range.
fn timestamp(text: &str) -> Option<Open> {
    let dates: Vec<Date> = text
        .split("--")
        .filter_map(|part| {
            let part = part.trim_start_matches(['<', '[']);
            let year = part.get(0..4)?.parse().ok()?;
            let month = part.get(5..7)?.parse().ok()?;
            let day = part.get(8..10)?.parse().ok()?;
            (part.as_bytes().get(4) == Some(&b'-')).then(|| Date::new(year, month, day))
        })
        .collect();
    match dates.as_slice() {
        // A range of centuries opens as Emacs's would, cut short to the
        // longest agenda the server builds (`agenda::DAYS_MAX`).
        [first, last] => {
            Some(Open::Agenda { date: *first, span: (last.to_day() - first.to_day() + 1).min(crate::agenda::DAYS_MAX) })
        }
        [first, ..] => Some(Open::Agenda { date: *first, span: 1 }),
        [] => None,
    }
}

/// The label of `[fn:label]` or `[fn:label:definition]`; empty for an
/// anonymous footnote.
fn footnote_label(text: &str) -> String {
    let rest = text.trim_start_matches("[fn:");
    rest.split([':', ']']).next().unwrap_or("").to_string()
}

/// `org-footnote-goto-definition`: after the label of the definition.
fn footnote_reference(node: &SyntaxNode, root: &SyntaxNode) -> Result<Open, String> {
    let text = node.text().to_string();
    let label = footnote_label(&text);
    if label.is_empty() {
        return Err("This is an anonymous footnote".to_string());
    }
    let start = usize::from(node.text_range().start());
    // An inline definition is its own.
    if text[4 + label.len()..].starts_with(':') {
        return Ok(Open::Here(start + 4 + label.len() + 1));
    }
    root.descendants()
        .filter(|candidate| candidate.kind() == SyntaxKind::FOOTNOTE_DEFINITION)
        .find_map(|definition| {
            let first = definition.first_token().filter(|token| token.kind() == SyntaxKind::FOOTNOTE_LABEL)?;
            (footnote_label(first.text()) == label).then(|| usize::from(first.text_range().end()))
        })
        .map(Open::Here)
        .ok_or_else(|| format!("No definition for footnote {label}"))
}

/// `org-footnote-goto-previous-reference`: the nearest reference before
/// `offset`, else the first after it.
fn previous_reference(label: &str, root: &SyntaxNode, offset: usize) -> Result<Open, String> {
    let starts: Vec<usize> = root
        .descendants()
        .filter(|node| node.kind() == SyntaxKind::FOOTNOTE_REFERENCE)
        .filter(|node| footnote_label(&node.text().to_string()) == label)
        .map(|node| usize::from(node.text_range().start()))
        .collect();
    starts
        .iter()
        .rev()
        .find(|start| **start < offset)
        .or_else(|| starts.first())
        .map(|start| Open::Here(*start))
        .ok_or_else(|| format!("Cannot find reference of footnote \"{label}\""))
}

/// `org-open-at-point-global`, in a comment, a property or a keyword: a
/// link or a timestamp on the line, around `offset`.
fn open_in_line(text: &str, offset: usize, abbreviations: &Abbreviations) -> Result<Open, String> {
    let start = text[..offset].rfind('\n').map_or(0, |at| at + 1);
    let end = text[offset..].find('\n').map_or(text.len(), |at| offset + at);
    // The line as a paragraph: what precedes its text blanked out.
    let line = &text[start..end];
    let body = line.trim_start();
    let lead = line.len() - body.len();
    let marker = body.find(|c: char| c.is_whitespace()).filter(|_| body.starts_with('#') || body.starts_with(':'));
    let blank = lead + marker.unwrap_or(0);
    let paragraph = format!("{}{}", " ".repeat(blank), &line[blank..]);
    let parse = org_syntax::parse(&paragraph, &Default::default());
    let at = offset - start;
    for node in parse.syntax().descendants() {
        let range = node.text_range();
        if !(usize::from(range.start()) <= at && at < usize::from(range.end())) {
            continue;
        }
        match node.kind() {
            SyntaxKind::LINK | SyntaxKind::PLAIN_LINK | SyntaxKind::ANGLE_LINK => {
                return match open_link(&node, &paragraph, &parse, abbreviations)? {
                    // An internal link from here searches the text itself.
                    Open::Here(_) => Err(NO_LINK.to_string()),
                    open => Ok(open),
                };
            }
            SyntaxKind::TIMESTAMP => return timestamp(&node.text().to_string()).ok_or_else(|| NO_LINK.to_string()),
            _ => {}
        }
    }
    Err(NO_LINK.to_string())
}

/// On a headline: its tag under the cursor (`org-tags-view`), else the
/// links of the entry (`org-offer-links-in-entry`).
#[allow(clippy::too_many_arguments)]
fn headline(
    node: &SyntaxNode,
    text: &str,
    parse: &Parse,
    offset: usize,
    answer: Option<char>,
    global: &[(String, String)],
    call: Option<CallAbbreviation>,
    settings: &Settings,
) -> Result<Open, String> {
    let line_end = text[usize::from(node.text_range().start())..]
        .find('\n')
        .map_or(text.len(), |at| usize::from(node.text_range().start()) + at);
    if offset < line_end {
        if let Some(tags) = node.children().find(|child| child.kind() == SyntaxKind::TAGS) {
            let range = tags.text_range();
            let (start, end) = (usize::from(range.start()), usize::from(range.end()));
            if start <= offset && offset < end {
                // A colon belongs to the tag before it.
                let names = tags.text().to_string();
                let at = offset - start;
                let before = names[..at].rfind(':').filter(|_| at > 0).unwrap_or(0);
                let after = names[before + 1..].find(':').map_or(names.len(), |rest| before + 1 + rest);
                return Ok(Open::Tags(names[before + 1..after].to_string()));
            }
        }
    }
    let links = entry_links(node, text);
    let chosen = match links.len() {
        0 => return Ok(Open::Nothing("No links".to_string())),
        1 => 0,
        _ => match answer {
            None => {
                let choices = links
                    .iter()
                    .enumerate()
                    .map(|(index, (_, label, _))| Choice { key: offered_key(index), label: label.clone() })
                    .collect();
                return Ok(Open::Choose(choices));
            }
            Some('q') => return Err("Abort".to_string()),
            Some(key) => (0..links.len()).find(|index| offered_key(*index) == key).ok_or("Abort")?,
        },
    };
    at_point_calling(text, parse, links[chosen].2, None, global, call, settings)
}

/// The key of the link at `index` among those offered: `1`, `2`…
fn offered_key(index: usize) -> char {
    char::from_u32('1' as u32 + index as u32).unwrap_or('?')
}

/// The links of the entry of `headline` up to the next headline, each
/// once: their text, the label Emacs lists them with, and where the first
/// one starts.
fn entry_links(headline: &SyntaxNode, text: &str) -> Vec<(String, String, usize)> {
    let start = usize::from(headline.text_range().start());
    let end = headline
        .descendants()
        .skip(1)
        .find(|node| node.kind() == SyntaxKind::HEADLINE)
        .map_or(usize::from(headline.text_range().end()), |next| usize::from(next.text_range().start()));
    let mut found: Vec<(String, String, usize)> = Vec::new();
    let mut add = |link: &SyntaxNode, at: usize| {
        let whole = link.text().to_string();
        if found.iter().any(|(seen, _, _)| *seen == whole) {
            return;
        }
        let (raw, _) = raw_link(link);
        let label = match link.kind() {
            SyntaxKind::LINK => {
                let description = whole
                    .find("][")
                    .map(|at| whole[at + 2..whole.len().saturating_sub(2)].to_string())
                    .filter(|description| !description.is_empty());
                match description {
                    Some(description) => format!("{description} ({raw})"),
                    None => raw,
                }
            }
            _ => raw,
        };
        found.push((whole, label, at));
    };
    for node in headline.descendants() {
        let range = node.text_range();
        let at = usize::from(range.start());
        if at >= end {
            break;
        }
        match node.kind() {
            SyntaxKind::LINK | SyntaxKind::PLAIN_LINK | SyntaxKind::ANGLE_LINK => add(&node, at),
            // Links where Org's parser sees none, as `org-open-at-point`
            // still opens them.
            SyntaxKind::COMMENT | SyntaxKind::NODE_PROPERTY | SyntaxKind::KEYWORD => {
                let line = &text[at..usize::from(range.end())];
                let blanked: String =
                    line.chars().map(|c| if matches!(c, '#' | ':' | '+') { ' ' } else { c }).collect();
                let parse = org_syntax::parse(&blanked, &Default::default());
                for link in parse.syntax().descendants() {
                    if matches!(link.kind(), SyntaxKind::LINK | SyntaxKind::PLAIN_LINK | SyntaxKind::ANGLE_LINK) {
                        add(&link, at + usize::from(link.text_range().start()));
                    }
                }
            }
            _ => {}
        }
    }
    let _ = start;
    found
}

/// The search string of a link as `org-link-search` reads it, with the
/// newlines of a link that wraps as single spaces.
fn normalized(search: &str) -> String {
    collapse_newlines(search)
}

/// `org-link-search` in an Org text: where `search` leads. `#id` is a
/// custom ID, `(name)` a coderef, `*words` a headline, other words a
/// target, then a `#+NAME:`, then a headline. A headline must be there,
/// as with `org-link-search-must-match-exact-headline` set.
pub fn search(text: &str, parse: &Parse, search: &str) -> Result<usize, String> {
    search_with(text, parse, search, None, ExactHeadline::Yes)
}

/// Whether a search that found nothing is one `org-link-search` offers
/// to make a headline for: one that is not a custom ID, a coderef or a
/// regular expression.
pub fn may_create(search: &str) -> bool {
    let normalized = normalized(search);
    !(search.trim().is_empty()
        || normalized.starts_with('#')
        || (normalized.len() > 1 && normalized.starts_with('(') && normalized.ends_with(')'))
        || (normalized.len() > 1 && normalized.starts_with('/') && normalized.ends_with('/')))
}

/// `org-link-search`, with `org-link-search-must-match-exact-headline`
/// as `exact` says: without it, words that are no headline are looked
/// for in the text, leaving out a match over `avoid` (the link itself).
/// A headline to make is an error here, which the caller may offer.
pub fn search_with(
    text: &str,
    parse: &Parse,
    search: &str,
    avoid: Option<usize>,
    exact: ExactHeadline,
) -> Result<usize, String> {
    if search.trim().is_empty() {
        return Err(format!("Invalid search string \"{search}\""));
    }
    let normalized = normalized(search);
    let root = parse.syntax();
    if let Some(id) = normalized.strip_prefix('#') {
        return find_property(&root, "CUSTOM_ID", id).ok_or_else(|| format!("No match for custom ID: {id}"));
    }
    if normalized.len() > 1 && normalized.starts_with('(') && normalized.ends_with(')') {
        let name = &normalized[1..normalized.len() - 1];
        return coderef(&root, text, name).ok_or_else(|| format!("No match for coderef: {name}"));
    }
    if normalized.len() > 1 && normalized.starts_with('/') && normalized.ends_with('/') {
        return Err("A search by regular expression is followed only in the file the link is in".to_string());
    }
    let starred = normalized.starts_with('*');
    let words: Vec<String> = (if starred { &normalized[1..] } else { normalized.as_str() })
        .split_whitespace()
        .map(str::to_uppercase)
        .collect();
    let same = |text: &str| text.split_whitespace().map(str::to_uppercase).eq(words.iter().cloned());
    if !starred {
        // A target `<<words>>`.
        let target = root.descendants().find(|node| {
            node.kind() == SyntaxKind::TARGET
                && same(node.text().to_string().trim_start_matches("<<").trim_end_matches(">>"))
        });
        if let Some(target) = target {
            return Ok(usize::from(target.text_range().start()));
        }
        // An element named `#+NAME: words`.
        let named = root.descendants().find(|node| {
            node.kind() == SyntaxKind::KEYWORD && {
                let key = token_of(node, SyntaxKind::KEYWORD_KEY).unwrap_or_default();
                key.eq_ignore_ascii_case("#+NAME:")
                    && same(&token_of(node, SyntaxKind::KEYWORD_VALUE).unwrap_or_default())
            }
        });
        if let Some(named) = named {
            return Ok(usize::from(named.text_range().start()));
        }
    }
    // A headline, without its TODO keyword, priority, COMMENT, cookies and
    // tags.
    let headline = org_syntax::ast::headlines(&root).find(|headline| {
        let title = headline.title().map(|title| title.text().to_string()).unwrap_or_default();
        same(&without_cookies(&title))
    });
    if let Some(headline) = headline {
        return Ok(usize::from(headline.syntax().text_range().start()));
    }
    if exact == ExactHeadline::No && !starred {
        if let Some(at) = words_in_text(text, search, avoid) {
            return Ok(at);
        }
    }
    Err(format!("No match for fuzzy expression: {normalized}"))
}

/// The regular text search of `org-link-search`: the first place of the
/// words of `search`, in any case, with any blanks and newlines between
/// them, that does not hold `avoid`.
fn words_in_text(text: &str, search: &str, avoid: Option<usize>) -> Option<usize> {
    let words: Vec<String> = search.split_whitespace().map(regex::escape).collect();
    if words.is_empty() {
        return None;
    }
    let pattern = regex::RegexBuilder::new(&words.join("[ \\t\\n]+")).case_insensitive(true).build().ok()?;
    let found = pattern
        .find_iter(text)
        .find(|found| !avoid.is_some_and(|avoid| found.start() <= avoid && avoid < found.end()))
        .map(|found| found.start());
    found
}

/// The text of a node's token of `kind`.
fn token_of(node: &SyntaxNode, kind: SyntaxKind) -> Option<String> {
    node.children_with_tokens()
        .filter_map(|child| child.into_token())
        .find(|token| token.kind() == kind)
        .map(|token| token.text().to_string())
}

/// `org-link--normalize-string`: statistics cookies (`[1/2]`, `[50%]`)
/// out.
fn without_cookies(title: &str) -> String {
    let mut out = String::new();
    let mut rest = title;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let digits = after.chars().take_while(char::is_ascii_digit).count();
        let tail = &after[digits..];
        let cookie = if let Some(tail) = tail.strip_prefix('%') {
            tail.starts_with(']').then_some(digits + 2)
        } else if let Some(tail) = tail.strip_prefix('/') {
            let more = tail.chars().take_while(char::is_ascii_digit).count();
            tail[more..].starts_with(']').then_some(digits + more + 2)
        } else {
            None
        };
        match cookie {
            Some(length) => {
                out.push(' ');
                rest = &after[length..];
            }
            None => {
                out.push('[');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// `org-find-property`: where the entry starts whose `name` property is
/// `value`, ignoring case; the start of the text for a property before
/// the first headline.
fn find_property(root: &SyntaxNode, name: &str, value: &str) -> Option<usize> {
    let drawer = root.descendants().filter(|node| node.kind() == SyntaxKind::PROPERTY_DRAWER).find(|drawer| {
        drawer.children().filter(|node| node.kind() == SyntaxKind::NODE_PROPERTY).any(|property| {
            let key = token_of(&property, SyntaxKind::PROPERTY_KEY).unwrap_or_default();
            let found = token_of(&property, SyntaxKind::PROPERTY_VALUE).unwrap_or_default();
            key.trim_matches(':').eq_ignore_ascii_case(name)
                && found.trim().to_lowercase() == value.trim().to_lowercase()
        })
    })?;
    let headline = drawer.ancestors().find(|node| node.kind() == SyntaxKind::HEADLINE);
    Some(headline.map_or(0, |headline| usize::from(headline.text_range().start())))
}

/// `org-id-find-id-in-file`: where the entry with this `ID` starts.
pub fn find_id(parse: &Parse, id: &str) -> Option<usize> {
    find_property(&parse.syntax(), "ID", id)
}

/// A coderef `(ref:name)` at the end of a line of a source or example
/// block: where it starts.
fn coderef(root: &SyntaxNode, text: &str, name: &str) -> Option<usize> {
    let wanted = format!("(ref:{name})");
    root.descendants()
        .filter(|node| node.kind() == SyntaxKind::BLOCK)
        .filter(|block| matches!(block_type(block).as_deref(), Some("src" | "example")))
        .find_map(|block| {
            let range = block.text_range();
            let (start, end) = (usize::from(range.start()), usize::from(range.end()));
            let mut at = start;
            for line in text[start..end].split_inclusive('\n') {
                let trimmed = line.trim_end_matches(['\n', ' ', '\t']);
                if trimmed.ends_with(&wanted) {
                    return Some(at + trimmed.len() - wanted.len());
                }
                at += line.len();
            }
            None
        })
}

/// Where a `file:` link's search option leads in its file: a line number,
/// `org-link-search` in an Org file (`parse`), else the words in the text.
pub fn search_in_file(text: &str, parse: Option<&Parse>, option: &str) -> Result<usize, String> {
    search_in_file_with(text, parse, option, ExactHeadline::Yes)
}

/// `search_in_file`, with `org-link-search-must-match-exact-headline` as
/// `exact` says.
pub fn search_in_file_with(
    text: &str,
    parse: Option<&Parse>,
    option: &str,
    exact: ExactHeadline,
) -> Result<usize, String> {
    if !option.is_empty() && option.chars().all(|c| c.is_ascii_digit()) {
        let line: usize = option.parse().unwrap_or(1);
        return Ok(text.split_inclusive('\n').take(line.saturating_sub(1)).map(str::len).sum());
    }
    if let Some(parse) = parse {
        return search_with(text, parse, option, None, exact);
    }
    let words: Vec<String> = normalized(option).split_whitespace().map(str::to_lowercase).collect();
    let lower = text.to_lowercase();
    if words.is_empty() || lower.len() != text.len() {
        return Err(format!("No match for fuzzy expression: {option}"));
    }
    let mut from = 0;
    while let Some(found) = lower[from..].find(&words[0]) {
        let start = from + found;
        let mut at = start + words[0].len();
        let all = words[1..].iter().all(|word| {
            let blanks = lower[at..].len() - lower[at..].trim_start_matches([' ', '\t', '\n']).len();
            if blanks == 0 || !lower[at + blanks..].starts_with(word.as_str()) {
                return false;
            }
            at += blanks + word.len();
            true
        });
        if all {
            return Ok(start);
        }
        from = start + words[0].len();
    }
    Err(format!("No match for fuzzy expression: {option}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_abbreviations_from_the_file_then_the_setting() {
        let global =
            vec![("gh".to_string(), "https://global/%s".to_string()), ("w".to_string(), "https://w/".to_string())];
        let text = "#+LINK: gh https://local/%s\n[[gh:x]] [[w:Page]] [[fn:f]]\n";
        let parsed = org_syntax::parse(text, &Default::default());
        let opened: Vec<_> =
            links(text, &parsed, &global, &Settings::default()).into_iter().map(|link| link.open).collect();
        assert_eq!(opened[0], Ok(Open::External("https://local/x".to_string())));
        assert_eq!(opened[1], Ok(Open::External("https://w/Page".to_string())));
        let function = vec![("f".to_string(), "%(my-function)".to_string())];
        assert!(expand_abbreviation("f:x", &function).is_err());
        assert_eq!(expand_abbreviation("other:x", &function), Ok("other:x".to_string()));
    }

    #[test]
    fn opens_a_range_of_centuries_cut_short() {
        let Some(Open::Agenda { span, .. }) = timestamp("<2026-10-12 Mon>--<2126-10-12 Mon>") else { panic!() };
        assert_eq!(span, crate::agenda::DAYS_MAX);
    }
}
