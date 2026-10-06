//! Archiving: a port of `org-archive.el`. [`archive_subtree`] moves a
//! subtree to its archive location (`org-archive-subtree`), [`to_sibling`]
//! under an `Archive` sibling (`org-archive-to-archive-sibling`),
//! [`toggle_tag`] sets or removes the `ARCHIVE` tag
//! (`org-toggle-archive-tag`), and [`archive_all`] does either to every
//! subtree without open TODO items, asking for each (the same commands with
//! `C-u`). Checked against Emacs in `tests/archive`.

use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

use org_syntax::TodoKeywords;
use regex::Regex;

use crate::checkbox;
use crate::date::{weekday, Date, DateTime, MONTH_NAMES, WEEKDAY_NAMES};
use crate::outline::{complex_heading_title, keyword_values, Outline};
use crate::refile::{self, heading_at, outline_path, shift_levels, subtree_end};
use crate::stamp::format_timestamp;
use crate::text::tag_group;
use crate::todo::{self, headline_level, tags_padding, Editor, Target};
use crate::Refusal;

/// `org-archive-tag`
pub const ARCHIVE_TAG: &str = "ARCHIVE";

/// `org-archive-mark-done`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkDone {
    No,
    /// The first done keyword.
    Yes,
    /// This keyword, when it is a done one.
    Keyword(String),
}

/// `org-archive-subtree-add-inherited-tags`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InheritedTags {
    Never,
    /// When archiving to the same file.
    InFile,
    Always,
}

/// An entry of `org-archive-save-context-info`: what the archived entry
/// keeps as an `ARCHIVE_…` property.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextInfo {
    Time,
    File,
    Olpath,
    Category,
    Todo,
    Itags,
    Ltags,
    Olid,
    /// Offered by Org's customization but never stored.
    Priority,
}

impl ContextInfo {
    pub const ALL: [ContextInfo; 9] = [
        ContextInfo::Time,
        ContextInfo::File,
        ContextInfo::Olpath,
        ContextInfo::Category,
        ContextInfo::Todo,
        ContextInfo::Itags,
        ContextInfo::Ltags,
        ContextInfo::Olid,
        ContextInfo::Priority,
    ];

    /// The symbol in `org-archive-save-context-info`.
    pub fn name(self) -> &'static str {
        match self {
            ContextInfo::Time => "time",
            ContextInfo::File => "file",
            ContextInfo::Olpath => "olpath",
            ContextInfo::Category => "category",
            ContextInfo::Todo => "todo",
            ContextInfo::Itags => "itags",
            ContextInfo::Ltags => "ltags",
            ContextInfo::Olid => "olid",
            ContextInfo::Priority => "priority",
        }
    }

    pub fn from_name(name: &str) -> Option<ContextInfo> {
        ContextInfo::ALL.into_iter().find(|info| info.name() == name)
    }
}

/// Settings, named after Org's variables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// `org-archive-location`
    pub location: String,
    /// `org-archive-reversed-order`
    pub reversed_order: bool,
    /// `org-archive-mark-done`
    pub mark_done: MarkDone,
    /// `org-archive-subtree-add-inherited-tags`
    pub add_inherited_tags: InheritedTags,
    /// `org-archive-save-context-info`
    pub context_info: Vec<ContextInfo>,
    /// `org-archive-sibling-heading`
    pub sibling_heading: String,
    /// `org-archive-file-header-format`
    pub file_header_format: Option<String>,
    /// How a subtree changes level, as refiling does it.
    pub levels: refile::Settings,
    /// For the done keyword, the properties and the statistics cookies.
    pub todo: todo::Settings,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            location: "%s_archive::".to_string(),
            reversed_order: false,
            mark_done: MarkDone::No,
            add_inherited_tags: InheritedTags::InFile,
            context_info: vec![
                ContextInfo::Time,
                ContextInfo::File,
                ContextInfo::Olpath,
                ContextInfo::Category,
                ContextInfo::Todo,
                ContextInfo::Itags,
            ],
            sibling_heading: "Archive".to_string(),
            file_header_format: Some("\nArchived entries from file %s\n\n".to_string()),
            levels: refile::Settings::default(),
            todo: todo::Settings::default(),
        }
    }
}

/// Where the archiving happens.
pub struct Context<'a> {
    /// The file of the subtree, absolute.
    pub path: &'a Path,
    /// The home directory, for `~` in locations and in `ARCHIVE_FILE`.
    pub home: Option<&'a Path>,
    pub now: DateTime,
}

/// An archive location: the file, and the heading in it (`*` stars, a
/// `datetree/` or nothing).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub file: PathBuf,
    pub heading: String,
}

/// A file archiving wrote to, other than the subtree's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveFile {
    pub path: PathBuf,
    /// Its text before, or none when it did not exist.
    pub before: Option<String>,
    pub text: String,
}

/// The outcome of archiving.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Archived {
    /// The subtree's file.
    pub source: String,
    /// The other files written to.
    pub files: Vec<ArchiveFile>,
    /// What Emacs says.
    pub message: String,
    /// Where the cursor goes in the subtree's file.
    pub cursor: usize,
}

/// What [`archive_all`] needs next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllOutcome {
    /// A yes or no for this question, after the answers given.
    Ask(String),
    Done(Archived),
}

/// The files read so far, and how they read now.
struct Files<'r> {
    read: &'r mut dyn FnMut(&Path) -> Option<String>,
    files: Vec<ArchiveFile>,
}

impl ArchiveFile {
    /// Whether the file is still to be made: Emacs saves it after each
    /// archiving.
    fn is_new(&self) -> bool {
        self.before.is_none() && self.text.is_empty()
    }
}

impl Files<'_> {
    fn get(&mut self, path: &Path) -> usize {
        if let Some(index) = self.files.iter().position(|file| file.path == path) {
            return index;
        }
        let before = (self.read)(path);
        let text = before.clone().unwrap_or_default();
        self.files.push(ArchiveFile { path: path.to_path_buf(), before, text });
        self.files.len() - 1
    }
}

// ----- paths

