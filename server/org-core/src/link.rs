//! Storing and inserting links (`ol.el`): `org-store-link` (`C-c l`),
//! `org-insert-link` (`C-c C-l`), `org-insert-last-stored-link` (`C-c
//! M-l`), `org-insert-all-links` (`C-c C-M-l`) and `org-id-get-create`,
//! with the strings they make (`org-link-make-string`,
//! `org-link-make-string-for-buffer`).
//!
//! The stored links are the caller's (`org-stored-links`), the most
//! recent first. The commands that ask something take the answers given
//! so far and say what they ask next, so that they can be run again with
//! one more answer.

use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

use regex::{Regex, RegexBuilder};

use crate::outline::{complex_heading_title, Outline};
use crate::properties;

/// `org-id-link-to-org-use-id`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UseId {
    /// `nil`: links to entries search for their headline.
    #[default]
    Never,
    /// `t`: an ID, made when the entry has none.
    Always,
    /// `create-if-interactive`
    CreateIfInteractive,
    /// `create-if-interactive-and-no-custom-id`
    CreateIfInteractiveAndNoCustomId,
    /// `use-existing`
    UseExisting,
}

/// `org-link-file-path-type`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PathType {
    /// Relative below the file's directory, else absolute with `~`.
    #[default]
    Adaptive,
    /// Absolute, with `~` for the home directory.
    Absolute,
    /// Absolute.
    NoAbbrev,
    /// Relative to the file's directory.
    Relative,
}

/// The options of the link commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub use_id: UseId,
    /// `org-link-context-for-files`
    pub context_for_files: bool,
    /// `org-id-link-use-context`
    pub id_use_context: bool,
    /// `org-id-link-consider-parent-id`
    pub consider_parent_id: bool,
    /// `org-link-file-path-type`
    pub path_type: PathType,
    /// `org-link-keep-stored-after-insertion`
    pub keep_stored: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            use_id: UseId::Never,
            context_for_files: true,
            id_use_context: true,
            consider_parent_id: false,
            path_type: PathType::Adaptive,
            keep_stored: false,
        }
    }
}

/// The file a command runs in.
#[derive(Debug, Clone, Copy)]
pub struct File<'a> {
    /// Its absolute path.
    pub path: &'a str,
    /// The home directory, which `~` stands for.
    pub home: Option<&'a str>,
}

impl File<'_> {
    /// `default-directory`: the file's directory.
    fn directory(&self) -> String {
        Path::new(self.path).parent().map_or_else(|| "/".to_string(), |dir| dir.display().to_string())
    }

    /// `expand-file-name` against the file's directory.
    fn expand(&self, name: &str) -> String {
        expand(name, &self.directory(), self.home)
    }

    /// `abbreviate-file-name`
    fn abbreviate(&self, path: &str) -> String {
        match self.home.map(|home| home.trim_end_matches('/')).filter(|home| !home.is_empty()) {
            Some(home) if path == home => "~".to_string(),
            Some(home) => match path.strip_prefix(home).filter(|rest| rest.starts_with('/')) {
                Some(rest) => format!("~{rest}"),
                None => path.to_string(),
            },
            None => path.to_string(),
        }
    }

    /// `file-truename`: the path expanded, its links followed when it
    /// exists.
    fn truename(&self, name: &str) -> String {
        let expanded = self.expand(name);
        std::fs::canonicalize(&expanded).map_or(expanded, |path| path.display().to_string())
    }
}

/// `expand-file-name`: `~` and relative names resolved, `.` and `..`
/// taken out.
fn expand(name: &str, directory: &str, home: Option<&str>) -> String {
    let joined = if name == "~" || name.starts_with("~/") {
        format!("{}{}", home.unwrap_or("~"), &name[1..])
    } else if name.starts_with('/') {
        name.to_string()
    } else {
        format!("{}/{name}", directory.trim_end_matches('/'))
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in joined.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    let mut out = format!("/{}", parts.join("/"));
    if joined.ends_with('/') && out != "/" {
        out.push('/');
    }
    out
}

/// `file-relative-name`
fn relative_name(path: &str, directory: &str) -> String {
    let path = PathBuf::from(path);
    let directory = PathBuf::from(directory);
    let from: Vec<Component> = directory.components().collect();
    let to: Vec<Component> = path.components().collect();
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut out = PathBuf::new();
    for _ in common..from.len() {
        out.push("..");
    }
    for part in &to[common..] {
        out.push(part.as_os_str());
    }
    let out = out.display().to_string();
    if out.is_empty() {
        ".".to_string()
    } else {
        out
    }
}

/// `org-link--normalize-filename`
fn normalize_filename(name: &str, method: PathType, file: &File) -> String {
    match method {
        PathType::Absolute => file.abbreviate(&file.expand(name)),
        PathType::NoAbbrev => file.expand(name),
        PathType::Relative => relative_name(&file.expand(name), &file.directory()),
        PathType::Adaptive => {
            let directory = format!("{}/", file.directory().trim_end_matches('/'));
            let expanded = file.expand(name);
            match expanded.strip_prefix(&directory) {
                Some(rest) => rest.to_string(),
                None => file.abbreviate(&expanded),
            }
        }
    }
}

// ----- link strings

/// `org-link-escape`: brackets, and the backslashes before them or at the
/// end, escaped with backslashes.
pub fn escape(link: &str) -> String {
    let mut out = String::with_capacity(link.len());
    let mut backslashes = 0;
    for c in link.chars() {
        match c {
            '\\' => backslashes += 1,
            '[' | ']' => {
                out.push_str(&"\\".repeat(backslashes * 2 + 1));
                backslashes = 0;
                out.push(c);
            }
            _ => {
                out.push_str(&"\\".repeat(backslashes));
                backslashes = 0;
                out.push(c);
            }
        }
    }
    out.push_str(&"\\".repeat(backslashes * 2));
    out
}

/// `org-link-unescape`
pub fn unescape(link: &str) -> String {
    let mut out = String::with_capacity(link.len());
    let mut backslashes = 0;
    for c in link.chars() {
        match c {
            '\\' => backslashes += 1,
            '[' | ']' => {
                out.push_str(&"\\".repeat(backslashes / 2));
                backslashes = 0;
                out.push(c);
            }
            _ => {
                out.push_str(&"\\".repeat(backslashes));
                backslashes = 0;
                out.push(c);
            }
        }
    }
    out.push_str(&"\\".repeat(backslashes / 2));
    out
}

/// `org-string-nw-p`: some text that is not blank (` `, `\r`, `\t`,
/// `\n`).
fn not_blank(text: &str) -> bool {
    text.chars().any(|c| !matches!(c, ' ' | '\r' | '\t' | '\n'))
}

/// `org-link-make-string`: `[[LINK][DESCRIPTION]]`, the link escaped,
/// the description kept from closing the link early.
pub fn make_string(link: &str, description: Option<&str>) -> Result<String, String> {
    let description = description.filter(|description| not_blank(description)).map(|description| {
        let mut description = crate::text::trim(description).to_string();
        if description.ends_with(']') {
            description.push('\u{200B}');
        }
        description.replace("]]", "]\u{200B}]")
    });
    if !not_blank(link) {
        return description.ok_or_else(|| "Empty link".to_string());
    }
    Ok(match description {
        Some(description) => format!("[[{}][{description}]]", escape(link)),
        None => format!("[[{}]]", escape(link)),
    })
}

/// `org-link-bracket-re`
static BRACKET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[\[((?:[^\[\]\\]|\\(?:\\\\)*[\[\]]|\\+[^\[\]])+)\](?:\[((?s:.)+?)\])?\]").expect("a valid regex")
});

