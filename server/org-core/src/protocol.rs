//! `org-protocol.el`: what an `org-protocol://` URL asks for. A browser's
//! bookmarklet or another program opens such a URL, the system hands it to
//! the handler (`emacsclient` for Emacs), and Org reads it here: which of
//! its handlers it names (`org-protocol-protocol-alist-default`: `capture`,
//! `store-link`, `open-source`), and their parameters, in the new style
//! (`org-protocol://capture?url=…&title=…`) or the old one
//! (`org-protocol://capture:/x/URL/TITLE/BODY`).
//!
//! The URL goes through what `emacsclient` and Emacs's server do to it
//! first, as file name (`expand-file-name`: runs of slashes made one, `.`
//! and `..` resolved), which is why `org-protocol-sanitize-uri` puts the
//! double slash of `https://` back. Matching ignores case, as
//! `case-fold-search` is on.

use std::sync::LazyLock;

use regex::{Regex, RegexBuilder};

/// `org-protocol-the-protocol`
pub const THE_PROTOCOL: &str = "org-protocol";

/// `org-protocol-data-separator`'s default.
pub const DEFAULT_DATA_SEPARATOR: &str = r"/+\|\?";

/// A handler of `org-protocol-protocol-alist-default`, in its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handler {
    Capture,
    StoreLink,
    OpenSource,
}

impl Handler {
    const ALL: [Handler; 3] = [Handler::Capture, Handler::StoreLink, Handler::OpenSource];

    /// Its `:protocol`, as the URL names it.
    pub fn name(self) -> &'static str {
        match self {
            Handler::Capture => "capture",
            Handler::StoreLink => "store-link",
            Handler::OpenSource => "open-source",
        }
    }
}

/// The options `org-protocol.el` reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    /// `org-protocol-default-template-key`
    pub default_template_key: Option<String>,
    /// `org-protocol-data-separator`, an Emacs regexp; its default unset.
    pub data_separator: Option<String>,
    /// `org-protocol-project-alist`
    pub projects: Vec<Project>,
}

/// An entry of `org-protocol-project-alist`: the URLs under `base_url`
/// are the files under `working_directory`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Project {
    /// The entry's `module-name`, which only names it.
    pub name: String,
    /// `:base-url`, with its last slash.
    pub base_url: String,
    /// `:working-directory`, with its last slash.
    pub working_directory: String,
    /// `:online-suffix`, taken off the URL.
    pub online_suffix: Option<String>,
    /// `:working-suffix`, put on in its place.
    pub working_suffix: Option<String>,
    /// `:rewrites`: an Emacs regexp on the URL and the path, relative to
    /// the working directory, that it stands for when the file is not
    /// there.
    pub rewrites: Vec<(String, String)>,
}

/// A property list as Org reads it from a URL: keys with their colon, in
/// order, values that may be missing.
pub type Plist = Vec<(String, Option<String>)>;

/// `plist-get`: the first value under `key`.
pub fn plist_get<'a>(plist: &'a Plist, key: &str) -> Option<&'a str> {
    plist.iter().find(|(name, _)| name == key).and_then(|(_, value)| value.as_deref())
}

/// What a URL asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// `org-protocol-store-link`: a link to push on `org-stored-links`, and
    /// on the kill ring.
    StoreLink { url: String, title: Option<String> },
    /// `org-protocol-capture`
    Capture(Capture),
    /// `org-protocol-open-source`: the URL to map to a local file.
    OpenSource { url: String },
}

/// What `org-protocol-capture` starts a capture with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capture {
    /// The template's keys: the URL's, or `org-protocol-default-template-key`;
    /// none for the menu.
    pub template: Option<String>,
    /// The link pushed on `org-stored-links`, when the URL has one.
    pub stored: Option<(String, String)>,
    /// What `org-link-store-props` stores, in its order: `:type`, `:link`,
    /// `:description`, `:annotation` and `:initial`; `%:keyword` reads them,
    /// `%a` and `%i` the last two.
    pub props: Vec<(String, Option<String>)>,
    /// `:query`: the URL's parameters, which no escape reads as text.
    pub query: Plist,
}

