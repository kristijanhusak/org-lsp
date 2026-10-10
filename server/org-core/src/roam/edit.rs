//! org-roam's editing of the node at the cursor: its aliases, tags and
//! references added and removed (`org-roam-alias-add` and the like), and
//! `roam:` links turned into `id:` links (`org-roam-link-replace-all`).

use std::sync::LazyLock;

use regex::Regex;

use crate::outline::Outline;
use crate::properties::{self, Context};
use crate::tags;
use crate::todo::headline_level;
use crate::Refusal;

/// The level of the headline whose subtree holds `at`, 0 before the first
/// (`org-outline-level`).
fn outline_level(text: &str, at: usize) -> usize {
    properties::heading_at(text, at).map_or(0, |heading| level_of(text, heading))
}

fn level_of(text: &str, heading: usize) -> usize {
    let end = text[heading..].find('\n').map_or(text.len(), |newline| heading + newline);
    headline_level(&text[heading..end]).unwrap_or(0)
}

/// `org-roam-up-heading-or-point-min`: the parent headline, or the start
/// of the file.
fn up(text: &str, at: usize) -> usize {
    let Some(heading) = properties::heading_at(text, at) else { return 0 };
    let level = level_of(text, heading);
    if level <= 1 {
        return 0;
    }
    // `org-up-heading-safe`: the nearest headline above of a lower level.
    let mut start = heading;
    while start > 0 {
        let Some(above) = properties::heading_at(text, start - 1) else { return 0 };
        if level_of(text, above) < level {
            return above;
        }
        start = above;
    }
    0
}

/// `org-id-get`: the `ID` of the entry at `at`, when it is not blank.
fn id_at(text: &str, at: usize, context: &Context) -> Option<String> {
    properties::entry_get(text, at, "ID", context).filter(|id| id.chars().any(|c| !c.is_whitespace()))
}

/// The node at the cursor, as `org-roam-node-at-point` finds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeAt {
    pub id: String,
    /// The start of its headline, 0 for the file.
    pub heading: usize,
    /// Where the search stopped: the cursor, when its entry is the node.
    pub point: usize,
}

/// `org-roam-node-at-point` with ASSERT: the node that holds `at`. Each
/// entry up from `at` is tried until one with an `ID` and no
/// `ROAM_EXCLUDE` (`org-roam-db-node-p`). Edits go to where the database
/// has the node of that `ID` (`org-roam-node-point`), when it has it.
pub fn node_at(text: &str, at: usize, context: &Context) -> Result<NodeAt, Refusal> {
    let mut point = at.min(text.len());
    loop {
        let is_node = id_at(text, point, context).is_some()
            && properties::entry_get(text, point, "ROAM_EXCLUDE", context).is_none();
        if is_node || point == 0 || outline_level(text, point) == outline_level(text, up(text, point)) {
            break;
        }
        point = up(text, point);
    }
    let id = id_at(text, point, context).ok_or_else(|| Refusal::Failed("No node at point".to_string()))?;
    Ok(NodeAt { id, heading: properties::heading_at(text, point).unwrap_or(0), point })
}