/// `org-link-angle-re`
static ANGLE: LazyLock<Regex> = LazyLock::new(|| {
    let types: Vec<String> = org_syntax::link_types().iter().map(|ty| regex::escape(ty)).collect();
    RegexBuilder::new(&format!(r"<({}):([^>\n]*(?:\n[ \t]*[^> \t\n][^>\n]*)*)>", types.join("|")))
        .case_insensitive(true)
        .build()
        .expect("a valid regex")
});

/// `org-link-display-format`: links as their descriptions, or their
/// targets.
pub fn display_format(text: &str) -> String {
    BRACKET
        .replace_all(text, |captures: &regex::Captures| {
            captures.get(2).or_else(|| captures.get(1)).map_or(String::new(), |found| found.as_str().to_string())
        })
        .into_owned()
}

/// `org-link--normalize-string`: statistics cookies and runs of blanks
/// gone; with `context`, the parentheses around it and the `#` and `*`
/// before it too.
pub fn normalize_string(text: &str, context: bool) -> String {
    static COOKIE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[[0-9]*(?:%|/[0-9]*)\]").expect("a valid regex"));
    static BLANKS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ \t]+").expect("a valid regex"));
    let text = COOKIE.replace_all(text, " ");
    let mut string = crate::text::trim(&BLANKS.replace_all(&text, " ")).to_string();
    if context {
        loop {
            if string.len() > 1 && string.starts_with('(') && string.ends_with(')') {
                string = crate::text::trim(&string[1..string.len() - 1]).to_string();
            } else if string.starts_with(['#', '*']) {
                string = string.trim_start_matches(['#', '*']).trim_start_matches([' ', '\t']).to_string();
            } else {
                break;
            }
        }
    }
    string
}

// ----- where the cursor is

fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |found| found + 1)
}

fn line_end(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |found| at + found)
}

/// The text `org-in-regexp` searches with NLINES 1: from the start of the
/// line before the cursor's to the end of the line after it.
fn around(text: &str, at: usize) -> (usize, usize) {
    let start = line_start(text, at);
    let start = if start > 0 { line_start(text, start - 1) } else { 0 };
    let end = line_end(text, at);
    let end = if end < text.len() { line_end(text, end + 1) } else { end };
    (start, end)
}

/// `org-in-regexp` with NLINES 1: the match of `pattern` around `at` that
/// holds it, its end included.
fn in_regexp<'t>(pattern: &Regex, text: &'t str, at: usize) -> Option<regex::Captures<'t>> {
    let (start, end) = around(text, at);
    for captures in pattern.captures_iter(&text[..end]).skip_while(|captures| captures.get(0).unwrap().start() < start)
    {
        let found = captures.get(0)?;
        if found.start() > at {
            return None;
        }
        if found.end() >= at {
            return Some(captures);
        }
    }
    None
}

/// `(org-in-regexp org-link-plain-re 1)`
fn in_plain_link(text: &str, at: usize) -> Option<(usize, usize)> {
    let (start, end) = around(text, at);
    let mut pos = start;
    while pos < end && pos <= at {
        if text.is_char_boundary(pos) {
            if let Some(found) = org_syntax::plain_link_end(&text[..end], pos) {
                if found >= at {
                    return Some((pos, found));
                }
                pos = found;
                continue;
            }
        }
        pos += 1;
    }
    None
}

/// Whether a plain link is somewhere in `text` (`string-match` of
/// `org-link-plain-re`).
fn has_plain_link(text: &str) -> bool {
    text.char_indices().any(|(at, _)| org_syntax::plain_link_end(text, at).is_some())
}

/// `org-before-first-heading-p`
fn before_first_heading(text: &str, at: usize) -> bool {
    crate::properties::heading_at(text, at).is_none()
}

/// The headline line of the entry at `at`, if any.
fn heading_of(text: &str, at: usize) -> Option<usize> {
    crate::properties::heading_at(text, at)
}

/// The level of the headline line at `heading`.
fn level(text: &str, heading: usize) -> usize {
    text[heading..].bytes().take_while(|b| *b == b'*').count()
}

/// `org-get-heading t t t t`: the title without keyword, priority,
/// `COMMENT` and tags.
fn clean_heading(text: &str, heading: usize, keywords: &org_syntax::TodoKeywords) -> String {
    let line = &text[heading..line_end(text, heading)];
    let title = complex_heading_title(line, level(text, heading), keywords).map_or("", |(from, to)| &line[from..to]);
    match title.strip_prefix("COMMENT") {
        Some(rest) if rest.is_empty() || rest.starts_with([' ', '\t']) => rest.trim_start().to_string(),
        _ => title.to_string(),
    }
}

