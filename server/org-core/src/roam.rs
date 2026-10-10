//! org-roam (2.3.1, as of 903bd4e): the nodes of a directory of Org
//! files, as `org-roam-db-sync` records them in its database, and what
//! is made from them: the slug of a title, the lines that offer nodes
//! (`org-roam-node-display-template`).
//!
//! A node is an entry with an `ID` property, or a file whose top
//! property drawer has one, unless it has `ROAM_EXCLUDE`. A file is read
//! into the rows org-roam inserts ([`scan`]); [`Db`] puts the files'
//! rows together as SQLite does, which refuses a node whose `ID` is
//! taken and the rows of a node that is not there.

use std::collections::HashSet;
use std::sync::LazyLock;

use org_syntax::{Parse, SyntaxKind, SyntaxNode};
use regex::Regex;

use crate::outline::{complex_heading_parts, Outline};

pub mod capture;
pub mod edit;

/// Which files are org-roam's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// `org-roam-file-extensions`
    pub extensions: Vec<String>,
    /// `org-roam-file-exclude-regexp`: Emacs regexps, matched against the
    /// file's name relative to the directory.
    pub exclude: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        // `org-attach-id-dir` is "data/".
        Settings { extensions: vec!["org".to_string()], exclude: vec!["data/".to_string()] }
    }
}

/// `org-roam-file-p`: whether the file at `relative` (to the directory,
/// with `/` between directories) is one of org-roam's.
pub fn is_roam_file(relative: &str, settings: &Settings) -> bool {
    let extension = |name: &str| -> Option<String> {
        let base = name.rsplit('/').next().unwrap_or(name);
        let at = base.rfind('.').filter(|at| *at > 0)?;
        Some(base[at + 1..].to_string())
    };
    let mut ext = extension(relative);
    if matches!(ext.as_deref(), Some("gpg" | "age")) {
        ext = extension(&relative[..relative.len() - 4]);
    }
    let valid = ext.is_some_and(|ext| settings.extensions.contains(&ext));
    let excluded = settings
        .exclude
        .iter()
        .filter_map(|pattern| crate::agenda::matcher::emacs_regex(pattern))
        .any(|regex| regex.is_match(relative));
    valid && !excluded
}

/// The order `org-roam-db-sync` reads files in, given their names
/// relative to the directory (`org-roam--directory-files-recursively`):
/// depth first, the files of a directory's subdirectories, each sorted by
/// name with its slash, before its own files, sorted. It decides which of
/// two nodes with the same `ID` stays.
pub fn sync_order(names: &mut [String]) {
    names.sort_by(|a, b| {
        let (a_parts, b_parts): (Vec<&str>, Vec<&str>) = (a.split('/').collect(), b.split('/').collect());
        for (level, (x, y)) in a_parts.iter().zip(&b_parts).enumerate() {
            let x_dir = level + 1 < a_parts.len();
            let y_dir = level + 1 < b_parts.len();
            let ordering = match (x_dir, y_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                (true, true) => format!("{x}/").cmp(&format!("{y}/")),
                (false, false) => x.cmp(y),
            };
            if ordering != std::cmp::Ordering::Equal {
                return ordering;
            }
        }
        a_parts.len().cmp(&b_parts.len())
    });
}

/// A row of the `nodes` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub id: String,
    /// 0 for a file's node.
    pub level: usize,
    /// Byte offset of the headline; 0 for a file's node.
    pub offset: usize,
    pub todo: Option<String>,
    /// `org-priority-to-value`: a letter's character code, or a number.
    pub priority: Option<i64>,
    /// `%FT%T`, as `org-roam-db-get-scheduled-time` writes it.
    pub scheduled: Option<String>,
    pub deadline: Option<String>,
    pub title: String,
    /// `org-entry-properties`, in its order; a property without a value
    /// is `None`.
    pub properties: Vec<(String, Option<String>)>,
    /// The titles of the headlines above (`org-get-outline-path`).
    pub olp: Vec<String>,
}

/// A row of the `refs` table: a citation key (`cite`), or a link's path
/// without its type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ref {
    pub node: String,
    pub reference: String,
    pub kind: String,
}

/// A row of the `links` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// Byte offset of the link.
    pub offset: usize,
    /// The node the link is in.
    pub source: String,
    /// The link's path, without a search option.
    pub dest: String,
    /// The link's type (`id`, `https`, `roam`, `fuzzy` …).
    pub kind: String,
    /// The headlines down to the link's, none before the first headline.
    pub outline: Vec<String>,
    /// What came after `::` in the path.
    pub search: Option<String>,
}

/// A row of the `citations` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Citation {
    pub node: String,
    pub key: String,
    /// Byte offset of the reference.
    pub offset: usize,
    pub outline: Vec<String>,
}

/// What org-roam inserts for one file, in the order it inserts it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileData {
    /// `org-roam-db--file-title`
    pub title: String,
    pub rows: Vec<Row>,
}

/// One insertion. The rows of a node go in after it, and SQLite refuses
/// them when there is no node of their `ID`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    Node(Node),
    Aliases(String, Vec<String>),
    Tags(String, Vec<String>),
    Refs(Vec<Ref>),
    Link(Link),
    Citation(Citation),
}

/// The tables, for the files read so far.
#[derive(Debug, Clone, Default)]
pub struct Db {
    pub files: Vec<(String, String)>,
    /// Each node with its file.
    pub nodes: Vec<(String, Node)>,
    pub aliases: Vec<(String, String)>,
    pub tags: Vec<(String, String)>,
    pub refs: Vec<Ref>,
    /// Each link with the file it is in.
    pub links: Vec<(String, Link)>,
    pub citations: Vec<(String, Citation)>,
    ids: HashSet<String>,
}