/// `expand-file-name` of `name` in `directory`.
fn expand(name: &str, directory: &Path, home: Option<&Path>) -> PathBuf {
    let path = match (name.strip_prefix('~'), home) {
        (Some(rest), Some(home)) if rest.is_empty() || rest.starts_with('/') => home.join(rest.trim_start_matches('/')),
        _ => directory.join(name),
    };
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// `abbreviate-file-name`: the home directory as `~`.
fn abbreviate(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) if !rest.as_os_str().is_empty() => format!("~/{}", rest.display()),
        _ => path.display().to_string(),
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
}

/// `(format FORMAT NAME)` for a format with one `%s`.
fn format_name(format: &str, name: &str) -> String {
    format.replacen("%s", name, 1).replace("%%", "%")
}

fn has_text(text: &str) -> bool {
    text.chars().any(|c| !c.is_whitespace())
}

/// The archive location that applies to the entry at `heading`: its
/// `ARCHIVE` property or an ancestor's, `#+PROPERTY: ARCHIVE`, the first
/// `#+ARCHIVE:`, or `org-archive-location`.
fn location_spec(text: &str, heading: Option<usize>, keywords: &TodoKeywords, default: &str) -> String {
    let parse = org_syntax::parse(text, keywords);
    let outline = Outline::new(&parse, text, "");
    let mut entry = heading.and_then(|heading| outline.entries.iter().position(|entry| entry.offset == heading));
    while let Some(at) = entry {
        let found = outline.entries[at].properties.iter().find(|(key, _)| key == "ARCHIVE");
        if let Some((_, value)) = found {
            return value.clone();
        }
        entry = outline.entries[at].parent;
    }
    let root = parse.syntax();
    let property = keyword_values(&root, "PROPERTY").into_iter().rev().find_map(|value| {
        let value = value.trim();
        let rest = value.strip_prefix("ARCHIVE")?;
        rest.starts_with([' ', '\t']).then(|| rest.trim().to_string())
    });
    if let Some(value) = property {
        return value;
    }
    keyword_values(&root, "ARCHIVE").into_iter().next().unwrap_or_else(|| default.to_string())
}

/// `org-archive--compute-location`
fn compute_location(spec: &str, context: &Context) -> Result<Location, Refusal> {
    let Some(split) = spec.find("::") else {
        return Err(Refusal::Failed(format!("Invalid archive location: {spec:?}")));
    };
    let name = file_name(context.path);
    let directory = context.path.parent().unwrap_or(Path::new("/"));
    let file = &spec[..split];
    let file = if has_text(file) {
        expand(&format_name(file, &name), directory, context.home)
    } else {
        context.path.to_path_buf()
    };
    Ok(Location { file, heading: format_name(&spec[split + 2..], &name) })
}

/// `org-all-archive-files`: the files the subtrees of `text` are archived
/// to: the file's archive location (`#+ARCHIVE:` or `default`), then those
/// of `ARCHIVE` properties, each once. Whether they exist is the caller's
/// to check; a location that is not one is left out.
pub fn archive_files(text: &str, keywords: &TodoKeywords, context: &Context, default: &str) -> Vec<PathBuf> {
    let root = org_syntax::parse(text, keywords).syntax();
    let mut specs = vec![keyword_values(&root, "ARCHIVE").into_iter().next().unwrap_or_else(|| default.to_string())];
    // `:ARCHIVE:` lines in property drawers.
    let mut in_drawer = false;
    for line in text.lines() {
        let trimmed = line.trim_matches([' ', '\t']);
        if trimmed.eq_ignore_ascii_case(":PROPERTIES:") {
            in_drawer = true;
        } else if trimmed.eq_ignore_ascii_case(":END:") {
            in_drawer = false;
        } else if in_drawer {
            let value = trimmed.get(..9).filter(|key| key.eq_ignore_ascii_case(":ARCHIVE:")).map(|_| &trimmed[9..]);
            if let Some(value) = value.filter(|value| value.starts_with([' ', '\t'])) {
                specs.push(value.trim().to_string());
            }
        }
    }
    let mut files: Vec<PathBuf> = Vec::new();
    for spec in specs {
        if let Ok(location) = compute_location(&spec, context) {
            if !files.contains(&location.file) {
                files.push(location.file);
            }
        }
    }
    files
}

/// The archive location of the entry at byte `offset`.
pub fn location(
    text: &str,
    offset: usize,
    keywords: &TodoKeywords,
    context: &Context,
    settings: &Settings,
) -> Result<Location, Refusal> {
    let spec = location_spec(text, heading_at(text, offset), keywords, &settings.location);
    compute_location(&spec, context)
}

// ----- a buffer, as Emacs has it

/// Text with a point and a narrowing, and markers that move with edits.
struct Buffer {
    text: String,
    begv: usize,
    zv: usize,
    point: usize,
    /// Positions that follow the edits; with `true`, text inserted right
    /// there goes before them.
    markers: Vec<(usize, bool)>,
}

impl Buffer {
    fn new(text: String) -> Buffer {
        let zv = text.len();
        Buffer { text, begv: 0, zv, point: 0, markers: Vec::new() }
    }

    fn insert(&mut self, what: &str) {
        let at = self.point;
        self.text.insert_str(at, what);
        self.zv += what.len();
        self.point += what.len();
        for (marker, advances) in &mut self.markers {
            if *marker > at || (*marker == at && *advances) {
                *marker += what.len();
            }
        }
    }

    fn delete(&mut self, from: usize, to: usize) {
        if from >= to {
            return;
        }
        self.text.replace_range(from..to, "");
        let moved = |at: &mut usize| {
            if *at >= to {
                *at -= to - from;
            } else if *at > from {
                *at = from;
            }
        };
        moved(&mut self.zv);
        moved(&mut self.point);
        for (marker, _) in &mut self.markers {
            moved(marker);
        }
    }

    fn widen(&mut self) {
        self.begv = 0;
        self.zv = self.text.len();
    }

    fn bolp(&self) -> bool {
        self.point == self.begv || self.text.as_bytes()[self.point - 1] == b'\n'
    }

    fn eobp(&self) -> bool {
        self.point == self.zv
    }

    fn line_start(&self, at: usize) -> usize {
        self.text[self.begv..at].rfind('\n').map_or(self.begv, |index| self.begv + index + 1)
    }

