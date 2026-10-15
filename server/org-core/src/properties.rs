//! Properties of an entry, as Org's commands change them: a port of
//! `org-entry-put` and `org-entry-delete`, of what `org-set-property`
//! (`C-c C-x p`) offers, `org-delete-property`,
//! `org-delete-property-globally`, the allowed values of a property
//! (`NAME_ALL`) that `S-<right>` and `S-<left>` go through
//! (`org-property-next-allowed-value`), `org-set-effort` (`C-c C-x e`),
//! `org-inc-effort` and `org-toggle-ordered-property`.
//!
//! The entry is the headline at or above the position, or the file's own
//! property drawer before the first headline. `TODO` and `PRIORITY` are
//! set by the commands that change them; the others of Org's special
//! properties cannot be set.

use std::sync::LazyLock;

use org_syntax::TodoKeywords;
use regex::{Regex, RegexBuilder};

use crate::date::{Date, DateTime};
use crate::outline::{keyword_values, Outline};
use crate::priority::Priorities;
use crate::todo::{headline_level, Editor, Settings};

/// `org-special-properties`: those Org makes of the entry itself.
pub const SPECIAL: [&str; 14] = [
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

/// `org-default-properties`, offered for completion.
const DEFAULT: [&str; 27] = [
    "ARCHIVE",
    "CATEGORY",
    "SUMMARY",
    "DESCRIPTION",
    "CUSTOM_ID",
    "LOCATION",
    "LOGGING",
    "COLUMNS",
    "VISIBILITY",
    "TABLE_EXPORT_FORMAT",
    "TABLE_EXPORT_FILE",
    "EXPORT_OPTIONS",
    "EXPORT_TEXT",
    "EXPORT_FILE_NAME",
    "EXPORT_TITLE",
    "EXPORT_AUTHOR",
    "EXPORT_DATE",
    "UNNUMBERED",
    "ORDERED",
    "NOBLOCKING",
    "COOKIE_DATA",
    "LOG_INTO_DRAWER",
    "REPEAT_TO_STATE",
    "CLOCK_MODELINE_TOTAL",
    "STYLE",
    "HTML_CONTAINER_CLASS",
    "ORG-IMAGE-ACTUAL-WIDTH",
];

/// `org-effort-property`
pub const EFFORT: &str = "Effort";

/// `org-global-properties-fixed`, which come after the user's.
const FIXED_GLOBALS: [(&str, &str); 2] =
    [("VISIBILITY_ALL", "folded children content all"), ("CLOCK_MODELINE_TOTAL_ALL", "current today repeat all auto")];

/// What the commands need besides the text.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// The file's TODO keywords (`org-todo-keywords-1`).
    pub keywords: TodoKeywords,
    pub priorities: Priorities,
    /// `org-global-properties`
    pub globals: Vec<(String, String)>,
    /// How Org writes and indents lines.
    pub settings: Settings,
    /// The category of the file's entries without one of their own: the
    /// file's name.
    pub category: String,
}

impl Context {
    fn globals(&self) -> Vec<(String, String)> {
        let fixed = FIXED_GLOBALS.iter().map(|(name, value)| (name.to_string(), value.to_string()));
        self.globals.iter().cloned().chain(fixed).collect()
    }

    fn outline(&self, text: &str) -> Outline {
        Outline::new(&org_syntax::parse(text, &self.keywords), text, &self.category)
    }

    fn editor<'a>(&'a self, text: &str) -> Editor<'a> {
        Editor::new(text, &self.keywords, &self.settings, DateTime::new(Date::new(1970, 1, 1), 0, 0))
    }
}

/// A property line, as `org-property-re` matches it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyLine {
    /// The start of the line.
    pub start: usize,
    /// The end of the line, before its line break.
    pub end: usize,
    /// The name between the colons.
    pub key: String,
    /// The value, without blanks around it.
    pub value: String,
}

/// `org-property-re` on one line: indentation, `:KEY:`, the value.
static PROPERTY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[ \t]*:(\S+):(?:[ \t]+(.*?))?[ \t]*$").expect("a valid regex"));

fn property_line(text: &str, start: usize) -> Option<PropertyLine> {
    let end = line_end(text, start);
    let captures = PROPERTY_RE.captures(&text[start..end])?;
    Some(PropertyLine {
        start,
        end,
        key: captures[1].to_string(),
        value: captures.get(2).map_or("", |value| value.as_str()).to_string(),
    })
}

