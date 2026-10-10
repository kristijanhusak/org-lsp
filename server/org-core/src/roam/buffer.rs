//! The org-roam buffer (`org-roam-mode`): for a node, the nodes that link
//! to it (`org-roam-backlinks-section`), those that link to or cite its
//! references (`org-roam-reflinks-section`), and the places that name it
//! without a link (`org-roam-unlinked-references-section`), each with the
//! text around the place (`org-roam-preview-default-function`).

use std::path::Path;

use super::{Db, Node};
use crate::refile::link_display_format;

/// A section of the buffer, and what it shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// `Backlinks`, `Reflinks`, `Unlinked References`.
    pub name: &'static str,
    pub items: Vec<Item>,
}

/// One place in a section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// A link from a node (`org-roam-node-insert-section`): the node's
    /// title, the outline path of the place, the text around it, and where
    /// it is.
    Node { title: String, outline: Vec<String>, preview: String, file: String, offset: usize },
    /// A line naming the node (`org-roam-grep-section`): where, 1-based,
    /// the column counting bytes as ripgrep does, and the line.
    Grep { file: String, row: usize, column: usize, line: String },
}

/// Which sections to show (`org-roam-mode-sections`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Backlinks,
    Reflinks,
    UnlinkedReferences,
}

/// The sections of the buffer for the node `id`, in the order of `kinds`.
/// `read` gives the text of a file, as the editor has it.
pub fn sections(
    db: &Db,
    id: &str,
    kinds: &[Kind],
    read: &dyn Fn(&str) -> Option<String>,
    files: &[String],
) -> Vec<Section> {
    // The node may not be in the database (its file is not org-roam's),
    // while links to its `ID` are.
    let found = db.nodes.iter().find(|(_, node)| node.id == id);
    let mut out = Vec::new();
    for kind in kinds {
        match kind {
            Kind::Backlinks => {
                let items = backlinks(db, id, read);
                if !items.is_empty() {
                    out.push(Section { name: "Backlinks", items });
                }
            }
            Kind::Reflinks => {
                let items = reflinks(db, id, read);
                if !items.is_empty() {
                    out.push(Section { name: "Reflinks", items });
                }
            }
            // Only for a node with a title, which one not in the database
            // has not.
            Kind::UnlinkedReferences => {
                let Some((node_file, node)) = found else { continue };
                let mut titles = vec![node.title.clone()];
                titles.extend(db.aliases.iter().filter(|(owner, _)| owner == id).map(|(_, alias)| alias.clone()));
                let items = unlinked(&titles, node_file, files, read);
                out.push(Section { name: "Unlinked References", items });
            }
        }
    }
    out
}

fn node<'a>(db: &'a Db, id: &str) -> Option<&'a (String, Node)> {
    db.nodes.iter().find(|(_, node)| node.id == id)
}

/// The item of a link or citation from `source` at `offset` of `file`.
/// Its place is taken in the file the database has for the source node
/// (`org-roam-node-file`), which with an `ID` given to two nodes is not
/// always the link's.
fn item(
    db: &Db,
    source: &str,
    file: &str,
    offset: usize,
    outline: &[String],
    read: &dyn Fn(&str) -> Option<String>,
) -> Option<Item> {
    let (node_file, source) = node(db, source)?;
    let link_text = read(file).unwrap_or_default();
    let text = if node_file == file { link_text.clone() } else { read(node_file).unwrap_or_default() };
    // The position is a character's, as Emacs counts them.
    let chars = link_text[..offset.min(link_text.len())].chars().count();
    let offset = text.char_indices().nth(chars).map_or(text.len(), |(at, _)| at);
    Some(Item::Node {
        title: source.title.clone(),
        outline: outline.to_vec(),
        preview: preview(&text, offset),
        file: node_file.clone(),
        offset,
    })
}