    fn line_end(&self, at: usize) -> usize {
        self.text[at..self.zv].find('\n').map_or(self.zv, |index| at + index)
    }

    fn level(&self, start: usize) -> Option<usize> {
        headline_level(&self.text[start..self.line_end(start)])
    }

    /// `outline-next-heading`: the first headline that starts after
    /// `at`, or the end.
    fn next_heading(&self, at: usize) -> usize {
        let mut line = at;
        loop {
            match self.text[line..self.zv].find('\n') {
                Some(index) => line += index + 1,
                None => return self.zv,
            }
            if line < self.zv && self.level(line).is_some() {
                return line;
            }
        }
    }

    /// `org-back-to-heading`: the headline at or above `at`.
    fn heading_before(&self, at: usize) -> Option<usize> {
        let mut line = self.line_start(at);
        loop {
            if self.level(line).is_some() {
                return Some(line);
            }
            if line == self.begv {
                return None;
            }
            line = self.line_start(line - 1);
        }
    }

    /// `org-narrow-to-subtree` for the headline at `heading`.
    fn narrow_to_subtree(&mut self, heading: usize) {
        let end = subtree_end(&self.text, heading);
        self.begv = heading;
        self.zv = self.zv.min(if end == self.zv { end } else { end - 1 });
    }

    /// `skip-chars-backward` over `chars`.
    fn skip_back(&self, at: usize, chars: &[char]) -> usize {
        self.begv + self.text[self.begv..at].trim_end_matches(chars).len()
    }

    /// `org-previous-line-empty-p` at `at`.
    fn previous_line_empty(&self, at: usize) -> bool {
        if at == self.begv {
            return false;
        }
        let start = self.line_start(at);
        let previous = if start == self.begv { start } else { self.line_start(start - 1) };
        self.text[previous..self.line_end(previous)].trim_matches([' ', '\t']).is_empty()
    }

    /// `org--blank-before-heading-p`, with `org-blank-before-new-entry`
    /// at its default (`auto` for headings).
    fn blank_before_heading(&self) -> bool {
        let heading = match self.heading_before(self.point) {
            Some(heading) => heading,
            None => {
                let next = self.next_heading(self.point);
                if next == self.zv {
                    return false;
                }
                next
            }
        };
        if heading != self.begv {
            return self.previous_line_empty(heading);
        }
        let next = self.next_heading(heading);
        if next < self.zv {
            return self.previous_line_empty(next);
        }
        let end = self.skip_back(self.zv, &[' ', '\t']);
        let bol = end == self.begv || self.text.as_bytes()[end - 1] == b'\n';
        bol && self.previous_line_empty(end)
    }

    /// `org-paste-subtree` of `tree` at the point, its first headline at
    /// `level`. Returns where it starts.
    fn paste(&mut self, tree: &str, level: usize, settings: &refile::Settings) -> usize {
        if !(self.bolp() && !self.eobp()) {
            self.point = self.next_heading(self.point);
            if !self.bolp() {
                self.insert("\n");
            }
        }
        let start = self.point;
        let shifted = shift_levels(tree, level, settings, false);
        self.insert(&shifted);
        if !shifted.ends_with('\n') {
            self.insert("\n");
        }
        self.point = start;
        start
    }
}

/// Where `at` of `old` is in `new`, when what changed is one stretch.
fn track(old: &str, new: &str, at: usize) -> usize {
    let prefix = old.bytes().zip(new.bytes()).take_while(|(a, b)| a == b).count();
    let max_suffix = old.len().min(new.len()) - prefix;
    let suffix = old.bytes().rev().zip(new.bytes().rev()).take(max_suffix).take_while(|(a, b)| a == b).count();
    if at <= prefix {
        at
    } else if at >= old.len() - suffix {
        at + new.len() - old.len()
    } else {
        prefix
    }
}

// ----- tags

/// The tags on the headline line `line`.
fn local_tags(line: &str) -> Vec<String> {
    match tag_group(line) {
        Some((_, start)) => line[start..]
            .trim_end_matches([' ', '\t'])
            .split(':')
            .filter(|tag| !tag.is_empty())
            .map(String::from)
            .collect(),
        None => Vec::new(),
    }
}

/// `org-set-tags` on the headline at `heading`.
/// An edit as Emacs makes it: `start..end` deleted, then `inserted`
/// bytes inserted at `start`.
type Edit = (usize, usize, usize);

/// Where a marker at `at` is after `edits`. With `advances`, text inserted
/// right at it goes before it (`insertion-type` t).
fn follow(edits: &[Edit], mut at: usize, advances: bool) -> usize {
    for &(start, end, inserted) in edits {
        if at > end || (at == end && at == start && advances) {
            at = at + inserted - (end - start);
        } else if at > start {
            at = start;
        }
    }
    at
}

/// `org-set-tags` on the headline at `heading`. Returns the edits.
fn set_tags(text: &mut String, heading: usize, tags: &[String], tags_column: i64) -> Vec<Edit> {
    let mut edits = Vec::new();
    let end = text[heading..].find('\n').map_or(text.len(), |index| heading + index);
    let line = &text[heading..end];
    if local_tags(line) != tags {
        let cut = match tag_group(line) {
            Some((_, start)) => start,
            None => line.len(),
        };
        let kept = line[..cut].trim_end_matches([' ', '\t']).len();
        let mut inserted = String::new();
        if headline_level(&line[..kept]).is_none() {
            inserted.push(' ');
        }
        if !tags.is_empty() {
            inserted.push_str(&format!(" :{}:", tags.join(":")));
        }
        text.replace_range(heading + kept..end, &inserted);
        edits.push((heading + kept, end, inserted.len()));
    }
    if !tags.is_empty() {
        let end = text[heading..].find('\n').map_or(text.len(), |index| heading + index);
        if let Some((blanks, start, padding)) = tags_padding(&text[heading..end], tags_column) {
            text.replace_range(heading + blanks..heading + start, &padding);
            edits.push((heading + blanks, heading + start, padding.len()));
        }
    }
    edits
}