impl Db {
    /// `org-roam-db-update-file`: the rows of `file`. A file that cannot be
    /// read leaves nothing (`org-roam-db-clear-file`).
    pub fn add(&mut self, file: &str, data: Result<FileData, String>) {
        let Ok(data) = data else { return };
        self.files.push((file.to_string(), data.title));
        for row in data.rows {
            match row {
                Row::Node(node) => {
                    // The `ID` is the primary key: a second node of it is
                    // refused with a warning.
                    if self.ids.insert(node.id.clone()) {
                        self.nodes.push((file.to_string(), node));
                    }
                }
                Row::Aliases(id, aliases) if self.ids.contains(&id) => {
                    self.aliases.extend(aliases.into_iter().map(|alias| (id.clone(), alias)));
                }
                Row::Tags(id, tags) if self.ids.contains(&id) => {
                    self.tags.extend(tags.into_iter().map(|tag| (id.clone(), tag)));
                }
                Row::Refs(refs) if refs.iter().all(|reference| self.ids.contains(&reference.node)) => {
                    self.refs.extend(refs);
                }
                Row::Link(link) if self.ids.contains(&link.source) => self.links.push((file.to_string(), link)),
                Row::Citation(citation) if self.ids.contains(&citation.node) => {
                    self.citations.push((file.to_string(), citation))
                }
                _ => {}
            }
        }
    }
}

/// Where `org-roam-id-at-point` and the like stand: a headline, or the
/// start of the file.
type At = Option<usize>;

/// One file read as org-roam reads it.
struct Reader<'a> {
    text: &'a str,
    parse: &'a Parse,
    outline: &'a Outline,
    /// The file's name, for `FILE`.
    file: &'a str,
}

/// `org-roam-db-update-file` for one file: what it inserts, or why it
/// failed. `file` is the file's absolute name.
pub fn scan(text: &str, parse: &Parse, outline: &Outline, file: &str) -> Result<FileData, String> {
    Reader { text, parse, outline, file }.scan()
}