/// `org-roam-backlinks-get`, sorted by the titles of their nodes
/// (`org-roam-backlinks-sort`, a stable sort).
fn backlinks(db: &Db, id: &str, read: &dyn Fn(&str) -> Option<String>) -> Vec<Item> {
    let mut items: Vec<Item> = db
        .links
        .iter()
        .filter(|(_, link)| link.dest == id && link.kind == "id")
        .filter_map(|(file, link)| item(db, &link.source, file, link.offset, &link.outline, read))
        .collect();
    items.sort_by(|a, b| title_of(a).cmp(title_of(b)));
    items
}

fn title_of(item: &Item) -> &str {
    match item {
        Item::Node { title, .. } => title,
        Item::Grep { .. } => "",
    }
}

/// `org-roam-reflinks-get`: the links whose destination is one of the
/// node's references, and the citations of its keys, as SQLite's `UNION`
/// returns them (sorted, each once), pushed onto a list, then sorted by
/// title (`org-roam-reflinks-sort`).
fn reflinks(db: &Db, id: &str, read: &dyn Fn(&str) -> Option<String>) -> Vec<Item> {
    let refs: Vec<&str> =
        db.refs.iter().filter(|reference| reference.node == id).map(|r| r.reference.as_str()).collect();
    if refs.is_empty() {
        return Vec::new();
    }
    // (ref, source, pos, outline, file), as emacsql prints them.
    let mut rows: Vec<(String, String, usize, Vec<String>, String)> = Vec::new();
    for reference in &refs {
        for (file, link) in db.links.iter().filter(|(_, link)| link.dest == *reference) {
            rows.push((reference.to_string(), link.source.clone(), link.offset, link.outline.clone(), file.clone()));
        }
        for (file, citation) in db.citations.iter().filter(|(_, citation)| citation.key == *reference) {
            rows.push((
                reference.to_string(),
                citation.node.clone(),
                citation.offset,
                citation.outline.clone(),
                file.clone(),
            ));
        }
    }
    let key = |row: &(String, String, usize, Vec<String>, String)| {
        (
            super::printed(&row.0),
            super::printed(&row.1),
            row.2,
            row.3.iter().map(|part| super::printed(part)).collect::<Vec<_>>(),
        )
    };
    rows.sort_by_key(key);
    rows.dedup_by(|a, b| key(a) == key(b));
    rows.reverse();
    let mut items: Vec<Item> = rows
        .iter()
        .filter_map(|(_, source, offset, outline, file)| item(db, source, file, *offset, outline, read))
        .collect();
    items.sort_by(|a, b| title_of(a).cmp(title_of(b)));
    items
}

/// `org-roam-preview-default-function`, then `org-roam-strip-comments`:
/// the entry that holds `offset` past its meta data and keywords, up to
/// the next headline, trimmed, without comment lines.
pub fn preview(text: &str, offset: usize) -> String {
    let begin = super::edit::end_of_meta_data_from(text, offset, true);
    // `org-next-visible-heading`: the next headline after the line.
    let mut end = text.len();
    let mut line = text[offset..].find('\n').map_or(text.len(), |at| offset + at + 1);
    while line < text.len() {
        let line_end = text[line..].find('\n').map_or(text.len(), |at| line + at);
        if crate::todo::headline_level(&text[line..line_end]).is_some() {
            end = line;
            break;
        }
        line = line_end + 1;
    }
    let (from, to) = (begin.min(end), begin.max(end));
    let trimmed = text[from..to].trim_matches([' ', '\t', '\n', '\r']);
    strip_comments(trimmed)
}

/// `org-roam-strip-comments`: the lines `org-at-comment-p` holds for go.
fn strip_comments(text: &str) -> String {
    let mut out = String::new();
    for line in text.split_inclusive('\n') {
        let bare = line.trim_end_matches('\n').trim_start_matches([' ', '\t']);
        if bare == "#" || bare.starts_with("# ") {
            continue;
        }
        out.push_str(line);
    }
    out
}