/// `org-toggle-tag` for `ARCHIVE` on the headline at `heading`, or `on`
/// to set it. Whether the tag is now set, and the edits.
fn toggle_archive_tag(text: &mut String, heading: usize, on: bool, tags_column: i64) -> (bool, Vec<Edit>) {
    let end = text[heading..].find('\n').map_or(text.len(), |index| heading + index);
    let mut tags = local_tags(&text[heading..end]);
    let set = on || !tags.iter().any(|tag| tag == ARCHIVE_TAG);
    if set {
        if !tags.iter().any(|tag| tag == ARCHIVE_TAG) {
            tags.push(ARCHIVE_TAG.to_string());
        }
    } else {
        tags.retain(|tag| tag != ARCHIVE_TAG);
    }
    let edits = set_tags(text, heading, &tags, tags_column);
    (set, edits)
}

// ----- statistics

static CHECKBOX_ITEM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^[ \t]*(?:[-+*]|[0-9]+[.)]) \[[- X]\]").unwrap());
static COOKIE_DATA_TODO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i):COOKIE_DATA:.*\btodo\b").unwrap());
static COOKIE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[[0-9]*%\]|\[[0-9]*/[0-9]*\]").unwrap());

fn line_end(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |index| at + index)
}

/// The headline above `heading` (`org-up-heading-safe`).
fn parent(text: &str, heading: usize) -> Option<usize> {
    let level = headline_level(&text[heading..line_end(text, heading)])?;
    let mut at = heading;
    while at > 0 {
        at = text[..at - 1].rfind('\n').map_or(0, |index| index + 1);
        if headline_level(&text[at..line_end(text, at)]).is_some_and(|found| found < level) {
            return Some(at);
        }
    }
    None
}

/// `outline-up-heading`: back to a headline of a lower level, or as far
/// as the first line. None at the top level.
fn up_heading(text: &str, heading: usize) -> Option<usize> {
    let start = headline_level(&text[heading..line_end(text, heading)])?;
    if start <= 1 {
        return None;
    }
    let mut at = heading;
    while at > 0 {
        at = text[..at - 1].rfind('\n').map_or(0, |index| index + 1);
        while at > 0 && headline_level(&text[at..line_end(text, at)]).is_none() {
            at = text[..at - 1].rfind('\n').map_or(0, |index| index + 1);
        }
        if headline_level(&text[at..line_end(text, at)]).is_none_or(|level| level < start) {
            break;
        }
    }
    Some(at)
}

/// `org-update-statistics-cookies` at `at`.
fn update_statistics(
    text: String,
    at: usize,
    keywords: &TodoKeywords,
    settings: &todo::Settings,
    now: DateTime,
) -> String {
    let at = at.min(text.len());
    let start = text[..at].rfind('\n').map_or(0, |index| index + 1);
    let Some(level) = headline_level(&text[start..line_end(&text, start)]) else {
        return checkbox::update_counts(&text, keywords, at, settings.tags_column);
    };
    let at = start;
    let end = refile::next_heading(&text, at).unwrap_or(text.len());
    let child = (end < text.len()).then(|| headline_level(&text[end..line_end(&text, end)])).flatten();
    let section = &text[at..end];
    if CHECKBOX_ITEM.is_match(section) && !COOKIE_DATA_TODO.is_match(section) {
        return checkbox::update_counts(&text, keywords, at, settings.tags_column);
    }
    if child.is_some_and(|child| child > level) {
        let mut editor = Editor::new(&text, keywords, settings, now);
        editor.heading = end;
        editor.update_parent_statistics();
        return editor.text;
    }
    let end = line_end(&text, at);
    let line = COOKIE.replace_all(&text[at..end], |found: &regex::Captures| {
        if found[0].ends_with("%]") {
            "[100%]".to_string()
        } else {
            "[0/0]".to_string()
        }
    });
    format!("{}{}{}", &text[..at], line, &text[end..])
}

/// After a subtree is cut at `at`: `org-up-heading-safe`, then the
/// statistics cookies there.
fn update_after_cut(
    text: String,
    at: usize,
    keywords: &TodoKeywords,
    settings: &todo::Settings,
    now: DateTime,
) -> String {
    let target = match heading_at(&text, at) {
        Some(heading) => parent(&text, heading).unwrap_or(heading),
        None => at,
    };
    update_statistics(text, target, keywords, settings, now)
}

/// After the command, the cursor moves off a blank line to the next
/// headline.
fn settle(text: &str, at: usize) -> usize {
    let start = text[..at].rfind('\n').map_or(0, |index| index + 1);
    if text[at..line_end(text, at)].trim_matches([' ', '\t']).is_empty() && start == at {
        return refile::next_heading(text, at).unwrap_or(text.len());
    }
    at
}

// ----- the date tree

static YEAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([12][0-9]{3})").unwrap());
static MONTH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([12][0-9]{3}-[01][0-9]) \w+").unwrap());
static DAY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([12][0-9]{3}-[01][0-9]-[0123][0-9]) \w+").unwrap());

/// `org-datetree-comparefun-from-regex`
fn compare(pattern: &Regex, sibling: &str, new: &str) -> Option<std::cmp::Ordering> {
    let group = |text: &str| pattern.captures(text).map(|found| found[1].to_string());
    Some(group(sibling)?.cmp(&group(new)?))
}

/// `org-datetree--find-create-subheading`
fn find_create(buffer: &mut Buffer, pattern: &Regex, title: &str, level: usize, keywords: &TodoKeywords) {
    let mut sibling = None;
    let mut line = buffer.begv;
    while line < buffer.zv {
        let end = buffer.line_end(line);
        let text = &buffer.text[line..end];
        if headline_level(text) == Some(level) {
            let raw = complex_heading_title(text, level, keywords).map_or("", |(from, to)| &text[from..to]);
            if let Some(order) = compare(pattern, raw, title).filter(|order| order.is_ge()) {
                sibling = Some((line, order));
                break;
            }
        }
        if end >= buffer.zv {
            break;
        }
        line = end + 1;
    }
    match sibling {
        Some((at, _)) => buffer.point = at,
        None => {
            buffer.point = buffer.zv;
            if !buffer.bolp() {
                buffer.insert("\n");
            }
        }
    }
    if let Some((at, std::cmp::Ordering::Equal)) = sibling {
        buffer.narrow_to_subtree(at);
        return;
    }
    let from = buffer.skip_back(buffer.point, &[' ', '\t', '\n']);
    buffer.delete(from, buffer.point);
    if buffer.blank_before_heading() {
        buffer.insert("\n");
    }
    buffer.insert(&format!("\n{} {}\n", "*".repeat(level), title));
    let heading = buffer.line_start(buffer.point - 1);
    buffer.point = heading;
    buffer.narrow_to_subtree(heading);
}

