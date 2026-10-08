//! Refiling: a port of `org-refile`. [`targets`] lists the headlines
//! `org-refile-targets` selects in a file, named as `org-refile-get-location`
//! offers them, and [`refile`] moves a subtree under one of them, as
//! `org-copy-subtree`, `org-paste-subtree` and the deletion after it do.

use std::sync::LazyLock;

use org_syntax::TodoKeywords;
use regex::Regex;

use crate::indent::{self, Adapt};
use crate::outline::complex_heading_title;
use crate::todo::{headline_level, tags_padding};
use crate::Refusal;

/// `org-link-bracket-re`: a link, its target and its description.
static LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[((?:[^\[\]\\]|\\(?:\\\\)*[\[\]]|\\+[^\[\]])+)\](?:\[(.+?)\])?\]").unwrap());

/// Which headlines of a file are targets: the second half of an entry of
/// `org-refile-targets`.
#[derive(Debug, Clone)]
pub enum Select {
    /// `t`: every headline.
    All,
    /// `(:level . N)`
    Level(usize),
    /// `(:maxlevel . N)`
    MaxLevel(usize),
    /// `(:tag . "TAG")`: headlines that carry the tag themselves.
    Tag(String),
    /// `(:todo . "KEYWORD")`
    Todo(String),
    /// `(:regexp . "RE")`, in the syntax of the `regex` crate rather than
    /// Emacs's: headlines with a match on their line.
    Regexp(Regex),
}

impl Select {
    /// `(:regexp . RE)`, or none when `pattern` is no valid expression.
    pub fn regexp(pattern: &str) -> Option<Select> {
        Regex::new(pattern).ok().map(Select::Regexp)
    }

    /// Whether `line` is a target (`descre` in `org-refile-get-targets`).
    fn matches(&self, line: &str) -> bool {
        let stars = line.bytes().take_while(|b| *b == b'*').count();
        let blank = |at: usize| matches!(line.as_bytes().get(at), Some(b' ' | b'\t'));
        if stars == 0 {
            return false;
        }
        match self {
            Select::All => blank(stars),
            Select::Level(level) => stars == *level && blank(stars),
            Select::MaxLevel(level) => stars <= *level && blank(stars),
            Select::Tag(tag) => blank(stars) && line[stars + 1..].contains(&format!(":{tag}:")),
            Select::Todo(keyword) => {
                let rest = line[stars..].trim_start_matches([' ', '\t']);
                blank(stars) && rest.strip_prefix(keyword.as_str()).is_some_and(|after| after.starts_with([' ', '\t']))
            }
            Select::Regexp(regexp) => regexp.is_match(line),
        }
    }
}

/// `org-refile-use-outline-path`: whether targets are named by their
/// outline path, and what the path starts with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutlinePath {
    /// `nil`: by the headline alone.
    #[default]
    No,
    /// `t`
    Yes,
    /// `file`: the file's name first; the file itself is a target too.
    File,
    /// `full-file-path`
    FullFilePath,
    /// `title`: the file's `#+TITLE:`, or else its name.
    Title,
}

/// A file targets are taken from.
#[derive(Clone, Copy)]
pub struct File<'a> {
    pub text: &'a str,
    pub keywords: &'a TodoKeywords,
    /// The file's name, without directories.
    pub name: &'a str,
    /// Its full path.
    pub path: &'a str,
    /// Its `#+TITLE:`.
    pub title: Option<&'a str>,
}

/// The title of the headline at or above byte `offset`, its links shown
/// by their description, as the prompt of `org-refile` names the subtree.
pub fn heading_text(text: &str, offset: usize, keywords: &TodoKeywords) -> String {
    let Some(heading) = heading_at(text, offset) else { return String::new() };
    let line = line_at(text, heading);
    let level = headline_level(line).unwrap_or(1);
    let Some((from, to)) = complex_heading_title(line, level, keywords) else { return String::new() };
    // `(replace-regexp-in-string org-link-bracket-re "\\2" …)`
    LINK.replace_all(&line[from..to], |captures: &regex::Captures| {
        captures.get(2).map_or(String::new(), |found| found.as_str().to_string())
    })
    .into_owned()
}

/// The file's title (`org-get-title`): its `#+TITLE:` lines, joined.
pub fn file_title(parse: &org_syntax::Parse) -> Option<String> {
    let titles = crate::outline::keyword_values(&parse.syntax(), "TITLE");
    (!titles.is_empty()).then(|| titles.join(" "))
}