fn line_end(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |index| at + index)
}

fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |index| index + 1)
}

/// The start of the line after the one at `at`, or the end of the text.
fn next_line(text: &str, at: usize) -> usize {
    (line_end(text, at) + 1).min(text.len())
}

/// The start of every line from `from` up to `to`.
fn lines(text: &str, from: usize, to: usize) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut at = from;
    while at < to {
        starts.push(at);
        let next = next_line(text, at);
        if next == at {
            break;
        }
        at = next;
    }
    starts
}

/// `org-back-to-heading-or-point-min`: the headline at or above `offset`,
/// `None` before the first one.
pub fn heading_at(text: &str, offset: usize) -> Option<usize> {
    let mut line = line_start(text, offset.min(text.len()));
    loop {
        if headline_level(&text[line..line_end(text, line)]).is_some() {
            return Some(line);
        }
        if line == 0 {
            return None;
        }
        line = line_start(text, line - 1);
    }
}

/// `org-at-comment-p` on the line at `start`.
fn is_comment(text: &str, start: usize) -> bool {
    let line = text[start..line_end(text, start)].trim_start_matches([' ', '\t']);
    line == "#" || line.starts_with("# ")
}

/// Where the property drawer of the entry at `offset` goes: the start of
/// the line after the headline and its planning line, or after the
/// comment lines that start the file.
fn drawer_place(text: &str, heading: Option<usize>) -> usize {
    match heading {
        Some(heading) => {
            let mut at = next_line(text, heading);
            if at > heading && at < text.len() && is_planning(&text[at..line_end(text, at)]) {
                at = next_line(text, at);
            }
            // `forward-line` stops at the end of a last line without a
            // line break: there is no line after it.
            if at == text.len() && !text.ends_with('\n') {
                return text.len();
            }
            at
        }
        None => {
            let mut at = 0;
            // `(while (and (org-at-comment-p) (bolp)) (forward-line))`
            while at < text.len() && is_comment(text, at) {
                let next = next_line(text, at);
                if next == text.len() && !text.ends_with('\n') {
                    return text.len();
                }
                at = next;
            }
            at
        }
    }
}

/// `org-planning-line-re`
fn is_planning(line: &str) -> bool {
    let line = line.trim_start_matches([' ', '\t']);
    ["SCHEDULED:", "DEADLINE:", "CLOSED:"].iter().any(|keyword| line.starts_with(keyword))
}

/// `org-property-drawer-re` at the line `at`: the start of the line
/// after `:PROPERTIES:` and of the `:END:` line.
fn drawer_at(text: &str, at: usize) -> Option<(usize, usize)> {
    if at >= text.len() {
        return None;
    }
    let first = text[at..line_end(text, at)].trim_matches([' ', '\t']);
    if !first.eq_ignore_ascii_case(":PROPERTIES:") || line_end(text, at) == text.len() {
        return None;
    }
    let begin = next_line(text, at);
    let mut line = begin;
    loop {
        let content = &text[line..line_end(text, line)];
        if content.trim_matches([' ', '\t']).eq_ignore_ascii_case(":END:") {
            return Some((begin, line));
        }
        // `[ \t]*:\S-+:\(?:[ \t].*\)?[ \t]*\n`
        let trimmed = content.trim_start_matches([' ', '\t']);
        let valid = trimmed.strip_prefix(':').is_some_and(|rest| {
            rest.char_indices().any(|(index, c)| {
                c == ':'
                    && index > 0
                    && !rest[..index].contains(char::is_whitespace)
                    && rest[index + 1..].chars().next().is_none_or(|c| c == ' ' || c == '\t')
            })
        });
        if !valid || line_end(text, line) == text.len() {
            return None;
        }
        line = next_line(text, line);
    }
}

/// `org-get-property-block`: the body of the property drawer of the
/// entry at `offset`, from the start of its first line to the start of
/// its `:END:` line.
pub fn block(text: &str, offset: usize) -> Option<(usize, usize)> {
    drawer_at(text, drawer_place(text, heading_at(text, offset)))
}

/// Whether `offset` is in the property drawer of its entry, from its
/// `:PROPERTIES:` line to its `:END:` line: where `C-c C-c` is
/// `org-property-action`.
pub fn in_drawer(text: &str, offset: usize) -> bool {
    let start = line_start(text, offset.min(text.len()));
    let place = drawer_place(text, heading_at(text, offset));
    drawer_at(text, place).is_some_and(|(_, end)| start >= place && start <= end)
}