impl Capture {
    /// `plist-get` of `org-store-link-plist`.
    pub fn prop(&self, key: &str) -> Option<&str> {
        self.props.iter().find(|(name, _)| name == key).and_then(|(_, value)| value.as_deref())
    }
}

/// What a handler says, as Emacs says it in the echo area.
pub const CAPTURED: &str = "Item captured.";

/// `org-protocol-store-link`'s message: the keys of `org-insert-link` and
/// `yank` in an Org buffer.
pub fn store_link_message(url: &str) -> String {
    format!("‘C-c C-l’ to insert new Org link, ‘C-y’ to insert {}", prin1(url))
}

/// The error Emacs stops with: in the URL of `handler`, before Org calls
/// it (reading its parameters) or in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    pub handler: Handler,
    pub called: bool,
    pub error: String,
}

/// Read an `org-protocol:` URL: `Ok(None)` when it names no handler, which
/// Emacs then takes for a file name.
pub fn parse(url: &str, options: &Options) -> Result<Option<Request>, Refused> {
    let fname = expand_file_name(url);
    let Some((handler, result, new_style)) = check_filename_for_protocol(&fname) else { return Ok(None) };
    let separator = options.data_separator.as_deref().unwrap_or(DEFAULT_DATA_SEPARATOR);
    // New-style links come as a property list; old-style ones as the
    // rest of the URL, which each handler splits itself.
    let info = match new_style {
        true => Info::Plist(parse_query(result).map_err(|error| Refused { handler, called: false, error })?),
        false => Info::Old(result.to_string()),
    };
    handle(handler, &info, separator, options).map(Some).map_err(|error| Refused { handler, called: true, error })
}

/// What `handler` makes of its parameters.
fn handle(handler: Handler, info: &Info, separator: &str, options: &Options) -> Result<Request, String> {
    Ok(match handler {
        Handler::StoreLink => {
            let parts = parse_parameters(info, separator, &[":url", ":title"])?;
            let url = sanitize_uri(plist_get(&parts, ":url"))?;
            Request::StoreLink { url, title: plist_get(&parts, ":title").map(str::to_string) }
        }
        Handler::Capture => Request::Capture(capture(info, separator, options)?),
        Handler::OpenSource => {
            let parts = parse_parameters(info, separator, &[":url"])?;
            Request::OpenSource { url: sanitize_uri(plist_get(&parts, ":url"))? }
        }
    })
}

/// The parameters of a handler: a property list, or the old-style rest.
enum Info {
    Plist(Plist),
    Old(String),
}

/// `org-protocol-capture`, up to `org-capture`.
fn capture(info: &Info, separator: &str, options: &Options) -> Result<Capture, String> {
    let parts = match info {
        Info::Plist(plist) => plist.clone(),
        // Old style: a template's key first when the first part is one
        // character long.
        Info::Old(text) => {
            let parts = split_data(text, separator)?;
            let order: &[&str] = if parts.first().is_some_and(|first| first.chars().count() == 1) {
                &[":template", ":url", ":title", ":body"]
            } else {
                &[":url", ":title", ":body"]
            };
            assign_parameters(parts, order)
        }
    };
    let template = plist_get(&parts, ":template").map(str::to_string).or_else(|| options.default_template_key.clone());
    let url = plist_get(&parts, ":url").map(|url| sanitize_uri(Some(url))).transpose()?;
    static TYPE: LazyLock<Regex> = LazyLock::new(|| {
        RegexBuilder::new("^([a-z]+):").case_insensitive(true).multi_line(true).build().expect("a valid regex")
    });
    let kind = url.as_deref().and_then(|url| TYPE.captures(url)).map(|captures| captures[1].to_string());
    let title = plist_get(&parts, ":title").unwrap_or("").to_string();
    let region = plist_get(&parts, ":body").unwrap_or("").to_string();
    let orglink = match &url {
        None => title.clone(),
        Some(url) => {
            let description = if string_nw_p(&title) { &title } else { url };
            crate::link::make_string(url, Some(description))?
        }
    };
    Ok(Capture {
        template,
        stored: url.clone().map(|url| (url, title.clone())),
        props: vec![
            (":type".to_string(), kind),
            (":link".to_string(), url),
            (":description".to_string(), Some(title)),
            (":annotation".to_string(), Some(orglink)),
            (":initial".to_string(), Some(region)),
        ],
        query: parts,
    })
}