/// The first headline from `from` whose property drawer has `name`
/// (`org-find-property`).
fn find_property(text: &str, from: usize, name: &str) -> Option<usize> {
    let wanted = format!(":{name}:");
    let mut at = text[..from].rfind('\n').map_or(0, |index| index + 1);
    while at < text.len() {
        let end = line_end(text, at);
        let line = text[at..end].trim_start_matches([' ', '\t']);
        let found = line.get(..wanted.len()).is_some_and(|key| key.eq_ignore_ascii_case(&wanted))
            && line[wanted.len()..].chars().next().is_none_or(|c| c == ' ' || c == '\t');
        if found && at >= from {
            if let Some(heading) = heading_at(text, at) {
                if in_property_drawer(text, heading, at) {
                    return Some(heading);
                }
            }
        }
        at = end + 1;
    }
    None
}

/// Whether the line at `line` is in the property drawer of the entry at
/// `heading`.
fn in_property_drawer(text: &str, heading: usize, line: usize) -> bool {
    let mut at = line_end(text, heading) + 1;
    if at < text.len() && is_planning(&text[at..line_end(text, at)]) {
        at = line_end(text, at) + 1;
    }
    if at >= text.len() || !text[at..line_end(text, at)].trim().eq_ignore_ascii_case(":PROPERTIES:") {
        return false;
    }
    while at < line {
        at = line_end(text, at) + 1;
        if text[at.min(text.len())..line_end(text, at.min(text.len()))].trim().eq_ignore_ascii_case(":END:") {
            return false;
        }
    }
    true
}

fn is_planning(line: &str) -> bool {
    let line = line.trim_start_matches([' ', '\t']);
    ["SCHEDULED:", "DEADLINE:", "CLOSED:"].iter().any(|keyword| line.starts_with(keyword))
}

/// `org-datetree-find-date-create`: the year, month and day headlines for
/// `date`, made where missing, and the buffer narrowed to the day.
fn datetree(buffer: &mut Buffer, date: Date, keywords: &TodoKeywords) {
    buffer.widen();
    let mut level = 1;
    if let Some(tree) = find_property(&buffer.text, buffer.point, "DATE_TREE") {
        buffer.point = tree;
        buffer.narrow_to_subtree(tree);
        level = buffer.level(tree).unwrap_or(0) + 1;
    }
    let day = date.to_day();
    let titles = [
        (&*YEAR, format!("{:04}", date.year)),
        (&*MONTH, format!("{:04}-{:02} {}", date.year, date.month, MONTH_NAMES[date.month as usize - 1])),
        (&*DAY, format!("{:04}-{:02}-{:02} {}", date.year, date.month, date.day, WEEKDAY_NAMES[weekday(day)])),
    ];
    for (pattern, title) in titles {
        find_create(buffer, pattern, &title, level, keywords);
        level += 1;
    }
}

/// `org-date-to-gregorian` of a timestamp's text.
fn timestamp_date(raw: &str) -> Option<Date> {
    let raw = raw.trim_start_matches(['[', '<']);
    let year = raw.get(..4)?.parse().ok()?;
    let month = raw.get(5..7)?.parse().ok()?;
    let day = raw.get(8..10)?.parse().ok()?;
    Some(Date::new(year, month, day))
}

// ----- org-archive-subtree

/// What the entry loses by moving, kept as `ARCHIVE_…` properties.
struct Saved {
    tags: Vec<String>,
    inherited: Vec<String>,
    values: Vec<(ContextInfo, String)>,
}

fn saved(text: &str, heading: usize, keywords: &TodoKeywords, context: &Context, time: &str) -> Saved {
    let parse = org_syntax::parse(text, keywords);
    let stem = context.path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
    let outline = Outline::new(&parse, text, &stem);
    let entry = outline.entries.iter().find(|entry| entry.offset == heading);
    let tags: Vec<String> =
        entry.map(|entry| entry.tags.iter().map(|tag| tag.name.clone()).collect()).unwrap_or_default();
    let inherited: Vec<String> = entry
        .map(|entry| entry.tags.iter().filter(|tag| tag.inherited).map(|tag| tag.name.clone()).collect())
        .unwrap_or_default();
    let local: Vec<String> = entry
        .map(|entry| entry.tags.iter().filter(|tag| !tag.inherited).map(|tag| tag.name.clone()).collect())
        .unwrap_or_default();
    let olid = entry
        .and_then(|entry| entry.parent)
        .and_then(|parent| outline.entries[parent].properties.iter().find(|(key, _)| key == "ID"))
        .map(|(_, value)| value.clone())
        .unwrap_or_default();
    let values = vec![
        (ContextInfo::Category, entry.map(|entry| entry.category.clone()).unwrap_or_default()),
        (ContextInfo::File, abbreviate(context.path, context.home)),
        (ContextInfo::Itags, inherited.join(" ")),
        (ContextInfo::Ltags, local.join(" ")),
        (ContextInfo::Olpath, outline_path(text, heading, keywords).join("/")),
        (ContextInfo::Olid, olid),
        (ContextInfo::Time, time.to_string()),
        (ContextInfo::Todo, entry.and_then(|entry| entry.todo.clone()).unwrap_or_default()),
    ];
    Saved { tags, inherited, values }
}

/// `org-archive-subtree`: move the subtree at byte `offset` of `source`
/// to its archive location. `read` gives the text of another file, or
/// none when it does not exist.
pub fn archive_subtree(
    source: &str,
    offset: usize,
    keywords: &TodoKeywords,
    context: &Context,
    settings: &Settings,
    read: &mut dyn FnMut(&Path) -> Option<String>,
) -> Result<Archived, Refusal> {
    let mut files = Files { read, files: Vec::new() };
    let mut source = source.to_string();
    let (message, cursor) = archive_one(&mut source, offset, keywords, context, settings, &mut files, &mut [])?;
    Ok(Archived { source, files: files.files, message, cursor })
}