/// `org-at-property-p`: the property line at `offset`, in the property
/// drawer of its entry.
pub fn property_at(text: &str, offset: usize) -> Option<PropertyLine> {
    let start = line_start(text, offset.min(text.len()));
    let line = property_line(text, start)?;
    let (begin, end) = block(text, start)?;
    (start >= begin && start < end).then_some(line)
}

/// `org-re-property` for `name` (quoted), with or without a value
/// (`allow_null`), case folded, on whole lines; `extended` also matches
/// `NAME+`.
fn property_regex(name: &str, allow_null: bool, extended: bool) -> Regex {
    let plus = if extended { r"\+?" } else { "" };
    let value = if allow_null { r"(?:[ \t]+.*?)?" } else { r"[ \t]+[^ \r\t\n]+.*?" };
    RegexBuilder::new(&format!(r"^[ \t]*:{}{plus}:{value}[ \t]*$", regex::escape(name)))
        .case_insensitive(true)
        .multi_line(true)
        .build()
        .expect("a valid regex")
}

/// `org-entry-get` without inheritance: the value of `name` in the entry
/// at `offset`, its `NAME+` values after it.
pub fn local_value(text: &str, offset: usize, context: &Context) -> impl Fn(&str) -> Option<String> {
    let outline = context.outline(text);
    let entry = heading_at(text, offset).and_then(|heading| outline.entry_at(heading));
    move |name: &str| outline.entry_property(entry, name, false)
}

/// `org-entry-get` with inheritance: the value of `name` in the entry at
/// `offset`, or the nearest one above it that has it, or the file's.
pub fn inherited(text: &str, offset: usize, name: &str, context: &Context) -> Option<String> {
    let outline = context.outline(text);
    let entry = heading_at(text, offset).and_then(|heading| outline.entry_at(heading));
    outline.entry_property(entry, name, true)
}

/// `org-entry-get` without inheritance, `TODO`, `PRIORITY`, `ITEM` and
/// `CATEGORY` as `org-entry-properties` has them: the headline's keyword,
/// its priority or else the default one, its title, the category.
pub fn entry_get(text: &str, offset: usize, name: &str, context: &Context) -> Option<String> {
    let heading = heading_at(text, offset);
    match (name, heading) {
        ("TODO", Some(heading)) => {
            let outline = context.outline(text);
            outline.entry_at(heading).and_then(|entry| outline.entries[entry].todo.clone())
        }
        ("PRIORITY", Some(heading)) => {
            static COOKIE: LazyLock<Regex> =
                LazyLock::new(|| Regex::new(r"^.*?\[#([A-Z0-9]+)\]").expect("a valid regex"));
            let line = &text[heading..line_end(text, heading)];
            Some(match COOKIE.captures(line) {
                Some(captures) => captures[1].to_string(),
                None => priority_string(context.priorities.default),
            })
        }
        ("ITEM", Some(heading)) => {
            let line = &text[heading..line_end(text, heading)];
            let level = headline_level(line).unwrap_or(1);
            let title = crate::outline::complex_heading_title(line, level, &context.keywords);
            Some(title.map_or(String::new(), |(start, end)| line[start..end].to_string()))
        }
        ("CATEGORY", _) => {
            let outline = context.outline(text);
            Some(match heading.and_then(|heading| outline.entry_at(heading)) {
                Some(entry) => outline.entries[entry].category.clone(),
                None => outline.category.clone(),
            })
        }
        _ => local_value(text, offset, context)(name),
    }
}

/// `org-entry-put` for a property that is not special: `name` set to
/// `value` in the property drawer of the entry at `offset`, which is made
/// when there is none.
pub fn put(text: &str, offset: usize, name: &str, value: &str, context: &Context) -> Result<String, String> {
    if !valid_name(name) {
        return Err(format!("Invalid property name: \"{name}\""));
    }
    if SPECIAL.contains(&name) {
        return Err(format!("The {name} property cannot be set with ‘org-entry-put’"));
    }
    let mut editor = context.editor(text);
    let heading = heading_at(text, offset);
    let (begin, end) = match drawer_at(text, drawer_place(text, heading)) {
        Some(found) => found,
        None => insert_drawer(&mut editor, heading),
    };
    let pattern = property_regex(name, true, false);
    let found = pattern.find_at(&editor.text, begin).filter(|found| found.end() <= end).map(|found| found.range());
    let line = match found {
        Some(found) => {
            editor.replace(found.start, found.end, "");
            found.start
        }
        None => {
            editor.replace(end, end, "\n");
            end
        }
    };
    editor.replace(line, line, &format!(":{name}: {value}"));
    editor.indent_line(line);
    Ok(editor.text)
}