/// `org-link-heading-search-string`
pub fn heading_search_string(title: &str) -> String {
    format!("*{}", normalize_string(title, false))
}

/// `org-link-precise-link-target`: the search string to the place at
/// `at`, its description and where it matches.
fn precise_target(
    text: &str,
    at: usize,
    region: Option<(usize, usize)>,
    context: &properties::Context,
) -> Option<(String, Option<String>, usize)> {
    let (begin, name) = crate::indent::element_at(text, at).unwrap_or((at, None));
    // The active region, as it is (`org-link--context-from-region`), with
    // no description.
    let result = if let Some((start, end)) = region {
        (normalize_string(&text[start..end], true), None, start)
    } else if let Some(name) = name {
        (name.clone(), Some(name), begin)
    } else if before_first_heading(text, at) {
        let start = line_start(text, at);
        (normalize_string(&text[start..line_end(text, at)], true), None, start)
    } else {
        let heading = heading_of(text, at)?;
        let search = match properties::entry_get(text, heading, "CUSTOM_ID", context) {
            Some(id) => format!("#{id}"),
            None => heading_search_string(&clean_heading(text, heading, &context.keywords)),
        };
        (search, Some(normalize_string(&clean_heading(text, heading, &context.keywords), false)), heading)
    };
    not_blank(&result.0).then_some(result)
}

/// `org-link--file-link-to-here`
fn file_link_to_here(
    text: &str,
    at: usize,
    region: Option<(usize, usize)>,
    file: &File,
    context_for_files: bool,
    context: &properties::Context,
) -> (String, Option<String>) {
    let mut link = format!("file:{}", file.abbreviate(file.path));
    let mut description = None;
    if context_for_files {
        if let Some((search, desc, _)) = precise_target(text, at, region, context) {
            link = format!("{link}::{search}");
            description = desc;
        }
    }
    (link, description)
}

/// `<<target>>` around the cursor, as `org-store-link` looks for one.
fn target_at(text: &str, at: usize) -> Option<String> {
    static TARGET: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^<]<<([^<>]+)>>[^>]").expect("a valid regex"));
    in_regexp(&TARGET, text, at).and_then(|captures| captures.get(1)).map(|found| found.as_str().to_string())
}

// ----- storing

/// A link `org-store-link` made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Store {
    pub link: String,
    pub description: Option<String>,
    /// The other link stored to an entry with a `CUSTOM_ID`.
    pub custom_id: Option<(String, Option<String>)>,
    /// The file with the ID made for the link, and where the cursor is
    /// then.
    pub edited: Option<(String, usize)>,
    /// The link's properties (`org-store-link-plist`) when a function of
    /// a link type stored it: an `id:` link's `:link`, `:description` and
    /// `:type`.
    pub props: Vec<(String, String)>,
}

/// A file after a change, and where the cursor is in it.
pub type Edited = (String, usize);

/// The stored links (`org-stored-links`), the most recent first.
pub type Stored = Vec<(String, Option<String>)>;

/// `org-entry-get` of `ID` at `at`, inherited when `inherit`: the value
/// and the headline it comes from.
fn entry_id(text: &str, at: usize, inherit: bool, context: &properties::Context) -> Option<(String, Option<usize>)> {
    let parse = org_syntax::parse(text, &context.keywords);
    let outline = Outline::new(&parse, text, &context.category);
    let valid = |value: Option<String>| value.filter(|value| not_blank(value));
    let mut entry = heading_of(text, at).and_then(|heading| outline.entry_at(heading));
    if entry.is_none() {
        return valid(outline.entry_property(None, "ID", false)).map(|value| (value, None));
    }
    while let Some(current) = entry {
        if let Some(value) = valid(outline.entry_property(Some(current), "ID", false)) {
            return Some((value, Some(outline.entries[current].offset)));
        }
        if !inherit {
            return None;
        }
        entry = outline.entries[current].parent;
    }
    valid(outline.entry_property(None, "ID", false)).map(|value| (value, None))
}

/// Where the cursor at `at` of `old` is in `new`, which has text inserted:
/// it moves along when the text goes in before it. Of the places the
/// text may have gone in, the first: Org puts a drawer before the line
/// break that ends a headline, not after it.
fn after_insertion(old: &str, new: &str, at: usize) -> usize {
    let added = new.len().saturating_sub(old.len());
    let (old_bytes, new_bytes) = (old.as_bytes(), new.as_bytes());
    let mut place = old_bytes.iter().zip(new_bytes).take_while(|(a, b)| a == b).count();
    while place > 0 && old_bytes[place - 1] == new_bytes[place - 1 + added] {
        place -= 1;
    }
    if at > place {
        at + added
    } else {
        at
    }
}

/// `org-id-store-link`: a link to the entry at `at` by its ID, the ID
/// made when there is none.
#[allow(clippy::too_many_arguments)]
fn id_link(
    text: &str,
    at: usize,
    region: Option<(usize, usize)>,
    file: &File,
    settings: &Settings,
    negate: bool,
    context: &properties::Context,
    new_id: &mut dyn FnMut() -> String,
) -> Result<(String, Option<String>, Option<Edited>), String> {
    let context_for_files = settings.context_for_files != negate;
    let precise =
        (context_for_files && settings.id_use_context).then(|| precise_target(text, at, region, context)).flatten();
    let inherit = settings.consider_parent_id && settings.id_use_context && context_for_files;
    let (id, inherited_from, edited) = match entry_id(text, at, inherit, context) {
        Some((id, from)) => (id, from, None),
        None => {
            let id = new_id();
            let edited = properties::put(text, at, "ID", &id, context)?;
            let cursor = after_insertion(text, &edited, at);
            (id, None, Some((edited, cursor)))
        }
    };
    let (current, current_at) = match &edited {
        Some((edited, cursor)) => (edited.as_str(), *cursor),
        None => (text, at),
    };
    let id_location = inherited_from.unwrap_or_else(|| heading_of(current, current_at).unwrap_or(0));
    let mut link = format!("id:{id}");
    let mut description = if before_first_heading(current, id_location) {
        let parse = org_syntax::parse(current, &context.keywords);
        let titles = crate::outline::keyword_values(&parse.syntax(), "TITLE");
        Some(titles.into_iter().next().unwrap_or_else(|| {
            Path::new(file.path).file_name().map_or(String::new(), |name| name.to_string_lossy().into_owned())
        }))
    } else {
        let line = &current[id_location..line_end(current, id_location)];
        Some(match complex_heading_title(line, level(current, id_location), &context.keywords) {
            Some((from, to)) => line[from..to].to_string(),
            None => line.to_string(),
        })
    };
    // The precise target, found before the ID went in, is wanted only
    // below the place of the ID.
    if let Some((search, desc, _)) = precise.filter(|(_, _, position)| *position > id_location) {
        link = format!("{link}::{search}");
        description = desc;
    }
    Ok((link, description, edited))
}