/// `org-roam-unlinked-references-section`: the places of the files but
/// the node's own where a title or alias of it stands as a word, outside
/// brackets, ignoring case, as its ripgrep search finds them.
fn unlinked(titles: &[String], node_file: &str, files: &[String], read: &dyn Fn(&str) -> Option<String>) -> Vec<Item> {
    let lowered: Vec<String> = titles.iter().map(|title| title.to_lowercase()).collect();
    let mut items = Vec::new();
    let mut files: Vec<&String> = files.iter().filter(|file| file.as_str() != node_file).collect();
    files.sort();
    for file in files {
        let Some(text) = read(file) else { continue };
        for (row, line) in text.split('\n').enumerate() {
            let matcher = Matcher { line: line.as_bytes(), titles };
            let mut at = 0;
            while at <= line.len() {
                match matcher.pattern(at).into_iter().next() {
                    Some(end) => {
                        let found = &line[at..end];
                        if lowered.contains(&found.to_lowercase()) {
                            items.push(Item::Grep {
                                file: file.clone(),
                                row: row + 1,
                                column: at + 1,
                                line: line.to_string(),
                            });
                        }
                        // After an empty match (an empty title), the search
                        // goes on from the next character.
                        at = if end > at { end } else { at + line[at..].chars().next().map_or(1, char::len_utf8) };
                    }
                    None => {
                        at += line[at..].chars().next().map_or(1, char::len_utf8);
                    }
                }
            }
        }
    }
    items
}

/// The pattern of the ripgrep search, `\[([^[]]++|(?R))*\]|(\bTITLE\b)|…`,
/// as PCRE2 reads it: a bracket holding pairs of a character and closing
/// brackets, or the pattern again; or one of the titles.
struct Matcher<'a> {
    line: &'a [u8],
    titles: &'a [String],
}

impl Matcher<'_> {
    /// The ends of the matches of the whole pattern at `at`, the ones PCRE
    /// tries first first.
    fn pattern(&self, at: usize) -> Vec<usize> {
        let mut ends = self.bracket(at, 0);
        for title in self.titles {
            ends.extend(self.title(at, title));
        }
        ends
    }

    fn bracket(&self, at: usize, depth: usize) -> Vec<usize> {
        if depth > 32 || self.line.get(at) != Some(&b'[') {
            return Vec::new();
        }
        self.repeat(at + 1, depth)
            .into_iter()
            .filter(|end| self.line.get(*end) == Some(&b']'))
            .map(|end| end + 1)
            .collect()
    }

    /// `([^[]]++|(?R))*`, greedy: the ends of as many repetitions as can
    /// be, then fewer.
    fn repeat(&self, at: usize, depth: usize) -> Vec<usize> {
        let mut ends = Vec::new();
        for next in self.group(at, depth) {
            if next > at {
                ends.extend(self.repeat(next, depth));
            }
        }
        ends.push(at);
        ends
    }

    fn group(&self, at: usize, depth: usize) -> Vec<usize> {
        let mut ends = Vec::new();
        // A character but `[`, then closing brackets, all of them.
        if let Some(&first) = self.line.get(at) {
            if first != b'[' {
                let width = std::str::from_utf8(&self.line[at..])
                    .ok()
                    .and_then(|rest| rest.chars().next())
                    .map_or(1, char::len_utf8);
                let closing = self.line[at + width..].iter().take_while(|byte| **byte == b']').count();
                if closing > 0 {
                    ends.push(at + width + closing);
                }
            }
        }
        ends.extend(self.bracket(at, depth + 1));
        for title in self.titles {
            ends.extend(self.title(at, title));
        }
        ends
    }

    /// `\bTITLE\b`, ignoring case. The title is quoted by `regexp-quote`,
    /// whose parentheses group in PCRE: they match nothing.
    fn title(&self, at: usize, title: &str) -> Option<usize> {
        let Ok(rest) = std::str::from_utf8(&self.line[at..]) else { return None };
        let before = std::str::from_utf8(&self.line[..at]).ok()?.chars().next_back();
        let mut chars = rest.char_indices();
        let mut end = 0;
        let wanted: Vec<char> = title.chars().filter(|c| *c != '(' && *c != ')').collect();
        let word = |c: char| c.is_alphanumeric() || c == '_';
        // An empty title, `\b\b`: a word boundary.
        let (Some(&first), Some(&last)) = (wanted.first(), wanted.last()) else {
            let after = rest.chars().next();
            return (before.is_some_and(word) != after.is_some_and(word)).then_some(at);
        };
        for want in &wanted {
            let (index, got) = chars.next()?;
            if !got.to_lowercase().eq(want.to_lowercase()) {
                return None;
            }
            end = index + got.len_utf8();
        }
        let after = rest[end..].chars().next();
        let boundary = |inside: char, outside: Option<char>| word(inside) != outside.is_some_and(word);
        (boundary(first, before) && boundary(last, after)).then_some(at + end)
    }
}