/// `org-insert-property-drawer`, in the entry of `heading` (the file's
/// before the first headline): the body of the new drawer.
fn insert_drawer(editor: &mut Editor, heading: Option<usize>) -> (usize, usize) {
    let mut at = drawer_place(&editor.text, heading);
    // `(when (and (bolp) (> (point) (point-min))) (backward-char))`
    if at > 0 && editor.text.as_bytes()[at - 1] == b'\n' {
        at -= 1;
    }
    let begin = if at == 0 { 0 } else { at + 1 };
    let mut drawer = if at == 0 { String::new() } else { "\n".to_string() };
    drawer.push_str(":PROPERTIES:\n:END:");
    if at == editor.text.len() || begin == 0 {
        drawer.push('\n');
    }
    editor.replace(at, at, &drawer);
    editor.indent_region(begin, at + drawer.len());
    drawer_at(&editor.text, drawer_place(&editor.text, heading)).expect("the new drawer")
}

/// `org--valid-property-p`
pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(char::is_whitespace)
}

/// `org-entry-delete`: `name` and `NAME+` taken out of the property drawer
/// of the entry at `offset`, and the drawer itself when it is left
/// empty. The new text, the ranges taken out (in order, each in the text
/// as it was then), and whether a property went.
pub fn delete(text: &str, offset: usize, name: &str) -> (String, Vec<(usize, usize)>, bool) {
    let mut text = text.to_string();
    let mut removed = Vec::new();
    let Some((begin, origin)) = block(&text, offset) else { return (text, removed, false) };
    let pattern = property_regex(name, true, true);
    let mut end = origin;
    let mut point = begin;
    while let Some(found) = pattern.find_at(&text, point).filter(|found| found.end() <= end).map(|found| found.range())
    {
        let to = next_line(&text, found.start).max(found.end);
        text.replace_range(found.start..to, "");
        removed.push((found.start, to));
        end -= to - found.start;
        point = found.start;
    }
    if begin == end {
        // The `:PROPERTIES:` line and the `:END:` line.
        let from = line_start(&text, point - 1);
        let to = next_line(&text, point);
        text.replace_range(from..to, "");
        removed.push((from, to));
    }
    (text, removed, end != origin)
}

/// Where a position goes when the ranges `removed` are taken out in turn:
/// what was in one goes to its start, as with a marker.
fn after_removal(mut at: usize, removed: &[(usize, usize)]) -> usize {
    for (start, end) in removed {
        if at >= *end {
            at -= end - start;
        } else if at > *start {
            at = *start;
        }
    }
    at
}

/// `org-delete-property-globally`: `name` taken out of every entry; the
/// new text and how many entries had it.
pub fn delete_globally(text: &str, name: &str) -> (String, usize) {
    let mut text = text.to_string();
    let pattern = property_regex(name, true, true);
    let mut count = 0;
    let mut point = 0;
    while let Some(found) = pattern.find_at(&text, point) {
        let at = found.end();
        let (new, removed, any) = delete(&text, at, name);
        text = new;
        count += usize::from(any);
        point = after_removal(at, &removed);
        if point >= text.len() {
            break;
        }
    }
    (text, count)
}

/// `org-property-values`: the values `name` has in the file, the last
/// first.
pub fn values(text: &str, name: &str, context: &Context) -> Vec<String> {
    let outline = context.outline(text);
    let pattern = property_regex(name, false, false);
    let mut values: Vec<String> = Vec::new();
    for found in pattern.find_iter(text) {
        let entry = heading_at(text, found.end()).and_then(|heading| outline.entry_at(heading));
        if let Some(value) = outline.entry_property(entry, name, false) {
            values.insert(0, value);
        }
    }
    let mut seen: Vec<String> = Vec::new();
    for value in values {
        if !seen.contains(&value) {
            seen.push(value);
        }
    }
    seen
}