/// `org-store-link` in an Org file at `at`: the link, with its
/// description. `region` is the active region, whose text is the search
/// of a link to the file; `interactive` is the command (`C-c l`), else the
/// link capture's `%a` makes; `negate` is `C-u`, which turns
/// `org-link-context-for-files` around.
#[allow(clippy::too_many_arguments)]
pub fn store(
    text: &str,
    at: usize,
    region: Option<(usize, usize)>,
    file: &File,
    settings: &Settings,
    interactive: bool,
    negate: bool,
    context: &properties::Context,
    new_id: &mut dyn FnMut() -> String,
) -> Result<Store, String> {
    let context_for_files = settings.context_for_files != negate;
    let custom_id_here = || properties::entry_get(text, at, "CUSTOM_ID", context).filter(|id| !id.is_empty()).is_some();
    // `org-id-store-link-maybe`
    let by_id = match settings.use_id {
        UseId::Always => true,
        UseId::CreateIfInteractive if interactive => true,
        UseId::CreateIfInteractiveAndNoCustomId if interactive && !custom_id_here() => true,
        UseId::Never => false,
        _ => entry_id(text, at, settings.consider_parent_id && settings.id_use_context && context_for_files, context)
            .is_some(),
    };
    let mut props = Vec::new();
    let (link, description, edited) = if by_id {
        let (link, description, edited) = id_link(text, at, region, file, settings, negate, context, new_id)?;
        props = vec![
            (":link".to_string(), link.clone()),
            (":description".to_string(), description.clone().unwrap_or_default()),
            (":type".to_string(), "id".to_string()),
        ];
        (link, description, edited)
    } else if let Some(target) = target_at(text, at) {
        (format!("file:{}::{target}", file.abbreviate(file.path)), None, None)
    } else {
        let (link, description) = file_link_to_here(text, at, region, file, context_for_files, context);
        (link, description, None)
    };
    let description = match description {
        Some(description) if description == "NONE" => None,
        Some(description) => Some(display_format(&description)),
        None => None,
    };
    let custom_id = (interactive && custom_id_here()).then(|| {
        let (current, current_at) = edited.as_ref().map_or((text, at), |(text, cursor)| (text.as_str(), *cursor));
        // The region, where it is once the ID went in.
        let region =
            region.map(|(start, end)| (after_insertion(text, current, start), after_insertion(text, current, end)));
        file_link_to_here(current, current_at, region, file, context_for_files, context)
    });
    Ok(Store { link, description, custom_id, edited, props })
}

/// `org-link--add-to-stored-links`: the link first in `stored`, and the
/// message about it.
pub fn add_to_stored(stored: &mut Stored, link: &str, description: Option<&str>) -> String {
    let pair = (link.to_string(), description.map(str::to_string));
    let shown = description.unwrap_or(link);
    if stored.first() == Some(&pair) {
        return "This link has already been stored".to_string();
    }
    match stored.iter().position(|stored| *stored == pair) {
        None => {
            stored.insert(0, pair);
            format!("Stored: {shown}")
        }
        Some(index) => {
            stored.remove(index);
            stored.insert(0, pair);
            format!("Link moved to front: {shown}")
        }
    }
}

/// `org-store-link` run as a command: the links it made go to `stored`.
/// Returns the message it shows.
pub fn remember(stored: &mut Stored, made: &Store) -> String {
    let mut message = add_to_stored(stored, &made.link, made.description.as_deref());
    if let Some((link, description)) = &made.custom_id {
        if stored.first() != Some(&(link.clone(), description.clone())) {
            message = add_to_stored(stored, link, description.as_deref());
        }
    }
    message
}

/// `(org-store-link nil)`, as capture's `%a` has it: the link as a
/// string.
pub fn annotation(made: &Store) -> String {
    make_string(&made.link, made.description.as_deref()).unwrap_or_default()
}

/// `org-id-get-create`: the ID of the entry at `at`, made when there is
/// none. Returns the file with it, if it changed, and the ID.
pub fn id_get_create(
    text: &str,
    at: usize,
    context: &properties::Context,
    new_id: &mut dyn FnMut() -> String,
) -> Result<(Option<(String, usize)>, String), String> {
    if let Some((id, _)) = entry_id(text, at, false, context) {
        return Ok((None, id));
    }
    let id = new_id();
    let edited = properties::put(text, at, "ID", &id, context)?;
    let cursor = after_insertion(text, &edited, at);
    Ok((Some((edited, cursor)), id))
}

/// How new IDs are made: the options of `org-id-new`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdSettings {
    /// `org-id-method`
    pub method: IdMethod,
    /// `org-id-prefix`: before the ID and a colon.
    pub prefix: Option<String>,
    /// `org-id-ts-format`, for the `ts` method.
    pub ts_format: String,
    /// `org-id-include-domain`: `@` and the domain after the ID, with the
    /// `org` and `ts` methods.
    pub include_domain: bool,
}

impl Default for IdSettings {
    fn default() -> Self {
        IdSettings {
            method: IdMethod::Uuid,
            prefix: None,
            ts_format: "%Y%m%dT%H%M%S.%6N".to_string(),
            include_domain: false,
        }
    }
}

/// The values of `org-id-method`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdMethod {
    /// `uuid` (and `uuidgen`): a random UUID.
    Uuid,
    /// `org`: the time in base 36, reversed.
    Org,
    /// `ts`: the time as `org-id-ts-format` writes it.
    Ts,
}