/// `combine-and-quote-strings`: the values with spaces between, those
/// with a space, a backslash or a double quote quoted.
pub fn combine_and_quote(values: &[String]) -> String {
    values
        .iter()
        .map(|value| {
            if value.contains([' ', '\\', '"']) {
                format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
            } else {
                value.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The values of `name` at the node at `point`, as
/// `split-string-and-unquote` reads them.
pub fn property_values(text: &str, point: usize, name: &str, context: &Context) -> Result<Vec<String>, String> {
    match properties::entry_get(text, point, name, context) {
        Some(value) => super::split_and_unquote(&value),
        None => Ok(Vec::new()),
    }
}

/// `org-set-property`: the text unchanged when the value is the same.
fn set_property(text: &str, point: usize, name: &str, value: &str, context: &Context) -> Result<String, String> {
    if properties::entry_get(text, point, name, context).as_deref() == Some(value) {
        return Ok(text.to_string());
    }
    properties::put(text, point, name, value, context)
}

/// `org-roam-property-add`: `value` first among the values of `name`.
pub fn property_add(text: &str, point: usize, name: &str, value: &str, context: &Context) -> Result<String, String> {
    let mut values = vec![value.to_string()];
    for other in property_values(text, point, name, context)? {
        if !values.contains(&other) {
            values.push(other);
        }
    }
    set_property(text, point, name, &combine_and_quote(&values), context)
}

/// `org-roam-property-remove`: `value` taken out of the values of `name`,
/// and the property when none is left; with what Emacs says then.
pub fn property_remove(
    text: &str,
    point: usize,
    name: &str,
    value: &str,
    context: &Context,
) -> Result<(String, Option<String>), String> {
    let mut values = property_values(text, point, name, context)?;
    values.retain(|other| other != value);
    if !values.is_empty() {
        return Ok((set_property(text, point, name, &combine_and_quote(&values), context)?, None));
    }
    // `org-delete-property`, which says so whatever was there.
    let (edited, _, _) = properties::delete(text, point, name);
    Ok((edited, Some(format!("Property \"{name}\" deleted"))))
}

/// The tags `org-roam-tag-add` and `org-roam-tag-remove` work on at
/// `point`: the file's first `#+filetags:`, split at its colons, before
/// the first headline (`None` without one); else the headline's tags,
/// inherited ones too (`org-get-tags`).
pub fn node_tags(text: &str, outline: &Outline, point: usize) -> Option<Vec<String>> {
    let Some(heading) = properties::heading_at(text, point) else {
        let root = org_syntax::parse(text, &outline.todo).syntax();
        let first = crate::outline::keyword_values(&root, "FILETAGS").into_iter().next()?;
        return Some(first.split(':').filter(|tag| !tag.is_empty()).map(str::to_string).collect());
    };
    let entry = outline.entries.iter().find(|entry| entry.offset == heading)?;
    Some(entry.tags.iter().map(|tag| tag.name.clone()).collect())
}

/// `org-roam-tag-add`: `added` before the node's tags, each once, in
/// `#+filetags:` for the file's node, else on the headline (`org-set-tags`).
pub fn tag_add(text: &str, outline: &Outline, point: usize, added: &[String], settings: &tags::Settings) -> String {
    let current = node_tags(text, outline, point).unwrap_or_default();
    let mut tags: Vec<String> = Vec::new();
    for tag in added.iter().chain(&current) {
        if !tags.contains(tag) {
            tags.push(tag.clone());
        }
    }
    set_node_tags(text, point, &tags, settings)
}

/// `org-roam-tag-remove`: `removed` taken out of the node's tags.
pub fn tag_remove(
    text: &str,
    outline: &Outline,
    point: usize,
    removed: &[String],
    settings: &tags::Settings,
) -> Result<String, Refusal> {
    let current = node_tags(text, outline, point)
        .filter(|tags| outline_level(text, point) == 0 || !tags.is_empty())
        .ok_or_else(|| Refusal::Failed("No tag to remove".to_string()))?;
    let tags: Vec<String> = current.into_iter().filter(|tag| !removed.contains(tag)).collect();
    Ok(set_node_tags(text, point, &tags, settings))
}

fn set_node_tags(text: &str, point: usize, tags: &[String], settings: &tags::Settings) -> String {
    let Some(heading) = properties::heading_at(text, point) else {
        return set_keyword(text, "filetags", &tags::tag_string(tags));
    };
    // `org-set-tags` keeps the order it is given.
    let settings = tags::Settings { sort: None, ..settings.clone() };
    tags::set_tags(text, heading, tags, &settings)
}

/// The start of the line after the one at `at`, or the end of the text
/// (`forward-line`).
fn next_line(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |newline| at + newline + 1)
}

fn line_at(text: &str, at: usize) -> &str {
    let end = text[at..].find('\n').map_or(text.len(), |newline| at + newline);
    &text[at..end]
}

/// `org-roam-end-of-meta-data` from the start of the file, with FULL
/// `drawers`: past its headline, planning line and property drawer, then
/// blank and clock lines and drawers.
fn end_of_meta_data(text: &str) -> usize {
    end_of_meta_data_from(text, 0, false)
}

/// `org-roam-end-of-meta-data` from `at`, with FULL non-nil: from the
/// headline whose entry holds `at` (or the start of the file), past it,
/// its planning line and property drawer, blank and clock lines, drawers,
/// and with `keywords` (FULL `t`) keyword lines too.
pub(crate) fn end_of_meta_data_from(text: &str, at: usize, keywords: bool) -> usize {
    static PLANNING: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^[ \t]*(SCHEDULED:|DEADLINE:|CLOSED:)").expect("a valid regex"));
    static PROPERTY_DRAWER: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^[ \t]*:PROPERTIES:[ \t]*\n(?:[ \t]*:\S+:(?:[ \t].*)?[ \t]*\n)*?[ \t]*:END:[ \t]*$")
            .expect("a valid regex")
    });
    static SKIPPED: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(?:[ \t]*$|[ \t]*CLOCK:)").expect("a valid regex"));
    static DRAWER: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^[ \t]*:(?:\w|[-_])+:[ \t]*$").expect("a valid regex"));
    static END: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?im)^[ \t]*:END:[ \t]*$").expect("a valid regex"));
    static KEYWORD: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^[ \t]*#\+(\S+?):[ \t]*(.*)$").expect("a valid regex"));
    let is_heading = |at: usize| headline_level(line_at(text, at)).is_some();
    // `org-back-to-heading-or-point-min`
    let mut point = properties::heading_at(text, at.min(text.len())).unwrap_or(0);
    if is_heading(point) {
        point = next_line(text, point);
    }
    if PLANNING.is_match(line_at(text, point)) {
        point = next_line(text, point);
    }
    if let Some(found) = PROPERTY_DRAWER.find(&text[point..]) {
        point = next_line(text, point + found.end());
    }
    if is_heading(point) {
        return point;
    }
    // The next headline bounds the drawers.
    let mut bound = text.len();
    let mut line = point;
    while line < text.len() {
        if is_heading(line) {
            bound = line;
            break;
        }
        line = next_line(text, line);
    }
    while point < text.len() {
        let current = line_at(text, point);
        if SKIPPED.is_match(current) {
            point = next_line(text, point);
        } else if DRAWER.is_match(current) {
            // `org-logbook-drawer-re` is a drawer too.
            match END.find_at(&text[..bound], next_line(text, point).min(bound)) {
                Some(end) => point = next_line(text, end.end()),
                None => break,
            }
        } else if keywords && KEYWORD.is_match(current) {
            point = next_line(text, point);
        } else {
            break;
        }
    }
    point
}