/// `org-buffer-property-keys`: the names of the properties of the file,
/// with Org's special ones (`specials`), those Org uses (`defaults`) and
/// those of column formats (`columns`), sorted without regard to case.
pub fn names(text: &str, specials: bool, defaults: bool, columns: bool, context: &Context) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    if specials {
        found.extend(SPECIAL.iter().map(|name| name.to_string()));
    }
    if defaults {
        found.push(EFFORT.to_string());
        found.extend(DEFAULT.iter().map(|name| name.to_string()));
    }
    let parse = org_syntax::parse(text, &context.keywords);
    let root = parse.syntax();
    for value in keyword_values(&root, "PROPERTY") {
        if let Some(name) = value.split_whitespace().next() {
            found.push(name.to_string());
        }
    }
    // The drawers of the file and of each entry.
    let mut places = vec![None];
    places.extend(
        lines(text, 0, text.len())
            .into_iter()
            .filter(|at| {
                (*at == 0 || text.as_bytes()[at - 1] == b'\n')
                    && headline_level(&text[*at..line_end(text, *at)]).is_some()
            })
            .map(Some),
    );
    for heading in places {
        let Some((begin, end)) = drawer_at(text, drawer_place(text, heading)) else { continue };
        for line in lines(text, begin, end) {
            if let Some(property) = property_line(text, line) {
                let key = property.key.strip_suffix('+').unwrap_or(&property.key).to_string();
                found.insert(0, key);
            }
        }
    }
    if columns {
        let format = Regex::new(r"%[0-9]*([[:alnum:]_-]+)(?:\([^)]+\))?(?:\{[^}]+\})?").expect("a valid regex");
        let line_re = RegexBuilder::new(r"^[ \t]*(?:#\+|:)COLUMNS:(.*)$")
            .case_insensitive(true)
            .multi_line(true)
            .build()
            .expect("a valid regex");
        for captures in line_re.captures_iter(text) {
            let whole = captures.get(0).expect("the match");
            let line = whole.as_str().trim_start_matches([' ', '\t']);
            let value = if line.starts_with("#+") {
                captures[1].trim().to_string()
            } else {
                match property_at(text, whole.start()) {
                    Some(property) => property.value,
                    None => continue,
                }
            };
            for name in format.captures_iter(&value) {
                let name = name[1].to_string();
                if !SPECIAL.iter().any(|special| special.eq_ignore_ascii_case(&name)) {
                    found.insert(0, name);
                }
            }
        }
    }
    let bare: Vec<String> = found
        .iter()
        .filter(|name| name.len() > 4 && name.ends_with("_ALL"))
        .map(|name| name[..name.len() - 4].to_string())
        .collect();
    found.extend(bare);
    let mut names: Vec<String> = Vec::new();
    for name in found {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names.sort_by_key(|name| name.to_uppercase());
    names
}

/// The allowed values of a property, and whether others may be typed too
/// (`:ETC`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Allowed {
    pub values: Vec<String>,
    pub unrestricted: bool,
}

/// `org-property-get-allowed-values` at `offset`: the keywords for `TODO`
/// (and none), the priorities, or the words of `NAME_ALL`, inherited.
///
/// A `NAME_ALL` that is there but blank is no list of words to Org: it
/// fails with Lisp's error.
pub fn allowed(text: &str, offset: usize, name: &str, context: &Context) -> Result<Allowed, String> {
    let mut values: Vec<String> = match name {
        "TODO" => context.keywords.names().map(str::to_string).chain([String::new()]).collect(),
        "PRIORITY" => (context.priorities.highest..=context.priorities.lowest).map(priority_string).collect(),
        "CATEGORY" => Vec::new(),
        name if SPECIAL.contains(&name) => Vec::new(),
        name => {
            let outline = context.outline(text);
            let entry = heading_at(text, offset).and_then(|heading| outline.entry_at(heading));
            let words = outline.entry_property_with(entry, &format!("{name}_ALL"), true, &context.globals());
            match words {
                Some(words) if words.contains(|c: char| !c.is_whitespace()) => read_words(&words),
                Some(words) => return Err(format!("Wrong type argument: listp, {words:?}")),
                None => Vec::new(),
            }
        }
    };
    let unrestricted = values.iter().any(|value| value == ":ETC");
    values.retain(|value| value != ":ETC");
    Ok(Allowed { values, unrestricted })
}

/// `org-priority-to-string`
pub(crate) fn priority_string(value: i64) -> String {
    if value < 65 {
        value.to_string()
    } else {
        u32::try_from(value).ok().and_then(char::from_u32).map_or_else(String::new, String::from)
    }
}