/// `org-string-nw-p`
fn string_nw_p(text: &str) -> bool {
    text.chars().any(|c| !matches!(c, ' ' | '\r' | '\t' | '\n'))
}

/// `org-protocol-check-filename-for-protocol`: the handler `fname` names,
/// the text after it, and whether that is new style (`?`, not `:/`).
fn check_filename_for_protocol(fname: &str) -> Option<(Handler, &str, bool)> {
    static THE: LazyLock<Regex> = LazyLock::new(|| {
        RegexBuilder::new(&format!("{}:/+", regex::escape(THE_PROTOCOL)))
            .case_insensitive(true)
            .build()
            .expect("a valid regex")
    });
    static PROTOCOLS: LazyLock<Vec<(Handler, Regex)>> = LazyLock::new(|| {
        Handler::ALL
            .iter()
            .map(|handler| {
                let pattern = format!("{}:/+{}(:/+|/*\\?)", regex::escape(THE_PROTOCOL), regex::escape(handler.name()));
                (*handler, RegexBuilder::new(&pattern).case_insensitive(true).build().expect("a valid regex"))
            })
            .collect()
    });
    if !THE.is_match(fname) {
        return None;
    }
    for (handler, proto) in PROTOCOLS.iter() {
        let matches: Vec<regex::Captures> = proto.captures_iter(fname).collect();
        let (Some(first), Some(last)) = (matches.first(), matches.last()) else { continue };
        // `(cadr (split-string fname proto))`: up to the next match.
        let end = matches.get(1).map_or(fname.len(), |next| next.get(0).expect("whole match").start());
        let result = &fname[first.get(0).expect("whole match").end()..end];
        // `split-string` leaves the match data at its last match, which
        // `(match-string 1 fname)` reads.
        let new_style = !last[1].starts_with(':');
        return Some((*handler, result, new_style));
    }
    None
}

/// `org-protocol-convert-query-to-plist`, then the values decoded as
/// `org-protocol-parse-parameters` does: a key without `=` has no value,
/// which `org-link-decode` cannot take.
fn parse_query(query: &str) -> Result<Plist, String> {
    let plus_decoded = query.replace('+', " ");
    let mut plist = Vec::new();
    for pair in plus_decoded.split('&') {
        let mut parts = pair.split('=');
        let key = format!(":{}", parts.next().unwrap_or(""));
        let value = parts.next().ok_or_else(|| "Wrong type argument: arrayp, nil".to_string())?;
        plist.push((key, Some(link_decode(value)?)));
    }
    Ok(plist)
}

/// `org-protocol-parse-parameters` with `default-order`: a property list
/// as it is, or the old-style parts named in that order.
fn parse_parameters(info: &Info, separator: &str, order: &[&str]) -> Result<Plist, String> {
    match info {
        Info::Plist(plist) => Ok(plist.clone()),
        Info::Old(text) => Ok(assign_parameters(split_data(text, separator)?, order)),
    }
}

/// `org-protocol-assign-parameters`: the parts named by `order`, then
/// those left as pairs of a key and its value.
fn assign_parameters(data: Vec<String>, order: &[&str]) -> Plist {
    let mut data = data.into_iter();
    let mut result: Plist = order.iter().map(|key| (key.to_string(), data.next())).collect();
    while let Some(key) = data.next() {
        result.push((format!(":{key}"), data.next()));
    }
    result
}