/// A place to refile to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// The name `org-refile-get-location` offers it under.
    pub name: String,
    /// Its name in `org-refile-target-table`: without the file's name a
    /// target from another file gets in the offer.
    pub raw: String,
    /// The byte offset of its headline, or none for the file itself, where
    /// entries go at the top level.
    pub offset: Option<usize>,
}

/// The targets `select` finds in `file`, in the order of the file, named
/// as offered when refiling from `current` (which is not this file when
/// it says no, and is no file at all in the agenda).
pub fn targets(file: &File, select: &Select, outline_path: OutlinePath, current: bool) -> Vec<Target> {
    let base = match outline_path {
        OutlinePath::No | OutlinePath::Yes => None,
        OutlinePath::File => Some(file.name),
        OutlinePath::FullFilePath => Some(file.path),
        OutlinePath::Title => Some(file.title.unwrap_or(file.name)),
    };
    // `org-refile-get-location`: a `/` after a path, and the file's name
    // after a target from another file, unless the path starts with it.
    let extra = if outline_path == OutlinePath::No { "" } else { "/" };
    let name = |path: &str| {
        if base.is_none() && !current {
            format!("{path}{extra} ({})", file.name)
        } else {
            format!("{path}{extra}")
        }
    };
    let mut out = Vec::new();
    if let Some(base) = base {
        out.push(Target { name: name(base), raw: base.to_string(), offset: None });
    }
    let text = file.text;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        let line = line.strip_suffix('\n').unwrap_or(line);
        let Some(level) = headline_level(line) else { continue };
        if !select.matches(line) {
            continue;
        }
        let Some((from, to)) = complex_heading_title(line, level, file.keywords) else { continue };
        let path = if outline_path == OutlinePath::No {
            link_display_format(&line[from..to])
        } else {
            let mut parts: Vec<String> =
                ancestors(text, start).into_iter().map(|at| path_part(text, at, file)).collect();
            parts.push(path_part(text, start, file));
            let parts = parts.iter().map(|part| part.replace('/', "\\/"));
            base.map(str::to_string).into_iter().chain(parts).collect::<Vec<_>>().join("/")
        };
        out.push(Target { name: name(&path), raw: path, offset: Some(start) });
    }
    out
}

/// The targets of every selection in turn, each in the files it names
/// (indexes into `files`), without repeats (`org-refile-get-targets`).
/// `current` is the file refiled from, if it is one of them.
pub fn all_targets(
    selections: &[(Select, Vec<usize>)],
    files: &[File],
    current: Option<usize>,
    outline_path: OutlinePath,
) -> Vec<(usize, Target)> {
    let mut out: Vec<(usize, Target)> = Vec::new();
    for (select, indexes) in selections {
        for &index in indexes {
            for target in targets(&files[index], select, outline_path, current == Some(index)) {
                let found = (index, target);
                if !out.contains(&found) {
                    out.push(found);
                }
            }
        }
    }
    out
}

/// The names to offer, each once.
pub fn names(targets: &[(usize, Target)]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (_, target) in targets {
        if !out.contains(&target.name) {
            out.push(target.name.clone());
        }
    }
    out
}

/// The target `answer` names, with or without a `/` at its end
/// (`org-refile--get-location`); the first of that name.
pub fn find<'a>(targets: &'a [(usize, Target)], answer: &str) -> Option<&'a (usize, Target)> {
    let without = answer.strip_suffix('/').unwrap_or(answer).to_string();
    let with = if answer.ends_with('/') { answer.to_string() } else { format!("{answer}/") };
    // Each spelling is looked up among the names offered, then among the
    // targets as `org-refile-target-table` has them.
    [without, with].into_iter().find_map(|name| {
        targets
            .iter()
            .find(|(_, target)| target.name == name)
            .or_else(|| targets.iter().find(|(_, target)| target.raw == name))
    })
}

/// `org-get-outline-path`: the titles of the headlines above the one at
/// `offset`, outermost first.
pub(crate) fn outline_path(text: &str, offset: usize, keywords: &TodoKeywords) -> Vec<String> {
    let file = File { text, keywords, name: "", path: "", title: None };
    ancestors(text, offset).into_iter().map(|at| path_part(text, at, &file)).collect()
}