/// The words of `NAME_ALL` as Lisp reads them: strings in double quotes,
/// numbers written again as Emacs writes numbers, symbols; a list or a
/// vector is `???`.
fn read_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        if c.is_whitespace() {
            at += 1;
        } else if c == ';' {
            while at < chars.len() && chars[at] != '\n' {
                at += 1;
            }
        } else if c == '"' {
            let mut word = String::new();
            at += 1;
            while at < chars.len() && chars[at] != '"' {
                if chars[at] == '\\' && at + 1 < chars.len() {
                    at += 1;
                    match chars[at] {
                        'n' => word.push('\n'),
                        't' => word.push('\t'),
                        '\n' | ' ' => {}
                        other => word.push(other),
                    }
                } else {
                    word.push(chars[at]);
                }
                at += 1;
            }
            at += 1;
            words.push(word);
        } else if c == '(' || c == '[' {
            let close = if c == '(' { ')' } else { ']' };
            let mut depth = 0;
            while at < chars.len() {
                if chars[at] == c {
                    depth += 1;
                } else if chars[at] == close {
                    depth -= 1;
                    if depth == 0 {
                        at += 1;
                        break;
                    }
                }
                at += 1;
            }
            words.push("???".to_string());
        } else {
            let mut word = String::new();
            while at < chars.len() && !chars[at].is_whitespace() && !"()[]\";".contains(chars[at]) {
                if chars[at] == '\\' && at + 1 < chars.len() {
                    at += 1;
                }
                word.push(chars[at]);
                at += 1;
            }
            words.push(lisp_number(&word).unwrap_or(word));
        }
    }
    words
}

/// A symbol that Lisp reads as a number, written as `number-to-string`
/// writes it.
fn lisp_number(word: &str) -> Option<String> {
    static INTEGER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[+-]?[0-9]+\.?$").expect("a valid regex"));
    static FLOAT: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^[+-]?(?:[0-9]*\.[0-9]+(?:e[+-]?[0-9]+)?|[0-9]+\.?e[+-]?[0-9]+)$").expect("a valid regex")
    });
    if INTEGER.is_match(word) {
        let digits = word.trim_end_matches('.').trim_start_matches('+');
        let negative = digits.starts_with('-');
        let digits = digits.trim_start_matches('-').trim_start_matches('0');
        let digits = if digits.is_empty() { "0" } else { digits };
        return Some(if negative && digits != "0" { format!("-{digits}") } else { digits.to_string() });
    }
    if FLOAT.is_match(word) {
        let value: f64 = word.parse().ok()?;
        let mut written = format!("{value}");
        if !written.contains(['.', 'e', 'i', 'N']) {
            written.push_str(".0");
        }
        return Some(written);
    }
    None
}

/// `org-property-next-allowed-value` (`previous` for
/// `org-property-previous-allowed-value`): the property on the line at
/// `offset` set to the value after (or before) its own among its allowed
/// values. The new text and where the cursor goes.
pub fn next_allowed(text: &str, offset: usize, previous: bool, context: &Context) -> Result<(String, usize), String> {
    let Some(line) = property_at(text, offset) else { return Err("Not at a property".to_string()) };
    let mut allowed = allowed(text, line.start, &line.key, context)?.values;
    // `org-heading-components`, for the effort of the clocked entry.
    if heading_at(text, offset).is_none() {
        return Err("Before first headline".to_string());
    }
    if allowed.is_empty() && ["[ ]", "[-]", "[X]"].contains(&line.value.as_str()) {
        allowed = vec!["[ ]".to_string(), "[X]".to_string()];
    }
    if allowed.is_empty() {
        return Err("Allowed values for this property have not been defined".to_string());
    }
    if previous {
        allowed.reverse();
    }
    let next = allowed
        .iter()
        .position(|value| *value == line.value)
        .and_then(|at| allowed.get(at + 1))
        .unwrap_or(&allowed[0])
        .clone();
    if next == line.value {
        return Err("Only one allowed value for this property".to_string());
    }
    let mut editor = context.editor(text);
    editor.replace(line.start, line.end, &format!(" :{}: {next}", line.key));
    editor.indent_line(line.start);
    let blanks = editor.text[line.start..].bytes().take_while(|b| *b == b' ' || *b == b'\t').count();
    Ok((editor.text, line.start + blanks))
}