impl Reader<'_> {
    fn scan(&self) -> Result<FileData, String> {
        let root = self.parse.syntax();
        let titles = crate::outline::keyword_values(&root, "TITLE");
        let title = crate::refile::link_display_format(&titles.join(" "));
        let mut rows = Vec::new();
        // `org-roam-db-insert-file-node`: the file's own node, when the
        // file does not start with a headline.
        let starts_with_headline = self.outline.entries.first().is_some_and(|entry| entry.offset == 0);
        if !starts_with_headline {
            if let Some(id) = self.node_id(None) {
                rows.push(Row::Node(Node {
                    id: id.clone(),
                    level: 0,
                    offset: 0,
                    todo: None,
                    priority: None,
                    scheduled: None,
                    deadline: None,
                    title: title.clone(),
                    properties: self.entry_properties(None),
                    olp: Vec::new(),
                }));
                if !self.outline.file_tags.is_empty() {
                    rows.push(Row::Tags(id.clone(), self.outline.file_tags.clone()));
                }
                self.aliases_and_refs(None, &id, &mut rows, false)?;
            }
        }
        // `org-roam-db-map-nodes`: each headline that is a node.
        for (index, entry) in self.outline.entries.iter().enumerate() {
            let Some(id) = self.node_id(Some(index)) else { continue };
            let line = &self.text[entry.offset..entry.line_end];
            let parts = complex_heading_parts(line, entry.level, &self.outline.todo);
            // A node without a title is skipped; its aliases, tags and
            // refs go to the node of its `ID` if there is one.
            let Some((start, end)) = parts.title else {
                self.aliases_and_refs(Some(index), &id, &mut rows, true)?;
                continue;
            };
            let priority = parts.priority.map(|(from, to)| priority_value(&line[from + 2..to - 1]));
            let todo = parts.keyword.map(|(from, to)| line[from..to].to_string());
            rows.push(Row::Node(Node {
                id: id.clone(),
                level: entry.level,
                offset: entry.offset,
                todo,
                priority,
                scheduled: entry.scheduled.as_ref().and_then(|planning| iso_time(&planning.raw)),
                deadline: entry.deadline.as_ref().and_then(|planning| iso_time(&planning.raw)),
                title: crate::refile::link_display_format(&line[start..end]),
                properties: self.entry_properties(Some(index)),
                olp: self.outline_path(Some(index), false),
            }));
            self.aliases_and_refs(Some(index), &id, &mut rows, true)?;
        }
        // `org-roam-db-map-links`, then the citations: those Org's
        // element parser sees.
        for (start, _, kind, path) in
            self.links().into_iter().filter(|(start, end, _, _)| self.in_context(*start, *end))
        {
            let offset = start;
            let Some(source) = self.id_at(offset) else { continue };
            let (dest, search) = match path.find("::") {
                Some(at) => (path[..at].to_string(), Some(path[at + 2..].to_string())),
                None => (path, None),
            };
            rows.push(Row::Link(Link { offset, source, dest, kind, outline: self.outline_path_at(offset), search }));
        }
        for (offset, key) in self.citations().into_iter().filter(|(offset, _)| self.parsed(*offset)) {
            let Some(node) = self.id_at(offset) else { continue };
            rows.push(Row::Citation(Citation { node, key, offset, outline: self.outline_path_at(offset) }));
        }
        Ok(FileData { title, rows })
    }

    /// `org-roam-db-insert-aliases`, `-tags` and `-refs` for the node at
    /// `at` (the file's node takes its tags apart).
    fn aliases_and_refs(&self, at: At, id: &str, rows: &mut Vec<Row>, tags: bool) -> Result<(), String> {
        if let Some(value) = self.outline.entry_property(at, "ROAM_ALIASES", false) {
            let aliases = split_and_unquote(&value)?;
            if !aliases.is_empty() {
                rows.push(Row::Aliases(id.to_string(), aliases));
            }
        }
        if tags {
            let entry = &self.outline.entries[at.unwrap_or(0)];
            let names: Vec<String> = entry.tags.iter().map(|tag| tag.name.clone()).collect();
            if !names.is_empty() {
                rows.push(Row::Tags(id.to_string(), names));
            }
        }
        if let Some(value) = self.outline.entry_property(at, "ROAM_REFS", false) {
            let mut refs = Vec::new();
            for reference in split_and_unquote(&value)? {
                for (path, kind) in parse_ref(&reference) {
                    refs.push(Ref { node: id.to_string(), reference: path, kind });
                }
            }
            refs.reverse();
            if !refs.is_empty() {
                rows.push(Row::Refs(refs));
            }
        }
        Ok(())
    }

    /// Whether `org-element-context` finds the link from `start` to
    /// `end`: on a headline, only in the title as
    /// `org-complex-heading-regexp` delimits it, after a `COMMENT`.
    fn in_context(&self, start: usize, end: usize) -> bool {
        let Some(index) = self.heading_of(start) else { return true };
        let entry = &self.outline.entries[index];
        if start >= entry.line_end {
            return true;
        }
        let line = &self.text[entry.offset..entry.line_end];
        let Some((mut from, to)) = complex_heading_parts(line, entry.level, &self.outline.todo).title else {
            return false;
        };
        if line[from..].starts_with("COMMENT") {
            from += "COMMENT".len();
        }
        let (start, last) = (start - entry.offset, end - entry.offset - 1);
        from < to && start >= from && last <= to
    }

    /// Whether Org's element parser reads the object at `offset`: not
    /// the part of a headline before where it takes the title to start
    /// (`org-element--headline-parse-title`), which `org-priority-regexp`
    /// may put far into the line.
    fn parsed(&self, offset: usize) -> bool {
        let Some(index) = self.heading_of(offset) else { return true };
        let entry = &self.outline.entries[index];
        if offset >= entry.line_end {
            return true;
        }
        let line = &self.text[entry.offset..entry.line_end];
        offset - entry.offset >= element_title_start(line, entry.level, &self.outline.todo)
    }

    /// `org-roam-db-node-p` at `at`: its `ID`, unless it has
    /// `ROAM_EXCLUDE`. The start of a file that starts with a headline is
    /// that headline.
    fn node_id(&self, at: At) -> Option<String> {
        let at = self.normalize(at);
        let id = self.outline.entry_property(at, "ID", false)?;
        if self.outline.entry_property(at, "ROAM_EXCLUDE", false).is_some() {
            return None;
        }
        Some(id)
    }

    fn normalize(&self, at: At) -> At {
        match at {
            None if self.outline.entries.first().is_some_and(|entry| entry.offset == 0) => Some(0),
            at => at,
        }
    }

    /// The headline whose entry `offset` is in
    /// (`org-back-to-heading-or-point-min`).
    fn heading_of(&self, offset: usize) -> At {
        self.outline.entries.iter().rposition(|entry| entry.offset <= offset)
    }

    /// `org-roam-id-at-point`: the `ID` of the node `offset` is in, going
    /// up the headlines (`org-roam-up-heading-or-point-min`).
    fn id_at(&self, offset: usize) -> Option<String> {
        let mut at = self.normalize(self.heading_of(offset));
        loop {
            if let Some(id) = self.node_id(at) {
                return Some(id);
            }
            let at_start = at.is_none_or(|index| self.outline.entries[index].offset == 0);
            if at_start {
                return None;
            }
            let index = at.unwrap_or(0);
            let entry = &self.outline.entries[index];
            at = if entry.level > 1 { entry.parent } else { None };
            at = self.normalize(at);
        }
    }

    /// `org-get-outline-path` of the headline at `at`, with that
    /// headline's own title when `with_self`.
    fn outline_path(&self, at: At, with_self: bool) -> Vec<String> {
        let Some(index) = at else { return Vec::new() };
        let mut path = Vec::new();
        let mut current = if with_self { Some(index) } else { self.outline.entries[index].parent };
        while let Some(index) = current {
            let entry = &self.outline.entries[index];
            path.push(crate::refile::headline_path_part(self.text, entry.offset, &self.outline.todo));
            current = entry.parent;
        }
        path.reverse();
        path
    }

    /// `org-get-outline-path` with the entry's own headline, for the
    /// entry `offset` is in; none before the first headline.
    fn outline_path_at(&self, offset: usize) -> Vec<String> {
        self.outline_path(self.heading_of(offset), true)
    }

    /// The links org-roam records: those Org parses (`org-element-
    /// context`), and those in keyword lines and property drawers
    /// (`org-roam-db-extra-links-elements`), but for `#+TRANSCLUDE:` and
    /// `ROAM_REFS`. Each with its offset, type and path.
    fn links(&self) -> Vec<(usize, usize, String, String)> {
        let root = self.parse.syntax();
        let mut out: Vec<(usize, usize, String, String)> = Vec::new();
        let abbreviations = crate::open::abbreviations(self.text, &[]);
        for node in root.descendants() {
            let is_link = |node: &SyntaxNode| {
                matches!(node.kind(), SyntaxKind::LINK | SyntaxKind::PLAIN_LINK | SyntaxKind::ANGLE_LINK)
            };
            if !is_link(&node) || node.ancestors().skip(1).any(|above| is_link(&above)) {
                continue;
            }
            let start = usize::from(node.text_range().start());
            let end = usize::from(node.text_range().end());
            if let Some((kind, path)) = link_type_and_path(&self.text[start..end], &abbreviations) {
                out.push((start, end, kind, path));
            }
        }
        for node in root.descendants() {
            let (key, value) = match node.kind() {
                SyntaxKind::KEYWORD => (SyntaxKind::KEYWORD_KEY, SyntaxKind::KEYWORD_VALUE),
                SyntaxKind::NODE_PROPERTY => (SyntaxKind::PROPERTY_KEY, SyntaxKind::PROPERTY_VALUE),
                _ => continue,
            };
            let key_text = token(&node, key).map(|token| token.text().to_string()).unwrap_or_default();
            let name = key_text.trim_start_matches("#+").trim_matches(':').to_ascii_uppercase();
            let excluded = match node.kind() {
                SyntaxKind::KEYWORD => name == "TRANSCLUDE",
                _ => name == "ROAM_REFS",
            };
            let Some(value) = token(&node, value) else { continue };
            if excluded {
                continue;
            }
            let base = usize::from(value.text_range().start());
            for found in LINK_ANY.find_iter(value.text()) {
                if let Some((kind, path)) = link_type_and_path(found.as_str(), &abbreviations) {
                    out.push((base + found.start(), base + found.end(), kind, path));
                }
            }
        }
        out.sort_by_key(|(offset, _, _, _)| *offset);
        out.dedup_by_key(|(offset, _, _, _)| *offset);
        out
    }

    /// The references of the file's citations, with where each starts.
    fn citations(&self) -> Vec<(usize, String)> {
        let mut out = Vec::new();
        for node in self.parse.syntax().descendants().filter(|node| node.kind() == SyntaxKind::CITATION) {
            let start = usize::from(node.text_range().start());
            let text = node.text().to_string();
            out.extend(citation_references(&text).into_iter().map(|(at, key)| (start + at, key)));
        }
        out
    }

    /// `org-entry-properties` at `at`: the special properties, then those
    /// of the property drawer, then the category, in the order Org
    /// returns them (the last one it finds first).
    fn entry_properties(&self, at: At) -> Vec<(String, Option<String>)> {
        let at = self.normalize(at);
        let mut props: Vec<(String, Option<String>)> = Vec::new();
        let push = |props: &mut Vec<(String, Option<String>)>, key: &str, value: Option<String>| {
            props.insert(0, (key.to_string(), value));
        };
        let beg = at.map_or(0, |index| self.outline.entries[index].offset);
        let line_end = self.text[beg..].find('\n').map_or(self.text.len(), |end| beg + end);
        let first_line = &self.text[beg..line_end];
        if let Some(index) = at {
            let entry = &self.outline.entries[index];
            let title = complex_heading_parts(first_line, entry.level, &self.outline.todo).title;
            let item = title
                .map(|(start, end)| &first_line[start..end])
                .filter(|title| !title.trim().is_empty())
                .map_or(String::new(), remove_tabs);
            push(&mut props, "ITEM", Some(item));
            if let Some(todo) = &entry.todo {
                push(&mut props, "TODO", Some(todo.clone()));
            }
        }
        // `org-priority-regexp`, from the start of the line, ignoring case.
        let priority = PRIORITY.captures(first_line).map(|found| found[1].to_string()).unwrap_or_else(|| {
            let default = self.outline.priorities.map_or(i64::from(b'B'), |priorities| priorities.default);
            crate::properties::priority_string(default)
        });
        push(&mut props, "PRIORITY", Some(priority));
        push(&mut props, "FILE", Some(self.file.to_string()));
        let tags: Vec<&crate::outline::Tag> = match at {
            Some(index) => self.outline.entries[index].tags.iter().collect(),
            None => Vec::new(),
        };
        let local: Vec<&str> = tags.iter().filter(|tag| !tag.inherited).map(|tag| tag.name.as_str()).collect();
        if !local.is_empty() {
            push(&mut props, "TAGS", Some(format!(":{}:", local.join(":"))));
        }
        let all: Vec<String> = match at {
            Some(_) => tags.iter().map(|tag| tag.name.clone()).collect(),
            None => {
                let mut names: Vec<String> = Vec::new();
                for tag in &self.outline.file_tags {
                    if self.outline.inheritance.inherits_tag(tag) {
                        names.retain(|known| known != tag);
                        names.push(tag.clone());
                    }
                }
                names
            }
        };
        if !all.is_empty() {
            push(&mut props, "ALLTAGS", Some(format!(":{}:", all.join(":"))));
        }
        push(&mut props, "BLOCKED", Some(String::new()));
        // The planning line under the headline (the second line).
        let next = (line_end + 1).min(self.text.len());
        let next_end = self.text[next..].find('\n').map_or(self.text.len(), |end| next + end);
        let next_line = &self.text[next..next_end];
        let is_planning = PLANNING.is_match(next_line);
        if is_planning {
            for keyword in ["CLOSED", "DEADLINE", "SCHEDULED"] {
                let needle = format!("{keyword}:");
                if let Some(found) = next_line.rfind(&needle) {
                    let after = next_line[found + needle.len()..].trim_start_matches([' ', '\t']);
                    if let Some(stamp) = TIMESTAMP.find(after).filter(|stamp| stamp.start() == 0) {
                        push(&mut props, keyword, Some(stamp.as_str().to_string()));
                    }
                }
            }
        }
        // The first active and inactive timestamps of the headline, then
        // of the section after the planning line.
        let section_end = match at {
            Some(index) => self.outline.entries.get(index + 1).map_or(self.text.len(), |entry| entry.offset),
            None => self.outline.entries.first().map_or(self.text.len(), |entry| entry.offset),
        };
        let mut found: Vec<(String, String)> = Vec::new();
        let stamps = self.timestamps();
        let take = |from: usize, to: usize, found: &mut Vec<(String, String)>| {
            for (offset, active, raw) in &stamps {
                if found.len() == 2 {
                    break;
                }
                if *offset < from || *offset >= to {
                    continue;
                }
                let key = if *active { "TIMESTAMP" } else { "TIMESTAMP_IA" };
                if !found.iter().any(|(known, _)| known == key) {
                    found.insert(0, (key.to_string(), raw.clone()));
                }
            }
        };
        take(beg, line_end, &mut found);
        if found.len() < 2 {
            let start = if is_planning { (next_end + 1).min(self.text.len()) } else { next };
            take(start.max(next), section_end.max(start), &mut found);
        }
        for (key, value) in found.into_iter().rev() {
            push(&mut props, &key, Some(value));
        }
        // The property drawer.
        let drawer = match at {
            Some(index) => &self.outline.entries[index].properties,
            None => &self.outline.file_properties,
        };
        let mut seen: Vec<String> = Vec::new();
        for (key, value) in drawer {
            let key = key.to_uppercase();
            let value = value.clone();
            let (base, extending) = match key.strip_suffix('+') {
                Some(base) => (base.to_string(), true),
                None => (key.clone(), false),
            };
            if SPECIAL.iter().any(|special| special.eq_ignore_ascii_case(&base)) {
                continue;
            }
            if extending {
                // `org--update-property-plist`
                let previous = props.iter().position(|(known, _)| *known == base).map(|at| props.remove(at).1);
                let joined = match previous.flatten() {
                    Some(previous) => format!("{previous} {value}"),
                    None => value,
                };
                props.insert(0, (base, Some(joined)));
            } else if seen.contains(&key) {
                continue;
            } else {
                seen.push(key.clone());
                match props.iter_mut().find(|(known, _)| known.eq_ignore_ascii_case(&key)) {
                    Some(existing) => {
                        let rest = existing.1.clone().unwrap_or_default();
                        existing.1 = Some(format!("{value} {rest}"));
                    }
                    None => props.insert(0, (key, Some(value))),
                }
            }
        }
        if !props.iter().any(|(key, _)| key == "CATEGORY") {
            let category = match at {
                Some(index) => self.outline.entries[index].category.clone(),
                None => self.outline.category.clone(),
            };
            push(&mut props, "CATEGORY", Some(category));
        }
        props
    }

    /// The timestamps of the file Org reads as timestamp objects, with
    /// whether they are active and how they are written.
    fn timestamps(&self) -> Vec<(usize, bool, String)> {
        self.parse
            .syntax()
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::TIMESTAMP)
            .filter(|node| {
                !node.ancestors().any(|above| matches!(above.kind(), SyntaxKind::PLANNING | SyntaxKind::CLOCK))
            })
            .filter_map(|node| {
                let raw = node.text().to_string();
                if raw.starts_with("<%%") {
                    return None;
                }
                Some((usize::from(node.text_range().start()), raw.starts_with('<'), raw))
            })
            .collect()
    }
}