/// A headline's title as a part of an outline path, from its level and
/// the text after its stars.
pub(crate) fn heading_path_part(level: usize, head: &str, keywords: &TodoKeywords) -> String {
    let line = format!("{} {head}", "*".repeat(level));
    let file = File { text: &line, keywords, name: "", path: "", title: None };
    path_part(&line, 0, &file)
}

/// The headlines above the one at `offset`, outermost first
/// (`org-up-heading-safe`).
fn ancestors(text: &str, offset: usize) -> Vec<usize> {
    let mut level = headline_level(line_at(text, offset)).unwrap_or(1);
    let mut out = Vec::new();
    let mut at = offset;
    while level > 1 && at > 0 {
        at = text[..at - 1].rfind('\n').map_or(0, |index| index + 1);
        if let Some(found) = headline_level(line_at(text, at)).filter(|found| *found < level) {
            out.push(at);
            level = found;
        }
    }
    out.reverse();
    out
}

fn line_at(text: &str, offset: usize) -> &str {
    text[offset..].split('\n').next().unwrap_or("")
}

/// A headline as a part of an outline path (`org--get-outline-path-1`):
/// its title without statistics cookies, links shown by their
/// description, and without `COMMENT`.
fn path_part(text: &str, offset: usize, file: &File) -> String {
    static COOKIE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[[0-9]+%\]|\[[0-9]+/[0-9]+\]").unwrap());
    static COMMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^COMMENT[ \t]*").unwrap());
    let line = line_at(text, offset);
    let Some(level) = headline_level(line) else { return String::new() };
    let Some((from, to)) = complex_heading_title(line, level, file.keywords) else { return String::new() };
    let title = link_display_format(&COOKIE.replace_all(&line[from..to], ""));
    let title = title.trim_matches([' ', '\t', '\n', '\r']);
    // `:commentedp`: the title, after any keyword and priority, starts
    // with `COMMENT` and a space or the end of the line.
    let commented = line[from..].strip_prefix("COMMENT").is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
        || line[from..to] == *"COMMENT";
    if commented {
        COMMENT.replace(title, "").into_owned()
    } else {
        title.to_string()
    }
}

/// `org-link-display-format`: links replaced by their description, or by
/// their target when they have none.
pub(crate) fn link_display_format(text: &str) -> String {
    LINK.replace_all(text, |captures: &regex::Captures| {
        captures.get(2).or(captures.get(1)).map_or(String::new(), |found| found.as_str().to_string())
    })
    .into_owned()
}

/// How a subtree is filed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// `org-reverse-note-order`: as the first child rather than the last.
    pub reverse_note_order: bool,
    /// `org-tags-column`, for the tags of headlines whose level changes.
    pub tags_column: i64,
    /// `org-adapt-indentation`: the text of entries whose level changes
    /// moves with their headline (`org-fixup-indentation`).
    pub adapt: Adapt,
    /// `org-src-preserve-indentation`
    pub src_preserve_indentation: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { reverse_note_order: false, tags_column: -77, adapt: Adapt::Off, src_preserve_indentation: false }
    }
}

/// The texts after a refile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refiled {
    /// The file the subtree came from.
    pub source: String,
    /// The file it went to, when that is another file.
    pub target: Option<String>,
    /// Where its headline is now, in the file it went to.
    pub pasted: Option<usize>,
}