fn archive_one(
    source: &mut String,
    offset: usize,
    keywords: &TodoKeywords,
    context: &Context,
    settings: &Settings,
    files: &mut Files,
    marks: &mut [usize],
) -> Result<(String, usize), Refusal> {
    let stamp = format_timestamp(context.now, true, true, "");
    let time = &stamp[1..stamp.len() - 1];
    let entry = heading_at(source, offset);
    let spec = location_spec(source, entry, keywords, &settings.location);
    let location = compute_location(&spec, context)?;
    let heading = entry.ok_or_else(|| Refusal::NotHere("Before first headline".to_string()))?;
    let same = location.file == context.path;
    let mut target = location.heading.clone();
    // `datetree/` is three levels of headlines, and more stars a headline
    // under the day.
    let mut date = None;
    let mut subheading = false;
    if let Some(rest) = target.strip_prefix("datetree/").map(String::from) {
        let stars = rest.bytes().take_while(|b| *b == b'*').count();
        target = format!("{}{}", "*".repeat(3 + stars), &rest[stars..]);
        subheading = stars > 0;
        let parse = org_syntax::parse(source, keywords);
        let outline = Outline::new(&parse, source, "");
        let closed =
            outline.entries.iter().find(|found| found.offset == heading).and_then(|found| found.closed.clone());
        date = Some(closed.and_then(|closed| timestamp_date(&closed.raw)).unwrap_or(context.now.date()));
    }
    let (target, level) = match target.bytes().take_while(|b| *b == b'*').count() {
        0 => (None, 0),
        stars => (Some(target), stars),
    };

    let saved = saved(source, heading, keywords, context, time);
    let tree = source[heading..subtree_end(source, heading)].to_string();

    // The archive buffer.
    let index = (!same).then(|| files.get(&location.file));
    let (text, new_file) = match index {
        Some(index) => (files.files[index].text.clone(), files.files[index].is_new()),
        None => (source.clone(), false),
    };
    let mut buffer = Buffer::new(text);
    if same {
        buffer.markers.push((offset, false));
        buffer.markers.extend(marks.iter().map(|mark| (*mark, false)));
        buffer.point = heading;
    }
    // `org-mode` in an empty buffer, as a file whose name does not say Org.
    if !same && buffer.text.is_empty() && !file_name(&location.file).ends_with(".org") {
        buffer.insert("#    -*- mode: org -*-\n\n");
    }
    if let (true, Some(header)) = (new_file, &settings.file_header_format) {
        buffer.point = buffer.zv;
        buffer.insert(&format_name(header, &context.path.display().to_string()));
    }
    if let Some(date) = date {
        datetree(&mut buffer, date, keywords);
    }
    match &target {
        Some(target) if date.is_none() || subheading => {
            static TAGS: &str = r"(?:[ \t]+:(?:[\w@#%]+:)+)?[ \t]*$";
            let pattern = Regex::new(&format!(r"(?mi)^{}{TAGS}", regex::escape(target))).expect("a pattern");
            match pattern.find(&buffer.text[buffer.begv..buffer.zv]) {
                Some(found) => buffer.point = buffer.begv + found.end(),
                None => {
                    buffer.point = buffer.zv;
                    if !buffer.bolp() {
                        buffer.insert("\n");
                    }
                    buffer.insert(&format!("{}{target}\n", if date.is_some() { "" } else { "\n" }));
                    buffer.point -= 1;
                }
            }
            if settings.reversed_order {
                let at = buffer.heading_before(buffer.point).unwrap_or(buffer.point);
                buffer.point = buffer.next_heading(at);
            } else {
                let at = buffer.heading_before(buffer.point).unwrap_or(buffer.point);
                buffer.point = subtree_end(&buffer.text, at).min(buffer.zv);
            }
            buffer.point = buffer.skip_back(buffer.point, &[' ', '\t', '\r', '\n']);
            let blanks = buffer.text[buffer.point..buffer.zv].len()
                - buffer.text[buffer.point..buffer.zv].trim_start_matches([' ', '\t', '\r', '\n']).len();
            buffer.delete(buffer.point, buffer.point + blanks);
            buffer.insert(if date.is_some() { "\n" } else { "\n\n" });
        }
        _ => {
            if settings.reversed_order {
                buffer.point = buffer.begv;
                if buffer.level(buffer.point).is_none() {
                    buffer.point = buffer.next_heading(buffer.point);
                }
            } else {
                buffer.point = buffer.zv;
                if !(date.is_some() && buffer.bolp()) {
                    buffer.insert("\n");
                }
            }
        }
    }
    let new_level = if target.is_some() { level + 1 } else { 1 };
    let pasted = buffer.paste(&tree, new_level, &settings.levels);
    let marker = buffer.markers.first().map(|(at, _)| *at);
    let before = buffer.text.clone();
    let mut text = before.clone();

    // Inherited tags, the done keyword and the context.
    let infile = same;
    let add_tags = match settings.add_inherited_tags {
        InheritedTags::Never => false,
        InheritedTags::InFile => infile,
        InheritedTags::Always => true,
    };
    if add_tags && !saved.inherited.is_empty() {
        set_tags(&mut text, pasted, &saved.tags, settings.levels.tags_column);
    }
    if settings.mark_done != MarkDone::No {
        let done: Vec<&str> = keywords.names().filter(|name| keywords.is_done(name)).collect();
        let line = &text[pasted..line_end(&text, pasted)];
        let current = keyword_of(line, keywords);
        if current.as_deref().is_none_or(|current| !keywords.is_done(current)) {
            let state = match &settings.mark_done {
                MarkDone::Keyword(word) if done.contains(&word.as_str()) => Some(word.clone()),
                _ => done.first().map(|word| word.to_string()),
            };
            if let Some(state) = state {
                let quiet = todo::Settings { log_done: None, ..settings.todo.clone() };
                text = todo::change_state(&text, keywords, pasted, &Target::State(Some(state)), &quiet, context.now)?;
            }
        }
    }
    let mut editor = Editor::new(&text, keywords, &settings.todo, context.now);
    editor.heading = pasted;
    for info in &settings.context_info {
        let value = saved.values.iter().find(|(kind, _)| kind == info).map_or("", |(_, value)| value.as_str());
        if has_text(value) {
            editor.put_property(&format!("ARCHIVE_{}", info.name().to_uppercase()), value);
        }
    }
    let text = editor.text;

    // Back in the subtree's file: cut it, and count again.
    let follow = |at: usize| if at <= pasted { at } else { track(&before, &text, at) };
    let cursor = match marker {
        Some(marker) => {
            for (mark, (moved, _)) in marks.iter_mut().zip(&buffer.markers[1..]) {
                *mark = follow(*moved);
            }
            let cursor = follow(marker);
            *source = text;
            cursor
        }
        None => {
            // Emacs saves it, with `require-final-newline`.
            let mut text = text;
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            files.files[index.expect("another file")].text = text;
            offset
        }
    };
    let cut = heading_at(source, cursor).unwrap_or(heading);
    let end = subtree_end(source, cut);
    source.replace_range(cut..end, "");
    for mark in marks.iter_mut() {
        if *mark >= end {
            *mark -= end - cut;
        } else if *mark > cut {
            *mark = cut;
        }
    }
    let cut_text = std::mem::take(source);
    *source = update_after_cut(cut_text.clone(), cut, keywords, &settings.todo, context.now);
    for mark in marks.iter_mut() {
        *mark = track(&cut_text, source, *mark);
    }
    let message = if same {
        format!("Subtree archived under heading: {}", target.unwrap_or_default())
    } else {
        format!("Subtree archived in file: {}", abbreviate(&location.file, context.home))
    };
    let cursor = settle(source, cut.min(source.len()));
    Ok((message, cursor))
}