/// `org-protocol-split-data` with each part decoded.
fn split_data(data: &str, separator: &str) -> Result<Vec<String>, String> {
    let separator = emacs_regex(separator).ok_or_else(|| format!("Invalid regexp: {separator}"))?;
    split_string(data, &separator).into_iter().map(link_decode).collect()
}

/// An Emacs regexp matched against a string as `string-match` does it:
/// ignoring case, `^` and `$` at the lines' ends too.
fn emacs_regex(pattern: &str) -> Option<Regex> {
    RegexBuilder::new(&crate::agenda::matcher::translate_capturing(pattern))
        .case_insensitive(true)
        .multi_line(true)
        .build()
        .ok()
}

/// `split-string` with a separator, keeping empty parts.
fn split_string<'a>(text: &'a str, separator: &Regex) -> Vec<&'a str> {
    let mut parts = Vec::new();
    let (mut start, mut last_begin) = (0, None);
    loop {
        // An empty match where the last one began moves on a character.
        let from = match last_begin {
            Some(begin) if begin == start && start < text.len() => {
                start + text[start..].chars().next().map_or(1, char::len_utf8)
            }
            _ => start,
        };
        let Some(found) = (from <= text.len()).then(|| separator.find_at(text, from)).flatten() else { break };
        if start >= text.len() {
            break;
        }
        parts.push(&text[start..found.start()]);
        start = found.end();
        last_begin = Some(found.start());
    }
    parts.push(&text[start..]);
    parts
}

/// `org-protocol-sanitize-uri`: the slashes after a scheme doubled again,
/// and every other run of them made one. Nothing to sanitize is Emacs's
/// error.
pub fn sanitize_uri(uri: Option<&str>) -> Result<String, String> {
    let uri = uri.ok_or_else(|| "Wrong type argument: stringp, nil".to_string())?;
    static SCHEME: LazyLock<Regex> = LazyLock::new(|| {
        RegexBuilder::new("^[a-z]+:/").case_insensitive(true).multi_line(true).build().expect("a valid regex")
    });
    static SLASHES: LazyLock<Regex> = LazyLock::new(|| Regex::new("/+").expect("a valid regex"));
    if !SCHEME.is_match(uri) {
        return Ok(uri.to_string());
    }
    let parts = split_string(uri, &SLASHES);
    Ok(format!("{}//{}", parts[0], parts[1..].join("/")))
}

/// `org-link-decode`: each run of `%XX` decoded as UTF-8, or byte by
/// byte when it does not end a character.
pub fn link_decode(text: &str) -> Result<String, String> {
    static HEX: LazyLock<Regex> = LazyLock::new(|| Regex::new("(?:%[0-9A-Za-z]{2})+").expect("a valid regex"));
    let mut out = String::new();
    let mut last = 0;
    for found in HEX.find_iter(text) {
        out.push_str(&text[last..found.start()]);
        out.push_str(&decode_compound(found.as_str())?);
        last = found.end();
    }
    out.push_str(&text[last..]);
    Ok(out)
}

/// `string-to-number` in base 16 of two characters: the hex digits they
/// start with.
fn hex_value(digits: &str) -> u32 {
    let mut value = 0;
    for c in digits.chars() {
        match c.to_digit(16) {
            Some(digit) => value = value * 16 + digit,
            None => break,
        }
    }
    value
}

/// Emacs's character `code` as text: what Unicode has no place for
/// (Emacs's characters past it, its raw bytes, surrogates) as U+FFFD.
fn char_to_string(code: u64) -> Result<char, String> {
    if code > 0x3F_FFFF {
        return Err(format!("Wrong type argument: characterp, {code}"));
    }
    Ok(u32::try_from(code).ok().and_then(char::from_u32).unwrap_or('\u{FFFD}'))
}