/// Move the subtree at byte `offset` of `source` under the headline at
/// byte `to` of `target` (`None`: the same file), or to the top level of
/// that file when `to` is none. With `keep` (`org-refile-keep`, as
/// `org-refile-copy` has it) the subtree stays where it was too.
pub fn refile(
    source: &str,
    offset: usize,
    target: Option<&str>,
    to: Option<usize>,
    settings: &Settings,
    keep: bool,
) -> Result<Refiled, Refusal> {
    // `org-back-to-heading`, and the subtree up to the next headline of the
    // same or a higher level, blank lines included.
    let heading = heading_at(source, offset).ok_or_else(|| Refusal::NotHere("Before first headline".to_string()))?;
    let end = subtree_end(source, heading);
    if target.is_none() && to.is_some_and(|to| (heading..end).contains(&to)) {
        return Err(Refusal::Failed("Cannot refile to position inside the tree or region".to_string()));
    }
    let tree = &source[heading..end];
    let into = target.unwrap_or(source);
    // Where the subtree goes, and at which level.
    let (at, level) = match to {
        Some(to) => {
            let level = headline_level(line_at(into, to)).unwrap_or(1) + 1;
            let at = if settings.reverse_note_order {
                next_heading(into, to).unwrap_or(into.len())
            } else {
                // After the target's subtree: before its next sibling, or
                // where its parent's next child or anything higher starts.
                subtree_end(into, to)
            };
            (at, level)
        }
        None if settings.reverse_note_order => {
            // `outline-next-heading` from the start of the file, which skips
            // a headline on the first line.
            (next_heading(into, 0).unwrap_or(into.len()), 1)
        }
        None => (into.len(), 1),
    };
    let mut pasted = String::new();
    // `(unless (bolp) (newline))`
    if at > 0 && !into[..at].ends_with('\n') {
        pasted.push('\n');
    }
    pasted.push_str(&shift_levels(tree, level, settings, true));
    if !pasted.ends_with('\n') {
        pasted.push('\n');
    }
    let mut changed = into.to_string();
    changed.insert_str(at, &pasted);
    let pasted_at = at + usize::from(pasted.starts_with('\n'));
    if target.is_some() {
        let mut source = source.to_string();
        if !keep {
            source.replace_range(heading..end, "");
        }
        return Ok(Refiled { source, target: Some(changed), pasted: Some(pasted_at) });
    }
    if keep {
        return Ok(Refiled { source: changed, target: None, pasted: Some(pasted_at) });
    }
    // The same file: Emacs goes back to where the cursor was, which moves
    // with text inserted there or before, and deletes the subtree of the
    // headline it finds from there, up to the end of that subtree now. That
    // may take a newline added for the paste along, and when the paste went
    // right where the cursor was, it is the pasted copy that goes.
    let cursor = offset.min(source.len());
    let origin = if at <= cursor { cursor + pasted.len() } else { cursor };
    let heading = heading_at(&changed, origin).unwrap_or(heading);
    let end = subtree_end(&changed, heading);
    changed.replace_range(heading..end, "");
    // The copy pasted, where it is once the subtree is gone, unless it was
    // the copy that went.
    let pasted = match pasted_at {
        at if at >= end => Some(at - (end - heading)),
        at if at < heading => Some(at),
        _ => None,
    };
    Ok(Refiled { source: changed, target: None, pasted })
}

/// The headline at or above byte `offset`.
pub(crate) fn heading_at(text: &str, offset: usize) -> Option<usize> {
    let mut at = text[..offset.min(text.len())].rfind('\n').map_or(0, |index| index + 1);
    loop {
        if headline_level(line_at(text, at)).is_some() {
            return Some(at);
        }
        if at == 0 {
            return None;
        }
        at = text[..at - 1].rfind('\n').map_or(0, |index| index + 1);
    }
}

/// The start of the first headline after the line at `offset`
/// (`outline-next-heading`).
pub(crate) fn next_heading(text: &str, offset: usize) -> Option<usize> {
    let mut at = offset;
    loop {
        at += text[at..].find('\n')? + 1;
        if headline_level(line_at(text, at)).is_some() {
            return Some(at);
        }
    }
}

/// `org-refile-allow-creating-parent-nodes`: whether an answer that names
/// no target makes one, a new child of the target named by what comes
/// before its last `/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NewNodes {
    #[default]
    No,
    Yes,
    /// `confirm`: once the user says yes.
    Confirm,
}

/// When no target is named `answer` (`org-refile-allow-creating-parent-nodes`):
/// the target named by what comes before its last `/`, and the name after
/// it, of a new child to create there.
pub fn new_node<'a>(targets: &'a [(usize, Target)], answer: &str) -> Option<(&'a (usize, Target), String)> {
    let (parent, child) = answer.rsplit_once('/')?;
    if child.is_empty() || child.contains('\n') || parent.contains('\n') {
        return None;
    }
    find(targets, parent).map(|found| (found, child.to_string()))
}

/// `org-refile-new-child`: a headline `child` at the end of the subtree of
/// the headline at `parent`, a level below it, or at the end of the file
/// at the top level; before the blank lines there, one blank line above
/// it. Returns the text and where the new headline is.
pub fn new_child(text: &str, parent: Option<usize>, child: &str) -> (String, usize) {
    let mut text = text.to_string();
    let (end, level) = match parent {
        Some(heading) => (subtree_end(&text, heading), headline_level(line_at(&text, heading)).unwrap_or(1) + 1),
        None => {
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            (text.len(), 1)
        }
    };
    // `org-back-over-empty-lines`: back over the blanks, then to the start
    // of the line after the last that is not blank.
    let content = text[..end].trim_end_matches([' ', '\t', '\n', '\r']).len();
    let at = text[content..].find('\n').map_or(text.len(), |newline| content + newline + 1).min(end);
    text.insert_str(at, &format!("\n{} {child}\n", "*".repeat(level)));
    (text, at + 1)
}