/// The names `org-delete-property` offers for the entry at `offset`: its
/// properties, upper case, the last first, after `CATEGORY`.
pub fn entry_names(text: &str, offset: usize) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    if let Some((begin, end)) = block(text, offset) {
        for line in lines(text, begin, end) {
            let Some(property) = property_line(text, line) else { continue };
            let key = property.key.to_uppercase();
            let base = key.strip_suffix('+').unwrap_or(&key).to_string();
            if SPECIAL.iter().any(|special| special.eq_ignore_ascii_case(&base)) || names.contains(&base) {
                continue;
            }
            names.insert(0, base);
        }
    }
    if !names.iter().any(|name| name == "CATEGORY") {
        names.insert(0, "CATEGORY".to_string());
    }
    names
}

/// A question of the property commands, as `completing-read` asks it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub prompt: String,
    /// What completion offers.
    pub collection: Vec<String>,
    /// Only one of `collection` will do.
    pub require_match: bool,
    /// What an empty answer stands for.
    pub default: Option<String>,
}

impl Question {
    /// What the answer `typed` comes to: the default when it is empty.
    pub fn answer(&self, typed: &str) -> String {
        match &self.default {
            Some(default) if typed.is_empty() => default.clone(),
            _ => typed.to_string(),
        }
    }
}

/// `org-read-property-name`: the property's name, the one on the line at
/// `offset` or else `last` (`org-last-set-property`) by default.
pub fn name_question(text: &str, offset: usize, last: Option<&str>, context: &Context) -> Question {
    let default = property_at(text, offset).map(|line| line.key).or_else(|| last.map(str::to_string));
    Question {
        prompt: match &default {
            Some(default) => format!("Property [{default}]: "),
            None => "Property: ".to_string(),
        },
        collection: names(text, false, true, true, context),
        require_match: false,
        default,
    }
}

/// `org-read-property-value`: one of the allowed values of `name`, or any
/// value, the file's values of it offered and the entry's by default.
pub fn value_question(text: &str, offset: usize, name: &str, context: &Context) -> Result<Question, String> {
    let allowed = allowed(text, offset, name, context)?;
    let current = entry_get(text, offset, name, context);
    let prompt = match current.as_deref().filter(|current| current.contains(|c: char| !c.is_whitespace())) {
        Some(current) => format!("{name} value [{current}]: "),
        None => format!("{name} value: "),
    };
    Ok(if allowed.values.is_empty() {
        Question { prompt, collection: values(text, name, context), require_match: false, default: current }
    } else {
        Question { prompt, collection: allowed.values, require_match: !allowed.unrestricted, default: None }
    })
}

/// `org-delete-property`'s question for the entry at `offset`, or the
/// name it takes without asking when the entry offers only one.
pub fn delete_question(text: &str, offset: usize) -> Result<Question, String> {
    let names = entry_names(text, offset);
    if names.len() > 1 {
        Ok(Question { prompt: "Property: ".to_string(), collection: names, require_match: true, default: None })
    } else {
        Err(names.into_iter().next().unwrap_or_default())
    }
}

/// `org-delete-property-globally`'s question.
pub fn delete_globally_question(text: &str, context: &Context) -> Question {
    Question {
        prompt: "Globally remove property: ".to_string(),
        collection: names(text, false, false, false, context),
        require_match: false,
        default: None,
    }
}

/// `org-set-effort`'s question.
pub fn effort_question(text: &str, offset: usize, context: &Context) -> Result<Question, String> {
    let allowed = allowed(text, offset, EFFORT, context)?;
    Ok(Question {
        prompt: "Effort: ".to_string(),
        require_match: !allowed.values.is_empty() && !allowed.unrestricted,
        collection: allowed.values,
        default: None,
    })
}

/// What setting a property comes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// The property has that value already.
    None,
    /// The new text.
    Text(String),
    /// `TODO`: `org-todo` to this keyword, or to none.
    Todo(Option<String>),
    /// `PRIORITY`: `org-priority` to this one, or to none.
    Priority(Option<String>),
}

/// `org-set-property` with `name` and the answer `value` (trimmed here):
/// `org-entry-put` unless the entry has that value already.
pub fn set(text: &str, offset: usize, name: &str, value: &str, context: &Context) -> Result<Change, String> {
    if !valid_name(name) {
        return Err(format!("Invalid property name: \"{name}\""));
    }
    let value = value.trim_matches([' ', '\t', '\n', '\r']);
    if entry_get(text, offset, name, context).as_deref() == Some(value) {
        return Ok(Change::None);
    }
    let blank = !value.contains(|c: char| !c.is_whitespace());
    match name {
        "TODO" if blank => Ok(Change::Todo(None)),
        "TODO" if context.keywords.names().any(|keyword| keyword == value) => Ok(Change::Todo(Some(value.to_string()))),
        "TODO" => Err(format!("\"{value}\" is not a valid TODO state")),
        "PRIORITY" => Ok(Change::Priority((!blank).then(|| value.to_string()))),
        "SCHEDULED" | "DEADLINE" => Err(format!("Set {name} with its own command")),
        _ => put(text, offset, name, value, context).map(Change::Text),
    }
}