/// `org-link--decode-compound`
fn decode_compound(hex: &str) -> Result<String, String> {
    let bytes: Vec<u32> = hex.split('%').skip(1).map(hex_value).collect();
    let mut ret = String::new();
    let (mut eat, mut sum) = (0u32, 0u64);
    for (index, &byte) in bytes.iter().enumerate() {
        let mut val = byte;
        let (shift, xor) = if eat == 0 {
            match val {
                252.. => (6, 252),
                248.. => (5, 248),
                240.. => (4, 240),
                224.. => (3, 224),
                192.. => (2, 192),
                _ => (0, 0),
            }
        } else {
            (6, 128)
        };
        if val >= 192 {
            eat = shift;
        }
        val ^= xor;
        sum = (sum << shift) + u64::from(val);
        eat = eat.saturating_sub(1);
        if eat == 0 {
            ret.push(char_to_string(sum)?);
            sum = 0;
        } else if index + 1 == bytes.len() {
            // `org-link--decode-single-byte-sequence`: all of them.
            ret = bytes.iter().map(|&byte| char_to_string(u64::from(byte))).collect::<Result<String, String>>()?;
        }
    }
    Ok(ret)
}

/// `expand-file-name` of the URL, which is no absolute file name, in the
/// root directory, as Emacs's server reads it: runs of slashes made one,
/// `.` dropped and `..` taking the name before it away (or standing at
/// the root), a last slash kept.
pub fn expand_file_name(name: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in name.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    parts.push("..");
                }
            }
            part => parts.push(part),
        }
    }
    let mut expanded = format!("/{}", parts.join("/"));
    if name.ends_with('/') && !parts.is_empty() {
        expanded.push('/');
    }
    expanded
}

/// Whether a file is there, as `file-exists-p` and `file-readable-p`
/// tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileState {
    Missing,
    Unreadable,
    Readable,
}

/// What `org-protocol-open-source` makes of a URL: the file to visit, if
/// any, and what it says on the way.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Opened {
    pub file: Option<String>,
    /// The working directory of the project that maps the URL to `file`.
    pub working_directory: Option<String>,
    pub messages: Vec<String>,
    /// The error it stopped with, after those messages.
    pub error: Option<String>,
}

/// `org-protocol-open-source`: the URL `f` mapped to a local file through
/// `projects` (`org-protocol-project-alist`); `state` says whether a file
/// is there, its name as Org makes it (`~/` for the home directory).
pub fn open_source(f: &str, projects: &[Project], state: &dyn Fn(&str) -> FileState) -> Opened {
    let mut opened = Opened::default();
    if let Err(error) = map_source(f, projects, state, &mut opened) {
        opened.error = Some(error);
    }
    opened
}