/// `org-roam-set-keyword`: the value of the first `#+KEY:` line replaced,
/// the line taken out for a blank value; without one, a line put after
/// the file's meta data.
pub fn set_keyword(text: &str, key: &str, value: &str) -> String {
    let pattern = Regex::new(&format!(r"(?im)^#\+{}:(.*)", regex::escape(key))).expect("a valid regex");
    if let Some(captures) = pattern.captures(text) {
        let line = captures.get(0).expect("a match");
        let group = captures.get(1).expect("a group");
        if value.chars().all(char::is_whitespace) {
            // `kill-whole-line`
            let (start, end) = (line.start(), line.end());
            return if end < text.len() {
                format!("{}{}", &text[..start], &text[end + 1..])
            } else if start > 0 {
                text[..start - 1].to_string()
            } else {
                String::new()
            };
        }
        return format!("{} {value}{}", &text[..group.start()], &text[group.end()..]);
    }
    let point = end_of_meta_data(text);
    let line = format!("#+{key}: {value}\n");
    let line_end = text[point..].find('\n').map_or(text.len(), |newline| point + newline);
    if line_end == text.len() {
        format!("{text}\n{line}")
    } else {
        let at = line_end + 1;
        format!("{}{line}{}", &text[..at], &text[at..])
    }
}