/// Where Org's element parser takes a headline's title to start: after
/// the stars and blanks, a TODO keyword followed by a space or the end,
/// what `org-priority-regexp` matches from there (ignoring case, and
/// anywhere on the line), and `COMMENT`.
fn element_title_start(line: &str, level: usize, todo: &org_syntax::TodoKeywords) -> usize {
    let mut at = level + line[level..].len() - line[level..].trim_start_matches([' ', '\t']).len();
    if let Some(name) = todo
        .names()
        .find(|name| line[at..].strip_prefix(*name).is_some_and(|rest| rest.is_empty() || rest.starts_with(' ')))
    {
        at += name.len();
        at += line[at..].len() - line[at..].trim_start_matches([' ', '\t']).len();
    }
    if let Some(found) = PRIORITY.find(&line[at..]) {
        at += found.end();
    }
    static COMMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^COMMENT(?:[ \t]|$)").expect("a valid regex"));
    if COMMENT.is_match(&line[at..]) {
        at += "COMMENT".len();
        at += line[at..].len() - line[at..].trim_start_matches([' ', '\t']).len();
    }
    at
}

/// The special properties, which a drawer cannot set
/// (`org-special-properties`).
const SPECIAL: &[&str] = &[
    "ALLTAGS",
    "BLOCKED",
    "CLOCKSUM",
    "CLOCKSUM_T",
    "CLOSED",
    "DEADLINE",
    "FILE",
    "ITEM",
    "PRIORITY",
    "SCHEDULED",
    "TAGS",
    "TIMESTAMP",
    "TIMESTAMP_IA",
    "TODO",
];