fn map_source(
    f: &str,
    projects: &[Project],
    state: &dyn Fn(&str) -> FileState,
    opened: &mut Opened,
) -> Result<(), String> {
    static QUERY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)(?:[\\?#].*)?$").expect("a valid regex"));
    for project in projects {
        let base_url = &project.base_url;
        let wsearch =
            RegexBuilder::new(&regex::escape(base_url)).case_insensitive(true).build().map_err(|e| e.to_string())?;
        if !wsearch.is_match(f) {
            continue;
        }
        let wdir = &project.working_directory;
        // Strip "[?#].*$" if `f` is a redirect with another ending than
        // strip-suffix here.
        let f1 = &f[..QUERY.find(f).map_or(f.len(), |found| found.start())];
        let start = wsearch.find(f1).ok_or_else(|| "Wrong type argument: number-or-marker-p, nil".to_string())?.start()
            + base_url.len();
        let end = match &project.online_suffix {
            Some(suffix) => match RegexBuilder::new(&regex::escape(suffix)).case_insensitive(true).build() {
                Ok(suffix) => suffix.find(f1).map_or(f1.len(), |found| found.start()),
                Err(error) => return Err(error.to_string()),
            },
            None => f1.len(),
        };
        let path = substring(f1, start, end)?;
        let f2 = format!("{wdir}{path}");
        let the_file = match &project.working_suffix {
            Some(suffix) => format!("{f2}{suffix}"),
            None => f2,
        };
        // Redirects.
        if state(&the_file) == FileState::Missing {
            opened.messages.push(format!("File {the_file} does not exist.\nTesting for rewritten URLs."));
            if !project.rewrites.is_empty() {
                opened.messages.push(format!("Rewrites found: {}", prin1_rewrites(&project.rewrites)));
                for (pattern, replacement) in &project.rewrites {
                    let regex = emacs_regex(pattern).ok_or_else(|| format!("Invalid regexp: {pattern}"))?;
                    let Some(captures) = regex.captures(f1) else { continue };
                    // `(replace-match "" nil nil f1 1)`
                    let Some(group) = captures.get(1) else {
                        return Err("replace-match subexpression does not exist: 1".to_string());
                    };
                    let replaced = format!("{}{}", &f1[..group.start()], &f1[group.end()..]);
                    opened.file = Some(format!("{wdir}{}{replacement}", directory_file_name(&replaced)));
                    opened.working_directory = Some(wdir.clone());
                    return Ok(());
                }
            }
        }
        match state(&the_file) {
            FileState::Readable => {
                opened.file = Some(the_file);
                opened.working_directory = Some(wdir.clone());
                return Ok(());
            }
            FileState::Unreadable => opened.messages.push(format!("{the_file}: permission denied!")),
            FileState::Missing => opened.messages.push(format!("{the_file}: no such file or directory.")),
        }
    }
    Ok(())
}

/// `substring` from byte `start` to `end`, Emacs's error when they do not
/// make a range of the text.
fn substring(text: &str, start: usize, end: usize) -> Result<&str, String> {
    text.get(start..end).filter(|_| start <= end).ok_or_else(|| {
        let chars = |at: usize| text.get(..at).map_or(at, |before| before.chars().count());
        format!("Args out of range: {}, {}, {}", prin1(text), chars(start), chars(end))
    })
}

/// `directory-file-name`: the slashes at the end gone, unless that is all
/// there is.
fn directory_file_name(name: &str) -> &str {
    let trimmed = name.trim_end_matches('/');
    if trimmed.is_empty() {
        // The root, `/`; `//` stands for itself.
        if name.len() == 2 {
            name
        } else {
            &name[..name.len().min(1)]
        }
    } else {
        trimmed
    }
}

/// `prin1` of a string: in double quotes, with `"` and `\` escaped.
pub fn prin1(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// `prin1` of `:rewrites`, a list of conses of strings.
fn prin1_rewrites(rewrites: &[(String, String)]) -> String {
    let conses: Vec<String> =
        rewrites.iter().map(|(pattern, path)| format!("({} . {})", prin1(pattern), prin1(path))).collect();
    format!("({})", conses.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_new_style_capture() {
        let options = Options::default();
        let url = "org-protocol://capture?template=x&url=https%3A%2F%2Forgmode.org%2F&title=Org+mode&body=Hi";
        let Ok(Some(Request::Capture(capture))) = parse(url, &options) else { panic!() };
        assert_eq!(capture.template.as_deref(), Some("x"));
        assert_eq!(capture.prop(":link"), Some("https://orgmode.org/"));
        assert_eq!(capture.prop(":type"), Some("https"));
        assert_eq!(capture.prop(":annotation"), Some("[[https://orgmode.org/][Org mode]]"));
        assert_eq!(capture.prop(":initial"), Some("Hi"));
    }

    #[test]
    fn expands_as_a_file_name() {
        assert_eq!(expand_file_name("org-protocol://a//b/./c/../d/"), "/org-protocol:/a/b/d/");
        assert_eq!(expand_file_name("org-protocol:/a/../../../b"), "/../b");
        assert_eq!(expand_file_name("org-protocol:/a/../../../.."), "/");
    }
}