/// The entry whose headline is at `heading`, without its children
/// (`org-copy-subtree` with `nosubtrees`), at the top level as
/// `org-paste-subtree` puts it there: for `org-agenda-write` to an Org
/// file.
pub fn entry_at_top(text: &str, heading: usize, settings: &Settings) -> String {
    let end = next_heading(text, heading).unwrap_or(text.len());
    let mut entry = text[heading..end].to_string();
    if !entry.ends_with('\n') {
        entry.push('\n');
    }
    shift_levels(&entry, 1, settings, false)
}

/// Where the subtree of the headline at `heading` ends: at the next
/// headline of the same or a higher level, or at the end of the text.
pub(crate) fn subtree_end(text: &str, heading: usize) -> usize {
    let level = headline_level(line_at(text, heading)).unwrap_or(1);
    let mut at = heading;
    while let Some(next) = next_heading(text, at) {
        if headline_level(line_at(text, next)).is_some_and(|found| found <= level) {
            return next;
        }
        at = next;
    }
    text.len()
}

/// The subtree `tree` with its first headline at `level` and every
/// headline in it moved by as many levels, their tags aligned again
/// (`org-paste-subtree`, `org-demote`, `org-promote`). With
/// `org-adapt-indentation`, each entry's text moves too, a level at a time
/// as `org-demote` does it. With `align_top`, the first headline's tags are
/// aligned even when nothing moves, as `org-refile` does after pasting
/// (`org-align-tags`).
pub(crate) fn shift_levels(tree: &str, level: usize, settings: &Settings, align_top: bool) -> String {
    let first = headline_level(tree).unwrap_or(level);
    let shift = level as i64 - first as i64;
    let mut tree = tree.to_string();
    if settings.adapt != Adapt::Off && shift != 0 {
        let indent = indent::Settings {
            adapt: settings.adapt,
            src_preserve_indentation: settings.src_preserve_indentation,
            ..indent::Settings::default()
        };
        let step: isize = if shift > 0 { 1 } else { -1 };
        for _ in 0..shift.unsigned_abs() {
            tree = entries(&tree)
                .into_iter()
                .map(|entry| {
                    let Some(stars) = headline_level(entry) else { return entry.to_string() };
                    if step < 0 && stars == 1 {
                        return entry.to_string();
                    }
                    let moved = format!("{}{}", "*".repeat((stars as isize + step) as usize), &entry[stars..]);
                    indent::fixup_entry(&moved, step, &indent)
                })
                .collect();
        }
    }
    let mut out = String::with_capacity(tree.len() + 8);
    let mut top = true;
    for line in tree.split_inclusive('\n') {
        let (body, newline) = line.strip_suffix('\n').map_or((line, ""), |body| (body, "\n"));
        let Some(stars) = headline_level(body) else {
            out.push_str(line);
            continue;
        };
        if shift == 0 && !(top && align_top) {
            out.push_str(line);
            continue;
        }
        top = false;
        let stars = if settings.adapt != Adapt::Off { stars } else { (stars as i64 + shift).max(1) as usize };
        let mut changed = format!("{}{}", "*".repeat(stars), &body[headline_level(body).unwrap_or(0)..]);
        if let Some((blanks, tags, padding)) = tags_padding(&changed, settings.tags_column) {
            changed.replace_range(blanks..tags, &padding);
        }
        out.push_str(&changed);
        out.push_str(newline);
    }
    out
}

/// `text` cut before each headline.
fn entries(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        if at > start && headline_level(line.trim_end_matches('\n')).is_some() {
            out.push(&text[start..at]);
            start = at;
        }
        at += line.len();
    }
    out.push(&text[start..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_display() {
        assert_eq!(link_display_format("a [[https://x.org][Site]] [[file:y]]"), "a Site file:y");
    }

    #[test]
    fn subtree_ends() {
        let text = "* A\n** B\n\n* C\n";
        assert_eq!(subtree_end(text, 0), 10);
        assert_eq!(subtree_end(text, 4), 10);
        assert_eq!(next_heading(text, 0), Some(4));
        assert_eq!(heading_at(text, 9), Some(4));
    }
}