static PRIORITY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^.*?\[#([A-Z]|[0-9]|[1-5][0-9]|6[0-4])\] ?").expect("a valid regex"));
static PLANNING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[ \t]*(?:CLOSED:|DEADLINE:|SCHEDULED:)").expect("a valid regex"));
/// `org-ts-regexp-both`
static TIMESTAMP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\[<]([0-9]{4}-[0-9]{2}-[0-9]{2}(?: [^\n]*?)?)[\]>]").expect("a valid regex"));
/// `org-link-any-re`: a bracket link, an angle link or a plain link, of
/// Org's link types and org-roam's `roam`.
static LINK_ANY: LazyLock<Regex> = LazyLock::new(|| {
    let types =
        org_syntax::link_types().iter().chain(&["roam"]).map(|ty| regex::escape(ty)).collect::<Vec<_>>().join("|");
    let bracket = r"\[\[(?:[^\]\[\\]|\\(?:\\\\)*[\]\[]|\\+[^\]\[])+\](?:\[[\s\S]+?\])?\]";
    let angle = format!(r"<(?:{types}):[^>\n]*(?:\n[ \t]*[^> \t\n][^>\n]*)*>");
    let group = r"[(<\[](?:[^\]\[ \t\n()<>]|[(<\[][^\]\[ \t\n()<>]*[\])>])*[\])>]";
    let plain = format!(r"\b(?:{types}):(?:[^\]\[ \t\n()<>]|{group})+(?:[^[:punct:]\s]|-|/|{group})");
    Regex::new(&format!("(?i){bracket}|{angle}|{plain}")).expect("a valid regex")
});

fn token(node: &SyntaxNode, kind: SyntaxKind) -> Option<org_syntax::SyntaxToken> {
    node.children_with_tokens().filter_map(|child| child.into_token()).find(|token| token.kind() == kind)
}