/// The buffer's text as Emacs shows it (without the header line, which has
/// the node's title): each section's heading with its count, then its
/// items.
pub fn plain(sections: &[Section]) -> String {
    let mut out = String::new();
    for section in sections {
        out.push_str(&heading(section));
        out.push('\n');
        for item in &section.items {
            match item {
                Item::Node { title, outline, preview, .. } => {
                    out.push_str(&format!("{title} ({})\n{preview}\n\n", outline_text(outline)));
                }
                Item::Grep { file, row, column, line } => {
                    out.push_str(&format!("{}:{row}:{column} {line}\n", base_name(file)));
                }
            }
        }
        out.push('\n');
    }
    out
}

/// `Backlinks (2)`: magit shows the count of a section's items in place
/// of the colon its heading ends with.
fn heading(section: &Section) -> String {
    if section.items.is_empty() {
        format!("{}:", section.name)
    } else {
        format!("{} ({})", section.name, section.items.len())
    }
}

/// The outline path of a place, `Top` for none.
fn outline_text(outline: &[String]) -> String {
    if outline.is_empty() {
        "Top".to_string()
    } else {
        outline.iter().map(|part| link_display_format(part)).collect::<Vec<_>>().join(" > ")
    }
}

/// `(truncate-string-to-width (file-name-base f) 15 nil nil t)`
fn base_name(file: &str) -> String {
    let name = Path::new(file).file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default();
    let base = match name.rfind('.') {
        Some(at) if at > 0 => name[..at].to_string(),
        _ => name,
    };
    if base.chars().count() <= 15 {
        base
    } else {
        format!("{}…", base.chars().take(14).collect::<String>())
    }
}

/// The buffer as an Org document, for an editor to show: the node's title,
/// a headline for each section with its count, and for each item a
/// headline that links to its place (a line of its file) with the text
/// around it below.
pub fn org(title: &str, sections: &[Section], line_of: &dyn Fn(&str, usize) -> usize) -> String {
    let mut out = format!("#+title: {title}\n");
    for section in sections {
        out.push_str(&format!("\n* {}\n", heading(section)));
        for item in &section.items {
            match item {
                Item::Node { title, outline, preview, file, offset } => {
                    let line = line_of(file, *offset);
                    let link =
                        crate::link::make_string(&format!("file:{file}::{line}"), Some(title)).unwrap_or_default();
                    out.push_str(&format!("** {link} ({})\n", outline_text(outline)));
                    if !preview.is_empty() {
                        out.push_str(preview);
                        if !preview.ends_with('\n') {
                            out.push('\n');
                        }
                    }
                }
                Item::Grep { file, row, column, line } => {
                    let name = format!("{}:{row}:{column}", base_name(file));
                    let link =
                        crate::link::make_string(&format!("file:{file}::{row}"), Some(&name)).unwrap_or_default();
                    out.push_str(&format!("- {link} {line}\n"));
                }
            }
        }
    }
    out
}