/// The TODO keyword of a headline line.
fn keyword_of(line: &str, keywords: &TodoKeywords) -> Option<String> {
    let level = headline_level(line)?;
    let word = line[level..].trim_start_matches(' ').split([' ', '\t']).next()?;
    (keywords.contains(word) && line[level..].starts_with(' ')).then(|| word.to_string())
}

// ----- org-archive-to-archive-sibling

/// `org-archive-to-archive-sibling`: move the subtree at byte `offset`
/// under its `Archive` sibling, made at the end of the parent's subtree
/// when there is none. Returns the text and the cursor.
pub fn to_sibling(
    text: &str,
    offset: usize,
    keywords: &TodoKeywords,
    now: DateTime,
    settings: &Settings,
) -> Result<(String, usize), Refusal> {
    let heading = heading_at(text, offset).ok_or_else(|| Refusal::NotHere("Before first headline".to_string()))?;
    let level = headline_level(&text[heading..line_end(text, heading)]).unwrap_or(1);
    let leader = format!("{} ", "*".repeat(level));
    let mut buffer = Buffer::new(text.to_string());
    buffer.markers.push((heading, true));
    let (begin, end) = match up_heading(text, heading) {
        Some(up) if headline_level(&text[up..line_end(text, up)]).is_some() => (up, subtree_end(text, up)),
        _ => (0, text.len()),
    };
    let pattern = Regex::new(&format!(
        r"(?mi)^{}[ \t]*{}[ \t]*:{ARCHIVE_TAG}:",
        regex::escape(&leader),
        regex::escape(&settings.sibling_heading)
    ))
    .expect("a pattern");
    match pattern.find(&buffer.text[begin..end]) {
        Some(found) => buffer.point = begin + found.start(),
        None => {
            buffer.point = end;
            if !buffer.bolp() {
                buffer.insert("\n");
            }
            let line = buffer.point;
            buffer.insert(&format!("{leader}{}\n", settings.sibling_heading));
            let (_, edits) = toggle_archive_tag(&mut buffer.text, line, true, settings.levels.tags_column);
            for (marker, advances) in &mut buffer.markers {
                *marker = follow(&edits, *marker, *advances);
            }
            buffer.zv = buffer.text.len();
            buffer.point = line;
        }
    }
    let sibling = buffer.line_start(buffer.point);
    buffer.point =
        if settings.reversed_order { buffer.next_heading(sibling) } else { subtree_end(&buffer.text, sibling) };
    // Cut the subtree, where the marker has it now.
    let at = buffer.markers[0].0;
    let cut_end = subtree_end(&buffer.text, at);
    let tree = buffer.text[at..cut_end].to_string();
    buffer.delete(at, cut_end);
    let pasted = buffer.paste(&tree, level + 1, &settings.levels);
    let marker = buffer.markers[0].0;
    let stamp = format_timestamp(now, true, true, "");
    let mut editor = Editor::new(&buffer.text, keywords, &settings.todo, now);
    editor.heading = pasted;
    let before = buffer.text.clone();
    editor.put_property("ARCHIVE_TIME", &stamp[1..stamp.len() - 1]);
    let marker = if marker <= pasted { marker } else { track(&before, &editor.text, marker) };
    // The parent's TODO statistics, from the archive sibling.
    let sibling = parent(&editor.text, editor.heading).unwrap_or(editor.heading);
    let before = editor.text.clone();
    editor.heading = sibling;
    editor.update_parent_statistics();
    let marker = track(&before, &editor.text, marker);
    let text = editor.text;
    let cursor = settle(&text, marker.min(text.len()));
    Ok((text, cursor))
}

// ----- org-toggle-archive-tag

/// `org-toggle-archive-tag`: the `ARCHIVE` tag of the headline at byte
/// `offset` set or removed. Returns the text and what Emacs says.
pub fn toggle_tag(text: &str, offset: usize, settings: &Settings) -> Result<(String, String), Refusal> {
    let heading = heading_at(text, offset).ok_or_else(|| Refusal::NotHere("Before first headline".to_string()))?;
    let mut text = text.to_string();
    let (set, _) = toggle_archive_tag(&mut text, heading, false, settings.levels.tags_column);
    Ok((text, format!("Subtree {}", if set { "archived" } else { "unarchived" })))
}

// ----- org-archive-all-done