/// `org-element-link-parser`: the type and path of the link written as
/// `raw` (a bracket, angle or plain link).
fn link_type_and_path(raw: &str, abbreviations: &[(String, String)]) -> Option<(String, String)> {
    let (kind, path) = link_parts(raw, abbreviations)?;
    // A file link (`file`, `file+sys` …) has its search option taken
    // apart, and its leading slashes made one.
    static FILE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^file(?:\+.+)?$").expect("a valid regex"));
    if FILE.is_match(&kind) {
        let path = path.find("::").map_or(path.as_str(), |at| &path[..at]);
        static SLASHES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^///*(.:)?/").expect("a valid regex"));
        return Some(("file".to_string(), SLASHES.replace(path, "$1/").into_owned()));
    }
    Some((kind, path))
}

fn link_parts(raw: &str, abbreviations: &[(String, String)]) -> Option<(String, String)> {
    if let Some(inner) = raw.strip_prefix("[[") {
        let end = bracket_path_end(inner)?;
        let path = unescape(&inner[..end]);
        // Line breaks and the blanks around them are a space.
        static BREAKS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:[ \t]*\n[ \t]*)+").expect("a valid regex"));
        let path = BREAKS.replace_all(&path, " ").into_owned();
        let path = crate::open::expand_abbreviation(&path, abbreviations).unwrap_or(path);
        if path.starts_with('/') || path.starts_with("./") || path.starts_with("../") || path.starts_with("~/") {
            return Some(("file".to_string(), path));
        }
        if let Some((kind, rest)) = typed(&path) {
            return Some((kind, rest));
        }
        if path.starts_with('(') && path.ends_with(')') && path.len() >= 2 {
            return Some(("coderef".to_string(), path[1..path.len() - 1].to_string()));
        }
        if let Some(id) = path.strip_prefix('#') {
            return Some(("custom-id".to_string(), id.to_string()));
        }
        return Some(("fuzzy".to_string(), path));
    }
    let inner = raw.strip_prefix('<').and_then(|rest| rest.strip_suffix('>')).unwrap_or(raw);
    // An angle link's line breaks and the blanks after them go.
    static ANGLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n[ \t]*").expect("a valid regex"));
    let inner = ANGLE.replace_all(inner, "").into_owned();
    typed(&inner)
}

/// `TYPE:path` with one of Org's link types (and org-roam's `roam`).
fn typed(path: &str) -> Option<(String, String)> {
    let (kind, rest) = path.split_once(':')?;
    let known = org_syntax::link_types().iter().chain(&["roam"]).any(|ty| ty.eq_ignore_ascii_case(kind));
    known.then(|| (kind.to_string(), rest.to_string()))
}

/// Where the path of a bracket link ends, in the text after `[[`.
fn bracket_path_end(inner: &str) -> Option<usize> {
    let bytes = inner.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at += 2,
            b']' => return Some(at),
            b'[' => return None,
            _ => at += 1,
        }
    }
    None
}

/// `org-link-unescape`: backslashes before brackets and backslashes
/// that precede them.
fn unescape(path: &str) -> String {
    static ESCAPED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\\+)([\]\[])").expect("a valid regex"));
    ESCAPED
        .replace_all(path, |found: &regex::Captures| {
            let slashes = found[1].len();
            format!("{}{}", "\\".repeat(slashes / 2), &found[2])
        })
        .into_owned()
}

/// The references of a citation (`[cite/style:prefix @key suffix; …]`):
/// where each starts in `text` and its key.
fn citation_references(text: &str) -> Vec<(usize, String)> {
    static KEY: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"@([!#-+./:<>-@^-`{-~\p{L}\p{N}_-]+)").expect("a valid regex"));
    let Some(colon) = text.find(':') else { return Vec::new() };
    let body_end = text.len().saturating_sub(1);
    let mut out = Vec::new();
    let mut start = colon + 1;
    for segment in text[colon + 1..body_end].split(';') {
        if let Some(found) = KEY.captures(segment) {
            // The first reference starts after the blanks that follow the
            // colon; the others right after their semicolon.
            let blanks = if start == colon + 1 { segment.len() - segment.trim_start().len() } else { 0 };
            out.push((start + blanks, found[1].to_string()));
        }
        start += segment.len() + 1;
    }
    out
}

/// `split-string-and-unquote`: words, and strings in double quotes,
/// read as Emacs Lisp reads them.
pub fn split_and_unquote(text: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut rest = text;
    loop {
        let Some(quote) = rest.find('"') else {
            out.extend(rest.split_whitespace().map(str::to_string));
            return Ok(out);
        };
        out.extend(rest[..quote].split_whitespace().map(str::to_string));
        let mut value = String::new();
        let mut chars = rest[quote + 1..].char_indices();
        let mut end = None;
        while let Some((at, c)) = chars.next() {
            match c {
                '"' => {
                    end = Some(quote + 1 + at + 1);
                    break;
                }
                '\\' => match chars.next() {
                    Some((_, 'n')) => value.push('\n'),
                    Some((_, 't')) => value.push('\t'),
                    // A backslash and a newline, or a space, stand for
                    // nothing.
                    Some((_, '\n' | ' ')) => {}
                    Some((_, other)) => value.push(other),
                    None => break,
                },
                other => value.push(other),
            }
        }
        let Some(end) = end else { return Err("End of file during parsing".to_string()) };
        out.push(value);
        rest = &rest[end..];
    }
}