/// The time a new ID is made at: the local date and minute, the seconds
/// and nanoseconds of the minute, and the time since the epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdClock {
    pub local: crate::date::DateTime,
    pub second: u32,
    pub nanosecond: u32,
    pub unix_seconds: u64,
    pub unix_microseconds: u32,
}

/// `org-id-new`: a new ID, as `settings` say, `fqdn` the domain
/// (`message-make-fqdn`).
pub fn new_id(settings: &IdSettings, clock: &IdClock, fqdn: &str) -> String {
    let prefix = match settings.prefix.as_deref() {
        Some(prefix) if !prefix.is_empty() => format!("{prefix}:"),
        _ => String::new(),
    };
    let domain = || if settings.include_domain { format!("@{fqdn}") } else { String::new() };
    let unique = match settings.method {
        IdMethod::Uuid => new_uuid(),
        IdMethod::Org => {
            // `org-id-time-to-b36`: the high and low parts of the seconds
            // and the microseconds, four digits each, then reversed.
            let time = format!(
                "{}{}{}",
                to_b36(clock.unix_seconds >> 16, 4),
                to_b36(clock.unix_seconds & 0xffff, 4),
                to_b36(u64::from(clock.unix_microseconds), 4)
            );
            format!("{}{}", time.chars().rev().collect::<String>(), domain())
        }
        IdMethod::Ts => format!("{}{}", format_time_precise(&settings.ts_format, clock), domain()),
    };
    format!("{prefix}{unique}")
}

/// `org-id-int-to-b36`: `number` in base 36, with at least `length` digits.
fn to_b36(mut number: u64, length: usize) -> String {
    let mut digits = Vec::new();
    while number > 0 {
        digits.push(char::from_digit((number % 36) as u32, 36).unwrap_or('0'));
        number /= 36;
    }
    let mut out: String = digits.iter().rev().collect();
    while out.len() < length.max(1) {
        out.insert(0, '0');
    }
    out
}

/// `format-time-string` with the seconds (`%S`), their fraction (`%N`,
/// `%3N`) and the seconds since the epoch (`%s`) of `clock`.
fn format_time_precise(format: &str, clock: &IdClock) -> String {
    let mut out = String::new();
    let mut chars = format.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let mut spec = String::new();
        while let Some(&next) = chars.peek() {
            if next.is_ascii_digit() || matches!(next, '-' | '_' | '0' | '^' | '#') {
                spec.push(next);
                chars.next();
            } else {
                break;
            }
        }
        match chars.next() {
            Some('S') if spec.is_empty() => out.push_str(&format!("{:02}", clock.second)),
            Some('s') if spec.is_empty() => out.push_str(&clock.unix_seconds.to_string()),
            Some('N') => {
                let digits = spec.parse::<usize>().ok().filter(|digits| (1..=9).contains(digits)).unwrap_or(9);
                out.push_str(&format!("{:09}", clock.nanosecond)[..digits]);
            }
            Some(other) => {
                out.push_str(&crate::capture::format_time(&format!("%{spec}{other}"), clock.local));
            }
            None => out.push('%'),
        }
    }
    out
}

/// `message-make-fqdn` for a system named `system_name`, without a mail
/// address set: the name when it has a dot and is not `localhost.` or
/// `.local`, else the name and `.mail-host-address-is-not-set`.
pub fn fqdn(system_name: &str) -> String {
    let lower = system_name.to_lowercase();
    let bogus = lower.starts_with("localhost.") || lower.ends_with(".local");
    if system_name.contains('.') && !bogus {
        system_name.to_string()
    } else {
        format!("{system_name}.mail-host-address-is-not-set")
    }
}

/// A new ID (`org-id-uuid`): a random UUID.
pub fn new_uuid() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut bytes = [0u8; 16];
    for (index, chunk) in bytes.chunks_mut(8).enumerate() {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_usize(index);
        if let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            hasher.write_u128(now.as_nanos());
        }
        chunk.copy_from_slice(&hasher.finish().to_le_bytes());
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}

// ----- inserting

/// What `org-insert-link` knows beyond the file it inserts in.
pub struct Outside<'a> {
    /// The link abbreviations of the file and `org-link-abbrev-alist`,
    /// whose names it offers as link types too.
    pub abbreviations: &'a [(String, String)],
    /// The entries an `id:` link completes to (`org-id-complete`), as
    /// `org-refile-get-location` offers them.
    pub id_entries: &'a [String],
    /// The entry an "Entry: " answer names.
    pub id_entry: &'a mut dyn FnMut(&str) -> Option<IdEntry>,
    /// The headline of the entry with an ID, as `org-id-description` gives
    /// it, when the ID is known.
    pub id_heading: &'a mut dyn FnMut(&str) -> Option<String>,
    /// For the ID an entry of this file gets.
    pub context: &'a properties::Context,
    pub new_id: &'a mut dyn FnMut() -> String,
}

/// An entry `id:` completes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdEntry {
    /// The headline at this byte offset of the file the link goes in; the
    /// entry gets an ID there if it has none.
    Here(usize),
    /// An entry in another file, by its ID (which the caller makes when it
    /// has none) and its headline.
    There { id: String, heading: String },
}

/// The entries `org-id-complete` offers (`org-id-completion-targets`),
/// as `org-refile-get-targets` finds them: `files[0]` is the file the
/// link goes in, `selections` the files and headlines of each target.
pub fn id_targets(
    selections: &[(crate::refile::Select, Vec<usize>)],
    files: &[crate::refile::File],
    outline_path: crate::refile::OutlinePath,
) -> Vec<(usize, crate::refile::Target)> {
    crate::refile::all_targets(selections, files, Some(0), outline_path)
}