/// `org-archive-all-done`, after `C-u`: each subtree below the headline
/// at byte `offset` (or each top-level one before the first headline)
/// without open TODO items is archived, or tagged with `tag`, when the
/// answer to its question is yes. Asks when `answers` run out.
#[allow(clippy::too_many_arguments)]
pub fn archive_all(
    source: &str,
    offset: usize,
    keywords: &TodoKeywords,
    context: &Context,
    settings: &Settings,
    tag: bool,
    answers: &[bool],
    read: &mut dyn FnMut(&Path) -> Option<String>,
) -> Result<AllOutcome, Refusal> {
    let mut files = Files { read, files: Vec::new() };
    let mut text = source.to_string();
    let not_done: Vec<&str> = keywords.names().filter(|name| !keywords.is_done(name)).collect();
    let start = text[..offset.min(text.len())].rfind('\n').map_or(0, |index| index + 1);
    let (stars, mut begin, mut end) = match headline_level(&text[start..line_end(&text, start)]) {
        Some(level) => (level + 1, offset, end_of_subtree(&text, start)),
        None => (1, 0, text.len()),
    };
    let question = if tag { "Set ARCHIVE tag? " } else { "Move subtree to archive? " };
    let mut answers = answers.iter();
    let mut count = 0;
    let mut cursor = offset;
    loop {
        if begin > end {
            return Err(Refusal::Failed("Invalid search bound (wrong side of point)".to_string()));
        }
        // `re-search-forward` for a headline of the level, up to the end.
        let found = {
            let mut line = text[..begin].rfind('\n').map_or(0, |index| index + 1);
            if line < begin {
                line = line_end(&text, begin) + 1;
            }
            let mut found = None;
            while line < end.min(text.len()) {
                let line_text = &text[line..line_end(&text, line)];
                if line_text.len() > stars
                    && line_text.as_bytes()[..stars].iter().all(|b| *b == b'*')
                    && line_text.as_bytes()[stars] == b' '
                    && line + stars < end
                {
                    found = Some(line);
                    break;
                }
                line = line_end(&text, line) + 1;
            }
            found
        };
        let Some(tree) = found else { break };
        let tree_end = end_of_subtree(&text, tree);
        let open = text[tree..tree_end].split('\n').any(|line| {
            headline_level(line).is_some_and(|level| {
                let rest = &line[level..];
                let word = rest.trim_start_matches(' ');
                rest.starts_with(' ')
                    && not_done.iter().any(|keyword| {
                        word.strip_prefix(keyword).is_some_and(|after| {
                            after.is_empty() || after.starts_with(' ') || after.trim_matches([' ', '\t']).is_empty()
                        })
                    })
            })
        });
        if open {
            begin = tree_end;
            continue;
        }
        let tagged = text[tree..line_end(&text, tree)].contains(&format!(":{ARCHIVE_TAG}:"));
        if tag && tagged {
            begin = tree_end;
            continue;
        }
        let Some(&yes) = answers.next() else {
            return Ok(AllOutcome::Ask(format!("{question}(no open TODO items)")));
        };
        if !yes {
            begin = tree_end;
            continue;
        }
        count += 1;
        if tag {
            let (_, edits) = toggle_archive_tag(&mut text, tree, true, settings.levels.tags_column);
            begin = tree;
            end = follow(&edits, end, false);
            cursor = follow(&edits, cursor, false);
        } else {
            let mut marks = [end, cursor];
            let (_, at) = archive_one(&mut text, tree, keywords, context, settings, &mut files, &mut marks)?;
            begin = at;
            [end, cursor] = marks;
        }
    }
    Ok(AllOutcome::Done(Archived {
        source: text,
        files: files.files,
        message: format!("{count} trees archived"),
        cursor,
    }))
}

/// `org-end-of-subtree` without going to the next headline: the end of
/// the last line that is not blank.
fn end_of_subtree(text: &str, heading: usize) -> usize {
    let end = subtree_end(text, heading);
    if end > 0 && text.as_bytes()[end - 1] == b'\n' {
        heading + text[heading..end - 1].trim_end_matches(['\n', '\r', '\t', ' ']).len()
    } else {
        end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_locations() {
        let context = Context {
            path: Path::new("/home/u/notes/a.org"),
            home: Some(Path::new("/home/u")),
            now: DateTime::new(Date::new(2026, 5, 18), 10, 30),
        };
        let found = compute_location("%s_archive::", &context).unwrap();
        assert_eq!(found, Location { file: PathBuf::from("/home/u/notes/a.org_archive"), heading: String::new() });
        let found = compute_location("~/old/%s::* From %s", &context).unwrap();
        assert_eq!(found, Location { file: PathBuf::from("/home/u/old/a.org"), heading: "* From a.org".to_string() });
        let found = compute_location("../x.org::", &context).unwrap();
        assert_eq!(found.file, PathBuf::from("/home/u/x.org"));
        assert!(compute_location("bad", &context).is_err());
        assert_eq!(abbreviate(Path::new("/home/u/notes/a.org"), context.home), "~/notes/a.org");
    }

    #[test]
    fn lists_the_archive_files() {
        let context = Context {
            path: Path::new("/home/u/notes/a.org"),
            home: Some(Path::new("/home/u")),
            now: DateTime::new(Date::new(2026, 5, 18), 10, 30),
        };
        let keywords = TodoKeywords::default();
        let files = |text: &str| archive_files(text, &keywords, &context, "%s_archive::");
        assert_eq!(files("* a\n"), [PathBuf::from("/home/u/notes/a.org_archive")]);
        let text = "#+ARCHIVE: old.org::\n* a\n:PROPERTIES:\n:ARCHIVE: ~/x.org::* X\n:END:\n* b\n:PROPERTIES:\n:ARCHIVE: old.org::\n:END:\n";
        assert_eq!(files(text), [PathBuf::from("/home/u/notes/old.org"), PathBuf::from("/home/u/x.org")]);
    }

    #[test]
    fn toggles_the_tag() {
        let (text, message) = toggle_tag("* A :x:\n", 0, &Settings::default()).unwrap();
        assert_eq!(message, "Subtree archived");
        assert!(text.starts_with("* A") && text.trim_end().ends_with(":x:ARCHIVE:"));
        let (text, message) = toggle_tag(&text, 0, &Settings::default()).unwrap();
        assert_eq!(message, "Subtree unarchived");
        assert!(text.trim_end().ends_with(":x:"));
    }
}