/// What a `ROAM_REFS` word stands for: citation keys (`cite`), or a
/// link's path and type.
fn parse_ref(reference: &str) -> Vec<(String, String)> {
    if let Some(key) = reference.strip_prefix('@') {
        return vec![(key.to_string(), "cite".to_string())];
    }
    if reference.starts_with("[cite:") {
        return citation_references(reference).into_iter().map(|(_, key)| (key, "cite".to_string())).collect();
    }
    // `org-link-encode` of its spaces, then the first link in it.
    let encoded = reference.replace(' ', "%20");
    let Some(found) = LINK_ANY.find(&encoded) else { return Vec::new() };
    let url = match found.as_str().strip_prefix("[[") {
        Some(inner) => match bracket_path_end(inner) {
            Some(end) => inner[..end].to_string(),
            None => return Vec::new(),
        },
        None => found.as_str().to_string(),
    };
    let Some((scheme, rest)) = url.split_once(':') else { return Vec::new() };
    // `url-generic-parse-url`: the type and the host in lower case.
    let rest = match rest.strip_prefix("//") {
        Some(after) => {
            let end = after.find(['/', '?', '#']).unwrap_or(after.len());
            let (authority, tail) = after.split_at(end);
            let authority = match authority.rsplit_once('@') {
                Some((user, host)) => format!("{user}@{}", host.to_lowercase()),
                None => authority.to_lowercase(),
            };
            format!("//{authority}{tail}")
        }
        None => rest.to_string(),
    };
    vec![(link_decode(&rest), scheme.to_lowercase())]
}

/// `org-link-decode`: `%XX` sequences, read as UTF-8.
fn link_decode(text: &str) -> String {
    static ENCODED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:%[0-9A-Za-z]{2})+").expect("a valid regex"));
    ENCODED
        .replace_all(text, |found: &regex::Captures| {
            let bytes: Vec<u8> = found[0]
                .as_bytes()
                .chunks(3)
                .map(|chunk| u8::from_str_radix(std::str::from_utf8(&chunk[1..]).unwrap_or("00"), 16).unwrap_or(0))
                .collect();
            String::from_utf8_lossy(&bytes).into_owned()
        })
        .into_owned()
}

/// `org-priority-to-value`: a number, or else a character's code.
fn priority_value(text: &str) -> i64 {
    let digits: String = text.chars().skip_while(|c| !c.is_ascii_digit()).take_while(char::is_ascii_digit).collect();
    digits.parse().unwrap_or_else(|_| text.chars().next().map_or(0, |c| c as i64))
}

/// `org-time-string-to-time` written as `%FT%T`.
fn iso_time(raw: &str) -> Option<String> {
    let parsed = crate::timestr::parse_time_string(raw)?;
    let (hour, minute) = parsed.time.unwrap_or((0, 0));
    let time = crate::date::DateTime::new(parsed.date, hour, minute);
    let date = time.date();
    Some(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:00",
        date.year,
        date.month,
        date.day,
        time.minute / 60,
        time.minute % 60
    ))
}

/// `org-remove-tabs`: each tab as the spaces up to the next multiple of 8
/// columns, counted in characters.
fn remove_tabs(text: &str) -> String {
    let mut out = String::new();
    let mut column = 0;
    for c in text.chars() {
        if c == '\t' {
            let next = (column / 8 + 1) * 8;
            out.push_str(&" ".repeat(next - column));
            column = next;
        } else {
            out.push(c);
            column += 1;
        }
    }
    out
}

/// `org-roam-node-slugify`: the title without the accents of
/// `slug-trim-chars`, each run of characters other than letters and
/// digits one `_`, in lower case.
pub fn slugify(title: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    const TRIM: &[u32] = &[
        0x300, 0x301, 0x302, 0x303, 0x304, 0x306, 0x307, 0x308, 0x309, 0x30A, 0x30B, 0x30C, 0x31B, 0x323, 0x324, 0x325,
        0x327, 0x32D, 0x32E, 0x330, 0x331,
    ];
    let stripped: String = title.nfd().filter(|c| !TRIM.contains(&(*c as u32))).nfc().collect();
    // `[[:alnum:]]`: letters, marks, decimal digits and letter numbers.
    static OTHER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^\p{L}\p{M}\p{Nd}\p{Nl}]").expect("a valid regex"));
    static RUNS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"__*").expect("a valid regex"));
    let slug = OTHER.replace_all(&stripped, "_");
    let slug = RUNS.replace_all(&slug, "_");
    let slug = slug.strip_prefix('_').unwrap_or(&slug);
    let slug = slug.strip_suffix('_').unwrap_or(slug);
    slug.to_lowercase()
}

/// A node as `org-roam-node-read` offers it: the line shown for one of
/// its titles (its title or an alias), and its `ID`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub display: String,
    pub id: String,
    /// The title or alias the line was made from.
    pub title: String,
}

impl Db {
    /// `org-roam-node-read--completions`: a line for the title and each
    /// alias of every node, formatted with `template`
    /// (`org-roam-node-display-template`) for a window `width` columns
    /// wide, the nodes of the files modified last first
    /// (`org-roam-node-default-sort`). `mtime` gives a file's modification
    /// time. Fails as Emacs does on a field it has no accessor for.
    pub fn candidates(
        &self,
        template: &str,
        width: usize,
        mtime: &dyn Fn(&str) -> u64,
    ) -> Result<Vec<Candidate>, String> {
        // `org-roam-node-list`: by `ID`, as SQLite groups them; a node's
        // aliases and tags once each, in order.
        let mut nodes: Vec<&(String, Node)> = self.nodes.iter().collect();
        nodes.sort_by_key(|(_, node)| printed(&node.id));
        let mut lines: Vec<(u64, Candidate)> = Vec::new();
        let processed = process_display_format(template);
        for (file, node) in nodes {
            let mut aliases: Vec<&str> =
                self.aliases.iter().filter(|(id, _)| *id == node.id).map(|(_, alias)| alias.as_str()).collect();
            aliases.sort_by_key(|alias| printed(alias));
            aliases.dedup();
            let mut tags: Vec<&str> =
                self.tags.iter().filter(|(id, _)| *id == node.id).map(|(_, tag)| tag.as_str()).collect();
            tags.sort_by_key(|tag| printed(tag));
            tags.dedup();
            let file_title = self.files.iter().find(|(name, _)| name == file).map_or("", |(_, title)| title.as_str());
            for title in std::iter::once(node.title.as_str()).chain(aliases.iter().copied()) {
                let fields = Fields { node, file, file_title, title, aliases: &aliases, tags: &tags };
                let display = format_entry(&processed, &fields, width)?;
                lines.push((mtime(file), Candidate { display, id: node.id.clone(), title: title.to_string() }));
            }
        }
        // `org-roam-node-read-sort-by-file-mtime`, a stable sort.
        lines.sort_by_key(|line| std::cmp::Reverse(line.0));
        Ok(lines.into_iter().map(|(_, candidate)| candidate).collect())
    }
}

