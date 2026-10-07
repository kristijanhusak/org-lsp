//! Renaming a tag or a headline everywhere it is used. Org has no such
//! command; this is what renaming means for the server's
//! `textDocument/rename`: a tag in every headline that has it and in the
//! `#+FILETAGS:` and `#+TAGS:` lines, the tags aligned again; a headline's
//! title, and the links that search for it (`[[*Title]]`,
//! `[[file:notes.org::*Title]]`).

use org_syntax::TodoKeywords;

use crate::outline::complex_heading_title;
use crate::tags::{self, Settings as TagSettings};
use crate::todo::headline_level;

/// What renaming at a place renames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A tag, at `start..end`.
    Tag { start: usize, end: usize, name: String },
    /// The title of the headline at `heading`, at `start..end`.
    Heading { heading: usize, start: usize, end: usize, title: String },
}

fn line_bounds(text: &str, at: usize) -> (usize, usize) {
    let start = text[..at.min(text.len())].rfind('\n').map_or(0, |found| found + 1);
    let end = text[start..].find('\n').map_or(text.len(), |found| start + found);
    (start, end)
}

fn is_tag_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '@' | '#' | '%')
}

/// Whether `name` may be a tag.
pub fn valid_tag(name: &str) -> bool {
    !name.is_empty() && name.chars().all(is_tag_char)
}

/// The tag of `group` (`:a:b:`, starting at `offset`) the cursor at `at`
/// is on.
fn tag_in(group: &str, offset: usize, at: usize) -> Option<Target> {
    let mut start = offset;
    for part in group.split(':') {
        let end = start + part.len();
        if !part.is_empty() && start <= at && at <= end {
            return Some(Target::Tag { start, end, name: part.to_string() });
        }
        start = end + 1;
    }
    None
}

/// What renaming at `at` renames: the tag under the cursor in a headline
/// or a `#+FILETAGS:` line, else the title of the headline the cursor is
/// on.
pub fn target(text: &str, at: usize, keywords: &TodoKeywords) -> Option<Target> {
    let (start, end) = line_bounds(text, at);
    let line = &text[start..end];
    if let Some(level) = headline_level(line) {
        if let Some((_, tags_start)) = crate::text::tag_group(line) {
            if at >= start + tags_start {
                let group = line[tags_start..].trim_end_matches([' ', '\t']);
                return tag_in(group, start + tags_start, at);
            }
        }
        let (from, to) = complex_heading_title(line, level, keywords)?;
        let title = &line[from..to];
        return Some(Target::Heading {
            heading: start,
            start: start + from,
            end: start + to,
            title: title.to_string(),
        });
    }
    let upper = line.trim_start().to_ascii_uppercase();
    if upper.starts_with("#+FILETAGS:") {
        let value_start = start + line.find(':').map_or(0, |colon| colon + 1);
        let value = &text[value_start..end];
        // Tags separated by colons or blanks.
        let mut from = value_start;
        for part in value.split([':', ' ', '\t']) {
            let to = from + part.len();
            if !part.is_empty() && from <= at && at <= to {
                return Some(Target::Tag { start: from, end: to, name: part.to_string() });
            }
            from = to + 1;
        }
    }
    None
}

/// `text` with every whole word `old` of a `#+TAGS:` or `#+FILETAGS:`
/// line's value written `new`.
fn rename_in_keyword_value(value: &str, old: &str, new: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(found) = rest.find(old) {
        let before = rest[..found].chars().next_back();
        let after = rest[found + old.len()..].chars().next();
        out.push_str(&rest[..found]);
        if before.is_none_or(|c| !is_tag_char(c)) && after.is_none_or(|c| !is_tag_char(c)) {
            out.push_str(new);
        } else {
            out.push_str(old);
        }
        rest = &rest[found + old.len()..];
    }
    out.push_str(rest);
    out
}