/// The files of an `org-id-locations-file` as `org-id-locations-load`
/// reads it: the alist `(("FILE" "ID" ...) ...)` that
/// `org-id-locations-save` prints, the first string of each element.
/// Names are as written, relative ones relative to the file's directory.
pub fn id_location_files(text: &str) -> Vec<String> {
    let mut files = Vec::new();
    let mut depth = 0;
    // Whether the next string is the first thing of an element.
    let mut first = false;
    let mut chars = text.chars();
    while let Some(char) = chars.next() {
        match char {
            '(' => {
                depth += 1;
                first = depth == 2;
            }
            ')' => {
                depth -= 1;
                first = false;
                if depth <= 0 {
                    break;
                }
            }
            '"' => {
                let mut string = String::new();
                while let Some(char) = chars.next() {
                    match char {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some('\n') | None => {}
                            Some(next) => string.push(next),
                        },
                        _ => string.push(char),
                    }
                }
                if first && !files.contains(&string) {
                    files.push(string);
                }
                first = false;
            }
            ';' if depth == 0 => {
                chars.by_ref().find(|char| *char == '\n');
            }
            char if char.is_whitespace() => {}
            _ => first = false,
        }
    }
    files
}

/// Whether a file has entries with IDs, and so is one of `org-id-files`
/// once Org has scanned it.
pub fn has_ids(text: &str) -> bool {
    static ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?mi)^[ \t]*:ID:[ \t]+\S").expect("a valid regex"));
    ID.is_match(text)
}

/// `org-id-description` of the entry whose headline is at `offset`:
/// `(org-link-display-format (org-get-heading t t t t))`.
pub fn entry_heading(text: &str, offset: usize, keywords: &org_syntax::TodoKeywords) -> String {
    static COMMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\ACOMMENT[ \t]+").expect("a valid regex"));
    let line = text[offset..].lines().next().unwrap_or("");
    let level = line.bytes().take_while(|byte| *byte == b'*').count();
    let Some((from, to)) = complex_heading_title(line, level, keywords) else { return String::new() };
    crate::refile::link_display_format(&COMMENT.replace(&line[from..to], ""))
}

/// `org-link-get-description` without a description to fall back on: what
/// the link's type says it is (`:insert-description`), for `id:` and
/// `info:` links.
fn link_description(link: &str, outside: &mut Outside) -> Option<String> {
    let ty = link.find(':').map(|at| &link[..at])?;
    let known = outside.abbreviations.iter().any(|(name, _)| name == ty) || org_syntax::link_types().contains(&ty);
    if !known {
        return None;
    }
    let rest = &link[ty.len() + 1..];
    match ty {
        "id" => (outside.id_heading)(rest),
        // `org-info-description-as-command`
        "info" => {
            let (file, node) = crate::manual::info_file_node(rest);
            Some(match (file.as_str(), node.as_str()) {
                ("dir", "Top") => "info \"(dir)\"".to_string(),
                (file, "Top") => format!("info {file}"),
                (file, node) => format!("info \"({file}) {node}\""),
            })
        }
        _ => None,
    }
}

/// What a command asks next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub prompt: String,
    /// What the answer starts as.
    pub initial: String,
    /// What it may complete to.
    pub completions: Vec<String>,
}

/// The file after a link went in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inserted {
    pub text: String,
    pub cursor: usize,
    /// The stored links after it.
    pub stored: Stored,
}

/// What `insert` does with the answers so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Ask(Question),
    Done(Inserted),
}

fn ask(prompt: &str, initial: &str) -> Step {
    Step::Ask(Question { prompt: prompt.to_string(), initial: initial.to_string(), completions: Vec::new() })
}

/// `org-unbracket-string "<" ">"`
fn unbracket(text: &str) -> &str {
    match text.strip_prefix('<').and_then(|rest| rest.strip_suffix('>')) {
        Some(inner) => inner,
        None => text,
    }
}

/// `org-link-complete-file`: a file link to the name typed at "File: ".
fn complete_file(name: &str, absolute: bool, file: &File) -> String {
    let directory = file.directory();
    let pwd = format!("{}/", directory.trim_end_matches('/'));
    let pwd1 = format!("{}/", file.abbreviate(&directory).trim_end_matches('/'));
    if absolute {
        return format!("file:{}", file.abbreviate(&file.expand(name)));
    }
    if let Some(rest) = name.strip_prefix(&pwd1).filter(|rest| !rest.is_empty()) {
        return format!("file:{rest}");
    }
    if let Some(rest) = file.expand(name).strip_prefix(&pwd).filter(|rest| !rest.is_empty()) {
        return format!("file:{rest}");
    }
    format!("file:{name}")
}

/// What "File: " starts as: the file's directory.
fn file_prompt_initial(file: &File) -> String {
    format!("{}/", file.abbreviate(&file.directory()).trim_end_matches('/'))
}

/// `org-link-make-string-for-buffer`: `link` as it goes into the file,
/// relative to it. `ask` is the "Description: " answer when there is
/// one to give.
fn make_string_for_buffer(
    link: &str,
    description: Option<&str>,
    file: &File,
    path_type: PathType,
    answer: Option<Option<&String>>,
    outside: Option<&mut Outside>,
) -> Result<Result<String, Step>, String> {
    static TIMESTAMP: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"<[0-9]{4}-[0-9]{2}-[0-9]{2}(?: +[^\]+0-9>\r\n -]+)?(?: +[0-9]{1,2}:[0-9]{2})?>")
            .expect("a valid regex")
    });
    let mut link = link.to_string();
    let mut description = description.map(str::to_string);
    if has_plain_link(&link) && !TIMESTAMP.is_match(&link) {
        link = unbracket(&link).to_string();
    }
    // A link to this file with a search: the search alone.
    if let Some(rest) = link.strip_prefix("file:") {
        if let Some(split) = rest.find("::").filter(|split| *split > 0) {
            let (path, search) = (&rest[..split], &rest[split + 2..]);
            if file.truename(file.path) == file.truename(path) {
                link = search.to_string();
            }
        }
    }
    let kind = ["file:", "docview:"].into_iter().find(|kind| link.starts_with(kind));
    if let Some(kind) = kind {
        let rest = &link[kind.len()..];
        let (path, search) = match rest.find("::").filter(|split| !rest[split + 2..].contains('\n')) {
            Some(split) => (&rest[..split], Some(&rest[split + 2..])),
            None => (rest, None),
        };
        let original = if not_blank(path) { path.to_string() } else { file.path.to_string() };
        let path = normalize_filename(&original, path_type, file);
        if description.as_deref() == Some(original.as_str()) {
            description = Some(path.clone());
        }
        link = format!("{kind}{path}{}", search.map(|search| format!("::{search}")).unwrap_or_default());
    }
    if description.is_none() {
        if let Some(outside) = outside {
            description = link_description(&link, outside);
        }
    }
    if let Some(answer) = answer {
        match answer {
            Some(answer) => description = Some(answer.clone()),
            None => return Ok(Err(ask("Description", description.as_deref().unwrap_or("")))),
        }
    }
    let description = description.filter(|description| not_blank(description));
    make_string(&link, description.as_deref()).map(Ok)
}