/// A string as emacsql stores it, printed as Emacs Lisp prints it: what
/// SQLite sorts by.
fn printed(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// `char-width`: a tab is `tab-width` columns.
fn char_width(c: char) -> usize {
    if c == '\t' {
        8
    } else {
        unicode_width::UnicodeWidthChar::width(c).unwrap_or(0)
    }
}

/// What a template's fields read of a node.
struct Fields<'a> {
    node: &'a Node,
    file: &'a str,
    file_title: &'a str,
    title: &'a str,
    aliases: &'a [&'a str],
    tags: &'a [&'a str],
}

impl Fields<'_> {
    /// `org-roam-node-FIELD`, as a list of values.
    fn get(&self, field: &str) -> Result<Vec<String>, String> {
        let one = |value: &str| vec![value.to_string()];
        let maybe = |value: &Option<String>| value.iter().cloned().collect::<Vec<_>>();
        Ok(match field {
            "title" => one(self.title),
            "id" => one(&self.node.id),
            "file" => one(self.file),
            "file-title" => one(self.file_title),
            "todo" => maybe(&self.node.todo),
            "scheduled" => maybe(&self.node.scheduled),
            "deadline" => maybe(&self.node.deadline),
            "olp" if self.node.olp.is_empty() => Vec::new(),
            "olp" => one(&self.node.olp.join(" > ")),
            "tags" => self.tags.iter().map(|tag| tag.to_string()).collect(),
            "aliases" => self.aliases.iter().map(|alias| alias.to_string()).collect(),
            "slug" => one(&slugify(self.title)),
            "category" => self
                .node
                .properties
                .iter()
                .find(|(key, _)| key == "CATEGORY")
                .and_then(|(_, value)| value.clone())
                .into_iter()
                .collect(),
            "level" => one(&self.node.level.to_string()),
            "priority" => self.node.priority.map(|value| value.to_string()).into_iter().collect(),
            other => return Err(format!("Symbol’s function definition is void: org-roam-node-{other}")),
        })
    }
}

/// `org-roam-node-template-prefixes`
fn template_prefix(field: &str) -> &'static str {
    match field {
        "tags" => "#",
        "todo" => "t:",
        _ => "",
    }
}

/// The `${field}` and `${field=default}` of a template.
static FIELD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$\{([^}]+)\}").expect("a valid regex"));

/// `org-roam-node--process-display-format`: the template and the width it
/// takes besides its fields of any width, as org-roam counts it: the
/// widths given, and the template with each field replaced by the sum of
/// the widths given so far.
fn process_display_format(template: &str) -> (String, usize) {
    let mut fields_width = 0usize;
    let shown = FIELD.replace_all(template, |found: &regex::Captures| {
        let field = found[1].split_once('=').map_or(&found[1], |(name, _)| name);
        let width = field.split(':').nth(1).map_or(0, |width| leading_number(width).max(0) as usize);
        fields_width += width;
        fields_width.to_string()
    });
    let string_width = shown.chars().map(char_width).sum::<usize>();
    (template.to_string(), fields_width + string_width)
}

/// `string-to-number`: the number at the start of `text`, or 0.
fn leading_number(text: &str) -> i64 {
    let text = text.trim_start();
    let end = text
        .char_indices()
        .find(|(at, c)| !(c.is_ascii_digit() || (*at == 0 && matches!(c, '-' | '+'))))
        .map_or(text.len(), |(at, _)| at);
    text[..end].parse().unwrap_or(0)
}

/// `org-roam-node--format-entry`: the line for a node, what a completion
/// shows of it (the part a width cuts off is invisible).
fn format_entry(processed: &(String, usize), fields: &Fields, width: usize) -> Result<String, String> {
    let (template, template_width) = processed;
    let mut error = None;
    let line = FIELD.replace_all(template, |found: &regex::Captures| {
        let field = found[1].split_once('=').map_or(&found[1], |(name, _)| name);
        let mut parts = field.split(':');
        let name = parts.next().unwrap_or("");
        let field_width = parts.next();
        let values = match fields.get(name) {
            Ok(values) => values,
            Err(message) => {
                error.get_or_insert(message);
                return String::new();
            }
        };
        let prefix = template_prefix(name);
        let value = values.iter().map(|value| format!("{prefix}{value}")).collect::<Vec<_>>().join(" ");
        let field_width = match field_width {
            None => None,
            Some("*") => Some(width.saturating_sub(*template_width)),
            Some(text) => usize::try_from(leading_number(text)).ok(),
        };
        let Some(field_width) = field_width else { return value };
        let truncated = truncate_to_width(&value, field_width);
        let (truncated_len, len) = (truncated.chars().count(), value.chars().count());
        if truncated_len < len {
            value.chars().take(truncated_len).collect()
        } else {
            truncated
        }
    });
    match error {
        Some(message) => Err(message),
        None => Ok(line.into_owned()),
    }
}

/// `truncate-string-to-width` with `?\s` as padding: the characters that
/// fit in `width` columns, then spaces up to it.
fn truncate_to_width(text: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = char_width(c);
        if used + w > width {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push_str(&" ".repeat(width.saturating_sub(used)));
    out
}