/// `org-duration-p` for `org-duration-to-minutes`, which takes the empty
/// string and a bare number too.
pub fn is_duration(text: &str) -> bool {
    static FORMS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
        let unit = r"[0-9]+(?:\.[0-9]*)?[ \t]*(?:min|[hdwmy])";
        [
            r"^[ \t]*[0-9]+(?::[0-9]{2}){1,2}[ \t]*$".to_string(),
            format!(r"^(?:[ \t]*{unit})+[ \t]*$"),
            format!(r"^(?:[ \t]*{unit})+[ \t]*[0-9]+(?::[0-9][0-9]){{1,2}}[ \t]*$"),
            r"^[0-9]+(?:\.[0-9]*)?$".to_string(),
        ]
        .iter()
        .map(|form| {
            Regex::new(&format!(r"(?s){}", form.replace('$', r"\z").replace('^', r"\A"))).expect("a valid regex")
        })
        .collect()
    });
    text.is_empty() || FORMS.iter().any(|form| form.is_match(text))
}

/// `org-set-effort` with `value`, or `org-inc-effort` (`value` none): the
/// new text, unchanged when the effort already is that value, and Org's
/// message.
pub fn set_effort(
    text: &str,
    offset: usize,
    value: Option<&str>,
    context: &Context,
) -> Result<(String, String), String> {
    let current = local_value(text, offset, context)(EFFORT);
    let value = match value {
        Some(value) => value.to_string(),
        None => {
            let allowed = allowed(text, offset, EFFORT, context)?.values;
            if allowed.is_empty() {
                return Err("Allowed effort values are not set".to_string());
            }
            let next = current
                .as_ref()
                .and_then(|current| allowed.iter().position(|value| value == current))
                .and_then(|at| allowed.get(at + 1));
            match next {
                Some(next) => next.clone(),
                None => {
                    let shown = current.as_ref().map_or("nil".to_string(), |current| format!("{current:?}"));
                    return Err(format!("Unknown value {shown} among allowed values"));
                }
            }
        }
    };
    if !is_duration(&value) {
        return Err(format!("Invalid duration format: {value:?}"));
    }
    let text = if current.as_deref() == Some(value.as_str()) {
        text.to_string()
    } else {
        put(text, offset, EFFORT, &value, context)?
    };
    Ok((text, format!("{EFFORT} is now {value}")))
}

/// What `org-toggle-ordered-property` says when it sets `ORDERED`.
pub const ORDERED_ON: &str = "Subtasks must be completed in sequence";

/// `org-toggle-ordered-property`: `ORDERED` set to `t`, or taken out when
/// the entry has it; the new text and Org's message.
pub fn toggle_ordered(text: &str, offset: usize, context: &Context) -> Result<(String, String), String> {
    let Some(heading) = heading_at(text, offset) else { return Err("Before first headline".to_string()) };
    if local_value(text, heading, context)("ORDERED").is_some() {
        let (text, _, _) = delete(text, heading, "ORDERED");
        Ok((text, "Subtasks can be completed in arbitrary order".to_string()))
    } else {
        Ok((put(text, heading, "ORDERED", "t", context)?, ORDERED_ON.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_allowed_values_as_lisp_does() {
        assert_eq!(
            read_words(r#"red "light blue" 1.50 007 -0 5. 1e3 (a b) x\ y :ETC"#),
            ["red", "light blue", "1.5", "7", "0", "5", "1000.0", "???", "x y", ":ETC"]
        );
    }

    #[test]
    fn durations_are_those_org_reads() {
        for valid in ["", "0:30", "1:00:00", "3h", "2d 4h", "1.5h", "1d 3:00", "90", "1.5"] {
            assert!(is_duration(valid), "{valid}");
        }
        for invalid in ["0:3", "1x", " 90", "h", "abc"] {
            assert!(!is_duration(invalid), "{invalid}");
        }
    }
}