/// `org-roam-link-replace-all`: each `[[roam:TITLE]]` link whose title or
/// alias is one node's turned into `[[id:ID][TITLE]]`, keeping its
/// description. `lookup` gives the `ID` of the node with a title or
/// alias, or fails as `org-roam-node-from-title-or-alias` does when
/// several have it; the links before such a one are replaced, and the
/// failure comes with the text.
pub fn replace_links(
    text: &str,
    parse: &org_syntax::Parse,
    lookup: &dyn Fn(&str) -> Result<Option<String>, String>,
) -> (String, Option<String>) {
    let mut out = String::new();
    let mut copied = 0;
    for (start, end, path, description) in roam_links(text, parse) {
        let id = match lookup(&path) {
            Ok(Some(id)) => id,
            Ok(None) => continue,
            Err(error) => {
                out.push_str(&text[copied..]);
                return (out, Some(error));
            }
        };
        let shown = description.unwrap_or_else(|| path.clone());
        let Ok(link) = crate::link::make_string(&format!("id:{id}"), Some(&shown)) else { continue };
        out.push_str(&text[copied..start]);
        out.push_str(&link);
        copied = end;
    }
    out.push_str(&text[copied..]);
    (out, None)
}

/// The bracket links of type `roam`: where each is, its title and its
/// description.
pub fn roam_links(text: &str, parse: &org_syntax::Parse) -> Vec<(usize, usize, String, Option<String>)> {
    use org_syntax::{SyntaxKind, SyntaxNode};
    let is_link = |node: &SyntaxNode| node.kind() == SyntaxKind::LINK;
    let mut out = Vec::new();
    for node in parse.syntax().descendants() {
        if !is_link(&node) || node.ancestors().skip(1).any(|above| is_link(&above)) {
            continue;
        }
        let (start, end) = (usize::from(node.text_range().start()), usize::from(node.text_range().end()));
        let raw = &text[start..end];
        let Some(inner) = raw.strip_prefix("[[").and_then(|rest| rest.strip_suffix("]]")) else { continue };
        if !inner.starts_with("roam:") {
            continue;
        }
        let (path, description) = match super::bracket_path_end(inner) {
            Some(at) => (&inner["roam:".len()..at], inner[at..].strip_prefix("][").map(str::to_string)),
            None => (&inner["roam:".len()..], None),
        };
        let path = super::unescape(path).replace('\n', " ");
        out.push((start, end, path, description));
    }
    out
}

/// Whether `offset` is in a source block (`org-in-src-block-p`).
pub fn in_src_block(parse: &org_syntax::Parse, offset: usize) -> bool {
    parse.syntax().descendants().any(|node| {
        let (start, end) = (usize::from(node.text_range().start()), usize::from(node.text_range().end()));
        node.kind() == org_syntax::SyntaxKind::BLOCK
            && start <= offset
            && offset <= end
            && node
                .text()
                .to_string()
                .trim_start()
                .get(..11)
                .is_some_and(|begin| begin.eq_ignore_ascii_case("#+begin_src"))
    })
}

/// Whether `offset` is in a link (`org-in-regexp org-link-any-re`).
pub fn in_link(parse: &org_syntax::Parse, offset: usize) -> bool {
    use org_syntax::SyntaxKind;
    [SyntaxKind::LINK, SyntaxKind::PLAIN_LINK, SyntaxKind::ANGLE_LINK]
        .into_iter()
        .any(|kind| in_node(parse, offset, kind))
}

fn in_node(parse: &org_syntax::Parse, offset: usize, kind: org_syntax::SyntaxKind) -> bool {
    parse.syntax().descendants().any(|node| {
        let (start, end) = (usize::from(node.text_range().start()), usize::from(node.text_range().end()));
        node.kind() == kind && start <= offset && offset <= end
    })
}