/// The link at `at` that `org-insert-link` edits: where it is, the link
/// as the prompt starts with it and its description.
fn link_at(text: &str, at: usize) -> Option<((usize, usize), String, Option<String>)> {
    if let Some(captures) = in_regexp(&BRACKET, text, at) {
        let found = captures.get(0)?;
        let link = unescape(captures.get(1).map_or("", |m| m.as_str()));
        let description = captures.get(2).map(|m| m.as_str().to_string());
        return Some(((found.start(), found.end()), link, description));
    }
    if let Some(captures) = in_regexp(&ANGLE, text, at) {
        let found = captures.get(0)?;
        return Some(((found.start(), found.end()), unbracket(found.as_str()).to_string(), None));
    }
    let (start, end) = in_plain_link(text, at)?;
    Some(((start, end), unbracket(&text[start..end]).to_string(), None))
}

/// `org-insert-link` (`C-c C-l`) at `at`, with `prefix` the `C-u`s (0 to
/// 3), the answers so far and the stored links. The active region,
/// `region`, is the link's description, and the link takes its place.
#[allow(clippy::too_many_arguments)]
pub fn insert(
    text: &str,
    at: usize,
    region: Option<(usize, usize)>,
    file: &File,
    stored: &Stored,
    prefix: usize,
    answers: &[String],
    settings: &Settings,
    outside: &mut Outside,
) -> Result<Step, String> {
    let mut answers = answers.iter();
    let mut remove = region;
    let mut description = region.map(|(start, end)| text[start..end].to_string());
    let link;
    // The file and the cursor once an `id:` entry of this file has its ID.
    let mut text = std::borrow::Cow::Borrowed(text);
    let mut at = at;
    // The headline of the entry an `id:` link was completed to.
    let mut completed = None;
    if let Some((range, initial, desc)) = link_at(&text, at) {
        remove = Some(range);
        description = desc;
        match answers.next() {
            Some(answer) => link = answer.clone(),
            None => return Ok(ask("Link", &initial)),
        }
    } else if prefix == 1 || prefix == 2 {
        match answers.next() {
            Some(answer) => link = complete_file(answer, prefix == 2, file),
            None => return Ok(ask("File", &file_prompt_initial(file))),
        }
    } else {
        let mut prefixes: Vec<&str> = outside.abbreviations.iter().map(|(name, _)| name.as_str()).collect();
        prefixes.extend(org_syntax::link_types());
        let default = stored.first().map(|(link, _)| link.clone());
        let Some(answer) = answers.next() else {
            let mut completions: Vec<String> = prefixes.iter().map(|ty| format!("{ty}:")).collect();
            completions.extend(stored.iter().map(|(link, _)| link.clone()));
            completions.extend(stored.iter().filter_map(|(_, description)| description.clone()));
            let prompt = match &default {
                Some(default) => format!("Insert link (default {default})"),
                None => "Insert link".to_string(),
            };
            return Ok(Step::Ask(Question { prompt, initial: String::new(), completions }));
        };
        let mut answer = if answer.is_empty() { default.unwrap_or_default() } else { answer.clone() };
        if !not_blank(&answer) {
            return Err("No link selected".to_string());
        }
        for (stored_link, stored_description) in stored {
            if stored_description.as_deref() == Some(answer.as_str()) {
                answer = stored_link.clone();
            }
        }
        let bare = answer.strip_suffix(':').unwrap_or(&answer);
        if prefixes.iter().any(|ty| *ty == answer || *ty == bare) {
            let ty = bare.to_string();
            answer = match (ty.as_str(), answers.next()) {
                ("file", Some(name)) => complete_file(name, false, file),
                ("file", None) => return Ok(ask("File", &file_prompt_initial(file))),
                // `org-id-complete`: an entry, which gets an ID if it has
                // none; failing that, the link typed.
                ("id", None) if !outside.id_entries.is_empty() => {
                    return Ok(Step::Ask(Question {
                        prompt: "Entry".to_string(),
                        initial: String::new(),
                        completions: outside.id_entries.to_vec(),
                    }));
                }
                ("id", Some(entry)) if !outside.id_entries.is_empty() => match (outside.id_entry)(entry) {
                    Some(IdEntry::Here(heading)) => {
                        let (edited, id) = id_get_create(&text, heading, outside.context, outside.new_id)?;
                        if let Some((edited, _)) = edited {
                            at = after_insertion(&text, &edited, at);
                            remove = remove.map(|(start, end)| {
                                (after_insertion(&text, &edited, start), after_insertion(&text, &edited, end))
                            });
                            text = std::borrow::Cow::Owned(edited);
                        }
                        completed = Some(entry_heading(&text, heading, &outside.context.keywords));
                        format!("id:{id}")
                    }
                    Some(IdEntry::There { id, heading }) => {
                        completed = Some(heading);
                        format!("id:{id}")
                    }
                    None => match answers.next() {
                        Some(typed) => typed.clone(),
                        None => return Ok(ask("Link", "id:")),
                    },
                },
                ("id", None) => return Ok(ask("Link", "id:")),
                (_, Some(typed)) => typed.clone(),
                (_, None) => return Ok(ask("Link (no completion support)", &format!("{ty}:"))),
            };
        }
        link = answer;
        // The region's text, else the stored link's description.
        description = description
            .or_else(|| stored.iter().find(|(stored_link, _)| *stored_link == link).and_then(|(_, desc)| desc.clone()));
        // `org-id-description`, of an entry whose ID may be new.
        description = description.or(completed);
    }
    let path_type = if prefix == 2 { PathType::Absolute } else { settings.path_type };
    let made = match make_string_for_buffer(
        &link,
        description.as_deref(),
        file,
        path_type,
        Some(answers.next()),
        Some(outside),
    )? {
        Ok(made) => made,
        Err(step) => return Ok(step),
    };
    let mut stored = stored.clone();
    if settings.keep_stored == (prefix == 3) {
        if let Some(index) = stored.iter().position(|(stored_link, _)| *stored_link == link) {
            stored.remove(index);
        }
    }
    let (start, end) = remove.unwrap_or((at, at));
    let mut edited = text.into_owned();
    edited.replace_range(start..end, &made);
    Ok(Step::Done(Inserted { text: edited, cursor: start + made.len(), stored }))
}