/// `text` with the tag `old` named `new` in every headline (aligned again,
/// as `org-set-tags` does) and in its `#+FILETAGS:` and `#+TAGS:` lines.
/// A headline that has both keeps one.
pub fn rename_tag(text: &str, old: &str, new: &str, settings: &TagSettings) -> String {
    let settings = TagSettings { sort: None, ..settings.clone() };
    let mut out = text.to_string();
    let mut start = 0;
    while start < out.len() {
        let end = out[start..].find('\n').map_or(out.len(), |found| start + found);
        let line = out[start..end].to_string();
        let mut next_end = end;
        if headline_level(&line).is_some() {
            let current = tags::local_tags(&line);
            if current.iter().any(|tag| tag == old) {
                let mut renamed: Vec<String> = Vec::new();
                for tag in current {
                    let tag = if tag == old { new.to_string() } else { tag };
                    if !renamed.contains(&tag) {
                        renamed.push(tag);
                    }
                }
                out = tags::set_tags(&out, start, &renamed, &settings);
                next_end = out[start..].find('\n').map_or(out.len(), |found| start + found);
            }
        } else {
            let upper = line.trim_start().to_ascii_uppercase();
            if upper.starts_with("#+FILETAGS:") || upper.starts_with("#+TAGS:") {
                let colon = line.find(':').map_or(0, |colon| colon + 1);
                let value = rename_in_keyword_value(&line[colon..], old, new);
                let replaced = format!("{}{value}", &line[..colon]);
                out.replace_range(start..end, &replaced);
                next_end = start + replaced.len();
            }
        }
        start = next_end + 1;
    }
    out
}

/// The search of a link to a headline titled `title`, renamed: `*Old` or
/// a fuzzy `Old` becomes the new title; `#custom-id` stays as it is.
/// `raw` is the link as Org reads it.
pub fn retarget(raw: &str, title: &str) -> Option<String> {
    let (prefix, search) = match raw.find("::") {
        Some(split)
            if raw.starts_with("file:") || raw.starts_with('/') || raw.starts_with('~') || raw.starts_with('.') =>
        {
            (&raw[..split + 2], &raw[split + 2..])
        }
        _ => ("", raw),
    };
    if search.starts_with('#') || search.starts_with('(') || search.starts_with('/') {
        return None;
    }
    let new = if search.starts_with('*') {
        crate::link::heading_search_string(title)
    } else {
        crate::link::normalize_string(title, false)
    };
    Some(format!("{prefix}{new}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renames_tags_everywhere() {
        let text = "#+FILETAGS: :work:home:\n#+TAGS: work(w) homework\n* A  :work:x:\n* B :x:\n";
        let settings = TagSettings { tags_column: 0, ..TagSettings::default() };
        assert_eq!(
            rename_tag(text, "work", "job", &settings),
            "#+FILETAGS: :job:home:\n#+TAGS: job(w) homework\n* A :job:x:\n* B :x:\n"
        );
        assert_eq!(rename_tag("* A :a:b:\n", "a", "b", &settings), "* A :b:\n");
    }

    #[test]
    fn finds_what_to_rename() {
        let keywords = TodoKeywords::default();
        let text = "* TODO Title here :a:bb:\n#+FILETAGS: x y\n";
        assert_eq!(target(text, 21, &keywords), Some(Target::Tag { start: 21, end: 23, name: "bb".to_string() }));
        assert_eq!(
            target(text, 2, &keywords),
            Some(Target::Heading { heading: 0, start: 7, end: 17, title: "Title here".to_string() })
        );
        assert_eq!(target(text, 39, &keywords), Some(Target::Tag { start: 39, end: 40, name: "y".to_string() }));
        assert_eq!(retarget("file:a.org::*Old  [1/2]", "New one"), Some("file:a.org::*New one".to_string()));
        assert_eq!(retarget("Old", "New"), Some("New".to_string()));
        assert_eq!(retarget("#cid", "New"), None);
    }
}