/// `org-insert-all-links`: the stored links at `at`, each after `pre` and
/// before `post`. `count` is how many (`org-insert-last-stored-link`
/// inserts one), all of them without it; `keep` keeps them stored.
#[allow(clippy::too_many_arguments)]
pub fn insert_all(
    text: &str,
    at: usize,
    file: &File,
    stored: &Stored,
    count: Option<usize>,
    keep: bool,
    pre: &str,
    post: &str,
    settings: &Settings,
) -> Result<Option<Inserted>, String> {
    if stored.is_empty() {
        return Ok(None);
    }
    let mut stored = stored.clone();
    let mut copy = stored.clone();
    let mut edited = text.to_string();
    let mut cursor = at;
    let mut inserted = 0;
    loop {
        if count.is_some_and(|count| inserted >= count) {
            break;
        }
        let next = match count {
            Some(_) if stored.is_empty() => None,
            Some(_) => Some(stored.remove(0)),
            None if copy.is_empty() => None,
            None => Some(copy.remove(0)),
        };
        let Some((link, description)) = next else { break };
        inserted += 1;
        let description = description.unwrap_or_else(|| "<no description>".to_string());
        let made = match make_string_for_buffer(&link, Some(&description), file, settings.path_type, None, None)? {
            Ok(made) => made,
            Err(_) => unreachable!("nothing is asked"),
        };
        if !keep {
            if let Some(index) = stored.iter().position(|(stored_link, _)| *stored_link == link) {
                stored.remove(index);
            }
        }
        let piece = format!("{pre}{made}{post}");
        edited.insert_str(cursor, &piece);
        cursor += piece.len();
    }
    Ok(Some(Inserted { text: edited, cursor, stored }))
}

#[cfg(test)]
mod tests {

    #[test]
    fn reads_the_files_of_an_id_locations_file() {
        // As `org-id-locations-save` prints it; Emacs's `read` gives the
        // same names.
        let text = "\n((\"~/notes/a.org\" \"id-1\" \"id-2\") (\"rel/b \\\"q\\\" \\\\ c.org\" \"x\") (\"/abs/ü.org\" \"y\") (\"~/notes/a.org\" \"z\"))\n";
        assert_eq!(id_location_files(text), ["~/notes/a.org", "rel/b \"q\" \\ c.org", "/abs/ü.org"]);
        assert!(id_location_files("").is_empty());
        assert!(id_location_files("nil").is_empty());
    }

    /// `org-id-new` at 2025-10-09 08:53:20.123456789 UTC, as Emacs makes
    /// it with each method.
    #[test]
    fn new_ids_as_emacs_makes_them() {
        use crate::date::{Date, DateTime};
        let clock = IdClock {
            local: DateTime { day: Date::new(2025, 10, 9).to_day(), minute: 8 * 60 + 53 },
            second: 20,
            nanosecond: 123_456_789,
            unix_seconds: 1_760_000_000,
            unix_microseconds: 123_456,
        };
        let id = |method, prefix: Option<&str>, domain, format: &str| {
            let settings = IdSettings {
                method,
                prefix: prefix.map(str::to_string),
                ts_format: format.to_string(),
                include_domain: domain,
            };
            new_id(&settings, &clock, &fqdn("vm"))
        };
        let ts = IdSettings::default().ts_format;
        assert_eq!(id(IdMethod::Org, None, false, &ts), "c9n2cpn0zpk0");
        assert_eq!(id(IdMethod::Org, Some("Org"), false, &ts), "Org:c9n2cpn0zpk0");
        assert_eq!(id(IdMethod::Org, None, true, &ts), "c9n2cpn0zpk0@vm.mail-host-address-is-not-set");
        assert_eq!(id(IdMethod::Ts, None, false, &ts), "20251009T085320.123456");
        assert_eq!(
            id(IdMethod::Ts, None, false, "%Y-%m-%d %H:%M:%S %3N %s %% %N"),
            "2025-10-09 08:53:20 123 1760000000 % 123456789"
        );
        assert_eq!(fqdn("host.example.org"), "host.example.org");
        assert_eq!(fqdn("box.local"), "box.local.mail-host-address-is-not-set");
        let uuid = id(IdMethod::Uuid, Some("P"), true, &ts);
        assert!(uuid.starts_with("P:") && uuid.len() == 38 && !uuid.contains('@'), "{uuid}");
    }
    use super::*;

    #[test]
    fn escapes_links() {
        assert_eq!(escape("a[b]\\"), "a\\[b\\]\\\\");
        assert_eq!(unescape(&escape("x\\[y]\\")), "x\\[y]\\");
        assert_eq!(make_string("a", Some("b]")).unwrap(), "[[a][b]\u{200B}]]");
        assert_eq!(make_string("", None), Err("Empty link".to_string()));
    }

    #[test]
    fn normalizes_file_names() {
        let file = File { path: "/home/u/org/main.org", home: Some("/home/u") };
        assert_eq!(normalize_filename("sub/x.org", PathType::Adaptive, &file), "sub/x.org");
        assert_eq!(normalize_filename("../x.org", PathType::Adaptive, &file), "~/x.org");
        assert_eq!(normalize_filename("/tmp/x.org", PathType::Relative, &file), "../../../tmp/x.org");
        assert_eq!(normalize_filename("x.org", PathType::NoAbbrev, &file), "/home/u/org/x.org");
    }
}
